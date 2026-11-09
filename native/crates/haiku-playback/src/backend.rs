use futures::future::BoxFuture;
use haiku_audio::playback::{LoudnessOptions, PlaybackEventKind, Player};
use haiku_model::{Error, ErrorCode, Track, events::EventSink};
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
}
impl RealAudio {
    pub fn new() -> (Arc<Self>, mpsc::UnboundedReceiver<AudioEvent>) {
        let (events, rx) = mpsc::unbounded_channel();
        (
            Arc::new(Self {
                player: Mutex::new(None),
                events,
            }),
            rx,
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
        let events = self.events.clone();
        std::thread::Builder::new()
            .name("haiku-audio-events".into())
            .spawn(move || {
                while let Ok(event) = rx.recv() {
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
fn loudness() -> LoudnessOptions {
    LoudnessOptions {
        enabled: false,
        target_lufs: -18.0,
        known_gain_db: None,
        measure: false,
        track_key: String::new(),
    }
}
impl AudioBackend for RealAudio {
    fn open(&self, path: PathBuf, generation: u64) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async move {
            self.player()?
                .play_local(path.to_string_lossy().into(), None, loudness(), generation)
                .await
                .map_err(audio_error)
        })
    }
    fn prepare(&self, path: PathBuf, generation: u64) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async move {
            self.player()?
                .prepare_next_local(path.to_string_lossy().into(), None, loudness(), generation)
                .await
                .map_err(audio_error)
        })
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
