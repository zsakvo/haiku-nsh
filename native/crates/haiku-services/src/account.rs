//! Haiku account authority. Tokens never enter snapshots or ordinary JSON files.
use haiku_store::json_file;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;

pub const DEFAULT_SERVER: &str = "https://api.haiku.zhendong.ltd";
type Result<T> = std::result::Result<T, AccountError>;

#[derive(Clone)]
pub struct AccountError {
    pub code: i64,
    pub message: String,
    pub transport: bool,
}
impl fmt::Debug for AccountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl fmt::Display for AccountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}
impl std::error::Error for AccountError {}
fn error(code: i64, message: impl Into<String>) -> AccountError {
    AccountError {
        code,
        message: message.into(),
        transport: false,
    }
}
fn io_error(e: impl fmt::Display) -> AccountError {
    error(-1, format!("账号存储失败：{e}"))
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Token {
    pub access_token: String,
    pub refresh_token: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
pub trait Credentials: Send + Sync {
    fn read(&self) -> Result<Option<Token>>;
    fn write(&self, token: Option<&Token>) -> Result<()>;
}
#[derive(Default)]
pub struct MemoryCredentials(Mutex<Option<Token>>);
impl Credentials for MemoryCredentials {
    fn read(&self) -> Result<Option<Token>> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn write(&self, token: Option<&Token>) -> Result<()> {
        *self.0.lock().unwrap() = token.cloned();
        Ok(())
    }
}
pub struct SystemCredentials;
#[cfg(target_os = "macos")]
impl Credentials for SystemCredentials {
    fn read(&self) -> Result<Option<Token>> {
        match security_framework::passwords::get_generic_password("Haiku NSH", "account-token") {
            Ok(bytes) => serde_json::from_slice::<Token>(&bytes)
                .map(Some)
                .map_err(io_error),
            Err(e) if e.code() == -25300 => Ok(None),
            Err(_) => Err(error(-1, "无法读取系统钥匙串")),
        }
    }
    fn write(&self, token: Option<&Token>) -> Result<()> {
        use security_framework::passwords::{delete_generic_password, set_generic_password};
        let result = if let Some(token) = token {
            set_generic_password(
                "Haiku NSH",
                "account-token",
                &serde_json::to_vec(token).map_err(io_error)?,
            )
        } else {
            delete_generic_password("Haiku NSH", "account-token")
        };
        match result {
            Ok(()) => Ok(()),
            Err(e) if token.is_none() && e.code() == -25300 => Ok(()),
            Err(_) => Err(error(-1, "无法写入系统钥匙串")),
        }
    }
}
#[cfg(not(target_os = "macos"))]
impl Credentials for SystemCredentials {
    fn read(&self) -> Result<Option<Token>> {
        Ok(None)
    }
    fn write(&self, _: Option<&Token>) -> Result<()> {
        Err(error(-1, "当前平台不支持系统凭据存储"))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Cache {
    user: Value,
    subscription: Value,
    updated_at: u64,
}
impl Default for Cache {
    fn default() -> Self {
        Self {
            user: Value::Null,
            subscription: json!({"active":false}),
            updated_at: 0,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    #[serde(flatten)]
    cache: Cache,
    pub signed_in: bool,
    pub base_url: String,
    pub code_until: BTreeMap<String, u64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub epoch: u64,
    pub account_id: String,
}
#[derive(Clone)]
struct Session {
    epoch: u64,
    base: String,
    token: Option<Token>,
    refresh_revision: u64,
}
struct State {
    session: Session,
    cache: Cache,
    cooldown: BTreeMap<String, u64>,
    sending: BTreeSet<String>,
    last_refresh: Result<()>,
}
pub struct Service {
    root: PathBuf,
    credentials: Arc<dyn Credentials>,
    state: Mutex<State>,
    refresh_lock: tokio::sync::Mutex<()>,
    events: watch::Sender<Snapshot>,
    client: reqwest::Client,
}
impl Service {
    pub fn open(root: &Path, credentials: Arc<dyn Credentials>) -> Result<Arc<Self>> {
        let config: Value = json_file::read(&root.join("haiku_state/api_config.json"))
            .map_err(io_error)?
            .unwrap_or(json!({}));
        let base = config["baseUrl"]
            .as_str()
            .unwrap_or(DEFAULT_SERVER)
            .to_owned();
        validate_server(&base)?;
        let token = credentials
            .read()?
            .filter(|t| !t.access_token.is_empty() && !t.refresh_token.is_empty());
        let cache = if token.is_some() {
            json_file::read(&root.join("haiku_state/account_cache.json"))
                .map_err(io_error)?
                .unwrap_or_default()
        } else {
            Cache::default()
        };
        let snapshot = Snapshot {
            signed_in: token.is_some(),
            base_url: base.clone(),
            cache: cache.clone(),
            code_until: BTreeMap::new(),
        };
        Ok(Arc::new(Self {
            root: root.into(),
            credentials,
            state: Mutex::new(State {
                session: Session {
                    epoch: 0,
                    base,
                    token,
                    refresh_revision: 0,
                },
                cache,
                cooldown: BTreeMap::new(),
                sending: BTreeSet::new(),
                last_refresh: Ok(()),
            }),
            refresh_lock: tokio::sync::Mutex::new(()),
            events: watch::channel(snapshot).0,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(io_error)?,
        }))
    }
    fn publish(&self, state: &State) -> Result<()> {
        json_file::write(
            &self.root.join("haiku_state/account_cache.json"),
            &state.cache,
        )
        .map_err(io_error)?;
        self.events.send_replace(Snapshot {
            cache: state.cache.clone(),
            signed_in: state.session.token.is_some(),
            base_url: state.session.base.clone(),
            code_until: state.cooldown.clone(),
        });
        Ok(())
    }
    pub fn snapshot(&self) -> Snapshot {
        self.events.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.events.subscribe()
    }
    pub fn identity(&self) -> Option<Identity> {
        let state = self.state.lock().unwrap();
        state.session.token.as_ref()?;
        let id = &state.cache.user["id"];
        let account_id = id
            .as_str()
            .map(str::to_owned)
            .or_else(|| id.as_i64().map(|id| id.to_string()))?;
        Some(Identity {
            epoch: state.session.epoch,
            account_id,
        })
    }
    pub fn is_current(&self, identity: &Identity) -> bool {
        self.identity()
            .is_some_and(|i| i.epoch == identity.epoch && i.account_id == identity.account_id)
    }
    /// Commit local sync metadata while excluding logout and account changes.
    pub fn with_current<T>(&self, identity: &Identity, commit: impl FnOnce() -> T) -> Option<T> {
        let state = self.state.lock().unwrap();
        (state.session.epoch == identity.epoch && state.session.token.is_some()).then(commit)
    }
    async fn raw(
        &self,
        session: &Session,
        method: &str,
        endpoint: &str,
        body: Option<&Value>,
    ) -> Result<Value> {
        let base = session.base.trim_end_matches('/');
        let prefix = if base.ends_with("/api/v1") {
            ""
        } else {
            "/api/v1"
        };
        let url = format!("{base}{prefix}{endpoint}");
        let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(io_error)?;
        let mut request = self
            .client
            .request(method, url)
            .header("Content-Type", "application/json");
        if let Some(token) = &session.token {
            request = request.bearer_auth(&token.access_token);
        }
        if let Some(body) = body {
            request = request.body(serde_json::to_vec(body).map_err(io_error)?);
        }
        let response = request.send().await.map_err(|_| AccountError {
            code: -1,
            message: "无法连接账号服务器，请检查网络连接后重试".into(),
            transport: true,
        })?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(|_| AccountError {
            code: -1,
            message: "账号服务器连接中断".into(),
            transport: true,
        })?;
        if status.as_u16() == 401 {
            return Err(error(10002, "登录已失效"));
        }
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| error(-1, "账号服务器响应异常"))?;
        let code = value["code"].as_i64().unwrap_or(-1);
        if !status.is_success() || code != 0 {
            return Err(error(
                code,
                value["message"].as_str().unwrap_or("账号请求失败"),
            ));
        }
        Ok(value["data"].clone())
    }
    /// Authenticated requests retry at most once; concurrent expiry shares one refresh result.
    pub async fn request(
        &self,
        method: &str,
        endpoint: &str,
        body: Option<Value>,
    ) -> Result<Value> {
        let sent = self.state.lock().unwrap().session.clone();
        self.request_session(sent, method, endpoint, body).await
    }
    /// Bind a request to a captured login, even if the account changes before it is sent.
    pub async fn request_for(
        &self,
        identity: &Identity,
        method: &str,
        endpoint: &str,
        body: Option<Value>,
    ) -> Result<Value> {
        let sent = {
            let state = self.state.lock().unwrap();
            if state.session.epoch != identity.epoch || state.session.token.is_none() {
                return Err(error(-2, "账号会话已更改"));
            }
            state.session.clone()
        };
        self.request_session(sent, method, endpoint, body).await
    }
    async fn request_session(
        &self,
        sent: Session,
        method: &str,
        endpoint: &str,
        body: Option<Value>,
    ) -> Result<Value> {
        let mut result = self.raw(&sent, method, endpoint, body.as_ref()).await;
        if self.state.lock().unwrap().session.epoch != sent.epoch {
            return Err(error(-2, "账号会话已更改"));
        }
        if result.as_ref().is_err_and(|e| e.code == 10002) && sent.token.is_some() {
            self.refresh_token(&sent).await?;
            let renewed = self.state.lock().unwrap().session.clone();
            result = self.raw(&renewed, method, endpoint, body.as_ref()).await;
            let mut state = self.state.lock().unwrap();
            if state.session.epoch != renewed.epoch {
                return Err(error(-2, "账号会话已更改"));
            }
            if result.as_ref().is_err_and(|e| e.code == 10002) {
                self.clear(&mut state)?;
            }
        }
        result
    }
    async fn refresh_token(&self, expected: &Session) -> Result<()> {
        let _flight = self.refresh_lock.lock().await;
        {
            let state = self.state.lock().unwrap();
            if state.session.epoch != expected.epoch {
                return Err(error(-2, "账号会话已更改"));
            }
            if state.session.refresh_revision != expected.refresh_revision {
                return state.last_refresh.clone();
            }
        }
        let old = expected
            .token
            .as_ref()
            .ok_or_else(|| error(10002, "登录已失效"))?;
        let mut unauthenticated = expected.clone();
        unauthenticated.token = None;
        let result = self
            .raw(
                &unauthenticated,
                "POST",
                "/auth/refresh",
                Some(&json!({"refresh_token":old.refresh_token})),
            )
            .await;
        let mut state = self.state.lock().unwrap();
        if state.session.epoch != expected.epoch {
            return Err(error(-2, "账号会话已更改"));
        }
        let result = result.and_then(|data| {
            let mut raw = data.get("token").cloned().unwrap_or(data);
            if raw["refresh_token"].as_str().is_none_or(str::is_empty) {
                raw["refresh_token"] = json!(old.refresh_token);
            }
            let token: Token =
                serde_json::from_value(raw).map_err(|_| error(-1, "刷新响应异常"))?;
            if token.access_token.is_empty() {
                return Err(error(-1, "刷新响应异常"));
            }
            self.credentials.write(Some(&token))?;
            state.session.token = Some(token);
            Ok(())
        });
        if result.as_ref().is_err_and(|e| !e.transport) {
            self.clear(&mut state)?;
        }
        state.session.refresh_revision += 1;
        state.last_refresh = result.clone();
        result
    }
    fn clear(&self, state: &mut State) -> Result<()> {
        state.session.epoch += 1;
        state.session.token = None;
        state.cache = Cache::default();
        let saved = self.credentials.write(None);
        self.publish(state)?;
        saved
    }
    pub async fn send_code(&self, email: String) -> Result<()> {
        let email = email.trim();
        let key = email.to_lowercase();
        let valid = email
            .split_once('@')
            .is_some_and(|(a, b)| !a.is_empty() && b.contains('.') && !b.starts_with('.'));
        if !valid || email.chars().any(char::is_whitespace) {
            return Err(error(-1, "请输入有效邮箱"));
        }
        let mut session = {
            let mut state = self.state.lock().unwrap();
            if state.cooldown.get(&key).is_some_and(|until| *until > now())
                || !state.sending.insert(key.clone())
            {
                return Err(error(10006, "发送太频繁，请稍后再试"));
            }
            state.session.clone()
        };
        session.token = None;
        let result = self
            .raw(
                &session,
                "POST",
                "/auth/code",
                Some(&json!({"email":email})),
            )
            .await;
        let mut state = self.state.lock().unwrap();
        state.sending.remove(&key);
        if state.session.epoch != session.epoch {
            return Err(error(-2, "账号会话已更改"));
        }
        if result.is_ok() || result.as_ref().is_err_and(|e| e.code == 10006) {
            state.cooldown.insert(key, now() + 60_000);
            self.publish(&state)?;
        }
        result.map(|_| ())
    }
    pub async fn login(&self, email: String, code: String) -> Result<Snapshot> {
        if email.trim().is_empty() || code.trim().is_empty() {
            return Err(error(-1, "请输入邮箱和验证码"));
        }
        let mut session = {
            let mut state = self.state.lock().unwrap();
            self.clear(&mut state)?;
            state.session.clone()
        };
        session.token = None;
        let data = self
            .raw(
                &session,
                "POST",
                "/auth/login",
                Some(&json!({"email":email.trim(),"code":code.trim()})),
            )
            .await?;
        {
            let mut state = self.state.lock().unwrap();
            if state.session.epoch != session.epoch {
                return Err(error(-2, "账号会话已更改"));
            }
            let token: Token = serde_json::from_value(data["token"].clone())
                .map_err(|_| error(-1, "登录响应异常"))?;
            if token.access_token.is_empty()
                || token.refresh_token.is_empty()
                || !data["user"].is_object()
            {
                return Err(error(-1, "登录响应异常"));
            }
            self.credentials.write(Some(&token))?;
            state.session.token = Some(token);
            state.cache.user = data["user"].clone();
            self.publish(&state)?;
        }
        // A subscription transport failure must not turn a successful login into logout.
        let _ = self.subscription(true).await;
        Ok(self.snapshot())
    }
    pub async fn logout(&self) -> Result<()> {
        let previous = {
            let mut state = self.state.lock().unwrap();
            let previous = state.session.clone();
            self.clear(&mut state)?;
            previous
        };
        if let Some(token) = &previous.token {
            let _ = self
                .raw(
                    &previous,
                    "POST",
                    "/auth/logout",
                    Some(&json!({"refresh_token":token.refresh_token})),
                )
                .await;
        }
        Ok(())
    }
    pub async fn refresh(&self) -> Result<Snapshot> {
        let epoch = self.state.lock().unwrap().session.epoch;
        let user = self.request("GET", "/me", None).await?;
        {
            let mut state = self.state.lock().unwrap();
            if state.session.epoch != epoch {
                return Err(error(-2, "账号会话已更改"));
            }
            state.cache.user = user;
            self.publish(&state)?;
        }
        self.subscription(true).await?;
        Ok(self.snapshot())
    }
    pub async fn subscription(&self, force: bool) -> Result<Value> {
        let epoch = {
            let state = self.state.lock().unwrap();
            if state.session.token.is_none()
                || (!force && now().saturating_sub(state.cache.updated_at) < 300_000)
            {
                return Ok(state.cache.subscription.clone());
            }
            state.session.epoch
        };
        let subscription = self.request("GET", "/me/subscription", None).await?;
        let mut state = self.state.lock().unwrap();
        if state.session.epoch != epoch {
            return Err(error(-2, "账号会话已更改"));
        }
        state.cache.subscription = subscription;
        state.cache.updated_at = now();
        self.publish(&state)?;
        Ok(state.cache.subscription.clone())
    }
    pub async fn plans(&self) -> Result<Value> {
        let data = self.request("GET", "/plans", None).await?;
        let mut plans = data.as_array().cloned().unwrap_or_default();
        plans.retain(|p| p.is_object() && p["active"] != false);
        plans.sort_by_key(|p| p["sort"].as_i64().unwrap_or(0));
        Ok(json!(plans))
    }
    pub fn set_server(&self, base: String) -> Result<()> {
        let base = validate_server(&base)?;
        let mut state = self.state.lock().unwrap();
        self.clear(&mut state)?;
        json_file::write(
            &self.root.join("haiku_state/api_config.json"),
            &json!({"baseUrl":base}),
        )
        .map_err(io_error)?;
        state.session.base = base;
        state.cooldown.clear();
        self.publish(&state)
    }
}
fn validate_server(base: &str) -> Result<String> {
    let url = reqwest::Url::parse(base).map_err(|_| error(-1, "无效服务地址"))?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(error(-1, "无效服务地址"));
    }
    Ok(base.trim_end_matches('/').into())
}

#[cfg(test)]
#[path = "account_tests.rs"]
pub(crate) mod tests;
