//! All remote formats converge on the same absolute-second line/word model.
use crate::{LyricLine, Lyrics, Word};
use std::io::Read;

/// Decode Kugou's krc1 container (XOR then zlib), with a 1 MiB inflated limit.
pub fn decrypt_krc(bytes: &[u8]) -> Result<String, String> {
    if !bytes.starts_with(b"krc1") {
        return Err("Invalid KRC signature".into());
    }
    let key = [
        0x40, 0x47, 0x61, 0x77, 0x5e, 0x32, 0x74, 0x47, 0x51, 0x36, 0x31, 0x2d, 0xce, 0xd2, 0x6e,
        0x69,
    ];
    let compressed: Vec<_> = bytes[4..]
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ key[i % 16])
        .collect();
    let mut plain = Vec::new();
    flate2::read::ZlibDecoder::new(compressed.as_slice())
        .take(1_048_577)
        .read_to_end(&mut plain)
        .map_err(|e| e.to_string())?;
    if plain.len() > 1_048_576 {
        return Err("KRC exceeds 1 MiB".into());
    }
    String::from_utf8(plain).map_err(|e| e.to_string())
}

/// NetEase YRC uses absolute word timestamps in milliseconds.
pub fn parse_yrc(text: &str) -> Lyrics {
    let row = regex::Regex::new(r"^\[(\d+),(\d+)\](.*)$").unwrap();
    let word = regex::Regex::new(r"\((\d+),(\d+),\d+\)([^()]*)").unwrap();
    let mut lines = Vec::new();
    for raw in text.lines() {
        let Some(c) = row.captures(raw) else { continue };
        let start = c[1].parse::<f64>().unwrap_or(0.) / 1000.;
        let duration = c[2].parse::<f64>().unwrap_or(0.) / 1000.;
        let words: Vec<_> = word
            .captures_iter(&c[3])
            .map(|w| {
                let at = w[1].parse::<f64>().unwrap_or(0.) / 1000.;
                Word {
                    start: at,
                    end: at + w[2].parse::<f64>().unwrap_or(0.) / 1000.,
                    text: w[3].into(),
                }
            })
            .collect();
        let text = words.iter().map(|w| w.text.as_str()).collect::<String>();
        if !text.is_empty() {
            lines.push(LyricLine {
                at: start,
                end: start + duration,
                text,
                words,
                translation: None,
            });
        }
    }
    lines.sort_by(|a, b| a.at.total_cmp(&b.at));
    Lyrics {
        synced: !lines.is_empty(),
        lines,
    }
}
fn seconds(raw: &str) -> Option<f64> {
    let value = if let Some(ms) = raw.strip_suffix("ms") {
        ms.parse::<f64>().ok()? / 1000.
    } else if let Some(s) = raw.strip_suffix('s') {
        s.parse().ok()?
    } else {
        raw.split(':')
            .try_fold(0., |v, p| Some(v * 60. + p.parse::<f64>().ok()?))?
    };
    (value.is_finite() && value >= 0.).then_some(value)
}
fn role<'a, 'input>(n: roxmltree::Node<'a, 'input>) -> &'a str {
    n.attributes()
        .find(|a| a.name() == "role")
        .map(|a| a.value())
        .unwrap_or("")
}
fn content(n: roxmltree::Node<'_, '_>) -> String {
    n.descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect()
}
/// TTML namespaces, nested timed spans, audio lyricOffset and x-translation are retained.
pub fn parse_ttml(xml: &str) -> Lyrics {
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return Lyrics::default();
    };
    if !doc.descendants().any(|n| n.has_tag_name("body")) {
        return Lyrics::default();
    }
    let offset = doc
        .descendants()
        .find(|n| n.has_tag_name("audio"))
        .and_then(|n| n.attribute("lyricOffset"))
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|n| n.is_finite())
        .unwrap_or(0.);
    let mut lines = Vec::new();
    for p in doc.descendants().filter(|n| n.has_tag_name("p")) {
        let start = p.attribute("begin").and_then(seconds).or_else(|| {
            p.descendants()
                .filter_map(|n| n.attribute("begin").and_then(seconds))
                .min_by(f64::total_cmp)
        });
        let Some(at) = start else { continue };
        let excluded = |n: roxmltree::Node<'_, '_>| {
            n.ancestors()
                .take_while(|a| *a != p)
                .any(|a| ["x-translation", "x-bg", "x-roman"].contains(&role(a)))
        };
        let words: Vec<_> = p
            .descendants()
            .filter(|n| n.has_tag_name("span") && !excluded(*n))
            .filter(|n| {
                !n.descendants()
                    .skip(1)
                    .any(|c| c.attribute("begin").is_some())
            })
            .filter_map(|n| {
                Some(Word {
                    start: seconds(n.attribute("begin")?)? + offset,
                    end: seconds(n.attribute("end")?)? + offset,
                    text: content(n),
                })
            })
            .collect();
        let text = if words.is_empty() {
            p.descendants()
                .filter(|n| n.is_text() && !excluded(*n))
                .filter_map(|n| n.text())
                .collect()
        } else {
            words.iter().map(|w| w.text.as_str()).collect::<String>()
        };
        let translation = p
            .descendants()
            .filter(|n| role(*n) == "x-translation")
            .map(content)
            .collect::<String>();
        if !text.trim().is_empty() {
            lines.push(LyricLine {
                at: at + offset,
                end: p
                    .attribute("end")
                    .and_then(seconds)
                    .map(|s| s + offset)
                    .or_else(|| words.last().map(|w| w.end))
                    .unwrap_or(at + offset),
                text: text.trim().into(),
                words,
                translation: (!translation.trim().is_empty()).then(|| translation.trim().into()),
            });
        }
    }
    lines.sort_by(|a, b| a.at.total_cmp(&b.at));
    Lyrics {
        synced: !lines.is_empty(),
        lines,
    }
}
/// Parse format labels used by the SDK and reference cache.
pub fn parse(text: &str, format: &str, translation: Option<&str>) -> Lyrics {
    let mut lyrics = match format.to_lowercase().as_str() {
        "ttml" => parse_ttml(text),
        "yrc" => parse_yrc(text),
        "krc" => crate::parse_krc(text),
        _ => {
            let (main, trans) = crate::split_bilingual(text);
            let mut l = crate::parse(&main);
            if let Some(t) = trans {
                crate::merge_translation(&mut l, &t);
            }
            l
        }
    };
    if let Some(t) = translation {
        crate::merge_translation(&mut lyrics, t);
    }
    lyrics
}
