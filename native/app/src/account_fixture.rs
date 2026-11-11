//! Loopback-only account fixture retained by an explicitly isolated Core.
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread::JoinHandle,
};

pub struct Server {
    pub url: String,
    thread: Option<JoinHandle<()>>,
}
impl Server {
    pub fn start() -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let url = format!(
            "http://{}",
            listener.local_addr().map_err(|e| e.to_string())?
        );
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
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
                let path = head.split_whitespace().nth(1).unwrap_or("").to_owned();
                if path == "/__stop" {
                    break;
                }
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
                let (code, message, data) = match path.as_str() {
                    "/api/v1/auth/code" => (0, "", json!({})),
                    "/api/v1/auth/login" if body["code"] != "123456" => {
                        (10003, "验证码错误", Value::Null)
                    }
                    "/api/v1/auth/login" => (
                        0,
                        "",
                        json!({
                            "token":{"access_token":"fixture-access","refresh_token":"fixture-refresh"},
                            "user":{"id":7,"name":"林间回声","email":body["email"]},
                        }),
                    ),
                    "/api/v1/me" => (
                        0,
                        "",
                        json!({"id":7,"name":"林间回声","email":"listener@example.com"}),
                    ),
                    "/api/v1/me/subscription" => {
                        (0, "", json!({"active":true,"is_permanent":true}))
                    }
                    "/api/v1/plans" => (
                        0,
                        "",
                        json!([{"id":1,"name":"Haiku 永久会员","active":true,
                        "price_cents":9900,"permanent":true}]),
                    ),
                    "/api/v1/auth/logout" => (0, "", Value::Null),
                    "/api/v1/listening/meta" => (0, "", json!({"exists":false,"revision":0})),
                    "/api/v1/listening/events" => (0, "", json!({"accepted":0})),
                    _ => (404, "fixture endpoint missing", Value::Null),
                };
                let body = json!({"code":code,"message":message,"data":data}).to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Ok(Self {
            url,
            thread: Some(thread),
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if let Ok(mut socket) = TcpStream::connect(self.url.trim_start_matches("http://")) {
            let _ = socket.write_all(b"GET /__stop HTTP/1.1\r\n\r\n");
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
