//! The engine owns query handles and listeners; blocking file work stays off it.
use crate::{
    core::{CoreBridge, decode, js_error},
    track_source::{self, TrackDataSource},
};
use haiku_library::service::{Library, Query, Snapshot};
use haiku_model::{Error, ErrorCode, Track};
use nsh_core::{
    JsonValue,
    element::{Trace, Tracer},
    prelude::*,
};
use std::{cell::RefCell, collections::BTreeMap, path::PathBuf, rc::Rc};

#[derive(Clone, Default)]
struct Listeners(Rc<RefCell<BTreeMap<u64, Callback<JsonValue>>>>);
// SAFETY: all retained JS callbacks are visited, including shared copies.
unsafe impl Trace for Listeners {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        self.0.as_ref().trace(tracer);
    }
}
fn value<T: serde::Serialize>(v: &T) -> JsonValue {
    serde_json::to_value(v).expect("library JSON")
}
fn error(e: String) -> JsError {
    js_error(Error::new(ErrorCode::Io, e))
}
fn notify(listeners: &Listeners, snapshot: &Snapshot) {
    let callbacks: Vec<_> = listeners.0.borrow().values().cloned().collect();
    for callback in callbacks {
        callback.post(value(snapshot));
    }
}
struct LocalSource {
    library: Library,
    query: RefCell<Query>,
}
impl TrackDataSource for LocalSource {
    fn ids(&self) -> Vec<String> {
        self.library.query(&self.query.borrow())
    }
    fn track(&self, id: &str) -> Option<Track> {
        self.library.track(id)
    }
}
struct Bridge {
    owner: Owner,
    library: Library,
    player: Rc<CoreBridge>,
    queries: RefCell<BTreeMap<u32, Rc<LocalSource>>>,
    changed: Listeners,
    progress: Listeners,
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.library.cancel();
        // Retained pages release individual handles. The remaining registry is
        // engine-owned and drops with the engine; finalizers must not access it.
        self.owner.dispose();
    }
}

/// Local library commands. Query handles keep all tracks and selection in Rust.
#[class]
pub struct LibraryService {
    bridge: Rc<Bridge>,
    // Keep traced roots directly on the class: Rc<Bridge> is intentionally opaque to Trace.
    changed: Listeners,
    progress: Listeners,
    next_listener: u64,
}
impl LibraryService {
    pub(crate) fn new(library: Library, player: Rc<CoreBridge>) -> Instance<Self> {
        let bridge = Rc::new(Bridge {
            owner: Owner::detached(),
            library,
            player,
            queries: RefCell::new(BTreeMap::new()),
            changed: Listeners::default(),
            progress: Listeners::default(),
        });
        let weak = Rc::downgrade(&bridge);
        let mut events = bridge.library.subscribe();
        bridge.owner.run(|| {
            spawn(async move {
                let mut revision = events.borrow().revision;
                while events.changed().await.is_ok() {
                    let snapshot = events.borrow_and_update().clone();
                    let Some(bridge) = weak.upgrade() else {
                        break;
                    };
                    if revision != snapshot.revision {
                        revision = snapshot.revision;
                        for handle in bridge.queries.borrow().keys() {
                            if let Some(source) = track_source::get(*handle) {
                                source.refresh();
                            }
                        }
                        notify(&bridge.changed, &snapshot);
                    }
                    notify(&bridge.progress, &snapshot);
                }
            })
        });
        Instance::new(Self {
            changed: bridge.changed.clone(),
            progress: bridge.progress.clone(),
            bridge,
            next_listener: 0,
        })
    }
    fn listen(&mut self, progress: bool, callback: Callback<JsonValue>) -> Stop {
        self.next_listener += 1;
        let id = self.next_listener;
        let listeners = if progress {
            &self.progress
        } else {
            &self.changed
        }
        .clone();
        callback.post(value(&self.bridge.library.snapshot()));
        listeners.0.borrow_mut().insert(id, callback);
        Stop::new(move || {
            listeners.0.borrow_mut().remove(&id);
        })
    }
}
#[class]
impl LibraryService {
    /// Ask the native directory picker for scan roots; cancellation returns an empty list.
    pub async fn pick_folders() -> Result<Vec<String>, JsError> {
        let paths = pick_paths(nsh_core::file_dialog::OpenDialog::folders().multiple(true))
            .await
            .map_err(|e| error(e.to_string()))?
            .unwrap_or_default();
        Ok(paths
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect())
    }
    /// Ask the native file picker for a metadata cover; validation happens during import.
    pub async fn pick_cover() -> Result<Option<String>, JsError> {
        let paths = pick_paths(nsh_core::file_dialog::OpenDialog::files().filter(
            nsh_core::file_dialog::FileFilter::new("封面图片", ["png", "jpg", "jpeg", "webp"]),
        ))
        .await
        .map_err(|e| error(e.to_string()))?
        .unwrap_or_default();
        Ok(paths.first().map(|p| p.to_string_lossy().into_owned()))
    }
    /// Reveal a local track in the platform file manager.
    pub async fn reveal(this: Instance<Self>, id: String) -> Result<(), JsError> {
        if this.with(|s| s.bridge.library.track(&id).is_none()) {
            return Err(error("未找到本地曲目".into()));
        }
        reveal_path(PathBuf::from(id))
            .await
            .map_err(|e| error(e.to_string()))
    }
    /// Current scan state, progress and per-file report, without the library's tracks.
    pub fn snapshot(&self) -> JsonValue {
        value(&self.bridge.library.snapshot())
    }
    /// Absolute, canonical scan directories, persisted in library/roots.json.
    pub fn roots(&self) -> Vec<String> {
        self.bridge.library.roots()
    }
    /// Add directories atomically. Duplicate and overlapping roots are safe.
    pub async fn add_roots(this: Instance<Self>, roots: Vec<String>) -> Result<(), JsError> {
        let library = this.with(|s| s.bridge.library.clone());
        blocking(move || library.add_roots(roots))
            .await
            .map_err(|e| error(e.to_string()))?
            .map_err(error)
    }
    /// Remove a root; the next scan rebuilds the index, preserving overrides.
    pub async fn remove_root(this: Instance<Self>, root: String) -> Result<(), JsError> {
        let library = this.with(|s| s.bridge.library.clone());
        blocking(move || library.remove_root(&root))
            .await
            .map_err(|e| error(e.to_string()))?
            .map_err(error)
    }
    /// Rebuild the index. Progress is delivered while this Promise is pending.
    pub async fn scan(this: Instance<Self>) -> Result<JsonValue, JsError> {
        let library = this.with(|s| s.bridge.library.clone());
        let report = blocking(move || library.scan())
            .await
            .map_err(|e| error(e.to_string()))?
            .map_err(error)?;
        Ok(value(&report))
    }
    /// Cooperatively cancel traversal/tag reads, retaining the last complete index.
    pub fn cancel(&self) {
        self.bridge.library.cancel();
    }
    /// Subscribe to the initial state and index/root/metadata changes; call Stop to detach.
    pub fn on_changed(&mut self, #[args(snapshot)] callback: Callback<JsonValue>) -> Stop {
        self.listen(false, callback)
    }
    /// Subscribe to the initial state and scan progress; call Stop to detach.
    pub fn on_progress(&mut self, #[args(snapshot)] callback: Callback<JsonValue>) -> Stop {
        self.listen(true, callback)
    }
    /// Create a TrackList source from {search, sort, descending, group}; only its handle crosses JS.
    pub async fn query(this: Instance<Self>, query: JsonValue) -> Result<u32, JsError> {
        let query: Query = decode(query)?;
        let bridge = this.with(|s| s.bridge.clone());
        let provider = Rc::new(LocalSource {
            library: bridge.library.clone(),
            query: RefCell::new(query),
        });
        let handle = bridge
            .owner
            .run(|| track_source::register(provider.clone()));
        bridge.queries.borrow_mut().insert(handle, provider);
        Ok(handle)
    }
    /// Update the query in place; selection remains keyed by track identity.
    pub async fn set_query(
        this: Instance<Self>,
        handle: u32,
        query: JsonValue,
    ) -> Result<(), JsError> {
        let query = decode(query)?;
        let bridge = this.with(|s| s.bridge.clone());
        let queries = bridge.queries.borrow();
        let provider = queries
            .get(&handle)
            .ok_or_else(|| error("查询已释放".into()))?;
        *provider.query.borrow_mut() = query;
        if let Some(source) = track_source::get(handle) {
            source.refresh();
        }
        Ok(())
    }
    /// Release a page's source when the retained page is disposed.
    pub async fn release_query(this: Instance<Self>, handle: u32) {
        this.with(|s| {
            s.bridge.queries.borrow_mut().remove(&handle);
        });
        track_source::remove(handle);
    }
    /// IDs in complete filter/sort order, for other Rust-backed consumers and diagnostics.
    pub async fn query_ids(handle: u32) -> Vec<String> {
        track_source::get(handle)
            .map(|s| s.ids.get_untracked())
            .unwrap_or_default()
    }
    /// Return at most 100 album/artist summaries per page, never their track arrays.
    pub async fn groups(
        this: Instance<Self>,
        kind: String,
        query: JsonValue,
        offset: u32,
        limit: u32,
    ) -> Result<JsonValue, JsError> {
        if kind != "albums" && kind != "artists" {
            return Err(error("未知分组视图".into()));
        }
        let query = decode(query)?;
        let library = this.with(|s| s.bridge.library.clone());
        let (total, items) =
            blocking(move || library.groups(&kind, &query, offset as usize, limit as usize))
                .await
                .map_err(|e| error(e.to_string()))?;
        Ok(serde_json::json!({"total":total,"items":items}))
    }
    /// Read one effective track for an editor. List rows read this directly in Rust.
    pub fn track(&self, id: String) -> JsonValue {
        value(&self.bridge.library.track(&id))
    }
    /// Describe one opaque local group for its detail header, without serializing its tracks.
    pub async fn describe_group(this: Instance<Self>, key: String) -> Result<JsonValue, JsError> {
        let entity = haiku_model::EntityRef::decode(&key).map_err(js_error)?;
        let library = this.with(|s| s.bridge.library.clone());
        blocking(move || {
            let ids = library.query(&Query {
                group: Some(key),
                ..Default::default()
            });
            let first = ids.first().and_then(|id| library.track(id));
            let title = first
                .as_ref()
                .map(|t| {
                    if entity.kind == "artist" {
                        haiku_library::service::artist_names(t)
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "未知艺术家".into())
                    } else {
                        t.album
                            .as_ref()
                            .map(|a| a.title.clone())
                            .unwrap_or_else(|| "未知专辑".into())
                    }
                })
                .unwrap_or_else(|| "未找到分组".into());
            serde_json::json!({"title":title,"count":ids.len()})
        })
        .await
        .map_err(|e| error(e.to_string()))
    }
    /// Store only the supplied changed fields, preserving all other per-track values.
    pub async fn edit(
        this: Instance<Self>,
        ids: Vec<String>,
        patch: JsonValue,
    ) -> Result<(), JsError> {
        let patch = decode(patch)?;
        let library = this.with(|s| s.bridge.library.clone());
        blocking(move || library.edit(&ids, patch))
            .await
            .map_err(|e| error(e.to_string()))?
            .map_err(error)
    }
    /// Remove application overrides after the page has confirmed the action.
    pub async fn reset(this: Instance<Self>, ids: Vec<String>) -> Result<(), JsError> {
        let library = this.with(|s| s.bridge.library.clone());
        blocking(move || library.reset(&ids))
            .await
            .map_err(|e| error(e.to_string()))?
            .map_err(error)
    }
    /// Write effective tags after confirmation; returns a separate error for each failed file.
    pub async fn write_tags(
        this: Instance<Self>,
        ids: Vec<String>,
    ) -> Result<Vec<String>, JsError> {
        let library = this.with(|s| s.bridge.library.clone());
        blocking(move || library.write_tags(&ids))
            .await
            .map_err(|e| error(e.to_string()))
    }
    /// Copy a validated PNG/JPEG/WebP into the metadata cover cache.
    pub async fn import_cover(this: Instance<Self>, path: String) -> Result<String, JsError> {
        let library = this.with(|s| s.bridge.library.clone());
        blocking(move || library.import_cover(&PathBuf::from(path)))
            .await
            .map_err(|e| error(e.to_string()))?
            .map_err(error)
    }
    /// Play a source's complete current result starting at the given ID.
    pub async fn play_source(this: Instance<Self>, handle: u32, id: String) -> Result<(), JsError> {
        let source = track_source::get(handle).ok_or_else(|| error("查询已释放".into()))?;
        let player = this.with(|s| s.bridge.player.controller.clone());
        let ids = source.ids.get_untracked();
        let index = ids
            .iter()
            .position(|v| v == &id)
            .ok_or_else(|| error("曲目已不在筛选结果中".into()))?;
        let tracks = ids
            .iter()
            .filter_map(|id| source.provider.track(id))
            .collect();
        player
            .execute(haiku_playback::Command::Play(tracks, index as i32))
            .await
            .map_err(js_error)
    }
}
