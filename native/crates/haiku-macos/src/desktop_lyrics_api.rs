//! In-process adapter. All creation and mutation is serialized on the main queue.
use crate::desktop_lyrics::{LyricLine, Style, geometry::Position};
#[cfg(target_os = "macos")]
static HANDLE: std::sync::OnceLock<crate::desktop_lyrics::DesktopLyricsHandle> =
    std::sync::OnceLock::new();
pub enum Update {
    Show(bool),
    Locked(bool),
    Style(Style),
    Lines {
        current: Option<LyricLine>,
        next: Option<LyricLine>,
        start: f64,
        position: f64,
        paused: bool,
    },
    Progress(f64, bool),
    Playing(bool),
    Favorite(bool),
}
/// Worker-only; the caller excludes isolated fixtures and Headless before entering.
pub fn install(position: Option<Position>) -> bool {
    #[cfg(target_os = "macos")]
    {
        dispatch::Queue::main().exec_sync(move || {
            HANDLE.get_or_init(|| {
                let (tx, rx) = std::sync::mpsc::channel();
                let h = crate::desktop_lyrics::install(tx, position);
                std::thread::spawn(move || {
                    while let Ok(event) = rx.recv() {
                        let _ = crate::events::emit(
                            "desktopLyrics.event",
                            serde_json::to_value(event).unwrap(),
                        );
                    }
                });
                h
            });
        });
        true
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = position;
        false
    }
}
pub fn apply(updates: Vec<Update>) {
    #[cfg(target_os = "macos")]
    if let Some(h) = HANDLE.get() {
        for update in updates {
            match update {
                Update::Show(v) => h.show(v),
                Update::Locked(v) => h.set_locked(v),
                Update::Style(v) => h.set_style(v),
                Update::Lines {
                    current,
                    next,
                    start,
                    position,
                    paused,
                } => h.set_lines(current, next, start, position, paused),
                Update::Progress(pos, paused) => h.set_progress(pos, paused),
                Update::Playing(v) => h.set_playing(v),
                Update::Favorite(v) => h.set_favorite(v),
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = updates;
}
/// Opens NSFontPanel only after the caller has confirmed a real native runtime.
pub fn pick_font(family: String, name: String, size: f64, weight: f64) -> bool {
    #[cfg(target_os = "macos")]
    {
        HANDLE
            .get()
            .is_some_and(|h| h.pick_font(family, name, size, weight))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (family, name, size, weight);
        false
    }
}
/// Applied AppKit state. Does not create a panel merely to inspect it.
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
pub fn hide() {
    apply(vec![Update::Show(false)]);
}
