//! Gist synchronization of the authoritative Loro document, fenced by credential/binding epochs.
use base64::{Engine, engine::general_purpose::STANDARD};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use haiku_store::{collections::Collections, json_file, settings::SettingsStore};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
use tokio::{sync::watch, time::Instant};

type Result<T> = std::result::Result<T, String>;
const PREFIX: &[u8] = b"HAIKU_SYNC_2\n";
const LIMIT: u64 = 64 * 1024 * 1024;

pub trait Secrets: Send + Sync {
    fn read(&self) -> Result<Option<String>>;
    fn write(&self, token: Option<&str>) -> Result<()>;
}
#[derive(Default)]
pub struct MemorySecrets(Mutex<Option<String>>);
impl Secrets for MemorySecrets {
    fn read(&self) -> Result<Option<String>> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn write(&self, token: Option<&str>) -> Result<()> {
        *self.0.lock().unwrap() = token.map(str::to_owned);
        Ok(())
    }
}
pub struct SystemSecrets;
#[cfg(target_os = "macos")]
impl Secrets for SystemSecrets {
    fn read(&self) -> Result<Option<String>> {
        match security_framework::passwords::get_generic_password("Haiku NSH", "github_pat") {
            Ok(bytes) => String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| "凭据格式异常".into()),
            Err(e) if e.code() == -25300 => Ok(None),
            Err(_) => Err("无法读取系统钥匙串".into()),
        }
    }
    fn write(&self, token: Option<&str>) -> Result<()> {
        use security_framework::passwords::{delete_generic_password, set_generic_password};
        let result = match token {
            Some(token) => set_generic_password("Haiku NSH", "github_pat", token.as_bytes()),
            None => delete_generic_password("Haiku NSH", "github_pat"),
        };
        match result {
            Ok(()) => Ok(()),
            Err(e) if token.is_none() && e.code() == -25300 => Ok(()),
            Err(_) => Err("无法写入系统钥匙串".into()),
        }
    }
}
#[cfg(not(target_os = "macos"))]
impl Secrets for SystemSecrets {
    fn read(&self) -> Result<Option<String>> {
        Ok(None)
    }
    fn write(&self, _: Option<&str>) -> Result<()> {
        Err("当前平台不支持系统凭据存储".into())
    }
}
#[derive(Clone)]
pub struct Options {
    pub github_base: String,
    pub debounce: Duration,
    pub poll: Duration,
    pub retry_base: Duration,
    pub retry_cap: Duration,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            github_base: "https://api.github.com".into(),
            debounce: Duration::from_secs(12),
            poll: Duration::from_secs(120),
            retry_base: Duration::from_secs(15),
            retry_cap: Duration::from_secs(300),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub provider: String,
    pub connected: bool,
    pub username: String,
    pub busy: bool,
    pub last_synced: u64,
    pub error: String,
    pub paused: bool,
    pub revision: Value,
    pub gist_id: String,
    pub retry_at: Option<u64>,
}
#[derive(Clone)]
struct Session {
    epoch: u64,
    base: String,
    token: String,
    gist: String,
}
struct State {
    session: Session,
    status: Status,
    closed: bool,
    cycle: u64,
    last_result: Result<()>,
    progress: BTreeMap<String, Value>,
}
pub struct Service {
    collections: Collections,
    settings: Arc<SettingsStore>,
    secrets: Arc<dyn Secrets>,
    root: PathBuf,
    options: Options,
    client: reqwest::Client,
    state: Mutex<State>,
    gate: tokio::sync::Mutex<()>,
    config: watch::Sender<u64>,
    events: watch::Sender<Status>,
    worker: Mutex<Option<tokio::task::JoinHandle<()>>>,
}
impl Service {
    /// Opening restores credentials/configuration without accessing the network.
    pub fn open(
        root: &Path,
        collections: Collections,
        settings: Arc<SettingsStore>,
        secrets: Arc<dyn Secrets>,
        options: Options,
    ) -> Result<Arc<Self>> {
        let base = reqwest::Url::parse(&options.github_base).map_err(|_| "无效 GitHub API 地址")?;
        if options.github_base != "https://api.github.com"
            && !(base.scheme() == "http"
                && base.host_str() == Some("127.0.0.1")
                && base.username().is_empty()
                && base.password().is_none()
                && base.query().is_none()
                && base.fragment().is_none())
        {
            return Err("自定义 GitHub API 仅用于 loopback 夹具".into());
        }
        let token = secrets.read()?.unwrap_or_default();
        let gist = settings.get().gist_id;
        let status = Status {
            provider: "gist".into(),
            connected: !token.is_empty(),
            username: String::new(),
            busy: false,
            last_synced: 0,
            error: String::new(),
            paused: false,
            revision: json!(0),
            gist_id: gist.clone(),
            retry_at: None,
        };
        let progress = json_file::read(&root.join("haiku_state/album-progress.json"))
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        Ok(Arc::new(Self {
            collections,
            settings,
            secrets,
            root: root.into(),
            options: options.clone(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent("Haiku-NSH")
                .build()
                .map_err(|_| "无法初始化 GitHub 客户端")?,
            state: Mutex::new(State {
                session: Session {
                    epoch: 0,
                    base: options.github_base,
                    token,
                    gist,
                },
                status: status.clone(),
                closed: false,
                cycle: 0,
                last_result: Ok(()),
                progress,
            }),
            gate: tokio::sync::Mutex::new(()),
            config: watch::channel(0).0,
            events: watch::channel(status).0,
            worker: Mutex::new(None),
        }))
    }
    pub fn snapshot(&self) -> Status {
        self.events.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<Status> {
        self.events.subscribe()
    }
    /// Replace the API only for a disconnected loopback fixture.
    pub fn set_fixture_base(&self, base: String) -> Result<()> {
        let url = reqwest::Url::parse(&base).map_err(|_| "无效夹具地址")?;
        if url.scheme() != "http"
            || url.host_str() != Some("127.0.0.1")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("夹具仅允许 loopback 地址".into());
        }
        let mut state = self.state.lock().unwrap();
        Self::check(&state, state.session.epoch)?;
        if !state.session.token.is_empty() {
            return Err("请先清除 PAT".into());
        }
        state.session.epoch += 1;
        state.session.base = base;
        self.config.send_modify(|v| *v += 1);
        Ok(())
    }
    fn publish(&self, state: &State) {
        self.events.send_replace(state.status.clone());
    }
    fn check(state: &State, epoch: u64) -> Result<()> {
        if state.closed || state.session.epoch != epoch {
            Err("同步连接已更改".into())
        } else {
            Ok(())
        }
    }
    fn begin(&self) -> Result<Session> {
        let mut state = self.state.lock().unwrap();
        Self::check(&state, state.session.epoch)?;
        state.session.epoch += 1;
        state.status.busy = true;
        state.status.error.clear();
        state.status.paused = false;
        state.status.revision = json!(0);
        state.status.last_synced = 0;
        state.status.retry_at = None;
        self.config.send_replace(state.session.epoch);
        self.publish(&state);
        Ok(state.session.clone())
    }
    async fn canceled(&self, epoch: u64) {
        let mut config = self.config.subscribe();
        loop {
            if Self::check(&self.state.lock().unwrap(), epoch).is_err() {
                return;
            }
            if config.changed().await.is_err() {
                return;
            }
            config.borrow_and_update();
        }
    }
    async fn raw(
        &self,
        session: &Session,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value> {
        let url = format!("{}{}", session.base.trim_end_matches('/'), path);
        let bytes = self.fetch(session, method, &url, body).await?;
        serde_json::from_slice(&bytes).map_err(|_| "GitHub 响应格式异常".into())
    }
    async fn fetch(
        &self,
        session: &Session,
        method: &str,
        url: &str,
        body: Option<Value>,
    ) -> Result<Vec<u8>> {
        let request = async {
            Self::check(&self.state.lock().unwrap(), session.epoch)?;
            let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|_| "无效请求")?;
            let mut request = self
                .client
                .request(method, url)
                .bearer_auth(&session.token)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28");
            if let Some(body) = body {
                request = request
                    .header("Content-Type", "application/json")
                    .body(body.to_string());
            }
            let mut response = request
                .send()
                .await
                .map_err(|_| "无法连接 GitHub，请检查网络")?;
            match response.status().as_u16() {
                200..=299 => {}
                401 => return Err("GitHub token 无效或过期 (401)".into()),
                403 => return Err("GitHub 拒绝访问，请检查 Gist 权限或请求限额 (403)".into()),
                404 => return Err("Gist 不存在或无权访问 (404)".into()),
                status => return Err(format!("GitHub HTTP {status}")),
            }
            let mut bytes = vec![];
            while let Some(chunk) = response.chunk().await.map_err(|_| "GitHub 连接中断")? {
                if bytes.len() as u64 + chunk.len() as u64 > LIMIT {
                    return Err("同步数据超过安全上限".into());
                }
                bytes.extend_from_slice(&chunk);
            }
            Self::check(&self.state.lock().unwrap(), session.epoch)?;
            Ok(bytes)
        };
        tokio::select! { result = request => result, _ = self.canceled(session.epoch) => Err("同步连接已更改".into()) }
    }
    fn finish_error(&self, epoch: u64, result: &Result<()>) {
        let mut state = self.state.lock().unwrap();
        if Self::check(&state, epoch).is_err() {
            return;
        }
        state.status.busy = false;
        if let Err(error) = result {
            state.status.error = error.clone();
            state.status.paused = ["(401)", "(403)", "(404)"]
                .iter()
                .any(|code| error.contains(code));
        }
        self.publish(&state);
    }
    /// Validate then store a PAT; failed validation preserves the previous credential.
    pub async fn set_token(&self, token: String) -> Result<()> {
        let token = token.trim().to_owned();
        if token.is_empty() {
            return Err("请输入 GitHub PAT".into());
        }
        let mut session = self.begin()?;
        session.token = token.clone();
        let _gate = self.gate.lock().await;
        let result = self.raw(&session, "GET", "/user", None).await;
        let result = result.and_then(|user| {
            let mut state = self.state.lock().unwrap();
            Self::check(&state, session.epoch)?;
            let username = user["login"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("GitHub 用户信息异常")?;
            self.secrets.write(Some(&token))?;
            state.session.token = token;
            state.status.connected = true;
            state.status.username = username.into();
            self.publish(&state);
            Ok(())
        });
        self.finish_error(session.epoch, &result);
        self.config.send_modify(|v| *v += 1);
        result
    }
    pub fn clear_token(&self) -> Result<()> {
        self.detach(false)
    }
    pub fn disconnect(&self) -> Result<()> {
        self.detach(true)
    }
    fn detach(&self, unbind: bool) -> Result<()> {
        let session = self.begin()?;
        let mut state = self.state.lock().unwrap();
        Self::check(&state, session.epoch)?;
        state.session.token.clear();
        state.status.connected = false;
        state.status.username.clear();
        state.status.busy = false;
        let credential_result = self.secrets.write(None);
        let settings_result = if unbind {
            self.settings
                .update(json!({"syncProvider":"gist","gistId":""}))
                .map(|_| {
                    state.session.gist.clear();
                    state.status.gist_id.clear();
                })
        } else {
            Ok(())
        };
        self.publish(&state);
        credential_result.and(settings_result)
    }
    fn files(&self) -> Result<Value> {
        let progress = self.state.lock().unwrap().progress.clone();
        let bytes = encode(&self.collections.export_snapshot(), &progress)?;
        Ok(json!({
            "playlists.loro.b64":{"content":STANDARD.encode(bytes)},
            "sync-meta.json":{"content":json!({"app":"haiku","schemaVersion":1,
                "updatedAt":chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)}).to_string()},
        }))
    }
    fn bind_result(&self, session: &Session, id: String, revision: Value) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        Self::check(&state, session.epoch)?;
        self.settings
            .update(json!({"syncProvider":"gist","gistId":id}))?;
        state.session.gist = id.clone();
        state.status.gist_id = id;
        state.status.revision = revision;
        state.status.busy = false;
        self.publish(&state);
        Ok(())
    }
    pub async fn create_gist(&self) -> Result<String> {
        let session = self.begin()?;
        let result =
            async {
                let _gate = self.gate.lock().await;
                if session.token.is_empty() {
                    return Err("请先保存 GitHub PAT".into());
                }
                let data = self.raw(&session, "POST", "/gists", Some(json!({
                "description":"Haiku playlists sync","public":false,"files":self.files()?,
            }))).await?;
                let id = gist_id(data["id"].as_str().ok_or("GitHub 未返回 Gist ID")?)?;
                self.bind_result(&session, id.clone(), data["updated_at"].clone())?;
                Ok(id)
            }
            .await;
        self.finish_error(
            session.epoch,
            &result.as_ref().map(|_| ()).map_err(Clone::clone),
        );
        result
    }
    pub async fn bind_gist(&self, input: String) -> Result<()> {
        let id = gist_id(&input)?;
        let mut session = self.begin()?;
        session.gist = id.clone();
        let result = async {
            let _gate = self.gate.lock().await;
            if session.token.is_empty() {
                return Err("请先保存 GitHub PAT".into());
            }
            self.raw(&session, "GET", &format!("/gists/{id}"), None)
                .await?;
            self.bind_result(&session, id, json!(0))
        }
        .await;
        self.finish_error(session.epoch, &result);
        result?;
        self.sync_now(false).await
    }
    /// Concurrent callers share the result of the same cycle, including failures.
    pub async fn sync_now(&self, pull_only: bool) -> Result<()> {
        let (session, cycle) = {
            let state = self.state.lock().unwrap();
            (state.session.clone(), state.cycle)
        };
        let _gate = self.gate.lock().await;
        {
            let mut state = self.state.lock().unwrap();
            Self::check(&state, session.epoch)?;
            if cycle != state.cycle {
                return state.last_result.clone();
            }
            if state.status.busy {
                return Err("同步连接正在更新".into());
            }
            state.status.busy = true;
            state.status.error.clear();
            self.publish(&state);
        }
        let result = self.cycle(&session, pull_only).await;
        {
            let mut state = self.state.lock().unwrap();
            if Self::check(&state, session.epoch).is_ok() {
                state.cycle += 1;
                state.last_result = result.clone();
                if result.is_ok() {
                    state.status.last_synced = crate::account::now();
                    state.status.paused = false;
                    state.status.retry_at = None;
                }
            }
        }
        self.finish_error(session.epoch, &result);
        result
    }
    async fn cycle(&self, session: &Session, pull_only: bool) -> Result<()> {
        if session.token.is_empty() || session.gist.is_empty() {
            return Err("请先保存 PAT 并创建或绑定 Gist".into());
        }
        let path = format!("/gists/{}", session.gist);
        let remote = self.raw(session, "GET", &path, None).await?;
        let revision = self.snapshot().revision;
        if remote["updated_at"] != revision {
            if let Some(file) = remote["files"].get("playlists.loro.b64") {
                let text = if file["truncated"] == true {
                    let url = file["raw_url"].as_str().ok_or("Gist 缺少原始文件地址")?;
                    self.validate_raw(session, url)?;
                    String::from_utf8(self.fetch(session, "GET", url, None).await?)
                        .map_err(|_| "Gist 文件编码异常")?
                } else {
                    file["content"].as_str().ok_or("Gist 文件内容异常")?.into()
                };
                let packed = STANDARD
                    .decode(
                        text.chars()
                            .filter(|c| !c.is_whitespace())
                            .collect::<String>(),
                    )
                    .map_err(|_| "同步快照 Base64 损坏")?;
                let (bytes, progress) = decode(&packed)?;
                let mut state = self.state.lock().unwrap();
                Self::check(&state, session.epoch)?;
                self.collections.import(&bytes)?;
                merge_progress(&mut state.progress, progress);
                json_file::write(
                    &self.root.join("haiku_state/album-progress.json"),
                    &state.progress,
                )
                .map_err(|e| e.to_string())?;
            }
            let mut state = self.state.lock().unwrap();
            Self::check(&state, session.epoch)?;
            state.status.revision = remote["updated_at"].clone();
            self.publish(&state);
        }
        if !pull_only {
            let result = self
                .raw(
                    session,
                    "PATCH",
                    &path,
                    Some(json!({"files":self.files()?})),
                )
                .await?;
            let mut state = self.state.lock().unwrap();
            Self::check(&state, session.epoch)?;
            state.status.revision = result["updated_at"].clone();
            self.publish(&state);
        }
        Ok(())
    }
    fn validate_raw(&self, session: &Session, raw: &str) -> Result<()> {
        let url = reqwest::Url::parse(raw).map_err(|_| "无效 Gist 原始文件 URL")?;
        let base = reqwest::Url::parse(&session.base).unwrap();
        let trusted = if base.host_str() == Some("127.0.0.1") {
            url.origin() == base.origin()
        } else {
            url.scheme() == "https"
                && matches!(
                    url.host_str(),
                    Some("gist.githubusercontent.com" | "gist.github.com")
                )
                && url.port_or_known_default() == Some(443)
        };
        if !trusted || !url.username().is_empty() || url.password().is_some() {
            return Err("不安全的 Gist 原始文件 URL".into());
        }
        Ok(())
    }
    pub fn start(self: &Arc<Self>) {
        let mut worker = self.worker.lock().unwrap();
        if worker.is_some() {
            return;
        }
        *worker = Some(crate::downloader::runtime().spawn(run(Arc::downgrade(self))));
    }
    pub fn stop(&self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        state.session.epoch += 1;
        state.status.busy = false;
        self.config.send_modify(|v| *v += 1);
        self.publish(&state);
    }
    /// Cancel requests, join the scheduler, and drain any manual operation before flushing.
    pub async fn shutdown(&self) -> Result<()> {
        self.stop();
        let worker = self.worker.lock().unwrap().take();
        if let Some(worker) = worker {
            let _ = worker.await;
        }
        let _gate = self.gate.lock().await;
        self.collections.flush()
    }
    /// Rebuild restored credentials/binding after shutdown, keeping status subscriptions alive.
    pub fn resume_after_restore(self: &Arc<Self>) -> Result<()> {
        let token = self.secrets.read()?.unwrap_or_default();
        let gist = self.settings.get().gist_id;
        let progress = json_file::read(&self.root.join("haiku_state/album-progress.json"))
            .map_err(|e| e.to_string())?.unwrap_or_default();
        {
            let mut state = self.state.lock().unwrap();
            state.closed = false;
            state.session.epoch += 1;
            state.session.token = token.clone();
            state.session.gist = gist.clone();
            state.progress = progress;
            state.status = Status {
                provider: "gist".into(), connected: !token.is_empty(), username: String::new(),
                busy: false, last_synced: 0, error: String::new(), paused: false,
                revision: json!(0), gist_id: gist, retry_at: None,
            };
            self.config.send_modify(|v| *v += 1);
            self.publish(&state);
        }
        self.start();
        Ok(())
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn run(weak: Weak<Service>) {
    let Some(service) = weak.upgrade() else {
        return;
    };
    let mut local = service.collections.subscribe_local();
    let mut config = service.config.subscribe();
    let mut status_events = service.subscribe();
    let mut pending = false;
    let mut pull = Instant::now() + service.options.poll;
    let mut push = pull;
    let mut retry = Instant::now();
    let mut failures: u32 = 0;
    drop(service);
    loop {
        let Some(service) = weak.upgrade() else {
            return;
        };
        let snapshot = service.snapshot();
        if service.state.lock().unwrap().closed {
            return;
        }
        let enabled = snapshot.connected
            && !snapshot.gist_id.is_empty()
            && !snapshot.busy
            && !snapshot.paused;
        let deadline = if pending { pull.min(push) } else { pull }.max(retry);
        tokio::select! {
            _ = config.changed() => {
                config.borrow_and_update();
                let snapshot = service.snapshot();
                pending = snapshot.connected && !snapshot.gist_id.is_empty();
                failures = 0;
                retry = Instant::now(); pull = retry + service.options.poll; push = pull;
                if pending { push = retry + service.options.debounce; }
                continue;
            },
            _ = status_events.changed() => { status_events.borrow_and_update(); continue; },
            result = local.changed() => {
                if result.is_err() { return; }
                local.borrow_and_update(); pending = true;
                push = Instant::now() + service.options.debounce;
                continue;
            },
            _ = tokio::time::sleep_until(deadline), if enabled => {},
        }
        let observed = *local.borrow_and_update();
        let result = service.sync_now(!pending).await;
        if result.is_ok() {
            pending = *local.borrow() != observed;
            failures = 0;
            retry = Instant::now();
            pull = retry + service.options.poll;
            push = retry + service.options.debounce;
        } else {
            failures += 1;
            let delay = service
                .options
                .retry_base
                .saturating_mul(1u32 << failures.saturating_sub(1).min(20))
                .min(service.options.retry_cap);
            retry = Instant::now() + delay;
            // A failed push must remain a push when its retry runs.
            let mut state = service.state.lock().unwrap();
            state.status.retry_at =
                (!state.status.paused).then(|| crate::account::now() + delay.as_millis() as u64);
            drop(state);
            let state = service.state.lock().unwrap();
            service.publish(&state);
        }
    }
}
pub fn gist_id(input: &str) -> Result<String> {
    let input = input.trim();
    let id = if input.starts_with("https://") {
        let url = reqwest::Url::parse(input).map_err(|_| "无效 Gist URL")?;
        if url.host_str() != Some("gist.github.com")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
        {
            return Err("请输入 gist.github.com 地址或 Gist ID".into());
        }
        url.path_segments()
            .and_then(|segments| segments.filter(|s| !s.is_empty()).next_back())
            .ok_or("无效 Gist URL")?
            .to_owned()
    } else {
        input.to_owned()
    };
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("无效 Gist ID".into());
    }
    Ok(id)
}
pub fn encode(playlists: &[u8], progress: &BTreeMap<String, Value>) -> Result<Vec<u8>> {
    let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
    gzip.write_all(PREFIX).map_err(|e| e.to_string())?;
    serde_json::to_writer(
        &mut gzip,
        &json!({"version":2,"playlists":STANDARD.encode(playlists),"albumProgress":progress}),
    )
    .map_err(|e| e.to_string())?;
    gzip.finish().map_err(|e| e.to_string())
}
pub fn decode(bytes: &[u8]) -> Result<(Vec<u8>, BTreeMap<String, Value>)> {
    let bytes = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut decoded = vec![];
        GzDecoder::new(bytes)
            .take(LIMIT + 1)
            .read_to_end(&mut decoded)
            .map_err(|_| "同步快照 gzip 损坏")?;
        if decoded.len() as u64 > LIMIT {
            return Err("同步快照超过安全上限".into());
        }
        decoded
    } else {
        bytes.to_vec()
    };
    let Some(payload) = bytes.strip_prefix(PREFIX) else {
        return Ok((bytes, BTreeMap::new()));
    };
    let data: Value = serde_json::from_slice(payload).map_err(|_| "无效的同步快照")?;
    if data["version"] != 2 {
        return Err("不支持的同步快照版本".into());
    }
    let playlists = STANDARD
        .decode(
            data["playlists"]
                .as_str()
                .ok_or("同步快照缺少歌单")?
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>(),
        )
        .map_err(|_| "同步快照 Base64 损坏")?;
    let progress =
        serde_json::from_value(data["albumProgress"].clone()).map_err(|_| "同步进度格式异常")?;
    Ok((playlists, progress))
}
fn merge_progress(local: &mut BTreeMap<String, Value>, incoming: BTreeMap<String, Value>) {
    for (key, value) in incoming {
        if value["trackId"].as_str().is_some_and(|s| !s.is_empty())
            && value["index"].as_i64().is_some_and(|i| i >= 0)
            && value["positionSec"]
                .as_f64()
                .is_some_and(|p| p >= 0. && p.is_finite())
            && value["updatedAt"].as_u64().is_some_and(|time| {
                local
                    .get(&key)
                    .and_then(|v| v["updatedAt"].as_u64())
                    .is_none_or(|old| time > old)
            })
        {
            local.insert(key, value);
        }
    }
    let mut ordered = local
        .iter()
        .map(|(key, value)| (key.clone(), value["updatedAt"].as_u64().unwrap_or(0)))
        .collect::<Vec<_>>();
    ordered.sort_by_key(|(_, time)| std::cmp::Reverse(*time));
    for (key, _) in ordered.into_iter().skip(500) {
        local.remove(&key);
    }
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
