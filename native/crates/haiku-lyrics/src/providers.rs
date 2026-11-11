//! Reference provider IDs, request parameters and candidate scoring.
use base64::Engine;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};

pub type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Query {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_sec: f64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Text {
    pub text: String,
    pub format: String,
    pub translation: Option<String>,
    pub instrumental: bool,
}
impl Text {
    /// Reference caches store enhanced LRC rather than raw TTML/YRC documents.
    pub fn reference_text(&self) -> Self {
        if !["ttml", "yrc"].contains(&self.format.to_lowercase().as_str()) {
            return self.clone();
        }
        let parsed = self.parsed();
        let mut lines = Vec::new();
        let mut translations = Vec::new();
        for l in parsed.lines {
            let at = stamp(l.at, false);
            let body = if l.words.is_empty() {
                l.text
            } else {
                l.words
                    .iter()
                    .map(|w| format!("{}{}{}", stamp(w.start, true), w.text, stamp(w.end, true)))
                    .collect()
            };
            lines.push(format!("{at}{body}"));
            if let Some(t) = l.translation {
                translations.push(format!("{at}{t}"));
            }
        }
        Self {
            text: lines.join("\n"),
            format: "lrc".into(),
            translation: (!translations.is_empty()).then(|| translations.join("\n")),
            instrumental: self.instrumental,
        }
    }
    pub fn parsed(&self) -> crate::Lyrics {
        crate::formats::parse(&self.text, &self.format, self.translation.as_deref())
    }
    pub fn quality(&self) -> u8 {
        let parsed = self.parsed();
        if parsed.is_empty() {
            0
        } else if parsed.lines.iter().any(|l| !l.words.is_empty()) {
            3
        } else if parsed.synced || self.instrumental {
            2
        } else {
            1
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Candidate {
    pub provider_id: String,
    pub candidate_id: String,
    pub title: String,
    pub artist: String,
    pub duration_ms: Option<f64>,
    pub source_label: Option<String>,
    pub has_word_level: bool,
    pub has_translation: bool,
    pub scrollable: Option<bool>,
    pub inline_result: Option<Text>,
    pub resolve_data: Value,
    pub score: u32,
    pub recommended: bool,
    pub stars: f64,
}
pub trait Provider: Send + Sync {
    fn id(&self) -> &str;
    fn search(&self, query: Query) -> BoxFuture<'_, Result<Vec<Candidate>>>;
    fn resolve(&self, candidate: Candidate) -> BoxFuture<'_, Result<Option<Text>>> {
        Box::pin(async move { Ok(candidate.inline_result) })
    }
}
pub fn clean_title(s: &str) -> String {
    let re=regex::Regex::new(r"(?i)\s*[\(\[][^\)\]]*(?:official|video|audio|lyrics?|visualizer|hd|hq|4k|remaster|remix|live|acoustic|version|edit|extended|radio|clean|explicit)[^\)\]]*[\)\]]|\s*【.*?】|\s*\|.*$|\s*(?:feat\.|ft\.).*$").unwrap();
    re.replace_all(s, "").trim().into()
}
pub fn clean_artist(s: &str) -> String {
    regex::Regex::new(r"(?i) & | and |, | x | feat\.? | ft\.? | featuring | with ")
        .unwrap()
        .split(s)
        .next()
        .unwrap_or("")
        .trim()
        .into()
}
fn similarity(a: &str, b: &str) -> f64 {
    let (a, b) = (a.trim().to_lowercase(), b.trim().to_lowercase());
    if a == b {
        return 1.;
    }
    if a.is_empty() || b.is_empty() {
        return 0.;
    }
    if a.contains(&b) || b.contains(&a) {
        return 0.8;
    }
    // JS scores UTF-16 code units, including astral characters.
    let a: Vec<_> = a.encode_utf16().collect();
    let b: Vec<_> = b.encode_utf16().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut row = vec![i + 1];
        for (j, y) in b.iter().enumerate() {
            row.push(
                (prev[j + 1] + 1)
                    .min(row[j] + 1)
                    .min(prev[j] + usize::from(x != y)),
            );
        }
        prev = row;
    }
    1. - prev[b.len()] as f64 / a.len().max(b.len()) as f64
}
pub fn score(query: &Query, c: &Candidate) -> u32 {
    let mut result = 0.;
    if query.duration_sec > 0. && c.duration_ms.unwrap_or(0.) > 0. {
        let d = (c.duration_ms.unwrap() / 1000. - query.duration_sec).abs();
        result += if d <= 2. {
            40.
        } else if d <= 5. {
            25.
        } else if d <= 8. {
            10.
        } else if d <= 15. {
            0.
        } else {
            -30.
        };
    }
    let a = clean_title(&query.title);
    let b = clean_title(if c.title.is_empty() {
        &query.title
    } else {
        &c.title
    });
    result += if !a.is_empty() && a == b {
        30.
    } else if !a.is_empty() && !b.is_empty() && (a.contains(&b) || b.contains(&a)) {
        18.
    } else {
        (similarity(&a, &b) * 30.).round()
    };
    let a = clean_artist(&query.artist).to_lowercase();
    let b = clean_artist(if c.artist.is_empty() {
        &query.artist
    } else {
        &c.artist
    })
    .to_lowercase();
    result += if !a.is_empty() && !b.is_empty() && (a == b || a.contains(&b) || b.contains(&a)) {
        20.
    } else {
        (similarity(&a, &b) * 15.).round()
    };
    result += if c.has_word_level { 8. } else { 0. };
    result += if c.has_translation { 6. } else { 0. };
    result += if c.source_label.as_deref() == Some("官方推荐") {
        6.
    } else {
        0.
    };
    result -= if c.scrollable == Some(false) { 40. } else { 0. };
    result.clamp(0., 100.) as u32
}
pub fn rank(q: &Query, mut candidates: Vec<Candidate>) -> Vec<Candidate> {
    for c in &mut candidates {
        c.score = score(q, c);
    }
    candidates.sort_by_key(|c| std::cmp::Reverse(c.score));
    for (i, c) in candidates.iter_mut().enumerate() {
        c.recommended = i == 0 && c.score > 0;
        c.stars = (c.score as f64 / 10.).round() / 2.;
    }
    candidates
}
fn inline(id: &str, q: &Query, text: Text) -> Candidate {
    let parsed = text.parsed();
    Candidate {
        provider_id: id.into(),
        candidate_id: id.into(),
        title: q.title.clone(),
        artist: q.artist.clone(),
        duration_ms: Some(q.duration_sec * 1000.),
        has_word_level: parsed.lines.iter().any(|l| !l.words.is_empty()),
        has_translation: text.translation.is_some()
            || parsed.lines.iter().any(|l| l.translation.is_some()),
        scrollable: Some(parsed.synced),
        inline_result: Some(text),
        ..Default::default()
    }
}
fn string(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or("").into()
}
fn stamp(seconds: f64, word: bool) -> String {
    let ms = (seconds.max(0.) * 1000.).round() as u64;
    format!(
        "{}{:02}:{:02}.{:03}{}",
        if word { '<' } else { '[' },
        ms / 60000,
        ms / 1000 % 60,
        ms % 1000,
        if word { '>' } else { ']' }
    )
}

/// Named endpoint overrides preserve every request's query and headers in offline fixtures.
#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
    pub endpoints: BTreeMap<String, String>,
}
impl Default for Http {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoints: BTreeMap::new(),
        }
    }
}
impl Http {
    async fn get(
        &self,
        url: &str,
        params: Value,
        headers: &BTreeMap<String, String>,
    ) -> Result<String> {
        let target = self.endpoints.get(url).map(String::as_str).unwrap_or(url);
        let mut url = reqwest::Url::parse(target).map_err(|e| e.to_string())?;
        if let Some(map) = params.as_object() {
            let mut pairs = url.query_pairs_mut();
            for (k, v) in map {
                if !v.is_null() && v != "" {
                    pairs.append_pair(
                        k,
                        &v.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| v.to_string()),
                    );
                }
            }
        }
        let mut request = self.client.get(url);
        for (k, v) in headers {
            request = request.header(k, v);
        }
        let mut response = request
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            if bytes.len() + chunk.len() > 1_048_576 {
                return Err("Lyrics response exceeds 1 MiB".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        String::from_utf8(bytes).map_err(|e| e.to_string())
    }
    async fn json(&self, url: &str, params: Value) -> Result<Value> {
        serde_json::from_str(&self.get(url, params, &BTreeMap::new()).await?)
            .map_err(|e| e.to_string())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Custom {
    pub id: String,
    pub name: String,
    pub url_template: String,
    pub response_type: String,
    #[serde(default)]
    pub json_path: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}
struct Remote {
    id: String,
    http: Http,
    custom: Option<Custom>,
}
/// Built-in order and disabled defaults are applied by service Options, matching settings.
pub fn builtins(http: Http) -> Vec<Arc<dyn Provider>> {
    ["betterlyrics", "lrclib", "kugou", "paxsenix", "lyricsplus"]
        .into_iter()
        .map(|id| {
            Arc::new(Remote {
                id: id.into(),
                http: http.clone(),
                custom: None,
            }) as Arc<dyn Provider>
        })
        .collect()
}
pub fn custom(config: Custom, http: Http) -> Arc<dyn Provider> {
    Arc::new(Remote {
        id: config.id.clone(),
        http,
        custom: Some(config),
    })
}
impl Remote {
    async fn search_remote(&self, q: Query) -> Result<Vec<Candidate>> {
        let h = &self.http;
        if let Some(c) = &self.custom {
            let mut url = c.url_template.clone();
            for (k, v) in [
                ("title", q.title.clone()),
                ("artist", q.artist.clone()),
                ("album", q.album.clone().unwrap_or_default()),
                (
                    "duration",
                    if q.duration_sec >= 0. {
                        q.duration_sec.to_string()
                    } else {
                        String::new()
                    },
                ),
            ] {
                let encoded: String = url_component(&v);
                url = url.replace(&format!("{{{k}}}"), &encoded);
            }
            let body = h.get(&url, Value::Null, &c.headers).await?;
            let text = if c.response_type == "json" {
                let mut j: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
                for segment in c.json_path.split('.').filter(|s| !s.is_empty()) {
                    j = if let Some(a) = j.as_array() {
                        segment
                            .parse::<usize>()
                            .ok()
                            .and_then(|i| a.get(i))
                            .cloned()
                            .unwrap_or(Value::Null)
                    } else {
                        j.get(segment).cloned().unwrap_or(Value::Null)
                    };
                }
                j.as_str().unwrap_or("").into()
            } else {
                body
            };
            return Ok(if text.trim().is_empty() {
                vec![]
            } else {
                vec![inline(
                    &self.id,
                    &q,
                    Text {
                        text,
                        format: "lrc".into(),
                        ..Default::default()
                    },
                )]
            });
        }
        match self.id.as_str() {
            "lrclib" => {
                let title = clean_title(&q.title);
                let artist = clean_artist(&q.artist);
                for params in [
                    json!({"track_name":title,"artist_name":artist,"album_name":q.album}),
                    json!({"track_name":title}),
                    json!({"q":format!("{artist} {title}")}),
                    json!({"q":title}),
                    json!({"track_name":q.title,"artist_name":q.artist}),
                ] {
                    let j = h.json("https://lrclib.net/api/search", params).await?;
                    let mut result = Vec::new();
                    for t in j.as_array().into_iter().flatten() {
                        let synced = string(t, "syncedLyrics");
                        let plain = string(t, "plainLyrics");
                        let instrumental = t["instrumental"].as_bool().unwrap_or(false);
                        if synced.is_empty() && plain.is_empty() && !instrumental {
                            continue;
                        }
                        let mut c = inline(
                            &self.id,
                            &q,
                            Text {
                                text: if !synced.is_empty() {
                                    synced.clone()
                                } else if !plain.is_empty() {
                                    plain
                                } else {
                                    "纯音乐，请欣赏".into()
                                },
                                format: if synced.is_empty() { "plain" } else { "lrc" }.into(),
                                instrumental,
                                ..Default::default()
                            },
                        );
                        c.candidate_id = t["id"].to_string();
                        c.title = string(t, "trackName");
                        c.artist = string(t, "artistName");
                        c.duration_ms = t["duration"].as_f64().map(|d| d * 1000.);
                        result.push(c);
                    }
                    if !result.is_empty() {
                        return Ok(result);
                    }
                }
                Ok(vec![])
            }
            "betterlyrics" => {
                let j=h.json("https://lyrics-api.boidu.dev/getLyrics",json!({"s":q.title,"a":q.artist,"al":q.album,"d":(q.duration_sec>0.).then_some(q.duration_sec)})).await?;
                let raw = Text {
                    text: string(&j, "ttml"),
                    format: "ttml".into(),
                    ..Default::default()
                };
                Ok(if raw.quality() > 0 {
                    vec![inline(&self.id, &q, raw)]
                } else {
                    vec![]
                })
            }
            "kugou" => {
                let title =
                    regex::Regex::new(r"\(.*\)|（.*）|「.*」|『.*』|<.*>|《.*》|〈.*〉|＜.*＞")
                        .unwrap()
                        .replace_all(&q.title, "")
                        .into_owned();
                let artist = q
                    .artist
                    .replace(", ", "、")
                    .replace(" & ", "、")
                    .replace('和', "、")
                    .replace('.', "");
                let keyword = format!(
                    "{title} - {artist}{}",
                    q.album
                        .as_ref()
                        .map(|a| format!(" {a}"))
                        .unwrap_or_default()
                );
                let songs=h.json("https://mobileservice.kugou.com/api/v3/search/song",json!({"version":"9108","plat":0,"pagesize":8,"showtype":0,"keyword":keyword})).await?;
                let mut raw = vec![];
                for song in songs["data"]["info"].as_array().into_iter().flatten() {
                    if q.duration_sec > 0.
                        && (song["duration"].as_f64().unwrap_or(0.) - q.duration_sec).abs() > 8.
                    {
                        continue;
                    }
                    let j = h
                        .json(
                            "https://lyrics.kugou.com/search",
                            json!({"ver":1,"man":"yes","client":"pc","hash":song["hash"]}),
                        )
                        .await?;
                    raw = j["candidates"].as_array().cloned().unwrap_or_default();
                    if !raw.is_empty() {
                        break;
                    }
                }
                if raw.is_empty() {
                    raw=h.json("https://lyrics.kugou.com/search",json!({"ver":1,"man":"yes","client":"pc","keyword":keyword,"duration":(q.duration_sec>0.).then_some(q.duration_sec*1000.)})).await?["candidates"].as_array().cloned().unwrap_or_default();
                }
                let mut seen = std::collections::BTreeSet::new();
                Ok(raw
                    .into_iter()
                    .filter(|v| {
                        !v["id"].is_null()
                            && v["accesskey"].is_string()
                            && seen.insert(v["id"].to_string())
                    })
                    .map(|v| Candidate {
                        provider_id: self.id.clone(),
                        candidate_id: v["id"]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| v["id"].to_string()),
                        title: v["song"].as_str().unwrap_or(&q.title).into(),
                        artist: v["singer"].as_str().unwrap_or(&q.artist).into(),
                        duration_ms: v["duration"].as_f64(),
                        has_word_level: v["krctype"] == 1,
                        scrollable: Some(v["contenttype"] != 2),
                        source_label: Some(if v["product_from"] == "官方推荐歌词" {
                            "官方推荐".into()
                        } else {
                            string(&v, "product_from")
                        }),
                        resolve_data: json!({"accesskey":v["accesskey"]}),
                        ..Default::default()
                    })
                    .collect())
            }
            "lyricsplus" => {
                let mut best: Option<Text> = None;
                for base in [
                    "https://lyricsplus.binimum.org",
                    "https://lyricsplus.atomix.one",
                    "https://lyricsplus.prjktla.my.id",
                    "https://lyricsplus-seven.vercel.app",
                ] {
                    let Ok(j)=h.json(&format!("{base}/v2/lyrics/get"),json!({"title":q.title,"artist":q.artist,"album":q.album,"duration":(q.duration_sec>0.).then_some(q.duration_sec)})).await else{continue};
                    let mut lines = vec![];
                    let mut trans = vec![];
                    for l in j["lyrics"].as_array().into_iter().flatten() {
                        let words: Vec<_> = l["syllabus"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|w| w["isBackground"] != true)
                            .collect();
                        if l["syllabus"].as_array().is_some_and(|a| !a.is_empty())
                            && words.is_empty()
                        {
                            continue;
                        }
                        let text = if string(&j, "type").eq_ignore_ascii_case("word")
                            && !words.is_empty()
                        {
                            words
                                .iter()
                                .map(|w| {
                                    format!(
                                        "{}{}{}",
                                        stamp(w["time"].as_f64().unwrap_or(0.) / 1000., true),
                                        string(w, "text"),
                                        stamp(
                                            (w["time"].as_f64().unwrap_or(0.)
                                                + w["duration"].as_f64().unwrap_or(0.))
                                                / 1000.,
                                            true
                                        )
                                    )
                                })
                                .collect()
                        } else {
                            string(l, "text")
                        };
                        let at = stamp(l["time"].as_f64().unwrap_or(0.) / 1000., false);
                        if !text.is_empty() {
                            lines.push(format!("{at}{text}"));
                        }
                        if let Some(t) = l["translation"]["text"].as_str() {
                            trans.push(format!("{at}{t}"));
                        }
                    }
                    if !lines.is_empty() {
                        best = Some(Text {
                            text: lines.join("\n"),
                            format: "lrc".into(),
                            translation: (!trans.is_empty()).then(|| trans.join("\n")),
                            ..Default::default()
                        });
                        break;
                    }
                }
                if let Ok(j)=h.json("https://lyrics-api.binimum.org/",json!({"track":q.title,"artist":q.artist,"album":q.album,"duration":(q.duration_sec>0.).then_some(q.duration_sec)})).await {
                    if let Some(url)=j["results"].as_array().into_iter().flatten().find_map(|r|r["lyricsUrl"].as_str()) {
                        if let Ok(text)=h.get(url,Value::Null,&BTreeMap::new()).await {
                            let t=Text{text,format:"ttml".into(),..Default::default()};
                            if t.quality()>0&&best.as_ref().is_none_or(|b|t.quality()>=b.quality()){best=Some(t);}
                        }
                    }
                }
                Ok(best
                    .map(|t| vec![inline(&self.id, &q, t)])
                    .unwrap_or_default())
            }
            "paxsenix" => self.search_apple(q).await,
            _ => Err("Unknown lyrics provider".into()),
        }
    }
    async fn search_apple(&self, q: Query) -> Result<Vec<Candidate>> {
        let h = &self.http;
        let html = h
            .get(
                "https://beta.music.apple.com",
                Value::Null,
                &BTreeMap::new(),
            )
            .await?;
        let script = regex::Regex::new(r#"/assets/index~[^/"']+\.js"#)
            .unwrap()
            .find(&html)
            .map(|m| m.as_str())
            .ok_or("Apple token script missing")?;
        let js = h
            .get(
                &format!("https://beta.music.apple.com{script}"),
                Value::Null,
                &BTreeMap::new(),
            )
            .await?;
        let token = regex::Regex::new(r"eyJ[A-Za-z0-9_=-]+\.[A-Za-z0-9_=-]+\.[A-Za-z0-9_=-]+")
            .unwrap()
            .find(&js)
            .ok_or("Apple token missing")?
            .as_str();
        let headers = BTreeMap::from([
            ("Authorization".into(), format!("Bearer {token}")),
            ("Origin".into(), "https://music.apple.com".into()),
            ("Referer".into(), "https://music.apple.com/".into()),
            ("x-apple-renewal".into(), "true".into()),
        ]);
        for term in [
            format!("{} {}", clean_title(&q.title), clean_artist(&q.artist)),
            clean_title(&q.title),
            format!(
                "{} {} {}",
                q.title,
                q.artist,
                q.album.as_deref().unwrap_or("")
            ),
        ] {
            let body=h.get("https://amp-api.music.apple.com/v1/catalog/us/search",json!({"term":term,"types":"songs","limit":25,"l":"en-US","platform":"web","format[resources]":"map","include[songs]":"artists","extend":"artistUrl"}),&headers).await?;
            let j: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
            let mut list = vec![];
            for s in j["results"]["songs"]["data"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let id = s["id"].as_str().unwrap_or("");
                let a = &j["resources"]["songs"][id]["attributes"];
                let c = Candidate {
                    provider_id: self.id.clone(),
                    candidate_id: id.into(),
                    title: a["name"].as_str().unwrap_or(&q.title).into(),
                    artist: a["artistName"].as_str().unwrap_or(&q.artist).into(),
                    duration_ms: a["durationInMillis"].as_f64(),
                    scrollable: Some(true),
                    ..Default::default()
                };
                list.push(c);
            }
            if !list.is_empty() {
                return Ok(rank(&q, list).into_iter().take(10).collect());
            }
        }
        Ok(vec![])
    }
    async fn resolve_remote(&self, c: Candidate) -> Result<Option<Text>> {
        if c.inline_result.is_some() {
            return Ok(c.inline_result);
        }
        if self.id == "kugou" {
            for format in ["krc", "lrc"] {
                let Ok(j)=self.http.json("https://lyrics.kugou.com/download",json!({"fmt":format,"charset":"utf8","client":"pc","ver":1,"id":c.candidate_id,"accesskey":c.resolve_data["accesskey"]})).await else {continue};
                let Ok(bytes) =
                    base64::engine::general_purpose::STANDARD.decode(string(&j, "content"))
                else {
                    continue;
                };
                let text = if format == "krc" {
                    crate::formats::decrypt_krc(&bytes)
                } else {
                    String::from_utf8(bytes).map_err(|e| e.to_string())
                };
                if let Ok(text) = text {
                    if !text.trim().is_empty() {
                        return Ok(Some(Text {
                            text,
                            format: format.into(),
                            ..Default::default()
                        }));
                    }
                }
            }
            return Ok(None);
        }
        if self.id == "paxsenix" {
            let headers = BTreeMap::from([("User-Agent".into(), "Haiku/1.0.0".into())]);
            let body = self
                .http
                .get(
                    "https://lyrics.paxsenix.org/apple-music/lyrics",
                    json!({"id":c.candidate_id}),
                    &headers,
                )
                .await?;
            let j: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
            if let Some(text) = j["ttmlContent"].as_str() {
                if crate::formats::parse_ttml(text).synced {
                    return Ok(Some(Text {
                        text: text.into(),
                        format: "ttml".into(),
                        ..Default::default()
                    }));
                }
            }
            for key in ["elrcMultiPerson", "elrc", "plain"] {
                if let Some(text) = j[key].as_str().filter(|s| !s.is_empty()) {
                    let text = regex::Regex::new(r"\{agent:[^}]*\}|\{bg\}")
                        .unwrap()
                        .replace_all(text, "")
                        .into_owned();
                    return Ok(Some(Text {
                        text,
                        format: if key == "plain" { "plain" } else { "lrc" }.into(),
                        ..Default::default()
                    }));
                }
            }
        }
        Ok(None)
    }
}
fn url_component(s: &str) -> String {
    s.as_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(b) {
                (*b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
impl Provider for Remote {
    fn id(&self) -> &str {
        &self.id
    }
    fn search(&self, q: Query) -> BoxFuture<'_, Result<Vec<Candidate>>> {
        Box::pin(self.search_remote(q))
    }
    fn resolve(&self, c: Candidate) -> BoxFuture<'_, Result<Option<Text>>> {
        Box::pin(self.resolve_remote(c))
    }
}
