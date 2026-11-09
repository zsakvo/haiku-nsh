//! 桌面歌词浮窗的 FRB 暴露面。
//!
//! 全部为顶层函数（模块级单例，不做 opaque 对象）；非 macOS 编译为 no-op
//! （[`desktop_lyrics_show`] 返回 `false`，其余 setter 因单例未装而静默）。窗口本体在
//! [`crate::desktop_lyrics`]，本模块只负责：主线程派发安装、把按钮事件转成 FRB
//! [`DesktopLyricsEvent`] 推给 Dart、把 Dart 下发的歌词/进度/样式同步给窗口句柄。

use std::sync::mpsc::Receiver;
use std::sync::{Mutex, OnceLock};

use crate::desktop_lyrics::DesktopLyricsHandle;
use haiku_model::events::EventSink;

/// 桌面歌词窗口事件类型。无字段枚举，FRB 生成普通 Dart enum（不依赖 freezed）。
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopLyricsEventKind {
    /// 上一首。
    Prev,
    /// 播放 / 暂停切换。
    Toggle,
    /// 下一首。
    Next,
    /// 收藏态切换（红心）。
    ToggleFavorite,
    /// 点击锁定按钮。
    Lock,
    /// 点击解锁按钮。
    Unlock,
    /// 回到主窗口。
    ShowApp,
    /// 打开桌面歌词设置。
    Settings,
    /// 隐藏桌面歌词，播放继续。
    Close,
}

/// 推给 Dart 的桌面歌词事件。无载荷，仅 [kind]。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct DesktopLyricsEvent {
    pub kind: DesktopLyricsEventKind,
}

impl DesktopLyricsEvent {
    fn from(ev: crate::desktop_lyrics::DesktopLyricsEvent) -> Self {
        let kind = match ev {
            crate::desktop_lyrics::DesktopLyricsEvent::Prev => DesktopLyricsEventKind::Prev,
            crate::desktop_lyrics::DesktopLyricsEvent::Toggle => DesktopLyricsEventKind::Toggle,
            crate::desktop_lyrics::DesktopLyricsEvent::Next => DesktopLyricsEventKind::Next,
            crate::desktop_lyrics::DesktopLyricsEvent::ToggleFavorite => {
                DesktopLyricsEventKind::ToggleFavorite
            }
            crate::desktop_lyrics::DesktopLyricsEvent::Lock => DesktopLyricsEventKind::Lock,
            crate::desktop_lyrics::DesktopLyricsEvent::Unlock => DesktopLyricsEventKind::Unlock,
            crate::desktop_lyrics::DesktopLyricsEvent::ShowApp => DesktopLyricsEventKind::ShowApp,
            crate::desktop_lyrics::DesktopLyricsEvent::Settings => DesktopLyricsEventKind::Settings,
            crate::desktop_lyrics::DesktopLyricsEvent::Close => DesktopLyricsEventKind::Close,
        };
        DesktopLyricsEvent { kind }
    }
}

/// 逐字歌词的一个字（绝对时间轴）。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct DesktopLyricsWord {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// 一行歌词（当前行 / 下一行）。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct DesktopLyricsLine {
    pub text: String,
    /// 行级译文；空 = 无翻译。
    pub translation: String,
    pub words: Vec<DesktopLyricsWord>,
}

/// 外观配置（由 Dart 下发）。颜色为 0xRRGGBB；`None` = 跟随默认。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct DesktopLyricsStyle {
    pub text_color: Option<u32>,
    pub highlight_color: Option<u32>,
    pub outline: bool,
    /// `none` | `liquid` | `solid`。旧值 `blur` 由 Dart 迁移为 `liquid`，Rust 侧
    /// 也会把 `blur` 当 `liquid` 处理以兼容未迁移调用方。
    pub background: String,
    pub font_size: f64,
    pub font_family: String,
    #[serde(default)]
    pub font_name: String,
    /// NSFontWeight 数值（0=Regular、0.4=Bold）。
    pub font_weight: f64,
    /// `classic` | `focus` | `left` | `stage`（排版方案，见 `crate::desktop_lyrics` 模块头）。
    pub layout: String,
    pub show_translation: bool,
}

/// 已安装的窗口句柄（进程级单例）。非 macOS 上始终为空。
static HANDLE: OnceLock<DesktopLyricsHandle> = OnceLock::new();
/// 事件接收端，等 [`desktop_lyrics_events`] 首次订阅时取走、在常驻转发线程持有。
static EVENTS_RX: Mutex<Option<Receiver<crate::desktop_lyrics::DesktopLyricsEvent>>> =
    Mutex::new(None);
/// 当前活跃的 Dart 事件流。Dart 重新订阅时替换（热重载 / provider 重建后旧流已死）。
static EVENTS_SINK: Mutex<Option<EventSink<DesktopLyricsEvent>>> = Mutex::new(None);

/// 懒安装（幂等）：首次调用在主线程创建窗口并记录事件接收端。
fn ensure_installed() {
    #[cfg(target_os = "macos")]
    {
        if HANDLE.get().is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel::<crate::desktop_lyrics::DesktopLyricsEvent>();
        // AppKit 只能在主线程操作；FRB 调用来自 worker 线程，用主队列 sync 派发安装。
        let handle = dispatch::Queue::main().exec_sync(move || crate::desktop_lyrics::install(tx));
        if HANDLE.set(handle).is_err() {
            return; // 竞态下已被另一次安装抢先设置。
        }
        *EVENTS_RX.lock().unwrap() = Some(rx);
    }
}

/// 显示/隐藏桌面歌词窗口。首次调用懒创建（幂等）。非 macOS 返回 `false`。
pub fn desktop_lyrics_show(visible: bool) -> bool {
    #[cfg(target_os = "macos")]
    {
        ensure_installed();
        if let Some(h) = HANDLE.get() {
            h.show(visible);
            true
        } else {
            false
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = visible;
        false
    }
}

/// Open the non-modal system font panel. Selections emit desktopLyrics.font_changed.
pub fn desktop_lyrics_pick_font(
    family: String,
    name: String,
    size: f64,
    weight: f64,
) -> Result<bool, String> {
    if !size.is_finite() || !weight.is_finite() {
        return Err("Font size and weight must be finite".into());
    }
    ensure_installed();
    Ok(HANDLE
        .get()
        .map(|h| h.pick_font(family, name, size, weight))
        .unwrap_or(false))
}

/// 订阅桌面歌词事件流（按钮点击）。可重复订阅：新流替换旧流成为转发目标。
pub fn desktop_lyrics_events(sink: EventSink<DesktopLyricsEvent>) {
    *EVENTS_SINK.lock().unwrap() = Some(sink);
    let Some(rx) = EVENTS_RX.lock().unwrap().take() else {
        return;
    };
    std::thread::spawn(move || {
        while let Ok(ev) = rx.recv() {
            if let Some(sink) = EVENTS_SINK.lock().unwrap().as_ref() {
                let _ = sink.add(DesktopLyricsEvent::from(ev));
            }
        }
    });
}

/// 设置当前行 / 下一行（`None` 表示该行无内容）。
pub fn desktop_lyrics_set_lines(
    current: Option<DesktopLyricsLine>,
    next: Option<DesktopLyricsLine>,
) {
    if let Some(h) = HANDLE.get() {
        h.set_lines(current.map(to_line), next.map(to_line));
    }
}

fn to_line(l: DesktopLyricsLine) -> crate::desktop_lyrics::LyricLine {
    crate::desktop_lyrics::LyricLine {
        text: l.text,
        translation: l.translation,
        words: l
            .words
            .into_iter()
            .map(|w| (w.start, w.end, w.text))
            .collect(),
    }
}

/// 同步播放进度锚点（每 tick 调用），驱动逐字高亮外推。
pub fn desktop_lyrics_set_progress(pos: f64, paused: bool) {
    if let Some(h) = HANDLE.get() {
        h.set_progress(pos, paused);
    }
}

/// 播放态 → 切换播放/暂停图标。
pub fn desktop_lyrics_set_playing(playing: bool) {
    if let Some(h) = HANDLE.get() {
        h.set_playing(playing);
    }
}

/// 收藏态 → 切换实心/空心红心。
pub fn desktop_lyrics_set_favorite(fav: bool) {
    if let Some(h) = HANDLE.get() {
        h.set_favorite(fav);
    }
}

/// 锁定态切换。
/// 把一个字体文件注册给原生桌面歌词窗口。Flutter 的 `FontLoader` 只注册进 Skia，
/// AppKit / CoreText 看不到，因此选中下载/导入字体时桥接层会把对应文件路径送过来。
/// 非 macOS 或窗口尚未安装时为 no-op。
pub fn desktop_lyrics_register_font(family: String, path: String) {
    #[cfg(target_os = "macos")]
    {
        ensure_installed();
        if let Some(h) = HANDLE.get() {
            h.register_font(family, path);
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (family, path);
    }
}

pub fn desktop_lyrics_set_locked(locked: bool) {
    if let Some(h) = HANDLE.get() {
        h.set_locked(locked);
    }
}

/// 下发外观配置。
pub fn desktop_lyrics_set_style(style: DesktopLyricsStyle) {
    if let Some(h) = HANDLE.get() {
        h.set_style(crate::desktop_lyrics::Style {
            text_color: style.text_color.map(rgb),
            highlight_color: style.highlight_color.map(rgb),
            outline: style.outline,
            background: style.background,
            font_size: style.font_size,
            font_family: style.font_family,
            font_name: style.font_name,
            font_weight: style.font_weight,
            layout: style.layout,
            show_translation: style.show_translation,
        });
    }
}

/// 0xRRGGBB → sRGB 分量 0..1。
fn rgb(v: u32) -> (f64, f64, f64) {
    (
        ((v >> 16) & 0xFF) as f64 / 255.0,
        ((v >> 8) & 0xFF) as f64 / 255.0,
        (v & 0xFF) as f64 / 255.0,
    )
}

/// Native window state for lifecycle persistence verification and parent controls.
pub fn desktop_lyrics_snapshot() -> serde_json::Value {
    ensure_installed();
    HANDLE
        .get()
        .map(|h| h.snapshot())
        .unwrap_or(serde_json::json!({"supported":false}))
}
/// Move the lyric panel in AppKit screen coordinates; clamp to connected displays and persist.
pub fn desktop_lyrics_set_position(x: f64, y: f64) -> Result<(), String> {
    if !x.is_finite() || !y.is_finite() {
        return Err("Position must be finite".into());
    }
    ensure_installed();
    if let Some(h) = HANDLE.get() {
        h.set_position(x, y);
    }
    Ok(())
}

/// Unregister a previously registered font file before replacing a preview subset.
pub fn desktop_lyrics_unregister_font(path: String) -> Result<bool, String> {
    if let Some(h) = HANDLE.get() {
        h.unregister_font(path)
    } else {
        Ok(true)
    }
}
