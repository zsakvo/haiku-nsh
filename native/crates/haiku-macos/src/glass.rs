//! The native background is a leaf; NSH overlays are later siblings.
use nsh_platform_macos::{
    objc2::{rc::Retained, MainThreadMarker},
    objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
        NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
        NSVisualEffectState, NSVisualEffectView,
    },
    Components, NativeComponent,
};
use std::rc::Rc;

struct GlassSurface(Retained<NSVisualEffectView>);
impl NativeComponent for GlassSurface {
    fn view(&self) -> Retained<NSView> {
        Retained::into_super(self.0.clone())
    }
    fn set_prop(&mut self, name: &str, value: Option<&serde_json::Value>) {
        if name == "dark" {
            // Explicitly follow the NSH surface, which can differ from the OS appearance.
            let dark = value.and_then(|v| v.as_bool()).unwrap_or(false);
            let name = unsafe { if dark { NSAppearanceNameDarkAqua } else { NSAppearanceNameAqua } };
            let appearance = NSAppearance::appearanceNamed(name);
            self.0.setAppearance(appearance.as_deref());
        } else if name == "radius" {
            use objc::{msg_send, sel, sel_impl};
            unsafe {
                let view = Retained::as_ptr(&self.0) as cocoa::base::id;
                let layer: cocoa::base::id = msg_send![view, layer];
                let radius = value.and_then(|v| v.as_f64()).unwrap_or(14.).clamp(0., 40.);
                let _: () = msg_send![layer, setCornerRadius: radius];
                let _: () = msg_send![layer, setMasksToBounds: cocoa::base::YES];
            }
        }
    }
}

/// Register before the host loads its NSH surface; factory runs on the main thread.
pub fn register(components: &Components) {
    components.register(
        "haiku.GlassSurface",
        Rc::new(|_| {
            let mtm = MainThreadMarker::new().expect("native component main thread");
            let view = NSVisualEffectView::new(mtm);
            view.setWantsLayer(true);
            // Sample this app's content, not unrelated windows behind it. No app tint.
            view.setMaterial(NSVisualEffectMaterial::UnderWindowBackground);
            view.setBlendingMode(NSVisualEffectBlendingMode::WithinWindow);
            view.setState(NSVisualEffectState::Active);
            view.setEmphasized(false);
            Box::new(GlassSurface(view))
        }),
    );
}
