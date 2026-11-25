//! Native macOS popup menu (`NSMenu`) shown at the current mouse cursor.
//! Supports nested submenus and disabled items; non-macOS builds compile as a no-op.

pub use crate::menubar::MenuItemSpec;

#[cfg(not(target_os = "macos"))]
pub fn show(_items: &[MenuItemSpec]) -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
pub fn show(items: &[MenuItemSpec]) -> Option<String> {
    imp::show(items)
}

#[cfg(target_os = "macos")]
mod imp {
    use super::MenuItemSpec;
    use cocoa::base::{id, nil, NO, YES};
    use cocoa::foundation::{NSPoint, NSString};
    use objc::declare::ClassDecl;
    use objc::runtime::{Class, Object, Sel, BOOL};
    use objc::{class, msg_send, sel, sel_impl};
    use std::cell::RefCell;
    use std::ffi::{CStr, c_void};
    use std::sync::OnceLock;

    static TARGET_CLASS: OnceLock<&'static Class> = OnceLock::new();

    fn target_class() -> &'static Class {
        TARGET_CLASS.get_or_init(|| unsafe {
            let mut decl = ClassDecl::new("HaikuPopupMenuTarget", class!(NSObject))
                .expect("register HaikuPopupMenuTarget");
            decl.add_ivar::<*mut c_void>("selected");
            decl.add_method(
                sel!(onItem:),
                on_item as extern "C" fn(&Object, Sel, id),
            );
            decl.register()
        })
    }

    extern "C" fn on_item(this: &Object, _: Sel, sender: id) {
        unsafe {
            let ptr: *mut c_void = *this.get_ivar("selected");
            if ptr.is_null() {
                return;
            }
            let obj: id = msg_send![sender, representedObject];
            if obj == nil {
                return;
            }
            let cstr: *const std::os::raw::c_char = msg_send![obj, UTF8String];
            if cstr.is_null() {
                return;
            }
            let value = CStr::from_ptr(cstr).to_string_lossy().into_owned();
            let slot = &*(ptr as *const RefCell<Option<String>>);
            *slot.borrow_mut() = Some(value);
        }
    }

    unsafe fn build_menu(items: &[MenuItemSpec], target: id) -> id {
        let menu: id = msg_send![class!(NSMenu), alloc];
        let menu: id = msg_send![menu, init];
        let _: () = msg_send![menu, setAutoenablesItems: NO];
        for spec in items {
            if spec.is_separator {
                let sep: id = msg_send![class!(NSMenuItem), separatorItem];
                let _: () = msg_send![menu, addItem: sep];
                continue;
            }
            let title = NSString::alloc(nil).init_str(&spec.title);
            let empty = NSString::alloc(nil).init_str("");
            let item: id = msg_send![class!(NSMenuItem), alloc];
            let action = if spec.children.is_empty() {
                sel!(onItem:)
            } else {
                Sel::from_ptr(std::ptr::null())
            };
            let item: id = msg_send![item, initWithTitle:title action:action keyEquivalent:empty];
            let _: () = msg_send![item, setEnabled: if spec.enabled { YES } else { NO }];
            if !spec.children.is_empty() {
                let sub = build_menu(&spec.children, target);
                let _: () = msg_send![item, setSubmenu: sub];
            } else {
                let _: () = msg_send![item, setTarget: target];
                let rep = NSString::alloc(nil).init_str(&spec.id);
                let _: () = msg_send![item, setRepresentedObject: rep];
            }
            let _: () = msg_send![menu, addItem: item];
        }
        menu
    }

    unsafe fn show_on_main(items: &[MenuItemSpec]) -> Option<String> {
        let app: id = msg_send![class!(NSApplication), sharedApplication];
        if app == nil {
            return None;
        }
        let selected = RefCell::new(None::<String>);
        let target: id = msg_send![target_class(), new];
        (*target).set_ivar::<*mut c_void>(
            "selected",
            (&selected as *const RefCell<Option<String>>) as *mut c_void,
        );
        let menu = build_menu(items, target);
        let location: NSPoint = msg_send![class!(NSEvent), mouseLocation];
        let _: BOOL = msg_send![menu, popUpMenuPositioningItem:nil atLocation:location inView:nil];
        let result = selected.into_inner();
        let _: () = msg_send![target, release];
        result
    }

    pub fn show(items: &[MenuItemSpec]) -> Option<String> {
        if items.is_empty() {
            return None;
        }
        unsafe {
            let is_main: BOOL = msg_send![class!(NSThread), isMainThread];
            if is_main == YES {
                return show_on_main(items);
            }
        }
        if !crate::lifecycle::native_host() {
            return None;
        }
        dispatch::Queue::main().exec_sync(|| unsafe { show_on_main(items) })
    }
}
