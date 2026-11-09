//! Small platform and theme entry points; application screens live in TSX.
use nsh_core::prelude::*;
mod platform;

/// The native API imported by the TSX entry.
#[native_module("haiku/native")]
fn native(registry: &mut Registry) {
    registry.module::<Core>();
}

/// Application platform services.
#[module]
pub struct Core;

#[module]
impl Core {
    /// Install the window lifecycle policy at application startup.
    pub fn initialize() {
        platform::initialize();
    }

    /// Replace semantic tokens on the engine, shared by TSX and Rust views.
    pub async fn apply_theme(json: String) -> Result<(), String> {
        let theme = nsh_base::Theme::from_json(&json).map_err(|error| error.to_string())?;
        set_tokens(theme);
        Ok(())
    }
}
