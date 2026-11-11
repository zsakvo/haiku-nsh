//! Persistent, reversible metadata jobs. This service never writes audio tags.
use futures::{StreamExt, stream};
use haiku_library::service::{Library, Patch, artist_names};
use haiku_model::Track;
use haiku_plugins::{
    Cancellation, Host, ScrapeQuery, Source,
    scraper::{ScoredCandidate, Scraper, Verdict},
};
use haiku_store::{json_file, settings::SettingsStore};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub provider_id: String,
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub cover: Option<String>,
    pub duration: Option<f64>,
    pub year: Option<u32>,
    pub track_no: Option<u32>,
    pub score: f64,
    pub source_count: usize,
}
impl From<ScoredCandidate> for Candidate {
    fn from(scored: ScoredCandidate) -> Self {
        let c = scored.candidate;
        Self {
            provider_id: scored.source_id,
            id: c.id,
            title: c.title,
            artists: c.artists,
            album: c.album,
            cover: c.cover,
            duration: c.duration,
            year: c.year,
            track_no: c.track,
            score: scored.score,
            source_count: scored.source_count,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
    pub mode: Mode,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Missing,
    #[default]
    Replace,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub track: Track,
    pub status: String,
    pub candidates: Vec<Candidate>,
    pub error: Option<String>,
    pub selected: Option<Candidate>,
    pub previous_override: Option<Patch>,
    #[serde(default)]
    pub applied_override: Option<Patch>,
    #[serde(default)]
    pub automatic: bool,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub write_back_applied: bool,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub running: bool,
    pub paused: bool,
    pub total: usize,
    pub completed: usize,
    pub items: Vec<Item>,
}
pub struct Service {
    path: PathBuf,
    library: Library,
    host: Arc<Host>,
    settings: Arc<SettingsStore>,
    scraper: Scraper,
    state: Mutex<Snapshot>,
    cancel: Mutex<Cancellation>,
    events: watch::Sender<Snapshot>,
}
impl Service {
    pub fn open(
        root: &Path,
        library: Library,
        host: Arc<Host>,
        settings: Arc<SettingsStore>,
    ) -> Result<Arc<Self>> {
        let path = root.join("scrape_review.json");
        let persisted: Vec<serde_json::Value> = json_file::read(&path)
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let mut items = persisted
            .into_iter()
            .map(read_item)
            .collect::<Result<Vec<_>>>()?;
        for item in &mut items {
            if item.status == "searching" {
                item.status = "pending".into();
            }
            // Legacy reviews contain previousOverride but no conflict-checking extension.
            if item.status == "applied"
                && item.previous_override.is_some()
                && item.applied_override.is_none()
            {
                item.applied_override = Some(library.get_override(&item.track.id));
            }
        }
        let state = Snapshot {
            total: items.len(),
            completed: items
                .iter()
                .filter(|i| !matches!(i.status.as_str(), "pending" | "searching"))
                .count(),
            items,
            ..Default::default()
        };
        let (events, _) = watch::channel(state.clone());
        Ok(Arc::new(Self {
            path,
            library,
            host,
            settings,
            scraper: Scraper::default(),
            state: Mutex::new(state),
            cancel: Mutex::new(Default::default()),
            events,
        }))
    }
    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().unwrap().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.events.subscribe()
    }
    fn save(&self, state: &Snapshot) -> Result<()> {
        let persisted = state.items.iter().map(write_item).collect::<Vec<_>>();
        json_file::write(&self.path, &persisted).map_err(|e| e.to_string())?;
        self.events.send_replace(state.clone());
        Ok(())
    }
    pub fn cancel(&self) {
        self.cancel.lock().unwrap().cancel();
    }
    pub async fn shutdown(&self) {
        self.cancel();
        let mut events = self.subscribe();
        loop {
            if !events.borrow_and_update().running {
                return;
            }
            if events.changed().await.is_err() {
                return;
            }
        }
    }
    pub fn sources(&self) -> Vec<Arc<Source>> {
        let settings = self.settings.get();
        let disabled: Vec<String> = settings
            .extra
            .get("scrapersDisabled")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let order: Vec<String> = settings
            .extra
            .get("scraperOrder")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let music = settings
            .extra
            .get("scrapeUseMusicPlugins")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut sources = self
            .host
            .records()
            .into_iter()
            .filter(|s| s.enabled && !disabled.contains(&s.source_id))
            .filter_map(|s| self.host.source(&s.source_id).ok())
            .filter(|s| {
                let caps = s.descriptor().capabilities;
                caps.iter().any(|c| c == "scrape.search")
                    || music && caps.iter().any(|c| c == "search.track")
            })
            .collect::<Vec<_>>();
        sources.sort_by_key(|s| {
            order
                .iter()
                .position(|id| id == &s.record.source_id)
                .unwrap_or(usize::MAX)
        });
        sources
    }
    /// Begin one batch; completed results remain available after cancellation.
    pub fn start(self: &Arc<Self>, ids: Vec<String>, options: Options) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        if state.running {
            return Err("刮削任务正在运行".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut items = Vec::new();
        for id in ids.into_iter().filter(|id| seen.insert(id.clone())) {
            let track = self
                .library
                .track(&id)
                .ok_or_else(|| format!("未找到本地曲目：{id}"))?;
            items.push(Item {
                track,
                status: "pending".into(),
                candidates: vec![],
                error: None,
                selected: None,
                previous_override: None,
                applied_override: None,
                automatic: false,
                mode: options.mode,
                write_back_applied: false,
            });
        }
        let next = Snapshot {
            running: true,
            total: items.len(),
            items,
            ..Default::default()
        };
        self.save(&next)?;
        *state = next;
        let token = Cancellation::default();
        *self.cancel.lock().unwrap() = token.clone();
        let service = self.clone();
        tokio::spawn(async move {
            service.run(token).await;
        });
        Ok(())
    }
    async fn run(self: Arc<Self>, token: Cancellation) {
        let tracks = self
            .snapshot()
            .items
            .into_iter()
            .map(|i| i.track)
            .collect::<Vec<_>>();
        let sources = self.sources();
        let mut work = stream::iter(tracks.into_iter().enumerate().map(|(index, track)| {
            let (service, sources, cancel) = (self.clone(), sources.clone(), token.clone());
            async move {
                if cancel.is_cancelled() {
                    return (index, None);
                }
                {
                    let mut state = service.state.lock().unwrap();
                    state.items[index].status = "searching".into();
                    service.events.send_replace(state.clone());
                }
                let result = service
                    .scraper
                    .search(query(&track, None), sources, cancel.clone())
                    .await;
                (index, (!cancel.is_cancelled()).then_some(result))
            }
        }))
        .buffer_unordered(4);
        while let Some((index, result)) = work.next().await {
            let Some(result) = result else { continue };
            let mut state = self.state.lock().unwrap();
            if token.is_cancelled() {
                continue;
            }
            let item = &mut state.items[index];
            match result {
                Ok(report) => {
                    item.status = match report.verdict {
                        Verdict::Auto => "auto",
                        Verdict::Review => "review",
                        Verdict::None => "none",
                    }
                    .into();
                    item.candidates = report.candidates.into_iter().map(Into::into).collect();
                    if !report.errors.is_empty() {
                        item.error =
                            Some(serde_json::to_string(&report.errors).unwrap_or_default());
                    }
                    if report.verdict == Verdict::Auto {
                        item.automatic = true;
                        if let Err(e) = self.apply(item, 0) {
                            item.error = Some(e);
                            item.status = "error".into();
                        }
                    }
                }
                Err(error) => {
                    item.status = "error".into();
                    item.error = Some(error.to_string());
                }
            }
            state.completed += 1;
            if let Err(error) = self.save(&state) {
                state.items[index].error = Some(format!("审阅记录保存失败：{error}"));
                self.events.send_replace(state.clone());
                token.cancel();
            }
        }
        let mut state = self.state.lock().unwrap();
        state.running = false;
        for item in &mut state.items {
            if item.status == "searching" {
                item.status = "pending".into();
            }
        }
        if let Err(error) = self.save(&state) {
            log::error!("Scrape review save: {error}");
            self.events.send_replace(state.clone());
        }
    }
    fn apply(&self, item: &mut Item, index: usize) -> Result<()> {
        if item.status == "applied" {
            return Err("此结果已经采用，请先撤销".into());
        }
        let candidate = item.candidates.get(index).ok_or("候选不存在")?.clone();
        let current = self.library.track(&item.track.id).ok_or("本地曲目不存在")?;
        let previous = self.library.get_override(&item.track.id);
        let patch = patch(&current, &candidate, item.mode);
        self.library
            .edit(std::slice::from_ref(&item.track.id), patch)?;
        item.previous_override = Some(previous);
        item.applied_override = Some(self.library.get_override(&item.track.id));
        item.selected = Some(candidate);
        item.status = "applied".into();
        Ok(())
    }
    pub fn accept(&self, id: &str, candidate: usize) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        let item = state
            .items
            .iter_mut()
            .find(|i| i.track.id == id)
            .ok_or("审阅曲目不存在")?;
        if matches!(item.status.as_str(), "pending" | "searching") {
            return Err("此曲目仍在查找".into());
        }
        self.apply(item, candidate)?;
        self.save(&state)
    }
    pub fn skip(&self, id: &str) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        let item = state
            .items
            .iter_mut()
            .find(|i| i.track.id == id)
            .ok_or("审阅曲目不存在")?;
        if matches!(item.status.as_str(), "pending" | "searching" | "applied") {
            return Err("请等待查找结束；已采用的结果请先撤销".into());
        }
        item.status = "skipped".into();
        self.save(&state)
    }
    pub fn undo(&self, id: &str) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        let item = state
            .items
            .iter_mut()
            .find(|i| i.track.id == id)
            .ok_or("审阅曲目不存在")?;
        let previous = item.previous_override.clone().ok_or("没有撤销记录")?;
        let applied = item
            .applied_override
            .as_ref()
            .ok_or("没有已应用的覆盖记录")?;
        self.library
            .restore_override(&item.track.id, applied, previous)?;
        item.status = "review".into();
        item.selected = None;
        item.previous_override = None;
        item.applied_override = None;
        self.save(&state)
    }
    pub async fn retry(&self, id: &str, keyword: String) -> Result<()> {
        let track = {
            let mut state = self.state.lock().unwrap();
            if state.running {
                return Err("请等待当前批次结束".into());
            }
            let item = state
                .items
                .iter_mut()
                .find(|i| i.track.id == id)
                .ok_or("审阅曲目不存在")?;
            if item.status == "applied" {
                return Err("请先撤销当前结果".into());
            }
            let track = item.track.clone();
            item.status = "searching".into();
            state.running = true;
            self.events.send_replace(state.clone());
            track
        };
        let token = Cancellation::default();
        *self.cancel.lock().unwrap() = token.clone();
        let report = self
            .scraper
            .search(query(&track, Some(keyword)), self.sources(), token.clone())
            .await;
        let mut state = self.state.lock().unwrap();
        state.running = false;
        let item = state
            .items
            .iter_mut()
            .find(|i| i.track.id == id)
            .ok_or("审阅曲目已移除")?;
        item.status = "review".into();
        match report {
            Ok(report) if !token.is_cancelled() => {
                item.candidates = report.candidates.into_iter().map(Into::into).collect();
                item.error = None;
            }
            _ if token.is_cancelled() => item.error = Some("查找已取消".into()),
            Err(error) => item.error = Some(error.to_string()),
            _ => {}
        }
        self.save(&state)
    }
}
fn query(track: &Track, keyword: Option<String>) -> ScrapeQuery {
    ScrapeQuery {
        title: keyword.unwrap_or_else(|| track.title.clone()),
        artists: artist_names(track),
        album: track.album.as_ref().map(|a| a.title.clone()),
        duration: track.duration,
    }
}
fn patch(track: &Track, c: &Candidate, mode: Mode) -> Patch {
    let missing = matches!(mode, Mode::Missing);
    let basename = Path::new(&track.id)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let artists = artist_names(track);
    let extra = track.extra.as_ref();
    Patch {
        title: (!c.title.trim().is_empty()
            && (!missing || track.title.is_empty() || track.title == basename))
            .then(|| c.title.clone()),
        artists: (!c.artists.is_empty()
            && (!missing || artists.is_empty() || artists[0] == "未知艺术家"))
            .then(|| c.artists.clone()),
        album: c
            .album
            .clone()
            .filter(|s| !s.trim().is_empty() && (!missing || track.album.is_none())),
        cover: c
            .cover
            .clone()
            .filter(|s| !s.trim().is_empty() && (!missing || track.cover.is_none())),
        year: c
            .year
            .filter(|_| !missing || extra.and_then(|v| v.get("year")).is_none()),
        track_no: c
            .track_no
            .filter(|_| !missing || extra.and_then(|v| v.get("track_no")).is_none()),
        ..Default::default()
    }
}

/// Disk reviews follow the reference service; the bridge continues to expose SDK tracks.
fn write_item(item: &Item) -> serde_json::Value {
    let mut value = serde_json::to_value(item).expect("serializable scrape item");
    let track = value["track"].as_object_mut().unwrap();
    if let Some(source) = track.remove("source") {
        track.insert("provider".into(), source.clone());
        track.insert("sourceId".into(), source);
    }
    if let Some(album) = track.get_mut("album") {
        if album.is_object() {
            *album = album["title"].clone();
        }
    }
    if let Some(artists) = track.get_mut("artists").and_then(|v| v.as_array_mut()) {
        for artist in artists {
            if artist.is_object() {
                *artist = artist["name"].clone();
            }
        }
    }
    if let Some(previous) = value
        .get_mut("previousOverride")
        .and_then(|v| v.as_object_mut())
    {
        let mut extra = serde_json::Map::new();
        for field in ["album_artist", "year", "track_no", "disc_no"] {
            if let Some(value) = previous.remove(field) {
                extra.insert(field.into(), value);
            }
        }
        if !extra.is_empty() {
            previous.insert("extra".into(), extra.into());
        }
    }
    value
}
fn read_item(mut value: serde_json::Value) -> Result<Item> {
    if let Some(track) = value.get_mut("track").and_then(|v| v.as_object_mut()) {
        if !track.contains_key("source") {
            if let Some(source) = track.get("sourceId").cloned() {
                track.insert("source".into(), source);
            }
        }
        if let Some(album) = track.get_mut("album") {
            if album.is_string() {
                *album = serde_json::json!({"title":album.clone()});
            }
        }
    }
    if let Some(previous) = value
        .get_mut("previousOverride")
        .and_then(|v| v.as_object_mut())
    {
        if let Some(serde_json::Value::Object(extra)) = previous.remove("extra") {
            for field in ["album_artist", "year", "track_no", "disc_no"] {
                if let Some(value) = extra.get(field) {
                    previous.insert(field.into(), value.clone());
                }
            }
        }
    }
    serde_json::from_value(value).map_err(|e| format!("刮削审阅记录无法读取：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use haiku_model::{Album, Artist};
    use serde_json::json;
    use std::collections::BTreeMap;

    async fn fixture() -> (tempfile::TempDir, Arc<Service>, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let mut index = BTreeMap::new();
        let ids = ["Auto", "Review", "Skip", "Slow"]
            .map(|title| {
                let id = dir
                    .path()
                    .join(format!("{title}.flac"))
                    .to_string_lossy()
                    .into_owned();
                std::fs::write(&id, b"original audio tags").unwrap();
                let mut track = Track::local(&id);
                track.title = title.into();
                track.artists = vec![Artist::Name("Artist".into())];
                track.duration = Some(120.);
                index.insert(id.clone(), track);
                id
            })
            .to_vec();
        json_file::write(&dir.path().join("library/index.json"), &index).unwrap();
        let host = Host::open(dir.path()).await.unwrap();
        host.install_files(
            BTreeMap::from([(
                "plugin.ts".into(),
                br#"
            import {definePlugin} from "haiku";
            export default definePlugin({
              id:"scrape.fixture",name:"Scraper",version:"1.0.0",
              scrape:{async search(q) {
                if(q.title==="Slow") await new Promise(()=>{});
                return [{id:q.title,title:q.title==="Skip"?"Unrelated":q.title,
                  artists:[q.title==="Review"||q.title==="Skip"?"Other":"Artist"],
                  duration:120,album:"New album",year:2026,track:3}];
              }}
            });
        "#
                .to_vec(),
            )]),
            false,
        )
        .await
        .unwrap();
        host.activate("scrape.fixture").await.unwrap();
        let settings = Arc::new(SettingsStore::open(dir.path()).unwrap());
        let disabled = host
            .records()
            .iter()
            .filter(|r| r.source_id != "scrape.fixture")
            .map(|r| r.source_id.clone())
            .collect::<Vec<_>>();
        settings
            .update(json!({"scrapersDisabled":disabled,"scraperOrder":["scrape.fixture"]}))
            .unwrap();
        let library = Library::open(dir.path()).unwrap();
        let service = Service::open(dir.path(), library, host, settings).unwrap();
        (dir, service, ids)
    }
    async fn done(service: &Service) {
        let mut events = service.subscribe();
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                if !events.borrow_and_update().running {
                    break;
                }
                events.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn scrape_verdicts_review_persistence_undo_and_no_tag_writeback() {
        let (dir, service, ids) = fixture().await;
        service
            .library
            .edit(
                &ids[1..2],
                Patch {
                    cover: Some("old-cover".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        service
            .start(ids[..3].to_vec(), Options::default())
            .unwrap();
        done(&service).await;
        let snapshot = service.snapshot();
        assert_eq!(snapshot.completed, 3);
        assert_eq!(
            snapshot
                .items
                .iter()
                .map(|i| i.status.as_str())
                .collect::<Vec<_>>(),
            vec!["applied", "review", "none"]
        );
        assert!(snapshot.items[0].automatic);
        assert_eq!(
            service.library.track(&ids[0]).unwrap().album.unwrap().title,
            "New album"
        );
        assert!(service.library.track(&ids[1]).unwrap().album.is_none());
        service.accept(&ids[1], 0).unwrap();
        service.skip(&ids[2]).unwrap();
        for id in &ids {
            assert_eq!(std::fs::read(id).unwrap(), b"original audio tags");
        }
        let library = Library::open(dir.path()).unwrap();
        assert_eq!(
            library.track(&ids[1]).unwrap().album.unwrap().title,
            "New album"
        );
        let reopened = Service::open(
            dir.path(),
            library,
            service.host.clone(),
            service.settings.clone(),
        )
        .unwrap();
        assert_eq!(reopened.snapshot().items[1].status, "applied");
        reopened.undo(&ids[1]).unwrap();
        assert_eq!(
            reopened.library.get_override(&ids[1]),
            Patch {
                cover: Some("old-cover".into()),
                ..Default::default()
            }
        );
        reopened.undo(&ids[0]).unwrap();
        assert_eq!(reopened.library.get_override(&ids[0]), Patch::default());
        assert!(reopened.library.track(&ids[0]).unwrap().album.is_none());
    }
    #[tokio::test]
    async fn scrape_cancel_preserves_completed_and_shutdown_drains_calls() {
        let (_dir, service, ids) = fixture().await;
        service
            .start(vec![ids[0].clone(), ids[3].clone()], Options::default())
            .unwrap();
        let mut events = service.subscribe();
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                if events.borrow_and_update().completed == 1 {
                    break;
                }
                events.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        service.shutdown().await;
        let snapshot = service.snapshot();
        assert!(!snapshot.running);
        assert_eq!(snapshot.completed, 1);
        assert_eq!(snapshot.items[0].status, "applied");
        assert_eq!(snapshot.items[1].status, "pending");
        assert!(service.library.track(&ids[0]).unwrap().album.is_some());
        assert!(service.library.track(&ids[3]).unwrap().album.is_none());
    }
    #[tokio::test]
    async fn scrape_missing_replace_disabled_sources_and_later_edit_conflict() {
        let (_dir, service, ids) = fixture().await;
        service
            .library
            .edit(
                &ids[..1],
                Patch {
                    album: Some("Existing".into()),
                    year: Some(2000),
                    ..Default::default()
                },
            )
            .unwrap();
        service
            .start(
                ids[..1].to_vec(),
                Options {
                    mode: Mode::Missing,
                },
            )
            .unwrap();
        done(&service).await;
        let track = service.library.track(&ids[0]).unwrap();
        assert_eq!(track.album.unwrap().title, "Existing");
        assert_eq!(track.extra.unwrap()["year"], 2000);
        service.undo(&ids[0]).unwrap();
        service
            .start(
                ids[..1].to_vec(),
                Options {
                    mode: Mode::Replace,
                },
            )
            .unwrap();
        done(&service).await;
        let track = service.library.track(&ids[0]).unwrap();
        assert_eq!(track.album.unwrap().title, "New album");
        service
            .library
            .edit(
                &ids[..1],
                Patch {
                    year: Some(2030),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(service.undo(&ids[0]).is_err());
        assert_eq!(
            service.library.track(&ids[0]).unwrap().extra.unwrap()["year"],
            2030
        );
        service
            .settings
            .update(json!({"scrapersDisabled":["scrape.fixture"]}))
            .unwrap();
        assert!(service.sources().is_empty());
        let c = Candidate {
            provider_id: "".into(),
            id: "".into(),
            title: "Replacement".into(),
            artists: vec![],
            album: Some("New".into()),
            cover: None,
            duration: None,
            year: None,
            track_no: None,
            score: 1.,
            source_count: 1,
        };
        let mut track = Track::local("/a.flac");
        track.title = "Existing".into();
        track.album = Some(Album {
            id: None,
            title: "Old".into(),
        });
        assert!(patch(&track, &c, Mode::Missing).title.is_none());
        assert_eq!(
            patch(&track, &c, Mode::Replace).title.as_deref(),
            Some("Replacement")
        );
    }
    #[tokio::test]
    async fn scrape_reference_review_format_roundtrips_without_applying_unreviewed_tags() {
        let (dir, service, ids) = fixture().await;
        let reference = json!([{
            "track":{"sourceId":"local","provider":"local","id":ids[1],"title":"Reference",
                "artists":["Artist"],"album":"Reference album","localPath":ids[1]},
            "status":"review","candidates":[],
            "previousOverride":{"title":"Prior title","extra":{"year":1999}}
        }]);
        json_file::write(&dir.path().join("scrape_review.json"), &reference).unwrap();
        let reopened = Service::open(
            dir.path(),
            service.library.clone(),
            service.host.clone(),
            service.settings.clone(),
        )
        .unwrap();
        let item = reopened.snapshot().items.remove(0);
        assert_eq!(item.track.source, "local");
        assert_eq!(item.track.album.unwrap().title, "Reference album");
        assert_eq!(item.previous_override.unwrap().year, Some(1999));
        assert!(reopened.library.track(&ids[1]).unwrap().album.is_none());
        reopened.skip(&ids[1]).unwrap();
        let saved: serde_json::Value = json_file::read(&dir.path().join("scrape_review.json"))
            .unwrap()
            .unwrap();
        assert_eq!(saved[0]["track"]["sourceId"], "local");
        assert_eq!(saved[0]["track"]["album"], "Reference album");
        assert!(saved[0]["track"].get("source").is_none());
        assert_eq!(saved[0]["previousOverride"]["extra"]["year"], 1999);
        assert_eq!(std::fs::read(&ids[1]).unwrap(), b"original audio tags");
    }
}
