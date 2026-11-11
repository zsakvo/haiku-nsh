//! Engine adapter: TSX mirrors events; native controls consume the same Rust revision.
use crate::{
    core::{CoreBridge, decode},
    track_source::{self, TrackDataSource},
};
use haiku_model::{EntityRef, Track};
use haiku_store::{
    collections::{Change, Collections},
    playlist_core::LIKED_PLAYLIST_ID,
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
#[serde(rename_all = "camelCase")]
struct Query {
    id: String,
    #[serde(default)]
    search: String,
    #[serde(default)]
    source: String,
}
struct CollectionSource {
    store: Collections,
    library: haiku_library::service::Library,
    query: RefCell<Query>,
}
impl CollectionSource {
    fn tracks(&self) -> Vec<Track> {
        let query = self.query.borrow();
        let needle = query.search.to_lowercase();
        self.store
            .tracks(&query.id)
            .into_iter()
            .map(|t| {
                if t.source == "local" {
                    self.library.track(&t.id).unwrap_or(t)
                } else {
                    t
                }
            })
            .filter(|t| {
                (query.source.is_empty() || t.source == query.source)
                    && (needle.is_empty()
                        || format!("{} {:?} {:?}", t.title, t.artists, t.album)
                            .to_lowercase()
                            .contains(&needle))
            })
            .collect()
    }
}
impl TrackDataSource for CollectionSource {
    fn ids(&self) -> Vec<String> {
        self.tracks()
            .iter()
            .map(|t| t.entity_ref().encode())
            .collect()
    }
    fn track(&self, id: &str) -> Option<Track> {
        self.tracks()
            .into_iter()
            .find(|t| t.entity_ref().encode() == id)
    }
    fn collection_id(&self) -> Option<String> {
        Some(self.query.borrow().id.clone())
    }
}
struct Bridge {
    owner: Owner,
    core: Rc<CoreBridge>,
    library: haiku_library::service::Library,
    queries: RefCell<BTreeMap<u32, Rc<CollectionSource>>>,
    listeners: Listeners,
    liked: Listeners,
    artwork: RefCell<BTreeMap<String, Vec<Option<String>>>>,
    artwork_request: std::cell::Cell<u64>,
}
impl Bridge {
    fn notify(&self, ids: &[String], reason: &str) {
        let value = serde_json::json!({"revision":self.core.collections.revision(),"playlistIds":ids,"reason":reason});
        let callbacks: Vec<_> = self.listeners.0.borrow().values().cloned().collect();
        for callback in callbacks {
            callback.post(value.clone());
        }
    }
    fn refresh_queries(&self) {
        for handle in self.queries.borrow().keys() {
            if let Some(source) = track_source::get(*handle) {
                source.refresh();
            }
        }
    }
    fn refresh_artwork(self: &Rc<Self>) {
        let request = self.artwork_request.get() + 1;
        self.artwork_request.set(request);
        let store = self.core.collections.clone();
        let library = self.library.clone();
        let weak = Rc::downgrade(self);
        self.owner.run(|| {
            spawn(async move {
                let loaded = blocking(move || load_artwork(&store, &library)).await;
                let Some(bridge) = weak.upgrade() else { return };
                if bridge.artwork_request.get() != request {
                    return;
                }
                if let Ok(loaded) = loaded {
                    let mut old = bridge.artwork.borrow_mut();
                    let ids = loaded
                        .iter()
                        .filter(|(id, images)| old.get(*id) != Some(*images))
                        .map(|(id, _)| id.clone())
                        .collect::<Vec<_>>();
                    *old = loaded;
                    drop(old);
                    if !ids.is_empty() {
                        bridge.notify(&ids, "artwork");
                    }
                }
            })
        });
    }
}
/// File reads and image encoding run only on the blocking pool, never a synchronous JS export.
fn load_artwork(
    store: &Collections,
    library: &haiku_library::service::Library,
) -> BTreeMap<String, Vec<Option<String>>> {
    use base64::Engine;
    use std::io::Read;
    store
        .list()
        .into_iter()
        .map(|p| {
            let images = store
                .tracks(&p.id)
                .into_iter()
                .take(4)
                .map(|t| {
                    let t = if t.source == "local" {
                        library.track(&t.id).unwrap_or(t)
                    } else {
                        t
                    };
                    let cover = t.cover?;
                    if cover.starts_with("http")
                        || cover.starts_with("data:")
                        || cover.starts_with("asset:")
                    {
                        return Some(cover);
                    }
                    let path = cover.strip_prefix("file://").unwrap_or(&cover);
                    let file = std::fs::File::open(path).ok()?;
                    let mut bytes = vec![];
                    file.take(8 * 1024 * 1024 + 1)
                        .read_to_end(&mut bytes)
                        .ok()?;
                    if bytes.len() > 8 * 1024 * 1024 {
                        return None;
                    }
                    let mime = if bytes.starts_with(b"\x89PNG") {
                        "image/png"
                    } else if bytes.starts_with(b"RIFF") {
                        "image/webp"
                    } else {
                        "image/jpeg"
                    };
                    Some(format!(
                        "data:{mime};base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(bytes)
                    ))
                })
                .collect();
            (p.id, images)
        })
        .collect()
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.owner.dispose();
    }
}
/// Likes, editable user playlists and read-only remote mirrors. Rust owns persistence and identity.
#[class]
pub struct CollectionsService {
    bridge: Rc<Bridge>,
    listeners: Listeners,
    liked: Listeners,
    next: u64,
}
impl CollectionsService {
    pub(crate) fn new(
        core: Rc<CoreBridge>,
        library: haiku_library::service::Library,
    ) -> Instance<Self> {
        let bridge = Rc::new(Bridge {
            owner: Owner::detached(),
            core,
            library,
            queries: RefCell::new(BTreeMap::new()),
            listeners: Listeners::default(),
            liked: Listeners::default(),
            artwork: RefCell::new(BTreeMap::new()),
            artwork_request: std::cell::Cell::new(0),
        });
        let events = bridge.core.collections.subscribe();
        let weak = Rc::downgrade(&bridge);
        bridge.owner.run(|| {
            set_interval(Duration::from_millis(50), move || {
                let Some(bridge) = weak.upgrade() else { return };
                let events: Vec<_> = events.try_iter().collect();
                if events.is_empty() {
                    return;
                }
                bridge
                    .core
                    .collections_revision
                    .set(bridge.core.collections.revision());
                bridge.refresh_queries();
                bridge.refresh_artwork();
                for event in events {
                    bridge.notify(&event.playlist_ids, "mutation");
                    if event.playlist_ids.iter().any(|id| id == LIKED_PLAYLIST_ID) {
                        let keys = liked_keys(&bridge.core.collections);
                        for callback in bridge.liked.0.borrow().values() {
                            callback.post(keys.clone());
                        }
                    }
                }
            })
        });
        bridge.refresh_artwork();
        let mut library_events = bridge.library.subscribe();
        let weak = Rc::downgrade(&bridge);
        bridge.owner.run(|| {
            spawn(async move {
                let mut revision = library_events.borrow().revision;
                while library_events.changed().await.is_ok() {
                    let next = library_events.borrow_and_update().revision;
                    if next == revision {
                        continue;
                    }
                    revision = next;
                    let Some(bridge) = weak.upgrade() else { return };
                    bridge.refresh_queries();
                    bridge.refresh_artwork();
                    let ids = bridge
                        .core
                        .collections
                        .list()
                        .into_iter()
                        .map(|p| p.id)
                        .collect::<Vec<_>>();
                    bridge.notify(&ids, "metadata");
                }
            })
        });
        Instance::new(Self {
            listeners: bridge.listeners.clone(),
            liked: bridge.liked.clone(),
            bridge,
            next: 0,
        })
    }
    fn listen(&mut self, liked: bool, callback: Callback<JsonValue>) -> Stop {
        self.next += 1;
        let id = self.next;
        callback.post(if liked {
            self.liked_keys()
        } else {
            serde_json::to_value(Change {
                revision: self.bridge.core.collections.revision(),
                playlist_ids: vec![],
            })
            .unwrap()
        });
        let listeners = if liked {
            self.liked.clone()
        } else {
            self.listeners.clone()
        };
        listeners.0.borrow_mut().insert(id, callback);
        Stop::new(move || {
            listeners.0.borrow_mut().remove(&id);
        })
    }
}
fn liked_keys(store: &Collections) -> JsonValue {
    serde_json::to_value(
        store
            .tracks(LIKED_PLAYLIST_ID)
            .iter()
            .map(|t| t.entity_ref().encode())
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
#[class]
impl CollectionsService {
    /// Opaque route identity for a local collection, safe for punctuation in IDs.
    pub fn key(&self, id: String) -> String {
        EntityRef {
            source: "collections".into(),
            kind: "playlist".into(),
            id,
        }
        .encode()
    }
    /// Decode a local collection route, rejecting another entity kind/source.
    pub fn id_for_key(&self, key: String) -> Result<String, JsError> {
        let key = EntityRef::decode(&key).map_err(crate::core::js_error)?;
        if key.source != "collections" || key.kind != "playlist" {
            return Err(JsError::Error("无效歌单路由".into()));
        }
        Ok(key.id)
    }
    /// Distinct stored source identities for a collection's filter menu.
    pub fn sources(&self, id: String) -> Vec<String> {
        self.bridge
            .core
            .collections
            .tracks(&id)
            .into_iter()
            .map(|t| t.source)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    /// Cached first four covers. Background reads publish onChange(reason: artwork) when ready.
    pub fn artwork(&self, id: String) -> Vec<Option<String>> {
        self.bridge
            .artwork
            .borrow()
            .get(&id)
            .cloned()
            .unwrap_or_default()
    }
    /// Summaries in display order, including the fixed __liked__ collection.
    pub fn list(&self) -> JsonValue {
        serde_json::to_value(self.bridge.core.collections.list()).unwrap()
    }
    /// SDK tracks in playlist order (likes newest first); native lists use query handles instead.
    pub fn tracks(&self, id: String) -> JsonValue {
        let tracks: Vec<_> = self
            .bridge
            .core
            .collections
            .tracks(&id)
            .into_iter()
            .map(|track| {
                if track.source == "local" {
                    self.bridge.library.track(&track.id).unwrap_or(track)
                } else {
                    track
                }
            })
            .collect();
        serde_json::to_value(tracks).unwrap()
    }
    /// Create an editable playlist; an empty title is rejected.
    pub fn create(&self, title: String) -> Result<String, String> {
        self.bridge.core.collections.create(&title)
    }
    /// Rename and replace the description of an editable user playlist.
    pub fn rename(&self, id: String, title: String, description: String) -> Result<(), String> {
        self.bridge
            .core
            .collections
            .rename(&id, &title, &description)
    }
    /// Delete a user playlist or remote favorite; the fixed likes collection cannot be deleted.
    pub fn delete(&self, id: String) -> Result<(), String> {
        self.bridge.core.collections.delete(&id)
    }
    /// Add SDK tracks to a user playlist, deduplicating by EntityRef.
    pub fn add(&self, id: String, tracks: Vec<JsonValue>) -> Result<(), JsError> {
        let tracks: Vec<Track> = decode(JsonValue::Array(tracks))?;
        self.bridge
            .core
            .collections
            .add(&id, &tracks)
            .map_err(JsError::Error)
    }
    /// Remove opaque track EntityRefs from a user playlist or likes.
    pub fn remove(&self, id: String, keys: Vec<String>) -> Result<(), JsError> {
        let keys = keys
            .iter()
            .map(|k| EntityRef::decode(k))
            .collect::<Result<Vec<_>, _>>()
            .map_err(crate::core::js_error)?;
        self.bridge
            .core
            .collections
            .remove(&id, &keys)
            .map_err(JsError::Error)
    }
    /// Move a track between visible indices in an editable playlist.
    pub fn move_track(&self, id: String, from: u32, to: u32) -> Result<(), String> {
        self.bridge.core.collections.move_track(&id, from, to)
    }
    /// Reorder playlists, keeping likes first and appending omitted live collections.
    pub fn reorder(&self, ids: Vec<String>) -> Result<(), String> {
        self.bridge.core.collections.reorder(&ids)
    }
    /// Atomic cross-source like/unlike/toggle (null toggles); emits only actual changes.
    pub fn set_liked(&self, track: JsonValue, liked: Option<bool>) -> Result<bool, JsError> {
        self.bridge
            .core
            .collections
            .set_liked(&decode(track)?, liked)
            .map_err(JsError::Error)
    }
    /// Test an opaque track EntityRef without involving the TSX mirror.
    pub fn is_liked(&self, key: String) -> Result<bool, JsError> {
        Ok(self
            .bridge
            .core
            .collections
            .is_liked(&EntityRef::decode(&key).map_err(crate::core::js_error)?))
    }
    /// Current opaque EntityRef set. Subscribe with onLiked for subsequent changes.
    pub fn liked_keys(&self) -> JsonValue {
        liked_keys(&self.bridge.core.collections)
    }
    /// Initial revision, then mutations/metadata/artwork events carrying affected playlist IDs. Stop unregisters.
    pub fn on_change(&mut self, #[args(change)] callback: Callback<JsonValue>) -> Stop {
        self.listen(false, callback)
    }
    /// Initial liked set and later liked mutations. Stop unregisters independently of onChange.
    pub fn on_liked(&mut self, #[args(keys)] callback: Callback<JsonValue>) -> Stop {
        self.listen(true, callback)
    }
    /// Persist an immutable complete offline remote mirror; online paging/refresh belongs to M4.
    pub fn favorite(&self, meta: JsonValue, tracks: Vec<JsonValue>) -> Result<String, JsError> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Meta {
            source_id: String,
            remote_id: String,
            source_label: String,
            title: String,
            #[serde(default)]
            description: String,
            #[serde(default)]
            cover: String,
        }
        let m: Meta = decode(meta)?;
        let tracks: Vec<Track> = decode(JsonValue::Array(tracks))?;
        self.bridge
            .core
            .collections
            .favorite(
                &m.source_id,
                &m.remote_id,
                &m.source_label,
                &m.title,
                &m.description,
                &m.cover,
                &tracks,
            )
            .map_err(JsError::Error)
    }
    /// Export reference-compatible Loro bytes including merge history.
    pub fn export_snapshot(&self) -> Vec<u8> {
        self.bridge.core.collections.export_snapshot()
    }
    /// Merge reference-compatible bytes idempotently, notifying only affected collections.
    pub fn import_snapshot(&self, bytes: Vec<u8>) -> Result<Vec<String>, String> {
        self.bridge.core.collections.import(&bytes)
    }
    /// Force pending writes for backup/exit; normal mutations automatically debounce.
    pub async fn flush(this: Instance<Self>) -> Result<(), String> {
        let store = this.with(|s| s.bridge.core.collections.clone());
        blocking(move || store.flush())
            .await
            .map_err(|e| e.to_string())?
    }
    /// Last background persistence error; a successful flush clears it.
    pub fn persistence_error(&self) -> Option<String> {
        self.bridge.core.collections.persistence_error()
    }
    /// Create an engine-owned virtual TrackList source with search and source filters.
    pub async fn query(this: Instance<Self>, query: JsonValue) -> Result<u32, JsError> {
        let query: Query = decode(query)?;
        let bridge = this.with(|s| s.bridge.clone());
        let provider = Rc::new(CollectionSource {
            store: bridge.core.collections.clone(),
            library: bridge.library.clone(),
            query: RefCell::new(query),
        });
        let handle = bridge
            .owner
            .run(|| track_source::register(provider.clone()));
        bridge.queries.borrow_mut().insert(handle, provider);
        Ok(handle)
    }
    /// Update filters without moving track arrays over the bridge.
    pub async fn set_query(
        this: Instance<Self>,
        handle: u32,
        query: JsonValue,
    ) -> Result<(), JsError> {
        let query = decode(query)?;
        this.with(|s| {
            let queries = s.bridge.queries.borrow();
            let provider = queries
                .get(&handle)
                .ok_or_else(|| JsError::Error("查询已释放".into()))?;
            *provider.query.borrow_mut() = query;
            if let Some(source) = track_source::get(handle) {
                source.refresh();
            }
            Ok(())
        })
    }
    /// Release a retained page's source.
    pub async fn release_query(this: Instance<Self>, handle: u32) {
        this.with(|s| {
            s.bridge.queries.borrow_mut().remove(&handle);
        });
        track_source::remove(handle);
    }
    /// Play the complete filtered source, beginning at an opaque track identity.
    pub async fn play_source(
        this: Instance<Self>,
        handle: u32,
        key: String,
    ) -> Result<(), JsError> {
        let source =
            track_source::get(handle).ok_or_else(|| JsError::Error("查询已释放".into()))?;
        let ids = source.ids.get_untracked();
        let at = ids.iter().position(|id| id == &key).unwrap_or(0);
        let tracks = ids
            .iter()
            .filter_map(|id| source.provider.track(id))
            .collect();
        let controller = this.with(|s| s.bridge.core.controller.clone());
        controller
            .execute(haiku_playback::Command::Play(tracks, at as i32))
            .await
            .map_err(crate::core::js_error)
    }
    /// Resolve selected opaque track identities in a native source for bulk commands.
    pub async fn selected_tracks(handle: u32, keys: Vec<String>) -> JsonValue {
        let tracks = track_source::get(handle)
            .map(|s| {
                keys.iter()
                    .filter_map(|id| s.provider.track(id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        serde_json::to_value(tracks).unwrap()
    }
}
/// One native like control shared by Dock and now-playing; no JS mirror in the mutation path.
pub(crate) fn like_button(dark: impl Into<Prop<bool>>) -> impl IntoElement {
    let dark = dark.into();
    let p = crate::presentation::get().expect("Core.open");
    let track = p.track;
    let revision = p.bridge.collections_revision;
    let store = p.bridge.collections.clone();
    let current = store.clone();
    let liked = computed(move || {
        revision.get();
        track.with(|t| {
            t.as_ref()
                .is_some_and(|t| current.is_liked(&t.entity_ref()))
        })
    });
    nsh_base::button().accessibility_label(move ||Some(if liked.get(){"取消喜欢"}else{"喜欢"}.into()))
        .selected(move ||liked.get()).disabled(move ||track.with(Option::is_none))
        .size_8().rounded_full().items_center().justify_center()
        .app_class("hover:opacity-80 active:opacity-60 focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40")
        .on_press(move || {if let Some(track)=track.get_untracked(){if let Err(e)=store.set_liked(&track,None){log::error!("Like: {e}");}}})
        .child(view().tooltip(crate::player_controls::hint("喜欢 / 取消喜欢"))
            .child(image(move || nsh_core::element::ImageSrc::svg_markup(if liked.get() {
                include_str!("../../../assets/icons/heart-filled.svg")
            } else { include_str!("../../../assets/icons/heart.svg") })).size_5()
                .tint_color(move ||if liked.get(){nsh_base::theme().primary}else if dark.get(){nsh_base::theme().color("now-playing-foreground").unwrap()}else{nsh_base::theme().foreground})))
}
