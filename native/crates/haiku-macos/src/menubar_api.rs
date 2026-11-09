//! macOS 顶栏歌词组件的 FRB 暴露面。
//!
//! 全部为顶层函数（模块级单例，不做 opaque 对象）；非 macOS 编译为 no-op
//! （[`menubar_install`] 返回 `false`，其余 setter 因单例未装而静默）。组件本体在
//! [`crate::menubar`]，本模块只负责：主线程派发安装、把按钮/菜单事件转成 FRB
//! [`MenubarEvent`] 推给 Dart、把 Dart 下发的歌词/图标/菜单同步给组件句柄。

use std::sync::mpsc::Receiver;
use std::sync::{Mutex, OnceLock};

use crate::menubar::{MenuBarEvent, MenuBarHandle, MenuItemSpec};
use haiku_model::events::EventSink;

/// 菜单栏事件类型。无字段枚举，FRB 生成普通 Dart enum（不依赖 freezed）。
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MenubarEventKind {
    /// 上一首。
    Prev,
    /// 播放 / 暂停切换。
    Toggle,
    /// 下一首。
    Next,
    /// 收藏态切换（红心）。
    ToggleFavorite,
    /// 右键菜单项点击（`menu_item_id` 为该项 id）。
    MenuItem,
}

/// 推给 Dart 的菜单栏事件。用「类型 + 载荷字段」而非带数据枚举，避免 FRB 走 freezed
/// （与 音频引擎 PlaybackEvent 一致）。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MenubarEvent {
    pub kind: MenubarEventKind,
    /// `MenuItem` 时为菜单项 id；其余为空串。
    pub menu_item_id: String,
}

impl MenubarEvent {
    fn from(ev: MenuBarEvent) -> Self {
        match ev {
            MenuBarEvent::Prev => MenubarEvent {
                kind: MenubarEventKind::Prev,
                menu_item_id: String::new(),
            },
            MenuBarEvent::Toggle => MenubarEvent {
                kind: MenubarEventKind::Toggle,
                menu_item_id: String::new(),
            },
            MenuBarEvent::Next => MenubarEvent {
                kind: MenubarEventKind::Next,
                menu_item_id: String::new(),
            },
            MenuBarEvent::ToggleFavorite => MenubarEvent {
                kind: MenubarEventKind::ToggleFavorite,
                menu_item_id: String::new(),
            },
            MenuBarEvent::MenuItem(id) => MenubarEvent {
                kind: MenubarEventKind::MenuItem,
                menu_item_id: id,
            },
        }
    }
}

/// 逐字歌词的一个字（绝对时间轴）。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MenubarWord {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// 右键菜单项（由 Dart 下发）。
///
/// `children` 非空时渲染为带子菜单的父项；`is_separator` 为真时忽略其它字段。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MenubarMenuItem {
    pub id: String,
    pub title: String,
    pub enabled: bool,
    pub is_separator: bool,
    pub children: Vec<MenubarMenuItem>,
}

/// 已安装的组件句柄（进程级单例）。非 macOS 上始终为空（install 返回 false）。
static HANDLE: OnceLock<MenuBarHandle> = OnceLock::new();
/// 事件接收端，等 [`menubar_events`] 首次订阅时取走、在常驻转发线程持有。
static EVENTS_RX: Mutex<Option<Receiver<MenuBarEvent>>> = Mutex::new(None);
/// 当前活跃的 Dart 事件流。转发线程每收到一个事件读一次「当前」sink——
/// Dart 侧重新订阅（热重载 / provider 重建后旧流已死）只需替换这里，
/// 转发线程与按钮事件通道全程不中断。
static EVENTS_SINK: Mutex<Option<EventSink<MenubarEvent>>> = Mutex::new(None);

/// 安装菜单栏组件（幂等）。macOS 主线程派发创建组件；成功或已装返回 `true`，非 macOS 返回 `false`。
pub fn menubar_install(display_name: String) -> bool {
    #[cfg(target_os = "macos")]
    {
        if HANDLE.get().is_some() {
            return true; // 已装，幂等
        }
        let (tx, rx) = std::sync::mpsc::channel::<MenuBarEvent>();
        // AppKit 只能在主线程操作；FRB 调用来自 worker 线程，用主队列 sync 派发安装。
        let handle =
            dispatch::Queue::main().exec_sync(move || crate::menubar::install(&display_name, tx));
        if HANDLE.set(handle).is_err() {
            // 竞态下已被另一次 install 抢先设置：本次创建的组件作废（leak，无害）。
            return true;
        }
        *EVENTS_RX.lock().unwrap() = Some(rx);
        true
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = display_name;
        false
    }
}

/// 订阅菜单栏事件流（按钮 / 右键菜单点击）。可重复订阅：新流替换旧流成为转发目标
/// （Dart 热重载 / provider 重建后重新调用即恢复接收，按钮不会失灵）。
pub fn menubar_events(sink: EventSink<MenubarEvent>) {
    *EVENTS_SINK.lock().unwrap() = Some(sink);
    // 转发线程只在首次订阅时启动（rx 只有一个）；此后常驻，每个事件转发给
    // 「当前」sink。旧 sink 已死时 add 返回 Err，静默丢弃即可——下次重新订阅
    // 替换 sink 后自动恢复，线程绝不退出（否则按钮事件从此丢失）。
    let Some(rx) = EVENTS_RX.lock().unwrap().take() else {
        return;
    };
    // 事件来自主线程的按钮回调，经 std mpsc；用独立线程阻塞收取并转发（EventSink 线程安全）。
    std::thread::spawn(move || {
        while let Ok(ev) = rx.recv() {
            if let Some(sink) = EVENTS_SINK.lock().unwrap().as_ref() {
                let _ = sink.add(MenubarEvent::from(ev));
            }
        }
    });
}

/// 设置歌词/标题文字（非逐字）。`scroll_delay` 为开始滚动前的等待秒数（超长才滚动）。
pub fn menubar_set_text(text: String, scroll_delay: f64) {
    if let Some(h) = HANDLE.get() {
        h.set_text(text, scroll_delay);
    }
}

/// 设置当前歌词行（逐字版）。`words` 空则回落到滚动 + 单色；带入当前位置/暂停态做初始锚定。
pub fn menubar_set_lyric_line(
    text: String,
    words: Vec<MenubarWord>,
    pos: f64,
    paused: bool,
    scroll_delay: f64,
) {
    if let Some(h) = HANDLE.get() {
        let words = words
            .into_iter()
            .map(|w| (w.start, w.end, w.text))
            .collect();
        h.set_lyric_line(text, words, pos, paused, scroll_delay);
    }
}

/// 同步播放进度锚点（每 tick 调用），驱动逐字高亮外推。
pub fn menubar_set_progress(pos: f64, paused: bool) {
    if let Some(h) = HANDLE.get() {
        h.set_progress(pos, paused);
    }
}

/// 播放态 → 切换播放/暂停图标。
pub fn menubar_set_playing(playing: bool) {
    if let Some(h) = HANDLE.get() {
        h.set_playing(playing);
    }
}

/// 收藏态 → 切换实心/空心红心。
pub fn menubar_set_favorite(fav: bool) {
    if let Some(h) = HANDLE.get() {
        h.set_favorite(fav);
    }
}

/// 设置逐字高亮色。`rgb` 为 0xRRGGBB；`None` = 跟随系统（systemBlue）。
pub fn menubar_set_highlight_color(rgb: Option<u32>) {
    if let Some(h) = HANDLE.get() {
        h.set_highlight_color(rgb.map(|v| {
            (
                ((v >> 16) & 0xFF) as f64 / 255.0,
                ((v >> 8) & 0xFF) as f64 / 255.0,
                (v & 0xFF) as f64 / 255.0,
            )
        }));
    }
}

/// 设置右键菜单内容（下次右键弹出时据此动态构建；支持一级子菜单）。
pub fn menubar_set_menu(items: Vec<MenubarMenuItem>) {
    if let Some(h) = HANDLE.get() {
        h.set_menu(items.into_iter().map(to_spec).collect());
    }
}

fn to_spec(i: MenubarMenuItem) -> MenuItemSpec {
    MenuItemSpec {
        id: i.id,
        title: i.title,
        enabled: i.enabled,
        is_separator: i.is_separator,
        children: i.children.into_iter().map(to_spec).collect(),
    }
}
