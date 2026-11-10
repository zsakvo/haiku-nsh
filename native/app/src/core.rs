use futures::{
    FutureExt,
    future::{Either, select},
};
use haiku_model::{Error, ErrorCode};
use haiku_playback::{
    Command, Controller, LocalResolver, PlayerState, RealAudio, Status, SystemClock,
};
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

/// Engine-thread adapter; its position/buffer signals are for Rust components only.
pub struct CoreBridge {
    pub(crate) controller: Controller,
    owner: Owner,
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
        Rc::new(Self {
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
        self.owner.dispose();
        self.controller.shutdown();
        crate::platform::clear_media();
    }
}

/// The application core. `open` restores local playback as paused, without opening audio.
#[class]
pub struct Core {
    plugins: Arc<haiku_plugins::Host>,
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
        let (controller, library, settings, plugins) = background(async {
            let root = haiku_store::data_root();
            std::fs::create_dir_all(&root).map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
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
            let controller = Controller::start(
                audio,
                Arc::new(LocalResolver),
                Arc::new(SystemClock::default()),
                events,
                Some(root.join("session.json")),
            )?;
            let plugins = haiku_plugins::Host::open(&root)
                .await
                .map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
            Ok::<_, Error>((controller, library, settings, plugins))
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
        let bridge = CoreBridge::new(controller, listeners.clone(), settings.get());
        crate::presentation::initialize(bridge.clone(), library.clone());
        bridge
            .owner
            .run(|| crate::platform::initialize_media(bridge.clone()));
        let settings = crate::settings::SettingsService::new(settings, bridge.clone());
        let library = crate::library::LibraryService::new(library, bridge.clone());
        let core = Instance::new(Self {
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
    /// Durable settings, effects and cache preferences; TSX is a subscribed mirror.
    #[getter]
    pub fn settings(&self) -> Instance<crate::settings::SettingsService> {
        self.settings.clone()
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
    #[tokio::test]
    async fn rust_component_clock_reads_progress_and_freezes_when_paused() {
        let audio = Arc::new(Audio::default());
        let (_events, rx) = tokio::sync::mpsc::unbounded_channel();
        let controller = Controller::start(
            audio.clone(),
            Arc::new(LocalResolver),
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
            let core = Core::open().await.unwrap();
            core.with(|value| {
                let packages = value.plugins.packages().unwrap();
                assert!(packages.iter().any(|p| p.descriptor.id == "subsonic.music"
                    && p.descriptor.builtin
                    && p.descriptor.kind == "library"));
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
