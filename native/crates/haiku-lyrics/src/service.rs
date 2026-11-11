//! Cancellable, reference-compatible lyrics authority. App supplies local/plugin adapters.
use crate::providers::{self, Candidate, Provider, Query, Result, Text};
use futures::future::BoxFuture;
use haiku_model::{Artist, Track};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::watch;

pub trait Origin: Send + Sync {
    fn local(&self, track: Track) -> BoxFuture<'_, Result<Option<Text>>>;
    fn plugin(&self, track: Track) -> BoxFuture<'_, Result<Option<Text>>>;
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
    pub media_cache_enabled: bool,
    pub lyrics_auto_search: bool,
    pub lyrics_provider_order: Vec<String>,
    pub lyrics_providers_disabled: Vec<String>,
    pub lyrics_custom_providers: Vec<providers::Custom>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            media_cache_enabled: true,
            lyrics_auto_search: true,
            lyrics_provider_order: vec![],
            lyrics_providers_disabled: vec!["lyricsplus".into()],
            lyrics_custom_providers: vec![],
        }
    }
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub loading: bool,
    pub track: Option<Track>,
    pub generation: u64,
    pub source: Option<String>,
    pub lyrics: Option<crate::Lyrics>,
    pub error: Option<String>,
    pub source_errors: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchReport {
    pub candidates: Vec<Candidate>,
    pub source_errors: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cached {
    pub result: Text,
    pub source: String,
    pub manual: bool,
    pub fingerprint: String,
    pub saved_at: u64,
}
pub fn query(t: &Track) -> Query {
    Query {
        title: t.title.clone(),
        artist: artists(t).join(", "),
        album: t.album.as_ref().map(|a| a.title.clone()),
        duration_sec: t.duration.unwrap_or(-1.),
    }
}
fn artists(t: &Track) -> Vec<String> {
    t.artists
        .iter()
        .map(|a| match a {
            Artist::Name(n) | Artist::Entity { name: n, .. } => n.clone(),
        })
        .collect()
}
pub fn key(t: &Track) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(&t.source, &t.id)).unwrap())
    )
}
pub fn fingerprint(t: &Track) -> String {
    serde_json::to_string(&(
        &t.title,
        artists(t),
        t.album.as_ref().map(|a| &a.title),
        t.duration,
    ))
    .unwrap()
}
#[derive(Clone)]
pub struct Service {
    root: PathBuf,
    providers: Vec<Arc<dyn Provider>>,
    http: providers::Http,
    state: watch::Sender<Snapshot>,
    gate: Arc<Mutex<u64>>,
    cancel: watch::Sender<u64>,
    preview_cancel: watch::Sender<u64>,
    pub source_timeout: Duration,
    pub total_timeout: Duration,
}
impl Service {
    pub fn new(root: PathBuf, http: providers::Http) -> Self {
        Self::with_providers(root, providers::builtins(http.clone()), http)
    }
    pub fn with_providers(
        root: PathBuf,
        providers: Vec<Arc<dyn Provider>>,
        http: providers::Http,
    ) -> Self {
        Self {
            root,
            providers,
            http,
            state: watch::channel(Snapshot::default()).0,
            gate: Arc::new(Mutex::new(0)),
            cancel: watch::channel(0).0,
            preview_cancel: watch::channel(0).0,
            source_timeout: Duration::from_secs(8),
            total_timeout: Duration::from_secs(25),
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.state.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.state.subscribe()
    }
    /// Invalidate before starting any replacement work, including clearing the queue.
    pub fn clear(&self, track: Option<Track>) -> u64 {
        let mut generation = self.gate.lock().unwrap();
        *generation += 1;
        self.cancel.send_replace(*generation);
        self.state.send_replace(Snapshot {
            loading: track.is_some(),
            track,
            generation: *generation,
            ..Default::default()
        });
        *generation
    }
    fn providers(&self, o: &Options) -> Vec<Arc<dyn Provider>> {
        let mut all = self.providers.clone();
        all.extend(
            o.lyrics_custom_providers
                .iter()
                .cloned()
                .map(|c| providers::custom(c, self.http.clone())),
        );
        all.retain(|p| !o.lyrics_providers_disabled.iter().any(|id| id == p.id()));
        all.sort_by_key(|p| {
            o.lyrics_provider_order
                .iter()
                .position(|id| id == p.id())
                .unwrap_or(usize::MAX)
        });
        all
    }
    fn path(&self, t: &Track) -> PathBuf {
        self.root.join(format!("{}.json", key(t)))
    }
    fn read(&self, t: &Track, o: &Options) -> Option<Cached> {
        let bytes = std::fs::read(self.path(t)).ok()?;
        if bytes.len() > 1_048_576 {
            return None;
        }
        let cached: Cached = serde_json::from_slice(&bytes).ok()?;
        (cached.manual || (o.media_cache_enabled && cached.fingerprint == fingerprint(t)))
            .then_some(cached)
    }
    fn persist(
        &self,
        t: &Track,
        result: &Text,
        source: &str,
        manual: bool,
        o: &Options,
    ) -> Result<()> {
        if !manual && !o.media_cache_enabled {
            return Ok(());
        }
        use std::io::Write;
        std::fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        let saved_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let item = Cached {
            result: result.reference_text(),
            source: source.into(),
            manual,
            fingerprint: fingerprint(t),
            saved_at,
        };
        let mut file = tempfile::NamedTempFile::new_in(&self.root).map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut file, &item).map_err(|e| e.to_string())?;
        file.flush()
            .and_then(|_| file.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        file.persist(self.path(t)).map_err(|e| e.to_string())?;
        Ok(())
    }
    /// Load work is dropped on replacement. Generation guards publication and disk writes.
    pub async fn load(&self, t: Track, o: Options, origin: Arc<dyn Origin>) {
        let generation = self.clear(Some(t.clone()));
        self.load_generation(t, o, origin, generation).await;
    }
    /// The caller may invalidate synchronously on the engine before scheduling background work.
    pub async fn load_generation(
        &self,
        t: Track,
        o: Options,
        origin: Arc<dyn Origin>,
        generation: u64,
    ) {
        let mut cancel = self.cancel.subscribe();
        if *cancel.borrow_and_update() != generation {
            return;
        }
        let mut errors = BTreeMap::new();
        let mut best: Option<(Text, String)> = None;
        let completed = {
            let work = self.chain(&t, &o, origin, &mut best, &mut errors);
            tokio::pin!(work);
            tokio::select! {
                _=cancel.changed()=>false,
                result=tokio::time::timeout(self.total_timeout,&mut work)=>result.is_ok()
            }
        };
        if *self.gate.lock().unwrap() != generation {
            return;
        }
        // End the borrow of the chain before publishing collected state.
        self.publish_result(t, o, generation, best, errors, completed);
    }
    fn publish_result(
        &self,
        t: Track,
        o: Options,
        generation: u64,
        best: Option<(Text, String)>,
        mut errors: BTreeMap<String, String>,
        completed: bool,
    ) {
        let guard = self.gate.lock().unwrap();
        if *guard != generation {
            return;
        }
        if !completed {
            errors.insert("total".into(), "Lyrics total timeout (25 seconds)".into());
        }
        if let Some((raw, source)) = &best {
            // Preserve an existing manual cache exactly; automatic writes never downgrade it.
            if !self.read(&t, &o).is_some_and(|c| c.manual) {
                if let Err(e) = self.persist(&t, raw, source, false, &o) {
                    errors.insert("cache".into(), e);
                }
            }
        }
        let lyrics = best.as_ref().map(|(t, _)| t.parsed());
        self.state.send_replace(Snapshot {
            loading: false,
            track: Some(t),
            generation,
            source: best.map(|(_, s)| s),
            error: if lyrics.is_none() && !errors.is_empty() {
                Some("未找到歌词，部分来源失败".into())
            } else {
                None
            },
            lyrics,
            source_errors: errors,
        });
    }
    async fn chain(
        &self,
        t: &Track,
        o: &Options,
        origin: Arc<dyn Origin>,
        best: &mut Option<(Text, String)>,
        errors: &mut BTreeMap<String, String>,
    ) {
        let take = |best: &mut Option<(Text, String)>, text: Text, source: String| {
            if text.quality() > best.as_ref().map(|(r, _)| r.quality()).unwrap_or(0) {
                *best = Some((text, source));
            }
        };
        if let Some(cache) = self.read(t, o) {
            let quality = cache.result.quality();
            take(best, cache.result, cache.source);
            if cache.manual || quality >= 2 {
                return;
            }
        }
        for (id, future) in [
            ("local-file", origin.local(t.clone())),
            ("plugin", origin.plugin(t.clone())),
        ] {
            match tokio::time::timeout(self.source_timeout, future).await {
                Ok(Ok(Some(text))) => {
                    let quality = text.quality();
                    take(
                        best,
                        text,
                        if id == "plugin" {
                            t.source.clone()
                        } else {
                            id.into()
                        },
                    );
                    if quality >= 2 {
                        return;
                    }
                }
                Ok(Ok(None)) => {}
                Ok(Err(e)) => {
                    errors.insert(id.into(), e);
                }
                Err(_) => {
                    errors.insert(id.into(), "Lyrics source timeout (8 seconds)".into());
                }
            }
        }
        if !o.lyrics_auto_search {
            return;
        }
        let q = query(t);
        for p in self.providers(o) {
            let work = async {
                let mut candidates = p.search(q.clone()).await?;
                if p.id() == "lrclib" && q.duration_sec > 0. {
                    candidates.retain(|c| {
                        (c.duration_ms.unwrap_or(0.) / 1000. - q.duration_sec).abs() <= 5.
                    });
                    candidates.sort_by(|a, b| {
                        b.scrollable.cmp(&a.scrollable).then_with(|| {
                            (a.duration_ms.unwrap_or(0.) / 1000. - q.duration_sec)
                                .abs()
                                .total_cmp(
                                    &(b.duration_ms.unwrap_or(0.) / 1000. - q.duration_sec).abs(),
                                )
                        })
                    });
                } else if !["kugou", "paxsenix"].contains(&p.id()) {
                    candidates = providers::rank(&q, candidates);
                }
                for candidate in
                    candidates
                        .into_iter()
                        .take(if p.id() == "paxsenix" { 10 } else { 1 })
                {
                    if let Some(text) = p.resolve(candidate).await? {
                        take(best, text, p.id().into());
                        if best.as_ref().is_some_and(|(t, _)| t.quality() == 3) {
                            break;
                        }
                    }
                }
                Ok::<_, String>(())
            };
            match tokio::time::timeout(self.source_timeout, work).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    errors.insert(p.id().into(), e);
                }
                Err(_) => {
                    errors.insert(p.id().into(), "Lyrics source timeout (8 seconds)".into());
                }
            }
            if best.as_ref().is_some_and(|(t, _)| t.quality() == 3) {
                break;
            }
        }
    }
    /// Search enabled providers concurrently; cancellation and the global deadline apply.
    pub async fn search(
        &self,
        q: Query,
        filter: Option<Vec<String>>,
        o: Options,
    ) -> Result<Vec<Candidate>> {
        Ok(self.search_report(q, filter, o).await?.candidates)
    }
    /// Keep successful candidates and report failures independently for each source.
    pub async fn search_report(
        &self,
        q: Query,
        filter: Option<Vec<String>>,
        o: Options,
    ) -> Result<SearchReport> {
        let generation = *self.gate.lock().unwrap();
        let mut cancel = self.cancel.subscribe();
        let work = async {
            let jobs = self
                .providers(&o)
                .into_iter()
                .filter(|p| {
                    filter
                        .as_ref()
                        .is_none_or(|f| f.iter().any(|id| id == p.id()))
                })
                .map(|p| {
                    let q = q.clone();
                    async move {
                        let result = tokio::time::timeout(self.source_timeout, p.search(q))
                            .await
                            .map_err(|_| "Lyrics source timeout (8 seconds)".to_string())
                            .and_then(|r| r);
                        (p.id().to_string(), result)
                    }
                });
            let mut report = SearchReport::default();
            for (id, result) in futures::future::join_all(jobs).await {
                match result {
                    Ok(candidates) => report.candidates.extend(candidates),
                    Err(error) => {
                        report.source_errors.insert(id, error);
                    }
                }
            }
            report.candidates = providers::rank(&q, report.candidates);
            report
        };
        let result = tokio::select! {_=cancel.changed()=>return Err("Lyrics request cancelled".into()),
        result=tokio::time::timeout(self.total_timeout,work)=>result.map_err(|_|"Lyrics search timeout".to_string())?};
        if *self.gate.lock().unwrap() != generation {
            return Err("Lyrics request replaced".into());
        }
        Ok(result)
    }
    /// Reserve synchronously before scheduling a preview; changing candidates drops old work.
    pub fn cancel_preview(&self) -> u64 {
        let mut ticket = 0;
        self.preview_cancel.send_modify(|v| {
            *v += 1;
            ticket = *v;
        });
        ticket
    }
    pub async fn preview(&self, c: Candidate, o: Options, ticket: u64) -> Result<Text> {
        let mut cancel = self.preview_cancel.subscribe();
        let mut track_cancel = self.cancel.subscribe();
        if *cancel.borrow_and_update() != ticket {
            return Err("Lyrics preview cancelled".into());
        }
        tokio::select! {
            _ = cancel.changed() => Err("Lyrics preview cancelled".into()),
            _ = track_cancel.changed() => Err("Track changed".into()),
            result = self.resolve(c, o) => result,
        }
    }
    pub async fn resolve(&self, c: Candidate, o: Options) -> Result<Text> {
        if let Some(text) = c.inline_result {
            return Ok(text);
        }
        let p = self
            .providers(&o)
            .into_iter()
            .find(|p| p.id() == c.provider_id)
            .ok_or("Lyrics provider disabled or missing")?;
        tokio::time::timeout(self.source_timeout, p.resolve(c))
            .await
            .map_err(|_| "Lyrics resolve timeout".to_string())??
            .ok_or("Candidate has no lyrics".into())
    }
    /// Resolve before committing, then reject a selection made for an obsolete track.
    pub async fn adopt(&self, c: Candidate, o: Options) -> Result<()> {
        let snapshot = self.snapshot();
        let track = snapshot.track.ok_or("No current track")?;
        let source = format!("{}:{}", c.provider_id, c.candidate_id);
        let raw = self.resolve(c, o.clone()).await?;
        let mut guard = self.gate.lock().unwrap();
        if *guard != snapshot.generation {
            return Err("Track changed".into());
        }
        self.persist(&track, &raw, &source, true, &o)?;
        *guard += 1;
        self.cancel.send_replace(*guard);
        self.state.send_replace(Snapshot {
            loading: false,
            track: Some(track),
            generation: *guard,
            lyrics: Some(raw.parsed()),
            source: Some(source),
            ..Default::default()
        });
        Ok(())
    }
    pub fn reset(&self) -> Result<()> {
        let guard = self.gate.lock().unwrap();
        if let Some(t) = self.snapshot().track {
            match std::fs::remove_file(self.path(&t)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        drop(guard);
        Ok(())
    }
}
