//! Bounded per-channel JSONL diagnostics; all persisted and exported strings are redacted.
use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

pub fn redact(text: &str) -> String {
    static RULES: OnceLock<Vec<Regex>> = OnceLock::new();
    let rules = RULES.get_or_init(|| vec![
        Regex::new(r#"(?i)(["'](?:authorization|cookie|set-cookie|x-api-key|access_token|refresh_token|password|token)["']\s*:\s*)("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')"#).unwrap(),
        Regex::new(r#"https?://[^\s<>"']+"#).unwrap(),
        Regex::new(r#"(?i)((?:authorization|cookie|set-cookie|x-api-key|access_token|refresh_token|password|token)\s*[=:]\s*)([^\r\n,;}]+)"#).unwrap(),
        Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9._~+/-]+=*").unwrap(),
    ]);
    let t = rules[0].replace_all(text, "${1}\"[REDACTED]\"");
    let t = rules[1].replace_all(&t, |c: &regex::Captures<'_>| {
        url::Url::parse(&c[0])
            .map(|mut u| {
                let _ = u.set_username("");
                let _ = u.set_password(None);
                u.set_query(None);
                u.set_fragment(None);
                u.to_string()
            })
            .unwrap_or_else(|_| "[URL]".into())
    });
    let t = rules[2].replace_all(&t, "${1}[REDACTED]");
    rules[3].replace_all(&t, "Bearer [REDACTED]").into_owned()
}
fn bounded(text: &str) -> String {
    redact(text).chars().take(8000).collect()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub time: String,
    pub session: u64,
    pub channel: String,
    pub level: String,
    pub source: String,
    pub scope: Option<String>,
    pub message: String,
    pub detail: Option<String>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Filter {
    pub channel: String,
    pub level: String,
    pub source: String,
    pub query: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Notice {
    pub id: u64,
    pub time: String,
    pub kind: String,
    pub message: String,
    pub detail: Option<String>,
    pub read: bool,
}
struct State {
    entries: Vec<LogEntry>,
    notices: Vec<Notice>,
    session: u64,
    revision: u64,
    next: u64,
}
#[derive(Clone)]
pub struct Logs {
    root: PathBuf,
    state: Arc<Mutex<State>>,
}
impl Logs {
    pub fn open(root: &Path) -> Result<Self, String> {
        let root = root.join("logs");
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let mut entries = vec![];
        for channel in ["app", "plugin"] {
            let path = root.join(format!("{channel}.jsonl"));
            if path.exists() {
                let bytes = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
                for line in bytes.lines() {
                    if let Ok(mut e) = serde_json::from_str::<LogEntry>(line) {
                        e.channel = channel.into();
                        e.message = bounded(&e.message);
                        e.detail = e.detail.as_deref().map(bounded);
                        e.source = bounded(&e.source);
                        e.scope = e.scope.as_deref().map(bounded);
                        entries.push(e);
                    }
                }
                let previous = root.join(format!("{channel}.previous.jsonl"));
                std::fs::rename(path, previous).map_err(|e| e.to_string())?;
            }
        }
        let session = entries.iter().map(|e| e.session).max().unwrap_or(0) + 1;
        let this = Self {
            root,
            state: Arc::new(Mutex::new(State {
                entries,
                notices: vec![],
                session,
                revision: 0,
                next: 0,
            })),
        };
        this.add("app", "info", "app", Some("session"), "会话开始", None)?;
        Ok(this)
    }
    pub fn revision(&self) -> u64 {
        self.state.lock().unwrap().revision
    }
    pub fn add(
        &self,
        channel: &str,
        level: &str,
        source: &str,
        scope: Option<&str>,
        message: &str,
        detail: Option<&str>,
    ) -> Result<(), String> {
        let mut s = self.state.lock().unwrap();
        let session = s.session;
        s.entries.push(LogEntry {
            time: Utc::now().to_rfc3339(),
            session,
            channel: if channel == "plugin" { "plugin" } else { "app" }.into(),
            level: if ["warn", "error"].contains(&level) {
                level
            } else {
                "info"
            }
            .into(),
            source: bounded(source),
            scope: scope.map(bounded),
            message: bounded(message),
            detail: detail.map(bounded),
        });
        let mut counts = BTreeMap::new();
        s.entries.reverse();
        s.entries.retain(|e| {
            let count = counts.entry(e.channel.clone()).or_insert(0);
            *count += 1;
            *count <= 1000
        });
        s.entries.reverse();
        s.revision += 1;
        self.persist(&s)
    }
    fn persist(&self, s: &State) -> Result<(), String> {
        use std::io::Write;
        for channel in ["app", "plugin"] {
            let mut file =
                tempfile::NamedTempFile::new_in(&self.root).map_err(|e| e.to_string())?;
            for e in s.entries.iter().filter(|e| e.channel == channel) {
                serde_json::to_writer(&mut file, e).map_err(|e| e.to_string())?;
                file.write_all(b"\n").map_err(|e| e.to_string())?;
            }
            file.flush()
                .and_then(|_| file.as_file().sync_all())
                .map_err(|e| e.to_string())?;
            file.persist(self.root.join(format!("{channel}.jsonl")))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn filter(&self, f: &Filter) -> Vec<LogEntry> {
        let needle = f.query.to_lowercase();
        self.state
            .lock()
            .unwrap()
            .entries
            .iter()
            .filter(|e| {
                (f.channel.is_empty() || e.channel == f.channel)
                    && (f.level.is_empty() || e.level == f.level)
                    && (f.source.is_empty() || e.source == f.source)
                    && format!("{} {}", e.message, e.detail.as_deref().unwrap_or(""))
                        .to_lowercase()
                        .contains(&needle)
            })
            .cloned()
            .collect()
    }
    pub fn text(&self, f: &Filter) -> String {
        self.filter(f)
            .iter()
            .map(|e| {
                format!(
                    "[{}] [session:{}] [{}/{}] [{}] {}{}",
                    e.time,
                    e.session,
                    e.channel,
                    e.level,
                    e.source,
                    e.message,
                    e.detail
                        .as_ref()
                        .map(|d| format!("\n{d}"))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub fn clear(&self, channel: &str) -> Result<(), String> {
        let mut s = self.state.lock().unwrap();
        s.entries
            .retain(|e| !channel.is_empty() && e.channel != channel);
        s.revision += 1;
        self.persist(&s)
    }
    pub fn notify(&self, kind: &str, message: &str, detail: Option<&str>) {
        let mut s = self.state.lock().unwrap();
        s.next += 1;
        let id = s.next;
        s.notices.insert(
            0,
            Notice {
                id,
                time: Utc::now().to_rfc3339(),
                kind: kind.into(),
                message: bounded(message),
                detail: detail.map(bounded),
                read: false,
            },
        );
        s.notices.truncate(200);
        s.revision += 1;
    }
    pub fn notices(&self) -> Vec<Notice> {
        self.state.lock().unwrap().notices.clone()
    }
    pub fn clear_notice(&self, id: Option<u64>) {
        let mut s = self.state.lock().unwrap();
        s.notices.retain(|n| id.is_some_and(|id| n.id != id));
        s.revision += 1;
    }
}
