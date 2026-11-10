//! Low-frequency mirrors; native history tracks remain in engine-owned sources.
use crate::{
    core::{CoreBridge, decode},
    track_source::{self, TrackDataSource},
};
use haiku_model::Track;
use haiku_store::{
    history::{History, PlaylistHistory},
    logs::Filter,
};
use nsh_core::{
    JsonValue,
    element::{Trace, Tracer},
    prelude::*,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, time::Duration};
#[derive(Clone, Default)]
struct Listeners(Rc<RefCell<BTreeMap<u64, Callback<JsonValue>>>>);
unsafe impl Trace for Listeners {
    fn trace(&self, t: &mut Tracer<'_>) {
        self.0.as_ref().trace(t);
    }
}
#[derive(Default, serde::Deserialize)]
#[serde(default)]
struct Query {
    search: String,
    source: String,
}
struct HistorySource {
    history: History,
    query: RefCell<Query>,
}
impl TrackDataSource for HistorySource {
    fn ids(&self) -> Vec<String> {
        let q = self.query.borrow();
        self.history
            .tracks(&q.search, &q.source)
            .iter()
            .map(|t| t.track.key())
            .collect()
    }
    fn track(&self, key: &str) -> Option<Track> {
        self.history
            .snapshot()
            .tracks
            .into_iter()
            .find(|t| t.track.key() == key)
            .map(|t| t.track.track())
    }
    fn group(&self, key: &str) -> Option<String> {
        let q = self.query.borrow();
        let tracks = self.history.tracks(&q.search, &q.source);
        let at = tracks.iter().position(|t| t.track.key() == key)?;
        let now = haiku_store::stats::wall_time().0;
        let label = haiku_store::history::group(tracks[at].played_at, now);
        (at == 0 || haiku_store::history::group(tracks[at - 1].played_at, now) != label)
            .then(|| label.into())
    }
}
struct Bridge {
    owner: Owner,
    core: Rc<CoreBridge>,
    listeners: Listeners,
    queries: RefCell<BTreeMap<u32, Rc<HistorySource>>>,
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.owner.dispose();
    }
}
/// Local history/statistics/diagnostics facade. State and filtering belong to Rust.
#[class]
pub struct ActivityService {
    bridge: Rc<Bridge>,
    listeners: Listeners,
    next: u64,
}
impl ActivityService {
    pub(crate) fn new(core: Rc<CoreBridge>) -> Instance<Self> {
        let bridge = Rc::new(Bridge {
            owner: Owner::detached(),
            core,
            listeners: Listeners::default(),
            queries: RefCell::default(),
        });
        let weak = Rc::downgrade(&bridge);
        let last = std::cell::Cell::new((0, 0, 0));
        bridge.owner.run(|| {
            set_interval(Duration::from_millis(100), move || {
                let Some(b) = weak.upgrade() else { return };
                let next = (
                    b.core.history.revision(),
                    b.core.stats.revision(),
                    b.core.logs.revision(),
                );
                if next == last.get() {
                    return;
                }
                if next.0 != last.get().0 {
                    for id in b.queries.borrow().keys() {
                        if let Some(s) = track_source::get(*id) {
                            s.refresh();
                        }
                    }
                }
                last.set(next);
                let callbacks = b.listeners.0.borrow().values().cloned().collect::<Vec<_>>();
                for cb in callbacks {
                    cb.post(serde_json::json!({"history":next.0,"stats":next.1,"logs":next.2}));
                }
            })
        });
        Instance::new(Self {
            listeners: bridge.listeners.clone(),
            bridge,
            next: 0,
        })
    }
}
#[class]
impl ActivityService {
    /// Subscribe to initial and subsequent history/stats/log/notification revisions. Stop detaches.
    pub fn on_change(&mut self, #[args(change)] callback: Callback<JsonValue>) -> Stop {
        self.next += 1;
        let id = self.next;
        callback.post(serde_json::json!({}));
        self.listeners.0.borrow_mut().insert(id, callback);
        let listeners = self.listeners.clone();
        Stop::new(move || {
            listeners.0.borrow_mut().remove(&id);
        })
    }
    /// Reference-compatible playback, playlist and search histories.
    pub fn list(&self) -> JsonValue {
        serde_json::to_value(self.bridge.core.history.snapshot()).unwrap()
    }
    /// Filter source identities without copying tracks into TSX.
    pub fn sources(&self) -> Vec<String> {
        let state = self.bridge.core.history.snapshot();
        state
            .tracks
            .iter()
            .map(|t| t.track.source_id.clone())
            .chain(state.playlists.iter().map(|p| p.source_id.clone()))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    /// Record a local or remote playlist visit; preserve cover headers and unknown counts.
    pub fn record_playlist(&self, entry: JsonValue) -> Result<(), JsError> {
        let mut entry: PlaylistHistory = decode(entry)?;
        if entry.played_at == 0 {
            entry.played_at = haiku_store::stats::wall_time().0;
        }
        self.bridge.core.history.record_playlist(entry);
        Ok(())
    }
    /// Add a nonempty query to the deduplicated 30-item search history.
    pub fn search(&self, query: String) {
        self.bridge.core.history.search(&query);
    }
    /// Remove a track EntityRef, playlist [kind,sourceId,id], or literal search query.
    pub fn remove(&self, kind: String, key: String) {
        self.bridge.core.history.remove(&kind, &key);
    }
    /// Clear one history kind. Destructive UI callers ask for confirmation first.
    pub fn clear(&self, kind: String) {
        self.bridge.core.history.clear(&kind);
    }
    /// Retain a native virtual history source with date group headings.
    pub async fn query(this: Instance<Self>, query: JsonValue) -> Result<u32, JsError> {
        let b = this.with(|s| s.bridge.clone());
        let provider = Rc::new(HistorySource {
            history: b.core.history.clone(),
            query: RefCell::new(decode(query)?),
        });
        let id = b.owner.run(|| track_source::register(provider.clone()));
        b.queries.borrow_mut().insert(id, provider);
        Ok(id)
    }
    /// Update a retained source's search and source filters.
    pub async fn set_query(this: Instance<Self>, id: u32, query: JsonValue) -> Result<(), JsError> {
        let q = decode(query)?;
        this.with(|s| {
            if let Some(p) = s.bridge.queries.borrow().get(&id) {
                *p.query.borrow_mut() = q;
            }
        });
        if let Some(s) = track_source::get(id) {
            s.refresh();
        }
        Ok(())
    }
    /// Release a retained page's history source.
    pub async fn release_query(this: Instance<Self>, id: u32) {
        this.with(|s| {
            s.bridge.queries.borrow_mut().remove(&id);
        });
        track_source::remove(id);
    }
    /// Play the complete filtered history beginning at an opaque EntityRef.
    pub async fn play_source(this: Instance<Self>, id: u32, key: String) -> Result<(), JsError> {
        let s = track_source::get(id).ok_or_else(|| JsError::Error("查询已释放".into()))?;
        let ids = s.ids.get_untracked();
        let at = ids.iter().position(|id| id == &key).unwrap_or(0);
        let tracks = ids.iter().filter_map(|id| s.provider.track(id)).collect();
        let c = this.with(|s| s.bridge.core.controller.clone());
        c.execute(haiku_playback::Command::Play(tracks, at as i32))
            .await
            .map_err(crate::core::js_error)
    }
    /// Local aggregates for day/month/year/all. Computation runs off the engine thread.
    pub async fn aggregate(this: Instance<Self>, range: String) -> Result<JsonValue, String> {
        let stats = this.with(|s| s.bridge.core.stats.clone());
        blocking(move || stats.aggregate(&range, haiku_store::stats::wall_time().0))
            .await
            .map_err(|e| e.to_string())
    }
    /// Clear local listening events, retaining device identity and leaving remote data alone.
    pub async fn clear_stats(this: Instance<Self>) {
        this.with(|s| s.bridge.core.stats.clear());
    }
    /// Filter then page diagnostics; at most 100 entries per request.
    pub async fn page(
        this: Instance<Self>,
        filter: JsonValue,
        offset: u32,
        limit: u32,
    ) -> Result<JsonValue, JsError> {
        let f: Filter = decode(filter)?;
        let logs = this.with(|s| s.bridge.core.logs.clone());
        blocking(move || { let entries = logs.filter(&f); serde_json::json!({"total":entries.len(),"items":entries.into_iter().rev().skip(offset as usize).take(limit.min(100) as usize).collect::<Vec<_>>()}) }).await.map_err(|e| JsError::Error(e.to_string()))
    }
    /// Redacted text of the current filter, for clipboard use.
    pub async fn copy_text(this: Instance<Self>, filter: JsonValue) -> Result<String, JsError> {
        let f: Filter = decode(filter)?;
        let logs = this.with(|s| s.bridge.core.logs.clone());
        blocking(move || logs.text(&f))
            .await
            .map_err(|e| JsError::Error(e.to_string()))
    }
    /// Export both complete channels with the native save picker.
    pub async fn export_logs(this: Instance<Self>) -> Result<(), String> {
        let path =
            pick_save_path(nsh_core::file_dialog::SaveDialog::new().file_name("haiku-logs.txt"))
                .await
                .map_err(|e| e.to_string())?;
        if let Some(path) = path {
            let logs = this.with(|s| s.bridge.core.logs.clone());
            blocking(move || {
                std::fs::write(path, logs.text(&Filter::default())).map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| e.to_string())??;
        }
        Ok(())
    }
    /// Clear one log channel or both (empty string), after UI confirmation.
    pub async fn clear_logs(this: Instance<Self>, channel: String) -> Result<(), String> {
        let logs = this.with(|s| s.bridge.core.logs.clone());
        blocking(move || logs.clear(&channel))
            .await
            .map_err(|e| e.to_string())?
    }
    /// Current bounded notification list; details have the same redaction as logs.
    pub fn notices(&self) -> JsonValue {
        serde_json::to_value(self.bridge.core.logs.notices()).unwrap()
    }
    /// Dismiss one notification, or all when id is null.
    pub fn clear_notice(&self, id: Option<u64>) {
        self.bridge.core.logs.clear_notice(id);
    }
    /// Populate isolated preview/test data through the same stores; never used by normal startup.
    pub async fn seed_fixture(this: Instance<Self>, tracks: Vec<JsonValue>) -> Result<(), JsError> {
        let tracks: Vec<Track> = decode(JsonValue::Array(tracks))?;
        let (h, s, l) = this.with(|s| {
            (
                s.bridge.core.history.clone(),
                s.bridge.core.stats.clone(),
                s.bridge.core.logs.clone(),
            )
        });
        blocking(move || {
            let (now, offset) = haiku_store::stats::wall_time();
            for (i, track) in tracks.iter().enumerate() { h.record_track(track, now - i as i64 * 86400); }
            for day in 0..28 {
                for at in 0..2 {
                    let Some(track) = tracks.get((day + at) % tracks.len().max(1)) else { continue };
                    let midnight = (now + offset as i64 * 60).div_euclid(86400) * 86400 - offset as i64 * 60;
                    let epoch = midnight - day as i64 * 86400 + ((day * 5 + at * 9) % 24) as i64 * 3600;
                    s.sample(Some(track), day as u64 * 2 + at as u64, 0, true, 0., 0, epoch, offset);
                    for second in 1..=(15 + day as u64 % 5 * 10) { s.sample(Some(track), day as u64 * 2 + at as u64, 0, true, second as f64, second * 1000, epoch, offset); }
                    s.finish();
                }
            }
            l.add("app", "info", "library", Some("scan"), "本地扫描完成", Some("找到 4 首歌曲"))?;
            l.add("plugin", "warn", "fixture.music", None, "来源暂时离线", Some("https://user:password@fixture.invalid/music?token=secret#fragment Authorization: Bearer secret"))?;
            l.notify("success", "本地扫描完成", Some("音乐已添加到曲库，可以开始播放。"));
            l.notify("warning", "来源暂时离线", Some("离线曲目仍可播放，稍后可以重试连接。"));
            Ok::<_, String>(())
        }).await.map_err(|e| JsError::Error(e.to_string()))?.map_err(JsError::Error)
    }
}
