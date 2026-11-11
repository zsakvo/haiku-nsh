//! Explicit fake GitHub API shared by Rust tests and isolated application previews.
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::Duration,
};
#[derive(Default)]
pub struct Data {
    pub files: Value,
    pub revision: usize,
    pub calls: Vec<(String, String, Value)>,
}
pub struct Server {
    pub url: String,
    pub data: Arc<Mutex<Data>>,
    pub hold: Arc<AtomicBool>,
    pub failure: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Server {
    pub fn start() -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let url = format!(
            "http://{}",
            listener.local_addr().map_err(|e| e.to_string())?
        );
        let data = Arc::new(Mutex::new(Data::default()));
        let hold = Arc::new(AtomicBool::new(false));
        let failure = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (state, held, failed, stopped) =
            (data.clone(), hold.clone(), failure.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            let mut workers = vec![];
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut input = vec![];
                let mut buffer = [0; 4096];
                let header_end = loop {
                    let n = stream.read(&mut buffer).unwrap_or(0);
                    if n == 0 {
                        break 0;
                    }
                    input.extend_from_slice(&buffer[..n]);
                    if let Some(at) = input.windows(4).position(|w| w == b"\r\n\r\n") {
                        break at + 4;
                    }
                };
                if header_end == 0 {
                    continue;
                }
                let head = String::from_utf8_lossy(&input[..header_end]);
                let mut words = head.split_whitespace();
                let method = words.next().unwrap_or("").to_owned();
                let path = words.next().unwrap_or("").to_owned();
                if path == "/__stop" {
                    break;
                }
                let authenticated = head.contains("Bearer fixture-token")
                    || head.contains("Bearer replacement-token");
                let length = head
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                while input.len() - header_end < length {
                    let n = stream.read(&mut buffer).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    input.extend_from_slice(&buffer[..n]);
                }
                let body: Value =
                    serde_json::from_slice(&input[header_end..]).unwrap_or(Value::Null);
                state
                    .lock()
                    .unwrap()
                    .calls
                    .push((method.clone(), path.clone(), body.clone()));
                let (state, held, failed, stopped) =
                    (state.clone(), held.clone(), failed.clone(), stopped.clone());
                workers.push(std::thread::spawn(move || {
                    if path != "/user" {
                        while held.load(Ordering::SeqCst) && !stopped.load(Ordering::SeqCst) {
                            std::thread::sleep(Duration::from_millis(3));
                        }
                    }
                    let mut status = if authenticated { 200 } else { 401 };
                    if path != "/user" && failed.load(Ordering::SeqCst) != 0 { status = failed.load(Ordering::SeqCst); }
                    let mut state = state.lock().unwrap();
                    let response = if status != 200 { json!({"message":"fixture failure"}) }
                    else if path == "/user" { json!({"login":"haiku-listener"}) }
                    else if (path == "/gists" && method == "POST") || (path == "/gists/abc123" && method == "PATCH") {
                        state.files = body["files"].clone(); state.revision += 1;
                        json!({"id":"abc123","updated_at":format!("fixture-version-{}", state.revision),"files":state.files})
                    } else if path == "/gists/abc123" && state.revision > 0 {
                        json!({"id":"abc123","updated_at":format!("fixture-version-{}", state.revision),"files":state.files})
                    } else { status = 404; json!({"message":"missing"}) };
                    drop(state);
                    let body = response.to_string();
                    let response = format!("HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    let _ = stream.write_all(response.as_bytes());
                }));
            }
            for worker in workers {
                let _ = worker.join();
            }
        });
        Ok(Self {
            url,
            data,
            hold,
            failure,
            stop,
            thread: Some(thread),
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(mut stream) = TcpStream::connect(self.url.trim_start_matches("http://")) {
            let _ = stream.write_all(b"GET /__stop HTTP/1.1\r\n\r\n");
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
