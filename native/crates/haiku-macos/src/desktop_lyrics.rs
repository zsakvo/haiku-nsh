//! Compact native desktop lyrics. The 32-point hover toolbar has a permanent reserved
//! row, so revealing controls never moves the lyrics. Classic, left and stage show
//! current + translation (or next); focus shows only the current line.
//!
//! Text is rendered in independent Core Animation layers, preserving word timing
//! through pause/seek and font changes. Long lines fit down to a readable minimum,
//! then pan inside a viewport. The window has no shadow (avoiding stale snapshots).
//! AppKit owns the SF Symbol controls; locked panels ignore mouse events entirely.
//! Solid and liquid backgrounds remain available; the default is transparent.

use std::sync::mpsc::Sender;

// ===================== 跨平台共享类型 =====================

pub mod geometry;
use geometry::Position;

#[derive(Debug,serde::Serialize,serde::Deserialize)]
#[serde(rename_all="snake_case")]
pub enum DesktopLyricsEvent {
    Prev,
    Toggle,
    Next,
    ToggleFavorite,
    Lock,
    Unlock,
    ShowApp,
    Settings,
    Close,
    Position(Position),
}

#[derive(Clone,Debug,PartialEq)]
pub struct Style {
    pub text_color: Option<(f64, f64, f64)>,
    pub highlight_color: Option<(f64, f64, f64)>,
    pub outline: bool,
    pub shadow: bool,
    pub background: String,
    pub font_size: f64,
    pub font_family: String,
    /// Exact PostScript face chosen in the system font panel (including italic traits).
    pub font_name: String,
    /// NSFontWeight 数值（例如 0=Regular、0.4=Bold）。应用到当前行与副行。
    pub font_weight: f64,
    pub layout: String,
    pub show_translation: bool,
}

#[derive(Clone,Debug,PartialEq)]
pub struct LyricLine {
    pub text: String,
    pub translation: String,
    pub words: Vec<(f64, f64, String)>,
}

/// Pure layout policy, shared by rendering and the geometry tests below.
pub mod layout {
    use super::{LyricLine, Style};

    pub const PAD_X: f64 = 16.0;
    pub const TOP: f64 = 40.0;
    pub const BOTTOM: f64 = 8.0;
    const GAP: f64 = 4.0;

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum LineKind {
        Current,
        Translation,
        Next,
    }

    impl LineKind {
        pub fn name(self) -> &'static str {
            match self {
                Self::Current => "current",
                Self::Translation => "translation",
                Self::Next => "next",
            }
        }
    }

    pub struct Row {
        pub text: String,
        pub kind: LineKind,
        pub font_size: f64,
        pub width: f64,
        pub height: f64,
        pub x: f64,
        pub y: f64,
        pub viewport_width: f64,
    }

    pub struct Layout {
        pub rows: Vec<Row>,
        pub height: f64,
    }

    /// Fit using the same native font metrics as rendering. Very long sentences stop
    /// shrinking at a readable size and scroll through their bounded viewport.
    pub fn build(
        style: &Style,
        current: Option<&LyricLine>,
        next: Option<&LyricLine>,
        width: f64,
        mut measure: impl FnMut(&str, f64) -> (f64, f64),
    ) -> Layout {
        let mut selected = Vec::with_capacity(2);
        if let Some(current) = current.filter(|line| !line.text.trim().is_empty()) {
            selected.push((current.text.as_str(), LineKind::Current));
        }
        if style.layout != "focus" {
            let translation = current
                .filter(|_| style.show_translation)
                .map(|line| line.translation.as_str())
                .filter(|text| !text.trim().is_empty());
            if let Some(text) = translation {
                selected.push((text, LineKind::Translation));
            } else if let Some(next) = next.filter(|line| !line.text.trim().is_empty()) {
                selected.push((next.text.as_str(), LineKind::Next));
            }
        }
        let available = (width - PAD_X * 2.0).max(1.0);
        let base = if style.font_size.is_finite() {
            style.font_size.clamp(14.0, 60.0)
        } else {
            28.0
        };
        let mut rows = Vec::with_capacity(selected.len());
        let mut current_size = base;
        for (text, kind) in selected {
            let factor = if kind == LineKind::Current {
                1.0
            } else if style.layout == "stage" {
                0.62
            } else {
                0.72
            };
            let preferred = (current_size * factor).max(14.0);
            let minimum = if kind == LineKind::Current {
                20.0_f64.min(preferred)
            } else {
                14.0
            };
            let mut font_size = preferred;
            let (mut measured_width, mut height) = measure(text, font_size);
            if measured_width > available && preferred > minimum {
                // Binary search also handles hinted/custom fonts whose metrics are
                // not quite proportional to the requested point size.
                let (mut low, mut high) = (minimum, preferred);
                for _ in 0..12 {
                    let mid = (low + high) * 0.5;
                    if measure(text, mid).0 <= available {
                        low = mid;
                    } else {
                        high = mid;
                    }
                }
                font_size = low;
                (measured_width, height) = measure(text, font_size);
            }
            if kind == LineKind::Current {
                current_size = font_size;
            }
            let viewport_width = measured_width.ceil().min(available);
            rows.push(Row {
                text: text.to_string(),
                kind,
                font_size,
                width: measured_width.ceil(),
                height: height.ceil() + 2.0,
                x: if style.layout == "left" {
                    PAD_X
                } else {
                    (width - viewport_width) * 0.5
                },
                y: 0.0,
                viewport_width,
            });
        }
        let text_height = rows.iter().map(|row| row.height).sum::<f64>()
            + GAP * rows.len().saturating_sub(1) as f64;
        let height = (TOP + text_height + BOTTOM).max(90.0).ceil();
        let mut y = height - TOP;
        for row in &mut rows {
            y -= row.height;
            row.y = y;
            y -= GAP;
        }
        Layout { rows, height }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn style(layout: &str) -> Style {
            Style {
                text_color: None,
                highlight_color: None,
                outline: true,
                shadow: true,
                background: "none".into(),
                font_size: 28.0,
                font_family: String::new(),
                font_name: String::new(),
                font_weight: 0.0,
                layout: layout.into(),
                show_translation: true,
            }
        }
        fn line(text: &str, translation: &str) -> LyricLine {
            LyricLine {
                text: text.into(),
                translation: translation.into(),
                words: vec![],
            }
        }
        fn measure(text: &str, size: f64) -> (f64, f64) {
            (text.chars().count() as f64 * size, size * 1.25)
        }

        #[test]
        fn translation_replaces_next_in_every_two_row_layout() {
            let current = line("current", "translation");
            let next = line("next", "");
            for name in ["classic", "left", "stage"] {
                let mut style = style(name);
                let result = build(&style, Some(&current), Some(&next), 600.0, measure);
                assert_eq!(result.rows.len(), 2);
                assert_eq!(result.rows[1].kind, LineKind::Translation);
                assert_eq!(result.rows[1].text, "translation");
                style.show_translation = false;
                let result = build(&style, Some(&current), Some(&next), 600.0, measure);
                assert_eq!(result.rows[1].kind, LineKind::Next);
                let blank = line("current", "  ");
                style.show_translation = true;
                assert_eq!(
                    build(&style, Some(&blank), Some(&next), 600.0, measure).rows[1].kind,
                    LineKind::Next
                );
            }
        }

        #[test]
        fn focus_has_only_current_even_with_translation() {
            let result = build(
                &style("focus"),
                Some(&line("current", "translation")),
                Some(&line("next", "")),
                600.0,
                measure,
            );
            assert_eq!(result.rows.len(), 1);
            assert_eq!(result.rows[0].kind, LineKind::Current);
            assert!(build(
                &style("focus"),
                None,
                Some(&line("next", "")),
                600.0,
                measure
            )
            .rows
            .is_empty());
        }

        #[test]
        fn long_lines_fit_or_scroll_at_a_readable_size_inside_bounds() {
            for width in [420.0, 600.0, 900.0] {
                for font_size in [14.0, 28.0, 60.0] {
                    for name in ["classic", "focus", "left", "stage"] {
                        let mut style = style(name);
                        style.font_size = font_size;
                        let current = line(&"词".repeat(300), &"译".repeat(200));
                        let result = build(&style, Some(&current), None, width, measure);
                        for row in &result.rows {
                            assert!(
                                row.x >= PAD_X
                                    && row.x + row.viewport_width <= width - PAD_X + 0.001
                            );
                            assert!(
                                row.y >= BOTTOM
                                    && row.y + row.height <= result.height - TOP + 0.001
                            );
                            assert!(
                                row.font_size
                                    >= if row.kind == LineKind::Current {
                                        font_size.min(20.0)
                                    } else {
                                        14.0
                                    }
                            );
                            assert!(row.width > row.viewport_width); // overflow is retained for panning
                        }
                        let row = &result.rows[0];
                        assert_eq!(row.width, measure(&row.text, row.font_size).0.ceil());
                        if let Some(secondary) = result.rows.get(1) {
                            assert!(secondary.font_size <= row.font_size);
                        }
                    }
                }
            }
        }

        #[test]
        fn medium_lines_fit_without_overflow_and_large_fonts_grow_height() {
            let current = line(&"词".repeat(24), "");
            let result = build(&style("focus"), Some(&current), None, 600.0, measure);
            assert!(result.rows[0].font_size < 28.0);
            assert!(result.rows[0].width <= 568.0);
            let mut large = style("classic");
            large.font_size = 60.0;
            let result = build(&large, Some(&line("字", "译")), None, 600.0, measure);
            assert!(result.height > 90.0);
            assert!(result.rows[1].y + result.rows[1].height < result.rows[0].y);
        }
    }
}

// ===================== 非 macOS：no-op =====================

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::{DesktopLyricsEvent, LyricLine, Sender, Style, Position};

    #[derive(Clone)]
    pub struct DesktopLyricsHandle;

    impl DesktopLyricsHandle {
        pub fn show(&self, _visible: bool) {}
        pub fn set_lines(&self, _current: Option<LyricLine>, _next: Option<LyricLine>, _start:f64, _pos:f64, _paused:bool) {}
        pub fn set_progress(&self, _pos: f64, _paused: bool) {}
        pub fn set_playing(&self, _playing: bool) {}
        pub fn set_favorite(&self, _fav: bool) {}
        pub fn set_locked(&self, _locked: bool) {}
        pub fn set_style(&self, _style: Style) {}
        pub fn register_font(&self, _family: String, _path: String) {}
        pub fn unregister_font(&self, _path: String) -> Result<bool, String> {
            Ok(false)
        }
        pub fn snapshot(&self) -> serde_json::Value {
            serde_json::json!({"supported":false})
        }
        pub fn set_position(&self, _x: f64, _y: f64) {}
        pub fn pick_font(&self, _family: String, _name: String, _size: f64, _weight: f64) -> bool {
            false
        }
    }

    pub fn install(_tx: Sender<DesktopLyricsEvent>, _position:Option<Position>) -> DesktopLyricsHandle {
        DesktopLyricsHandle
    }
}

// ===================== macOS =====================

#[cfg(target_os = "macos")]
mod imp {
    use std::collections::HashMap;
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_void};
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;

    use cocoa::base::{id, nil, NO, YES};
    use cocoa::foundation::{NSPoint, NSRect, NSSize};
    use dispatch::Queue;
    use objc::declare::ClassDecl;
    use objc::runtime::{Class, Object, Sel, BOOL};
    use objc::{class, msg_send, sel, sel_impl};

    use super::{DesktopLyricsEvent, LyricLine, Sender, Style, Position};

    use super::layout::{self, LineKind, Row};

    const WINDOW_W: f64 = 600.0;
    const WINDOW_H: f64 = 90.0;
    const SYMBOL_SIZE: f64 = 16.0;
    const TOOLBAR_W: f64 = 256.0;
    const TOOLBAR_H: f64 = 32.0;

    const GLASS_CORNER_RADIUS: f64 = 14.0;
    const GLASS_MATERIAL_HUD_WINDOW: i64 = 13;

    const TAG_PREV: i64 = 1;
    const TAG_PLAY: i64 = 2;
    const TAG_NEXT: i64 = 3;
    const TAG_LIKE: i64 = 4;
    const TAG_LOCK: i64 = 5;
    const TAG_SHOW_APP: i64 = 6;
    const TAG_SETTINGS: i64 = 7;
    const TAG_CLOSE: i64 = 8;
    const TAG_UNLOCK: i64 = 9;

    const NS_BORDERLESS: u64 = 0;
    const NS_NONACTIVATING_PANEL: u64 = 1 << 7;
    const NS_BACKING_BUFFERED: u64 = 2;
    const NS_FLOATING_LEVEL: i64 = 3;
    const COLLECTION_CAN_JOIN_ALL_SPACES: u64 = 1 << 0;
    const COLLECTION_STATIONARY: u64 = 1 << 4;
    const COLLECTION_FULLSCREEN_AUX: u64 = 1 << 8;

    const TRACKING_OPTS: u64 = 0x01 | 0x02 | 0x80 | 0x200;

    /// Dart 侧的字体族名 → CoreText 实际族名。Flutter 的 `FontLoader` 只把字体注册进
    /// Skia，AppKit 看不到；这里把选中的下载字体文件再用 CoreText 注册一遍，并记录
    /// 文件里的真实族名（导入字体的文件名未必等于真实族名）。
    static FONT_FAMILY_MAP: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    static FONT_PATHS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

    fn font_family_map() -> &'static Mutex<HashMap<String, String>> {
        FONT_FAMILY_MAP.get_or_init(|| Mutex::new(HashMap::new()))
    }

    struct ViewState {
        sender: Sender<DesktopLyricsEvent>,
        window: usize,
        current: Option<LyricLine>,
        next: Option<LyricLine>,
        style: Style,
        song_pos: f64,
        pos_at: Instant,
        paused: bool,
        locked: bool,
        playing: bool,
        favorite: bool,
        hovered: bool,
        visible: bool,
        dragging: bool,
        drag_start_mouse: NSPoint,
        drag_start_origin: NSPoint,
        // 图层
        host: id,
        /// Native behind-window blur; no screen capture permissions or render loop.
        glass: id,
        /// 文字容器层：所有 CATextLayer 都挂在这里。重建文字时只清这个容器，
        /// 与顶部原生按钮视图互不干扰。
        text_host: id,
        current_clip: id,
        word_x: Vec<f64>,
        current_height: f64,
        rows: Vec<Row>,
        scroll_layers: Vec<ScrollLayer>,
        line_start_pos: f64,
        toolbar: id,
        // Top toolbar controls (native NSButton siblings of the text view).
        btn_prev: id,
        btn_play: id,
        btn_next: id,
        btn_like: id,
        btn_lock: id,
        btn_show_app: id,
        btn_settings: id,
        btn_close: id,
        // A separate child panel keeps only the unlock button clickable while
        // the lyric window passes mouse events through to other applications.
        unlock_panel: id,
        btn_unlock: id,
        hover_timer: id,
        #[cfg(debug_assertions)]
        preview_hovered: Option<bool>,
    }

    #[derive(Clone)]
    pub struct DesktopLyricsHandle {
        view: usize,
        window: usize,
    }

    unsafe impl Send for DesktopLyricsHandle {}
    unsafe impl Sync for DesktopLyricsHandle {}

    impl DesktopLyricsHandle {
        /// Debug example support: exercise the same hover/action code as AppKit,
        /// without exposing diagnostic commands in the production RPC interface.
        #[cfg(debug_assertions)]
        pub fn preview_hover(&self, hovered: bool) {
            let view = self.view;
            Queue::main().exec_sync(move || unsafe {
                let st = state_of(&*(view as *const Object));
                st.preview_hovered = Some(hovered);
                if !st.locked {
                    set_hovered(view as id, hovered);
                }
            });
        }

        #[cfg(debug_assertions)]
        pub fn preview_capture(&self, path: &str) -> bool {
            let view = self.view;
            let path = path.to_owned();
            Queue::main().exec_sync(move || unsafe {
                let view = view as id;
                let bounds: NSRect = msg_send![view, bounds];
                let _: () = msg_send![view, displayIfNeeded];
                let bitmap: id = msg_send![view, bitmapImageRepForCachingDisplayInRect: bounds];
                if bitmap == nil {
                    return false;
                }
                let _: () = msg_send![view, cacheDisplayInRect: bounds toBitmapImageRep: bitmap];
                let properties: id = msg_send![class!(NSDictionary), dictionary];
                let data: id =
                    msg_send![bitmap, representationUsingType: 4u64 properties: properties];
                if data == nil {
                    return false;
                }
                let saved: BOOL = msg_send![data, writeToFile: ns_string(&path) atomically: YES];
                saved != NO
            })
        }

        #[cfg(debug_assertions)]
        pub fn preview_action(&self, name: &str) -> bool {
            let view = self.view;
            let name = name.to_owned();
            Queue::main().exec_sync(move || unsafe {
                let st = state_of(&*(view as *const Object));
                if !st.visible {
                    return false;
                }
                if name == "unlock" && st.locked && st.hovered {
                    let _: () = msg_send![st.btn_unlock, performClick: nil];
                    return true;
                }
                if st.locked || !st.hovered {
                    return false;
                }
                let Some((_, button, _)) = controls(st)
                    .into_iter()
                    .find(|(_, _, candidate)| *candidate == name)
                else {
                    return false;
                };
                let _: () = msg_send![button, performClick: nil];
                true
            })
        }

        pub fn snapshot(&self) -> serde_json::Value {
            let (view, window) = (self.view, self.window);
            Queue::main().exec_sync(move || unsafe {
                let st=state_of(&*(view as *const Object));
                let frame:NSRect=msg_send![window as id,frame];
                let visible:BOOL=msg_send![window as id,isVisible];
                let window_id: i64 = msg_send![window as id, windowNumber];
                let ignores_mouse_events: BOOL = msg_send![window as id, ignoresMouseEvents];
                let lines: Vec<_> = st.rows.iter().map(|row| {
                    let scroll_offset = st.scroll_layers.iter().find(|scroll| scroll.kind == row.kind).map(|scroll| {
                        let presentation: id = msg_send![scroll.layer, presentationLayer];
                        let layer = if presentation == nil { scroll.layer } else { presentation };
                        let position: NSPoint = msg_send![layer, position];
                        position.x
                    }).unwrap_or(0.0);
                    serde_json::json!({
                        "kind": row.kind.name(), "text": row.text, "font_size": row.font_size,
                        "text_width": row.width, "scrolling": row.width > row.viewport_width + 0.5,
                        "scroll_offset":scroll_offset,
                        "rect": rect_json(rect(row.x, row.y, row.viewport_width, row.height)),
                    })
                }).collect();
                let controls: Vec<_> = controls(st).into_iter().map(|(_, button, name)| {
                    let frame: NSRect = msg_send![button, frame];
                    let hidden: BOOL = msg_send![button, isHidden];
                    serde_json::json!({"name":name,"identifier":format!("desktop_lyrics_{name}"),"rect":rect_json(frame),"visible":hidden == NO})
                }).collect();
                let unlock_visible: BOOL = msg_send![st.unlock_panel, isVisible];
                let unlock_ignores_mouse: BOOL = msg_send![st.unlock_panel, ignoresMouseEvents];
                let unlock_frame: NSRect = msg_send![st.unlock_panel, frame];
                let clip_width = if st.current_clip == nil { 0.0 } else {
                    let presentation: id = msg_send![st.current_clip, presentationLayer];
                    let layer = if presentation == nil { st.current_clip } else { presentation };
                    let bounds: NSRect = msg_send![layer, bounds]; bounds.size.width
                };
                serde_json::json!({
                    "registered_font_count":FONT_PATHS.get_or_init(||Mutex::new(HashMap::new())).lock().unwrap().len(),
                    "supported":true,"visible":visible!=NO,"window_id":window_id,
                    "locked":st.locked,"ignores_mouse_events":ignores_mouse_events!=NO,"hovered":st.hovered,
                    "playing":st.playing,"paused":st.paused,"favorite":st.favorite,"layout":st.style.layout,
                    "background":st.style.background,"outline":st.style.outline,"shadow":st.style.shadow,"show_translation":st.style.show_translation,
                    "font_family":st.style.font_family,"font_name":st.style.font_name,"font_size":st.style.font_size,"font_weight":st.style.font_weight,
                    "x":frame.origin.x,"y":frame.origin.y,"width":frame.size.width,"height":frame.size.height,
                    "coordinate_system":"AppKit bottom-left, local rects in points",
                    "lines":lines,"controls":controls,"toolbar_rect":rect_json(toolbar_rect(frame.size.width,frame.size.height)),
                    "controls_visible":st.hovered && !st.locked,"highlight_width":clip_width,
                    "unlock_button":{"visible":unlock_visible!=NO,"ignores_mouse_events":unlock_ignores_mouse!=NO,
                        "identifier":"desktop_lyrics_unlock","rect":rect_json(rect(
                            unlock_frame.origin.x-frame.origin.x,unlock_frame.origin.y-frame.origin.y,32.0,32.0))},
                })
            })
        }
        pub fn set_position(&self, x: f64, y: f64) {
            let window = self.window;
            Queue::main().exec_sync(move || unsafe {
                let _: () = msg_send![window as id,setFrameOrigin:NSPoint::new(x,y)];
                constrain_position(window as id);
                send_position(window as id);
            });
        }
        pub fn show(&self, visible: bool) {
            let (view, window) = (self.view, self.window);
            Queue::main().exec_async(move || unsafe {
                if window == 0 {
                    return;
                }
                let w = window as id;
                if visible {
                    let _: () = msg_send![w, orderFront: nil];
                } else {
                    let _: () = msg_send![w, orderOut: nil];
                }
                if view != 0 {
                    let obj = &*(view as *const Object);
                    state_of(obj).visible = visible;
                    set_hovered(view as id, false);
                    update_buttons(view as id);
                    update_hover_timer(view as id);
                }
            });
        }

        pub fn set_lines(&self, current: Option<LyricLine>, next: Option<LyricLine>, start:f64, pos:f64, paused:bool) {
            let view = self.view;
            Queue::main().exec_async(move || unsafe {
                if view == 0 {
                    return;
                }
                let obj = &*(view as *const Object);
                let st = state_of(obj);
                st.song_pos=pos;st.pos_at=Instant::now();st.paused=paused;
                st.line_start_pos=start;
                st.current = current;
                st.next = next;
                rebuild_all(view as id);
            });
        }

        pub fn set_progress(&self, pos: f64, paused: bool) {
            let view = self.view;
            Queue::main().exec_async(move || unsafe {
                if view == 0 {
                    return;
                }
                let obj = &*(view as *const Object);
                let st = state_of(obj);
                st.song_pos = pos;
                st.pos_at = Instant::now();
                st.paused = paused;
                apply_timeline(view as id);
            });
        }

        pub fn set_playing(&self, playing: bool) {
            let view = self.view;
            Queue::main().exec_async(move || unsafe {
                if view == 0 {
                    return;
                }
                let obj = &*(view as *const Object);
                state_of(obj).playing = playing;
                update_buttons(view as id);
            });
        }

        pub fn set_favorite(&self, fav: bool) {
            let view = self.view;
            Queue::main().exec_async(move || unsafe {
                if view == 0 {
                    return;
                }
                let obj = &*(view as *const Object);
                state_of(obj).favorite = fav;
                update_buttons(view as id);
            });
        }

        pub fn set_locked(&self, locked: bool) {
            let view = self.view;
            Queue::main().exec_async(move || unsafe {
                if view == 0 {
                    return;
                }
                let obj = &*(view as *const Object);
                let st = state_of(obj);
                st.locked = locked;
                st.hovered = false;
                st.dragging = false;
                let _: () = msg_send![st.window as id, setIgnoresMouseEvents: if locked { YES } else { NO }];
                update_buttons(view as id);
                update_hover_timer(view as id);
            });
        }

        pub fn set_style(&self, style: Style) {
            let view = self.view;
            Queue::main().exec_async(move || unsafe {
                if view == 0 {
                    return;
                }
                let obj = &*(view as *const Object);
                state_of(obj).style = style;
                // 整棵重建（文字样式）+ 按钮跟随新排版对齐 + 窗口背景。
                rebuild_all(view as id);
                update_buttons(view as id);
                apply_background(view as id);
            });
        }

        pub fn pick_font(&self, family: String, name: String, size: f64, weight: f64) -> bool {
            let view = self.view;
            Queue::main().exec_sync(move || unsafe {
                let font = selected_font(&family, &name, size.clamp(14.0, 60.0), weight);
                let manager: id = msg_send![class!(NSFontManager), sharedFontManager];
                let _: () = msg_send![manager, setTarget: view as id];
                let _: () = msg_send![manager, setAction: sel!(changeFont:)];
                let _: () = msg_send![manager, setSelectedFont: font isMultiple: NO];
                let panel: id = msg_send![manager, fontPanel: YES];
                let _: () = msg_send![panel, setTitle: ns_string("桌面歌词字体")];
                let _: () = msg_send![panel, setHidesOnDeactivate: NO];
                let app: id = msg_send![class!(NSApplication), sharedApplication];
                let _: () = msg_send![app, activateIgnoringOtherApps: YES];
                let _: () = msg_send![manager, orderFrontFontPanel: nil];
                true
            })
        }

        /// 把一个字体文件注册给 AppKit / CoreText，并把 Dart 族名映射到真实族名。
        /// 字体文件在 Flutter 侧由 `FontLoader` 管理，原生窗口默认看不到这些字体。
        pub fn register_font(&self, family: String, path: String) {
            Queue::main().exec_sync(move || unsafe {
                register_native_font(&family, &path);
            });
        }
        pub fn unregister_font(&self, path: String) -> Result<bool, String> {
            Queue::main().exec_sync(move || unsafe {
                let mut paths = FONT_PATHS
                    .get_or_init(|| Mutex::new(HashMap::new()))
                    .lock()
                    .unwrap();
                let Some(family) = paths.get(&path).cloned() else {
                    return Ok(true);
                };
                let url: id = msg_send![class!(NSURL),fileURLWithPath:ns_string(&path)];
                let mut error: id = nil;
                let ok = CTFontManagerUnregisterFontsForURL(url, 1, &mut error);
                let message = if error != nil {
                    let description: id = msg_send![error, description];
                    let text = rust_string(description);
                    CFRelease(error);
                    text
                } else {
                    "Font is still in use".into()
                };
                if !ok {
                    return Err(message);
                }
                paths.remove(&path);
                if !paths.values().any(|f| f == &family) {
                    font_family_map().lock().unwrap().remove(&family);
                }
                Ok(true)
            })
        }
    }

    impl ViewState {
        fn current_pos(&self) -> f64 {
            if self.paused {
                self.song_pos
            } else {
                self.song_pos + self.pos_at.elapsed().as_secs_f64()
            }
        }
    }

    pub fn install(tx: Sender<DesktopLyricsEvent>, position:Option<Position>) -> DesktopLyricsHandle {
        unsafe {
            let style_mask = NS_BORDERLESS | NS_NONACTIVATING_PANEL;
            let content = rect(0.0, 0.0, WINDOW_W, WINDOW_H);

            let panel: id = msg_send![class!(NSPanel), alloc];
            let panel: id = msg_send![panel,
                initWithContentRect: content
                styleMask: style_mask
                backing: NS_BACKING_BUFFERED
                defer: NO];

            let _: () = msg_send![panel, setLevel: NS_FLOATING_LEVEL];
            let _: () = msg_send![panel, setOpaque: NO];
            let _: () = msg_send![panel, setBackgroundColor: clear_color()];
            // 必须关掉窗口阴影：边框less + 非不透明窗口的阴影会在窗口服务里缓存文字
            // 快照，造成换句后「旧句残留」（本功能历史上最大的一个坑）。
            let _: () = msg_send![panel, setHasShadow: NO];
            let _: () = msg_send![panel, setMovableByWindowBackground: NO];
            let collection =
                COLLECTION_CAN_JOIN_ALL_SPACES | COLLECTION_STATIONARY | COLLECTION_FULLSCREEN_AUX;
            let _: () = msg_send![panel, setCollectionBehavior: collection];
            let _: () = msg_send![panel, setHidesOnDeactivate: NO];
            let _: () = msg_send![panel, setAcceptsMouseMovedEvents: YES];

            place_initial(panel);
            if let Some(position)=position {
                constrain_to_position(panel,position,WINDOW_H);
            }
            constrain_position(panel);

            let view = new_view(content);
            let host: id = msg_send![view, layer];
            let _: () = msg_send![host, setMasksToBounds: YES];

            // Optional native material sits beneath text and controls.
            let glass = new_glass_view(content);
            configure_glass_backdrop(glass);
            let _: () = msg_send![glass, setHidden: YES];
            let _: () = msg_send![view, addSubview: glass];

            // 文字容器是独立子视图：所有文字图层挂到它的 backing layer。按钮是 view 的
            // 另一个子视图（原生 NSButton），二者兄弟节点，重建文字不会误删按钮。
            let text_view = new_text_container(content);
            let text_host: id = msg_send![text_view, layer];
            let _: () = msg_send![view, addSubview: text_view];

            let st = Box::new(ViewState {
                sender: tx,
                window: panel as usize,
                current: None,
                next: None,
                style: Style {
                    text_color: None,
                    highlight_color: None,
                    outline: true,
                shadow: true,
                    background: "none".to_string(),
                    font_size: 28.0,
                    font_family: String::new(),
                    font_name: String::new(),
                    font_weight: 0.0,
                    layout: "classic".to_string(),
                    show_translation: true,
                },
                song_pos: 0.0,
                pos_at: Instant::now(),
                paused: false,
                locked: false,
                playing: false,
                favorite: false,
                hovered: false,
                visible: false,
                dragging: false,
                drag_start_mouse: NSPoint::new(0.0, 0.0),
                drag_start_origin: NSPoint::new(0.0, 0.0),
                host,
                glass,
                text_host,
                current_clip: nil,
                word_x: Vec::new(),
                current_height: 0.0,
                rows: Vec::new(),
                scroll_layers: Vec::new(),
                line_start_pos: 0.0,
                toolbar: nil,
                btn_prev: nil,
                btn_play: nil,
                btn_next: nil,
                btn_like: nil,
                btn_lock: nil,
                btn_show_app: nil,
                btn_settings: nil,
                btn_close: nil,
                unlock_panel: nil,
                btn_unlock: nil,
                hover_timer: nil,
                #[cfg(debug_assertions)]
                preview_hovered: None,
            });
            let raw = Box::into_raw(st) as *mut c_void;
            let o: &mut Object = &mut *view;
            o.set_ivar("vs", raw);

            install_tracking(view);

            let _: () = msg_send![panel, setContentView: view];

            // 先挂原生按钮（在文字容器之后，保证 z 序在上），再构建文字图层树。
            install_buttons(view);
            install_unlock_panel(view);
            update_buttons(view);
            rebuild_all(view);

            DesktopLyricsHandle {
                view: view as usize,
                window: panel as usize,
            }
        }
    }

    // ---------- objc 类 ----------

    fn view_class() -> &'static Class {
        static CLASS: OnceLock<usize> = OnceLock::new();
        let ptr = *CLASS.get_or_init(|| unsafe { build_view_class() as *const Class as usize });
        unsafe { &*(ptr as *const Class) }
    }

    /// Non-interactive native background container.
    /// 容器本身对鼠标完全透明，拖拽 / 按钮命中继续由 `CjrtDesktopLyricsView`
    /// 与 NSButton 处理。
    fn backdrop_view_class() -> &'static Class {
        static CLASS: OnceLock<usize> = OnceLock::new();
        let ptr =
            *CLASS.get_or_init(|| unsafe { build_backdrop_view_class() as *const Class as usize });
        unsafe { &*(ptr as *const Class) }
    }

    unsafe fn build_backdrop_view_class() -> &'static Class {
        let superclass = class!(NSView);
        let mut decl = ClassDecl::new("CjrtDesktopLyricsBackdrop", superclass)
            .expect("注册 CjrtDesktopLyricsBackdrop 失败");
        decl.add_method(
            sel!(hitTest:),
            backdrop_hit_test as extern "C" fn(&Object, Sel, NSPoint) -> id,
        );
        decl.register()
    }

    extern "C" fn backdrop_hit_test(_: &Object, _: Sel, _point: NSPoint) -> id {
        nil
    }

    /// 文字容器用无命中视图：鼠标事件穿透给底下的桌面歌词视图，拖拽逻辑继续留在
    /// `CjrtDesktopLyricsView` 上；容器本身只负责承载 CATextLayer。
    fn text_container_class() -> &'static Class {
        static CLASS: OnceLock<usize> = OnceLock::new();
        let ptr =
            *CLASS.get_or_init(|| unsafe { build_text_container_class() as *const Class as usize });
        unsafe { &*(ptr as *const Class) }
    }

    unsafe fn build_text_container_class() -> &'static Class {
        let superclass = class!(NSView);
        let mut decl = ClassDecl::new("CjrtDesktopLyricsTextContainer", superclass)
            .expect("注册 CjrtDesktopLyricsTextContainer 失败");
        decl.add_method(
            sel!(hitTest:),
            text_container_hit_test as extern "C" fn(&Object, Sel, NSPoint) -> id,
        );
        decl.register()
    }

    extern "C" fn text_container_hit_test(_: &Object, _: Sel, _point: NSPoint) -> id {
        nil
    }

    unsafe fn build_view_class() -> &'static Class {
        let superclass = class!(NSView);
        let mut decl = ClassDecl::new("CjrtDesktopLyricsView", superclass)
            .expect("注册 CjrtDesktopLyricsView 失败");
        decl.add_ivar::<*mut c_void>("vs");
        decl.add_method(
            sel!(mouseDown:),
            mouse_down as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(mouseDragged:),
            mouse_dragged as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(sel!(mouseUp:), mouse_up as extern "C" fn(&Object, Sel, id));
        decl.add_method(
            sel!(mouseEntered:),
            mouse_entered as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(mouseExited:),
            mouse_exited as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(mouseMoved:),
            mouse_moved as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(updateTrackingAreas),
            update_tracking_areas as extern "C" fn(&Object, Sel),
        );
        decl.add_method(
            sel!(buttonClicked:),
            button_clicked as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(changeFont:),
            change_font as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(validModesForFontPanel:),
            font_panel_modes as extern "C" fn(&Object, Sel, id) -> u64,
        );
        decl.add_method(
            sel!(checkLockedHover:),
            check_locked_hover as extern "C" fn(&Object, Sel, id),
        );
        decl.register()
    }

    #[allow(
        clippy::mut_from_ref,
        reason = "Preserve the reference Objective-C ivar accessor during migration"
    )]
    unsafe fn state_of(view: &Object) -> &mut ViewState {
        let p: *mut c_void = *view.get_ivar("vs");
        &mut *(p as *mut ViewState)
    }

    // ---------- 按钮事件 ----------

    /// Native controls share one action, dispatched to the application event stream.
    extern "C" fn button_clicked(this: &Object, _: Sel, sender: id) {
        unsafe {
            let tag: i64 = msg_send![sender, tag];
            let st = state_of(this);
            let ev = match tag {
                TAG_PREV => Some(DesktopLyricsEvent::Prev),
                TAG_PLAY => Some(DesktopLyricsEvent::Toggle),
                TAG_NEXT => Some(DesktopLyricsEvent::Next),
                TAG_LIKE => Some(DesktopLyricsEvent::ToggleFavorite),
                TAG_LOCK => Some(DesktopLyricsEvent::Lock),
                TAG_SHOW_APP => Some(DesktopLyricsEvent::ShowApp),
                TAG_SETTINGS => Some(DesktopLyricsEvent::Settings),
                TAG_CLOSE => Some(DesktopLyricsEvent::Close),
                TAG_UNLOCK => Some(DesktopLyricsEvent::Unlock),
                _ => None,
            };
            if let Some(ev) = ev {
                let _ = st.sender.send(ev);
            }
        }
    }

    extern "C" fn font_panel_modes(_: &Object, _: Sel, _: id) -> u64 {
        1 | 2 | 4 // Face, size and collections; colors/shadows use dedicated settings.
    }

    extern "C" fn change_font(_this: &Object, _: Sel, manager: id) {
        unsafe {
            let selected: id = msg_send![manager, selectedFont];
            if selected == nil {
                return;
            }
            let font: id = msg_send![manager, convertFont: selected];
            if font == nil {
                return;
            }
            let family: id = msg_send![font, familyName];
            let name: id = msg_send![font, fontName];
            let size: f64 = msg_send![font, pointSize];
            let size = size.clamp(14.0, 60.0);
            let descriptor: id = msg_send![font, fontDescriptor];
            let traits: id = msg_send![descriptor, objectForKey: NSFontTraitsAttribute];
            let weight_number: id = msg_send![traits, objectForKey: NSFontWeightTrait];
            let weight: f64 = if weight_number == nil {
                0.0
            } else {
                msg_send![weight_number, doubleValue]
            };
            let data = serde_json::json!({"font_family":rust_string(family),"font_name":rust_string(name),"font_size":size,"font_weight":weight});
            let normalized:id=msg_send![manager,convertFont:font toSize:size];
            let _:()=msg_send![manager,setSelectedFont:normalized isMultiple:NO];
            let _ = crate::events::emit("desktopLyrics.font_changed", data);
        }
    }

    // ---------- 命中 / 追踪 ----------

    unsafe fn set_hovered(view: id, hovered: bool) {
        let st = state_of(&*view);
        let hovered = hovered && st.visible;
        if st.hovered != hovered {
            st.hovered = hovered;
            update_buttons(view);
        }
    }

    // A click-through NSWindow receives no tracking events. Read only the pointer
    // position while visible and locked; no event interception or accessibility permission.
    extern "C" fn check_locked_hover(this: &Object, _: Sel, _: id) {
        unsafe {
            let st = state_of(this);
            if !st.visible || !st.locked {
                return;
            }
            let frame: NSRect = msg_send![st.window as id, frame];
            let mouse = mouse_location();
            let hovered = mouse.x >= frame.origin.x
                && mouse.x < frame.origin.x + frame.size.width
                && mouse.y >= frame.origin.y
                && mouse.y < frame.origin.y + frame.size.height;
            #[cfg(debug_assertions)]
            let hovered = st.preview_hovered.unwrap_or(hovered);
            set_hovered(this as *const Object as id, hovered);
        }
    }

    unsafe fn update_hover_timer(view: id) {
        let st = state_of(&*view);
        if st.visible && st.locked {
            if st.hover_timer == nil {
                let timer: id = msg_send![class!(NSTimer), timerWithTimeInterval: 0.05f64
                    target: view selector: sel!(checkLockedHover:) userInfo: nil repeats: YES];
                let _: () = msg_send![timer, setTolerance: 0.01f64];
                let run_loop: id = msg_send![class!(NSRunLoop), mainRunLoop];
                let _: () = msg_send![run_loop, addTimer: timer forMode: ns_string("kCFRunLoopCommonModes")];
                st.hover_timer = timer;
            }
        } else if st.hover_timer != nil {
            let timer = st.hover_timer;
            st.hover_timer = nil;
            let _: () = msg_send![timer, invalidate];
        }
    }

    extern "C" fn mouse_entered(this: &Object, _: Sel, _: id) {
        unsafe {
            set_hovered(this as *const Object as id, true);
        }
    }

    extern "C" fn mouse_exited(this: &Object, _: Sel, _: id) {
        unsafe {
            if !state_of(this).locked {
                set_hovered(this as *const Object as id, false);
            }
        }
    }

    extern "C" fn mouse_moved(this: &Object, _: Sel, _: id) {
        unsafe {
            set_hovered(this as *const Object as id, true);
        }
    }

    extern "C" fn update_tracking_areas(this: &Object, _: Sel) {
        unsafe {
            let view = this as *const Object as id;
            let areas: id = msg_send![view, trackingAreas];
            let count: usize = msg_send![areas, count];
            for i in (0..count).rev() {
                let area: id = msg_send![areas, objectAtIndex: i];
                let _: () = msg_send![view, removeTrackingArea: area];
            }
            install_tracking(view);
        }
    }

    extern "C" fn mouse_down(this: &Object, _: Sel, _event: id) {
        unsafe {
            // 按钮是标准 NSButton 子视图，点击由 AppKit 直接派发给 `buttonClicked:`，
            // 不会走到这里。此处只需要处理歌词区的拖拽。
            let st = state_of(this);
            if st.locked {
                return;
            }
            st.dragging = true;
            st.drag_start_mouse = mouse_location();
            let w = st.window as id;
            let frame: NSRect = msg_send![w, frame];
            st.drag_start_origin = frame.origin;
        }
    }

    extern "C" fn mouse_dragged(this: &Object, _: Sel, _event: id) {
        unsafe {
            let st = state_of(this);
            if !st.dragging {
                return;
            }
            let cur = mouse_location();
            let dx = cur.x - st.drag_start_mouse.x;
            let dy = cur.y - st.drag_start_mouse.y;
            let w = st.window as id;
            let mut origin = st.drag_start_origin;
            origin.x += dx;
            origin.y += dy;
            let _: () = msg_send![w, setFrameOrigin: origin];
        }
    }

    extern "C" fn mouse_up(this: &Object, _: Sel, _event: id) {
        unsafe {
            let st = state_of(this);
            if st.dragging {
                constrain_position(st.window as id);
                send_position(st.window as id);
            }
            st.dragging = false;
        }
    }

    unsafe fn mouse_location() -> NSPoint {
        msg_send![class!(NSEvent), mouseLocation]
    }

    fn toolbar_rect(width: f64, height: f64) -> NSRect {
        rect(
            (width - TOOLBAR_W) * 0.5,
            height - TOOLBAR_H - 4.0,
            TOOLBAR_W,
            TOOLBAR_H,
        )
    }

    fn button_rect(tag: i64, width: f64, height: f64) -> NSRect {
        let bar = toolbar_rect(width, height);
        // Playback, favorite, window actions. Six-point separators distinguish groups.
        let offset = match tag {
            TAG_PREV => 6.0,
            TAG_PLAY => 34.0,
            TAG_NEXT => 62.0,
            TAG_LIKE => 96.0,
            TAG_LOCK => 130.0,
            TAG_SHOW_APP => 158.0,
            TAG_SETTINGS => 186.0,
            TAG_CLOSE => 220.0,
            _ => return rect(0.0, 0.0, 0.0, 0.0),
        };
        rect(bar.origin.x + offset, bar.origin.y + 2.0, 28.0, 28.0)
    }

    // ---------- 图层更新 ----------

    /// Transparent by default, with optional solid color or native macOS blur.
    /// Keep the panel shadow disabled: it caches stale lyric snapshots on macOS.
    unsafe fn apply_background(view: id) {
        let st = state_of(&*view);
        let liquid = matches!(st.style.background.as_str(), "liquid" | "blur");
        let _: () = msg_send![st.glass, setHidden: if liquid { NO } else { YES }];
        let background = if st.style.background == "solid" {
            color(0.0, 0.0, 0.0, 0.45)
        } else {
            clear_color()
        };
        let _: () = msg_send![st.window as id, setBackgroundColor: background];
    }

    fn controls(st: &ViewState) -> [(i64, id, &'static str); 8] {
        [
            (TAG_PREV, st.btn_prev, "previous"),
            (TAG_PLAY, st.btn_play, "play_pause"),
            (TAG_NEXT, st.btn_next, "next"),
            (TAG_LIKE, st.btn_like, "favorite"),
            (TAG_LOCK, st.btn_lock, "lock"),
            (TAG_SHOW_APP, st.btn_show_app, "show_app"),
            (TAG_SETTINGS, st.btn_settings, "settings"),
            (TAG_CLOSE, st.btn_close, "close"),
        ]
    }

    unsafe fn update_buttons(view: id) {
        let st = state_of(&*view);
        let show = !st.locked && st.hovered;
        let bounds: NSRect = msg_send![view, bounds];
        begin_tx();
        let _: () =
            msg_send![st.toolbar, setFrame: toolbar_rect(bounds.size.width, bounds.size.height)];
        let _: () = msg_send![st.toolbar, setHidden: if show { NO } else { YES }];
        for (tag, button, _) in controls(st) {
            set_btn_frame(
                button,
                button_rect(tag, bounds.size.width, bounds.size.height),
            );
            set_btn_visible(button, show);
        }
        set_button_symbol(
            st.btn_play,
            if st.playing {
                "pause.fill"
            } else {
                "play.fill"
            },
        );
        set_button_symbol(
            st.btn_like,
            if st.favorite { "heart.fill" } else { "heart" },
        );
        set_button_label(st.btn_play, if st.playing { "暂停" } else { "播放" });
        set_button_label(
            st.btn_like,
            if st.favorite {
                "取消收藏"
            } else {
                "收藏"
            },
        );
        let _: () = msg_send![st.btn_like, setContentTintColor: if st.favorite { color(1.0, 0.48, 0.52, 1.0) } else { color(1.0, 1.0, 1.0, 0.95) }];
        commit_tx();
        update_unlock_panel(view);
    }

    unsafe fn update_unlock_panel(view: id) {
        let st = state_of(&*view);
        if st.unlock_panel == nil {
            return;
        }
        let parent = st.window as id;
        let attached: id = msg_send![st.unlock_panel, parentWindow];
        if st.visible && st.locked && st.hovered {
            let frame: NSRect = msg_send![parent, frame];
            let button = button_rect(TAG_LOCK, frame.size.width, frame.size.height);
            let _: () = msg_send![st.unlock_panel, setFrame: rect(
                frame.origin.x + button.origin.x - 2.0,
                frame.origin.y + button.origin.y - 2.0, 32.0, 32.0) display: YES];
            if attached == nil {
                let _: () = msg_send![parent, addChildWindow: st.unlock_panel ordered: 1i64];
            }
            let _: () = msg_send![st.unlock_panel, orderFront: nil];
        } else {
            if attached != nil {
                let _: () = msg_send![attached, removeChildWindow: st.unlock_panel];
            }
            let _: () = msg_send![st.unlock_panel, orderOut: nil];
        }
    }

    /// 运行时切换图标（同一个 NSButton，避免旧图层/旧按钮副本残留）。
    unsafe fn set_button_symbol(btn: id, name: &str) {
        if btn == nil {
            return;
        }
        let img = button_symbol(name);
        if img != nil {
            let _: () = msg_send![btn, setImage: img];
        }
    }

    unsafe fn set_btn_frame(btn: id, r: NSRect) {
        if btn == nil {
            return;
        }
        let _: () = msg_send![btn, setFrame: r];
    }

    unsafe fn set_btn_visible(btn: id, visible: bool) {
        if btn == nil {
            return;
        }
        let _: () = msg_send![btn, setHidden: if visible { NO } else { YES }];
    }

    // ---------- 文字图层 ----------

    struct ScrollLayer {
        layer: id,
        overflow: f64,
        kind: LineKind,
    }

    /// Hover never reaches this function. All row geometry is computed once from
    /// content + style, with a permanent toolbar reserve above the text.
    unsafe fn rebuild_all(view: id) {
        let st = state_of(&*view);
        if st.text_host == nil {
            return;
        }
        let scale = backing_scale(view);
        let width = content_width(view);
        let layout = layout::build(
            &st.style,
            st.current.as_ref(),
            st.next.as_ref(),
            width,
            |text, size| {
                let font = selected_font(
                    &st.style.font_family,
                    &st.style.font_name,
                    size,
                    st.style.font_weight,
                );
                let measured = measure_size(text, font);
                (measured.width, measured.height)
            },
        );
        // Resizing can re-enter AppKit tracking callbacks. Finish reading the old
        // state before that call, then reacquire it for the layer rebuild.
        resize_window_height(st.window as id, layout.height);
        let st = state_of(&*view);
        begin_tx();
        // Drop raw layer references before removing the owning tree.
        st.current_clip = nil;
        st.scroll_layers.clear();
        st.word_x.clear();
        st.current_height = 0.0;
        let subs: id = msg_send![st.text_host, sublayers];
        if subs != nil {
            let count: usize = msg_send![subs, count];
            for i in (0..count).rev() {
                let layer: id = msg_send![subs, objectAtIndex: i];
                let _: () = msg_send![layer, removeAllAnimations];
                let _: () = msg_send![layer, removeFromSuperlayer];
            }
        }
        for row in &layout.rows {
            let viewport: id = msg_send![class!(CALayer), layer];
            // Extra vertical padding keeps the text shadow away from the clip edge.
            let _: () = msg_send![viewport, setFrame: rect(row.x, row.y - 3.0, row.viewport_width, row.height + 6.0)];
            let _: () = msg_send![viewport, setMasksToBounds: if row.width > row.viewport_width + 0.5 { YES } else { NO }];
            let _: () = msg_send![st.text_host, addSublayer: viewport];
            let content: id = msg_send![class!(CALayer), layer];
            let _: () = msg_send![content, setAnchorPoint: NSPoint::new(0.0, 0.0)];
            let _: () = msg_send![content, setPosition: NSPoint::new(0.0, 3.0)];
            let _: () = msg_send![content, setBounds: rect(0.0, 0.0, row.width, row.height)];
            let _: () = msg_send![viewport, addSublayer: content];
            let font = selected_font(
                &st.style.font_family,
                &st.style.font_name,
                row.font_size,
                st.style.font_weight,
            );
            if row.kind == LineKind::Current {
                build_current_line(st, content, &row.text, font, scale);
            } else {
                let layer = text_layer(
                    &row.text,
                    font,
                    text_color(st),
                    st.style.outline,
                    st.style.shadow,
                    0.0,
                    0.0,
                    scale,
                );
                let _: () = msg_send![content, addSublayer: layer];
            }
            let overflow = (row.width - row.viewport_width).max(0.0);
            if overflow > 0.5 {
                st.scroll_layers.push(ScrollLayer {
                    layer: content,
                    overflow,
                    kind: row.kind,
                });
            }
        }
        st.rows = layout.rows;
        commit_tx();
        update_buttons(view);
        apply_timeline(view);
    }

    unsafe fn build_current_line(
        st: &mut ViewState,
        container: id,
        text: &str,
        font: id,
        scale: f64,
    ) {
        let (x, y) = (0.0, 0.0);
        let base_attrs = text_attrs(font, text_color(st));
        let hi_attrs = text_attrs(font, highlight_color(st));

        let size = text_size(text, base_attrs);
        let tw = size.width;
        let th = size.height.ceil() + 2.0;
        st.current_height = th;

        // Stroke every glyph first, then fill the complete line above it. This
        // covers CJK overlap contours instead of stroking each filled glyph last.
        if st.style.outline {
            let stroke = new_text_layer(attr_string(text, styled_attrs(font)), tw, th, x, y, scale);
            let _: () = msg_send![container, addSublayer: stroke];
        }
        let base = new_text_layer(attr_string(text, base_attrs), tw, th, x, y, scale);
        apply_text_shadow(base, st.style.shadow);
        let _: () = msg_send![container, addSublayer: base];

        let has_words = matches!(&st.current, Some(c) if !c.words.is_empty());
        if has_words {
            measure_words(st, base_attrs);
            let clip: id = msg_send![class!(CALayer), layer];
            let _: () = msg_send![clip, setAnchorPoint: NSPoint::new(0.0, 0.0)];
            let _: () = msg_send![clip, setPosition: NSPoint::new(x, y)];
            let _: () = msg_send![clip, setBounds: rect(0.0, 0.0, 0.0, th)];
            let _: () = msg_send![clip, setMasksToBounds: YES];
            let hi = new_text_layer(attr_string(text, hi_attrs), tw, th, 0.0, 0.0, scale);
            let _: () = msg_send![clip, addSublayer: hi];
            let _: () = msg_send![container, addSublayer: clip];
            st.current_clip = clip;
        }
    }

    unsafe fn text_layer(
        text: &str,
        font: id,
        fg: id,
        outline: bool,
        shadow: bool,
        x: f64,
        y: f64,
        scale: f64,
    ) -> id {
        let attrs = text_attrs(font, fg);
        let size = text_size(text, attrs);
        let group: id = msg_send![class!(CALayer), layer];
        let _: () = msg_send![group, setFrame: rect(x,y,size.width,size.height.ceil()+2.0)];
        if outline {
            let stroke = new_text_layer(attr_string(text,styled_attrs(font)),size.width,size.height.ceil()+2.0,0.,0.,scale);
            let _: () = msg_send![group, addSublayer: stroke];
        }
        let l = new_text_layer(
            attr_string(text, attrs),
            size.width,
            size.height.ceil() + 2.0,
            0.0,
            0.0,
            scale,
        );
        let _: () = msg_send![group, addSublayer: l];
        apply_text_shadow(group, shadow);
        group
    }

    /// 文字可读性效果：在图层上叠加一层柔和黑色投影。投影由 Core Animation 按文字
    /// alpha 计算，不会像自绘按钮那样改变文字/图标的宽高比；关闭「文字阴影」时无此层。
    unsafe fn apply_text_shadow(layer: id, enabled: bool) {
        if !enabled || layer == nil {
            return;
        }
        let cg: id = msg_send![color(0.0, 0.0, 0.0, 0.85), CGColor];
        let _: () = msg_send![layer, setShadowColor: cg];
        let _: () = msg_send![layer, setShadowOpacity: 0.8f32];
        let _: () = msg_send![layer, setShadowRadius: 3.0];
        let _: () = msg_send![layer, setShadowOffset: NSSize::new(0.0, -1.5)];
        // 显式保持 shadow 不被自身 bounds 裁掉（CATextLayer 默认 masksToBounds=NO）。
        let _: () = msg_send![layer, setMasksToBounds: NO];
    }

    /// Karaoke rows pan with song time, so pause and seek use the same clock as
    /// the highlight. Untimed rows make a gentle round trip with a hold at each end.
    unsafe fn apply_scroll_timeline(st: &ViewState) {
        let now: f64 = msg_send![st.host, convertTime: CACurrentMediaTime() fromLayer: nil];
        let position = st.current_pos();
        begin_tx();
        for scroll in &st.scroll_layers {
            let _: () = msg_send![scroll.layer, removeAllAnimations];
            let timed = if scroll.kind == LineKind::Current {
                st.current
                    .as_ref()
                    .and_then(|line| Some((line.words.first()?.0, line.words.last()?.1)))
                    .filter(|(start, end)| end > start)
            } else {
                None
            };
            let (values, times, start, duration, repeat) = if let Some((start, end)) = timed {
                (
                    vec![0.0, 0.0, -scroll.overflow, -scroll.overflow],
                    vec![0.0, 0.12, 0.88, 1.0],
                    start,
                    end - start,
                    false,
                )
            } else {
                let travel = (scroll.overflow / 30.0).max(2.0);
                let duration = travel * 2.0 + 3.0;
                (
                    vec![0.0, 0.0, -scroll.overflow, -scroll.overflow, 0.0],
                    vec![
                        0.0,
                        1.5 / duration,
                        (1.5 + travel) / duration,
                        (3.0 + travel) / duration,
                        1.0,
                    ],
                    st.line_start_pos,
                    duration,
                    true,
                )
            };
            let elapsed = if repeat {
                (position - start).max(0.0) % duration
            } else {
                (position - start).clamp(0.0, duration)
            };
            let fraction = elapsed / duration;
            let mut x = *values.last().unwrap();
            for i in 1..times.len() {
                if fraction <= times[i] {
                    let part =
                        ((fraction - times[i - 1]) / (times[i] - times[i - 1])).clamp(0.0, 1.0);
                    x = values[i - 1] + (values[i] - values[i - 1]) * part;
                    break;
                }
            }
            let _: () = msg_send![scroll.layer, setPosition: NSPoint::new(x, 3.0)];
            if !st.paused {
                let begin = if repeat {
                    now - elapsed
                } else {
                    now - (position - start)
                };
                let animation = keyframe("position.x", &values, &times, begin, duration);
                if repeat {
                    let _: () = msg_send![animation, setRepeatCount: f32::INFINITY];
                }
                let _: () =
                    msg_send![scroll.layer, addAnimation: animation forKey: ns_string("lyric_pan")];
            }
        }
        commit_tx();
    }

    unsafe fn apply_timeline(view: id) {
        let obj = &*(view as *const Object);
        let st = state_of(obj);
        apply_scroll_timeline(st);
        let clip = st.current_clip;
        if clip == nil {
            return;
        }
        let words = match &st.current {
            Some(c) if !c.words.is_empty() => &c.words,
            _ => return,
        };
        if st.word_x.len() != words.len() {
            return;
        }
        let host = st.host;
        let th = st.current_height;

        begin_tx();
        let _: () = msg_send![clip, removeAllAnimations];
        let now: f64 = msg_send![host, convertTime: CACurrentMediaTime() fromLayer: nil];
        let t0 = words[0].0;
        let t1 = words[words.len() - 1].1;
        let span = (t1 - t0).max(1e-3);
        let pos = st.current_pos();

        if st.paused {
            let sung = sung_width(words, &st.word_x, pos);
            let _: () = msg_send![clip, setBounds: rect(0.0, 0.0, sung.max(0.0), th)];
            commit_tx();
            return;
        }

        let n = words.len();
        let mut times = Vec::with_capacity(n * 2 + 2);
        let mut widths = Vec::with_capacity(n * 2 + 2);
        times.push(0.0);
        widths.push(0.0);
        for (i, (ws, we, _)) in words.iter().enumerate() {
            let prev = if i == 0 { 0.0 } else { st.word_x[i - 1] };
            times.push(((ws - t0) / span).clamp(0.0, 1.0));
            widths.push(prev);
            times.push(((we - t0) / span).clamp(0.0, 1.0));
            widths.push(st.word_x[i]);
        }
        times.push(1.0);
        widths.push(st.word_x[n - 1]);
        for i in 1..times.len() {
            if times[i] < times[i - 1] {
                times[i] = times[i - 1];
            }
        }

        let begin = now - (pos - t0);
        let anim = keyframe("bounds.size.width", &widths, &times, begin, span);
        let _: () = msg_send![clip, addAnimation: anim forKey: ns_string("karaoke")];
        let _: () = msg_send![clip, setBounds: rect(0.0, 0.0, st.word_x[n - 1], th)];
        commit_tx();
    }

    unsafe fn sung_width(words: &[(f64, f64, String)], word_x: &[f64], pos: f64) -> f64 {
        let n = words.len();
        for i in 0..n {
            let (start, end, _) = &words[i];
            let prev = if i == 0 { 0.0 } else { word_x[i - 1] };
            if pos < *start {
                return prev;
            }
            if pos < *end {
                let frac = ((pos - start) / (end - start).max(1e-6)).clamp(0.0, 1.0);
                return prev + (word_x[i] - prev) * frac;
            }
        }
        *word_x.last().unwrap()
    }

    unsafe fn measure_words(st: &mut ViewState, attrs: id) {
        st.word_x.clear();
        let mut acc = String::new();
        if let Some(cur) = &st.current {
            for (_, _, t) in &cur.words {
                acc.push_str(t);
                let s = ns_string(&acc);
                let size: NSSize = msg_send![s, sizeWithAttributes: attrs];
                st.word_x.push(size.width);
            }
        }
    }

    // ---------- 小工具 ----------

    #[link(name = "AppKit", kind = "framework")]
    extern "C" {
        static NSFontAttributeName: id;
        static NSForegroundColorAttributeName: id;
        static NSStrokeWidthAttributeName:id;
        static NSStrokeColorAttributeName:id;
        static NSFontFamilyAttribute: id;
        static NSFontTraitsAttribute: id;
        static NSFontWeightTrait: id;
    }

    #[link(name = "CoreText", kind = "framework")]
    extern "C" {
        static kCTFontFamilyNameAttribute: id;
        fn CTFontManagerRegisterFontsForURL(url: id, scope: u32, error: *mut id) -> bool;
        fn CTFontManagerUnregisterFontsForURL(url: id, scope: u32, error: *mut id) -> bool;
        fn CTFontManagerCreateFontDescriptorsFromURL(url: id) -> id;
        fn CTFontDescriptorCopyAttribute(descriptor: id, attribute: id) -> id;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(value: id);
    }
    #[link(name = "QuartzCore", kind = "framework")]
    extern "C" {
        fn CACurrentMediaTime() -> f64;
    }

    unsafe fn ns_string(s: &str) -> id {
        let cstring =
            std::ffi::CString::new(s).unwrap_or_else(|_| std::ffi::CString::new("").unwrap());
        msg_send![class!(NSString), stringWithUTF8String: cstring.as_ptr()]
    }

    /// NSString / CFString → Rust String。
    unsafe fn rust_string(s: id) -> String {
        if s == nil {
            return String::new();
        }
        let c: *const c_char = msg_send![s, UTF8String];
        if c.is_null() {
            return String::new();
        }
        unsafe { CStr::from_ptr(c) }.to_string_lossy().into_owned()
    }

    /// 把字体文件注册到 CoreText 进程域，并记录「Dart 族名 → 文件真实族名」。
    /// Flutter 的 `FontLoader` 只注册进 Skia，原生 AppKit 字体系统看不到。
    unsafe fn register_native_font(family: &str, path: &str) {
        let path_ns = ns_string(path);
        let url: id = msg_send![class!(NSURL), fileURLWithPath: path_ns];
        if url == nil {
            return;
        }
        let mut error: id = nil;
        // 已注册过会返回 false，但后续 descriptor 查询仍可用；不用把 error 当致命错误。
        let registered = CTFontManagerRegisterFontsForURL(url, 1, &mut error);
        if error != nil {
            CFRelease(error);
        }
        if registered {
            FONT_PATHS
                .get_or_init(|| Mutex::new(HashMap::new()))
                .lock()
                .unwrap()
                .insert(path.to_string(), family.to_string());
        }

        let descriptors = CTFontManagerCreateFontDescriptorsFromURL(url);
        if descriptors == nil {
            return;
        }
        let count: usize = msg_send![descriptors, count];
        if count == 0 {
            CFRelease(descriptors);
            return;
        }
        let descriptor: id = msg_send![descriptors, objectAtIndex: 0];
        let cf_family = CTFontDescriptorCopyAttribute(descriptor, kCTFontFamilyNameAttribute);
        let actual_family = rust_string(cf_family);
        if cf_family != nil {
            CFRelease(cf_family);
        }
        CFRelease(descriptors);
        if !actual_family.is_empty() {
            if let Ok(mut map) = font_family_map().lock() {
                map.insert(family.to_string(), actual_family);
            }
        }
    }

    fn rect_json(rect: NSRect) -> serde_json::Value {
        serde_json::json!({"x":rect.origin.x,"y":rect.origin.y,"width":rect.size.width,"height":rect.size.height})
    }

    fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
    }

    unsafe fn content_width(view: id) -> f64 {
        let bounds: NSRect = msg_send![view, bounds];
        bounds.size.width
    }

    unsafe fn resize_window_height(window: id, height: f64) {
        let mut frame: NSRect = msg_send![window, frame];
        if (frame.size.height - height).abs() < 0.5 {
            return;
        }
        // The persisted bottom origin stays fixed; width never follows text length.
        frame.size.height = height;
        let _: () = msg_send![window, setFrame: frame display: YES];
        constrain_position(window);
    }

    unsafe fn new_view(frame: NSRect) -> id {
        let v: id = msg_send![view_class(), alloc];
        let v: id = msg_send![v, initWithFrame: frame];
        let _: () = msg_send![v, setWantsLayer: YES];
        v
    }

    /// 文字容器视图：只承载 CATextLayer，避免清层时碰到工具栏按钮。它使用
    /// `hitTest: -> nil`，让所有鼠标事件穿透到桌面歌词主视图。
    unsafe fn new_text_container(frame: NSRect) -> id {
        let v: id = msg_send![text_container_class(), alloc];
        let v: id = msg_send![v, initWithFrame: frame];
        let _: () = msg_send![v, setWantsLayer: YES];
        // NSViewWidthSizable | NSViewHeightSizable: follow the content rect.
        let _: () = msg_send![v, setAutoresizingMask: 18u64];
        let l: id = msg_send![v, layer];
        let _: () = msg_send![l, setMasksToBounds: YES];
        v
    }

    // ---------- 液态玻璃背景 ----------

    unsafe fn new_glass_view(frame: NSRect) -> id {
        let v: id = msg_send![backdrop_view_class(), alloc];
        let v: id = msg_send![v, initWithFrame: frame];
        let _: () = msg_send![v, setWantsLayer: YES];
        // Keep the glass sized to the compact content rect.
        let _: () = msg_send![v, setAutoresizingMask: 18u64];
        v
    }

    /// AppKit owns the live behind-window material. It adapts to resizing and
    /// display scale without polling the desktop or requiring screen recording.
    unsafe fn configure_glass_backdrop(glass_view: id) {
        let bounds: NSRect = msg_send![glass_view, bounds];
        let effect: id = msg_send![class!(NSVisualEffectView), alloc];
        let effect: id = msg_send![effect, initWithFrame: bounds];
        let _: () = msg_send![effect, setMaterial: GLASS_MATERIAL_HUD_WINDOW];
        let _: () = msg_send![effect, setBlendingMode: 0i64]; // behind window
        let _: () = msg_send![effect, setState: 1i64]; // always active
        let _: () = msg_send![effect, setWantsLayer: YES];
        let _: () = msg_send![effect, setAutoresizingMask: 18u64];
        let appearance: id = msg_send![class!(NSAppearance), appearanceNamed: ns_string("NSAppearanceNameVibrantDark")];
        let _: () = msg_send![effect, setAppearance: appearance];
        let layer: id = msg_send![effect, layer];
        let _: () = msg_send![layer, setCornerRadius: GLASS_CORNER_RADIUS];
        let _: () = msg_send![layer, setMasksToBounds: YES];
        let border: id = msg_send![color(1.0, 1.0, 1.0, 0.12), CGColor];
        let _: () = msg_send![layer, setBorderColor: border];
        let _: () = msg_send![layer, setBorderWidth: 0.5f64];
        let _: () = msg_send![glass_view, addSubview: effect];
        let _: () = msg_send![effect, release];
    }

    unsafe fn clear_color() -> id {
        msg_send![class!(NSColor), clearColor]
    }

    unsafe fn color(r: f64, g: f64, b: f64, a: f64) -> id {
        msg_send![class!(NSColor), colorWithSRGBRed: r green: g blue: b alpha: a]
    }

    unsafe fn text_color(st: &ViewState) -> id {
        match st.style.text_color {
            Some((r, g, b)) => color(r, g, b, 1.0),
            None => color(1.0, 1.0, 1.0, 1.0),
        }
    }

    unsafe fn highlight_color(st: &ViewState) -> id {
        match st.style.highlight_color {
            Some((r, g, b)) => color(r, g, b, 1.0),
            None => color(
                0xAC as f64 / 255.0,
                0x5E as f64 / 255.0,
                0x4F as f64 / 255.0,
                1.0,
            ),
        }
    }

    unsafe fn selected_font(family: &str, name: &str, size: f64, weight: f64) -> id {
        if !name.is_empty() {
            let font: id = msg_send![class!(NSFont), fontWithName: ns_string(name) size: size];
            if font != nil {
                return font;
            }
        }
        make_font(family, size, weight)
    }

    unsafe fn make_font(family: &str, size: f64, weight: f64) -> id {
        if family.is_empty() || family == "system" {
            return msg_send![class!(NSFont), systemFontOfSize: size weight: weight];
        }

        // Flutter 下载/导入的字体经 CoreText 注册后，用真实族名做 descriptor 匹配；
        // 这样既能命中自定义字体，也能按用户选择的字重拿到对应 face。
        let actual_family = font_family_map()
            .lock()
            .ok()
            .and_then(|map| map.get(family).cloned())
            .unwrap_or_else(|| family.to_string());

        let traits: id = msg_send![class!(NSMutableDictionary), dictionary];
        let weight_number: id = msg_send![class!(NSNumber), numberWithDouble: weight];
        let _: () = msg_send![traits, setObject: weight_number forKey: NSFontWeightTrait];

        let attrs: id = msg_send![class!(NSMutableDictionary), dictionary];
        let family_ns = ns_string(&actual_family);
        let _: () = msg_send![attrs, setObject: family_ns forKey: NSFontFamilyAttribute];
        let _: () = msg_send![attrs, setObject: traits forKey: NSFontTraitsAttribute];

        let descriptor: id =
            msg_send![class!(NSFontDescriptor), fontDescriptorWithFontAttributes: attrs];
        if descriptor != nil {
            let matched: id = msg_send![descriptor, matchingFontDescriptorWithMandatoryKeys: nil];
            if matched != nil {
                let f: id = msg_send![class!(NSFont), fontWithDescriptor: matched size: size];
                if f != nil {
                    return f;
                }
            }
        }

        // 兜底：真实族名直接按名字找。
        let f: id = msg_send![class!(NSFont), fontWithName: family_ns size: size];
        if f != nil {
            return f;
        }
        msg_send![class!(NSFont), systemFontOfSize: size weight: weight]
    }

    /// Base metrics stay independent of the separately configured stroke and layer shadow.
    unsafe fn text_attrs(font: id, fg: id) -> id {
        let dict: id = msg_send![class!(NSMutableDictionary), dictionary];
        let _: () = msg_send![dict, setObject: font forKey: NSFontAttributeName];
        let _: () = msg_send![dict, setObject: fg forKey: NSForegroundColorAttributeName];
        dict
    }

    unsafe fn styled_attrs(font:id)->id {
        let attrs=text_attrs(font,color(0.,0.,0.,0.85));
            let stroke:id=msg_send![class!(NSNumber),numberWithDouble:2.0f64];
            let _:()=msg_send![attrs,setObject:stroke forKey:NSStrokeWidthAttributeName];
            let _:()=msg_send![attrs,setObject:color(0.,0.,0.,0.85) forKey:NSStrokeColorAttributeName];
        attrs
    }

    unsafe fn text_size(text: &str, attrs: id) -> NSSize {
        let s = ns_string(text);
        msg_send![s, sizeWithAttributes: attrs]
    }

    /// 仅字体属性的度量（阴影不参与排版度量）。
    unsafe fn measure_size(text: &str, font: id) -> NSSize {
        let dict: id = msg_send![class!(NSMutableDictionary), dictionary];
        let _: () = msg_send![dict, setObject: font forKey: NSFontAttributeName];
        text_size(text, dict)
    }

    unsafe fn attr_string(text: &str, attrs: id) -> id {
        let s = ns_string(text);
        let a: id = msg_send![class!(NSAttributedString), alloc];
        let a: id = msg_send![a, initWithString: s attributes: attrs];
        let a: id = msg_send![a, autorelease];
        a
    }

    unsafe fn new_text_layer(attr: id, tw: f64, th: f64, x: f64, y: f64, scale: f64) -> id {
        let l: id = msg_send![class!(CATextLayer), layer];
        let _: () = msg_send![l, setAnchorPoint: NSPoint::new(0.0, 0.0)];
        let _: () = msg_send![l, setBounds: rect(0.0, 0.0, tw.ceil().max(1.0), th.ceil().max(1.0))];
        let _: () = msg_send![l, setPosition: NSPoint::new(x, y)];
        let _: () = msg_send![l, setContentsScale: scale];
        let _: () = msg_send![l, setWrapped: NO];
        let _: () = msg_send![l, setString: attr];
        l
    }

    unsafe fn begin_tx() {
        let _: () = msg_send![class!(CATransaction), begin];
        let _: () = msg_send![class!(CATransaction), setDisableActions: YES];
    }

    unsafe fn commit_tx() {
        let _: () = msg_send![class!(CATransaction), commit];
        // 立即刷给渲染服务：确保换行时旧层确实从渲染树撤下。
        let _: () = msg_send![class!(CATransaction), flush];
    }

    unsafe fn ns_numbers(vals: &[f64]) -> id {
        let arr: id = msg_send![class!(NSMutableArray), array];
        for v in vals {
            let n: id = msg_send![class!(NSNumber), numberWithDouble: *v];
            let _: () = msg_send![arr, addObject: n];
        }
        arr
    }

    unsafe fn keyframe(key: &str, values: &[f64], key_times: &[f64], begin: f64, dur: f64) -> id {
        let a: id = msg_send![class!(CAKeyframeAnimation), animationWithKeyPath: ns_string(key)];
        let _: () = msg_send![a, setValues: ns_numbers(values)];
        let _: () = msg_send![a, setKeyTimes: ns_numbers(key_times)];
        let _: () = msg_send![a, setCalculationMode: ns_string("linear")];
        let _: () = msg_send![a, setBeginTime: begin];
        let _: () = msg_send![a, setDuration: dur.max(1e-3)];
        let _: () = msg_send![a, setFillMode: ns_string("both")];
        let _: () = msg_send![a, setRemovedOnCompletion: NO];
        a
    }

    unsafe fn backing_scale(view: id) -> f64 {
        let win: id = msg_send![view, window];
        if win != nil {
            let s: f64 = msg_send![win, backingScaleFactor];
            if s > 0.5 {
                return s;
            }
        }
        2.0
    }

    unsafe fn send_position(panel:id) {
        let frame:NSRect=msg_send![panel,frame];
        let view:id=msg_send![panel,contentView];
        let st=state_of(&*view);
        let _=st.sender.send(DesktopLyricsEvent::Position(Position {center_x:frame.origin.x+frame.size.width/2.,y:frame.origin.y,width:frame.size.width}));
    }
    unsafe fn constrain_position(panel:id) {
        let frame:NSRect=msg_send![panel,frame];
        constrain_to_position(panel,Position{center_x:frame.origin.x+frame.size.width/2.,y:frame.origin.y,width:frame.size.width},frame.size.height);
    }
    unsafe fn constrain_to_position(panel:id,position:Position,height:f64) {
        use super::geometry::{self,Frame};
        let screens:id=msg_send![class!(NSScreen),screens];
        let count:usize=msg_send![screens,count];
        let mut displays=Vec::with_capacity(count);
        for i in 0..count {
            let screen:id=msg_send![screens,objectAtIndex:i];
            let r:NSRect=msg_send![screen,visibleFrame];
            displays.push(Frame{x:r.origin.x,y:r.origin.y,width:r.size.width,height:r.size.height});
        }
        let f=geometry::constrain(position,height,&displays);
        let _:()=msg_send![panel,setFrame:rect(f.x,f.y,f.width,f.height) display:NO];
    }
    unsafe fn place_initial(panel: id) {
        let scr: id = msg_send![class!(NSScreen), mainScreen];
        if scr == nil {
            let _: () = msg_send![panel, center];
            return;
        }
        let frame: NSRect = msg_send![scr, visibleFrame];
        let x = frame.origin.x + (frame.size.width - WINDOW_W) / 2.0;
        let y = frame.origin.y + 32.0;
        let _: () = msg_send![panel, setFrame: rect(x, y, WINDOW_W, WINDOW_H) display: NO];
    }

    unsafe fn sf_symbol(name: &str) -> id {
        let n = ns_string(name);
        msg_send![class!(NSImage), imageWithSystemSymbolName: n accessibilityDescription: nil]
    }

    /// 给 NSButton 用的 SF Symbol：按统一 point size 配置，保留符号自身宽高比。
    /// 不再 `lockFocus` 画进 19x19 的位图——那会把 `forward.fill` 这类横向符号横向压扁。
    unsafe fn button_symbol(name: &str) -> id {
        let symbol = sf_symbol(name);
        if symbol == nil {
            return nil;
        }
        let cfg: id = msg_send![class!(NSImageSymbolConfiguration),
            configurationWithPointSize: SYMBOL_SIZE
            weight: 0.0];
        msg_send![symbol, imageWithSymbolConfiguration: cfg]
    }

    /// 标准无边框 NSButton 图标按钮：模板图 + 按比例缩放，由 AppKit 原生绘制。
    unsafe fn add_native_button(view: id, symbol_name: &str, frame: NSRect, tag: i64) -> id {
        let btn: id = msg_send![class!(NSButton), alloc];
        let btn: id = msg_send![btn, initWithFrame: frame];
        let img = button_symbol(symbol_name);
        if img != nil {
            let _: () = msg_send![btn, setImage: img];
        }
        let _: () = msg_send![btn, setImagePosition: 1u64]; // NSImageOnly
        let _: () = msg_send![btn, setImageScaling: 2u64]; // NSImageScaleNone: preserve configured symbol size
        let _: () = msg_send![btn, setBordered: NO];
        let _: () = msg_send![btn, setButtonType: 5u64]; // NSMomentaryChange
        let _: () = msg_send![btn, setTarget: view];
        let _: () = msg_send![btn, setAction: sel!(buttonClicked:)];
        let _: () = msg_send![btn, setTag: tag];
        let tint = color(1.0, 1.0, 1.0, 0.95);
        let _: () = msg_send![btn, setContentTintColor: tint];
        let _: () = msg_send![btn, setFocusRingType: 1u64]; // NSFocusRingTypeNone
        let _: () = msg_send![btn, setHidden: YES];
        let _: () = msg_send![view, addSubview: btn];
        btn
    }

    unsafe fn set_button_label(button: id, label: &str) {
        let label = ns_string(label);
        let _: () = msg_send![button, setToolTip: label];
        let _: () = msg_send![button, setAccessibilityLabel: label];
    }

    unsafe fn style_control_card(layer: id) -> id {
        let background: id = msg_send![color(0.09, 0.10, 0.12, 0.94), CGColor];
        let border: id = msg_send![color(1.0, 1.0, 1.0, 0.12), CGColor];
        let _: () = msg_send![layer, setBackgroundColor: background];
        let _: () = msg_send![layer, setCornerRadius: 8.0f64];
        let _: () = msg_send![layer, setBorderWidth: 0.5f64];
        let _: () = msg_send![layer, setBorderColor: border];
        border
    }

    unsafe fn install_buttons(view: id) {
        let st = state_of(&*view);
        // A sibling view puts the card behind all controls, independently of text.
        let card = new_glass_view(rect(0.0, 0.0, TOOLBAR_W, TOOLBAR_H));
        let _: () = msg_send![card, setAutoresizingMask: 0u64];
        let layer: id = msg_send![card, layer];
        let border = style_control_card(layer);
        let _: () = msg_send![view, addSubview: card];
        st.toolbar = card;
        for x in [96.0 - 3.0, 130.0 - 3.0, 220.0 - 3.0] {
            let separator: id = msg_send![class!(CALayer), layer];
            let _: () = msg_send![separator, setFrame: rect(x, 9.0, 0.5, 14.0)];
            let _: () = msg_send![separator, setBackgroundColor: border];
            let _: () = msg_send![layer, addSublayer: separator];
        }
        let frame = rect(0.0, 0.0, 28.0, 28.0);
        st.btn_prev = add_native_button(view, "backward.fill", frame, TAG_PREV);
        st.btn_play = add_native_button(view, "play.fill", frame, TAG_PLAY);
        st.btn_next = add_native_button(view, "forward.fill", frame, TAG_NEXT);
        st.btn_like = add_native_button(view, "heart", frame, TAG_LIKE);
        st.btn_lock = add_native_button(view, "lock.fill", frame, TAG_LOCK);
        st.btn_show_app = add_native_button(view, "macwindow", frame, TAG_SHOW_APP);
        st.btn_settings = add_native_button(view, "gearshape", frame, TAG_SETTINGS);
        st.btn_close = add_native_button(view, "xmark", frame, TAG_CLOSE);
        for ((_, button, identifier), label) in controls(st).into_iter().zip([
            "上一首",
            "播放",
            "下一首",
            "收藏",
            "锁定歌词",
            "显示主窗口",
            "歌词设置",
            "关闭桌面歌词",
        ]) {
            set_button_label(button, label);
            let _: () = msg_send![button, setAccessibilityIdentifier: ns_string(&format!("desktop_lyrics_{identifier}"))];
        }
    }

    unsafe fn install_unlock_panel(view: id) {
        let st = state_of(&*view);
        let panel: id = msg_send![class!(NSPanel), alloc];
        let panel: id = msg_send![panel, initWithContentRect: rect(0.0, 0.0, 32.0, 32.0)
            styleMask: NS_BORDERLESS | NS_NONACTIVATING_PANEL backing: NS_BACKING_BUFFERED defer: NO];
        let _: () = msg_send![panel, setLevel: NS_FLOATING_LEVEL];
        let _: () = msg_send![panel, setOpaque: NO];
        let _: () = msg_send![panel, setBackgroundColor: clear_color()];
        let _: () = msg_send![panel, setHasShadow: NO];
        let _: () = msg_send![panel, setHidesOnDeactivate: NO];
        let _: () = msg_send![panel, setCollectionBehavior:
            COLLECTION_CAN_JOIN_ALL_SPACES | COLLECTION_STATIONARY | COLLECTION_FULLSCREEN_AUX];
        let content: id = msg_send![class!(NSView), alloc];
        let content: id = msg_send![content, initWithFrame: rect(0.0, 0.0, 32.0, 32.0)];
        let _: () = msg_send![content, setWantsLayer: YES];
        let layer: id = msg_send![content, layer];
        style_control_card(layer);
        let _: () = msg_send![panel, setContentView: content];
        let button = add_native_button(
            content,
            "lock.open.fill",
            rect(0.0, 0.0, 32.0, 32.0),
            TAG_UNLOCK,
        );
        let _: () = msg_send![button, setTarget: view];
        let _: () = msg_send![button, setHidden: NO];
        set_button_label(button, "解锁桌面歌词");
        let _: () =
            msg_send![button, setAccessibilityIdentifier: ns_string("desktop_lyrics_unlock")];
        st.unlock_panel = panel;
        st.btn_unlock = button;
        let _: () = msg_send![content, release];
    }

    unsafe fn install_tracking(view: id) {
        let area: id = msg_send![class!(NSTrackingArea), alloc];
        let area: id = msg_send![area,
            initWithRect: rect(0.0, 0.0, WINDOW_W, WINDOW_H)
            options: TRACKING_OPTS
            owner: view
            userInfo: nil];
        let _: () = msg_send![view, addTrackingArea: area];
    }
}

pub use imp::{install, DesktopLyricsHandle};
