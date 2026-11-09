//! Disposable M0 platform spikes; no application feature state lives here.

mod audio;
mod tracks;
pub use tracks::SpikeTrackList;

use nsh_core::prelude::*;

/// Temporary M0 native entry points.
#[module]
pub struct Spike;

#[module]
impl Spike {
    /// Doubles a number in an isolated plugin VM, verifying MD5 and Buffer.
    pub async fn plugin_echo(n: f64) -> Result<f64, String> {
        background(haiku_plugins::plugin_echo(n))
            .await
            .map_err(|error| format!("Plugin background task failed: {error}"))?
    }

    /// Plays a local file through the migrated playback API.
    pub async fn play_local(path: String) -> Result<(), String> {
        background(audio::play_local(path))
            .await
            .map_err(|error| format!("Playback background task failed: {error}"))?
    }

    /// Pauses the same native playback instance.
    pub fn pause() {
        audio::pause();
    }

    /// The most recent position reported by the engine, in seconds.
    pub fn position() -> f64 {
        audio::position()
    }

    /// Opt-in local fixture for the real-window acceptance run.
    pub fn audio_path() -> Option<String> {
        std::env::var("HAIKU_SPIKE_AUDIO").ok()
    }
}

/// One Rust counter, held by its views; TSX values are listener mirrors.
#[class]
pub struct SpikeCounter {
    value: u32,
    listeners: Vec<Callback<u32>>,
}

#[class]
impl SpikeCounter {
    /// Creates the counter for this spike.
    pub fn new() -> Self {
        Self {
            value: 0,
            listeners: Vec::new(),
        }
    }

    /// The Rust-owned value.
    #[getter]
    pub fn value(&self) -> u32 {
        self.value
    }

    /// Increments the owner, then notifies its mirrors.
    pub fn increment(&mut self) {
        self.value += 1;
        for listener in &self.listeners {
            listener.post(self.value);
        }
    }

    /// Observes changes without transferring ownership to TSX.
    pub fn on_change(&mut self, #[args(value)] listener: Callback<u32>) {
        self.listeners.push(listener);
    }
}
