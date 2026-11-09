//! LRC / KRC 歌词解析。
//!
//! 输入是插件 `getLyrics` 返回的文本：LRC（`[mm:ss.xx]歌词`，逐行）或酷狗 KRC
//! （`[lineStartMs,lineDurMs]<off,dur,0>字…`，逐字时间轴）。解析结果按时间升序，
//! 供 Flutter 侧随播放位置高亮当前行 / 逐字卡拉OK。纯文本（无时间标签）按顺序整段处理。
//!
//! 移植自 cjrt-workspace `examples/music-cli/src/lyrics.rs`。

/// 逐字歌词中的一个音节/字（来自 KRC）。时间为**绝对秒**。
#[derive(Debug, Clone)]
pub struct Word {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// 一行带时间戳的歌词。
#[derive(Debug, Clone)]
pub struct LyricLine {
    /// 该行起始时间（秒）。
    pub at: f64,
    /// 该行结束时间（秒）；未知时为 `at`（即 0 时长）。
    pub end: f64,
    pub text: String,
    /// 逐字时间轴（KRC / 增强 LRC）。普通 LRC / 纯文本为空，表示不支持逐字。
    pub words: Vec<Word>,
    /// 行级翻译副文本（KRC 内嵌翻译或平行 LRC 就近合并而来）。无翻译为 None。
    pub translation: Option<String>,
}

impl LyricLine {
    /// 仅逐行（无逐字时间轴、无翻译）。
    fn plain(at: f64, text: String) -> Self {
        LyricLine {
            at,
            end: at,
            text,
            words: Vec::new(),
            translation: None,
        }
    }
}

/// 解析后的歌词。`lines` 按 `at` 升序；`synced` 表示是否含有效时间标签（决定能否随播放滚动）。
#[derive(Debug, Clone, Default)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    pub synced: bool,
}

impl Lyrics {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// 解析 LRC 文本。识别 `[mm:ss]` / `[mm:ss.xx]` / `[mm:ss.xxx]`（以及一行多个标签）。
/// 忽略 `[ar:]` `[ti:]` `[offset:]` 等元数据标签（无 `:` 后接数字开头的时间）。
pub fn parse(text: &str) -> Lyrics {
    let mut lines: Vec<LyricLine> = Vec::new();
    let mut any_synced = false;

    for raw in text.lines() {
        let line = raw.trim_end_matches(['\r']);
        if line.is_empty() {
            continue;
        }
        // 收集本行所有前导时间标签，剩余部分为歌词文本。
        let (stamps, content) = split_stamps(line);
        let content = content.trim();
        if stamps.is_empty() {
            // 无时间标签：可能是元数据标签或纯文本行。元数据标签形如 [xx:yyy]，已被
            // split_stamps 跳过（不是时间）；若整行非标签内容，按纯文本收集。
            if !looks_like_meta_tag(line) && !content.is_empty() {
                // 占位，最后统一处理为顺序追加。
                lines.push(LyricLine::plain(f64::MAX, content.to_string()));
            }
            continue;
        }
        if content.is_empty() {
            continue; // 只有时间标签、无文字（间奏）：跳过
        }
        any_synced = true;
        // 增强 LRC：内容含 `<mm:ss.xx>` 词标签时切词构建 words；否则整行为普通 LRC 行。
        // 词标签识别严格要求 `<...>` 内是合法时间戳，含尖括号的普通歌词（如 `I <3 u`）
        // 不会被误判为词级（见 split_word_tags）。
        let enhanced = parse_enhanced_words(content);
        for at in stamps {
            match &enhanced {
                Some((text, words)) => {
                    let end = words.last().map(|w| w.end).unwrap_or(at);
                    lines.push(LyricLine {
                        at,
                        end,
                        text: text.clone(),
                        // 词的绝对时间已在 parse_enhanced_words 内确定，与行首标签无关。
                        words: words.clone(),
                        translation: None,
                    });
                }
                None => lines.push(LyricLine::plain(at, content.to_string())),
            }
        }
    }

    if any_synced {
        // 丢弃混入的纯文本占位行（at == MAX），仅保留带时间戳的行并排序。
        lines.retain(|l| l.at != f64::MAX);
        lines.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap_or(std::cmp::Ordering::Equal));
        Lyrics {
            lines,
            synced: true,
        }
    } else {
        // 纯文本歌词：合并为按出现顺序的行（at 归零，不随播放滚动）。
        let lines: Vec<LyricLine> = lines
            .into_iter()
            .map(|l| LyricLine::plain(0.0, l.text))
            .collect();
        Lyrics {
            lines,
            synced: false,
        }
    }
}

/// 解析 KRC 逐字歌词。每行形如：
/// `[lineStartMs,lineDurMs]<wordOffMs,wordDurMs,0>字<wordOffMs,wordDurMs,0>字...`
/// 行内时间为相对行首的毫秒，转成绝对秒存入 [`Word`]。忽略 `[ti:]`/`[ar:]`/`[offset:]` 等元数据。
pub fn parse_krc(text: &str) -> Lyrics {
    // KRC 内嵌翻译（`[language:base64]`，type==1）按歌词行序对齐，越界丢弃。
    // 在排序前按接受序号赋值，翻译字段随行一起被 sort 携带，不受排序打乱。
    let translations = extract_krc_translations(text);
    let mut lines: Vec<LyricLine> = Vec::new();
    let mut idx = 0usize;
    for raw in text.lines() {
        let line = raw.trim_end_matches(['\r']);
        let Some(mut parsed) = parse_krc_line(line) else {
            continue;
        };
        if parsed.text.trim().is_empty() {
            continue;
        }
        if let Some(t) = translations.get(idx) {
            if !t.trim().is_empty() {
                parsed.translation = Some(t.clone());
            }
        }
        idx += 1;
        lines.push(parsed);
    }
    lines.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap_or(std::cmp::Ordering::Equal));
    let synced = !lines.is_empty();
    Lyrics { lines, synced }
}

/// 从 KRC 提取 `[language:<base64>]` 内嵌翻译（type==1 的 `lyricContent`）。
/// base64 → JSON `{"content":[{"type":1,"lyricContent":[["译行"],…]}]}`。
/// type==0 为音译（本期忽略）。解析任一环节失败静默返回空表（无翻译）。
fn extract_krc_translations(text: &str) -> Vec<String> {
    use base64::Engine as _;
    for raw in text.lines() {
        let line = raw.trim();
        let Some(rest) = line.strip_prefix("[language:") else {
            continue;
        };
        let Some(b64) = rest.strip_suffix(']') else {
            continue;
        };
        let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(b64.trim()) else {
            return Vec::new();
        };
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(&decoded) else {
            return Vec::new();
        };
        let Some(content) = json.get("content").and_then(|v| v.as_array()) else {
            return Vec::new();
        };
        for entry in content {
            if entry.get("type").and_then(|v| v.as_i64()) != Some(1) {
                continue;
            }
            let Some(rows) = entry.get("lyricContent").and_then(|v| v.as_array()) else {
                return Vec::new();
            };
            return rows
                .iter()
                .map(|row| {
                    row.as_array()
                        .and_then(|a| a.first())
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string()
                })
                .collect();
        }
        return Vec::new();
    }
    Vec::new()
}

/// 把平行 LRC 译文按时间戳就近合并进歌词行：|Δt| ≤ 0.5s 取最近行，否则丢弃该译行；
/// 已有翻译（如 KRC 内嵌）的行不覆盖。译文本身按普通 LRC 解析（取时间标签行）。
pub fn merge_translation(lyrics: &mut Lyrics, translation_text: &str) {
    let trans = parse(translation_text);
    if trans.lines.is_empty() {
        return;
    }
    for line in lyrics.lines.iter_mut() {
        if line.translation.is_some() {
            continue;
        }
        let mut best: Option<(f64, &str)> = None;
        for t in &trans.lines {
            let d = (t.at - line.at).abs();
            if d <= 0.5 && best.map(|(bd, _)| d < bd).unwrap_or(true) {
                best = Some((d, t.text.as_str()));
            }
        }
        if let Some((_, txt)) = best {
            if !txt.trim().is_empty() {
                line.translation = Some(txt.to_string());
            }
        }
    }
}

/// 解析单行 KRC：返回带逐字时间轴的 [`LyricLine`]。非 `[num,num]…` 行返回 None。
fn parse_krc_line(line: &str) -> Option<LyricLine> {
    let rest = line.strip_prefix('[')?;
    let close = rest.find(']')?;
    let head = &rest[..close];
    let (start_ms, dur_ms) = head.split_once(',')?;
    let line_start = start_ms.trim().parse::<f64>().ok()? / 1000.0;
    let line_dur = dur_ms.trim().parse::<f64>().ok()? / 1000.0;
    let body = &rest[close + 1..];

    let mut words: Vec<Word> = Vec::new();
    let mut text = String::new();
    let mut chars = body;
    while let Some(open) = chars.find('<') {
        // `<` 前若有游离文字（少见）一并并入上一个/下一个文本。
        let close = chars[open..].find('>')?;
        let tag = &chars[open + 1..open + close];
        let after = &chars[open + close + 1..];
        // tag = "off,dur,0"
        let mut it = tag.split(',');
        let off = it.next()?.trim().parse::<f64>().ok()? / 1000.0;
        let wdur = it.next()?.trim().parse::<f64>().ok()? / 1000.0;
        // 该字文本 = 到下一个 `<` 之前的内容。
        let next = after.find('<').unwrap_or(after.len());
        let wtext = &after[..next];
        if !wtext.is_empty() {
            let ws = line_start + off;
            words.push(Word {
                start: ws,
                end: ws + wdur,
                text: wtext.to_string(),
            });
            text.push_str(wtext);
        }
        chars = &after[next..];
    }

    Some(LyricLine {
        at: line_start,
        end: line_start + line_dur,
        text,
        words,
        translation: None,
    })
}

/// 解析增强 LRC 行内容（行首时间标签已剥离）。内容含合法 `<mm:ss.xx>` 词标签时返回
/// `Some((整行文本, 词表))`，否则 None（按普通 LRC 行处理）。
///
/// 词模型：`<t0>词0<t1>词1…<tN>`——每个词 start = 其前的标签时间，end = 下一个标签时间；
/// 行尾收尾标签（其后无文字）仅作最后一个词的 end。词间有空文字标签段自然形成间隙。
fn parse_enhanced_words(content: &str) -> Option<(String, Vec<Word>)> {
    let segs = split_word_tags(content)?;
    let mut words: Vec<Word> = Vec::new();
    let mut text = String::new();
    for i in 0..segs.len() {
        let (t, ref txt) = segs[i];
        if txt.trim().is_empty() {
            continue; // 收尾/间隙标签：无文字，仅贡献前一个词的 end（下面用 segs[i+1].0）
        }
        let end = if i + 1 < segs.len() { segs[i + 1].0 } else { t };
        words.push(Word {
            start: t,
            end,
            text: txt.clone(),
        });
        text.push_str(txt);
    }
    if words.is_empty() {
        return None;
    }
    Some((text, words))
}

/// 把增强 LRC 行内容切成 `(标签时间, 其后文字)` 段序列。无任何合法时间戳标签返回 None
/// （交回普通文本处理，防止含尖括号的歌词被误判词级）。首个标签前的游离文字并入首段。
fn split_word_tags(content: &str) -> Option<Vec<(f64, String)>> {
    let mut result: Vec<(f64, String)> = Vec::new();
    let mut leading = String::new();
    let mut any = false;
    let mut rest = content;

    // 把 `s` 追加到「当前文字目标」（首标签前为 leading，之后为最后一段的文字）。
    fn push_text(result: &mut Vec<(f64, String)>, leading: &mut String, s: &str) {
        if let Some(last) = result.last_mut() {
            last.1.push_str(s);
        } else {
            leading.push_str(s);
        }
    }

    loop {
        let Some(lt) = rest.find('<') else {
            push_text(&mut result, &mut leading, rest);
            break;
        };
        let before = &rest[..lt];
        let after_lt = &rest[lt + 1..];
        let Some(gt) = after_lt.find('>') else {
            // 无闭合 `>`：`<` 及其后全部按字面文字处理。
            push_text(&mut result, &mut leading, before);
            push_text(&mut result, &mut leading, "<");
            rest = after_lt;
            continue;
        };
        let inner = &after_lt[..gt];
        match parse_timestamp(inner) {
            Some(t) => {
                push_text(&mut result, &mut leading, before);
                result.push((t, String::new()));
                any = true;
            }
            None => {
                // 非时间戳的 `<...>`：整段按字面文字（尖括号误伤防御）。
                push_text(&mut result, &mut leading, before);
                push_text(&mut result, &mut leading, "<");
                push_text(&mut result, &mut leading, inner);
                push_text(&mut result, &mut leading, ">");
            }
        }
        rest = &after_lt[gt + 1..];
    }

    if !any {
        return None;
    }
    if !leading.is_empty() {
        if let Some(first) = result.first_mut() {
            let mut merged = leading;
            merged.push_str(&first.1);
            first.1 = merged;
        }
    }
    Some(result)
}

/// 把「双语单文件 LRC」拆成 (主歌词, 译文) 两份文本。
///
/// 常见形状是同一个时间戳连着出现两行、原文在前译文在后 —— Haiku 自己的下载器
/// （`download_asset_service.dart` 的 `withTranslation`）写出来的 `.lrc` 就是这个
/// 样子，网上下来的双语歌词也多是如此。[`parse`] 对这两行一视同仁，直接渲染出来
/// 就是重复行；这里先按时间戳把它们分开，再交给 [`merge_translation`] 做就近合并。
///
/// **只在「至少有一个时间戳带了两行以上非空内容」时才判为双语**，否则原样返回
/// `(text, None)` —— 单语文件一个字节都不改，避免误伤。三语及以上只保留前两份
/// （渲染层只有主/副两行）。
pub fn split_bilingual(text: &str) -> (String, Option<String>) {
    use std::collections::HashMap;

    let mut counts: HashMap<i64, u32> = HashMap::new();
    for raw in text.lines() {
        let (stamps, content) = split_stamps(raw.trim_end_matches(['\r']));
        if content.trim().is_empty() {
            continue;
        }
        for at in stamps {
            *counts.entry(stamp_key(at)).or_insert(0) += 1;
        }
    }
    if !counts.values().any(|c| *c >= 2) {
        return (text.to_string(), None);
    }

    let mut emitted: HashMap<i64, u32> = HashMap::new();
    let mut main = String::new();
    let mut trans = String::new();
    for raw in text.lines() {
        let (stamps, content) = split_stamps(raw.trim_end_matches(['\r']));
        let content = content.trim();
        if content.is_empty() {
            continue;
        }
        for at in stamps {
            let seen = emitted.entry(stamp_key(at)).or_insert(0);
            let nth = *seen;
            *seen += 1;
            let target = match nth {
                0 => &mut main,
                1 => &mut trans,
                _ => continue,
            };
            target.push_str(&render_stamp(at));
            target.push_str(content);
            target.push('\n');
        }
    }

    if trans.trim().is_empty() {
        (main, None)
    } else {
        (main, Some(trans))
    }
}

/// 时间戳的比较键：毫秒整数。浮点秒直接当 HashMap 键不可靠。
fn stamp_key(at: f64) -> i64 {
    (at * 1000.0).round() as i64
}

/// 秒 → `[mm:ss.mmm]`。保留毫秒，重写一遍不掉精度。
fn render_stamp(at: f64) -> String {
    let ms = (at.max(0.0) * 1000.0).round() as u64;
    format!(
        "[{:02}:{:02}.{:03}]",
        ms / 60_000,
        (ms % 60_000) / 1000,
        ms % 1000
    )
}

/// 从行首逐个剥离 `[mm:ss(.xx)]` 时间标签，返回 (时间秒列表, 剩余文本)。
fn split_stamps(line: &str) -> (Vec<f64>, &str) {
    let mut stamps = Vec::new();
    let mut rest = line;
    loop {
        let bytes = rest.as_bytes();
        if bytes.first() != Some(&b'[') {
            break;
        }
        let Some(close) = rest.find(']') else { break };
        let inner = &rest[1..close];
        match parse_timestamp(inner) {
            Some(secs) => {
                stamps.push(secs);
                rest = &rest[close + 1..];
            }
            None => break, // 非时间标签（元数据），停止剥离
        }
    }
    (stamps, rest)
}

/// 解析 `mm:ss`、`mm:ss.xx`、`mm:ss.xxx` 为秒。非此格式返回 None。
fn parse_timestamp(s: &str) -> Option<f64> {
    let (mm, rest) = s.split_once(':')?;
    let minutes: f64 = mm.trim().parse().ok()?;
    if minutes < 0.0 {
        return None;
    }
    // rest 可能是 "ss" 或 "ss.xx"。
    let seconds: f64 = rest.trim().parse().ok()?;
    if seconds < 0.0 {
        return None;
    }
    Some(minutes * 60.0 + seconds)
}

/// 判断是否为 `[key:value]` 形式的元数据标签（如 `[ar:周杰伦]`），用于纯文本场景的过滤。
fn looks_like_meta_tag(line: &str) -> bool {
    let line = line.trim();
    line.starts_with('[') && line.ends_with(']') && line.contains(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_at(l: &Lyrics, pos: f64) -> Option<&str> {
        if l.lines.is_empty() {
            return None;
        }
        if !l.synced {
            return Some(l.lines[0].text.as_str());
        }
        if pos + 1e-3 < l.lines[0].at {
            return None;
        }
        let mut idx = 0usize;
        for (i, line) in l.lines.iter().enumerate() {
            if line.at <= pos + f64::EPSILON {
                idx = i;
            } else {
                break;
            }
        }
        Some(l.lines[idx].text.as_str())
    }

    #[test]
    fn synced_basic() {
        let l = parse("[00:01.00]hello\n[00:03.50]world");
        assert!(l.synced);
        assert_eq!(l.lines.len(), 2);
        assert_eq!(line_at(&l, 0.0), None);
        assert_eq!(line_at(&l, 1.0), Some("hello"));
        assert_eq!(line_at(&l, 2.0), Some("hello"));
        assert_eq!(line_at(&l, 4.0), Some("world"));
    }

    #[test]
    fn multi_stamp_line() {
        let l = parse("[00:01.00][00:05.00]repeat");
        assert_eq!(l.lines.len(), 2);
        assert_eq!(line_at(&l, 1.0), Some("repeat"));
        assert_eq!(line_at(&l, 5.0), Some("repeat"));
    }

    #[test]
    fn ignores_meta_and_blank_interludes() {
        let l = parse("[ti:song]\n[ar:artist]\n[00:02.00]\n[00:04.00]line");
        assert!(l.synced);
        assert_eq!(l.lines.len(), 1);
        assert_eq!(line_at(&l, 4.0), Some("line"));
    }

    #[test]
    fn plain_text() {
        let l = parse("just a line\nanother");
        assert!(!l.synced);
        assert_eq!(l.lines.len(), 2);
        assert_eq!(line_at(&l, 99.0), Some("just a line"));
    }

    #[test]
    fn enhanced_lrc_per_word() {
        // `<t0>词0<t1>词1<t2>`：末尾收尾标签标记最后一个词的 end。
        let l = parse("[00:01.00]<00:01.00>晴<00:01.30>天<00:01.80>");
        assert!(l.synced);
        assert_eq!(l.lines.len(), 1);
        let line = &l.lines[0];
        assert_eq!(line.text, "晴天");
        assert_eq!(line.words.len(), 2);
        assert!((line.words[0].start - 1.0).abs() < 1e-9);
        assert!((line.words[0].end - 1.30).abs() < 1e-9);
        assert!((line.words[1].start - 1.30).abs() < 1e-9);
        assert!((line.words[1].end - 1.80).abs() < 1e-9);
        assert!((line.end - 1.80).abs() < 1e-9);
    }

    #[test]
    fn enhanced_lrc_word_gap() {
        // 词间空文字标签段形成间隙：第一个词 end 落在间隙标签时间。
        let l = parse("[00:00.00]<00:00.00>a<00:00.50><00:01.00>b<00:01.50>");
        let line = &l.lines[0];
        assert_eq!(line.text, "ab");
        assert_eq!(line.words.len(), 2);
        assert!((line.words[0].end - 0.50).abs() < 1e-9);
        assert!((line.words[1].start - 1.00).abs() < 1e-9);
        assert!((line.words[1].end - 1.50).abs() < 1e-9);
    }

    #[test]
    fn enhanced_lrc_angle_bracket_not_misparsed() {
        // 含尖括号但非时间戳的普通歌词不得被当作词级。
        let l = parse("[00:02.00]I <3 you & 1 < 2");
        let line = &l.lines[0];
        assert!(line.words.is_empty());
        assert_eq!(line.text, "I <3 you & 1 < 2");
    }

    #[test]
    fn merge_translation_nearest() {
        let mut l = parse("[00:01.00]hello\n[00:03.00]world");
        merge_translation(&mut l, "[00:01.10]你好\n[00:03.40]世界\n[00:09.00]太远");
        assert_eq!(l.lines[0].translation.as_deref(), Some("你好"));
        assert_eq!(l.lines[1].translation.as_deref(), Some("世界"));
        // 第三条译行 |Δt| > 0.5 无归属，不影响两行。
    }

    #[test]
    fn merge_translation_does_not_override_existing() {
        let mut l = parse("[00:01.00]hello");
        l.lines[0].translation = Some("既有".to_string());
        merge_translation(&mut l, "[00:01.00]覆盖");
        assert_eq!(l.lines[0].translation.as_deref(), Some("既有"));
    }

    #[test]
    fn krc_embedded_translation() {
        use base64::Engine as _;
        // type==1 翻译，两行对齐两条歌词行。
        let meta = r#"{"content":[{"type":1,"language":0,"lyricContent":[["你好"],["世界"]]}]}"#;
        let b64 = base64::engine::general_purpose::STANDARD.encode(meta);
        let krc = format!("[language:{b64}]\n[0,500]<0,500,0>hi\n[1000,500]<0,500,0>world");
        let l = parse_krc(&krc);
        assert_eq!(l.lines.len(), 2);
        assert_eq!(l.lines[0].translation.as_deref(), Some("你好"));
        assert_eq!(l.lines[1].translation.as_deref(), Some("世界"));
    }

    #[test]
    fn krc_per_word() {
        let l = parse_krc("[ti:x]\n[offset:0]\n[1000,640]<0,160,0>晴<160,160,0>天<320,320,0>了");
        assert!(l.synced);
        assert_eq!(l.lines.len(), 1);
        let line = &l.lines[0];
        assert_eq!(line.text, "晴天了");
        assert_eq!(line.words.len(), 3);
        assert!((line.at - 1.0).abs() < 1e-9);
        assert!((line.end - 1.64).abs() < 1e-9);
        // 第三个字：行首 1.0s + 偏移 0.32s 起，时长 0.32s。
        assert!((line.words[2].start - 1.32).abs() < 1e-9);
        assert!((line.words[2].end - 1.64).abs() < 1e-9);
        assert_eq!(line.words[1].text, "天");
    }

    /// 单语文件必须原样返回：这是「不误伤」的底线。
    #[test]
    fn split_bilingual_leaves_monolingual_untouched() {
        let src = "[ti:x]\n[00:01.00]hello\n[00:02.00]world\n";
        let (main, trans) = split_bilingual(src);
        assert_eq!(main, src, "单语文件应一个字节都不改");
        assert!(trans.is_none());
    }

    /// 同一时间戳的第二行拆成译文，走 merge_translation 后落到同一行上。
    #[test]
    fn split_bilingual_extracts_second_line_per_stamp() {
        let src = "[00:01.00]Yesterday\n[00:01.00]昨日\n[00:05.50]All my troubles\n[00:05.50]我所有的烦恼\n";
        let (main, trans) = split_bilingual(src);
        let trans = trans.expect("应识别为双语");

        let mut parsed = parse(&main);
        assert_eq!(parsed.lines.len(), 2, "主歌词不该还留着译文行");
        merge_translation(&mut parsed, &trans);
        assert_eq!(parsed.lines[0].text, "Yesterday");
        assert_eq!(parsed.lines[0].translation.as_deref(), Some("昨日"));
        assert_eq!(parsed.lines[1].translation.as_deref(), Some("我所有的烦恼"));
    }

    /// 只有部分行有译文时，其余行照常无译文（别把下一行的译文串上去）。
    #[test]
    fn split_bilingual_handles_partial_translation() {
        let src = "[00:01.000]one\n[00:01.000]一\n[00:09.000]nine\n";
        let (main, trans) = split_bilingual(src);
        let mut parsed = parse(&main);
        merge_translation(&mut parsed, &trans.expect("应识别为双语"));
        assert_eq!(parsed.lines.len(), 2);
        assert_eq!(parsed.lines[0].translation.as_deref(), Some("一"));
        assert_eq!(parsed.lines[1].translation, None);
    }

    /// 毫秒精度不能在重写时丢掉，否则 merge_translation 的 0.5s 窗口白给。
    #[test]
    fn split_bilingual_keeps_millisecond_precision() {
        let src = "[00:01.234]a\n[00:01.234]甲\n[01:02.345]b\n[01:02.345]乙\n";
        let (main, _) = split_bilingual(src);
        let parsed = parse(&main);
        assert!((parsed.lines[0].at - 1.234).abs() < 1e-6);
        assert!((parsed.lines[1].at - 62.345).abs() < 1e-6);
    }
}
