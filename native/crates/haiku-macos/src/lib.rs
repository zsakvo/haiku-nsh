//! Haiku macos domain boundary.


pub mod desktop_lyrics;
pub mod desktop_lyrics_api;
#[cfg(target_os = "macos")]
pub mod events;
#[cfg(target_os = "macos")]
pub mod glass;
#[cfg(target_os = "macos")]
pub mod lifecycle;
pub mod menubar;
pub mod menubar_api;
pub mod shortcuts;
#[cfg(target_os = "macos")]
pub mod system;
#[cfg(target_os = "macos")]
pub mod web_auth;
