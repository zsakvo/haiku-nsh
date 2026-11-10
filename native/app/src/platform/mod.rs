//! Native lifecycle only; media center and status item return in M1-05.
pub fn initialize() {
    #[cfg(target_os = "macos")]
    haiku_macos::lifecycle::install_async();
}

#[cfg(target_os = "macos")]
mod media;
pub(crate) fn initialize_media(bridge: std::rc::Rc<crate::core::CoreBridge>) {
    #[cfg(target_os = "macos")]
    media::initialize(bridge);
    #[cfg(not(target_os = "macos"))]
    let _ = bridge;
}
pub(crate) fn clear_media() {
    #[cfg(target_os = "macos")]
    haiku_macos::system::shutdown_async();
}
