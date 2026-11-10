//! Haiku playback domain boundary.
pub mod backend;
pub mod controller;
pub mod queue;
pub use backend::{
    AudioBackend, AudioEvent, AudioEventKind, Clock, LocalResolver, Progress, RealAudio,
    SystemClock, TrackResolver,
};
pub use controller::{Command, Controller, PlayerState, Session, Sleep, Status, RadioProvider, RadioState};
pub use queue::{Mode, Queue};

mod loudness_cache;
mod network;
pub use network::Candidate;
