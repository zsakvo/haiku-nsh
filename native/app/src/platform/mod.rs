//! Native lifecycle only; media center and status item return in M1-05.
pub fn initialize() {
    #[cfg(target_os = "macos")]
    haiku_macos::lifecycle::install_async();
}

#[cfg(target_os = "macos")]
mod media;
#[cfg(target_os = "macos")]
mod menubar;
pub(crate) fn initialize_media(
    bridge: std::rc::Rc<crate::core::CoreBridge>,
    settings: nsh_core::prelude::Instance<crate::settings::SettingsService>,
    native: bool,
) {
    #[cfg(target_os = "macos")]
    media::initialize(bridge, settings, native);
    #[cfg(not(target_os = "macos"))]
    let _ = (bridge, settings, native);
}
pub(crate) fn clear_media() {
    #[cfg(target_os = "macos")]
    haiku_macos::system::shutdown_async();
}

/// Install the Core durability barrier even when no media session has ever existed.
pub(crate) fn install_exit_hook() {
    #[cfg(target_os = "macos")]
    haiku_macos::lifecycle::set_exit_handler(crate::exit::on_exit);
}
