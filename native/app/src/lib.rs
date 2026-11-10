//! Native Core and playback surfaces; retained shell navigation lives in TSX.
use nsh_core::prelude::*;
mod core;
mod command_keyboard;
mod exit;
mod collections;
mod activity;
mod library;
mod lyrics_view;
mod now_playing;
mod platform;
mod player_controls;
mod player_dock;
mod player_theme;
mod plugins;
mod sources;
mod discovery;
mod network_fixture;
mod plugin_tools;
mod presentation;
mod seek_bar;
mod settings;
mod track_list;
mod track_source;
pub use core::{Core, CoreBridge};
pub use command_keyboard::CommandKeyboard;
pub use now_playing::NowPlaying;
pub use player_dock::PlayerDock;
pub use seek_bar::SeekBar;
pub use track_list::TrackList;

/// The native API imported by the TSX entry.
#[native_module("haiku/native")]
fn native(registry: &mut Registry) {
    registry.class::<Core>();
    registry.register::<CommandKeyboard>();
    registry.register::<TrackList>();
    registry.register::<SeekBar>();
    registry.register::<PlayerDock>();
    registry.register::<NowPlaying>();
}

#[cfg(test)]
mod jsrt_tests;
