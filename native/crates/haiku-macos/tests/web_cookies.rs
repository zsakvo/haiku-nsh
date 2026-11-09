//! A custom harness keeps WebKit and its run loop on the process main thread.
use haiku_macos::web_auth::{Cookie, WebSession};
use nsh_platform_macos::{
    objc2::{rc::autoreleasepool, MainThreadMarker},
    objc2_app_kit::NSApplication,
    objc2_foundation::{NSDate, NSRunLoop},
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc::Receiver,
    time::{Duration, Instant},
};

fn pump() {
    NSRunLoop::mainRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.02));
}
fn read_cookies(rx: Receiver<Vec<Cookie>>) -> Vec<Cookie> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(cookies) = rx.try_recv() {
            return cookies;
        }
        assert!(Instant::now() < deadline, "cookie callback timeout");
        pump();
    }
}

fn main() {
    autoreleasepool(|_| {
        let mtm = MainThreadMarker::new().expect("test must run on main thread");
        let _app = NSApplication::sharedApplication(mtm);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/login", listener.local_addr().unwrap());
        let fixture = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 4096];
            let n = stream.read(&mut buffer).unwrap();
            assert!(String::from_utf8_lossy(&buffer[..n]).starts_with("GET /login "));
            let body = "<html><body>Local cookie fixture</body></html>";
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nSet-Cookie: visible=public; Path=/\r\nSet-Cookie: secret=private; Path=/; HttpOnly\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        });
        let first = WebSession::new(mtm);
        first.load(&url).unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        // Cookie callbacks and navigation both require the main run loop.
        let cookies = loop {
            let cookies = read_cookies(first.cookies());
            if !first.loading() && cookies.len() == 2 {
                break cookies;
            }
            assert!(Instant::now() < deadline, "fixture navigation timeout");
            pump();
        };
        assert!(cookies
            .iter()
            .any(|c| c.name == "visible" && c.value == "public" && !c.http_only));
        assert!(cookies
            .iter()
            .any(|c| c.name == "secret" && c.value == "private" && c.http_only));
        let second = WebSession::new(mtm);
        assert!(
            read_cookies(second.cookies()).is_empty(),
            "new session must be isolated"
        );
        // The second session is opened while the first is alive, so a shared
        // nonpersistent store would fail this assertion.
        fixture.join().unwrap();
        println!("S3 PASS: /login loaded; two cookies including HttpOnly; second session empty");
    });
}
