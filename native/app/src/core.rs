use futures::{
    FutureExt,
    future::{Either, select},
};
use haiku_model::{Error, ErrorCode};
use haiku_playback::{Command, Controller, PlayerState, RealAudio, Status, SystemClock};
use nsh_core::prelude::*;
use nsh_core::{
    JsonValue,
    element::{Trace, Tracer},
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc, time::Duration};

pub(crate) fn js_error(error: Error) -> JsError {
    JsError::Error(error.message)
        .with("code", serde_json::to_value(error.code).unwrap_or_default())
        .with("retriable", error.retriable)
        .with("sourceId", serde_json::json!(error.source_id))
        .with("detail", serde_json::json!(error.detail))
}
fn json(state: &PlayerState) -> JsonValue {
    serde_json::to_value(state).expect("finite core state")
}
pub(crate) fn decode<T: serde::de::DeserializeOwned>(value: JsonValue) -> Result<T, JsError> {
    serde_json::from_value(value)
        .map_err(|e| js_error(Error::new(ErrorCode::InvalidArgument, e.to_string())))
}
fn refresh_progress(core: &Controller, position: Signal<f64>, buffered: Signal<f64>) {
    let progress = core.progress();
    position.set(progress.position);
    buffered.set(progress.buffered);
}

#[derive(Clone, Default)]
struct Listeners(Rc<RefCell<BTreeMap<u64, Callback<JsonValue>>>>);
// SAFETY: report every callback held in the shared listener registry.
unsafe impl Trace for Listeners {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        self.0.as_ref().trace(tracer);
    }
}

#[derive(Clone)]
struct DataRoot {
    path: std::path::PathBuf,
    credentials: Arc<dyn haiku_services::account::Credentials>,
    sync_secrets: Arc<dyn haiku_services::sync::Secrets>,
    // A fixture directory lives as long as its Core/bridge or engine selection.
    _temporary: Option<Arc<tempfile::TempDir>>,
}
impl Core {
    async fn select_root(root: DataRoot) -> Result<Instance<Self>, JsError> {
        if let Some(existing) = global::<Option<DataRoot>>().get() {
            if existing.path != root.path {
                return Err(JsError::Error(
                    "An engine already owns a different Core data root".into(),
                ));
            }
        } else {
            global::<Option<DataRoot>>().set(Some(root));
        }
        Self::open().await
    }
}

/// Engine-thread adapter; its position/buffer signals are for Rust components only.
pub struct CoreBridge {
    pub(crate) lyrics: Arc<haiku_lyrics::service::Service>,
    pub(crate) source_host: Arc<haiku_plugins::Host>,
    pub(crate) artwork_root: std::path::PathBuf,
    pub(crate) controller: Controller,
    pub(crate) collections: haiku_store::collections::Collections,
    pub(crate) history: haiku_store::history::History,
    pub(crate) stats: haiku_store::stats::Stats,
    pub(crate) logs: haiku_store::logs::Logs,
    pub(crate) collections_revision: Signal<u64>,
    owner: Owner,
    _root: DataRoot,
    _exit: crate::exit::ExitGuard,
    pub(crate) settings: Signal<haiku_store::settings::Settings>,
    pub position: Signal<f64>,
    pub buffered: Signal<f64>,
    pub state: Signal<PlayerState>,
}
impl CoreBridge {
    fn new(
        controller: Controller,
        listeners: Listeners,
        settings: haiku_store::settings::Settings,
        collections: haiku_store::collections::Collections,
        history: haiku_store::history::History,
        stats: haiku_store::stats::Stats,
        logs: haiku_store::logs::Logs,
        root: DataRoot,
        source_host: Arc<haiku_plugins::Host>,
    ) -> Rc<Self> {
        let owner = Owner::detached();
        let (position, buffered, state) = owner.run(|| {
            let position = signal(controller.progress().position);
            let buffered = signal(controller.progress().buffered);
            let state = signal(controller.state());
            let mut states = controller.subscribe();
            let core = controller.clone();
            // One pump owns both event delivery and the display-frame clock.
            spawn(async move {
                loop {
                    let snapshot = states.borrow_and_update().clone();
                    state.set(snapshot.clone());
                    let value = json(&snapshot);
                    let callbacks: Vec<_> = listeners.0.borrow().values().cloned().collect();
                    for listener in callbacks {
                        listener.post(value.clone());
                    }
                    // Paused/loading/hidden surfaces don't request animation frames.
                    loop {
                        let visible_playback = snapshot.status == Status::Playing
                            && !snapshot.buffering
                            && lifecycle() != nsh_core::platform::Lifecycle::Background;
                        let changed = states.changed().boxed_local();
                        let frame = async {
                            if visible_playback {
                                next_frame().await;
                            } else {
                                timer(Duration::from_millis(200)).await;
                            }
                        }
                        .boxed_local();
                        match select(changed, frame).await {
                            Either::Left((result, _)) => {
                                if result.is_err() {
                                    return;
                                }
                                refresh_progress(&core, position, buffered);
                                break;
                            }
                            Either::Right(((), _)) => {
                                if visible_playback {
                                    refresh_progress(&core, position, buffered);
                                }
                            }
                        }
                    }
                }
            });
            (position, buffered, state)
        });
        let settings = owner.run(|| signal(settings));
        let collections_revision = owner.run(|| signal(collections.revision()));
        let exit_store = collections.clone();
        let exit_history = history.clone();
        let exit_stats = stats.clone();
        let exit = crate::exit::register(move || {
            exit_stats.flush()?;
            exit_history.flush()?;
            exit_store.flush()
        });
        crate::platform::install_exit_hook();
        Rc::new(Self {
            lyrics: Arc::new(haiku_lyrics::service::Service::new(
                root.path.join("lyrics"),
                Default::default(),
            )),
            source_host,
            artwork_root: root.path.join("artwork"),
            _root: root,
            _exit: exit,
            collections,
            history,
            stats,
            logs,
            collections_revision,
            settings,
            controller,
            owner,
            position,
            buffered,
            state,
        })
    }
}
impl Drop for CoreBridge {
    fn drop(&mut self) {
        self.lyrics.clear(None);
        self.owner.dispose();
        if let Err(e) = self.collections.flush() {
            log::error!("Collections exit flush: {e}");
        }
        let _ = self.history.flush();
        let _ = self.stats.flush();
        self.controller.shutdown();
        crate::platform::clear_media();
    }
}

/// The application core. `open` restores local playback as paused, without opening audio.
#[class]
pub struct Core {
    shortcuts: Instance<crate::shortcuts::ShortcutsService>,
    backup: Instance<crate::backup::BackupService>,
    sync: Instance<crate::sync::SyncService>,
    account: Instance<crate::account::AccountService>,
    downloads: Instance<crate::downloads::DownloadService>,
    scrape: Instance<crate::scrape::ScrapeService>,
    lyrics: Instance<crate::lyrics::LyricsService>,
    activity: Instance<crate::activity::ActivityService>,
    collections: Instance<crate::collections::CollectionsService>,
    plugins: Instance<crate::plugins::PluginService>,
    player: Instance<PlayerService>,
    settings: Instance<crate::settings::SettingsService>,
    library: Instance<crate::library::LibraryService>,
}
#[class]
impl Core {
    /// Initialize the data root and the unique playback service of this engine.
    pub async fn open() -> Result<Instance<Self>, JsError> {
        let slot = global::<Option<Instance<Core>>>();
        if let Some(core) = slot.get() {
            return Ok(core);
        }
        let selected = global::<Option<DataRoot>>()
            .get()
            .unwrap_or_else(|| DataRoot {
                path: haiku_store::data_root(),
                credentials: if cfg!(test) || std::env::var("HAIKU_NO_KEYCHAIN").as_deref() == Ok("1") {
                    Arc::new(haiku_services::account::MemoryCredentials::default())
                } else {
                    Arc::new(haiku_services::account::SystemCredentials)
                },
                sync_secrets: if cfg!(test) || std::env::var("HAIKU_NO_KEYCHAIN").as_deref() == Ok("1") {
                    Arc::new(haiku_services::sync::MemorySecrets::default())
                } else {
                    Arc::new(haiku_services::sync::SystemSecrets)
                },
                _temporary: None,
            });
        global::<Option<DataRoot>>().set(Some(selected.clone()));
        let root = selected.path.clone();
        let (controller, library, settings, plugins, collections, history, stats, logs, downloads) =
            background(async move {
                std::fs::create_dir_all(&root)
                    .map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
                let settings = Arc::new(
                    haiku_store::settings::SettingsStore::open(&root)
                        .map_err(|e| Error::new(ErrorCode::Io, e))?,
                );
                haiku_audio::media_cache::set_root(root.join("media-cache").display().to_string())
                    .map_err(|e| Error::new(ErrorCode::Io, e))?;
                crate::settings::apply_cache(&settings.get());
                let (audio, events) = RealAudio::configured(&root, settings.get());
                let library = haiku_library::service::Library::open(&root)
                    .map_err(|e| Error::new(ErrorCode::Io, e))?;
                let plugins = haiku_plugins::Host::open(&root)
                    .await
                    .map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
                let downloads = crate::download_service::Service::open(&root, plugins.clone(), settings.clone())
                    .map_err(|e| Error::new(ErrorCode::Io, e))?;
                let controller = Controller::start(
                    audio,
                    Arc::new(crate::sources::SourceResolver(plugins.clone())),
                    Arc::new(SystemClock::default()),
                    events,
                    Some(root.join("session.json")),
                )?;
                let collections = haiku_store::collections::Collections::open(&root)
                    .map_err(|e| Error::new(ErrorCode::Io, e))?;
                let history = haiku_store::history::History::open(&root)
                    .map_err(|e| Error::new(ErrorCode::Io, e))?;
                let stats = haiku_store::stats::Stats::open(&root)
                    .map_err(|e| Error::new(ErrorCode::Io, e))?;
                let logs = haiku_store::logs::Logs::open(&root)
                    .map_err(|e| Error::new(ErrorCode::Io, e))?;
                for warning in [
                    settings.recovery_warning(),
                    history.recovery(),
                    stats.recovery(),
                ]
                .into_iter()
                .flatten()
                {
                    let _ = logs.add(
                        "app",
                        "warn",
                        "storage",
                        None,
                        "数据文件损坏，已备份原文件",
                        Some(&warning),
                    );
                    logs.notify("warning", "数据文件损坏，已备份原文件", Some(&warning));
                }
                let (h, s, l) = (history.clone(), stats.clone(), logs.clone());
                let seen = std::sync::Mutex::new((None::<(String, u64)>, None::<String>));
                let fixture_second = std::sync::atomic::AtomicU64::new(u64::MAX);
                controller.observe(move |state, progress, mono| {
                    let (epoch, offset) = haiku_store::stats::wall_time();
                    let playing =
                        state.status == Status::Playing && !state.buffering && !progress.buffering;
                    let mut seen = seen.lock().unwrap();
                    if playing {
                        if let Some(track) = &state.current {
                            if track.source.starts_with("network.fixture") {
                                let second = progress.position as u64;
                                if fixture_second.swap(second, std::sync::atomic::Ordering::Relaxed) != second {
                                    println!("M4-01 loopback audio: track={} generation={} position={:.2}s buffered={:.3} playing=true",
                                        track.id,state.generation,progress.position,progress.buffered);
                                }
                            }
                            let key = (track.entity_ref().encode(), state.generation);
                            if seen.0.as_ref() != Some(&key) {
                                h.record_track(track, epoch);
                                seen.0 = Some(key);
                            }
                        }
                    }
                    if let Some(error) = &state.error {
                        if seen.1.as_ref() != Some(&error.message) {
                            let _ = l.add("app", "error", "player", None, &error.message, None);
                            l.notify("error", "播放失败", Some(&error.message));
                            seen.1 = Some(error.message.clone());
                        }
                    } else {
                        seen.1 = None;
                    }
                    s.sample(
                        state.current.as_ref(),
                        state.generation,
                        state.timeline_revision,
                        playing,
                        progress.position,
                        mono,
                        epoch,
                        offset,
                    );
                });
                Ok::<_, Error>((
                    controller,
                    library,
                    settings,
                    plugins,
                    collections,
                    history,
                    stats,
                    logs,
                    downloads,
                ))
            })
            .await
            .map_err(|e| JsError::Error(e.to_string()))?
            .map_err(js_error)?;
        // Check again: concurrent open calls may have initialized while this one awaited.
        if let Some(core) = slot.get() {
            controller.shutdown();
            return Ok(core);
        }
        let listeners = Listeners::default();
        let bridge = CoreBridge::new(
            controller,
            listeners.clone(),
            settings.get(),
            collections,
            history,
            stats,
            logs,
            selected,
            plugins.clone(),
        );
        crate::presentation::initialize(bridge.clone(), library.clone());
        let scrape = {
            let (root, library, host, settings) = (
                bridge._root.path.clone(), library.clone(), plugins.clone(), settings.clone(),
            );
            let service = blocking(move || crate::scrape_service::Service::open(&root, library, host, settings))
                .await.map_err(|e| JsError::Error(e.to_string()))?.map_err(JsError::Error)?;
            crate::scrape::ScrapeService::new(service)
        };
        downloads.watch_collections(bridge.collections.clone());
        let account = {
            let root = bridge._root.path.clone();
            let credentials = bridge._root.credentials.clone();
            let service = blocking(move || haiku_services::account::Service::open(&root, credentials))
                .await.map_err(|e| JsError::Error(e.to_string()))?
                .map_err(|e| JsError::Error(e.to_string()))?;
            crate::account::AccountService::new(service, bridge.stats.clone(), bridge._root._temporary.is_some())
        };
        let sync = {
            let root = bridge._root.path.clone();
            let secrets = bridge._root.sync_secrets.clone();
            let collections = bridge.collections.clone();
            let store = settings.clone();
            let isolated = bridge._root._temporary.is_some();
            let mut options = haiku_services::sync::Options::default();
            if isolated || cfg!(test) || std::env::var("HAIKU_NO_KEYCHAIN").as_deref() == Ok("1") {
                options.github_base = "http://127.0.0.1:9".into();
            }
            let service = blocking(move || haiku_services::sync::Service::open(
                &root, collections, store, secrets, options,
            )).await.map_err(|e| JsError::Error(e.to_string()))?.map_err(JsError::Error)?;
            crate::sync::SyncService::new(service, isolated)
        };
        let backup_runtime = crate::backup::Runtime {
            root: bridge._root.path.clone(), secrets: bridge._root.sync_secrets.clone(),
            controller: bridge.controller.clone(), host: plugins.clone(), settings: settings.clone(),
            collections: bridge.collections.clone(), history: bridge.history.clone(), stats: bridge.stats.clone(),
            sync: sync.with(|s| s.authority()), downloads: downloads.clone(),
        };
        let shortcut_store=settings.clone();
        let native_shortcuts=bridge._root._temporary.is_none();
        let settings = crate::settings::SettingsService::new(settings, bridge.clone(), downloads.clone());
        bridge.owner.run(|| crate::platform::initialize_media(bridge.clone(), settings.clone(), native_shortcuts));
        let shortcuts=crate::shortcuts::ShortcutsService::new(shortcut_store,settings.clone(),bridge.clone(),native_shortcuts);
        let downloads = crate::downloads::DownloadService::new(downloads, settings.clone());
        let collections =
            crate::collections::CollectionsService::new(bridge.clone(), library.clone());
        let library = crate::library::LibraryService::new(library, bridge.clone());
        let activity = crate::activity::ActivityService::new(bridge.clone());
        let lyrics = bridge
            .owner
            .run(|| crate::lyrics::LyricsService::new(bridge.clone()));
        let plugin_logs = bridge.logs.clone();
        let plugins = crate::plugins::PluginService::new(
            plugins, plugin_logs, bridge.collections.clone(), bridge._root.path.join("cache/home"),
        );
        let backup = crate::backup::BackupService::new(backup_runtime, settings.clone(), plugins.clone());
        let core = Instance::new(Self {
            shortcuts,
            backup,
            sync,
            account,
            downloads,
            scrape,
            lyrics,
            activity,
            collections,
            plugins,
            settings,
            library,
            player: Instance::new(PlayerService {
                bridge,
                listeners,
                next_listener: 0,
            }),
        });
        slot.set(Some(core.clone()));
        Ok(core)
    }
    /// Open an explicit data directory owned by this engine; no process environment is mutated.
    pub async fn open_in(dir: String) -> Result<Instance<Self>, JsError> {
        let path = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&path).map_err(|e| JsError::Error(e.to_string()))?;
        let path = path
            .canonicalize()
            .map_err(|e| JsError::Error(e.to_string()))?;
        Self::select_root(DataRoot {
            path,
            credentials: Arc::new(haiku_services::account::MemoryCredentials::default()),
            sync_secrets: Arc::new(haiku_services::sync::MemorySecrets::default()),
            _temporary: None,
        })
        .await
    }
    /// Create a temporary Core retained through close/open. Automatic lyrics lookup defaults off;
    /// offline fixtures may opt in with explicit provider configuration.
    pub async fn open_isolated() -> Result<Instance<Self>, JsError> {
        let base = std::env::temp_dir().join("haiku-m1-03-fixtures");
        std::fs::create_dir_all(&base).map_err(|e| JsError::Error(e.to_string()))?;
        let dir = tempfile::Builder::new()
            .prefix("core-")
            .tempdir_in(base)
            .map_err(|e| JsError::Error(e.to_string()))?;
        let path = dir
            .path()
            .canonicalize()
            .map_err(|e| JsError::Error(e.to_string()))?;
        haiku_store::json_file::write(
            &path.join("settings.json"),
            &serde_json::json!({"lyricsAutoSearch":false}),
        )
        .map_err(|e| JsError::Error(e.to_string()))?;
        Self::select_root(DataRoot {
            path,
            credentials: Arc::new(haiku_services::account::MemoryCredentials::default()),
            sync_secrets: Arc::new(haiku_services::sync::MemorySecrets::default()),
            _temporary: Some(Arc::new(dir)),
        })
        .await
    }
    /// Actual engine-owned root, for explicit persistence tests that reopen another engine.
    pub fn data_dir(&self) -> String {
        self.player
            .with(|p| p.bridge._root.path.display().to_string())
    }
    /// Likes, user playlists and offline favorite mirrors, with automatic persistence.
    #[getter]
    pub fn collections(&self) -> Instance<crate::collections::CollectionsService> {
        self.collections.clone()
    }
    /// Haiku account sessions and informational membership, backed by system credentials.
    #[getter]
    pub fn account(&self) -> Instance<crate::account::AccountService> {
        self.account.clone()
    }
    /// Gist synchronization of the Loro collections document; credentials stay in Keychain.
    #[getter]
    pub fn sync(&self) -> Instance<crate::sync::SyncService> {
        self.sync.clone()
    }
    /// Validated .haikubak exports, previews, safety backups and hot restoration.
    #[getter]
    pub fn backup(&self) -> Instance<crate::backup::BackupService> {
        self.backup.clone()
    }
    /// Offline download queue, resumable transfers and durable records.
    #[getter]
    pub fn downloads(&self) -> Instance<crate::downloads::DownloadService> {
        self.downloads.clone()
    }
    /// Current lyrics, provider search, previews and durable manual selections.
    #[getter]
    pub fn lyrics(&self) -> Instance<crate::lyrics::LyricsService> {
        self.lyrics.clone()
    }
    /// Local playback, playlist browsing and search history.
    #[getter]
    pub fn history(&self) -> Instance<crate::activity::ActivityService> {
        self.activity.clone()
    }
    /// Listening segments and aggregates, including account-fenced remote statistics.
    #[getter]
    pub fn stats(&self) -> Instance<crate::activity::ActivityService> {
        self.activity.clone()
    }
    /// Redacted application/plugin diagnostics, filtering and export.
    #[getter]
    pub fn logs(&self) -> Instance<crate::activity::ActivityService> {
        self.activity.clone()
    }
    /// Background results and recoverable errors.
    #[getter]
    pub fn notifications(&self) -> Instance<crate::activity::ActivityService> {
        self.activity.clone()
    }
    /// Flush pending collections and playback, releasing the cached Core for a later open.
    pub async fn close(this: Instance<Self>) -> Result<(), String> {
        crate::sync::SyncService::close(this.with(|s| s.sync.clone())).await?;
        this.with(|s| s.account.with(|a| a.stop()));
        crate::downloads::DownloadService::close(this.with(|s| s.downloads.clone())).await?;
        crate::scrape::ScrapeService::close(this.with(|s| s.scrape.clone())).await?;
        let bridge = this.with(|s| s.player.with(|p| p.bridge.clone()));
        let store = bridge.collections.clone();
        let history = bridge.history.clone();
        let stats = bridge.stats.clone();
        blocking(move || {
            stats.flush()?;
            history.flush()?;
            store.flush()
        })
        .await
        .map_err(|e| e.to_string())??;
        bridge
            .controller
            .execute(Command::Flush)
            .await
            .map_err(|e| e.message)?;
        bridge.controller.shutdown();
        global::<Option<Instance<Core>>>().set(None);
        Ok(())
    }
    /// Atomic global registration and durable, reference-compatible app bindings.
    #[getter]
    pub fn shortcuts(&self) -> Instance<crate::shortcuts::ShortcutsService> { self.shortcuts.clone() }
    /// Durable settings, effects and cache preferences; TSX is a subscribed mirror.
    #[getter]
    pub fn settings(&self) -> Instance<crate::settings::SettingsService> {
        self.settings.clone()
    }
    /// Cancellable local metadata jobs, durable review and reversible sparse overrides.
    #[getter]
    pub fn scrape(&self) -> Instance<crate::scrape::ScrapeService> {
        self.scrape.clone()
    }
    /// Installed plugin packages and developer tooling, shared with the source settings service.
    #[getter]
    pub fn plugins(&self) -> Instance<crate::plugins::PluginService> {
        self.plugins.clone()
    }
    /// Committed source registry and temporary authentication workflows.
    #[getter]
    pub fn sources(&self) -> Instance<crate::plugins::PluginService> {
        self.plugins.clone()
    }
    /// The local playback/queue service; position stays in Rust.
    #[getter]
    pub fn player(&self) -> Instance<PlayerService> {
        self.player.clone()
    }
    /// Persistent local roots, scanning, effective metadata and Rust-backed queries.
    #[getter]
    pub fn library(&self) -> Instance<crate::library::LibraryService> {
        self.library.clone()
    }

    /// Install the existing native window lifecycle policy.
    pub fn initialize() {
        crate::platform::initialize();
    }

    /// Software identity shared by Settings and the application About menu.
    pub fn about_info() -> JsonValue {
        let manifest: JsonValue=serde_json::from_str(include_str!("../../../nsh.json")).unwrap();
        serde_json::json!({"version":manifest["version"],"framework":"0.4.0",
            "license":"应用许可证尚未声明。",
            "notices":concat!("Iconoir icons — MIT\n",include_str!("../../../assets/icons/LICENSE"),"\n\nunDraw illustrations\n",include_str!("../../../assets/illustrations/LICENSE.txt"),"\n\nNative Shell 0.4.0 — Apache-2.0\nRust 依赖的完整发行声明将在打包阶段随应用提供。")})
    }
    /// Replace semantic tokens shared by TSX and Rust.
    pub async fn apply_theme(json: String) -> Result<(), String> {
        set_tokens(nsh_base::Theme::from_json(&json).map_err(|e| e.to_string())?);
        crate::presentation::reapply_theme();
        Ok(())
    }
}

/// Low-frequency playback commands and state subscription. All business state belongs to Rust.
#[class]
pub struct PlayerService {
    bridge: Rc<CoreBridge>,
    listeners: Listeners,
    next_listener: u64,
}
impl PlayerService {
    async fn send(this: Instance<Self>, command: Command) -> Result<(), JsError> {
        let controller = this.with(|p| p.bridge.controller.clone());
        controller.execute(command).await.map_err(js_error)
    }
}
#[class]
impl PlayerService {
    /// Start a continuous queue for one source; omitted channel selects private FM.
    pub async fn start_radio(
        this: Instance<Self>,
        source: String,
        channel: Option<String>,
    ) -> Result<(), JsError> {
        let host = this.with(|p| p.bridge.source_host.clone());
        let provider = host
            .source(&source)
            .map_err(|e| JsError::Error(e.to_string()))?;
        if !provider
            .descriptor()
            .capabilities
            .iter()
            .any(|c| c == "radio.next")
        {
            return Err(JsError::Error("来源不支持电台".into()));
        }
        let radio = Arc::new(crate::sources::SourceRadio {
            host,
            source: source.clone(),
            channel: channel.clone(),
        });
        Self::send(this, Command::StartRadio(source, channel, radio)).await
    }
    /// Replace the queue with SDK tracks (plus source/localPath), and play at index.
    pub async fn play(
        this: Instance<Self>,
        tracks: Vec<JsonValue>,
        index: i32,
    ) -> Result<(), JsError> {
        Self::send(
            this,
            Command::Play(decode(JsonValue::Array(tracks))?, index),
        )
        .await
    }
    /// Pause if playing/loading; otherwise resume.
    pub async fn toggle(this: Instance<Self>) -> Result<(), JsError> {
        Self::send(this, Command::Toggle).await
    }
    /// Publish paused intent, cancel loading, and pause the audio backend.
    pub async fn pause(this: Instance<Self>) -> Result<(), JsError> {
        Self::send(this, Command::Pause).await
    }
    /// Resume the existing stream, or open a restored session at its saved position.
    pub async fn resume(this: Instance<Self>) -> Result<(), JsError> {
        Self::send(this, Command::Resume).await
    }
    /// Stop audio while retaining the queue and selection.
    pub async fn stop(this: Instance<Self>) -> Result<(), JsError> {
        Self::send(this, Command::Stop).await
    }
    /// Manually advance; repeat-one behaves as repeat-all.
    pub async fn next(this: Instance<Self>) -> Result<(), JsError> {
        Self::send(this, Command::Next).await
    }
    /// Return through actual history; sequential queue head seeks to zero.
    pub async fn previous(this: Instance<Self>) -> Result<(), JsError> {
        Self::send(this, Command::Previous).await
    }
    /// Seek in seconds, clamped to the current duration.
    pub async fn seek(this: Instance<Self>, seconds: f64) -> Result<(), JsError> {
        Self::send(this, Command::Seek(seconds)).await
    }
    /// Set finite volume, clamped to 0..1.
    pub async fn set_volume(this: Instance<Self>, volume: f64) -> Result<(), JsError> {
        Self::send(this, Command::SetVolume(volume)).await
    }
    /// Set sequential, repeatAll, repeatOne, or shuffle.
    pub async fn set_mode(this: Instance<Self>, mode: String) -> Result<(), JsError> {
        Self::send(this, Command::SetMode(decode(serde_json::json!(mode))?)).await
    }
    /// Append tracks without opening audio.
    pub async fn append(this: Instance<Self>, tracks: Vec<JsonValue>) -> Result<(), JsError> {
        Self::send(this, Command::Append(decode(JsonValue::Array(tracks))?)).await
    }
    /// Place a track immediately after the current entry, including in shuffle.
    pub async fn play_next(this: Instance<Self>, track: JsonValue) -> Result<(), JsError> {
        Self::send(this, Command::PlayNext(decode(track)?)).await
    }
    /// Replace the unplayed list suffix, retaining shuffle history.
    pub async fn replace_after_current(
        this: Instance<Self>,
        tracks: Vec<JsonValue>,
    ) -> Result<(), JsError> {
        Self::send(
            this,
            Command::ReplaceAfterCurrent(decode(JsonValue::Array(tracks))?),
        )
        .await
    }
    /// Remove a queue position; removing the current entry starts its successor.
    pub async fn remove(this: Instance<Self>, index: i32) -> Result<(), JsError> {
        Self::send(this, Command::Remove(index)).await
    }
    /// Clear the queue and stop audio.
    pub async fn clear(this: Instance<Self>) -> Result<(), JsError> {
        Self::send(this, Command::Clear).await
    }
    /// Move a queue entry while retaining current and shuffle-history identities.
    pub async fn move_item(this: Instance<Self>, from: i32, to: i32) -> Result<(), JsError> {
        Self::send(this, Command::Move(from, to)).await
    }
    /// Select and play a queue position.
    pub async fn jump(this: Instance<Self>, index: i32) -> Result<(), JsError> {
        Self::send(this, Command::Jump(index)).await
    }
    /// Pause after 15/30/45/60/90 minutes, using the monotonic clock.
    pub async fn set_sleep(this: Instance<Self>, minutes: u32) -> Result<(), JsError> {
        Self::send(this, Command::SetSleep(minutes)).await
    }
    /// Extend the deadline by fifteen minutes (or start fifteen minutes).
    pub async fn extend_sleep(this: Instance<Self>) -> Result<(), JsError> {
        Self::send(this, Command::ExtendSleep).await
    }
    /// Stop after the current track, taking priority over all repeat modes.
    pub async fn sleep_after_track(this: Instance<Self>) -> Result<(), JsError> {
        Self::send(this, Command::SleepAfterTrack).await
    }
    /// Cancel sleep and restore normal preloading.
    pub async fn cancel_sleep(this: Instance<Self>) -> Result<(), JsError> {
        Self::send(this, Command::CancelSleep).await
    }
    /// Flush the current session atomically.
    pub async fn flush(this: Instance<Self>) -> Result<(), JsError> {
        Self::send(this, Command::Flush).await
    }
    /// Current backend preferences, including the effects used by every new audio stream.
    pub fn configuration(&self) -> JsonValue {
        serde_json::to_value(self.bridge.controller.configuration()).unwrap()
    }
    /// Current low-frequency snapshot; contains no position/buffer fraction.
    pub fn snapshot(&self) -> JsonValue {
        json(&self.bridge.controller.state())
    }
    /// Hear the initial snapshot and subsequent low-frequency changes. Stop unregisters once.
    pub fn on_state(&mut self, #[args(state)] listener: Callback<JsonValue>) -> Stop {
        self.next_listener += 1;
        let id = self.next_listener;
        self.listeners.0.borrow_mut().insert(id, listener);
        let listeners = self.listeners.clone();
        let initial = json(&self.bridge.controller.state());
        let listeners_for_initial = listeners.clone();
        // A deferred callback itself schedules JS for its next turn; post is safe here.
        if let Some(listener) = listeners_for_initial.0.borrow().get(&id) {
            listener.post(initial);
        }
        Stop::new(move || {
            listeners.0.borrow_mut().remove(&id);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::BoxFuture;
    use haiku_model::Track;
    use haiku_playback::{AudioBackend, Progress};
    use std::path::PathBuf;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Audio(Mutex<Progress>);
    impl AudioBackend for Audio {
        fn open(&self, _: PathBuf, generation: u64) -> BoxFuture<'_, Result<(), Error>> {
            self.0.lock().unwrap().generation = generation;
            Box::pin(async { Ok(()) })
        }
        fn prepare(&self, _: PathBuf, _: u64) -> BoxFuture<'_, Result<(), Error>> {
            Box::pin(async { Ok(()) })
        }
        fn clear_next(&self) {}
        fn pause(&self) {}
        fn resume(&self) {}
        fn stop(&self) {}
        fn seek(&self, seconds: f64) {
            self.0.lock().unwrap().position = seconds;
        }
        fn set_volume(&self, _: f64) {}
        fn progress(&self) -> Progress {
            *self.0.lock().unwrap()
        }
    }
    #[component]
    fn IsolatedFixture() -> impl IntoElement {
        let report = signal(String::new());
        spawn(async move {
            let core = Core::open_isolated().await.unwrap();
            let (root, count) = core.with(|c| {
                c.collections.with(|collections| {
                    collections.create("Only mine".into()).unwrap();
                });
                let count = c
                    .collections
                    .with(|collections| collections.list().as_array().unwrap().len());
                // The Core registers its exit barrier before native media is available
                // (Headless never installs a system media session).
                crate::exit::flush_all().unwrap();
                let restored = haiku_store::collections::Collections::open(std::path::Path::new(
                    &c.data_dir(),
                ))
                .unwrap();
                assert!(restored.list().iter().any(|p| p.title == "Only mine"));
                (c.data_dir(), count)
            });
            report.set(format!("isolated:{count}:{root}"));
        });
        text(move || report.get())
    }
    #[test]
    fn engines_own_independent_temporary_data_roots() {
        let mut first =
            nsh_testing::Headless::start(nsh_engine::EngineConfig::new(nsh_engine::Root::of::<
                IsolatedFixture,
            >()))
            .unwrap();
        first
            .wait_until("first independent Core", Duration::from_secs(5), |app| {
                app.texts().iter().any(|s| s.starts_with("isolated:2:"))
            })
            .unwrap();
        let first_root = first
            .screen()
            .texts()
            .into_iter()
            .find(|s| s.starts_with("isolated:2:"))
            .unwrap()
            .to_owned();
        let mut second =
            nsh_testing::Headless::start(nsh_engine::EngineConfig::new(nsh_engine::Root::of::<
                IsolatedFixture,
            >()))
            .unwrap();
        second
            .wait_until("second independent Core", Duration::from_secs(5), |app| {
                app.texts().iter().any(|s| s.starts_with("isolated:2:"))
            })
            .unwrap();
        let second_root = second
            .screen()
            .texts()
            .into_iter()
            .find(|s| s.starts_with("isolated:2:"))
            .unwrap();
        assert_ne!(first_root, second_root);
    }
    #[tokio::test]
    async fn rust_component_clock_reads_progress_and_freezes_when_paused() {
        let audio = Arc::new(Audio::default());
        let (_events, rx) = tokio::sync::mpsc::unbounded_channel();
        let controller = Controller::start(
            audio.clone(),
            Arc::new(haiku_playback::LocalResolver),
            Arc::new(SystemClock::default()),
            rx,
            None,
        )
        .unwrap();
        // Queue/seek while paused requires neither a valid file nor an audio open.
        controller
            .execute(Command::Append(vec![Track::local("/not-opened")]))
            .await
            .unwrap();
        controller.execute(Command::Seek(12.0)).await.unwrap();
        controller.execute(Command::Pause).await.unwrap();
        let runtime = nsh_core::element::Runtime::default();
        let _entered = runtime.enter();
        Owner::detached().run(|| {
            let position = signal(0.0);
            let buffered = signal(0.0);
            refresh_progress(&controller, position, buffered);
            assert_eq!(position.get(), 12.0);
            audio.0.lock().unwrap().position = 99.0;
            refresh_progress(&controller, position, buffered);
            assert_eq!(position.get(), 12.0);
        });
        controller.shutdown();
    }
}

#[cfg(test)]
mod plugin_host_tests {
    use super::*;
    use nsh_engine::{EngineConfig, Root};
    use nsh_testing::Headless;
    #[component]
    fn Fixture() -> impl IntoElement {
        let installed = signal(false);
        spawn(async move {
            let core = Core::open_isolated().await.unwrap();
            core.with(|value| {
                let packages = crate::plugins::PluginService::host(&value.plugins)
                    .packages()
                    .unwrap();
                for (id, kind) in [
                    ("subsonic.music", "library"),
                    ("feiniu.music", "library"),
                    ("qq.scraper", "scraper"),
                    ("kugou.scraper", "scraper"),
                ] {
                    assert!(packages.iter().any(|p| p.descriptor.id == id
                        && p.descriptor.builtin
                        && p.descriptor.kind == kind));
                }
            });
            installed.set(true);
        });
        text(move || {
            if installed.get() {
                "Builtin subsonic installed"
            } else {
                "Opening"
            }
        })
    }
    #[test]
    fn core_open_installs_readable_builtin_descriptor() {
        let mut app = Headless::start(EngineConfig::new(Root::of::<Fixture>())).unwrap();
        app.wait_for("Builtin subsonic installed").unwrap();
    }
}
