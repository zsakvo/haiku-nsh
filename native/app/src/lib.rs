//! Native Core and playback surfaces; retained shell navigation lives in TSX.
use nsh_core::prelude::*;
mod core;
mod account;
mod backup;
mod sync;
mod account_fixture;
mod command_keyboard;
mod exit;
mod collections;
mod favorite_refresh;
mod scrape_service;
mod download_service;
mod downloads;
mod scrape;
mod activity;
mod library;
mod lyrics_view;
mod lyrics;
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
mod shortcuts;
mod menubar;
mod desktop_lyrics;
mod track_list;
mod track_source;
pub use core::{Core, CoreBridge};
pub use command_keyboard::CommandKeyboard;
pub use now_playing::NowPlaying;
pub use player_dock::PlayerDock;
pub use seek_bar::SeekBar;
pub use track_list::TrackList;

/// Platform leaves are registered before the NSH surface is loaded.
#[native_components(macos)]
fn native_views(components: &nsh_platform_macos::Components) {
    haiku_macos::glass::register(components);
}

/// The native API imported by the TSX entry.
#[native_module("haiku/native")]
fn native(registry: &mut Registry) {
    registry.assets(assets!());
    // In addition to embedded first-launch installation bytes, ship readable
    // plugin resources through NSH's asset collector.
    macro_rules! plugin_asset {
        ($path:literal) => {
            nsh_core::element::AssetFile {
                uri: concat!("asset:/plugins/", $path),
                source: concat!(env!("CARGO_MANIFEST_DIR"), "/../../plugins/", $path),
                kind: nsh_core::element::AssetKind::File,
            }
        };
    }
    registry.assets(&[
        plugin_asset!("subsonic/plugin.ts"),
        plugin_asset!("subsonic/logo.png"),
        plugin_asset!("subsonic/fixtures/contract.json"),
        plugin_asset!("feiniu/plugin.ts"),
        plugin_asset!("feiniu/logo.png"),
        plugin_asset!("feiniu/fixtures/contract.json"),
        plugin_asset!("qq-scraper/plugin.ts"),
        plugin_asset!("qq-scraper/logo.png"),
        plugin_asset!("qq-scraper/fixtures/contract.json"),
        plugin_asset!("kugou-scraper/plugin.ts"),
        plugin_asset!("kugou-scraper/logo.png"),
        plugin_asset!("kugou-scraper/fixtures/contract.json"),
    ]);
    macro_rules! ui_asset {
        ($path:literal) => {
            nsh_core::element::AssetFile {
                uri: concat!("asset:/ui/", $path),
                source: concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/", $path),
                kind: nsh_core::element::AssetKind::File,
            }
        };
    }
    registry.assets(&[
        ui_asset!("icons/home-simple.svg"),
        ui_asset!("icons/view-grid.svg"),
        ui_asset!("icons/antenna.svg"),
        ui_asset!("icons/search.svg"),
        ui_asset!("icons/music-double-note.svg"),
        ui_asset!("icons/heart.svg"),
        ui_asset!("icons/clock.svg"),
        ui_asset!("icons/stats-up-square.svg"),
        ui_asset!("icons/download.svg"),
        ui_asset!("icons/settings.svg"),
        ui_asset!("icons/nav-arrow-left.svg"),
        ui_asset!("icons/nav-arrow-right.svg"),
        ui_asset!("icons/xmark.svg"),
        ui_asset!("icons/check.svg"),
        ui_asset!("icons/palette.svg"),
        ui_asset!("icons/user.svg"),
        ui_asset!("icons/play.svg"),
        ui_asset!("icons/closed-captions-tag.svg"),
        ui_asset!("icons/folder.svg"),
        ui_asset!("icons/plug-type-a.svg"),
        ui_asset!("icons/refresh-double.svg"),
        ui_asset!("icons/key-command.svg"),
        ui_asset!("icons/code-brackets.svg"),
        ui_asset!("illustrations/listening.svg"),
        ui_asset!("illustrations/listening-dark.svg"),
    ]);
    registry.class::<Core>();
    registry.register::<CommandKeyboard>();
    registry.register::<TrackList>();
    registry.register::<SeekBar>();
    registry.register::<PlayerDock>();
    registry.register::<NowPlaying>();
}

#[cfg(test)]
mod jsrt_tests;
