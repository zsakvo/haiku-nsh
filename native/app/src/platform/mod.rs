//! Native lifecycle only; media center and status item return in M1-05.
pub fn initialize() {
    #[cfg(target_os = "macos")]
    haiku_macos::lifecycle::install_async();
}
