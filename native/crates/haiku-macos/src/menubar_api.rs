//! In-process status-item adapter. Installation is serialized on AppKit's main queue;
//! callbacks share the existing native-event channel back to Core.
use crate::menubar::MenuItemSpec;
#[cfg(target_os = "macos")]
static HANDLE: std::sync::OnceLock<crate::menubar::MenuBarHandle> = std::sync::OnceLock::new();

pub enum Update {
    Line {
        text: String,
        words: Vec<(f64, f64, String)>,
        start: f64,
        position: f64,
        paused: bool,
        delay: f64,
    },
    Progress(f64, bool),
    Playing(bool),
    Favorite(bool),
    LyricsVisible(bool),
    Highlight(Option<u32>),
    Menu(Vec<MenuItemSpec>),
}
/// Call from a worker after the real NSH native window is ready. Never from Headless.
pub fn install(display_name: String) -> bool {
    #[cfg(target_os = "macos")]
    {
        dispatch::Queue::main().exec_sync(move || {
            HANDLE.get_or_init(|| {
                let (tx, rx) = std::sync::mpsc::channel();
                let handle = crate::menubar::install(&display_name, tx);
                std::thread::spawn(move || {
                    while let Ok(event) = rx.recv() {
                        let _ = crate::events::emit(
                            "menubar.event",
                            serde_json::to_value(event).unwrap(),
                        );
                    }
                });
                handle
            });
        });
        true
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = display_name;
        false
    }
}
/// Ordered changed-value pushes; no per-frame updates. Call from a worker.
pub fn apply(updates: Vec<Update>) {
    #[cfg(target_os = "macos")]
    if let Some(h) = HANDLE.get() {
        for update in updates {
            match update {
                Update::Line {
                    text,
                    words,
                    start,
                    position,
                    paused,
                    delay,
                } => h.set_lyric_line(text, words, start, position, paused, delay),
                Update::Progress(pos, paused) => h.set_progress(pos, paused),
                Update::Playing(v) => h.set_playing(v),
                Update::Favorite(v) => h.set_favorite(v),
                Update::LyricsVisible(v) => h.set_lyrics_visible(v),
                Update::Menu(v) => h.set_menu(v),
                Update::Highlight(rgb) => h.set_highlight_color(rgb.map(|v| {
                    (
                        ((v >> 16) & 255) as f64 / 255.,
                        ((v >> 8) & 255) as f64 / 255.,
                        (v & 255) as f64 / 255.,
                    )
                })),
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = updates;
}
/// Debug-only read of the applied AppKit state (not the producer's desired state).
#[cfg(debug_assertions)]
pub fn snapshot() -> Option<serde_json::Value> {
    #[cfg(target_os = "macos")]
    {
        HANDLE.get().map(|h| h.snapshot())
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}
