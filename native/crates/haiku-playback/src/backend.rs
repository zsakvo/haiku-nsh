use crate::loudness_cache::LoudnessCache;
use futures::future::BoxFuture;
use haiku_audio::playback::{AudioEffectsOptions, LoudnessOptions, PlaybackEventKind, Player};
use haiku_model::{Error, ErrorCode, Track, events::EventSink};
use haiku_store::settings::Settings;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::mpsc;

pub trait Clock: Send + Sync + 'static {
    fn now_ms(&self) -> u64;
}
pub struct SystemClock(Instant);
impl Default for SystemClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}
impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        self.0.elapsed().as_millis() as u64
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Progress {
    pub generation: u64,
    pub position: f64,
    pub buffered: f64,
    pub buffering: bool,
}
#[derive(Clone, Debug)]
pub enum AudioEventKind {
    Finished,
    TrackChanged,
    Error(Error),
    Buffering(bool),
}
#[derive(Clone, Debug)]
pub struct AudioEvent {
    pub generation: u64,
    pub kind: AudioEventKind,
}

/// Long opens are cancel-safe futures; short commands are called only by the actor.
pub trait AudioBackend: Send + Sync + 'static {
    fn open(&self, path: PathBuf, generation: u64) -> BoxFuture<'_, Result<(), Error>>;
    fn prepare(&self, path: PathBuf, generation: u64) -> BoxFuture<'_, Result<(), Error>>;
    fn open_track(
        &self,
        track: Track,
        path: PathBuf,
        generation: u64,
        prepare: bool,
    ) -> BoxFuture<'_, Result<(), Error>> {
        let _ = track;
        if prepare {
            self.prepare(path, generation)
        } else {
            self.open(path, generation)
        }
    }
    fn configure(&self, _: Settings) {}
    fn configuration(&self) -> Settings {
        Settings::default()
    }
    fn pin_tracks(&self, _: Option<Track>, _: Option<Track>) {}
    fn clear_next(&self);
    fn pause(&self);
    fn resume(&self);
    fn stop(&self);
    fn seek(&self, seconds: f64);
    fn set_volume(&self, volume: f64);
    fn progress(&self) -> Progress;
}
/// M4 can replace this local-only implementation with download/cache/stream resolution.
pub trait TrackResolver: Send + Sync + 'static {
    fn resolve(&self, track: Track) -> BoxFuture<'_, Result<PathBuf, Error>>;
}
pub struct LocalResolver;
impl TrackResolver for LocalResolver {
    fn resolve(&self, track: Track) -> BoxFuture<'_, Result<PathBuf, Error>> {
        Box::pin(async move {
            let path = track.local_path.ok_or_else(|| {
                Error::new(
                    ErrorCode::CapabilityUnsupported,
                    "This task supports localPath only",
                )
            })?;
            let metadata = tokio::fs::metadata(&path)
                .await
                .map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
            if !metadata.is_file() || metadata.len() == 0 {
                return Err(Error::new(
                    ErrorCode::Io,
                    "Local audio must be a nonempty regular file",
                ));
            }
            Ok(path.into())
        })
    }
}

/// The audio device is created lazily: restoring a paused session opens no audio.
pub struct RealAudio {
    player: Mutex<Option<Arc<Player>>>,
    events: mpsc::UnboundedSender<AudioEvent>,
    settings: Mutex<Settings>,
    loudness: Arc<Mutex<LoudnessCache>>,
    current: Mutex<Option<Track>>,
}
impl RealAudio {
    pub fn new() -> (Arc<Self>, mpsc::UnboundedReceiver<AudioEvent>) {
        let (events, rx) = mpsc::unbounded_channel();
        (
            Arc::new(Self {
                player: Mutex::new(None),
                events,
                settings: Mutex::new(Settings::default()),
                loudness: Arc::new(Mutex::new(LoudnessCache::new(None))),
                current: Mutex::new(None),
            }),
            rx,
        )
    }
    pub fn configured(
        root: &std::path::Path,
        settings: Settings,
    ) -> (Arc<Self>, mpsc::UnboundedReceiver<AudioEvent>) {
        let (mut audio, rx) = Self::new();
        let a = Arc::get_mut(&mut audio).unwrap();
        a.settings = Mutex::new(settings);
        a.loudness = Arc::new(Mutex::new(LoudnessCache::new(Some(
            root.join("loudness.json"),
        ))));
        (audio, rx)
    }
    fn options(&self, track: &Track) -> LoudnessOptions {
        self.loudness.lock().unwrap().options(
            track.entity_ref().encode(),
            track.duration.unwrap_or(0.),
            self.settings.lock().unwrap().volume_normalization,
        )
    }
    fn player(&self) -> Result<Arc<Player>, Error> {
        let mut slot = self.player.lock().unwrap();
        if let Some(player) = &*slot {
            return Ok(player.clone());
        }
        let player = Arc::new(Player::open().map_err(audio_error)?);
        let (tx, rx) = std::sync::mpsc::channel();
        player.events(EventSink::new(tx));
        player.set_audio_effects(effects(&self.settings.lock().unwrap()));
        let events = self.events.clone();
        let loudness_cache = self.loudness.clone();
        std::thread::Builder::new()
            .name("haiku-audio-events".into())
            .spawn(move || {
                while let Ok(event) = rx.recv() {
                    if matches!(event.data.kind, PlaybackEventKind::Loudness) {
                        let d = &event.data;
                        if let Err(error) = loudness_cache.lock().unwrap().record(
                            d.message.clone(),
                            haiku_audio::loudness::LoudnessReport {
                                lufs: d.loudness_lufs,
                                peak: d.loudness_peak,
                                measured_secs: d.loudness_secs,
                                complete: d.loudness_complete,
                            },
                        ) {
                            eprintln!("Loudness cache: {error}");
                        }
                        continue;
                    }
                    let kind = match event.data.kind {
                        PlaybackEventKind::Finished => AudioEventKind::Finished,
                        PlaybackEventKind::TrackChanged => AudioEventKind::TrackChanged,
                        PlaybackEventKind::Error => {
                            AudioEventKind::Error(audio_error(event.data.message))
                        }
                        PlaybackEventKind::BufferingStarted | PlaybackEventKind::OutputLost => {
                            AudioEventKind::Buffering(true)
                        }
                        PlaybackEventKind::BufferingEnded | PlaybackEventKind::OutputRestored => {
                            AudioEventKind::Buffering(false)
                        }
                        _ => continue, // Position/buffer never enter the low-frequency event bridge.
                    };
                    if events
                        .send(AudioEvent {
                            generation: event.generation.unwrap_or(0),
                            kind,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
        *slot = Some(player.clone());
        Ok(player)
    }
    fn with_player(&self, f: impl FnOnce(&Player)) {
        if let Some(player) = &*self.player.lock().unwrap() {
            f(player);
        }
    }
}
fn audio_error(message: String) -> Error {
    let code = if message.to_lowercase().contains("unsupported") || message.contains("不支持") {
        ErrorCode::UnsupportedFormat
    } else {
        ErrorCode::PlaybackError
    };
    Error::new(code, message)
}
pub fn effects(settings: &Settings) -> AudioEffectsOptions {
    let e = &settings.audio_effects;
    AudioEffectsOptions {
        enabled: e.enabled,
        band_gains_db: e.band_gains_db.to_vec(),
        auto_preamp: e.auto_preamp,
        preamp_db: e.preamp_db,
        stereo_width: e.stereo_width,
        balance: e.balance,
        mono: e.mono,
    }
}
impl AudioBackend for RealAudio {
    fn open(&self, path: PathBuf, generation: u64) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async move {
            self.player()?
                .play_local(
                    path.to_string_lossy().into(),
                    None,
                    LoudnessOptions {
                        enabled: false,
                        target_lufs: -18.,
                        known_gain_db: None,
                        measure: false,
                        track_key: String::new(),
                    },
                    generation,
                )
                .await
                .map_err(audio_error)
        })
    }
    fn prepare(&self, path: PathBuf, generation: u64) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async move {
            self.player()?
                .prepare_next_local(
                    path.to_string_lossy().into(),
                    None,
                    LoudnessOptions {
                        enabled: false,
                        target_lufs: -18.,
                        known_gain_db: None,
                        measure: false,
                        track_key: String::new(),
                    },
                    generation,
                )
                .await
                .map_err(audio_error)
        })
    }
    fn open_track(
        &self,
        track: Track,
        path: PathBuf,
        generation: u64,
        prepare: bool,
    ) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async move {
            let options = self.options(&track);
            let player = self.player()?;
            if prepare {
                player
                    .prepare_next_local(path.to_string_lossy().into(), None, options, generation)
                    .await
                    .map_err(audio_error)
            } else {
                *self.current.lock().unwrap() = Some(track);
                player
                    .play_local(path.to_string_lossy().into(), None, options, generation)
                    .await
                    .map_err(audio_error)
            }
        })
    }
    fn configure(&self, settings: Settings) {
        *self.settings.lock().unwrap() = settings.clone();
        self.with_player(|p| p.set_audio_effects(effects(&settings)));
        if let Some(track) = self.current.lock().unwrap().as_ref() {
            let options = self.options(track);
            self.with_player(|p| p.set_loudness(options));
        }
    }
    fn pin_tracks(&self, current: Option<Track>, next: Option<Track>) {
        let quality = self.settings.lock().unwrap().audio_quality.clone();
        haiku_audio::media_cache::set_pinned(
            current.as_ref().and_then(|t| cache_key(t, &quality)),
            next.as_ref().and_then(|t| cache_key(t, &quality)),
        );
        *self.current.lock().unwrap() = current;
    }
    fn configuration(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }
    fn clear_next(&self) {
        self.with_player(Player::clear_next);
    }
    fn pause(&self) {
        self.with_player(Player::pause);
    }
    fn resume(&self) {
        self.with_player(Player::resume);
    }
    fn stop(&self) {
        self.with_player(Player::stop);
    }
    fn seek(&self, seconds: f64) {
        self.with_player(|p| p.seek(seconds));
    }
    fn set_volume(&self, volume: f64) {
        self.with_player(|p| p.set_volume(volume as f32));
    }
    fn progress(&self) -> Progress {
        let slot = self.player.lock().unwrap();
        slot.as_ref()
            .map(|p| {
                let (generation, position, buffered, buffering) = p.progress();
                Progress {
                    generation,
                    position,
                    buffered,
                    buffering,
                }
            })
            .unwrap_or_default()
    }
}

/// Local files always bypass the disk cache. JSON tuples retain full source/entity/quality identity.
pub fn cache_key(track: &Track, quality: &str) -> Option<String> {
    track
        .local_path
        .is_none()
        .then(|| serde_json::json!([track.entity_ref(), quality]).to_string())
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    #[test]
    fn preferences_feed_dsp_survive_track_changes_and_exclude_local_cache() {
        let (audio, _) = RealAudio::new();
        let mut s = Settings::default();
        s.audio_effects.enabled = true;
        s.audio_effects.band_gains_db = [6.; 10];
        s.audio_effects.auto_preamp = false;
        s.audio_effects.preamp_db = -6.;
        s.audio_effects.stereo_width = 1.5;
        s.audio_effects.balance = 0.25;
        s.audio_effects.mono = true;
        audio.configure(s.clone());
        let config = haiku_audio::effects::EffectsConfig::new();
        config.set(effects(&audio.configuration()).into_settings());
        let mut dsp = haiku_audio::effects::EffectsProcessor::new(48000, 2);
        let mut pcm = vec![0.2_f32; 96000];
        dsp.process(&mut pcm, &config);
        assert!(pcm.iter().all(|x| x.is_finite() && x.abs() <= 1.));
        assert!(pcm.iter().any(|x| (*x - 0.2).abs() > 0.01));
        *audio.current.lock().unwrap() = Some(Track::local("a"));
        let a = audio.options(audio.current.lock().unwrap().as_ref().unwrap());
        assert!(a.enabled);
        assert!(a.measure);
        *audio.current.lock().unwrap() = Some(Track::local("b"));
        let b = audio.options(audio.current.lock().unwrap().as_ref().unwrap());
        assert_ne!(a.track_key, b.track_key);
        assert_eq!(audio.configuration(), s);
        assert!(cache_key(&Track::local("b"), "high").is_none());
        let mut remote = Track::local("remote");
        remote.local_path = None;
        assert_ne!(cache_key(&remote, "high"), cache_key(&remote, "lossless"));
    }
}
