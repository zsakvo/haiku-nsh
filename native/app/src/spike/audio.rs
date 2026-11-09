//! Thin playback adapter. Position is an engine event snapshot, not a timer.

use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};

use haiku_audio::playback::{LoudnessOptions, PlaybackEventKind, Player};
use haiku_model::events::EventSink;

static PLAYER: OnceLock<Result<Arc<Player>, String>> = OnceLock::new();
static POSITION: AtomicU64 = AtomicU64::new(0);
static GENERATION: AtomicU64 = AtomicU64::new(1);
static PLAYING: AtomicBool = AtomicBool::new(false);

fn player() -> Result<Arc<Player>, String> {
    PLAYER
        .get_or_init(|| {
            let player = Arc::new(Player::open()?);
            let (sender, receiver) = mpsc::channel();
            player.events(EventSink::new(sender));
            std::thread::Builder::new()
                .name("haiku-spike-position".into())
                .spawn(move || {
                    for event in receiver {
                        match event.data.kind {
                            PlaybackEventKind::Position => {
                                POSITION.store(event.data.position.to_bits(), Ordering::Release);
                            }
                            PlaybackEventKind::Error => {
                                PLAYING.store(false, Ordering::Release);
                                eprintln!("S5 playback error: {}", event.data.message);
                            }
                            PlaybackEventKind::Finished => {
                                PLAYING.store(false, Ordering::Release);
                            }
                            _ => {}
                        }
                    }
                })
                .map_err(|error| error.to_string())?;
            Ok(player)
        })
        .clone()
}

pub async fn play_local(path: String) -> Result<(), String> {
    player()?
        .play_local(
            path,
            None,
            LoudnessOptions {
                enabled: false,
                target_lufs: haiku_audio::playback::loudness_default_target_lufs(),
                known_gain_db: None,
                measure: false,
                track_key: String::new(),
            },
            GENERATION.fetch_add(1, Ordering::AcqRel),
        )
        .await?;
    PLAYING.store(true, Ordering::Release);
    Ok(())
}

pub fn pause() {
    if let Some(Ok(player)) = PLAYER.get() {
        player.pause();
        PLAYING.store(false, Ordering::Release);
    }
}

pub fn playing() -> bool {
    PLAYING.load(Ordering::Acquire)
}

pub fn resume() {
    if let Some(Ok(player)) = PLAYER.get() {
        player.resume();
        PLAYING.store(true, Ordering::Release);
    }
}

pub fn next() {
    if let Some(Ok(player)) = PLAYER.get() {
        // The spike has no queue. Use the real engine handoff path and report
        // whether a prefetched successor exists.
        eprintln!("S4 skip_to_next={}", player.skip_to_next());
    }
}

pub fn position() -> f64 {
    f64::from_bits(POSITION.load(Ordering::Acquire))
}
