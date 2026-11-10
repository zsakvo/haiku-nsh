//! Offline contract runner shared by cargo tests, CLI and future developer UI.
use crate::{Cancellation, Descriptor, Host, PluginError, Result, Source};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU16, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContractMode {
    #[default]
    Replay,
    Live,
    Record,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractCase {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub args: Vec<Value>,
    #[serde(default)]
    pub context: Value,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub expected_code: Option<String>,
    #[serde(default)]
    pub status: Option<u16>,
    #[serde(default)]
    pub abort_after_ms: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpFixture {
    pub path: String,
    #[serde(default = "get")]
    pub method: String,
    #[serde(default)]
    pub query: BTreeMap<String, String>,
    #[serde(default = "ok")]
    pub status: u16,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    pub body: Value,
    #[serde(default)]
    pub delay_ms: u64,
}
fn get() -> String {
    "GET".into()
}
fn ok() -> u16 {
    200
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractFixture {
    #[serde(default)]
    pub connection: BTreeMap<String, String>,
    #[serde(default)]
    pub login: Option<Value>,
    #[serde(default)]
    pub routes: Vec<HttpFixture>,
    pub cases: Vec<ContractCase>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseReport {
    pub name: String,
    pub passed: bool,
    pub duration_ms: u64,
    pub error: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractReport {
    pub schema_version: u32,
    pub plugin_id: String,
    pub passed: bool,
    pub cases: Vec<CaseReport>,
}
impl ContractReport {
    pub fn summary(&self) -> String {
        format!(
            "{}: {}/{} contracts passed",
            self.plugin_id,
            self.cases.iter().filter(|c| c.passed).count(),
            self.cases.len()
        )
    }
}
/// Loopback fixture server retained by the contract runner or an explicit developer preview.
pub struct ReplayServer {
    base: String,
    status: Arc<AtomicU16>,
    unmatched: Arc<Mutex<Vec<String>>>,
    recorded: Arc<Mutex<Vec<HttpFixture>>>,
    thread: Option<std::thread::JoinHandle<()>>,
    #[cfg(test)]
    requests: Arc<Mutex<Vec<String>>>,
}
impl ReplayServer {
    /// Serve package fixtures on loopback; no upstream requests are made.
    pub fn from_package(package: &Path) -> Result<Self> {
        let fixture: ContractFixture =
            serde_json::from_slice(&std::fs::read(package.join("fixtures/contract.json"))?)?;
        Self::start(fixture.routes, false)
    }
    /// Base URL for local fixture connections; dropping the server closes its listener.
    pub fn url(&self) -> &str {
        &self.base
    }

    fn start(routes: Vec<HttpFixture>, record: bool) -> Result<Self> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let base = format!("http://{}", listener.local_addr()?);
        let status = Arc::new(AtomicU16::new(0));
        let unmatched = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let (forced, missing, captures) = (status.clone(), unmatched.clone(), recorded.clone());
        #[cfg(test)]
        let requests = Arc::new(Mutex::new(Vec::new()));
        #[cfg(test)]
        let request_log = requests.clone();
        let thread = std::thread::spawn(move || {
            let routes = Arc::new(routes);
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut bytes = Vec::new();
                let mut buf = [0; 4096];
                let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
                let (head, body) = loop {
                    let Ok(n) = stream.read(&mut buf) else { return };
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&bytes[..end]).to_string();
                        let length = head
                            .lines()
                            .find_map(|line| {
                                line.split_once(':')
                                    .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                                    .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break (head, bytes[end + 4..end + 4 + length].to_vec());
                        }
                    }
                };
                let mut parts = head.split_whitespace();
                let method = parts.next().unwrap_or("GET").to_string();
                let path = parts.next().unwrap_or("/");
                if path == "/__stop" {
                    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
                    break;
                }
                let target = url::Url::parse(&format!("http://fixture{path}"))
                    .ok()
                    .and_then(|u| {
                        u.query_pairs()
                            .find(|(k, _)| k == "target")
                            .map(|(_, v)| v.into_owned())
                    })
                    .or_else(|| Some(format!("http://fixture{path}")));
                #[cfg(test)]
                request_log.lock().unwrap().push(format!(
                    "{target:?}\n{head}\n{}",
                    String::from_utf8_lossy(&body)
                ));
                let routes = routes.clone();
                let forced = forced.load(Ordering::Acquire);
                let missing = missing.clone();
                let captures = captures.clone();
                std::thread::spawn(move || {
                    let target = target.and_then(|s| url::Url::parse(&s).ok());
                    let found = target.as_ref().and_then(|target| {
                        routes.iter().find(|route| {
                            route.path == target.path()
                                && route.method == method
                                && route.query.iter().all(|(k, v)| {
                                    target.query_pairs().any(|(key, value)| {
                                        key == k.as_str() && value == v.as_str()
                                    })
                                })
                        })
                    });
                    let route = if record {
                        target.as_ref().and_then(|target| {
                            let runtime = tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()
                                .ok()?;
                            runtime.block_on(async {
                                let mut request = reqwest::Client::new()
                                    .request(method.parse().ok()?, target.as_str())
                                    .body(body);
                                for line in head.lines().skip(1) {
                                    if let Some((key, value)) = line.split_once(':') {
                                        if !["host", "content-length", "connection"]
                                            .iter()
                                            .any(|h| key.eq_ignore_ascii_case(h))
                                        {
                                            request = request.header(key, value.trim());
                                        }
                                    }
                                }
                                let response = request.send().await.ok()?;
                                let status = response.status().as_u16();
                                // Record no credentials/cookies or authentication response headers.
                                let headers = BTreeMap::new();
                                let text = response.text().await.ok()?;
                                let body =
                                    serde_json::from_str(&text).unwrap_or(Value::String(text));
                                Some(HttpFixture {
                                    path: target.path().into(),
                                    method: method.clone(),
                                    query: BTreeMap::new(),
                                    status,
                                    headers,
                                    body,
                                    delay_ms: 0,
                                })
                            })
                        })
                    } else {
                        found.cloned()
                    };
                    let route = route.unwrap_or_else(|| {
                        missing.lock().unwrap().push(
                            target
                                .as_ref()
                                .map(|u| u.path().to_owned())
                                .unwrap_or_default(),
                        );
                        HttpFixture {
                            path: String::new(),
                            method,
                            query: BTreeMap::new(),
                            status: 599,
                            headers: BTreeMap::new(),
                            body: json!({"error":"unmatched offline fixture"}),
                            delay_ms: 0,
                        }
                    });
                    if record {
                        captures.lock().unwrap().push(route.clone());
                    }
                    std::thread::sleep(Duration::from_millis(route.delay_ms));
                    let body = match &route.body {
                        Value::String(s) => s.clone(),
                        value => value.to_string(),
                    };
                    let status = if forced == 0 || route.method == "POST" {
                        route.status
                    } else {
                        forced
                    };
                    let headers = route
                        .headers
                        .iter()
                        .map(|(k, v)| format!("{k}: {v}\r\n"))
                        .collect::<String>();
                    let response = format!(
                        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                });
            }
        });
        Ok(Self {
            base,
            status,
            unmatched,
            recorded,
            thread: Some(thread),
            #[cfg(test)]
            requests,
        })
    }
}
impl Drop for ReplayServer {
    fn drop(&mut self) {
        if let Ok(mut stream) =
            std::net::TcpStream::connect(self.base.trim_start_matches("http://"))
        {
            let _ = stream.write_all(b"GET /__stop HTTP/1.1\r\nHost: fixture\r\n\r\n");
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn invalid(message: &str) -> PluginError {
    PluginError::new("CONTRACT", message)
}
fn entity(kind: &str, value: &Value) -> Result<()> {
    if !value.is_object()
        || value[if kind == "artist" { "name" } else { "title" }]
            .as_str()
            .is_none_or(|s| s.is_empty())
    {
        return Err(invalid("Entity missing title/name"));
    }
    if kind != "artist" && value["id"].as_str().is_none_or(|s| s.is_empty()) {
        return Err(invalid("Entity missing native id"));
    }
    if kind == "track" {
        let mut track = value.clone();
        track["source"] = json!("contract");
        serde_json::from_value::<haiku_model::Track>(track).map_err(|e| invalid(&e.to_string()))?;
    }
    Ok(())
}
/// Strict validation rejects bad entries instead of silently accepting host filtering.
pub fn validate_schema(schema: &str, value: &Value) -> Result<()> {
    if schema == "home" {
        let shelves = value["shelves"]
            .as_array()
            .ok_or_else(|| invalid("Home.shelves must be an array"))?;
        for shelf in shelves {
            if shelf["id"].as_str().is_none() || shelf["title"].as_str().is_none() {
                return Err(invalid("Shelf missing identity/title"));
            }
            let kind = match shelf["type"].as_str() {
                Some("tracks") => "track",
                Some("albums") => "album",
                Some("playlists") => "playlist",
                Some("toplists") => "toplist",
                Some("programs") => "program",
                Some("entries") => "entry",
                _ => return Err(invalid("Invalid shelf type")),
            };
            for item in shelf["items"]
                .as_array()
                .ok_or_else(|| invalid("Shelf.items must be an array"))?
            {
                if kind == "entry" {
                    if item["title"].as_str().is_none() {
                        return Err(invalid("Entry missing title"));
                    }
                    validate_link(&item["to"])?;
                } else {
                    entity(kind, item)?;
                }
            }
            if !shelf["more"].is_null() {
                validate_link(&shelf["more"])?;
            }
        }
        return Ok(());
    }
    if schema == "categories" {
        let groups = value["groups"]
            .as_array()
            .ok_or_else(|| invalid("Categories.groups must be an array"))?;
        for group in groups {
            if group["title"].as_str().is_none() {
                return Err(invalid("Category group missing title"));
            }
            validate_schema("list:category", &group["items"])?;
        }
        return Ok(());
    }
    if let Some(kind) = schema.strip_prefix("page:") {
        let items = value["items"]
            .as_array()
            .ok_or_else(|| invalid("Page.items is not an array"))?;
        for item in items {
            entity(kind, item)?;
        }
        if !value["next"].is_null() && !value["next"].is_string() {
            return Err(invalid("Cursor must serialize as a string"));
        }
    } else if let Some(kind) = schema.strip_prefix("detail:") {
        if !value["info"].is_null() {
            entity(kind, &value["info"])?;
        }
        validate_schema("page:track", &value["tracks"])?;
    } else if schema == "streams" {
        let streams: Vec<crate::Stream> = serde_json::from_value(value.clone())?;
        if streams.is_empty() || streams.iter().any(|s| url::Url::parse(&s.url).is_err()) {
            return Err(invalid("Missing playable URL"));
        }
    } else if schema == "lyrics" {
        let lyrics: Option<crate::Lyrics> = serde_json::from_value(value.clone())?;
        if lyrics
            .is_some_and(|l| !["lrc", "krc", "yrc", "ttml", "plain"].contains(&l.format.as_str()))
        {
            return Err(invalid("Unknown lyrics format"));
        }
    } else if schema == "candidates" {
        let items: Vec<crate::ScrapeCandidate> = serde_json::from_value(value.clone())?;
        if items.iter().any(|v| v.id.is_empty() || v.title.is_empty()) {
            return Err(invalid("Invalid scrape candidate"));
        }
    } else if let Some(kind) = schema.strip_prefix("list:") {
        for item in value
            .as_array()
            .ok_or_else(|| invalid("Expected entity list"))?
        {
            entity(kind, item)?;
        }
    } else {
        entity(schema, value)?;
    }
    Ok(())
}
fn validate_link(value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("Link must be an object"))?;
    if object.len() != 1
        || !object.iter().all(|(k, v)| {
            ["playlist", "album", "toplist", "radio", "category"].contains(&k.as_str())
                && v.as_str().is_some_and(|s| !s.is_empty())
        })
    {
        return Err(invalid("Invalid destination link"));
    }
    Ok(())
}
pub fn inferred_schema(path: &str) -> Option<String> {
    if let Some(kind) = path.strip_prefix("search.") {
        Some(format!("page:{kind}"))
    } else if [
        "album.get",
        "artist.get",
        "playlist.get",
        "toplist.get",
        "program.get",
    ]
    .contains(&path)
    {
        Some(format!("detail:{}", path.split('.').next()?))
    } else {
        Some(
            match path {
                "track.get" => "track",
                "track.streams" => "streams",
                "track.lyrics" | "scrape.lyrics" => "lyrics",
                "home" => "home",
                "categories.list" => "categories",
                "scrape.search" => "candidates",
                "artist.albums" => "page:album",
                "artist.tracks" | "me.likedTracks" => "page:track",
                "me.playlists" | "categories.playlists" => "page:playlist",
                "toplist.list" => "list:toplist",
                "radio.channels" => "list:radio",
                "radio.next" => "list:track",
                _ => return None,
            }
            .into(),
        )
    }
}
async fn check_case(source: &Source, descriptor: &Descriptor, case: &ContractCase) -> Result<()> {
    let cancel = Cancellation::default();
    let caller = cancel.clone();
    let timer = case.abort_after_ms.map(|ms| {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            caller.cancel();
        })
    });
    let start = Instant::now();
    let result = source
        .invoke(
            &case.path,
            case.args.clone(),
            case.context.clone(),
            Some(&cancel),
        )
        .await;
    if let Some(timer) = timer {
        let _ = timer.await;
    }
    if let Some(code) = &case.expected_code {
        if result.as_ref().err().map(|e| &e.code) != Some(code) {
            return Err(invalid(&format!("Expected {code}, received {result:?}")));
        }
        if case.abort_after_ms.is_some() && start.elapsed() > Duration::from_millis(350) {
            return Err(invalid("Abort did not finish promptly"));
        }
        return Ok(());
    }
    let mut value = result?;
    let schema = case
        .schema
        .clone()
        .or_else(|| inferred_schema(&case.path))
        .ok_or_else(|| invalid("Fixture must supply a schema for this capability"))?;
    validate_schema(&schema, &value)?;
    if let Some(kind) = schema
        .strip_prefix("page:")
        .or_else(|| schema.starts_with("detail:").then_some("track"))
    {
        let mut ids = BTreeSet::new();
        let mut cursors = BTreeSet::new();
        for page_index in 0..100 {
            let page = if schema.starts_with("detail:") {
                &value["tracks"]
            } else {
                &value
            };
            for item in page["items"].as_array().unwrap() {
                if let Some(id) = item["id"].as_str() {
                    if !ids.insert(id.to_string()) {
                        return Err(invalid("Duplicate entity across pages"));
                    }
                    let get = format!("{kind}.get");
                    if descriptor.capabilities.contains(&get) {
                        let detail = source
                            .invoke(&get, vec![json!(id)], json!({"limit":20}), None)
                            .await?;
                        let actual = if kind == "track" {
                            &detail["id"]
                        } else {
                            &detail["info"]["id"]
                        };
                        if actual != id {
                            return Err(invalid("Native id did not round-trip through get"));
                        }
                    }
                }
            }
            let Some(next) = page["next"].as_str() else {
                return Ok(());
            };
            if page_index == 99 || !cursors.insert(next.to_string()) {
                return Err(invalid("Pagination did not terminate"));
            }
            let cursor: String = serde_json::from_str(&serde_json::to_string(next)?)?;
            let (mut args, mut context) = (case.args.clone(), case.context.clone());
            if case.path.starts_with("search.") {
                args[0]["cursor"] = json!(cursor);
            } else {
                if !context.is_object() {
                    context = json!({});
                }
                context["cursor"] = json!(cursor);
            }
            value = source.invoke(&case.path, args, context, None).await?;
            validate_schema(&schema, &value)?;
        }
    }
    Ok(())
}
/// Run the package fixture in isolated real runtimes. Replay cannot reach the network.
pub async fn run_contract(package: &Path, mode: ContractMode) -> Result<ContractReport> {
    let file = package.join("fixtures/contract.json");
    let mut fixture: ContractFixture = serde_json::from_slice(&std::fs::read(&file)?)?;
    let server = if mode == ContractMode::Live {
        None
    } else {
        Some(ReplayServer::start(
            fixture.routes.clone(),
            mode == ContractMode::Record,
        )?)
    };
    let data = tempfile::tempdir()?;
    let host = Host::open_with_replay(data.path(), server.as_ref().map(|s| s.base.clone())).await?;
    let descriptor = host.install_directory(package, false).await?;
    let source = if descriptor.kind == "library" {
        host.connect(&descriptor.id, fixture.connection.clone(), None)
            .await?
    } else if let Some(login) = &fixture.login {
        let mut draft = host.begin_login(&descriptor.id).await?;
        if let Some(cookie) = login["cookie"].as_str() {
            draft.cookie(cookie, None).await?;
        } else {
            draft
                .password(serde_json::from_value(login["password"].clone())?, None)
                .await?;
        }
        draft.commit().await?
    } else {
        host.activate(&descriptor.id).await?
    };
    let mut cases = Vec::new();
    let content = descriptor
        .capabilities
        .iter()
        .filter(|p| !p.starts_with("auth."))
        .collect::<BTreeSet<_>>();
    let covered = fixture
        .cases
        .iter()
        .filter(|c| c.expected_code.is_none())
        .map(|c| &c.path)
        .collect::<BTreeSet<_>>();
    cases.push(CaseReport {
        name: "descriptor and capability coverage".into(),
        passed: content.is_subset(&covered),
        duration_ms: 0,
        error: (!content.is_subset(&covered)).then(|| {
            format!(
                "Missing fixture cases: {:?}",
                content.difference(&covered).collect::<Vec<_>>()
            )
        }),
    });
    for case in &fixture.cases {
        if let Some(server) = &server {
            server
                .status
                .store(case.status.unwrap_or(0), Ordering::Release);
        }
        let start = Instant::now();
        let result = check_case(&source, &descriptor, case).await;
        cases.push(CaseReport {
            name: case.name.clone(),
            passed: result.is_ok(),
            duration_ms: start.elapsed().as_millis() as u64,
            error: result.err().map(|e| e.to_string()),
        });
    }
    if let Some(server) = &server {
        let missing = server.unmatched.lock().unwrap();
        cases.push(CaseReport {
            name: "all HTTP requests matched offline fixtures".into(),
            passed: missing.is_empty(),
            duration_ms: 0,
            error: (!missing.is_empty()).then(|| format!("Unmatched paths: {missing:?}")),
        });
        if mode == ContractMode::Record {
            fixture.routes = server.recorded.lock().unwrap().clone();
            haiku_store::json_file::write(&package.join("fixtures/recorded.json"), &fixture)?;
        }
    }
    Ok(ContractReport {
        schema_version: 1,
        plugin_id: descriptor.id,
        passed: cases.iter().all(|c| c.passed),
        cases,
    })
}

#[cfg(test)]
mod regressions {
    use super::*;

    #[tokio::test]
    async fn feiniu_direct_fnid_relay_access_code_and_identity() {
        let fixture: ContractFixture = serde_json::from_str(include_str!(
            "../../../../plugins/feiniu/fixtures/contract.json"
        ))
        .unwrap();
        for (input, access, relay) in [
            ("https://direct.invalid/music", "", false),
            ("FixtureFNID", "", true),
            ("FixtureFNID", "安全码", true),
        ] {
            let mut routes = fixture.routes.clone();
            routes.push(serde_json::from_value(json!({"path":"/api/v1/fn/con","method":"POST","body":{"code":0,"data":{"fn":["relay.invalid:5666"]}}})).unwrap());
            let server = ReplayServer::start(routes, false).unwrap();
            let data = tempfile::tempdir().unwrap();
            let host = Host::open_with_replay(data.path(), Some(server.base.clone()))
                .await
                .unwrap();
            let source = host
                .connect(
                    "feiniu.music",
                    BTreeMap::from([
                        ("baseUrl".into(), input.into()),
                        ("username".into(), "fixture".into()),
                        ("password".into(), "fake-password".into()),
                        ("accessCode".into(), access.into()),
                    ]),
                    None,
                )
                .await
                .unwrap();
            source
                .invoke("track.streams", vec![json!("t1")], json!({}), None)
                .await
                .unwrap();
            let requests = server.requests.lock().unwrap().clone();
            let login = requests
                .iter()
                .find(|s| s.contains("/user/password-login"))
                .unwrap();
            assert_eq!(login.contains("cookie: mode=relay"), relay);
            assert_eq!(login.contains("x-access-source: app"), !access.is_empty());
            if !access.is_empty() {
                assert!(login.contains("x-access-code: 5a6J5YWo56CB"));
            }
            if relay {
                assert!(login.contains("https://relay.invalid/"));
                let resolver = requests
                    .iter()
                    .find(|s| s.contains("/api/v1/fn/con"))
                    .unwrap();
                assert!(resolver.contains("authx: nonce="));
                assert!(resolver.contains("&timestamp="));
                assert!(resolver.contains("&sign="));
                assert!(resolver.contains(r#""fnId":"FixtureFNID""#));
            }
            let descriptor = host.package("feiniu.music").unwrap().descriptor;
            // Reference manifest declares baseUrl as URL, even for FNID. The
            // original input must reach login while only identity is normalized.
            let values = BTreeMap::from([
                ("baseUrl".into(), input.into()),
                ("username".into(), "fixture".into()),
            ]);
            assert_eq!(
                host.records()[0].source_id,
                crate::source_id(&descriptor, &values)
            );
            assert!(server.unmatched.lock().unwrap().is_empty());
        }
    }
}
