use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

pub(crate) struct Server {
    pub url: String,
    pub refreshes: Arc<AtomicUsize>,
    pub expired: Arc<AtomicBool>,
    pub refresh_mode: Arc<AtomicUsize>,
    pub hold_refresh: Arc<AtomicBool>,
    pub calls: Arc<Mutex<Vec<(String, Value)>>>,
    pub remote: Arc<Mutex<Vec<Value>>>,
    pub fail_stats: Arc<AtomicBool>,
    pub hold_stats: Arc<AtomicBool>,
    pub repeat_cursor: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let refreshes = Arc::new(AtomicUsize::new(0));
        let expired = Arc::new(AtomicBool::new(false));
        let refresh_mode = Arc::new(AtomicUsize::new(0));
        let hold_refresh = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(Mutex::new(vec![]));
        let remote = Arc::new(Mutex::new(vec![]));
        let fail_stats = Arc::new(AtomicBool::new(false));
        let hold_stats = Arc::new(AtomicBool::new(false));
        let repeat_cursor = Arc::new(AtomicBool::new(false));
        let (remote_data, stats_fail, stats_hold, repeated) = (
            remote.clone(),
            fail_stats.clone(),
            hold_stats.clone(),
            repeat_cursor.clone(),
        );
        let (count, expire, mode, hold, stopped, records) = (
            refreshes.clone(),
            expired.clone(),
            refresh_mode.clone(),
            hold_refresh.clone(),
            stop.clone(),
            calls.clone(),
        );
        let thread = std::thread::spawn(move || {
            let mut workers = vec![];
            while !stopped.load(Ordering::SeqCst) {
                let (mut socket, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(3));
                        continue;
                    }
                };
                let (count, expire, mode, hold, stopped, records) = (
                    count.clone(),
                    expire.clone(),
                    mode.clone(),
                    hold.clone(),
                    stopped.clone(),
                    records.clone(),
                );
                let (remote, fail_stats, hold_stats, repeat_cursor) = (
                    remote_data.clone(),
                    stats_fail.clone(),
                    stats_hold.clone(),
                    repeated.clone(),
                );
                workers.push(std::thread::spawn(move || {
                    // Darwin may inherit the listener's O_NONBLOCK flag on accepted sockets.
                    // A request arriving after accept must block, rather than be treated as EOF.
                    socket.set_nonblocking(false).unwrap();
                    socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                    let mut input = Vec::new();
                    let mut buf = [0; 4096];
                    let header_end = loop {
                        let n = socket.read(&mut buf).unwrap_or(0);
                        if n == 0 { return; }
                        input.extend_from_slice(&buf[..n]);
                        if let Some(at) = input.windows(4).position(|w| w == b"\r\n\r\n") { break at + 4; }
                    };
                    let head = String::from_utf8_lossy(&input[..header_end]).into_owned();
                    let length = head.lines().find_map(|line| line.to_lowercase()
                        .strip_prefix("content-length: ").and_then(|n| n.parse::<usize>().ok())).unwrap_or(0);
                    while input.len() - header_end < length {
                        let n = socket.read(&mut buf).unwrap_or(0);
                        if n == 0 { return; }
                        input.extend_from_slice(&buf[..n]);
                    }
                    let path = head.split_whitespace().nth(1).unwrap().to_string();
                    let body: Value = serde_json::from_slice(&input[header_end..]).unwrap_or(Value::Null);
                    records.lock().unwrap().push((path.clone(), body.clone()));
                    if path.contains("/listening/") {
                        while hold_stats.load(Ordering::SeqCst) && !stopped.load(Ordering::SeqCst) {
                            std::thread::sleep(Duration::from_millis(3));
                        }
                    }
                    let mut status = "200 OK";
                    let mut code = 0;
                    let data = match path.as_str() {
                        "/api/v1/auth/code" => json!({}),
                        "/api/v1/auth/login" if body["code"] == "bad" => { code = 10003; Value::Null }
                        "/api/v1/auth/login" => json!({
                            "token":{"access_token":"old","refresh_token":"refresh-original"},
                            "user":{"id":if body["email"] == "other@example.com" {8} else {7},
                                "email":body["email"],"name":"测试用户"},
                        }),
                        "/api/v1/auth/refresh" => {
                            count.fetch_add(1, Ordering::SeqCst);
                            while hold.load(Ordering::SeqCst) && !stopped.load(Ordering::SeqCst) {
                                std::thread::sleep(Duration::from_millis(3));
                            }
                            match mode.load(Ordering::SeqCst) {
                                1 => { code = 10002; Value::Null }
                                2 => return, // A closed socket is a transport failure, not invalid auth.
                                _ => json!({"access_token":"new"}),
                            }
                        }
                        "/api/v1/me/subscription" => json!({"active":true,"is_permanent":true}),
                        "/api/v1/plans" => json!([{"id":1,"active":true,"sort":1},{"id":2,"active":false}]),
                        "/api/v1/me" => json!({"id":7,"email":"test@example.com","name":"测试用户"}),
                        "/api/v1/auth/logout" => Value::Null,
                        _ if path.contains("/listening/") && fail_stats.load(Ordering::SeqCst) => {
                            status = "503 Service Unavailable"; code = 503; Value::Null
                        }
                        "/api/v1/listening/events" if head.starts_with("POST") => json!({"accepted":1}),
                        "/api/v1/listening/meta" => json!({"exists":true,"revision":2}),
                        _ if path.starts_with("/api/v1/listening/events?") => json!({
                            "events":remote.lock().unwrap().clone(),"revision":2,
                            "has_more":repeat_cursor.load(Ordering::SeqCst),"next_cursor":"repeated",
                        }),
                        "/api/v1/protected" | "/api/v1/code-expired" => {
                            if expire.load(Ordering::SeqCst) && head.contains("Bearer old") {
                                code = 10002;
                                if path.ends_with("protected") { status = "401 Unauthorized"; }
                            }
                            json!({"ok":true})
                        }
                        _ => { code = 404; Value::Null }
                    };
                    let body = json!({"code":code,"message":if code == 10003 {"验证码错误"} else {"fixture"},"data":data}).to_string();
                    let response = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    let _ = socket.write_all(response.as_bytes());
                }));
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            url,
            refreshes,
            expired,
            refresh_mode,
            hold_refresh,
            calls,
            remote,
            fail_stats,
            hold_stats,
            repeat_cursor,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}
fn fixture() -> (
    tempfile::TempDir,
    Arc<Service>,
    Arc<MemoryCredentials>,
    Server,
) {
    let root = tempfile::tempdir().unwrap();
    let tokens = Arc::new(MemoryCredentials::default());
    let account = Service::open(root.path(), tokens.clone()).unwrap();
    let server = Server::start();
    account.set_server(server.url.clone()).unwrap();
    (root, account, tokens, server)
}
async fn signed_in(account: &Service) {
    account
        .login("test@example.com".into(), "123456".into())
        .await
        .unwrap();
}

#[tokio::test]
async fn account_login_cache_cooldown_restore_and_logout() {
    let (root, account, tokens, server) = fixture();
    account.send_code("Test@example.com".into()).await.unwrap();
    assert_eq!(
        account
            .send_code(" test@example.com ".into())
            .await
            .unwrap_err()
            .code,
        10006
    );
    assert_eq!(
        server
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(p, _)| p.ends_with("/auth/code"))
            .count(),
        1
    );
    assert_eq!(
        account
            .login("test@example.com".into(), "bad".into())
            .await
            .unwrap_err()
            .code,
        10003
    );
    signed_in(&account).await;
    assert!(account.snapshot().signed_in);
    assert_eq!(account.snapshot().cache.user["id"], 7);
    account.subscription(false).await.unwrap();
    assert_eq!(
        server
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(p, _)| p.ends_with("/me/subscription"))
            .count(),
        1
    );
    assert_eq!(account.plans().await.unwrap().as_array().unwrap().len(), 1);
    let restored = Service::open(root.path(), tokens.clone()).unwrap();
    assert!(restored.snapshot().signed_in);
    restored.refresh().await.unwrap();
    let cached =
        std::fs::read_to_string(root.path().join("haiku_state/account_cache.json")).unwrap();
    assert!(!cached.contains("access_token") && !cached.contains("refresh_token"));
    restored.logout().await.unwrap();
    assert!(!restored.snapshot().signed_in);
    assert!(tokens.read().unwrap().is_none());
    assert!(
        !Service::open(root.path(), tokens)
            .unwrap()
            .snapshot()
            .signed_in
    );
}

#[tokio::test]
async fn account_concurrent_401_single_refresh_replay_and_keeps_refresh_token() {
    let (_root, account, tokens, server) = fixture();
    signed_in(&account).await;
    server.expired.store(true, Ordering::SeqCst);
    let requests = (0..12).map(|i| {
        account.request(
            "GET",
            if i % 2 == 0 {
                "/protected"
            } else {
                "/code-expired"
            },
            None,
        )
    });
    for result in futures::future::join_all(requests).await {
        assert_eq!(result.unwrap()["ok"], true);
    }
    assert_eq!(server.refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(
        tokens.read().unwrap().unwrap().refresh_token,
        "refresh-original"
    );
}

#[tokio::test]
async fn account_invalid_refresh_clears_but_transport_keeps_offline_session() {
    let (root, account, tokens, server) = fixture();
    signed_in(&account).await;
    server.expired.store(true, Ordering::SeqCst);
    server.refresh_mode.store(2, Ordering::SeqCst);
    assert!(
        account
            .request("GET", "/protected", None)
            .await
            .unwrap_err()
            .transport
    );
    assert!(account.snapshot().signed_in && tokens.read().unwrap().is_some());
    let restored = Service::open(root.path(), tokens.clone()).unwrap();
    drop(server);
    assert!(restored.refresh().await.unwrap_err().transport);
    assert!(restored.snapshot().signed_in);
    let server = Server::start();
    account.set_server(server.url.clone()).unwrap();
    signed_in(&account).await;
    server.expired.store(true, Ordering::SeqCst);
    server.refresh_mode.store(1, Ordering::SeqCst);
    assert!(account.request("GET", "/protected", None).await.is_err());
    assert!(!account.snapshot().signed_in);
    assert!(tokens.read().unwrap().is_none());
}

#[tokio::test]
async fn account_logout_and_server_change_discard_late_refresh() {
    for switch in [false, true] {
        let (_root, account, tokens, server) = fixture();
        signed_in(&account).await;
        server.expired.store(true, Ordering::SeqCst);
        server.hold_refresh.store(true, Ordering::SeqCst);
        let pending = account.clone();
        let request = tokio::spawn(async move { pending.request("GET", "/protected", None).await });
        tokio::time::timeout(Duration::from_secs(2), async {
            while server.refreshes.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(3)).await;
            }
        })
        .await
        .unwrap();
        if switch {
            account
                .set_server(format!("{}/api/v1", server.url))
                .unwrap();
        } else {
            account.logout().await.unwrap();
        }
        server.hold_refresh.store(false, Ordering::SeqCst);
        assert_eq!(request.await.unwrap().unwrap_err().code, -2);
        assert!(tokens.read().unwrap().is_none());
        assert!(!account.snapshot().signed_in);
    }
}
