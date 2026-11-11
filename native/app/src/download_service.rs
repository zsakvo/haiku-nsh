//! Engine-owned download queue. Task identity fences events and filesystem publication;
//! a paused task remains registered until its transfer has released the partial file.
use futures::StreamExt;
use haiku_model::{EntityRef, Track};
use haiku_plugins::{Cancellation, Host, PageArgs, SourceApi};
use haiku_services::downloader::{self, AudioTags, CancelToken, Job, Update};
use haiku_store::{json_file, settings::SettingsStore};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::watch;

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub key: String,
    pub track: Track,
    pub status: String,
    pub quality: String,
    pub actual_quality: Option<String>,
    pub path: Option<String>,
    pub temp_path: Option<String>,
    pub received: u64,
    pub total: u64,
    pub file_size: u64,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub attempts: u32,
    pub origin: String,
    #[serde(default)]
    pub playlist_ids: Vec<String>,
    pub playlist_name: Option<String>,
    pub index: Option<usize>,
    pub enqueued_at: u64,
    pub completed_at: Option<u64>,
    #[serde(default)]
    pub sidecars: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
    // Preserve reference-only metadata (provider, headers, etc.) on read/write.
    #[serde(skip)]
    reference_track: Value,
}
struct Active {
    id: String,
    cancel: Arc<CancelToken>,
}
#[derive(Default)]
struct State {
    items: Vec<Item>,
    active: BTreeMap<String, Active>,
    retry_at: BTreeMap<String, Instant>,
    stopped: bool,
}
pub struct Service {
    root: PathBuf,
    host: Arc<Host>,
    settings: Arc<SettingsStore>,
    state: Mutex<State>,
    events: watch::Sender<Vec<Item>>,
    retry_delays: Vec<Duration>,
}
struct PluginCancel(Cancellation);
impl Drop for PluginCancel {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
impl Service {
    pub fn open(root: &Path, host: Arc<Host>, settings: Arc<SettingsStore>) -> Result<Arc<Self>> {
        let records: Vec<Value> = json_file::read(&root.join("haiku_state/downloads.json"))
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let mut items = records
            .into_iter()
            .map(read_item)
            .collect::<Result<Vec<_>>>()?;
        for item in &mut items {
            item.key = key(&item.track);
            if active_status(&item.status) {
                item.status = "queued".into();
            }
            if item.status == "completed"
                && !item.path.as_ref().is_some_and(|p| Path::new(p).is_file())
            {
                item.status = "missing".into();
            }
        }
        let (events, _) = watch::channel(items.clone());
        let service = Arc::new(Self {
            root: root.into(),
            host,
            settings,
            state: Mutex::new(State {
                items,
                ..Default::default()
            }),
            events,
            retry_delays: vec![Duration::from_secs(2), Duration::from_secs(8)],
        });
        service.save(&service.state.lock().unwrap())?;
        Ok(service)
    }
    pub fn snapshot(&self) -> Vec<Item> {
        self.state.lock().unwrap().items.clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<Vec<Item>> {
        self.events.subscribe()
    }
    fn save(&self, state: &State) -> Result<()> {
        json_file::write(
            &self.root.join("haiku_state/downloads.json"),
            &state.items.iter().map(write_item).collect::<Vec<_>>(),
        )
        .map_err(|e| e.to_string())?;
        self.events.send_replace(state.items.clone());
        Ok(())
    }
    pub async fn enqueue(self: &Arc<Self>, input: Value) -> Result<()> {
        let (tracks, playlist_name, playlist_id) = if input.is_array() {
            (
                serde_json::from_value::<Vec<Track>>(input).map_err(|e| e.to_string())?,
                None,
                None,
            )
        } else if input.get("kind").is_some() {
            let reference: EntityRef = serde_json::from_value(input).map_err(|e| e.to_string())?;
            if !["playlist", "album", "toplist"].contains(&reference.kind.as_str()) {
                return Err("下载仅支持曲目数组、歌单、专辑或榜单引用".into());
            }
            let api = SourceApi::new(
                self.host
                    .source(&reference.source)
                    .map_err(|e| e.to_string())?,
            );
            let guard = PluginCancel(Default::default());
            let first = api
                .detail(
                    &reference.kind,
                    &reference.id,
                    PageArgs::default(),
                    Some(&guard.0),
                )
                .await
                .map_err(|e| e.to_string())?;
            let title = first.info.map(|info| info.title);
            let tracks = SourceApi::collect_all(|cursor| {
                let api = &api;
                let reference = &reference;
                let guard = &guard;
                async move {
                    api.detail(
                        &reference.kind,
                        &reference.id,
                        PageArgs { cursor, limit: 100 },
                        Some(&guard.0),
                    )
                    .await
                    .map(|d| d.tracks)
                }
            })
            .await
            .map_err(|e| e.to_string())?;
            (tracks, title, Some(reference.encode()))
        } else {
            (
                vec![serde_json::from_value::<Track>(input).map_err(|e| e.to_string())?],
                None,
                None,
            )
        };
        let quality = self.settings.get().download_quality;
        let mut state = self.state.lock().unwrap();
        if state.stopped {
            return Err("下载服务已关闭".into());
        }
        let multiple = tracks.len() > 1;
        for (index, track) in tracks.into_iter().enumerate() {
            if track.source.contains('\0') || track.id.contains('\0') {
                return Err("来源或曲目 ID 不能包含 NUL".into());
            }
            let key = key(&track);
            if let Some(item) = state.items.iter_mut().find(|i| i.key == key) {
                item.origin = "manual".into();
                if let Some(id) = &playlist_id {
                    if !item.playlist_ids.contains(id) {
                        item.playlist_ids.push(id.clone());
                    }
                }
                continue;
            }
            let provider = self
                .host
                .source(&track.source)
                .ok()
                .map(|s| s.record.plugin_id.clone())
                .unwrap_or_else(|| track.source.clone());
            state.items.push(Item {
                key,
                reference_track: json!({"provider":provider}),
                track,
                status: "queued".into(),
                quality: quality.clone(),
                actual_quality: None,
                path: None,
                temp_path: None,
                received: 0,
                total: 0,
                file_size: 0,
                error: None,
                warning: None,
                attempts: 0,
                origin: "manual".into(),
                playlist_ids: playlist_id.clone().into_iter().collect(),
                playlist_name: playlist_name.clone(),
                index: multiple.then_some(index + 1),
                enqueued_at: now(),
                completed_at: None,
                sidecars: vec![],
                extra: BTreeMap::new(),
            });
        }
        self.save(&state)?;
        drop(state);
        self.pump();
        Ok(())
    }
    /// Called after enqueue, startup and settings updates. Workers own slots through cancellation.
    pub fn pump(self: &Arc<Self>) {
        let concurrency = self.settings.get().download_concurrency as usize;
        let mut state = self.state.lock().unwrap();
        while !state.stopped && state.active.len() < concurrency {
            let next = state.items.iter().position(|i| {
                i.status == "queued"
                    && !state.active.contains_key(&i.key)
                    && !state
                        .retry_at
                        .get(&i.key)
                        .is_some_and(|time| *time > Instant::now())
            });
            let Some(index) = next else { break };
            let id = uuid::Uuid::new_v4().to_string();
            let cancel = CancelToken::new();
            let item = &mut state.items[index];
            item.status = "resolving".into();
            item.attempts += 1;
            item.error = None;
            let key = item.key.clone();
            state.active.insert(
                key.clone(),
                Active {
                    id: id.clone(),
                    cancel: cancel.clone(),
                },
            );
            if let Err(error) = self.save(&state) {
                state.active.remove(&key);
                let item = &mut state.items[index];
                item.status = "failed".into();
                item.error = Some(error);
                self.events.send_replace(state.items.clone());
                break;
            }
            let service = self.clone();
            downloader::runtime().spawn(async move {
                // Drop the complete execution future on cancellation (streams, assets, lyrics).
                let result = tokio::select! {
                    biased;
                    _ = cancelled(&cancel) => Err("已取消".into()),
                    r = service.execute(&key, &id) => r,
                };
                service.finish(&key, &id, result);
            });
        }
    }
    fn current<'a>(state: &'a mut State, key: &str, id: &str) -> Result<&'a mut Item> {
        if !state
            .active
            .get(key)
            .is_some_and(|a| a.id == id && !a.cancel.is_cancelled())
        {
            return Err("已取消或过期的下载任务".into());
        }
        state
            .items
            .iter_mut()
            .find(|i| i.key == key)
            .ok_or_else(|| "下载任务不存在".into())
    }
    fn mutate(&self, key: &str, id: &str, change: impl FnOnce(&mut Item)) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        change(Self::current(&mut state, key, id)?);
        self.save(&state)
    }
    async fn execute(&self, key: &str, id: &str) -> Result<()> {
        let settings = self.settings.get();
        let (item, cancel) = {
            let mut state = self.state.lock().unwrap();
            let item = Self::current(&mut state, key, id)?.clone();
            (item, state.active[key].cancel.clone())
        };
        let directory = if settings.download_dir.is_empty() {
            PathBuf::from(std::env::var_os("HOME").ok_or("未找到用户目录")?).join("Music/Haiku")
        } else {
            PathBuf::from(&settings.download_dir)
        };
        if !directory.is_absolute() {
            return Err("下载目录必须是绝对路径".into());
        }
        let temp = item
            .temp_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                directory.join(".haiku-incomplete").join(format!(
                    "{}.part",
                    haiku_audio::media_cache::file_id(&format!("{key}|{}", item.quality))
                ))
            });
        fs::create_dir_all(temp.parent().ok_or("无效下载路径")?).map_err(|e| e.to_string())?;
        self.mutate(key, id, |i| {
            i.temp_path = Some(temp.to_string_lossy().into())
        })?;
        // Inspect only this Core's cache root. Never consult the process-wide cache singleton.
        let cache_key = haiku_playback::backend::cache_key(&item.track, &item.quality);
        let cache = cache_key.map(|k| {
            self.root
                .join("media-cache/objects")
                .join(haiku_audio::media_cache::file_id(&k))
        });
        let local = item.track.local_path.as_ref().map(PathBuf::from);
        let cached = settings
            .download_reuse_media_cache
            .then_some(cache)
            .flatten()
            .filter(|p| p.metadata().is_ok_and(|m| m.is_file() && m.len() > 0));
        let mut content_type = None;
        if let Some(path) = local.or(cached) {
            fs::copy(path, &temp).map_err(|e| e.to_string())?;
            self.mutate(key, id, |i| i.actual_quality = Some(i.quality.clone()))?;
        } else {
            let api = SourceApi::new(
                self.host
                    .source(&item.track.source)
                    .map_err(|e| e.to_string())?,
            );
            let guard = PluginCancel(Default::default());
            let streams = api
                .streams(&item.track.id, &item.quality, None, Some(&guard.0))
                .await
                .map_err(|e| e.to_string())?;
            let actual = streams.first().and_then(|s| s.quality.clone());
            let candidates = streams
                .into_iter()
                .filter(|s| s.url.starts_with("http://") || s.url.starts_with("https://"))
                .map(|s| downloader::Candidate {
                    url: s.url,
                    headers: s.headers.into_iter().collect(),
                    mime_type: s.format,
                })
                .collect();
            self.mutate(key, id, |i| {
                i.status = "downloading".into();
                i.actual_quality = actual;
            })?;
            content_type = downloader::transfer(
                &Job {
                    candidates,
                    temp_path: temp.clone(),
                    final_path: PathBuf::new(),
                    tag: None,
                },
                &cancel,
                |event| {
                    if let Update::Progress { received, total } = event {
                        // Progress broadcasts are throttled by the downloader and need no fsync.
                        let mut state = self.state.lock().unwrap();
                        if let Ok(item) = Self::current(&mut state, key, id) {
                            item.received = received;
                            item.total = total;
                            self.events.send_replace(state.items.clone());
                        }
                    }
                },
            )
            .await?;
        }
        if temp.metadata().map_err(|e| e.to_string())?.len() == 0 {
            return Err("下载内容为空".into());
        }
        let mut warnings = vec![];
        let cover = if settings.download_cover_mode != "none" {
            match self.cover(&item.track, settings.download_cover_hd).await {
                Ok(cover) => cover,
                Err(error) => {
                    warnings.push(format!("封面：{error}"));
                    None
                }
            }
        } else {
            None
        };
        let lyrics = if settings.download_lyrics_mode != "none" {
            // A per-job authority shares durable selections/cache, never the player's generation.
            let authority =
                haiku_lyrics::service::Service::new(self.root.join("lyrics"), Default::default());
            let options = serde_json::from_value(serde_json::to_value(&settings).unwrap())
                .map_err(|e| e.to_string())?;
            authority
                .load(
                    item.track.clone(),
                    options,
                    Arc::new(crate::lyrics::Sources(self.host.clone())),
                )
                .await;
            let snapshot = authority.snapshot();
            for (source, error) in snapshot.source_errors {
                warnings.push(format!("歌词 {source}：{error}"));
            }
            snapshot
                .lyrics
                .map(|l| lrc(l, settings.download_lyrics_translation))
        } else {
            None
        };
        let tag = AudioTags {
            title: item.track.title.clone(),
            artists: haiku_library::service::artist_names(&item.track),
            album: item.track.album.as_ref().map(|a| a.title.clone()),
            track_no: item.index.map(|i| i as u32),
            cover_path: if embeds(&settings.download_cover_mode) {
                cover.as_ref().map(|p| p.to_string_lossy().into())
            } else {
                None
            },
            lyrics: embeds(&settings.download_lyrics_mode)
                .then(|| lyrics.clone())
                .flatten(),
            ..Default::default()
        };
        self.mutate(key, id, |i| i.status = "tagging".into())?;
        // One queue lock covers path reservation, finalization and publication. Pause/remove
        // can win before this section or observe its completed result, never orphan a late file.
        let mut state = self.state.lock().unwrap();
        Self::current(&mut state, key, id)?;
        let extension = downloader::detect_extension(&temp, None, content_type.as_deref())
            .unwrap_or_else(|| "mp3".into());
        let relative = relative_path(&item, &settings, &extension);
        let final_path = reserve(&state, &directory.join(relative), &key)?;
        let done = downloader::finalize(
            &temp,
            &final_path,
            Some(&tag),
            None,
            content_type.as_deref(),
        )?;
        if !done.warning.is_empty() {
            warnings.push(done.warning);
        }
        let mut sidecars = vec![];
        let mut sidecar = |extension: &str, bytes: &[u8]| {
            let path = final_path.with_extension(extension);
            match fs::write(&path, bytes) {
                Ok(()) => sidecars.push(path.to_string_lossy().into_owned()),
                Err(e) => warnings.push(format!("侧车写入失败：{e}")),
            }
        };
        if sidecars_enabled(&settings.download_lyrics_mode) {
            if let Some(lyrics) = &lyrics {
                sidecar("lrc", lyrics.as_bytes());
            }
        }
        if sidecars_enabled(&settings.download_cover_mode) {
            if let Some(cover) = &cover {
                match fs::read(cover) {
                    Ok(bytes) => sidecar(
                        cover.extension().and_then(|e| e.to_str()).unwrap_or("jpg"),
                        &bytes,
                    ),
                    Err(e) => warnings.push(format!("封面侧车读取失败：{e}")),
                }
            }
        }
        let size = final_path.metadata().map_err(|e| e.to_string())?.len();
        let item = Self::current(&mut state, key, id)?;
        item.path = Some(final_path.to_string_lossy().into());
        item.file_size = size;
        item.received = size;
        item.total = size;
        item.sidecars = sidecars;
        item.completed_at = Some(now());
        item.status = "completed".into();
        item.warning = (!warnings.is_empty()).then(|| warnings.join("; "));
        item.error = None;
        self.save(&state)
    }
    async fn cover(&self, track: &Track, hd: bool) -> Result<Option<PathBuf>> {
        let Some(mut url) = track.cover.clone() else {
            return Ok(None);
        };
        // SDK image templates accept the same size token as SourceApi.
        url = url.replace("{size}", if hd { "1000" } else { "500" });
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Ok(Some(PathBuf::from(
                url.strip_prefix("file://").unwrap_or(&url),
            )));
        }
        let mut request = reqwest::Client::new()
            .get(&url)
            .timeout(Duration::from_secs(20));
        if let Ok(source) = self.host.source(&track.source) {
            if let Some(headers) = source
                .descriptor()
                .http
                .and_then(|h| h["headers"].as_object().cloned())
            {
                for (name, value) in headers {
                    if let Some(value) = value.as_str() {
                        request = request.header(name, value);
                    }
                }
            }
        }
        let mut stream = request
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .bytes_stream();
        let mut bytes = vec![];
        while let Some(chunk) = stream.next().await {
            bytes.extend_from_slice(&chunk.map_err(|e| e.to_string())?);
            if bytes.len() > 20 * 1024 * 1024 {
                return Err("封面超过 20 MiB".into());
            }
        }
        let extension = if bytes.starts_with(b"\x89PNG") {
            "png"
        } else if bytes.starts_with(b"RIFF") {
            "webp"
        } else {
            "jpg"
        };
        let dir = self.root.join("haiku_state/download-assets");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!(
            "{}.{}",
            haiku_audio::media_cache::file_id(&url),
            extension
        ));
        let temp = tempfile::NamedTempFile::new_in(&dir).map_err(|e| e.to_string())?;
        fs::write(temp.path(), bytes).map_err(|e| e.to_string())?;
        temp.persist(&path).map_err(|e| e.to_string())?;
        Ok(Some(path))
    }
    fn finish(self: &Arc<Self>, key: &str, id: &str, result: Result<()>) {
        let mut state = self.state.lock().unwrap();
        if !state.active.get(key).is_some_and(|a| a.id == id) {
            return;
        }
        let active = state.active.remove(key).unwrap();
        let mut retry_delay = None;
        if !active.cancel.is_cancelled() && !state.stopped {
            if let Err(error) = result {
                if let Some(item) = state.items.iter_mut().find(|i| i.key == key) {
                    // A disk persistence error after publication must never retry the audio.
                    if item.status != "completed" {
                        item.error = Some(error);
                        retry_delay = self
                            .retry_delays
                            .get(item.attempts.saturating_sub(1) as usize)
                            .copied();
                        item.status = if retry_delay.is_some() {
                            "queued"
                        } else {
                            "failed"
                        }
                        .into();
                    }
                }
            }
        }
        if let Some(delay) = retry_delay {
            state.retry_at.insert(key.into(), Instant::now() + delay);
            let service = self.clone();
            downloader::runtime().spawn(async move {
                tokio::time::sleep(delay).await;
                service.pump();
            });
        }
        if let Err(e) = self.save(&state) {
            log::error!("Downloads persistence: {e}");
        }
        // Also notify shutdown waiters when the last canceled worker drains.
        self.events.send_replace(state.items.clone());
        drop(state);
        self.pump();
    }
    pub fn control(self: &Arc<Self>, key: &str, action: &str) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        let item = state
            .items
            .iter_mut()
            .find(|i| i.key == key)
            .ok_or("下载任务不存在")?;
        match action {
            "pause" if !["completed", "missing", "canceled"].contains(&item.status.as_str()) => {
                item.status = "paused".into()
            }
            "cancel" if item.status != "completed" => item.status = "canceled".into(),
            "resume" | "retry"
                if ["paused", "failed", "missing", "canceled"].contains(&item.status.as_str()) =>
            {
                item.status = "queued".into();
                item.attempts = 0;
                item.error = None;
            }
            _ => return Ok(()),
        }
        if ["pause", "cancel"].contains(&action) {
            if let Some(active) = state.active.get(key) {
                active.cancel.cancel();
            }
        }
        state.retry_at.remove(key);
        self.save(&state)?;
        drop(state);
        self.pump();
        Ok(())
    }
    pub async fn remove(self: &Arc<Self>, key: &str, with_file: bool) -> Result<()> {
        // Preserve completed state until its files have been handled.
        self.control(key, "cancel")?;
        let mut events = self.subscribe();
        loop {
            if !self.state.lock().unwrap().active.contains_key(key) {
                break;
            }
            events.changed().await.map_err(|e| e.to_string())?;
        }
        let mut state = self.state.lock().unwrap();
        let item = state
            .items
            .iter()
            .find(|i| i.key == key)
            .ok_or("下载任务不存在")?;
        if with_file {
            for path in item
                .path
                .iter()
                .chain(item.temp_path.iter())
                .chain(item.sidecars.iter())
            {
                match fs::remove_file(path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(format!("删除下载文件失败：{e}")),
                }
            }
        }
        state.items.retain(|i| i.key != key);
        self.save(&state)
    }
    pub fn clear_completed(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.items.retain(|i| i.status != "completed");
        self.save(&state)
    }
    pub fn retry_failed(self: &Arc<Self>) -> Result<()> {
        for item in self
            .snapshot()
            .iter()
            .filter(|i| ["failed", "missing"].contains(&i.status.as_str()))
        {
            self.control(&item.key, "retry")?;
        }
        Ok(())
    }
    pub fn verify(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        for item in &mut state.items {
            if item.status == "completed"
                && !item.path.as_ref().is_some_and(|p| Path::new(p).is_file())
            {
                item.status = "missing".into();
            }
        }
        self.save(&state)
    }
    pub fn path(&self, key: &str) -> Result<PathBuf> {
        self.snapshot()
            .iter()
            .find(|i| i.key == key)
            .and_then(|i| i.path.as_ref())
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .ok_or_else(|| "下载文件不存在".into())
    }
    pub fn stop(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.stopped = true;
        for active in state.active.values() {
            active.cancel.cancel();
        }
        for item in &mut state.items {
            if active_status(&item.status) {
                item.status = "queued".into();
            }
        }
        self.save(&state)
    }
    pub async fn shutdown(&self) -> Result<()> {
        self.stop()?;
        let mut events = self.subscribe();
        loop {
            if self.state.lock().unwrap().active.is_empty() {
                return Ok(());
            }
            events.changed().await.map_err(|e| e.to_string())?;
        }
    }
}
async fn cancelled(token: &CancelToken) {
    while !token.is_cancelled() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
pub fn key(track: &Track) -> String {
    format!("{}\0{}", track.source, track.id)
}
fn active_status(status: &str) -> bool {
    ["resolving", "downloading", "tagging"].contains(&status)
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn embeds(mode: &str) -> bool {
    ["embed", "both"].contains(&mode)
}
fn sidecars_enabled(mode: &str) -> bool {
    ["sidecar", "both"].contains(&mode)
}
fn lrc(lyrics: haiku_lyrics::Lyrics, translation: bool) -> String {
    lyrics
        .lines
        .into_iter()
        .map(|line| {
            let stamp = format!("[{:02}:{:05.2}]", (line.at / 60.) as u64, line.at % 60.);
            let text = format!("{stamp}{}", line.text);
            match line.translation.filter(|_| translation) {
                Some(translation) => format!("{text}\n{stamp}{translation}"),
                None => text,
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn sanitize(raw: &str, fallback: &str) -> String {
    let value: String = raw
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let value = value
        .trim()
        .trim_end_matches(['.', ' '])
        .chars()
        .take(120)
        .collect::<String>();
    if value.is_empty() {
        return fallback.into();
    }
    let lower = value.to_ascii_lowercase();
    let device = lower.split('.').next().unwrap_or("");
    if ["con", "prn", "aux", "nul"].contains(&device)
        || (device.starts_with("com") || device.starts_with("lpt"))
            && device.len() == 4
            && matches!(device.as_bytes()[3], b'1'..=b'9')
    {
        format!("_{value}")
    } else {
        value
    }
}
fn relative_path(
    item: &Item,
    settings: &haiku_store::settings::Settings,
    extension: &str,
) -> PathBuf {
    let artist = haiku_library::service::artist_names(&item.track).join(" / ");
    let album = item
        .track
        .album
        .as_ref()
        .map(|a| a.title.as_str())
        .unwrap_or("");
    let provider = item.reference_track["provider"]
        .as_str()
        .unwrap_or(&item.track.source);
    let mut name = settings.download_naming.clone();
    for (token, value) in [
        ("title", item.track.title.clone()),
        ("artist", artist.clone()),
        ("album", album.into()),
        ("provider", provider.into()),
        ("quality", item.quality.clone()),
        (
            "index",
            item.index.map(|i| i.to_string()).unwrap_or_default(),
        ),
    ] {
        name = name.replace(&format!("{{{token}}}"), &value);
    }
    let mut path = match settings.download_layout.as_str() {
        "artist" => PathBuf::from(sanitize(&artist, "未知艺术家")),
        "artist-album" => {
            PathBuf::from(sanitize(&artist, "未知艺术家")).join(sanitize(album, "未知专辑"))
        }
        "playlist" => item
            .playlist_name
            .as_ref()
            .map(|p| PathBuf::from(sanitize(p, "未命名")))
            .unwrap_or_default(),
        "provider" => PathBuf::from(sanitize(provider, "未知来源")),
        _ => PathBuf::new(),
    };
    path.push(format!("{}.{}", sanitize(&name, "未命名"), extension));
    path
}
fn reserve(state: &State, target: &Path, key: &str) -> Result<PathBuf> {
    for suffix in 0..10000 {
        let path = if suffix == 0 {
            target.to_path_buf()
        } else {
            target.with_file_name(format!(
                "{} ({suffix}).{}",
                target.file_stem().unwrap_or_default().to_string_lossy(),
                target.extension().unwrap_or_default().to_string_lossy()
            ))
        };
        // Sidecar stems must be unique as well, including different audio formats.
        let base = path.with_extension("");
        let occupied = path.exists()
            || state.items.iter().any(|i| {
                i.key != key
                    && i.path
                        .as_ref()
                        .is_some_and(|p| Path::new(p).with_extension("") == base)
            })
            || ["lrc", "jpg", "png", "webp"]
                .iter()
                .any(|ext| path.with_extension(ext).exists());
        if !occupied {
            return Ok(path);
        }
    }
    Err("下载文件名冲突过多".into())
}
fn write_item(item: &Item) -> Value {
    let mut value = serde_json::to_value(item).unwrap();
    let mut track = item
        .reference_track
        .as_object()
        .cloned()
        .unwrap_or_default();
    let sdk = serde_json::to_value(&item.track).unwrap();
    track.extend(sdk.as_object().unwrap().clone());
    track.remove("source");
    track.insert("sourceId".into(), json!(item.track.source));
    track
        .entry("provider")
        .or_insert_with(|| json!(item.track.source));
    track.insert(
        "album".into(),
        json!(item.track.album.as_ref().map(|a| &a.title)),
    );
    track.insert(
        "artists".into(),
        json!(haiku_library::service::artist_names(&item.track)),
    );
    // SDK identity objects survive reference-format string fields via an additive extension.
    track.insert("haikuTrack".into(), sdk);
    value["track"] = track.into();
    value
}
fn read_item(mut value: Value) -> Result<Item> {
    let reference = value["track"].clone();
    let mut sdk = reference
        .get("haikuTrack")
        .cloned()
        .unwrap_or_else(|| reference.clone());
    if sdk.get("source").is_none() {
        sdk["source"] = reference["sourceId"].clone();
    }
    if sdk["album"].is_string() {
        sdk["album"] = json!({"title":sdk["album"]});
    }
    value["track"] = sdk;
    let mut item: Item =
        serde_json::from_value(value).map_err(|e| format!("下载记录无法读取：{e}"))?;
    item.reference_track = reference;
    Ok(item)
}

#[cfg(test)]
#[path = "download_service_tests.rs"]
mod tests;
