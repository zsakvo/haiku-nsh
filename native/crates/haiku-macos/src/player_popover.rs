//! AppKit owns popup positioning, dismissal, scrolling and keyboard input.
use nsh_platform_macos::{
    objc2::{
        define_class, msg_send,
        rc::{Retained, Weak},
        runtime::{AnyObject, ProtocolObject},
        sel, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly,
    },
    objc2_app_kit::*,
    objc2_foundation::{
        NSIndexSet, NSInteger, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect,
        NSRectEdge, NSSize, NSString,
    },
    ComponentContext, Components, NativeComponent,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

const ROW_HEIGHT: f64 = 48.;
const QUEUE_WIDTH: f64 = 360.;
static REGISTERED: AtomicBool = AtomicBool::new(false);

pub fn available() -> bool {
    REGISTERED.load(Ordering::Acquire)
}

define_class!(
    #[unsafe(super(NSViewController, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HaikuPlayerPopoverController"]
    #[ivars = Weak<NSPopover>]
    struct PopoverController;
    unsafe impl NSObjectProtocol for PopoverController {}
    impl PopoverController {
        #[unsafe(method(cancelOperation:))]
        fn cancel(&self, _sender: Option<&AnyObject>) {
            if let Some(popup) = self.ivars().load() { popup.close(); }
        }
    }
);

#[derive(Clone, Deserialize)]
struct Track {
    identity: String,
    title: String,
    duration: String,
}

struct State {
    context: ComponentContext,
    queue: bool,
    popup: Retained<NSPopover>,
    table: Option<Retained<NSTableView>>,
    queue_count: Option<Retained<NSTextField>>,
    clear_button: Option<Retained<NSButton>>,
    slider: Option<Retained<NSSlider>>,
    volume_label: Option<Retained<NSTextField>>,
    tracks: Vec<Track>,
    version: u64,
    current: i32,
    artwork: HashMap<String, String>,
    requested: HashSet<String>,
    images: HashMap<String, Retained<NSImage>>,
    colors: Value,
}

define_class!(
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HaikuPlayerPopoverAnchor"]
    #[ivars = RefCell<State>]
    struct Anchor;
    unsafe impl NSObjectProtocol for Anchor {}
    unsafe impl NSControlTextEditingDelegate for Anchor {}
    unsafe impl NSTableViewDataSource for Anchor {
        #[unsafe(method(numberOfRowsInTableView:))]
        fn row_count(&self, _table: &NSTableView) -> NSInteger {
            self.ivars().borrow().tracks.len() as NSInteger
        }
    }
    unsafe impl NSTableViewDelegate for Anchor {
        #[unsafe(method_id(tableView:viewForTableColumn:row:))]
        fn cell(&self, table: &NSTableView, _column: Option<&NSTableColumn>, row: NSInteger) -> Option<Retained<NSView>> {
            self.row(table, row)
        }
    }
    unsafe impl NSPopoverDelegate for Anchor {
        #[unsafe(method(popoverDidClose:))]
        fn closed(&self, _notification: &NSNotification) {
            self.ivars().borrow().context.emit("open", json!(false));
        }
        #[unsafe(method(popoverDidShow:))]
        fn shown(&self, _notification: &NSNotification) {
            let (table, slider, current) = {
                let s = self.ivars().borrow();
                (s.table.clone(), s.slider.clone(), s.current)
            };
            if let Some(table) = table {
                table.layoutSubtreeIfNeeded();
                if current >= 0 && (current as usize) < self.ivars().borrow().tracks.len() {
                    table.selectRowIndexes_byExtendingSelection(
                        &NSIndexSet::indexSetWithIndex(current as usize), false,
                    );
                    let row = table.rectOfRow(current as NSInteger);
                    let visible = table.visibleRect().size.height;
                    let top = (row.origin.y - (visible - ROW_HEIGHT) / 2.)
                        .clamp(0., (table.bounds().size.height - visible).max(0.));
                    table.scrollPoint(NSPoint::new(0., top));
                }
                if let Some(window) = table.window() {
                    window.makeKeyWindow();
                    window.makeFirstResponder(Some(&table));
                }
            } else if let Some(slider) = slider {
                if let Some(window) = slider.window() {
                    window.makeKeyWindow();
                    window.makeFirstResponder(Some(&slider));
                }
            }
            self.ivars().borrow().context.emit("open", json!(true));
        }
    }
    impl Anchor {
        // The existing NSH button remains the pointer and keyboard trigger.
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> Option<Retained<NSView>> { None }
        #[unsafe(method(volumeChanged:))]
        fn volume_changed(&self, sender: &NSSlider) {
            let value = sender.doubleValue().clamp(0., 1.);
            let s = self.ivars().borrow();
            if let Some(label) = &s.volume_label {
                label.setStringValue(&NSString::from_str(&format!("{:.0}%", value * 100.)));
            }
            s.context.emit("volume", json!(value));
        }
        #[unsafe(method(playRow:))]
        fn play_row(&self, sender: &NSTableView) {
            let row = sender.selectedRow();
            self.emit_track("jump", row);
        }
        #[unsafe(method(removeRow:))]
        fn remove_row(&self, sender: &NSButton) {
            self.emit_track("remove", sender.tag());
        }
        #[unsafe(method(clearQueue:))]
        fn clear_queue(&self, _sender: &NSButton) {
            let s = self.ivars().borrow();
            s.context.emit("clear", json!({"version": s.version}));
        }
    }
);

fn frame(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}
fn label(text: &str, rect: NSRect, size: f64, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    field.setFrame(rect);
    field.setFont(Some(&NSFont::systemFontOfSize(size)));
    field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    field.setUsesSingleLineMode(true);
    field
}
fn color(colors: &Value, key: &str, fallback: Retained<NSColor>) -> Retained<NSColor> {
    let Some(rgba) = colors[key].as_array().filter(|v| v.len() == 4) else {
        return fallback;
    };
    NSColor::colorWithSRGBRed_green_blue_alpha(
        rgba[0].as_f64().unwrap_or(0.),
        rgba[1].as_f64().unwrap_or(0.),
        rgba[2].as_f64().unwrap_or(0.),
        rgba[3].as_f64().unwrap_or(1.),
    )
}

impl Anchor {
    fn new(context: ComponentContext, queue: bool) -> Retained<Self> {
        let mtm = MainThreadMarker::new().expect("player popover main thread");
        let popup = NSPopover::new(mtm);
        popup.setBehavior(NSPopoverBehavior::Transient);
        let view = Self::alloc(mtm).set_ivars(RefCell::new(State {
            context,
            queue,
            popup,
            table: None,
            queue_count: None,
            clear_button: None,
            slider: None,
            volume_label: None,
            tracks: vec![],
            version: 0,
            current: -1,
            artwork: HashMap::new(),
            requested: HashSet::new(),
            images: HashMap::new(),
            colors: Value::Null,
        }));
        let view: Retained<Self> = unsafe { msg_send![super(view), init] };
        view.build();
        view.ivars()
            .borrow()
            .popup
            .setDelegate(Some(ProtocolObject::from_ref(&*view)));
        view
    }

    fn build(&self) {
        let mtm = MainThreadMarker::new().unwrap();
        let queue = self.ivars().borrow().queue;
        let (width, height) = if queue {
            (QUEUE_WIDTH, ROW_HEIGHT * 6. + 56.)
        } else {
            (72., 216.)
        };
        let content = NSView::initWithFrame(NSView::alloc(mtm), frame(0., 0., width, height));
        if queue {
            let heading = label(
                "播放列表",
                frame(16., height - 40., width - 32., 24.),
                14.,
                mtm,
            );
            heading.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
            heading.setFont(Some(&NSFont::boldSystemFontOfSize(14.)));
            heading.sizeToFit();
            let title_size = heading.frame().size;
            heading.setFrame(frame(
                16.,
                height - 28. - title_size.height / 2.,
                title_size.width,
                title_size.height,
            ));
            content.addSubview(&heading);
            let title_frame = heading.frame();
            let count = label(
                "0",
                frame(
                    title_frame.origin.x + title_frame.size.width + 6.,
                    height - 38.,
                    120.,
                    20.,
                ),
                12.,
                mtm,
            );
            count.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
            count.setTextColor(Some(&NSColor::secondaryLabelColor()));
            count.sizeToFit();
            count.setFrame(frame(
                title_frame.origin.x + title_frame.size.width + 6.,
                height - 28. - count.frame().size.height / 2.,
                120.,
                count.frame().size.height,
            ));
            content.addSubview(&count);
            let clear = NSButton::initWithFrame(
                NSButton::alloc(mtm),
                frame(width - 50., height - 40., 24., 24.),
            );
            clear.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
            clear.setBordered(false);
            clear.setImagePosition(NSCellImagePosition::ImageOnly);
            clear.setImage(
                NSImage::imageWithSystemSymbolName_accessibilityDescription(
                    &NSString::from_str("trash"),
                    Some(&NSString::from_str("清空播放队列")),
                )
                .as_deref(),
            );
            clear.setAccessibilityLabel(Some(&NSString::from_str("清空播放队列")));
            clear.setToolTip(Some(&NSString::from_str("清空播放队列")));
            clear.setEnabled(false);
            unsafe {
                clear.setTarget(Some(self));
                clear.setAction(Some(sel!(clearQueue:)));
            }
            content.addSubview(&clear);
            let scroll = NSScrollView::initWithFrame(
                NSScrollView::alloc(mtm),
                frame(0., 8., width, height - 56.),
            );
            scroll.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            scroll.setDrawsBackground(false);
            scroll.setHasVerticalScroller(true);
            scroll.setAutohidesScrollers(true);
            scroll.setScrollerStyle(NSScrollerStyle::Overlay);
            let table = NSTableView::initWithFrame(NSTableView::alloc(mtm), scroll.bounds());
            let column = NSTableColumn::initWithIdentifier(
                NSTableColumn::alloc(mtm),
                &NSString::from_str("track"),
            );
            column.setWidth(width);
            table.addTableColumn(&column);
            table.setHeaderView(None);
            table.setStyle(NSTableViewStyle::FullWidth);
            table.setRowHeight(ROW_HEIGHT);
            table.setIntercellSpacing(NSSize::new(0., 0.));
            table.setBackgroundColor(&NSColor::clearColor());
            table.setSelectionHighlightStyle(NSTableViewSelectionHighlightStyle::None);
            table.setAllowsEmptySelection(true);
            table.setAccessibilityLabel(Some(&NSString::from_str("播放队列")));
            table.setColumnAutoresizingStyle(
                NSTableViewColumnAutoresizingStyle::LastColumnOnlyAutoresizingStyle,
            );
            unsafe {
                table.setDataSource(Some(ProtocolObject::from_ref(self)));
                table.setDelegate(Some(ProtocolObject::from_ref(self)));
                table.setTarget(Some(self));
                table.setAction(Some(sel!(playRow:)));
                table.setDoubleAction(Some(sel!(playRow:)));
            }
            scroll.setDocumentView(Some(&table));
            content.addSubview(&scroll);
            let mut s = self.ivars().borrow_mut();
            s.table = Some(table);
            s.queue_count = Some(count);
            s.clear_button = Some(clear);
        } else {
            let slider = NSSlider::initWithFrame(NSSlider::alloc(mtm), frame(24., 44., 24., 148.));
            slider.setVertical(true);
            slider.setMinValue(0.);
            slider.setMaxValue(1.);
            slider.setContinuous(true);
            slider.setAccessibilityLabel(Some(&NSString::from_str("播放音量")));
            unsafe {
                slider.setTarget(Some(self));
                slider.setAction(Some(sel!(volumeChanged:)));
            }
            content.addSubview(&slider);
            let percent = label("100%", frame(8., 14., 56., 20.), 12., mtm);
            percent.setAlignment(NSTextAlignment::Center);
            content.addSubview(&percent);
            let mut s = self.ivars().borrow_mut();
            s.slider = Some(slider);
            s.volume_label = Some(percent);
        }
        let controller = PopoverController::alloc(mtm)
            .set_ivars(Weak::from_retained(&self.ivars().borrow().popup));
        let controller: Retained<PopoverController> = unsafe { msg_send![super(controller), init] };
        controller.setView(&content);
        let s = self.ivars().borrow();
        s.popup.setContentViewController(Some(&controller));
        s.popup.setContentSize(NSSize::new(width, height));
    }

    fn emit_track(&self, name: &str, index: NSInteger) {
        let s = self.ivars().borrow();
        if let Some(track) = usize::try_from(index).ok().and_then(|i| s.tracks.get(i)) {
            s.context.emit(
                name,
                json!({"index": index, "version": s.version, "identity": track.identity}),
            );
        }
    }

    fn row(&self, table: &NSTableView, index: NSInteger) -> Option<Retained<NSView>> {
        let mtm = MainThreadMarker::new().unwrap();
        let (track, active, colors, image) = {
            let mut s = self.ivars().borrow_mut();
            let track = s.tracks.get(usize::try_from(index).ok()?)?.clone();
            if s.requested.insert(track.identity.clone()) {
                s.context.emit(
                    "artwork",
                    json!({"index": index, "version": s.version, "identity": track.identity}),
                );
            }
            if !s.images.contains_key(&track.identity) {
                if let Some(image) = s.artwork.get(&track.identity).and_then(|path| {
                    NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(path))
                }) {
                    if s.images.len() >= 256 {
                        s.images.clear();
                        s.requested.clear();
                    }
                    s.images.insert(track.identity.clone(), image);
                }
            }
            (
                track.clone(),
                index == s.current as NSInteger,
                s.colors.clone(),
                s.images.get(&track.identity).cloned(),
            )
        };
        let id = NSString::from_str("queue-row");
        let cell = unsafe { table.makeViewWithIdentifier_owner(&id, None) }.unwrap_or_else(|| {
            let cell =
                NSView::initWithFrame(NSView::alloc(mtm), frame(0., 0., QUEUE_WIDTH, ROW_HEIGHT));
            cell.setIdentifier(Some(&id));
            cell
        });
        // AppKit recycles cells; replace all presentation from the current row.
        for subview in cell.subviews() {
            subview.removeFromSuperview();
        }
        // Padding belongs to the row's content; the scroll view reaches the
        // popup edge so the native scroller is independent of that padding.
        let body = NSView::initWithFrame(
            NSView::alloc(mtm),
            frame(8., 0., cell.bounds().size.width - 16., ROW_HEIGHT),
        );
        body.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
        let cover = NSImageView::initWithFrame(NSImageView::alloc(mtm), frame(8., 8., 32., 32.));
        let image = image.or_else(|| {
            NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &NSString::from_str("music.note"),
                Some(&NSString::from_str("歌曲封面")),
            )
        });
        cover.setImage(image.as_deref());
        cover.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        cover.setWantsLayer(true);
        if let Some(layer) = cover.layer() {
            layer.setCornerRadius(4.);
            layer.setMasksToBounds(true);
        }
        body.addSubview(&cover);
        let foreground = if active {
            color(&colors, "active", NSColor::controlAccentColor())
        } else {
            color(&colors, "foreground", NSColor::labelColor())
        };
        let body_width = body.bounds().size.width;
        let title = label(
            &track.title,
            frame(52., 13., body_width - 152., 22.),
            13.,
            mtm,
        );
        title.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
        title.setTextColor(Some(&foreground));
        title.setAccessibilityLabel(Some(&NSString::from_str(&format!(
            "{}{}",
            if active { "正在播放：" } else { "" },
            track.title
        ))));
        body.addSubview(&title);
        let duration = label(
            &track.duration,
            frame(body_width - 96., 14., 48., 20.),
            11.,
            mtm,
        );
        duration.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
        duration.setAlignment(NSTextAlignment::Right);
        let duration_color = if active {
            foreground
        } else {
            color(&colors, "muted", NSColor::secondaryLabelColor())
        };
        duration.setTextColor(Some(&duration_color));
        body.addSubview(&duration);
        let remove =
            NSButton::initWithFrame(NSButton::alloc(mtm), frame(body_width - 35., 13., 22., 22.));
        remove.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
        remove.setBordered(false);
        remove.setImagePosition(NSCellImagePosition::ImageOnly);
        let symbol_size = remove
            .font()
            .map(|font| font.pointSize())
            .unwrap_or(NSFont::systemFontSize())
            - 2.;
        remove.setFont(Some(&NSFont::systemFontOfSize(symbol_size)));
        remove.setToolTip(Some(&NSString::from_str("从队列移除")));
        remove.setImage(
            NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &NSString::from_str("xmark"),
                Some(&NSString::from_str("从队列移除")),
            )
            .as_deref(),
        );
        remove.setTag(index);
        remove.setAccessibilityLabel(Some(&NSString::from_str("从队列移除")));
        unsafe {
            remove.setTarget(Some(self));
            remove.setAction(Some(sel!(removeRow:)));
        }
        body.addSubview(&remove);
        cell.addSubview(&body);
        Some(cell)
    }
}

struct PlayerPopover(Retained<Anchor>);
impl NativeComponent for PlayerPopover {
    fn view(&self) -> Retained<NSView> {
        self.0.clone().into_super()
    }
    fn set_prop(&mut self, name: &str, value: Option<&Value>) {
        let value = value.cloned().unwrap_or(Value::Null);
        // Never hold the RefCell across AppKit calls that consult the datasource
        // or notify the delegate.
        let table = {
            let mut s = self.0.ivars().borrow_mut();
            match name {
                "queue" => {
                    s.tracks = serde_json::from_value(value["tracks"].clone()).unwrap_or_default();
                    s.version = value["version"].as_u64().unwrap_or(0);
                    s.requested.clear();
                    let keys: HashSet<_> = s.tracks.iter().map(|t| t.identity.clone()).collect();
                    s.images.retain(|key, _| keys.contains(key));
                    if let Some(count) = &s.queue_count {
                        count.setStringValue(&NSString::from_str(&s.tracks.len().to_string()));
                    }
                    if let Some(clear) = &s.clear_button {
                        clear.setEnabled(!s.tracks.is_empty());
                    }
                }
                "current" => s.current = value.as_i64().unwrap_or(-1) as i32,
                "artwork" => s.artwork = serde_json::from_value(value.clone()).unwrap_or_default(),
                "colors" => {
                    s.colors = value.clone();
                    if let Some(count) = &s.queue_count {
                        count.setTextColor(Some(&color(
                            &s.colors,
                            "muted",
                            NSColor::secondaryLabelColor(),
                        )));
                    }
                }
                "volume" => {
                    let volume = value.as_f64().unwrap_or(1.).clamp(0., 1.);
                    if let Some(slider) = &s.slider {
                        slider.setDoubleValue(volume);
                    }
                    if let Some(label) = &s.volume_label {
                        label
                            .setStringValue(&NSString::from_str(&format!("{:.0}%", volume * 100.)));
                    }
                }
                "dark" => {
                    let appearance = unsafe {
                        if value.as_bool().unwrap_or(false) {
                            NSAppearanceNameDarkAqua
                        } else {
                            NSAppearanceNameAqua
                        }
                    };
                    let appearance = NSAppearance::appearanceNamed(appearance);
                    s.popup.setAppearance(appearance.as_deref());
                }
                "animate" => s.popup.setAnimates(value.as_bool().unwrap_or(true)),
                _ => {}
            }
            s.table.clone()
        };
        if matches!(name, "queue" | "current" | "artwork" | "colors") {
            if let Some(table) = table {
                table.reloadData();
            }
        }
        if name == "queue" {
            let (popup, rows) = {
                let s = self.0.ivars().borrow();
                (s.popup.clone(), s.tracks.len().clamp(1, 6))
            };
            popup.setContentSize(NSSize::new(QUEUE_WIDTH, rows as f64 * ROW_HEIGHT + 56.));
        }
        if name == "open" {
            let popup = self.0.ivars().borrow().popup.clone();
            if value.as_bool().unwrap_or(false) {
                if self.0.window().is_some() {
                    popup.showRelativeToRect_ofView_preferredEdge(
                        self.0.bounds(),
                        &self.0,
                        NSRectEdge::MaxY,
                    );
                }
            } else {
                popup.close();
            }
        }
    }
}
impl Drop for PlayerPopover {
    fn drop(&mut self) {
        let (popup, table) = {
            let s = self.0.ivars().borrow();
            (s.popup.clone(), s.table.clone())
        };
        popup.setDelegate(None);
        popup.close();
        if let Some(table) = table {
            unsafe {
                table.setDelegate(None);
                table.setDataSource(None);
                table.setTarget(None);
            }
        }
    }
}

pub fn register(components: &Components) {
    for (name, queue) in [("haiku.VolumePopover", false), ("haiku.QueuePopover", true)] {
        components.register(
            name,
            Rc::new(move |context| Box::new(PlayerPopover(Anchor::new(context, queue)))),
        );
    }
    REGISTERED.store(true, Ordering::Release);
}
