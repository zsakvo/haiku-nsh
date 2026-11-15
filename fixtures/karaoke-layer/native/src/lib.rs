//! Fixture-only A/B registration. Run the fixture with HAIKU_KARAOKE_PREVIEW_NATIVE=1
//! or =0; both cases use KaraokeLayerPreview.tsx and the same application registry.
pub use nsh_app_haiku::__nsh_native_module;

#[cfg(target_os = "macos")]
#[doc(hidden)]
pub fn __nsh_native_components_macos(components: &nsh_platform_macos::Components) {
    haiku_macos::glass::register(components);
    let enabled = std::env::var("HAIKU_KARAOKE_PREVIEW_NATIVE").as_deref() != Ok("0");
    if enabled {
        haiku_macos::karaoke::register(components);
    }
    eprintln!("M7-03 preview KaraokeLayer registered={enabled}");
}
