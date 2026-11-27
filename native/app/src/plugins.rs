//! Plugin settings bridge. The host owns packages, sources, credentials and login drafts.
use haiku_plugins::{Cancellation, Host, LoginDraft, QrSession};
use nsh_core::{
    JsonValue,
    element::{Trace, Tracer},
    prelude::*,
};
use serde_json::json;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
struct Listeners(Rc<RefCell<BTreeMap<u64, Callback<JsonValue>>>>);
unsafe impl Trace for Listeners {
    fn trace(&self, t: &mut Tracer<'_>) {
        self.0.as_ref().trace(t);
    }
}
struct Login {
    draft: LoginDraft,
    qr: Option<QrSession>,
    cancel: Cancellation,
}
/// Packages, sources and plugin development tools. All writes commit in Rust.
#[class]
pub struct PluginService {
    host: Arc<Host>,
    home: crate::discovery::HomeCache,
    listeners: Listeners,
    event_listeners: Listeners,
    alive: Rc<std::cell::Cell<bool>>,
    next: u64,
    fixtures: Arc<Mutex<Vec<haiku_plugins::contract::ReplayServer>>>,
    logins: Arc<tokio::sync::Mutex<BTreeMap<String, Login>>>,
    cancellations: Arc<Mutex<BTreeMap<String, Cancellation>>>,
    collections: haiku_store::collections::Collections,
    favorite_refresh: crate::favorite_refresh::FavoriteRefresh,
    remote: Rc<crate::sources::RemoteSources>,
    network_fixtures: Arc<Mutex<Vec<crate::network_fixture::Server>>>,
    workspaces: std::path::PathBuf,
}
impl PluginService {
    pub(crate) fn new(
        host: Arc<Host>,
        logs: haiku_store::logs::Logs,
        collections: haiku_store::collections::Collections,
        home_root: std::path::PathBuf,
        workspaces: std::path::PathBuf,
    ) -> Instance<Self> {
        let event_listeners = Listeners::default();
        let listeners = event_listeners.clone();
        let alive = Rc::new(std::cell::Cell::new(true));
        let running = alive.clone();
        let mut events = host.subscribe_events();
        // Rust async initialization is entered under the engine's reactive graph;
        // synchronous JS class calls are not. Keep the pump at this boundary.
        Owner::detached().run(|| {
            spawn(async move {
                while running.get() {
                    loop {
                        match events.try_recv() {
                            Ok(event) => {
                                if event["type"].as_str() == Some("log")
                                    || event.get("level").is_some()
                                {
                                    let logs = logs.clone();
                                    let entry = event.clone();
                                    spawn(async move {
                                        let _ = blocking(move || {
                                            if entry["level"] == "error" {
                                                logs.notify(
                                                    "error",
                                                    "插件执行失败",
                                                    entry["message"].as_str(),
                                                );
                                            }
                                            logs.add(
                                                "plugin",
                                                entry["level"].as_str().unwrap_or("info"),
                                                entry["sourceId"]
                                                    .as_str()
                                                    .or_else(|| entry["pluginId"].as_str())
                                                    .unwrap_or("plugin"),
                                                None,
                                                entry["message"].as_str().unwrap_or("插件事件"),
                                                Some(&entry.to_string()),
                                            )
                                        })
                                        .await;
                                    });
                                }
                                let callbacks: Vec<_> =
                                    listeners.0.borrow().values().cloned().collect();
                                for callback in callbacks {
                                    if running.get() {
                                        callback.post(event.clone());
                                    }
                                }
                            }
                            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
                            Err(_) => break,
                        }
                    }
                    timer(std::time::Duration::from_millis(100)).await;
                }
            });
        });
        Instance::new(Self {
            home: crate::discovery::HomeCache::new(home_root),
            host: host.clone(),
            listeners: Listeners::default(),
            event_listeners,
            alive,
            next: 0,
            fixtures: Arc::default(),
            logins: Arc::default(),
            cancellations: Arc::default(),
            favorite_refresh: crate::favorite_refresh::FavoriteRefresh::new(host.clone(), collections.clone()),
            collections,
            remote: Rc::new(Default::default()),
            network_fixtures: Arc::default(),
            workspaces,
        })
    }
    pub(crate) fn host(this: &Instance<Self>) -> Arc<Host> {
        this.with(|s| s.host.clone())
    }
    fn notify(this: &Instance<Self>, value: &JsonValue) {
        let listeners = this.with(|s| s.listeners.clone());
        let callbacks: Vec<_> = listeners.0.borrow().values().cloned().collect();
        for callback in callbacks {
            callback.post(value.clone());
        }
    }
    async fn snapshot(host: Arc<Host>) -> Result<JsonValue, String> {
        background(async move {
            let packages = host
                .packages()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|p| {
                    let mut descriptor = serde_json::to_value(&p.descriptor).unwrap();
                    descriptor.as_object_mut().unwrap().remove("covers");
                    descriptor["logo"] = json!(crate::plugin_tools::logo(
                        &p.path,
                        p.descriptor.logo.as_deref()
                    ));
                    descriptor["parameters"] =
                        json!(crate::plugin_tools::parameters(&p.descriptor.capabilities));
                    descriptor["enabled"] = json!(host.package_enabled(&p.descriptor.id));
                    descriptor
                })
                .collect::<Vec<_>>();
            let sources = host
                .records()
                .into_iter()
                .map(|r| {
                    let mut value = serde_json::to_value(&r).unwrap();
                    let (needs_login, user, connection) = host.source_presentation(&r);
                    value["needsLogin"] = json!(needs_login);
                    value["user"] = json!(user);
                    value["connection"] = json!(connection);
                    value
                })
                .collect::<Vec<_>>();
            Ok(json!({"packages":packages,"sources":sources,"errors":host.source_errors()}))
        })
        .await
        .map_err(|e| e.to_string())?
    }
    /// Cancellation shared by the home continuation pages of one source, replaced on refresh.
    fn home_pages_key(source: &str) -> String {
        format!("home-pages:{source}")
    }
    pub(crate) async fn changed(this: &Instance<Self>) -> Result<JsonValue, String> {
        let value = Self::snapshot(Self::host(this)).await?;
        Self::notify(this, &value);
        Ok(value)
    }
}
impl Drop for PluginService {
    fn drop(&mut self) {
        self.alive.set(false);
        for cancel in self.cancellations.lock().unwrap().values() {
            cancel.cancel();
        }
    }
}
#[class]
impl PluginService {
    /// First-page home; refresh=false serves a fresh disk hit immediately. Refresh=true fetches and preserves radio entries.
    pub async fn home(
        this: Instance<Self>,
        source: String,
        refresh: bool,
    ) -> Result<JsonValue, String> {
        let projection_host = Self::host(&this);
        let projection_source = source.clone();
        let projection_size = 192;
        // A source runs plugin calls one at a time. A refresh discards every continuation of the
        // old feed, so it cancels those still queued or running instead of waiting behind them.
        if refresh {
            let key = Self::home_pages_key(&source);
            if let Some(pages) = this.with(|s| s.cancellations.lock().unwrap().remove(&key)) {
                pages.cancel();
            }
        }
        let result: Result<JsonValue, String> = async {
            let host = Self::host(&this);
            let cache = this.with(|s| s.home.clone());
            background(async move { cache.home(host, source, refresh).await })
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())
        }
        .await;
        result.map(|mut value| {
            let fallback = value["shelves"].as_array().into_iter().flatten()
                .filter(|s| s["kind"] == "tracks")
                .flat_map(|s| s["items"].as_array().into_iter().flatten())
                .find_map(|t| t["cover"].as_str())
                .map(|image| crate::sources::cover_url(&projection_host, &projection_source, image, 380));
            value["fallbackCover"] = json!(fallback);
            crate::sources::project_covers(&projection_host, &projection_source, value, projection_size)
        })
    }
    /// Continuation page of home shelves; never cached.
    pub async fn home_page(
        this: Instance<Self>,
        source: String,
        cursor: String,
    ) -> Result<JsonValue, String> {
        let projection_host = Self::host(&this);
        let projection_source = source.clone();
        let host = Self::host(&this);
        let cache = this.with(|s| s.home.clone());
        let key = Self::home_pages_key(&source);
        let cancel = this.with(|s| {
            s.cancellations.lock().unwrap().entry(key.clone()).or_default().clone()
        });
        let value = background(async move { cache.page(host, source, cursor, Some(&cancel)).await })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        Ok(crate::sources::project_covers(&projection_host, &projection_source, value, 192))
    }
    /// Category groups with optional provider-declared sorts.
    pub async fn categories(this: Instance<Self>, source: String) -> Result<JsonValue, String> {
        let projection_host = Self::host(&this);
        let projection_source = source.clone();
        let projection_size = 224;
        let result: Result<JsonValue, String> = async {
            let host = Self::host(&this);
            background(async move {
                Ok::<_, haiku_plugins::PluginError>(json!(
                    haiku_plugins::SourceApi::new(host.source(&source)?)
                        .categories(None)
                        .await?
                ))
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
        }
        .await;
        result.map(|value| {
            crate::sources::project_covers(
                &projection_host,
                &projection_source,
                value,
                projection_size,
            )
        })
    }
    /// One opaque cursor page of playlists for a category and optional declared sort.
    pub async fn category_playlists(
        this: Instance<Self>,
        source: String,
        id: String,
        sort: Option<String>,
        cursor: Option<String>,
    ) -> Result<JsonValue, String> {
        let projection_host = Self::host(&this);
        let projection_source = source.clone();
        let projection_size = 224;
        let result: Result<JsonValue, String> = async {
            let host = Self::host(&this);
            background(async move {
                Ok::<_, haiku_plugins::PluginError>(json!(
                    haiku_plugins::SourceApi::new(host.source(&source)?)
                        .category_playlists(
                            &id,
                            haiku_plugins::PageArgs { cursor, limit: 40 },
                            sort.as_deref(),
                            None
                        )
                        .await?
                ))
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
        }
        .await;
        result.map(|value| {
            crate::sources::project_covers(
                &projection_host,
                &projection_source,
                value,
                projection_size,
            )
        })
    }
    /// Ordinary nonempty radio channel IDs; private FM is selected separately.
    pub async fn radio_channels(this: Instance<Self>, source: String) -> Result<JsonValue, String> {
        let projection_host = Self::host(&this);
        let projection_source = source.clone();
        let projection_size = 224;
        let result: Result<JsonValue, String> = async {
            let host = Self::host(&this);
            background(async move {
                Ok::<_, haiku_plugins::PluginError>(json!(
                    haiku_plugins::SourceApi::new(host.source(&source)?)
                        .radio_channels(None)
                        .await?
                ))
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
        }
        .await;
        result.map(|value| {
            crate::sources::project_covers(
                &projection_host,
                &projection_source,
                value,
                projection_size,
            )
        })
    }
    /// Search one enabled source and SDK kind; the cursor is opaque, tracks remain playable.
    pub async fn search(
        this: Instance<Self>,
        source: String,
        kind: String,
        query: String,
        cursor: Option<String>,
    ) -> Result<JsonValue, String> {
        let projection_host = Self::host(&this);
        let projection_source = source.clone();
        let projection_size = if kind == "track" { 104 } else { 192 };
        let result: Result<JsonValue, String> = async {
            let host = Self::host(&this);
            background(
                async move { crate::sources::search(host, source, kind, query, cursor).await },
            )
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
        }
        .await;
        result.map(|value| {
            crate::sources::project_covers(
                &projection_host,
                &projection_source,
                value,
                projection_size,
            )
        })
    }
    /// Concurrent first-page track/playlist results per enabled source, with isolated errors.
    /// The UI previews three of each; tracks contains the entire first page for playback.
    pub async fn search_preview(this: Instance<Self>, query: String) -> Result<JsonValue, String> {
        let projection_host = Self::host(&this);
        let projection_source = String::new();
        let projection_size = 192;
        let result: Result<JsonValue, String> = async {
            let host = Self::host(&this);
            background(async move { crate::sources::search_preview(host, query).await })
                .await
                .map_err(|e| e.to_string())
        }
        .await;
        result.map(|value| {
            crate::sources::project_covers(
                &projection_host,
                &projection_source,
                value,
                projection_size,
            )
        })
    }
    /// Start two isolated search sources, including a track-only source, for offline acceptance.
    pub async fn start_search_fixture(this: Instance<Self>) -> Result<JsonValue, String> {
        let first = Self::start_network_fixture(this.clone()).await?;
        let host = Self::host(&this);
        let url = first["url"].as_str().unwrap().to_owned();
        let second = background(async move {
            host.install_files(
                BTreeMap::from([(
                    "plugin.ts".into(),
                    include_bytes!("../tests/fixtures/search-lite-plugin/plugin.ts").to_vec(),
                )]),
                false,
            )
            .await
            .map_err(|e| e.to_string())?;
            host.connect(
                "search.lite.fixture",
                BTreeMap::from([("url".into(), url)]),
                Some("轻量曲库".into()),
            )
            .await
            .map(|s| s.record.source_id.clone())
            .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        Self::changed(&this).await?;
        Ok(json!({"sourceId":first["sourceId"],"secondSourceId":second}))
    }
    /// Start the isolated loopback library/audio fixture for offline acceptance previews.
    pub async fn start_network_fixture(this: Instance<Self>) -> Result<JsonValue, String> {
        let host = Self::host(&this);
        let fixtures = this.with(|s| s.network_fixtures.clone());
        let result = background(async move {
            let server = crate::network_fixture::Server::start()?;
            let url = server.url.clone();
            host.install_files(
                BTreeMap::from([(
                    "plugin.ts".into(),
                    include_bytes!("../tests/fixtures/network-plugin/plugin.ts").to_vec(),
                )]),
                false,
            )
            .await
            .map_err(|e| e.to_string())?;
            let source = host
                .connect(
                    "network.fixture",
                    BTreeMap::from([("url".into(), url.clone())]),
                    Some("回声音乐库".into()),
                )
                .await
                .map_err(|e| e.to_string())?;
            fixtures.lock().unwrap().push(server);
            Ok::<_, String>(json!({"sourceId":source.record.source_id,"url":url}))
        })
        .await
        .map_err(|e| e.to_string())??;
        Self::changed(&this).await?;
        Ok(result)
    }
    /// Encode a lossless host identity for navigation; callers never split source/native IDs.
    pub fn reference(source: String, kind: String, id: String) -> String {
        haiku_model::EntityRef { source, kind, id }.encode()
    }
    /// Distinguinguish source detail routes from local library/collection routes.
    pub fn is_remote(reference: String) -> bool {
        haiku_model::EntityRef::decode(&reference)
            .is_ok_and(|r| r.source != "local" && r.source != "collections")
    }
    /// Fetch one SDK detail page by opaque EntityRef; artist pages also include paged albums.
    pub async fn detail(
        this: Instance<Self>,
        reference: String,
        cursor: Option<String>,
    ) -> Result<JsonValue, String> {
        let host = Self::host(&this);
        let projection_host = host.clone();
        let r = haiku_model::EntityRef::decode(&reference).map_err(|e| e.to_string())?;
        let projection_source = r.source.clone();
        let value = background(async move {
            let source = host.source(&r.source)?;
            let capabilities = source.descriptor().capabilities;
            let api = haiku_plugins::SourceApi::new(source);
            let page = haiku_plugins::PageArgs { cursor: cursor.clone(), limit: 40 };
            let mut detail = api.detail(&r.kind, &r.id, page.clone(), None).await?;
            if let Some(info) = &mut detail.info { info.reference = Some(r.clone()); }
            let mut albums = None;
            if r.kind == "artist" {
                if capabilities.iter().any(|p| p == "artist.tracks") {
                    detail.tracks = api.artist_tracks(&r.id, page.clone(), None).await?;
                }
                if cursor.is_none() && capabilities.iter().any(|p| p == "artist.albums") {
                    albums = Some(api.artist_albums(&r.id, haiku_plugins::PageArgs {cursor:None, limit:40}, None).await?);
                }
            }
            Ok::<_, haiku_plugins::PluginError>(json!({"info":detail.info,"tracks":detail.tracks,
                "albums":albums,"canFavorite":r.kind=="playlist" && capabilities.iter().any(|p| p=="playlist.get")}))
        }).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        Ok(crate::sources::project_covers(&projection_host, &projection_source, value, 192))
    }
    /// Fetch all user/favorite playlists declared by a source's `me.playlists` capability.
    pub async fn me_playlists(
        this: Instance<Self>,
        source_id: String,
    ) -> Result<JsonValue, String> {
        let host = Self::host(&this);
        let projection_host = host.clone();
        let projection_source = source_id.clone();
        let value = background(async move {
            let source = host.source(&source_id)?;
            let api = haiku_plugins::SourceApi::new(source);
            let items = haiku_plugins::SourceApi::collect_all(|cursor| {
                let api = api.clone();
                async move {
                    api.me_playlists(
                        haiku_plugins::PageArgs { cursor, limit: 50 },
                        None,
                    )
                    .await
                }
            })
            .await?;
            Ok::<_, haiku_plugins::PluginError>(json!(items))
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
        Ok(crate::sources::project_covers(
            &projection_host,
            &projection_source,
            value,
            192,
        ))
    }
    /// Fetch another artist album page; cursors remain opaque plugin values.
    pub async fn artist_albums(
        this: Instance<Self>,
        reference: String,
        cursor: Option<String>,
    ) -> Result<JsonValue, String> {
        let projection_host = Self::host(&this);
        let projection_source = haiku_model::EntityRef::decode(&reference)
            .map_err(|e| e.to_string())?
            .source;
        let projection_size = 192;
        let result: Result<JsonValue, String> = async {
            let host = Self::host(&this);
            let r = haiku_model::EntityRef::decode(&reference).map_err(|e| e.to_string())?;
            background(async move {
                let api = haiku_plugins::SourceApi::new(host.source(&r.source)?);
                Ok::<_, haiku_plugins::PluginError>(json!(
                    api.artist_albums(&r.id, haiku_plugins::PageArgs { cursor, limit: 40 }, None)
                        .await?
                ))
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
        }
        .await;
        result.map(|value| {
            crate::sources::project_covers(
                &projection_host,
                &projection_source,
                value,
                projection_size,
            )
        })
    }
    /// Browse one declared library card (or all cards' first pages when card is null).
    pub async fn browse(
        this: Instance<Self>,
        source: String,
        card: Option<String>,
        cursor: Option<String>,
    ) -> Result<JsonValue, String> {
        let projection_host = Self::host(&this);
        let projection_source = source.clone();
        let projection_size = 192;
        let result: Result<JsonValue, String> = async {
            let host = Self::host(&this);
            background(async move {
                let source = host.source(&source)?;
                if source.needs_login() {
                    return Err(haiku_plugins::PluginError::new(
                        "AUTH_REQUIRED",
                        "来源尚未登录，请前往设置连接",
                    ));
                }
                let api = haiku_plugins::SourceApi::new(source);
                let mut groups = vec![];
                for c in api
                    .library_cards()?
                    .into_iter()
                    .filter(|c| card.as_ref().is_none_or(|id| id == &c.id))
                {
                    let page = haiku_plugins::PageArgs {
                        cursor: cursor.clone(),
                        limit: 40,
                    };
                    let result = if c.entity == "track" {
                        json!(api.browse_tracks(&c.id, page, None).await?)
                    } else {
                        json!(api.browse(&c.id, &c.entity, page, None, None).await?)
                    };
                    groups.push(json!({"card":c,"page":result}));
                }
                Ok::<_, haiku_plugins::PluginError>(json!({"groups":groups}))
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
        }
        .await;
        result.map(|value| {
            crate::sources::project_covers(
                &projection_host,
                &projection_source,
                value,
                projection_size,
            )
        })
    }
    /// Reserve a shared refresh lease before awaiting; release it when the detail page leaves.
    pub fn begin_favorite_refresh(&self, reference: String) -> Result<String, String> {
        let reference = haiku_model::EntityRef::decode(&reference).map_err(|e| e.to_string())?;
        self.favorite_refresh.reserve(reference)
    }
    /// Load all pages and atomically replace the mirror. False means a newer version won.
    pub async fn refresh_favorite(this: Instance<Self>, lease: String) -> Result<bool, String> {
        let pending = this.with(|s| s.favorite_refresh.pending(&lease))?;
        background(pending).await.map_err(|e| e.to_string())?
    }
    /// Release this page's lease; the last departing consumer cancels upstream requests.
    pub fn cancel_favorite_refresh(&self, lease: String) {
        self.favorite_refresh.release(&lease);
    }
    /// Snapshot every playlist page (maximum 100) into the existing offline favorite contract.
    /// False removes only the local favorite mirror.
    pub async fn favorite_playlist(
        this: Instance<Self>,
        reference: String,
        favorite: bool,
    ) -> Result<String, String> {
        let r = haiku_model::EntityRef::decode(&reference).map_err(|e| e.to_string())?;
        if r.kind != "playlist" {
            return Err("只能收藏远端歌单".into());
        }
        let store = this.with(|s| s.collections.clone());
        let host = Self::host(&this);
        background(async move {
            let id = format!("favorite:{}:{}", r.source, r.id);
            if !favorite {
                store.delete(&id)?;
                return Ok(id);
            }
            let source = host.source(&r.source).map_err(|e| e.to_string())?;
            let label = source
                .record
                .display_name
                .clone()
                .unwrap_or_else(|| source.descriptor().name);
            let api = haiku_plugins::SourceApi::new(source);
            let first = api
                .detail(
                    "playlist",
                    &r.id,
                    haiku_plugins::PageArgs {
                        cursor: None,
                        limit: 40,
                    },
                    None,
                )
                .await
                .map_err(|e| e.to_string())?;
            let info = first.info.ok_or("歌单信息缺失")?;
            let tracks = haiku_plugins::SourceApi::collect_all(|cursor| {
                let api = api.clone();
                let id = r.id.clone();
                async move {
                    Ok(api
                        .detail(
                            "playlist",
                            &id,
                            haiku_plugins::PageArgs { cursor, limit: 40 },
                            None,
                        )
                        .await?
                        .tracks)
                }
            })
            .await
            .map_err(|e| e.to_string())?;
            store.favorite(
                &r.source,
                &r.id,
                &label,
                &info.title,
                info.description.as_deref().unwrap_or(""),
                info.cover.as_deref().unwrap_or(""),
                &tracks,
            )
        })
        .await
        .map_err(|e| e.to_string())?
    }
    /// Retain or update a native virtual TrackList source; release it when its page is disposed.
    pub async fn track_source(
        this: Instance<Self>,
        tracks: Vec<JsonValue>,
        handle: Option<u32>,
    ) -> Result<u32, String> {
        let tracks = crate::core::decode(json!(tracks)).map_err(|e| format!("{e:?}"))?;
        let remote = this.with(|s| s.remote.clone());
        if let Some(handle) = handle {
            let sources = remote.tracks.borrow();
            let source = sources.get(&handle).ok_or("曲目列表已释放")?;
            *source.0.borrow_mut() = tracks;
            if let Some(s) = crate::track_source::get(handle) {
                s.refresh();
            }
            Ok(handle)
        } else {
            let source = Rc::new(crate::sources::RemoteTracks(RefCell::new(tracks)));
            let id = remote
                .owner
                .run(|| crate::track_source::register(source.clone()));
            remote.tracks.borrow_mut().insert(id, source);
            Ok(id)
        }
    }
    /// Release an engine-local remote TrackList source, independently of other pages.
    pub async fn release_tracks(this: Instance<Self>, handle: u32) {
        this.with(|s| {
            s.remote.tracks.borrow_mut().remove(&handle);
        });
        crate::track_source::remove(handle);
    }
    /// Read installed descriptors and committed sources (secret connection fields are redacted).
    pub async fn list(this: Instance<Self>) -> Result<JsonValue, String> {
        Self::snapshot(Self::host(&this)).await
    }
    /// Subscribe to committed writes through this bridge. Call list for the initial snapshot.
    pub fn on_change(&mut self, #[args(snapshot)] callback: Callback<JsonValue>) -> Stop {
        self.next += 1;
        let id = self.next;
        self.listeners.0.borrow_mut().insert(id, callback);
        let listeners = self.listeners.clone();
        Stop::new(move || {
            listeners.0.borrow_mut().remove(&id);
        })
    }
    /// Hear redacted log/HTTP events from this host. Stop prevents further callbacks.
    /// HTTP traffic (headers and body previews) is traced only while some listener exists,
    /// across all windows: in practice, while the developer tools window is open.
    pub fn on_event(&mut self, #[args(event)] callback: Callback<JsonValue>) -> Stop {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static LISTENING: AtomicUsize = AtomicUsize::new(0);
        self.next += 1;
        let id = self.next;
        self.event_listeners.0.borrow_mut().insert(id, callback);
        if LISTENING.fetch_add(1, Ordering::SeqCst) == 0 {
            haiku_plugins::set_http_capture(true);
        }
        let listeners = self.event_listeners.clone();
        Stop::new(move || {
            if listeners.0.borrow_mut().remove(&id).is_some()
                && LISTENING.fetch_sub(1, Ordering::SeqCst) == 1
            {
                haiku_plugins::set_http_capture(false);
            }
        })
    }
    /// Start a loopback fixture server for an explicit offline developer preview.
    pub async fn start_fixture(this: Instance<Self>, plugin: String) -> Result<String, String> {
        let host = Self::host(&this);
        let fixtures = this.with(|s| s.fixtures.clone());
        background(async move {
            let server =
                haiku_plugins::contract::ReplayServer::from_package(&host.package(&plugin)?.path)?;
            let url = server.url().to_string();
            fixtures.lock().unwrap().push(server);
            Ok::<_, haiku_plugins::PluginError>(url)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
    }
    /// Stop explicit fixture previews when the test or preview surface closes.
    pub async fn stop_fixtures(this: Instance<Self>) -> Result<(), String> {
        let fixtures = this.with(|s| s.fixtures.clone());
        background(async move {
            fixtures.lock().unwrap().clear();
        })
        .await
        .map_err(|e| e.to_string())
    }
    /// Reserve a cancellable library connection draft before verification starts.
    pub fn begin_connection(&mut self) -> String {
        self.next += 1;
        let id = format!("connection-{}", self.next);
        self.cancellations
            .lock()
            .unwrap()
            .insert(id.clone(), Cancellation::default());
        id
    }
    /// Verify connection fields before registering a library; failed attempts roll back.
    pub async fn connect(
        this: Instance<Self>,
        plugin: String,
        values: JsonValue,
        name: Option<String>,
        request: Option<String>,
    ) -> Result<JsonValue, String> {
        let host = Self::host(&this);
        let cancel = this.with(|s| {
            request
                .as_ref()
                .and_then(|id| s.cancellations.lock().unwrap().get(id).cloned())
        });
        let values = serde_json::from_value(values).map_err(|e| e.to_string())?;
        background(async move {
            host.connect_cancellable(&plugin, values, name, cancel.as_ref())
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        Self::changed(&this).await
    }
    /// Enable or disable one existing source, cancelling its active calls when disabled.
    pub async fn set_enabled(
        this: Instance<Self>,
        source: String,
        enabled: bool,
    ) -> Result<JsonValue, String> {
        let host = Self::host(&this);
        background(async move {
            host.set_enabled(&source, enabled)
                .await
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        Self::changed(&this).await
    }
    /// Apply package enablement to its instances; activate public packages on first use.
    pub async fn set_package_enabled(
        this: Instance<Self>,
        plugin: String,
        enabled: bool,
    ) -> Result<JsonValue, String> {
        let host = Self::host(&this);
        background(async move {
            host.set_package_enabled(&plugin, enabled)
                .await
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        Self::changed(&this).await
    }
    /// Persist a complete ordering of source ids.
    pub async fn reorder(this: Instance<Self>, ids: Vec<String>) -> Result<JsonValue, String> {
        let host = Self::host(&this);
        background(async move { host.reorder(&ids).await.map_err(|e| e.to_string()) })
            .await
            .map_err(|e| e.to_string())??;
        Self::changed(&this).await
    }
    /// Change the display name without changing a source identity.
    pub async fn rename(
        this: Instance<Self>,
        source: String,
        name: Option<String>,
    ) -> Result<JsonValue, String> {
        let host = Self::host(&this);
        background(async move { host.rename(&source, name).await.map_err(|e| e.to_string()) })
            .await
            .map_err(|e| e.to_string())??;
        Self::changed(&this).await
    }
    /// Remove a source and its local credentials/storage after caller confirmation.
    pub async fn remove(this: Instance<Self>, source: String) -> Result<JsonValue, String> {
        let host = Self::host(&this);
        background(async move { host.remove_source(&source).await.map_err(|e| e.to_string()) })
            .await
            .map_err(|e| e.to_string())??;
        Self::changed(&this).await
    }
    /// Log out; local credentials are cleared even if remote logout fails or the source is disabled.
    pub async fn logout(this: Instance<Self>, source: String) -> Result<JsonValue, String> {
        let host = Self::host(&this);
        let result = background(async move {
            match host.source(&source) {
                Ok(active) => active.logout().await,
                Err(_) => host.remove_source(&source).await,
            }
        })
        .await
        .map_err(|e| e.to_string())?;
        let snapshot = Self::changed(&this).await?;
        result.map_err(|e| e.to_string())?;
        Ok(snapshot)
    }
    /// Import a directory, ZIP or single TypeScript file, or download an HTTPS ZIP/package.
    pub async fn import_package(
        this: Instance<Self>,
        location: String,
    ) -> Result<JsonValue, String> {
        let location = local_location(&location)?;
        let host = Self::host(&this);
        background(async move {
            host.import_location(&location)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        Self::changed(&this).await
    }
    /// Choose a ZIP, TypeScript file or directory with the native system file dialog.
    pub async fn pick_import(kind: String) -> Result<Option<String>, String> {
        use nsh_core::file_dialog::{FileFilter, OpenDialog};
        let dialog = match kind.as_str() {
            "directory" => OpenDialog::folders(),
            "zip" => OpenDialog::files().filter(FileFilter::new("插件 ZIP", ["zip"])),
            "typescript" => OpenDialog::files().filter(FileFilter::new("TypeScript 插件", ["ts"])),
            _ => return Err("Unknown import kind".into()),
        };
        let paths = pick_paths(dialog)
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        Ok(paths.first().map(|p| p.to_string_lossy().into_owned()))
    }
    /// Choose a destination ZIP path for exporting an installed plugin package.
    pub async fn pick_export(plugin: String) -> Result<Option<String>, String> {
        use nsh_core::file_dialog::{FileFilter, SaveDialog};
        let safe = plugin
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>();
        let name = if safe.is_empty() {
            "plugin.zip".into()
        } else {
            format!("{safe}.zip")
        };
        pick_save_path(
            SaveDialog::new()
                .file_name(name)
                .filter(FileFilter::new("插件 ZIP", ["zip"])),
        )
        .await
        .map(|p| p.map(|p| p.to_string_lossy().into_owned()))
        .map_err(|e| e.to_string())
    }
    /// Export an installed plugin package to a ZIP archive at the chosen path.
    pub async fn export_package(
        this: Instance<Self>,
        plugin: String,
        path: String,
    ) -> Result<(), String> {
        let host = Self::host(&this);
        let dest = std::path::PathBuf::from(path);
        background(async move { host.export_package(&plugin, &dest).map_err(|e| e.to_string()) })
            .await
            .map_err(|e| e.to_string())?
    }
    /// Poll a local package every 500ms and hot reload changes transactionally, preserving KV.
    /// Stop cancels future reloads; an already running install finishes its transaction.
    pub async fn watch_directory(
        this: Instance<Self>,
        location: String,
        #[args(status)] callback: Callback<String>,
    ) -> Result<Stop, String> {
        let path = std::path::PathBuf::from(local_location(&location)?);
        let first_path = path.clone();
        let first = background(async move { crate::plugin_tools::directory_hash(&first_path) })
            .await
            .map_err(|e| e.to_string())??;
        let running = Rc::new(std::cell::Cell::new(true));
        let active = running.clone();
        let host = Self::host(&this);
        Owner::detached().run(|| {
            spawn(async move {
                let mut previous = first;
                while active.get() {
                    timer(std::time::Duration::from_millis(500)).await;
                    if !active.get() {
                        break;
                    }
                    let path = path.clone();
                    let host = host.clone();
                    let change = background(async move {
                        let hash = crate::plugin_tools::directory_hash(&path)?;
                        if hash == previous {
                            return Ok::<_, String>((hash, None));
                        }
                        let result = host
                            .install_directory(&path, false)
                            .await
                            .map(|_| ())
                            .map_err(|e| e.to_string());
                        Ok((hash, Some(result)))
                    })
                    .await;
                    if !active.get() {
                        break;
                    }
                    match change {
                        Ok(Ok((hash, result))) => {
                            previous = hash;
                            if let Some(result) = result {
                                match result {
                                    Ok(()) => {
                                        let _ = Self::changed(&this).await;
                                        callback.post("已热重载，继续监听".into());
                                    }
                                    Err(error) => callback.post(format!("重载失败：{error}")),
                                }
                            }
                        }
                        result => callback.post(format!("目录读取失败：{result:?}")),
                    }
                }
            })
        });
        Ok(Stop::new(move || running.set(false)))
    }
    /// Create an agent authoring workspace: reference SDK files, an empty `plugin/` package
    /// directory and the prompt to paste into an agent. The prompt is also saved as AGENT.md.
    pub async fn create_workspace(
        this: Instance<Self>,
        name: String,
        author: String,
        website: String,
        description: String,
    ) -> Result<JsonValue, String> {
        let root = this.with(|s| s.workspaces.clone());
        blocking(move || crate::plugin_workspace::create(&root, &name, &author, &website, &description))
            .await
            .map_err(|e| e.to_string())?
    }
    /// Authoring workspaces created by the developer tools, newest first.
    pub async fn workspaces(this: Instance<Self>) -> Result<JsonValue, String> {
        let root = this.with(|s| s.workspaces.clone());
        blocking(move || json!(crate::plugin_workspace::list(&root)))
            .await
            .map_err(|e| e.to_string())
    }
    /// Delete a workspace directory. An installed package stays installed.
    pub async fn remove_workspace(this: Instance<Self>, path: String) -> Result<(), String> {
        let root = this.with(|s| s.workspaces.clone());
        blocking(move || crate::plugin_workspace::remove(&root, &path))
            .await
            .map_err(|e| e.to_string())?
    }
    /// Put any text on the system clipboard natively (see `copy_workspace_prompt`).
    pub async fn copy_text(text: String) -> Result<(), String> {
        blocking(move || crate::plugin_workspace::copy_to_clipboard(&text))
            .await
            .map_err(|e| e.to_string())?
    }
    /// Put a workspace's agent prompt on the system clipboard. Native, because the developer
    /// tools are a secondary window, where `Clipboard.writeText` does not reach the pasteboard.
    pub async fn copy_workspace_prompt(this: Instance<Self>, path: String) -> Result<(), String> {
        let root = this.with(|s| s.workspaces.clone());
        blocking(move || {
            let path = crate::plugin_workspace::resolve(&root, &path)?;
            crate::plugin_workspace::copy_to_clipboard(&crate::plugin_workspace::prompt_of(&path)?)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    /// Show a workspace directory in the system file manager.
    pub async fn reveal_workspace(this: Instance<Self>, path: String) -> Result<(), String> {
        let root = this.with(|s| s.workspaces.clone());
        blocking(move || {
            let path = crate::plugin_workspace::resolve(&root, &path)?;
            let opener = if cfg!(target_os = "macos") { "open" }
                else if cfg!(target_os = "windows") { "explorer" } else { "xdg-open" };
            std::process::Command::new(opener).arg(&path).spawn().map(|_| ()).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    /// Watch a workspace's `STATUS` file through file system events (no polling). Each
    /// `ready`/`done` the agent writes installs `plugin/` transactionally (keeping KV) when its
    /// content changed or the last load failed, and the outcome goes to `LOAD` for the agent.
    /// Status: `{phase, message, plugin?, summary?}` with phase one of waiting, writing, failed,
    /// loaded or done; only changes are posted. Stop drops the watcher, which ends the task.
    pub async fn watch_workspace(
        this: Instance<Self>,
        path: String,
        #[args(status)] callback: Callback<JsonValue>,
    ) -> Result<Stop, String> {
        use crate::plugin_workspace::{STATUS, Status};
        use futures::StreamExt;
        use notify::Watcher;
        let root = this.with(|s| s.workspaces.clone());
        let path = blocking(move || crate::plugin_workspace::resolve(&root, &path))
            .await
            .map_err(|e| e.to_string())??;
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<()>();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if event.is_ok_and(|e| e.paths.iter().any(|p| p.file_name().is_some_and(|n| n == STATUS))) {
                let _ = tx.unbounded_send(());
            }
        })
        .map_err(|e| e.to_string())?;
        watcher
            .watch(&path, notify::RecursiveMode::NonRecursive)
            .map_err(|e| e.to_string())?;
        let watcher = Rc::new(RefCell::new(Some(watcher)));
        let host = Self::host(&this);
        Owner::detached().run(|| {
            spawn(async move {
                let mut installed: Option<u64> = None;
                let mut loaded: Option<Result<JsonValue, String>> = None;
                let mut last = JsonValue::Null;
                loop {
                    let (dir, host, previous) = (path.clone(), host.clone(), installed);
                    let step = background(async move {
                        let status = crate::plugin_workspace::status(&dir);
                        if !matches!(status, Some(Status::Ready(_) | Status::Done(_))) {
                            return (status, None);
                        }
                        let package = dir.join("plugin");
                        let (hash, result) = match crate::plugin_tools::directory_hash(&package) {
                            Ok(hash) if Some(hash) == previous => return (status, None),
                            Ok(hash) => (Some(hash), host.install_directory(&package, false).await
                                .map(|d| json!({"id":d.id,"name":d.name,"version":d.version,
                                    "kind":d.kind,"capabilities":d.capabilities}))
                                .map_err(|e| e.to_string())),
                            Err(error) => (None, Err(error)),
                        };
                        crate::plugin_workspace::write_load(&dir, &result);
                        (status, Some((hash, result)))
                    })
                    .await;
                    let Ok((status, install)) = step else { break };
                    if let Some((hash, result)) = install {
                        installed = if result.is_ok() { hash } else { None };
                        if result.is_ok() {
                            let _ = Self::changed(&this).await;
                        }
                        loaded = Some(result);
                    }
                    let plugin = loaded.as_ref().and_then(|r| r.as_ref().ok()).cloned();
                    let value = match (&status, &loaded) {
                        (None, _) => json!({"phase":"waiting","message":"等待 Agent 写入 STATUS"}),
                        (Some(Status::Writing(message)), _) => json!({"phase":"writing",
                            "message": if message.is_empty() { "Agent 正在编写" } else { message }, "plugin": plugin}),
                        (Some(_), Some(Err(error))) => json!({"phase":"failed","message":error}),
                        (Some(Status::Ready(message)), _) => json!({"phase":"loaded",
                            "message": if message.is_empty() { "已热加载最新代码" } else { message }, "plugin": plugin}),
                        (Some(Status::Done(summary)), _) => json!({"phase":"done","message":"插件已完成并安装",
                            "plugin": plugin, "summary": summary}),
                    };
                    if value != last {
                        callback.post(value.clone());
                        last = value;
                    }
                    // Sleep until STATUS changes; one save emits a burst of events, so let it settle.
                    if rx.next().await.is_none() {
                        break;
                    }
                    timer(std::time::Duration::from_millis(150)).await;
                    while rx.try_recv().is_ok() {}
                }
            })
        });
        Ok(Stop::new(move || drop(watcher.borrow_mut().take())))
    }
    /// Delete an unused third-party package; the host rejects builtins and referenced packages.
    pub async fn delete_package(this: Instance<Self>, plugin: String) -> Result<JsonValue, String> {
        let host = Self::host(&this);
        background(async move {
            host.remove_package(&plugin)
                .await
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        Self::changed(&this).await
    }
    /// Begin a temporary platform login. Closing the surface must call cancelLogin.
    pub async fn begin_login(this: Instance<Self>, plugin: String) -> Result<String, String> {
        let (logins, cancellations, id) = this.with_mut(|s| {
            s.next += 1;
            (
                s.logins.clone(),
                s.cancellations.clone(),
                format!("login-{}", s.next),
            )
        });
        let host = Self::host(&this);
        let key = id.clone();
        let cancel = Cancellation::default();
        cancellations
            .lock()
            .unwrap()
            .insert(id.clone(), cancel.clone());
        let result = background(async move {
            let draft = host.begin_login(&plugin).await.map_err(|e| e.to_string())?;
            if cancel.is_cancelled() {
                return Err("Login cancelled".into());
            }
            logins.lock().await.insert(
                key,
                Login {
                    draft,
                    qr: None,
                    cancel,
                },
            );
            Ok::<_, String>(())
        })
        .await
        .map_err(|e| e.to_string())?;
        if result.is_err() {
            cancellations.lock().unwrap().remove(&id);
        }
        result?;
        Ok(id)
    }
    /// Drive password/cookie/SMS/web/QR authentication; only a successful login is committed.
    pub async fn login_step(
        this: Instance<Self>,
        id: String,
        action: String,
        input: JsonValue,
    ) -> Result<JsonValue, String> {
        let sessions = this.with(|s| s.logins.clone());
        let key = id.clone();
        let result = background(async move {
            let mut sessions = sessions.lock().await;
            let login = sessions.get_mut(&key).ok_or("Login is closed")?;
            let cancel = Some(&login.cancel);
            let text = input.as_str().unwrap_or("");
            let mut done = false;
            let result = match action.as_str() {
                "password" => {
                    let values = serde_json::from_value(input).map_err(|e| e.to_string())?;
                    login.draft.password(values, cancel).await.map(|r| {
                        done = true;
                        json!(r.user)
                    })
                }
                "cookie" => login.draft.cookie(text, cancel).await.map(|r| {
                    done = true;
                    json!(r.user)
                }),
                "sms.send" => login.draft.sms_send(text, cancel).await,
                "sms.verify" => login.draft.sms_verify(text, cancel).await.map(|r| {
                    done = true;
                    json!(r.user)
                }),
                "web" => login.draft.web_done(input, cancel).await.map(|r| {
                    done = r.is_some();
                    json!(r.map(|r| r.user))
                }),
                "qrcode.start" => {
                    let qr = login
                        .draft
                        .qrcode_start(text, login.cancel.clone())
                        .await
                        .map_err(|e| e.to_string())?;
                    let mut presentation = qr.presentation.clone();
                    if presentation["image"].as_str().is_none() {
                        if let Some(url) = presentation["url"].as_str() {
                            presentation["image"] = json!(crate::plugin_tools::qr_image(url)?);
                        }
                    }
                    login.qr = Some(qr);
                    Ok(presentation)
                }
                "qrcode.poll" => {
                    let qr = login.qr.as_mut().ok_or("Start QR first")?;
                    login.draft.qrcode_poll(qr).await.map(|r| {
                        done = r["status"] == "done";
                        r
                    })
                }
                _ => return Err("Unknown login action".into()),
            }
            .map_err(|e| e.to_string())?;
            if done {
                let login = sessions.remove(&key).unwrap();
                login.draft.commit().await.map_err(|e| e.to_string())?;
            }
            Ok::<_, String>(json!({"done":done,"result":result}))
        })
        .await
        .map_err(|e| e.to_string())?;
        if result.is_err() || result.as_ref().is_ok_and(|v| v["done"] == true) {
            Self::cancel_login(this.clone(), id).await?;
        }
        if result.as_ref().is_ok_and(|v| v["done"] == true) {
            Self::changed(&this).await?;
        }
        result
    }
    /// Cancel in-flight auth immediately, then remove the temporary instance.
    pub async fn cancel_login(this: Instance<Self>, id: String) -> Result<(), String> {
        let sessions = this.with(|s| {
            if let Some(cancel) = s.cancellations.lock().unwrap().remove(&id) {
                cancel.cancel();
            }
            s.logins.clone()
        });
        background(async move {
            sessions.lock().await.remove(&id);
        })
        .await
        .map_err(|e| e.to_string())
    }
    /// Invoke a selected instance capability; return schema warnings alongside its JSON value.
    pub async fn invoke(
        this: Instance<Self>,
        source: String,
        path: String,
        args: Vec<JsonValue>,
        context: JsonValue,
    ) -> Result<JsonValue, String> {
        let host = Self::host(&this);
        background(async move {
            let value = host
                .source(&source)?
                .invoke(&path, args, context, None)
                .await?;
            let warnings: Vec<_> = haiku_plugins::contract::inferred_schema(&path)
                .and_then(|schema| haiku_plugins::contract::validate_schema(&schema, &value).err())
                .map(|e| vec![e.to_string()])
                .unwrap_or_default();
            Ok::<_, haiku_plugins::PluginError>(json!({"value":value,"warnings":warnings}))
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
    }
    /// Run the package's offline contract fixtures without contacting the upstream service.
    pub async fn test_contract(this: Instance<Self>, plugin: String) -> Result<JsonValue, String> {
        let host = Self::host(&this);
        background(async move {
            let package = host.package(&plugin)?;
            let report = haiku_plugins::contract::run_contract(
                &package.path,
                haiku_plugins::contract::ContractMode::Replay,
            )
            .await?;
            Ok::<_, haiku_plugins::PluginError>(serde_json::to_value(report)?)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
    }
    /// Read instance KV data. Credentials are represented only by redacted connection fields.
    pub async fn storage(this: Instance<Self>, source: String) -> Result<JsonValue, String> {
        let host = Self::host(&this);
        background(async move { host.debug_storage(&source).await })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
    }
    /// Clear the selected instance's KV store through its own runtime.
    pub async fn clear_storage(this: Instance<Self>, source: String) -> Result<(), String> {
        let host = Self::host(&this);
        background(async move { host.clear_storage(&source).await })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
    }
}

/// Normalize a typed import location: trim spaces/quotes, expand `~`, and name the
/// missing path instead of surfacing a bare "os error 2". HTTPS URLs pass through.
fn local_location(input: &str) -> Result<String, String> {
    let value = input.trim().trim_matches(|c| c == '"' || c == '\'');
    if value.is_empty() {
        return Err("请填写插件目录、ZIP 或 .ts 文件路径".into());
    }
    if value.starts_with("https://") {
        return Ok(value.into());
    }
    let path = match value.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME")
            .map(|home| std::path::PathBuf::from(home).join(rest))
            .ok_or("无法展开 ~：未设置 HOME")?,
        None => std::path::PathBuf::from(value),
    };
    if !path.exists() {
        return Err(format!("找不到路径：{}", path.display()));
    }
    Ok(path.to_string_lossy().into_owned())
}
