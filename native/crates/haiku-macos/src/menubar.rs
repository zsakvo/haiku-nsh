//! macOS 顶部菜单栏播放控制组件（NSStatusItem）。
//!
//! 在系统菜单栏右侧绘制一个组件：左侧一行**当前歌词**（不足一行时居中、超出时横向滚动，
//! KRC 逐字卡拉OK高亮），右侧五个按钮——上一首 / 播放·暂停 / 下一首 / 喜欢（红心）/
//! logo（更多）。控制按钮图标以模板图渲染、随深浅色菜单栏自适应；歌词超长滚动时会等当前句
//! 唱到一半再开始滚动。
//!
//! 与 music-cli 的差异（见 `specs/features/menubar-lyrics.md`）：
//! - 更多按钮**左键/右键**：打开动态队列菜单；主窗口通过菜单项恢复；
//! - logo 按钮**右键**：弹出由 Flutter 经 [`MenuBarHandle::set_menu`] 下发的动态菜单
//!   （支持分隔线与一级子菜单）；叶子项点击把其 `id` 经事件通道回传。
//!
//! 线程模型：组件由主线程的 [`install`] 创建并被系统状态栏持有；按钮 target 指向一个用
//! `objc` 运行时声明的 `CjrtMenuTarget` 类，点击转成 [`MenuBarEvent`] 送进事件通道。工作
//! 线程通过 [`MenuBarHandle`] 更新歌词/图标/菜单——内部用 GCD 把操作派发到主队列（AppKit
//! 只能在主线程访问），id 指针以 `usize` 跨线程传递、仅在主队列闭包内解引用。

use std::sync::mpsc::Sender;

// ===================== 跨平台共享类型 =====================

/// 菜单栏按钮/菜单点击事件（送进 [`Sender`]，由 FRB 层转发给 Dart）。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MenuBarEvent {
    /// 上一首。
    Prev,
    /// 播放 / 暂停切换。
    Toggle,
    /// 下一首。
    Next,
    /// 收藏态切换（红心）。
    ToggleFavorite,
    /// 右键菜单项点击，载荷为该项的 `id`。
    MenuItem(String),
}

/// 右键菜单项描述（由 Flutter 下发）。
///
/// - `is_separator` 为真时忽略 `id/title/enabled/children`，渲染为分隔线。
/// - `children` 非空时渲染为带子菜单的父项（父项自身不触发点击回调）。
/// - 叶子项点击经事件通道回传 `id`。
#[derive(Clone, Debug, PartialEq)]
pub struct MenuItemSpec {
    pub id: String,
    pub title: String,
    pub enabled: bool,
    pub is_separator: bool,
    pub children: Vec<MenuItemSpec>,
}

// ===================== 非 macOS：no-op =====================

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::{MenuBarEvent, MenuItemSpec, Sender};

    /// 占位句柄：其它平台无菜单栏组件，所有方法 no-op。
    /// 方法签名须与 macOS 版逐一对应（调用方同一套代码跨平台编译）。
    #[derive(Clone)]
    pub struct MenuBarHandle;

    impl MenuBarHandle {
        pub fn set_visible(&self, _visible: bool) {}
        pub fn set_text(&self, _text: String, _scroll_delay: f64) {}
        pub fn set_lyric_line(
            &self,
            _text: String,
            _words: Vec<(f64, f64, String)>,
            _start: f64,
            _pos: f64,
            _paused: bool,
            _scroll_delay: f64,
        ) {
        }
        pub fn set_progress(&self, _pos: f64, _paused: bool) {}
        pub fn set_lyrics_visible(&self, _visible: bool) {}
        pub fn set_playing(&self, _playing: bool) {}
        pub fn set_favorite(&self, _fav: bool) {}
        pub fn set_highlight_color(&self, _rgb: Option<(f64, f64, f64)>) {}
        pub fn set_menu(&self, _items: Vec<MenuItemSpec>) {}
    }

    /// 非 macOS：无顶栏组件，直接返回占位句柄（FRB 层不会走到这里）。
    pub fn install(_display_name: &str, _tx: Sender<MenuBarEvent>) -> MenuBarHandle {
        MenuBarHandle
    }
}

// ===================== macOS =====================

#[cfg(target_os = "macos")]
mod imp {
    use std::os::raw::{c_char, c_void};
    use std::sync::OnceLock;
    use std::time::Instant;

    use cocoa::appkit::NSStatusBar;
    use cocoa::base::{id, nil, NO, YES};
    use cocoa::foundation::{NSPoint, NSRect, NSSize};
    use dispatch::Queue;
    use objc::declare::ClassDecl;
    use objc::runtime::{Class, Object, Sel, BOOL};
    use objc::{class, msg_send, sel, sel_impl};

    use super::{MenuBarEvent, MenuItemSpec, Sender};

    // ---- 内嵌图标（网易云 StatusBarControl，即其菜单栏组件原生图标） ----
    // 统一 24×22 画布、以**模板图**渲染：只用 alpha 形状，颜色随菜单栏自适应（深=白、浅=黑）。
    const IC_PREV: &[u8] = include_bytes!("../assets/menubar/prev.png"); // Rewind ⏮
    const IC_PLAY: &[u8] = include_bytes!("../assets/menubar/play.png"); // Play ▶
    const IC_PAUSE: &[u8] = include_bytes!("../assets/menubar/pause.png"); // Pause ⏸
    const IC_NEXT: &[u8] = include_bytes!("../assets/menubar/next.png"); // FastForward ⏭
    const IC_LIKE: &[u8] = include_bytes!("../assets/menubar/like.png"); // Love 空心红心（未收藏）
    const IC_LIKED: &[u8] = include_bytes!("../assets/menubar/liked.png"); // Loved 实心红心（已收藏）
    const IC_MORE: &[u8] = include_bytes!("../assets/menubar/more.png"); // Logo（更多/菜单）

    // ---- 几何常量 ----
    const BTN: f64 = 24.0; // 单个按钮宽度
    const LABEL_W: f64 = 152.0; // 歌词区域宽度
    const PAD: f64 = 6.0; // 标签与按钮组间距
    const MARGIN: f64 = 6.0; // 两侧外边距
    const N_BTN: f64 = 5.0;
    // 图标整体大小：把网易云 24×22 画布按此高度缩放（宽度随宽高比、居中）。
    const ICON_H: f64 = 20.0;
    const LYRIC_FONT: f64 = 14.0;

    /// 存放在 target 对象 ivar 里的状态（仅主线程访问，故无需加锁）。
    struct MenuState {
        sender: Sender<MenuBarEvent>,
        /// 右键菜单项列表，由 worker 经 [`MenuBarHandle::set_menu`] 同步。
        menu_items: Vec<MenuItemSpec>,
    }

    /// 歌词视图状态（ivar，仅主线程访问）。
    ///
    /// ## 为什么是 layer-backed（2026-07-28 重写，见 `specs/perf/menubar-lyrics-cpu.md`）
    ///
    /// 旧实现用 30fps 的 `NSTimer` + `setNeedsDisplay:` 驱动 `drawRect:`。对 profile
    /// 进程 `sample` 5 秒实测：**每一次 `setNeedsDisplay:` 都会让 AppKit 跑一遍
    /// `-[NSStatusItem _updateReplicants]`**——整棵状态项视图树的 appearance 重传播
    /// （递归 `_viewDidChangeAppearance:` + 每个 `NSButtonCell` 重算 bezel/vibrancy）、
    /// `_layoutSubtreeWithOldSize:` 全量布局，再发通知触发 `NSStatusBarWindow`
    /// `_testAndUpdateAllowsFlattening` 的递归 vibrancy 扫描。单次约 15ms CPU，30Hz
    /// 下吃掉半个核（主线程 47.5% 的采样落在这里，而 Flutter 自己的帧路径只占 0.2%）。
    ///
    /// 现在改走 Core Animation：文字放进 [`CATextLayer`]（内容一次栅格化后当纹理复用），
    /// 逐字高亮 = 裁剪层的 `bounds.size.width` 动画，长句滚动 = 位移层的 `position.x`
    /// 动画，两者都一次性提交给合成端插值。**稳态零 `setNeedsDisplay`、零主线程 CPU**；
    /// 只有换行 / seek / 暂停翻转 / 外观变化才重新提交一次事务。
    struct MarqueeState {
        text: String,
        /// 整句像素宽度（换行时量一次）。
        text_width: f64,
        /// 当前行在播放时间轴上的起点，用于可暂停的非逐字滚动。
        scroll_start: f64,
        /// 开始滚动前的等待秒数（歌词行 = 该句时长的一半；标题 = 固定 1.5s）。
        scroll_delay: f64,
        /// 长句滚动是否**无缝循环**：标题 `true`（两段副本首尾相接无限滚），
        /// 歌词行 `false`（等到阈值时间后只滚一次把剩余内容带出来，停在末尾不回头）。
        loop_scroll: bool,

        // ---- 逐字（KRC）卡拉OK高亮 ----
        /// 逐字时间轴：(绝对起始秒, 绝对结束秒, 该字文本)。空 = 非逐字（回落到滚动）。
        words: Vec<(f64, f64, String)>,
        /// 是否逐字（words 非空）。
        synced: bool,
        /// 每个字结束处的累计像素宽度（换行时量一次，供关键帧取值）。
        word_x: Vec<f64>,
        /// 最近一次同步的播放位置（秒）。
        song_pos: f64,
        /// 设置 song_pos 时的墙钟时刻（用于按 1x 外推、判定 seek 跳变）。
        pos_at: Instant,
        /// 是否暂停（暂停时定格为静态值，不挂动画）。
        paused: bool,

        /// 已唱部分高亮色（sRGB 分量 0..1）。`None` = 跟随系统，走 [`system_blue`]。
        highlight_rgb: Option<(f64, f64, f64)>,

        // ---- 图层（由 [`rebuild_content`] 建立，仅主线程解引用）----
        /// 位移层：长句滚动动的是它的 `position.x`；其子层是文字本体。
        scroll_layer: id,
        /// 逐字高亮的裁剪层：动的是它的 `bounds.size.width`。非逐字时为 [`nil`]。
        clip_layer: id,
    }

    impl MarqueeState {
        /// 当前估计的播放位置：暂停取快照，否则按墙钟从上次同步点外推。
        fn current_pos(&self) -> f64 {
            if self.paused {
                self.song_pos
            } else {
                self.song_pos + self.pos_at.elapsed().as_secs_f64()
            }
        }
    }

    /// 工作线程持有的更新句柄（只存裸指针的 usize，方法把操作派发到主队列执行）。
    #[derive(Clone)]
    pub struct MenuBarHandle {
        lyrics_view: usize,
        container: usize,
        button_bar: usize,
        play_btn: usize,
        heart_btn: usize,
        /// target 对象指针（持有 MenuState ivar），set_menu 经主队列更新其菜单列表。
        target: usize,
        status_item: usize,
    }

    // MenuBarHandle 只含 usize（跨线程仅在主队列闭包内解引用），可安全跨线程传递。
    unsafe impl Send for MenuBarHandle {}
    unsafe impl Sync for MenuBarHandle {}

    impl MenuBarHandle {
        pub fn set_visible(&self, visible: bool) {
            let item = self.status_item;
            Queue::main().exec_async(move || unsafe {
                let _: () = msg_send![item as id,setVisible:if visible{YES}else{NO}];
            });
        }
        /// 仅显示 / 隐藏歌词区域并收拢布局，播放按钮与状态栏项保持可用。
        pub fn set_lyrics_visible(&self, visible: bool) {
            let handle = self.clone();
            Queue::main().exec_async(move || unsafe {
                let view = handle.lyrics_view as id;
                let hidden: BOOL = msg_send![view, isHidden];
                if (hidden == NO) == visible {
                    return;
                }
                let container = handle.container as id;
                let bounds: NSRect = msg_send![container, bounds];
                let bar_x = MARGIN + if visible { LABEL_W + PAD } else { 0.0 };
                let full_len = bar_x + BTN * N_BTN + MARGIN;
                begin_tx();
                let _: () = msg_send![view, setHidden: if visible { NO } else { YES }];
                let _: () = msg_send![handle.button_bar as id,
                    setFrame: rect(bar_x, 0.0, BTN * N_BTN, bounds.size.height)];
                let _: () = msg_send![container,
                    setFrameSize: NSSize::new(full_len, bounds.size.height)];
                let _: () = msg_send![handle.status_item as id, setLength: full_len];
                commit_tx();
                if visible {
                    apply_timeline(view);
                }
            });
        }

        /// 设置歌词/标题文字（当前行）。`scroll_delay` 为开始滚动前的等待秒数（超长才滚动）。
        pub fn set_text(&self, text: String, scroll_delay: f64) {
            let view = self.lyrics_view;
            Queue::main().exec_async(move || unsafe {
                let _: () = msg_send![view as id, setScrollDelay: scroll_delay];
                let s = ns_string(&text);
                let _: () = msg_send![view as id, setMarqueeText: s];
            });
        }

        /// 设置当前歌词行（逐字版）。`words` 为 (绝对起始秒, 绝对结束秒, 字文本) 列表，
        /// 空则视为非逐字（回落到滚动 + 单色）。同时带入当前播放位置/暂停态做初始锚定。
        pub fn set_lyric_line(
            &self,
            text: String,
            words: Vec<(f64, f64, String)>,
            start: f64,
            pos: f64,
            paused: bool,
            scroll_delay: f64,
        ) {
            let view = self.lyrics_view;
            Queue::main().exec_async(move || unsafe {
                let obj = &*(view as *const Object);
                let st = marquee_state(obj);
                // 上游会因歌词来源切换等原因重发同一句：内容没变就别重建图层树
                // （重建要重新排版 + 提交事务，是这条链路上唯一还有量级的开销），
                // 只把时间锚点对齐即可。
                let same = st.text == text && st.words == words && !st.loop_scroll;
                st.song_pos = pos;
                st.pos_at = Instant::now();
                st.paused = paused;
                st.scroll_delay = scroll_delay.max(0.0);
                st.scroll_start = start;
                if same {
                    apply_timeline(view as id);
                    return;
                }
                st.text = text;
                st.words = words;
                st.synced = !st.words.is_empty();
                st.word_x.clear();
                st.text_width = 0.0; // rebuild_content 里重新测量

                // 歌词行的长句滚动是「一次性揭示」，不循环（与逐字模式观感一致）。
                st.loop_scroll = false;
                rebuild_content(view as id);
            });
        }

        /// Only called on pause/resume/seek; always reanchor, including sub-350ms seeks.
        pub fn set_progress(&self, pos: f64, paused: bool) {
            let view = self.lyrics_view;
            Queue::main().exec_async(move || unsafe {
                let obj = &*(view as *const Object);
                let st = marquee_state(obj);
                st.song_pos = pos;
                st.pos_at = Instant::now();
                st.paused = paused;
                apply_timeline(view as id);
            });
        }

        /// 设置逐字高亮色。`rgb` 为 sRGB 分量 0..1；`None` = 跟随系统（systemBlue）。
        /// 颜色已经烘进 [`CATextLayer`] 的内容里，改色要重建图层。
        pub fn set_highlight_color(&self, rgb: Option<(f64, f64, f64)>) {
            let view = self.lyrics_view;
            Queue::main().exec_async(move || unsafe {
                let obj = &*(view as *const Object);
                marquee_state(obj).highlight_rgb = rgb;
                rebuild_content(view as id);
            });
        }

        /// 播放态 → 切换播放/暂停图标（播放中显示暂停图标，反之亦然）。
        pub fn set_playing(&self, playing: bool) {
            let btn = self.play_btn;
            Queue::main().exec_async(move || unsafe {
                let img = make_image(if playing { IC_PAUSE } else { IC_PLAY });
                let _: () = msg_send![img, setAccessibilityDescription: ns_string(if playing {"playing"} else {"paused"})];
                let _: () = msg_send![btn as id, setImage: img];
            });
        }

        /// 收藏态 → 切换实心/空心红心。
        pub fn set_favorite(&self, fav: bool) {
            let btn = self.heart_btn;
            Queue::main().exec_async(move || unsafe {
                let img = make_image(if fav { IC_LIKED } else { IC_LIKE });
                let _: () = msg_send![img, setAccessibilityDescription: ns_string(if fav {"favorite"} else {"unfavorite"})];
                let _: () = msg_send![btn as id, setImage: img];
            });
        }

        /// Main-queue fence reads the native view, images and applied menu definitions.
        #[cfg(debug_assertions)]
        pub fn snapshot(&self) -> serde_json::Value {
            let h=self.clone();
            Queue::main().exec_sync(move || unsafe {
                let st=marquee_state(&*(h.lyrics_view as *const Object));
                let play:id=msg_send![h.play_btn as id,image];
                let heart:id=msg_send![h.heart_btn as id,image];
                let playing=nsstring_to_string(msg_send![play,accessibilityDescription])=="playing";
                let favorite=nsstring_to_string(msg_send![heart,accessibilityDescription])=="favorite";
                let hidden:BOOL=msg_send![h.lyrics_view as id,isHidden];
                let visible:BOOL=msg_send![h.status_item as id,isVisible];
                fn count(items:&[MenuItemSpec])->usize {items.iter().map(|i|1+count(&i.children)).sum()}
                let items=&state_of(&*(h.target as *const Object)).menu_items;
                serde_json::json!({"text":st.text,"playing":playing,"favorite":favorite,
                    "menuItemCount":count(items),"lyricsVisible":hidden==NO,"visible":visible==YES,
                    "paused":st.paused,"position":st.current_pos(),"wordCount":st.words.len()})
            })
        }

        /// 同步右键菜单项列表（下次右键弹出时据此动态构建）。
        pub fn set_menu(&self, items: Vec<MenuItemSpec>) {
            let target = self.target;
            Queue::main().exec_async(move || unsafe {
                let obj = &*(target as *const Object);
                state_of(obj).menu_items = items;
            });
        }
    }

    /// 在主线程安装菜单栏组件，返回工作线程用的更新句柄。
    ///
    /// 必须在 `NSApplication` 已创建后、主线程调用。组件由系统状态栏持有，target 对象被 leak，
    /// 故整个进程生命周期内有效。
    pub fn install(display_name: &str, tx: Sender<MenuBarEvent>) -> MenuBarHandle {
        unsafe {
            let h = status_thickness();
            let bar_w = BTN * N_BTN;
            let bar_x = MARGIN + LABEL_W + PAD;
            let full_len = bar_x + bar_w + MARGIN;

            // 容器视图。
            let container = new_view(rect(0.0, 0.0, full_len, h));

            // 歌词跑马灯（左）。
            let lyrics_view = new_marquee(rect(MARGIN, 0.0, LABEL_W, h));
            set_marquee_text_str(lyrics_view, display_name);
            // 这里曾经挂过一个 30fps 的 NSTimer 驱动 drawRect:。**别加回来**——
            // 每次 setNeedsDisplay: 都会触发 -[NSStatusItem _updateReplicants]，
            // 30Hz 下要吃掉半个核，见 [`MarqueeState`] 的注释。滚动与逐字高亮现在
            // 全部由 Core Animation 在合成端完成，本进程稳态零开销。

            // target 对象（按钮 action 接收者）。先建空对象，最后再填 state ivar。
            let target: id = msg_send![target_class(), new];

            // 按钮组（右），内部按钮以相对坐标排列。
            let button_bar = new_view(rect(bar_x, 0.0, bar_w, h));
            let mk = |i: f64, bytes: &[u8], action: Sel| -> id {
                let b = icon_button(rect(i * BTN, 0.0, BTN, h), bytes, target, action);
                let _: () = msg_send![button_bar, addSubview: b];
                b
            };
            let _prev = mk(0.0, IC_PREV, sel!(onPrev:));
            let play_btn = mk(1.0, IC_PLAY, sel!(onToggle:));
            let _next = mk(2.0, IC_NEXT, sel!(onNext:));
            let heart_btn = mk(3.0, IC_LIKE, sel!(onFavorite:));
            // 更多按钮左右键均弹菜单，真实屏幕坐标命中保持不变。
            let more_btn =
                icon_menu_button(rect(4.0 * BTN, 0.0, BTN, h), IC_MORE, target, sel!(onLogoRight:));
            let _: () = msg_send![button_bar, addSubview: more_btn];

            let _: () = msg_send![container, addSubview: lyrics_view];
            let _: () = msg_send![container, addSubview: button_bar];

            // 状态栏项：变长，安装自定义视图。
            let status_bar: id = NSStatusBar::systemStatusBar(nil);
            let status_item: id = msg_send![status_bar, statusItemWithLength: full_len];
            // The item contains independent controls, not one selectable menu button.
            // Use the public status button host: deprecated setView creates a
            // system replicant whose pressed capsule ignores highlightMode.
            let status_button: id = msg_send![status_item, button];
            let cell: id = msg_send![status_button, cell];
            let _: () = msg_send![cell, setHighlightsBy: 0u64];
            let _: () = msg_send![cell, setShowsStateBy: 0u64];
            let _: () = msg_send![status_button, setBordered: NO];
            let _: () = msg_send![status_button, addSubview: container];
            let _: id = msg_send![status_item, retain];
            // macOS status-item replicants forward clicks at the window centre,
            // not at the physical pointer. Resolve the actual screen position before
            // AppKit dispatches the synthetic event to the custom view.
            let container_ptr = container as usize;
            let buttons = [_prev, play_btn, _next, heart_btn, more_btn].map(|b| b as usize);
            let pressed = std::cell::Cell::new(None::<usize>);
            let monitor = block::ConcreteBlock::new(move |event: id| -> id {
                let view = container_ptr as id;
                let window: id = msg_send![event, window];
                let own_window: id = msg_send![view, window];
                if window == nil || window != own_window {
                    return event;
                }
                let kind: u64 = msg_send![event, type];
                let mouse: NSPoint = msg_send![class!(NSEvent), mouseLocation];
                let window_point: NSPoint = msg_send![window, convertPointFromScreen: mouse];
                let point: NSPoint = msg_send![view, convertPoint: window_point fromView: nil];
                let hit = buttons.iter().copied().find(|button| {
                    let b = *button as id;
                    let local: NSPoint = msg_send![b, convertPoint: point fromView: view];
                    let bounds: NSRect = msg_send![b, bounds];
                    local.x >= 0.0
                        && local.y >= 0.0
                        && local.x < bounds.size.width
                        && local.y < bounds.size.height
                });
                if kind == 1 {
                    pressed.set(hit);
                } else if kind == 2 {
                    if let Some(button) = pressed.take().filter(|b| Some(*b) == hit) {
                        let _: () = msg_send![button as id, performClick: nil];
                    }
                } else if kind == 3 && hit == Some(buttons[4]) {
                    let _: () = msg_send![buttons[4] as id, rightMouseDown: event];
                }
                nil
            })
            .copy();
            let _: id = msg_send![class!(NSEvent), addLocalMonitorForEventsMatchingMask: 30u64 handler: &*monitor];
            std::mem::forget(monitor);

            // 填充 target 状态（leak，进程生命周期有效）。
            let state = Box::new(MenuState {
                sender: tx,
                menu_items: Vec::new(),
            });
            let raw = Box::into_raw(state) as *mut c_void;
            let obj: &mut Object = &mut *target;
            obj.set_ivar("state", raw);

            MenuBarHandle {
                lyrics_view: lyrics_view as usize,
                container: container as usize,
                button_bar: button_bar as usize,
                play_btn: play_btn as usize,
                heart_btn: heart_btn as usize,
                target: target as usize,
                status_item: status_item as usize,
            }
        }
    }

    // ---------- objc target 类（按钮/菜单 action 接收者） ----------

    fn target_class() -> &'static Class {
        static CLASS: OnceLock<usize> = OnceLock::new();
        let ptr = *CLASS.get_or_init(|| unsafe { build_target_class() as *const Class as usize });
        unsafe { &*(ptr as *const Class) }
    }

    unsafe fn build_target_class() -> &'static Class {
        let superclass = class!(NSObject);
        let mut decl =
            ClassDecl::new("CjrtMenuTarget", superclass).expect("注册 CjrtMenuTarget 失败");
        decl.add_ivar::<*mut c_void>("state");
        decl.add_method(sel!(onPrev:), on_prev as extern "C" fn(&Object, Sel, id));
        decl.add_method(
            sel!(onToggle:),
            on_toggle as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(sel!(onNext:), on_next as extern "C" fn(&Object, Sel, id));
        decl.add_method(
            sel!(onFavorite:),
            on_favorite as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(sel!(onLogo:), on_logo as extern "C" fn(&Object, Sel, id));
        decl.add_method(
            sel!(onLogoRight:),
            on_logo_right as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(onMenuItem:),
            on_menu_item as extern "C" fn(&Object, Sel, id),
        );
        decl.register()
    }

    #[allow(
        clippy::mut_from_ref,
        reason = "Preserve the reference Objective-C ivar accessor during migration"
    )]
    unsafe fn state_of(this: &Object) -> &mut MenuState {
        let p: *mut c_void = *this.get_ivar("state");
        &mut *(p as *mut MenuState)
    }

    fn emit(this: &Object, ev: MenuBarEvent) {
        unsafe {
            let _ = state_of(this).sender.send(ev);
        }
    }

    extern "C" fn on_prev(this: &Object, _: Sel, _: id) {
        emit(this, MenuBarEvent::Prev);
    }
    extern "C" fn on_toggle(this: &Object, _: Sel, _: id) {
        emit(this, MenuBarEvent::Toggle);
    }
    extern "C" fn on_next(this: &Object, _: Sel, _: id) {
        emit(this, MenuBarEvent::Next);
    }
    extern "C" fn on_favorite(this: &Object, _: Sel, _: id) {
        emit(this, MenuBarEvent::ToggleFavorite);
    }

    /// logo 左键：激活 app，并**显示**主窗口。
    ///
    /// 覆盖三种状态（仅 `makeKeyAndOrderFront` 可见窗不够）：
    /// - 红灯 / Cmd+W → `orderOut` 后 `isVisible == NO`，仍须 `makeKeyAndOrderFront` 拉回
    /// - 黄灯最小化 → 先 `deminiaturize` 再前置
    /// - 已显示但被其它 app 遮住 → `makeKeyAndOrderFront` + activate
    ///
    /// 优先处理 `MainFlutterWindow`（Swift 模块名 `Runner`，className 含该串）；
    /// 找不到时回退到所有可成为 key 的窗，避免无主窗时点了没反应。
    extern "C" fn on_logo(this: &Object, _: Sel, _: id) {
        emit(this, MenuBarEvent::MenuItem("window:show".into()));
    }

    /// logo 右键：按 Flutter 下发的菜单项列表动态构建 NSMenu 并弹出。空列表弹 disabled「(空)」项。
    extern "C" fn on_logo_right(this: &Object, _: Sel, sender: id) {
        unsafe {
            let target = this as *const Object as id;
            let menu: id = msg_send![class!(NSMenu), new];
            let _: () = msg_send![menu, setAutoenablesItems: NO];

            let items = state_of(this).menu_items.clone();
            if items.is_empty() {
                let item = make_menu_item("(空)", sel!(onMenuItem:), target);
                let _: () = msg_send![item, setEnabled: NO];
                let _: () = msg_send![menu, addItem: item];
                let _: () = msg_send![item, release];
            } else {
                for spec in &items {
                    append_menu_item(menu, spec, target);
                }
            }

            let loc = NSPoint::new(0.0, -4.0);
            let _: BOOL =
                msg_send![menu, popUpMenuPositioningItem: nil atLocation: loc inView: sender];
            let _: () = msg_send![menu, release];
        }
    }

    /// 右键菜单项点击：从 `representedObject` 取回 id 经事件通道回传（兼容嵌套子菜单）。
    extern "C" fn on_menu_item(this: &Object, _: Sel, sender: id) {
        unsafe {
            let rep: id = msg_send![sender, representedObject];
            if rep == nil {
                return;
            }
            let id_str = nsstring_to_string(rep);
            if !id_str.is_empty() {
                emit(this, MenuBarEvent::MenuItem(id_str));
            }
        }
    }

    /// 把一条 [MenuItemSpec] 加进 `menu`（支持分隔线 / 叶子 / 带子菜单）。
    unsafe fn append_menu_item(menu: id, spec: &MenuItemSpec, target: id) {
        if spec.is_separator {
            add_separator(menu);
            return;
        }
        let item = make_menu_item(&spec.title, sel!(onMenuItem:), target);
        let enabled: BOOL = if spec.enabled { YES } else { NO };
        let _: () = msg_send![item, setEnabled: enabled];
        // id 放 representedObject，嵌套子菜单也能直接取回，不再依赖顶层下标。
        let _: () = msg_send![item, setRepresentedObject: ns_string(&spec.id)];
        if !spec.children.is_empty() {
            let submenu: id = msg_send![class!(NSMenu), new];
            let _: () = msg_send![submenu, setAutoenablesItems: NO];
            for child in &spec.children {
                append_menu_item(submenu, child, target);
            }
            let _: () = msg_send![item, setSubmenu: submenu];
            let _: () = msg_send![submenu, release];
        }
        let _: () = msg_send![menu, addItem: item];
        let _: () = msg_send![item, release];
    }

    // ---------- 跑马灯歌词视图（CjrtMarquee : NSView） ----------

    fn marquee_class() -> &'static Class {
        static CLASS: OnceLock<usize> = OnceLock::new();
        let ptr = *CLASS.get_or_init(|| unsafe { build_marquee_class() as *const Class as usize });
        unsafe { &*(ptr as *const Class) }
    }

    unsafe fn build_marquee_class() -> &'static Class {
        let superclass = class!(NSView);
        let mut decl = ClassDecl::new("CjrtMarquee", superclass).expect("注册 CjrtMarquee 失败");
        decl.add_ivar::<*mut c_void>("ms");
        decl.add_method(
            sel!(setMarqueeText:),
            marquee_set_text as extern "C" fn(&Object, Sel, id),
        );
        decl.add_method(
            sel!(setScrollDelay:),
            marquee_set_delay as extern "C" fn(&Object, Sel, f64),
        );
        // 图层里存的是**解析后的** CGColor / 位图，不像 drawRect: 会随外观和缩放自动
        // 重画，故这三处变化都要重建图层树（都是低频事件）。
        decl.add_method(
            sel!(viewDidChangeEffectiveAppearance),
            marquee_rebuild_hook as extern "C" fn(&Object, Sel),
        );
        decl.add_method(
            sel!(viewDidChangeBackingProperties),
            marquee_rebuild_hook as extern "C" fn(&Object, Sel),
        );
        decl.add_method(
            sel!(viewDidMoveToWindow),
            marquee_rebuild_hook as extern "C" fn(&Object, Sel),
        );
        decl.register()
    }

    unsafe fn new_marquee(frame: NSRect) -> id {
        let obj: id = msg_send![marquee_class(), alloc];
        let obj: id = msg_send![obj, initWithFrame: frame];
        let st = Box::new(MarqueeState {
            text: String::new(),
            text_width: 0.0,
            scroll_start: 0.,
            scroll_delay: 1.5,
            loop_scroll: true,
            words: Vec::new(),
            synced: false,
            word_x: Vec::new(),
            song_pos: 0.0,
            pos_at: Instant::now(),
            paused: false,
            highlight_rgb: None,
            scroll_layer: nil,
            clip_layer: nil,
        });
        let raw = Box::into_raw(st) as *mut c_void;
        let o: &mut Object = &mut *obj;
        o.set_ivar("ms", raw);
        // 图层承载一切绘制；宿主层负责把超宽文字裁在歌词区内（替代旧的 NSRectClip）。
        let _: () = msg_send![obj, setWantsLayer: YES];
        obj
    }

    #[allow(
        clippy::mut_from_ref,
        reason = "Preserve the reference Objective-C ivar accessor during migration"
    )]
    unsafe fn marquee_state(this: &Object) -> &mut MarqueeState {
        let p: *mut c_void = *this.get_ivar("ms");
        &mut *(p as *mut MarqueeState)
    }

    /// 设置歌词文字（直接传 Rust 字符串的便捷封装，仅安装时用）。
    unsafe fn set_marquee_text_str(view: id, text: &str) {
        let s = ns_string(text);
        let _: () = msg_send![view, setMarqueeText: s];
    }

    /// 无缝循环滚动时两段文字之间的间隙（仅标题走循环）。
    const MARQUEE_GAP: f64 = 44.0;
    /// 长句滚动速度（逻辑像素 / 秒）。与旧实现的 30fps × 1px 等价，手感不变。
    const SCROLL_SPEED: f64 = 30.0;
    extern "C" fn marquee_set_text(this: &Object, _: Sel, ns: id) {
        unsafe {
            let st = marquee_state(this);
            let new = nsstring_to_string(ns);
            if new == st.text {
                return;
            }
            st.text = new;
            st.scroll_start = st.current_pos(); // 标题也使用可暂停的播放时钟
            // set_text 用于标题等非逐字文本：清掉上一句的逐字时间轴。
            st.words.clear();
            st.word_x.clear();
            st.synced = false;
            // 标题会一直挂着（整首歌没有歌词时），滚一次就停在末尾会让歌名再也看不到，
            // 所以它保留无缝循环；歌词行才是一次性揭示（见 [`MarqueeState::loop_scroll`]）。
            st.loop_scroll = true;
            rebuild_content(this as *const Object as id);
        }
    }

    extern "C" fn marquee_set_delay(this: &Object, _: Sel, delay: f64) {
        unsafe {
            marquee_state(this).scroll_delay = delay.max(0.0);
        }
    }

    /// 外观 / 缩放 / 上屏变化：图层里是解析好的颜色与位图，得重建。
    ///
    /// 这三个回调由 AppKit 触发，时机不完全受我们控制，故先确认 ivar 已装好再动手
    /// （`new_marquee` 里是先写 ivar 再 `setWantsLayer:`，正常不会撞上）。
    extern "C" fn marquee_rebuild_hook(this: &Object, _: Sel) {
        unsafe {
            let p: *mut c_void = *this.get_ivar("ms");
            if p.is_null() {
                return;
            }
            rebuild_content(this as *const Object as id)
        }
    }

    /// 逐字累计宽度测量：word_x[i] = 前 i+1 个字拼接后的像素宽度。一句仅测一次，
    /// 之后整句的高亮推进全靠这张表生成关键帧，帧路径上不再有任何文本排版。
    unsafe fn measure_words(st: &mut MarqueeState, attrs: id) {
        st.word_x.clear();
        let mut acc = String::new();
        for (_, _, t) in &st.words {
            acc.push_str(t);
            let s = ns_string(&acc);
            let size: NSSize = msg_send![s, sizeWithAttributes: attrs];
            st.word_x.push(size.width);
        }
    }

    /// 位移层的 x：短句居中；长句让「已唱点」尽量居中（不越过两端）。
    fn scroll_x(vw: f64, tw: f64, sung: f64) -> f64 {
        if tw <= vw {
            (vw - tw) / 2.0
        } else {
            (vw / 2.0 - sung).clamp(vw - tw, 0.0)
        }
    }

    // ---------- 图层树构建 ----------

    /// 重建整棵图层树（换文字 / 换高亮色 / 外观或缩放变化时调用），末尾自动重挂动画。
    ///
    /// 结构（宿主层 `masksToBounds` 负责裁到歌词区）：
    /// ```text
    /// view.layer
    /// └── scroll            位移层：长句滚动动它的 position.x
    ///     ├── base          整句普通色文字（CATextLayer，尺寸恒定 → 纹理全程复用）
    ///     ├── base2         仅循环滚动时存在的第二段副本
    ///     └── clip          裁剪层：逐字高亮动它的 bounds.size.width
    ///         └── hi        整句高亮色文字（同样尺寸恒定）
    /// ```
    unsafe fn rebuild_content(view: id) {
        let this = &*(view as *const Object);
        let st = marquee_state(this);
        let host: id = msg_send![view, layer];
        if host == nil {
            return;
        }

        let bounds: NSRect = msg_send![view, bounds];
        let vw = bounds.size.width;
        let vh = bounds.size.height;
        let scale = backing_scale(view);

        begin_tx();
        let _: () = msg_send![host, setMasksToBounds: YES];
        let none: id = nil;
        let _: () = msg_send![host, setSublayers: none];
        st.scroll_layer = nil;
        st.clip_layer = nil;

        if st.text.is_empty() {
            commit_tx();
            return;
        }

        // 颜色按**本视图的 effectiveAppearance**（= 菜单栏那一档，未必等于 app 主题）
        // 现取：图层存的是解析后的 CGColor，不像 drawRect: 会随外观自动重画。
        let base_attrs = text_attrs(view);
        let hi_attrs = text_attrs_color(highlight_color(st, view));
        let plain = ns_string(&st.text);
        let size: NSSize = msg_send![plain, sizeWithAttributes: base_attrs];
        let attr_base = attr_string(&st.text, base_attrs);
        let attr_hi = attr_string(&st.text, hi_attrs);
        if st.synced {
            measure_words(st, base_attrs);
        } else {
            st.word_x.clear();
        }

        // 逐字时 word_x 的累计宽度未必与整句排版完全相等（KRC 的字拼起来可能与行文本
        // 差一点空白），取两者较大值：文字层不至于被自己的 bounds 裁掉，滚动的夹取范围
        // 也与高亮宽度同一把尺。
        st.text_width = match st.word_x.last() {
            Some(w) => size.width.max(*w),
            None => size.width,
        };
        let tw = st.text_width;
        let th = size.height;
        let ty = ((vh - th) / 2.0).max(0.0);

        let scroll: id = msg_send![class!(CALayer), layer];
        let _: () = msg_send![scroll, setAnchorPoint: NSPoint::new(0.0, 0.0)];
        let _: () = msg_send![scroll, setBounds: rect(0.0, 0.0, tw.max(1.0), vh)];
        let _: () = msg_send![scroll, setPosition: NSPoint::new(0.0, 0.0)];

        let base = new_text_layer(attr_base, tw, th, ty, scale);
        let _: () = msg_send![scroll, addSublayer: base];

        if st.loop_scroll && tw > vw {
            let dup = new_text_layer(attr_base, tw, th, ty, scale);
            let _: () = msg_send![dup, setPosition: NSPoint::new(tw + MARQUEE_GAP, ty)];
            let _: () = msg_send![scroll, addSublayer: dup];
        }

        if st.synced {
            let clip: id = msg_send![class!(CALayer), layer];
            let _: () = msg_send![clip, setAnchorPoint: NSPoint::new(0.0, 0.0)];
            let _: () = msg_send![clip, setPosition: NSPoint::new(0.0, 0.0)];
            let _: () = msg_send![clip, setBounds: rect(0.0, 0.0, 0.0, vh)];
            let _: () = msg_send![clip, setMasksToBounds: YES];
            let hi = new_text_layer(attr_hi, tw, th, ty, scale);
            let _: () = msg_send![clip, addSublayer: hi];
            let _: () = msg_send![scroll, addSublayer: clip];
            st.clip_layer = clip;
        }

        let _: () = msg_send![host, addSublayer: scroll];
        st.scroll_layer = scroll;
        commit_tx();

        apply_timeline(view);
    }

    /// 按当前播放位置 / 暂停态挂动画（暂停时改为定格成静态值）。
    ///
    /// **不在位置 tick 上调用**——稳态下一次都不跑，整句的推进由合成端按已提交的
    /// 关键帧自己走完（见 [`MenuBarHandle::set_progress`]）。
    unsafe fn apply_timeline(view: id) {
        let this = &*(view as *const Object);
        let st = marquee_state(this);
        let scroll = st.scroll_layer;
        if scroll == nil {
            return;
        }
        let host: id = msg_send![view, layer];
        let bounds: NSRect = msg_send![view, bounds];
        let vw = bounds.size.width;
        let vh = bounds.size.height;
        let tw = st.text_width;

        begin_tx();
        let _: () = msg_send![scroll, removeAllAnimations];
        if st.clip_layer != nil {
            let _: () = msg_send![st.clip_layer, removeAllAnimations];
        }

        // 图层时间轴的「现在」。用 convertTime 而不是 CACurrentMediaTime 直取，
        // 这样即便上层时间基被改过也仍然对齐。
        let now: f64 = msg_send![host, convertTime: CACurrentMediaTime() fromLayer: nil];

        // ---- 逐字：裁剪层宽度 + 位移，两条关键帧一次提交 ----
        if st.synced && !st.word_x.is_empty() && st.word_x.len() == st.words.len() {
            let t0 = st.words[0].0;
            let t1 = st.words[st.words.len() - 1].1;
            let span = (t1 - t0).max(1e-3);
            let pos = st.current_pos();

            if st.paused {
                let sung = sung_width(st, pos);
                let _: () = msg_send![st.clip_layer, setBounds: rect(0.0, 0.0, sung.max(0.0), vh)];
                let _: () =
                    msg_send![scroll, setPosition: NSPoint::new(scroll_x(vw, tw, sung), 0.0)];
                commit_tx();
                return;
            }

            // 每个字：起点保持上一字末宽（字间空隙不推进），终点到本字末宽，中间线性。
            let n = st.words.len();
            let mut times = Vec::with_capacity(n * 2 + 2);
            let mut widths = Vec::with_capacity(n * 2 + 2);
            times.push(0.0);
            widths.push(0.0);
            for (i, (ws, we, _)) in st.words.iter().enumerate() {
                let prev = if i == 0 { 0.0 } else { st.word_x[i - 1] };
                times.push(((ws - t0) / span).clamp(0.0, 1.0));
                widths.push(prev);
                times.push(((we - t0) / span).clamp(0.0, 1.0));
                widths.push(st.word_x[i]);
            }
            times.push(1.0);
            widths.push(st.word_x[n - 1]);
            // keyTimes 必须单调不减，否则 CA 直接忽略整条动画。
            for i in 1..times.len() {
                if times[i] < times[i - 1] {
                    times[i] = times[i - 1];
                }
            }

            let begin = now - (pos - t0);
            let fill = keyframe("bounds.size.width", &widths, &times, begin, span);
            let _: () = msg_send![st.clip_layer, addAnimation: fill forKey: ns_string("karaoke")];
            // 模型值定格在终态：万一动画被移除也不会跳回未唱状态。
            let _: () = msg_send![st.clip_layer, setBounds: rect(0.0, 0.0, st.word_x[n - 1], vh)];

            if tw > vw {
                let xs: Vec<f64> = widths.iter().map(|w| scroll_x(vw, tw, *w)).collect();
                let last_x = xs[xs.len() - 1];
                let anim = keyframe("position.x", &xs, &times, begin, span);
                let _: () = msg_send![scroll, addAnimation: anim forKey: ns_string("scroll")];
                let _: () = msg_send![scroll, setPosition: NSPoint::new(last_x, 0.0)];
            } else {
                let _: () =
                    msg_send![scroll, setPosition: NSPoint::new(scroll_x(vw, tw, 0.0), 0.0)];
            }
            commit_tx();
            return;
        }

        // ---- 非逐字 ----
        if tw <= vw {
            let _: () = msg_send![scroll, setPosition: NSPoint::new((vw - tw) / 2.0, 0.0)];
            commit_tx();
            return;
        }

        // 长句：等 scroll_delay（歌词行 = 本句时长的一半）过去再开始滚。
        // 使用播放时钟而非持续墙钟，暂停冻结，seek 后从句首重算。
        let elapsed = (st.current_pos() - st.scroll_start - st.scroll_delay).max(0.);
        if st.paused {
            let distance = if st.loop_scroll {
                (elapsed * SCROLL_SPEED) % (tw + MARQUEE_GAP)
            } else { (elapsed * SCROLL_SPEED).min(tw - vw) };
            let _: () = msg_send![scroll, setPosition: NSPoint::new(-distance, 0.0)];
            commit_tx();
            return;
        }
        let begin = now - (st.current_pos() - st.scroll_start) + st.scroll_delay;
        if st.loop_scroll {
            // 标题：两段副本首尾相接，滚过一个 loop_w 即无缝回到原位，无限重复。
            let loop_w = tw + MARQUEE_GAP;
            let anim = basic(
                "position.x",
                0.0,
                -loop_w,
                begin,
                loop_w / SCROLL_SPEED,
                true,
            );
            let _: () = msg_send![scroll, addAnimation: anim forKey: ns_string("scroll")];
            let _: () = msg_send![scroll, setPosition: NSPoint::new(0.0, 0.0)];
        } else {
            // 歌词行：只滚一次，把超出的部分带出来后停在末尾——不循环
            // （fillMode=both 保证 begin 之前停在行首、结束之后停在行尾）。
            let dist = tw - vw;
            let anim = basic("position.x", 0.0, -dist, begin, dist / SCROLL_SPEED, false);
            let _: () = msg_send![scroll, addAnimation: anim forKey: ns_string("scroll")];
            let _: () = msg_send![scroll, setPosition: NSPoint::new(-dist, 0.0)];
        }
        commit_tx();
    }

    /// 当前播放位置对应的「已唱像素宽度」：唱满的字累加整宽，正在唱的字按时间比例线性填充。
    fn sung_width(st: &MarqueeState, pos: f64) -> f64 {
        if st.words.is_empty() || st.word_x.is_empty() {
            return 0.0;
        }
        let n = st.words.len();
        for i in 0..n {
            let (start, end, _) = &st.words[i];
            let prev_x = if i == 0 { 0.0 } else { st.word_x[i - 1] };
            if pos < *start {
                return prev_x; // 处于上一字与本字之间的空隙：停在上一字末尾
            }
            if pos < *end {
                let frac = ((pos - start) / (end - start).max(1e-6)).clamp(0.0, 1.0);
                return prev_x + (st.word_x[i] - prev_x) * frac;
            }
        }
        *st.word_x.last().unwrap() // 全唱完
    }

    // ---------- 自定义 logo 按钮（CjrtMenuButton : NSButton） ----------

    #[cfg(test)]
    pub fn smoke_controls() {
        unsafe {
            let _app: id = msg_send![class!(NSApplication), sharedApplication];
            let (tx, rx) = std::sync::mpsc::channel();
            let handle = install("Haiku test", tx);
            let host: id = msg_send![handle.status_item as id, button];
            let cell: id = msg_send![host, cell];
            let highlights: u64 = msg_send![cell, highlightsBy];
            let state: u64 = msg_send![cell, showsStateBy];
            assert_eq!(
                highlights, 0,
                "status host must not highlight the whole item"
            );
            assert_eq!(state, 0);
            let bar = handle.button_bar as id;
            let container = handle.container as id;
            let buttons: id = msg_send![bar, subviews];
            for visible in [true, false, false, true] {
                handle.set_lyrics_visible(visible);
                if !visible {
                    handle.set_text("Updated while hidden".into(), 1.5);
                }
                // Setter operations are queued on the AppKit main thread.
                let run_loop: id = msg_send![class!(NSRunLoop), mainRunLoop];
                let until: id = msg_send![class!(NSDate), dateWithTimeIntervalSinceNow: 0.05f64];
                let _: () = msg_send![run_loop, runUntilDate: until];
                let hidden: BOOL = msg_send![handle.lyrics_view as id, isHidden];
                assert_eq!(hidden, if visible { NO } else { YES });
                let item_visible: BOOL = msg_send![handle.status_item as id, isVisible];
                assert_eq!(
                    item_visible, YES,
                    "lyrics must not hide the playback controls"
                );
                let bar_frame: NSRect = msg_send![bar, frame];
                let bar_x = MARGIN + if visible { LABEL_W + PAD } else { 0.0 };
                assert_eq!(bar_frame.origin.x, bar_x);
                let full_len = bar_x + BTN * N_BTN + MARGIN;
                let length: f64 = msg_send![handle.status_item as id, length];
                assert_eq!(length, full_len);
                let container_frame: NSRect = msg_send![container, frame];
                assert_eq!(container_frame.size.width, full_len);
                if !visible {
                    let state = marquee_state(&*(handle.lyrics_view as *const Object));
                    assert_eq!(state.text, "Updated while hidden");
                }
                for index in 0..5usize {
                    let button: id = msg_send![buttons,objectAtIndex:index];
                    let enabled: BOOL = msg_send![button, isEnabled];
                    assert_eq!(enabled, YES);
                    let bounds: NSRect = msg_send![button, bounds];
                    let parent: id = msg_send![container, superview];
                    let point: NSPoint = msg_send![button,convertPoint:NSPoint::new(bounds.size.width/2.0,bounds.size.height/2.0) toView:parent];
                    let hit: id = msg_send![container,hitTest:point];
                    assert_eq!(hit, button);
                    let _: () = msg_send![hit,performClick:nil];
                    let event = rx.try_recv().expect("button action");
                    assert!(matches!(
                        (index, event),
                        (0, MenuBarEvent::Prev)
                            | (1, MenuBarEvent::Toggle)
                            | (2, MenuBarEvent::Next)
                            | (3, MenuBarEvent::ToggleFavorite)
                            | (4, MenuBarEvent::MenuItem(_))
                    ));
                    assert!(rx.try_recv().is_err());
                }
            }
            let state = marquee_state(&*(handle.lyrics_view as *const Object));
            assert_eq!(state.text, "Updated while hidden");
            handle.set_visible(false);
        }
    }

    fn menu_button_class() -> &'static Class {
        static CLASS: OnceLock<usize> = OnceLock::new();
        let ptr =
            *CLASS.get_or_init(|| unsafe { build_menu_button_class() as *const Class as usize });
        unsafe { &*(ptr as *const Class) }
    }

    unsafe fn build_menu_button_class() -> &'static Class {
        let superclass = class!(NSButton);
        let mut decl =
            ClassDecl::new("CjrtMenuButton", superclass).expect("注册 CjrtMenuButton 失败");
        decl.add_ivar::<*mut c_void>("rtarget");
        decl.add_method(
            sel!(rightMouseDown:),
            right_mouse_down as extern "C" fn(&Object, Sel, id),
        );
        decl.register()
    }

    /// 右键按下：转调 target 的 `onLogoRight:`，把自身作为 sender（供菜单定位）。
    extern "C" fn right_mouse_down(this: &Object, _: Sel, _event: id) {
        unsafe {
            let t: *mut c_void = *this.get_ivar("rtarget");
            if t.is_null() {
                return;
            }
            let target = t as id;
            let btn = this as *const Object as id;
            let _: () = msg_send![target, onLogoRight: btn];
        }
    }

    // ---------- AppKit 小工具 ----------

    // 文字属性键（AppKit 的全局 NSString 常量）。
    #[link(name = "AppKit", kind = "framework")]
    extern "C" {
        static NSFontAttributeName: id;
        static NSForegroundColorAttributeName: id;
    }

    #[link(name = "QuartzCore", kind = "framework")]
    extern "C" {
        /// Core Animation 的时间基（秒）。图层动画的 `beginTime` 与它同一时间轴。
        fn CACurrentMediaTime() -> f64;
    }

    // ---------- Core Animation 小工具 ----------

    /// 开一个禁用隐式动画的事务。改图层属性默认会带 0.25s 的隐式动画，
    /// 定格 / 重锚都必须立刻生效，故一律包在这对函数里。
    unsafe fn begin_tx() {
        let _: () = msg_send![class!(CATransaction), begin];
        let _: () = msg_send![class!(CATransaction), setDisableActions: YES];
    }

    unsafe fn commit_tx() {
        let _: () = msg_send![class!(CATransaction), commit];
    }

    unsafe fn ns_numbers(vals: &[f64]) -> id {
        let arr: id = msg_send![class!(NSMutableArray), array];
        for v in vals {
            let n: id = msg_send![class!(NSNumber), numberWithDouble: *v];
            let _: () = msg_send![arr, addObject: n];
        }
        arr
    }

    /// 线性插值的关键帧动画。`fillMode = both` + `removedOnCompletion = NO`：
    /// `begin` 在过去时动画从中途接上，走完后停在终值，正是「换行时按当前进度对齐」
    /// 与「整句唱完保持全亮」两种需求。
    ///
    /// 字符串常量直接写字面量：`kCAAnimationLinear` / `kCAFillModeBoth` 就是 `"linear"`
    /// / `"both"`，这样不必再 link 一组 extern static。
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

    /// 匀速的两点动画（长句滚动用）。`repeat` 为真时无限重复（标题的无缝循环）。
    unsafe fn basic(key: &str, from: f64, to: f64, begin: f64, dur: f64, repeat: bool) -> id {
        let a: id = msg_send![class!(CABasicAnimation), animationWithKeyPath: ns_string(key)];
        let nf: id = msg_send![class!(NSNumber), numberWithDouble: from];
        let nt: id = msg_send![class!(NSNumber), numberWithDouble: to];
        let tf: id =
            msg_send![class!(CAMediaTimingFunction), functionWithName: ns_string("linear")];
        let _: () = msg_send![a, setFromValue: nf];
        let _: () = msg_send![a, setToValue: nt];
        let _: () = msg_send![a, setTimingFunction: tf];
        let _: () = msg_send![a, setBeginTime: begin];
        let _: () = msg_send![a, setDuration: dur.max(1e-3)];
        let _: () = msg_send![a, setFillMode: ns_string("both")];
        let _: () = msg_send![a, setRemovedOnCompletion: NO];
        if repeat {
            // CA 约定用 HUGE_VALF 表示无限重复。
            let _: () = msg_send![a, setRepeatCount: f32::INFINITY];
        }
        a
    }

    /// 一层文字：尺寸恒定，内容只在重建时栅格化一次，之后当纹理复用。
    unsafe fn new_text_layer(attr: id, tw: f64, th: f64, ty: f64, scale: f64) -> id {
        let l: id = msg_send![class!(CATextLayer), layer];
        let _: () = msg_send![l, setAnchorPoint: NSPoint::new(0.0, 0.0)];
        let _: () = msg_send![l, setBounds: rect(0.0, 0.0, tw.max(1.0), th.max(1.0))];
        let _: () = msg_send![l, setPosition: NSPoint::new(0.0, ty)];
        let _: () = msg_send![l, setContentsScale: scale];
        let _: () = msg_send![l, setWrapped: NO];
        let _: () = msg_send![l, setString: attr];
        l
    }

    /// 自动释放的 NSAttributedString。
    unsafe fn attr_string(text: &str, attrs: id) -> id {
        let s = ns_string(text);
        let a: id = msg_send![class!(NSAttributedString), alloc];
        let a: id = msg_send![a, initWithString: s attributes: attrs];
        let a: id = msg_send![a, autorelease];
        a
    }

    /// 视图所在屏幕的像素密度；未上屏时退回主屏，再不济按 Retina 猜 2.0。
    unsafe fn backing_scale(view: id) -> f64 {
        let win: id = msg_send![view, window];
        if win != nil {
            let s: f64 = msg_send![win, backingScaleFactor];
            if s > 0.5 {
                return s;
            }
        }
        let scr: id = msg_send![class!(NSScreen), mainScreen];
        if scr != nil {
            let s: f64 = msg_send![scr, backingScaleFactor];
            if s > 0.5 {
                return s;
            }
        }
        2.0
    }

    /// 菜单栏当前是不是深色。
    ///
    /// ## 为什么不能直接用 `labelColor`
    ///
    /// 图层里存的是**解析后的** CGColor。`drawRect:` 时代 AppKit 会把视图的绘制外观
    /// 设为当前，动态色（`labelColor` / `systemBlueColor`）自然求得菜单栏那一档；
    /// 改图层后没有这个上下文，得自己切。而 `+[NSAppearance setCurrentAppearance:]`
    /// 在 macOS 11+ 已废弃、**对 NSColor 的动态求值不再生效**——实测取到的仍是浅色
    /// 分支，于是深色菜单栏上画出黑字（右侧图标是模板图、由 AppKit 自己染色，所以
    /// 只有歌词错、图标对，看着格外突兀）。
    ///
    /// 正解要么是 `-[NSAppearance performAsCurrentDrawingAppearance:]`（要传 block，
    /// Rust 侧包 `FnOnce` 很别扭），要么就是这里的做法：读视图的 `effectiveAppearance`
    /// 判深浅，给确定的颜色。深色外观名有 `NSAppearanceNameDarkAqua` /
    /// `NSAppearanceNameVibrantDark` 两种，都含 `Dark`。
    ///
    /// 外观变了会走 `viewDidChangeEffectiveAppearance` 重建图层，所以切主题 /
    /// 换壁纸导致菜单栏明暗翻转时颜色会跟着更新。
    unsafe fn is_dark_appearance(view: id) -> bool {
        let eff: id = msg_send![view, effectiveAppearance];
        if eff == nil {
            return false;
        }
        // 用 bestMatchFromAppearancesWithNames: 而不是直接读 name：外观可能是复合的
        // （菜单栏在浅色系统 + 深色壁纸下走 VibrantDark），这是 Apple 文档给的判法。
        let names: id = msg_send![class!(NSMutableArray), array];
        for n in [
            "NSAppearanceNameAqua",
            "NSAppearanceNameDarkAqua",
            "NSAppearanceNameVibrantLight",
            "NSAppearanceNameVibrantDark",
        ] {
            let s = ns_string(n);
            let _: () = msg_send![names, addObject: s];
        }
        let best: id = msg_send![eff, bestMatchFromAppearancesWithNames: names];
        let resolved: id = if best == nil {
            msg_send![eff, name]
        } else {
            best
        };
        nsstring_to_string(resolved).contains("Dark")
    }

    /// 歌词正文色：与右侧那排模板图图标同步——深色菜单栏白字，浅色菜单栏黑字。
    unsafe fn text_attrs(view: id) -> id {
        let color: id = if is_dark_appearance(view) {
            msg_send![class!(NSColor), whiteColor]
        } else {
            msg_send![class!(NSColor), blackColor]
        };
        text_attrs_color(color)
    }

    /// 与 [`text_attrs`] 同字体，但指定前景色（逐字高亮用高亮色重绘已唱部分）。
    unsafe fn text_attrs_color(color: id) -> id {
        let font: id = msg_send![class!(NSFont), menuBarFontOfSize: LYRIC_FONT];
        let dict: id = msg_send![class!(NSMutableDictionary), dictionary];
        let _: () = msg_send![dict, setObject: font forKey: NSFontAttributeName];
        let _: () = msg_send![dict, setObject: color forKey: NSForegroundColorAttributeName];
        dict
    }

    /// 已唱歌词的默认高亮色 = 系统强调蓝。同样不能用动态色 `systemBlueColor`
    /// （理由见 [`is_dark_appearance`]），故直接给两档实测值：浅色 #007AFF、深色 #0A84FF。
    unsafe fn system_blue(view: id) -> id {
        if is_dark_appearance(view) {
            msg_send![class!(NSColor), colorWithSRGBRed: 0.039 green: 0.518 blue: 1.0 alpha: 1.0]
        } else {
            msg_send![class!(NSColor), colorWithSRGBRed: 0.0 green: 0.478 blue: 1.0 alpha: 1.0]
        }
    }

    /// 已唱部分高亮色：`highlight_rgb` 为 `Some` 时用该 sRGB 色（用户指定，不随外观变），
    /// 否则回落到 [`system_blue`]。
    unsafe fn highlight_color(st: &MarqueeState, view: id) -> id {
        match st.highlight_rgb {
            Some((r, g, b)) => {
                msg_send![class!(NSColor), colorWithSRGBRed: r green: g blue: b alpha: 1.0]
            }
            None => system_blue(view),
        }
    }

    unsafe fn status_thickness() -> f64 {
        let bar: id = NSStatusBar::systemStatusBar(nil);
        let t: f64 = msg_send![bar, thickness];
        if t < 1.0 {
            22.0
        } else {
            t
        }
    }

    fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
    }

    unsafe fn new_view(frame: NSRect) -> id {
        let v: id = msg_send![class!(NSView), alloc];
        msg_send![v, initWithFrame: frame]
    }

    /// 从内嵌字节构建**统一高度**的模板图：高度固定为 [`ICON_H`]，宽度随字形宽高比，
    /// 故所有图标视觉大小一致（颜色随菜单栏自适应）。
    unsafe fn make_image(bytes: &[u8]) -> id {
        let data: id = msg_send![class!(NSData),
            dataWithBytes: bytes.as_ptr() as *const c_void
            length: bytes.len() as u64];
        let img: id = msg_send![class!(NSImage), alloc];
        let img: id = msg_send![img, initWithData: data];
        if img != nil {
            let nat: NSSize = msg_send![img, size];
            let aspect = if nat.height > 0.1 {
                nat.width / nat.height
            } else {
                1.0
            };
            let _: () = msg_send![img, setSize: NSSize::new(ICON_H * aspect, ICON_H)];
            // 保留按钮默认 tint，由 AppKit 按菜单栏外观与 vibrancy 为模板图着色。
            let _: () = msg_send![img, setTemplate: YES];
        }
        img
    }

    /// 无边框、仅图标的菜单栏按钮（点击触发 action）。
    unsafe fn icon_button(frame: NSRect, bytes: &[u8], target: id, action: Sel) -> id {
        let btn: id = msg_send![class!(NSButton), alloc];
        let btn: id = msg_send![btn, initWithFrame: frame];
        let _: () = msg_send![btn, setImage: make_image(bytes)];
        let _: () = msg_send![btn, setImagePosition: 1u64]; // NSImageOnly
        let _: () = msg_send![btn, setBordered: NO];
        let _: () = msg_send![btn, setButtonType: 5u64]; // NSMomentaryChange
        let _: () = msg_send![btn, setTarget: target];
        let _: () = msg_send![btn, setAction: action];
        btn
    }

    /// 与 [`icon_button`] 同款外观，但用自定义 `CjrtMenuButton` 类以支持右键弹菜单
    /// （右键 target 存入 `rtarget` ivar，见 [`right_mouse_down`]）。
    unsafe fn icon_menu_button(frame: NSRect, bytes: &[u8], target: id, action: Sel) -> id {
        let btn: id = msg_send![menu_button_class(), alloc];
        let btn: id = msg_send![btn, initWithFrame: frame];
        let _: () = msg_send![btn, setImage: make_image(bytes)];
        let _: () = msg_send![btn, setImagePosition: 1u64]; // NSImageOnly
        let _: () = msg_send![btn, setBordered: NO];
        let _: () = msg_send![btn, setButtonType: 5u64]; // NSMomentaryChange
        let _: () = msg_send![btn, setTarget: target];
        let _: () = msg_send![btn, setAction: action];
        let o: &mut Object = &mut *btn;
        o.set_ivar("rtarget", target as *mut c_void);
        btn
    }

    /// 自动释放的 NSString。
    unsafe fn ns_string(s: &str) -> id {
        let cstring =
            std::ffi::CString::new(s).unwrap_or_else(|_| std::ffi::CString::new("").unwrap());
        msg_send![class!(NSString), stringWithUTF8String: cstring.as_ptr()]
    }

    unsafe fn nsstring_to_string(ns: id) -> String {
        if ns == nil {
            return String::new();
        }
        let ptr: *const c_char = msg_send![ns, UTF8String];
        if ptr.is_null() {
            return String::new();
        }
        std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }

    /// 创建一个 NSMenuItem（不加入任何菜单；调用方 addItem 后负责 release）。
    unsafe fn make_menu_item(title: &str, action: Sel, target: id) -> id {
        let t = ns_string(title);
        let empty = ns_string("");
        let item: id = msg_send![class!(NSMenuItem), alloc];
        let item: id = msg_send![item, initWithTitle: t action: action keyEquivalent: empty];
        let _: () = msg_send![item, setTarget: target];
        item
    }

    unsafe fn add_separator(menu: id) {
        let sep: id = msg_send![class!(NSMenuItem), separatorItem];
        let _: () = msg_send![menu, addItem: sep];
    }
}

pub use imp::{install, MenuBarHandle};

#[cfg(all(test, target_os = "macos"))]
pub use imp::smoke_controls;
