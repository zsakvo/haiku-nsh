//! Isolated, ephemeral WebKit session. All operations and destruction are main-thread only.
use block2::RcBlock;
use nsh_platform_macos::{
    objc2::{
        msg_send,
        rc::{Allocated, Retained},
        runtime::{AnyClass, AnyObject, Bool},
        MainThreadMarker,
    },
    objc2_foundation::{NSArray, NSHTTPCookie, NSRect, NSString, NSURLRequest, NSURL},
};
use std::sync::mpsc::{self, Receiver};

#[link(name = "WebKit", kind = "framework")]
extern "C" {}

#[derive(Debug)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub http_only: bool,
}

pub struct WebSession {
    web: Retained<AnyObject>,
    store: Retained<AnyObject>,
    // !Send/!Sync: this WebKit state belongs to the main thread.
    _main: MainThreadMarker,
}

impl WebSession {
    pub fn new(mtm: MainThreadMarker) -> Self {
        unsafe {
            let store: Retained<AnyObject> = msg_send![
                AnyClass::get(c"WKWebsiteDataStore").expect("WebKit linked"),
                nonPersistentDataStore
            ];
            let config: Retained<AnyObject> =
                msg_send![AnyClass::get(c"WKWebViewConfiguration").unwrap(), new];
            let _: () = msg_send![&config, setWebsiteDataStore: &*store];
            let allocated: Allocated<AnyObject> =
                msg_send![AnyClass::get(c"WKWebView").unwrap(), alloc];
            let web: Retained<AnyObject> =
                msg_send![allocated, initWithFrame: NSRect::ZERO, configuration: &*config];
            Self {
                web,
                store,
                _main: mtm,
            }
        }
    }

    pub fn load(&self, url: &str) -> Result<(), String> {
        let url = NSURL::URLWithString(&NSString::from_str(url)).ok_or("Invalid login URL")?;
        let request = NSURLRequest::requestWithURL(&url);
        let _: Option<Retained<AnyObject>> =
            unsafe { msg_send![&self.web, loadRequest: &*request] };
        Ok(())
    }

    pub fn cookies(&self) -> Receiver<Vec<Cookie>> {
        let (tx, rx) = mpsc::channel();
        let completion = RcBlock::new(move |cookies: *mut NSArray<NSHTTPCookie>| {
            let cookies = unsafe { &*cookies };
            let values = cookies
                .iter()
                .map(|cookie| Cookie {
                    name: cookie.name().to_string(),
                    value: cookie.value().to_string(),
                    http_only: cookie.isHTTPOnly(),
                })
                .collect();
            let _ = tx.send(values);
        });
        unsafe {
            let store: Retained<AnyObject> = msg_send![&self.store, httpCookieStore];
            let _: () = msg_send![&store, getAllCookies: &*completion];
        }
        rx
    }

    pub fn loading(&self) -> bool {
        let loading: Bool = unsafe { msg_send![&self.web, isLoading] };
        loading.as_bool()
    }
}

impl Drop for WebSession {
    fn drop(&mut self) {
        let _: () = unsafe { msg_send![&self.web, stopLoading] };
    }
}
