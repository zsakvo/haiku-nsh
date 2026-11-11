//! Haiku macos domain boundary.


#[cfg(target_os = "macos")]
pub mod desktop_lyrics;
#[cfg(target_os = "macos")]
pub mod desktop_lyrics_api;
#[cfg(target_os = "macos")]
pub mod events;
#[cfg(target_os = "macos")]
pub mod glass;
#[cfg(target_os = "macos")]
pub mod lifecycle;
#[cfg(target_os = "macos")]
pub mod menubar;
#[cfg(target_os = "macos")]
pub mod menubar_api;
pub mod shortcuts;
#[cfg(target_os = "macos")]
pub mod system;
#[cfg(target_os = "macos")]
pub mod web_auth;
