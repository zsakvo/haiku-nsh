//! Small platform and theme entry points; application screens live in TSX.
use nsh_core::prelude::*;
mod core;
mod library;
mod platform;
mod track_list;
mod track_source;
pub use core::{Core, CoreBridge};
pub use track_list::TrackList;

/// The native API imported by the TSX entry.
#[native_module("haiku/native")]
fn native(registry: &mut Registry) {
    registry.class::<Core>();
    registry.register::<TrackList>();
}
