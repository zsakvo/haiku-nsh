//! The native background is a leaf; NSH overlays are later siblings.
use nsh_platform_macos::{
    objc2::{rc::Retained, MainThreadMarker},
    objc2_app_kit::{
        NSApplication, NSColor, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
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
    fn set_prop(&mut self, _: &str, _: Option<&serde_json::Value>) {}
}

/// Register before the host loads its NSH surface; factory runs on the main thread.
pub fn register(components: &Components) {
    components.register(
        "haiku.GlassSurface",
        Rc::new(|_| {
            let mtm = MainThreadMarker::new().expect("native component main thread");
            let app = NSApplication::sharedApplication(mtm);
            if let Some(window) = app.mainWindow().or_else(|| app.keyWindow()) {
                window.setOpaque(false);
                window.setBackgroundColor(Some(&NSColor::clearColor()));
            }
            let view = NSVisualEffectView::new(mtm);
            view.setMaterial(NSVisualEffectMaterial::Sidebar);
            view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
            view.setState(NSVisualEffectState::Active);
            Box::new(GlassSurface(view))
        }),
    );
}
