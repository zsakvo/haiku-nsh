//! The minimal Haiku native exports used by the M0 verification screen.

use nsh_base as _;
use nsh_core::prelude::*;

/// The native API imported by the TSX entry.
#[native_module("haiku/native")]
fn native(registry: &mut Registry) {
    registry.module::<Core>().module::<Spike>();
}

/// Temporary M0 probes for the isolated JavaScript runtimes.
#[module]
pub struct Spike;

#[module]
impl Spike {
    /// Doubles a finite number in an isolated plugin VM, verifying MD5 and Buffer.
    pub async fn plugin_echo(n: f64) -> Result<f64, String> {
        background(haiku_plugins::plugin_echo(n))
            .await
            .map_err(|error| format!("Plugin background task failed: {error}"))?
    }
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
