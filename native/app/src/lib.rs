//! The minimal Haiku native exports used by the M0 verification screen.

use nsh_core::prelude::*;
mod spike;
use spike::{Spike, SpikeCounter, SpikeTrackList};

/// The native API imported by the TSX entry.
#[native_module("haiku/native")]
fn native(registry: &mut Registry) {
    registry
        .module::<Core>()
        .module::<Spike>()
        .class::<SpikeCounter>()
        .register::<SpikeTrackList>();
}

/// Build information for the Haiku core.
#[module]
pub struct Core;

#[module]
impl Core {
    /// The version of the native application crate.
    pub fn version() -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }
}
