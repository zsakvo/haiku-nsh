//! Keep NSH's delegate object and all its callbacks, overriding close only.
use nsh_platform_macos::{
    objc2::{
        MainThreadMarker,
        runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel},
        sel,
    },
    objc2_app_kit::{NSApplication, NSWindow},
};

static NATIVE_HOST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn native_host() -> bool {
    NATIVE_HOST.load(std::sync::atomic::Ordering::Acquire)
}

unsafe extern "C" fn hide_on_close(_: *mut AnyObject, _: Sel, window: *mut NSWindow) -> Bool {
    assert!(MainThreadMarker::new().is_some());
    unsafe { &*window }.orderOut(None);
    eprintln!("Haiku window close: hidden (engine retained)");
    Bool::NO
}

/// Install on the existing main window. Called from the engine, never the UI thread.
pub fn install() -> Result<String, String> {
    let install = || {
        let mtm = MainThreadMarker::new().expect("GCD main queue");
        let app = NSApplication::sharedApplication(mtm);
        let window = app
            .mainWindow()
            .or_else(|| app.keyWindow())
            .or_else(|| app.windows().firstObject())
            .ok_or("NSH window not ready")?;
        let delegate = window.delegate().ok_or("NSH window has no delegate")?;
        let object: &AnyObject = delegate.as_ref();
        let original = object.class();
        if original.name() == c"HaikuNshCloseDelegate" {
            return Ok("HaikuNshCloseDelegate already installed".into());
        }
        // Preserve the known NSH delegate; reject an unfamiliar embedding host.
        if original.name() != c"NshAppDelegate" {
            return Err(format!("Unsupported NSH delegate: {original:?}"));
        }
        let app_delegate = app.delegate().ok_or("NSH application has no delegate")?;
        let app_object: &AnyObject = app_delegate.as_ref();
        if !std::ptr::eq(object, app_object) {
            return Err("NSH window/application delegates differ".into());
        }
        let subclass = AnyClass::get(c"HaikuNshCloseDelegate").unwrap_or_else(|| {
            let mut builder = ClassBuilder::new(c"HaikuNshCloseDelegate", original).unwrap();
            // Exact NSWindowDelegate ABI; no ivars or other method overrides.
            unsafe {
                builder.add_method(
                    sel!(windowShouldClose:),
                    hide_on_close
                        as unsafe extern "C" fn(*mut AnyObject, Sel, *mut NSWindow) -> Bool,
                )
            };
            builder.register()
        });
        if subclass.superclass() != Some(original)
            || subclass.instance_size() != original.instance_size()
        {
            return Err("Delegate subclass layout mismatch".into());
        }
        // Both delegates refer to this same object. Main-thread confinement excludes
        // concurrent mutation; subclass has exactly the original instance layout.
        let previous = unsafe { AnyObject::set_class(object, subclass) };
        assert_eq!(previous, original);
        Ok("NSApp sharedApplication; mainWindow/keyWindow/windows; delegate identity retained; reopen/quit inherited".into())
    };
    if MainThreadMarker::new().is_some() {
        install()
    } else {
        dispatch::Queue::main().exec_sync(install)
    }
}

/// Schedule startup without blocking the engine or a headless runner.
pub fn install_async() {
    dispatch::Queue::main().exec_async(|| {
        if let Err(error) = install() {
            eprintln!("Haiku lifecycle: {error}");
        } else {
            NATIVE_HOST.store(true, std::sync::atomic::Ordering::Release);
        }
    });
}

static EXIT_HANDLER: std::sync::Mutex<Option<fn()>> = std::sync::Mutex::new(None);
/// Register Core's exit barrier independently of the media center and window delegate.
pub fn set_exit_handler(handler: fn()) {
    *EXIT_HANDLER.lock().unwrap() = Some(handler);
    dispatch::Queue::main().exec_async(|| {
        static INSTALL: std::sync::Once = std::sync::Once::new();
        INSTALL.call_once(|| unsafe {
            use cocoa::{base::{id,nil}, foundation::NSString};
            use objc::{class,msg_send,sel,sel_impl};
            let observer=block::ConcreteBlock::new(move |_notification:id| {
                let handler=*EXIT_HANDLER.lock().unwrap();
                if let Some(handler)=handler {handler();}
            }).copy();
            let center:id=msg_send![class!(NSNotificationCenter),defaultCenter];
            let name=NSString::alloc(nil).init_str("NSApplicationWillTerminateNotification");
            let _:id=msg_send![center,addObserverForName:name object:nil queue:nil usingBlock:&*observer];
            let _:()=msg_send![name,release];
        });
    });
}
