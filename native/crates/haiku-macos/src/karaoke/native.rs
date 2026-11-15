use super::{Clock, fragment_times, needs_resync};
use nsh_platform_macos::{
    ComponentContext, Components, NativeComponent,
    objc2::{
        AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
        rc::Retained, runtime::AnyObject,
    },
    objc2_app_kit::{
        NSAccessibility, NSAccessibilityStaticTextRole, NSFont, NSFontWeightMedium,
        NSFontWeightSemibold, NSLineBreakMode, NSMutableParagraphStyle, NSView,
    },
    objc2_foundation::{
        NSMutableAttributedString, NSNumber, NSObject, NSObjectProtocol, NSPoint, NSRange, NSSize,
        NSString,
    },
};
use objc2_core_foundation::{CFAttributedString, CFRange, CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGBlendMode, CGColorSpace, CGContext,
    CGImage, CGImageAlphaInfo, CGPath,
};
use objc2_core_text::{CTFramesetter, CTLine, kCTForegroundColorFromContextAttributeName};
use objc2_quartz_core::{
    CABasicAnimation, CACurrentMediaTime, CALayer, CAMediaTiming, CAMediaTimingFunction,
    CATransaction, kCAFillModeBoth, kCAMediaTimingFunctionLinear,
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    cell::RefCell,
    ptr,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

static REGISTERED: AtomicBool = AtomicBool::new(false);
pub fn available() -> bool {
    REGISTERED.load(Ordering::Acquire)
}

#[derive(Clone, Default, Deserialize)]
struct Word {
    text: String,
    start: f64,
    end: f64,
}
struct Fragment {
    clip: Retained<CALayer>,
    width: f64,
    height: f64,
    start: f64,
    end: f64,
}
struct State {
    context: ComponentContext,
    props: serde_json::Map<String, Value>,
    built: Option<(Value, f64, f64)>,
    height: f64,
    body: Retained<CALayer>,
    fragments: Vec<Fragment>,
    clock: Clock,
    anchored_at: f64,
}
define_class!(
    #[unsafe(super(NSView, nsh_platform_macos::objc2_app_kit::NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HaikuKaraokeView"]
    #[ivars = RefCell<State>]
    struct KaraokeView;
    unsafe impl NSObjectProtocol for KaraokeView {}
    impl KaraokeView {
        #[unsafe(method(isFlipped))]
        fn flipped(&self) -> bool { true }
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> Option<Retained<NSView>> { None }
        #[unsafe(method(layout))]
        fn layout_native(&self) {
            unsafe { let _: () = msg_send![super(self), layout]; }
            self.rebuild();
        }
        #[unsafe(method(viewDidChangeBackingProperties))]
        fn backing_changed(&self) {
            unsafe { let _: () = msg_send![super(self), viewDidChangeBackingProperties]; }
            self.setNeedsLayout(true);
        }
        #[unsafe(method(setFrameSize:))]
        fn frame_size_changed(&self, size: NSSize) {
            unsafe { let _: () = msg_send![super(self), setFrameSize: size]; }
            self.setNeedsLayout(true);
        }
    }
);

impl KaraokeView {
    fn new(context: ComponentContext) -> Retained<Self> {
        let mtm = MainThreadMarker::new().expect("KaraokeLayer main thread");
        let body = CALayer::layer();
        body.setAnchorPoint(CGPoint::new(0., 0.));
        body.setPosition(CGPoint::new(0., 0.));
        let view = Self::alloc(mtm).set_ivars(RefCell::new(State {
            context,
            props: Default::default(),
            built: None,
            height: 0.,
            body,
            fragments: vec![],
            clock: Clock::default(),
            anchored_at: CACurrentMediaTime(),
        }));
        let view: Retained<Self> = unsafe { msg_send![super(view), init] };
        view.setWantsLayer(true);
        // The flipped NSView's root uses top-left geometry for the fragment clips.
        let layer = view.layer().expect("layer-backed NSView");
        layer.setMasksToBounds(true);
        layer.addSublayer(&view.ivars().borrow().body);
        view.setAccessibilityElement(true);
        unsafe {
            view.setAccessibilityRole(Some(NSAccessibilityStaticTextRole));
        }
        view
    }

    fn rebuild(&self) {
        let width = self.bounds().size.width;
        if !width.is_finite() || width <= 0. {
            return;
        }
        let scale = self.window().map(|w| w.backingScaleFactor()).unwrap_or(1.);
        let mut state = self.ivars().borrow_mut();
        // Only raster content belongs in this key. Clock samples only call sync.
        let content = Value::Object(
            ["words", "font", "size", "weight", "color", "dim"]
                .into_iter()
                .filter_map(|name| {
                    state
                        .props
                        .get(name)
                        .map(|value| (name.into(), value.clone()))
                })
                .collect(),
        );
        if state.built.as_ref() == Some(&(content.clone(), width, scale)) {
            return;
        }
        let words: Vec<Word> = serde_json::from_value(content["words"].clone()).unwrap_or_default();
        if words.is_empty() {
            return;
        }
        let size = content["size"].as_f64().unwrap_or(20.);
        let weight = content["weight"].as_f64().unwrap_or(600.);
        let family = content["font"].as_str().unwrap_or("system-ui");
        let base = NSFont::systemFontOfSize_weight(
            size,
            if weight >= 600. {
                unsafe { NSFontWeightSemibold }
            } else if weight >= 500. {
                unsafe { NSFontWeightMedium }
            } else {
                0.
            },
        );
        let font = if family == "system-ui" || family.is_empty() {
            base
        } else {
            let descriptor = base
                .fontDescriptor()
                .fontDescriptorWithFamily(&NSString::from_str(family));
            NSFont::fontWithDescriptor_size(&descriptor, size).unwrap_or(base)
        };
        let text: String = words.iter().map(|word| word.text.as_str()).collect();
        self.setAccessibilityLabel(Some(&NSString::from_str(&text)));
        let string = NSString::from_str(&text);
        let attributed =
            NSMutableAttributedString::initWithString(NSMutableAttributedString::alloc(), &string);
        let range = NSRange::new(0, string.length());
        let paragraph = NSMutableParagraphStyle::new();
        paragraph.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
        // Match the lyrics view's 1.4 line-height, including font scaling.
        paragraph.setMinimumLineHeight(size * 1.4);
        paragraph.setMaximumLineHeight(size * 1.4);
        unsafe {
            attributed.addAttribute_value_range(
                nsh_platform_macos::objc2_app_kit::NSFontAttributeName,
                &font,
                range,
            );
            attributed.addAttribute_value_range(
                nsh_platform_macos::objc2_app_kit::NSParagraphStyleAttributeName,
                &paragraph,
                range,
            );
            attributed.addAttribute_value_range(
                &*(kCTForegroundColorFromContextAttributeName as *const _ as *const NSString),
                &NSNumber::numberWithBool(true),
                range,
            );
        }
        // NSAttributedString and CFAttributedString are toll-free bridged.
        let cf = unsafe { &*(Retained::as_ptr(&attributed).cast::<CFAttributedString>()) };
        let setter = unsafe { CTFramesetter::with_attributed_string(cf) };
        let measured = unsafe {
            setter.suggest_frame_size_with_constraints(
                CFRange::new(0, 0),
                None,
                CGSize::new(width, f64::MAX),
                ptr::null_mut(),
            )
        };
        let height = measured.height.ceil().max((size * 1.4).ceil());
        let rect = CGRect::new(CGPoint::new(0., 0.), CGSize::new(width, height));
        let path = unsafe { CGPath::with_rect(rect, ptr::null()) };
        let frame = unsafe { setter.frame(CFRange::new(0, 0), &path, None) };
        let Some(dim) = rgba(&content["dim"]) else {
            return;
        };
        let Some(color) = rgba(&content["color"]) else {
            return;
        };
        let Some(space) = CGColorSpace::new_device_rgb() else {
            return;
        };
        let bitmap = || unsafe {
            CGBitmapContextCreate(
                ptr::null_mut(),
                (width * scale).ceil() as usize,
                (height * scale).ceil() as usize,
                8,
                0,
                Some(&space),
                CGImageAlphaInfo::PremultipliedLast.0,
            )
        };
        let Some(glyph_context) = bitmap() else {
            return;
        };
        CGContext::scale_ctm(Some(&glyph_context), scale, scale);
        CGContext::set_rgb_fill_color(Some(&glyph_context), 1., 1., 1., 1.);
        unsafe {
            frame.draw(&glyph_context);
        }
        let Some(glyphs) = CGBitmapContextCreateImage(Some(&glyph_context)) else {
            return;
        };
        let tint = |color: [f64; 4]| -> Option<CFRetained<CGImage>> {
            let context = bitmap()?;
            let pixels = CGRect::new(
                CGPoint::new(0., 0.),
                CGSize::new((width * scale).ceil(), (height * scale).ceil()),
            );
            CGContext::draw_image(Some(&context), pixels, Some(&glyphs));
            CGContext::set_blend_mode(Some(&context), CGBlendMode::SourceIn);
            CGContext::set_rgb_fill_color(Some(&context), color[0], color[1], color[2], color[3]);
            CGContext::fill_rect(Some(&context), pixels);
            CGBitmapContextCreateImage(Some(&context))
        };
        let (Some(dim_image), Some(bright_image)) = (tint(dim), tint(color)) else {
            return;
        };
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        unsafe {
            state.body.setSublayers(None);
        }
        state.body.setBounds(rect);
        state.fragments.clear();
        let base = CALayer::layer();
        base.setFrame(rect);
        unsafe {
            base.setContents(Some(
                CFRetained::as_ptr(&dim_image).cast::<AnyObject>().as_ref(),
            ));
        }
        base.setContentsScale(scale);
        state.body.addSublayer(&base);
        let lines = unsafe { frame.lines() };
        let mut origins = vec![CGPoint::new(0., 0.); lines.count() as usize];
        if let Some(first) = origins.first_mut() {
            unsafe {
                frame.line_origins(CFRange::new(0, 0), ptr::NonNull::from(first));
            }
        }
        let mut offset = 0;
        for word in &words {
            let length = word.text.encode_utf16().count() as isize;
            let mut rects = Vec::new();
            for (index, origin) in origins.iter().enumerate() {
                let line = unsafe { &*lines.value_at_index(index as isize).cast::<CTLine>() };
                let range = unsafe { line.string_range() };
                let from = offset.max(range.location);
                let end = (offset + length).min(range.location + range.length);
                if end <= from {
                    continue;
                }
                let x0 = unsafe { line.offset_for_string_index(from, ptr::null_mut()) };
                let x1 = unsafe { line.offset_for_string_index(end, ptr::null_mut()) };
                let (mut ascent, mut descent, mut leading) = (0., 0., 0.);
                unsafe {
                    line.typographic_bounds(&mut ascent, &mut descent, &mut leading);
                }
                let width = (x1 - x0).abs();
                if width > 0. {
                    rects.push(CGRect::new(
                        CGPoint::new(origin.x + x0.min(x1), height - origin.y - ascent),
                        CGSize::new(width, (ascent + descent + leading).ceil() + 1.),
                    ));
                }
            }
            let widths: Vec<f64> = rects.iter().map(|rect| rect.size.width).collect();
            for (r, (start, end)) in rects
                .into_iter()
                .zip(fragment_times(word.start, word.end, &widths))
            {
                let clip = CALayer::layer();
                clip.setAnchorPoint(CGPoint::new(0., 0.));
                clip.setPosition(r.origin);
                clip.setBounds(CGRect::new(
                    CGPoint::new(0., 0.),
                    CGSize::new(0., r.size.height),
                ));
                clip.setMasksToBounds(true);
                let bright = CALayer::layer();
                bright.setFrame(CGRect::new(
                    CGPoint::new(-r.origin.x, -r.origin.y),
                    rect.size,
                ));
                unsafe {
                    bright.setContents(Some(
                        CFRetained::as_ptr(&bright_image)
                            .cast::<AnyObject>()
                            .as_ref(),
                    ));
                }
                bright.setContentsScale(scale);
                clip.addSublayer(&bright);
                state.body.addSublayer(&clip);
                state.fragments.push(Fragment {
                    clip,
                    width: r.size.width,
                    height: r.size.height,
                    start,
                    end,
                });
            }
            offset += length;
        }
        CATransaction::commit();
        state.built = Some((content, width, scale));
        if state.height != height {
            state.height = height;
            state.context.set_size(width as f32, height as f32);
        }
        let now = CACurrentMediaTime();
        let clock = Clock {
            position: state.clock.position
                + if state.clock.playing {
                    now - state.anchored_at
                } else {
                    0.
                },
            ..state.clock.clone()
        };
        state.sync(clock, true);
    }
}

fn rgba(value: &Value) -> Option<[f64; 4]> {
    let values = value.as_array()?;
    Some([
        values.first()?.as_f64()?,
        values.get(1)?.as_f64()?,
        values.get(2)?.as_f64()?,
        values.get(3)?.as_f64()?,
    ])
}
impl State {
    fn sync(&mut self, clock: Clock, force: bool) {
        let now = CACurrentMediaTime();
        if !force && !needs_resync(&self.clock, self.anchored_at, &clock, now) {
            return;
        }
        self.clock = clock;
        self.anchored_at = now;
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        for fragment in &self.fragments {
            fragment.clip.removeAllAnimations();
            let duration = fragment.end - fragment.start;
            let fill = if duration > 0. {
                ((self.clock.position - fragment.start) / duration).clamp(0., 1.)
            } else if self.clock.position >= fragment.start {
                1.
            } else {
                0.
            };
            fragment.clip.setBounds(CGRect::new(
                CGPoint::new(0., 0.),
                CGSize::new(fragment.width * fill, fragment.height),
            ));
            if self.clock.playing && self.clock.position < fragment.end {
                let animation = CABasicAnimation::animationWithKeyPath(Some(&NSString::from_str(
                    "bounds.size.width",
                )));
                unsafe {
                    animation
                        .setFromValue(Some(&NSNumber::numberWithDouble(fragment.width * fill)));
                    animation.setToValue(Some(&NSNumber::numberWithDouble(fragment.width)));
                }
                animation.setBeginTime(
                    fragment.clip.convertTime_fromLayer(now, None)
                        + (fragment.start - self.clock.position).max(0.),
                );
                animation.setDuration(
                    (fragment.end - self.clock.position.max(fragment.start)).max(0.001),
                );
                unsafe {
                    animation.setFillMode(kCAFillModeBoth);
                    animation.setTimingFunction(Some(&CAMediaTimingFunction::functionWithName(
                        kCAMediaTimingFunctionLinear,
                    )));
                }
                animation.setRemovedOnCompletion(false);
                fragment
                    .clip
                    .addAnimation_forKey(&animation, Some(&NSString::from_str("sing")));
            }
        }
        CATransaction::commit();
    }
}
struct KaraokeLayer(Retained<KaraokeView>);
impl NativeComponent for KaraokeLayer {
    fn view(&self) -> Retained<NSView> {
        self.0.clone().into_super()
    }
    fn set_prop(&mut self, name: &str, value: Option<&Value>) {
        if name == "clock" {
            if let Some(clock) = value.and_then(|value| serde_json::from_value(value.clone()).ok())
            {
                self.0.ivars().borrow_mut().sync(clock, false);
            }
        } else {
            let mut state = self.0.ivars().borrow_mut();
            if let Some(value) = value {
                state.props.insert(name.into(), value.clone());
            } else {
                state.props.remove(name);
            }
            drop(state);
            self.0.setNeedsLayout(true);
        }
    }
}
pub fn register(components: &Components) {
    components.register(
        "haiku.KaraokeLayer",
        Rc::new(|context| Box::new(KaraokeLayer(KaraokeView::new(context)))),
    );
    REGISTERED.store(true, Ordering::Release);
}
