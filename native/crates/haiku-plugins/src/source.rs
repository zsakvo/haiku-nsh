use crate::{Descriptor, Host, PluginError, Result, SourceRecord, descriptor, sdk};
use haiku_jsrt::{AbortHandle, Runtime};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

#[derive(Clone)]
pub struct Cancellation(tokio::sync::watch::Sender<bool>);
impl Default for Cancellation {
    fn default() -> Self {
        Self(tokio::sync::watch::channel(false).0)
    }
}
impl Cancellation {
    pub fn cancel(&self) {
        self.0.send_replace(true);
    }
    pub fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }
    pub(crate) async fn cancelled(&self) {
        let mut receiver = self.0.subscribe();
        if *receiver.borrow_and_update() {
            return;
        }
        let _ = receiver.wait_for(|v| *v).await;
    }
}
#[derive(Clone, Serialize, Deserialize, Default)]
struct Credentials {
    #[serde(default)]
    credential: Value,
    #[serde(default)]
    connection: Option<BTreeMap<String, String>>,
}
pub struct Source {
    pub record: SourceRecord,
    pub(crate) descriptor: Mutex<Descriptor>,
    pub(crate) runtime: tokio::sync::RwLock<Runtime>,
    directory: PathBuf,
    credentials: Mutex<Credentials>,
    generation: AtomicU64,
    refresh: tokio::sync::Mutex<()>,
    needs_login: Mutex<bool>,
    active: Mutex<BTreeMap<u64, AbortHandle>>,
    next: AtomicU64,
}
pub(crate) fn write_private(path: &Path, value: &impl Serialize) -> Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    serde_json::to_writer(&mut temp, value)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    std::fs::File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}
pub(crate) async fn open_runtime(
    root: &Path,
    record: &SourceRecord,
    descriptor: &Descriptor,
    replay: Option<&str>,
) -> Result<Runtime> {
    let mut options = descriptor::options(
        &record.source_id,
        Some(
            root.join("sources")
                .join(&record.source_id)
                .join("storage.json"),
        ),
        false,
    );
    if let Some(base) = replay {
        options.modules.insert(
            "haiku".into(),
            format!(
                "globalThis.__haikuReplayBase={};\n{}",
                serde_json::to_string(base)?,
                sdk::MODULE
            ),
        );
    }
    let vm = Runtime::with_options(options)?;
    vm.load_package(&root.join("packages").join(&descriptor.id), "plugin.ts")
        .await?;
    vm.load_module("haiku-host-bridge.js", sdk::BRIDGE).await?;
    Ok(vm)
}
impl Source {
    pub(crate) async fn open(
        root: &Path,
        record: &SourceRecord,
        descriptor: Descriptor,
        replay: Option<&str>,
    ) -> Result<Self> {
        let credentials: Credentials = haiku_store::json_file::read(
            &root
                .join("sources")
                .join(&record.source_id)
                .join("credential.json"),
        )?
        .unwrap_or_default();
        Self::open_with(
            root,
            record,
            descriptor,
            credentials.credential,
            credentials.connection,
            replay,
        )
        .await
    }
    pub(crate) async fn open_with(
        root: &Path,
        record: &SourceRecord,
        descriptor: Descriptor,
        credential: Value,
        connection: Option<BTreeMap<String, String>>,
        replay: Option<&str>,
    ) -> Result<Self> {
        let directory = root.join("sources").join(&record.source_id);
        std::fs::create_dir_all(&directory)?;
        let runtime = open_runtime(root, record, &descriptor, replay).await?;
        Ok(Self {
            record: record.clone(),
            descriptor: Mutex::new(descriptor),
            runtime: tokio::sync::RwLock::new(runtime),
            directory,
            credentials: Mutex::new(Credentials {
                credential,
                connection,
            }),
            generation: AtomicU64::new(0),
            refresh: tokio::sync::Mutex::new(()),
            needs_login: Mutex::new(false),
            active: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(0),
        })
    }
    pub fn descriptor(&self) -> Descriptor {
        self.descriptor.lock().unwrap().clone()
    }
    pub fn needs_login(&self) -> bool {
        *self.needs_login.lock().unwrap()
    }
    pub fn redacted_connection(&self) -> Option<BTreeMap<String, String>> {
        let descriptor = self.descriptor();
        self.credentials
            .lock()
            .unwrap()
            .connection
            .clone()
            .map(|mut values| {
                for (key, value) in &mut values {
                    if descriptor
                        .connection
                        .as_ref()
                        .is_some_and(|c| c["fields"][key]["type"] == "secret")
                    {
                        *value = "[REDACTED]".into();
                    }
                }
                values
            })
    }
    pub(crate) fn persist_credentials(&self) -> Result<()> {
        write_private(
            &self.directory.join("credential.json"),
            &*self.credentials.lock().unwrap(),
        )
    }
    fn set_credential(&self, value: Value, merge: bool) -> Result<()> {
        let mut guard = self.credentials.lock().unwrap();
        let before = guard.clone();
        let value = if merge {
            match (&guard.credential, value) {
                (Value::Object(old), Value::Object(new)) => {
                    let mut merged = old.clone();
                    merged.extend(new);
                    Value::Object(merged)
                }
                (_, v) => v,
            }
        } else {
            value
        };
        guard.credential = value;
        if let Err(error) = write_private(&self.directory.join("credential.json"), &*guard) {
            *guard = before;
            return Err(error);
        }
        self.generation.fetch_add(1, Ordering::AcqRel);
        *self.needs_login.lock().unwrap() = false;
        Ok(())
    }
    pub fn cancel_all(&self) {
        for handle in self.active.lock().unwrap().values() {
            handle.abort();
        }
    }
    pub(crate) async fn call_raw(
        &self,
        path: &str,
        args: Vec<Value>,
        extra: Value,
        cancel: Option<&Cancellation>,
    ) -> Result<Value> {
        if cancel.is_some_and(Cancellation::is_cancelled) {
            return Err(PluginError::new("CANCELLED", "Call cancelled"));
        }
        let mut context = extra.as_object().cloned().unwrap_or_default();
        {
            let credentials = self.credentials.lock().unwrap();
            context.insert("credential".into(), credentials.credential.clone());
            context.insert("connection".into(), json!(credentials.connection));
        }
        context.insert("locale".into(), json!("zh-CN"));
        let vm = self.runtime.read().await;
        let call = vm.start_call(
            "invoke",
            json!({"path":path,"args":args}),
            Value::Object(context),
            Duration::from_secs(30),
        )?;
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let abort = call.abort.clone();
        self.active.lock().unwrap().insert(id, abort.clone());
        let response = call.result();
        tokio::pin!(response);
        let result = if let Some(cancel) = cancel {
            tokio::select! {biased;_ = cancel.cancelled()=>{abort.abort();response.await.map_err(Into::into)},result=&mut response=>result.map_err(Into::into)}
        } else {
            response.await.map_err(Into::into)
        };
        self.active.lock().unwrap().remove(&id);
        result
    }
    /// Invoke one SDK handler. Authentication refresh is single-flight with one replay.
    pub async fn invoke(
        &self,
        path: &str,
        args: Vec<Value>,
        extra: Value,
        cancel: Option<&Cancellation>,
    ) -> Result<Value> {
        let version = self.generation.load(Ordering::Acquire);
        let result = self
            .call_raw(path, args.clone(), extra.clone(), cancel)
            .await;
        if !result.as_ref().is_err_and(|e| e.code == "AUTH_EXPIRED") {
            return result;
        }
        {
            let _refresh = self.refresh.lock().await;
            if self.generation.load(Ordering::Acquire) == version {
                if *self.needs_login.lock().unwrap() {
                    return Err(PluginError::new("AUTH_REQUIRED", "Please sign in again"));
                }
                if !self
                    .descriptor()
                    .capabilities
                    .iter()
                    .any(|p| p == "auth.refresh")
                {
                    *self.needs_login.lock().unwrap() = true;
                    return result;
                }
                match self
                    .call_raw("auth.refresh", vec![], json!({}), cancel)
                    .await
                {
                    Ok(credential) => self.set_credential(credential, true)?,
                    Err(error) => {
                        if error.code != "CANCELLED" {
                            *self.needs_login.lock().unwrap() = true;
                        }
                        return Err(error);
                    }
                }
            }
        }
        let replay = self.call_raw(path, args, extra, cancel).await;
        if replay
            .as_ref()
            .is_err_and(|e| matches!(e.code.as_str(), "AUTH_EXPIRED" | "AUTH_REQUIRED"))
        {
            *self.needs_login.lock().unwrap() = true;
        }
        replay
    }
    pub async fn logout(&self) -> Result<()> {
        let _lock = self.refresh.lock().await;
        self.cancel_all();
        let remote = if self
            .descriptor()
            .capabilities
            .iter()
            .any(|p| p == "auth.logout")
        {
            self.call_raw("auth.logout", vec![], json!({}), None)
                .await
                .map(|_| ())
        } else {
            Ok(())
        };
        self.set_credential(Value::Null, false)?;
        remote
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoginResult {
    pub credential: Value,
    pub user: Option<Value>,
}
pub struct QrSession {
    pub presentation: Value,
    state: Value,
    deadline: tokio::time::Instant,
    next: tokio::time::Instant,
    pub cancel: Cancellation,
}
pub struct LoginDraft {
    pub(crate) host: Arc<Host>,
    pub(crate) source: Arc<Source>,
    pub(crate) dir: Option<tempfile::TempDir>,
    pub(crate) authenticated: bool,
    pub(crate) sms: tokio::sync::Mutex<Option<(Value, tokio::time::Instant)>>,
}
impl Drop for LoginDraft {
    fn drop(&mut self) {
        self.source.cancel_all();
    }
}
impl LoginDraft {
    async fn accept(&mut self, value: Value) -> Result<LoginResult> {
        let login: LoginResult = serde_json::from_value(value)?;
        if login.credential.is_null() {
            return Err(PluginError::new("AUTH_REQUIRED", "Empty credential"));
        }
        self.source
            .set_credential(login.credential.clone(), false)?;
        self.authenticated = true;
        Ok(login)
    }
    pub async fn password(
        &mut self,
        values: BTreeMap<String, String>,
        cancel: Option<&Cancellation>,
    ) -> Result<LoginResult> {
        let value = self
            .source
            .call_raw(
                "auth.password.submit",
                vec![json!(values)],
                json!({}),
                cancel,
            )
            .await?;
        self.accept(value).await
    }
    pub async fn cookie(
        &mut self,
        cookie: &str,
        cancel: Option<&Cancellation>,
    ) -> Result<LoginResult> {
        let value = self
            .source
            .call_raw("auth.cookie.submit", vec![json!(cookie)], json!({}), cancel)
            .await?;
        self.accept(value).await
    }
    pub async fn sms_send(&self, phone: &str, cancel: Option<&Cancellation>) -> Result<Value> {
        let mut session = self.sms.lock().await;
        if let Some((_, until)) = &*session {
            if *until > tokio::time::Instant::now() {
                let mut error = PluginError::new("RATE_LIMITED", "SMS cooldown is active");
                error.details = Some(
                    json!({"retryAfter": (*until - tokio::time::Instant::now()).as_secs_f64()}),
                );
                return Err(error);
            }
        }
        let value = self
            .source
            .call_raw("auth.sms.send", vec![json!(phone)], json!({}), cancel)
            .await?;
        let state = value
            .get("state")
            .cloned()
            .ok_or_else(|| PluginError::new("PARSE_ERROR", "SMS state missing"))?;
        let cooldown = value["cooldown"].as_f64().unwrap_or(60.).clamp(0., 86400.);
        *session = Some((
            state,
            tokio::time::Instant::now() + Duration::from_secs_f64(cooldown),
        ));
        Ok(json!({"cooldown":cooldown}))
    }
    pub async fn sms_verify(
        &mut self,
        code: &str,
        cancel: Option<&Cancellation>,
    ) -> Result<LoginResult> {
        let state = self
            .sms
            .lock()
            .await
            .as_ref()
            .map(|s| s.0.clone())
            .ok_or_else(|| PluginError::new("AUTH_REQUIRED", "Send an SMS first"))?;
        let value = self
            .source
            .call_raw(
                "auth.sms.verify",
                vec![state, json!(code)],
                json!({}),
                cancel,
            )
            .await?;
        let login = self.accept(value).await?;
        *self.sms.lock().await = None;
        Ok(login)
    }
    /// The caller supplies a page from its isolated web session; no webview is created here.
    pub async fn web_done(
        &mut self,
        page: Value,
        cancel: Option<&Cancellation>,
    ) -> Result<Option<LoginResult>> {
        let value = self
            .source
            .call_raw("auth.web.done", vec![page], json!({}), cancel)
            .await?;
        if value.is_null() {
            Ok(None)
        } else {
            self.accept(value).await.map(Some)
        }
    }
    pub async fn qrcode_start(&self, variant: &str, cancel: Cancellation) -> Result<QrSession> {
        let value = self
            .source
            .call_raw(
                "auth.qrcode.start",
                vec![json!(variant)],
                json!({}),
                Some(&cancel),
            )
            .await?;
        let now = tokio::time::Instant::now();
        let lifetime = value["expiresIn"]
            .as_f64()
            .unwrap_or(120.)
            .clamp(0., 86400.);
        Ok(QrSession {
            state: value["state"].clone(),
            presentation: value,
            deadline: now + Duration::from_secs_f64(lifetime),
            next: now + Duration::from_secs(2),
            cancel,
        })
    }
    pub async fn qrcode_poll(&mut self, session: &mut QrSession) -> Result<Value> {
        tokio::select! {biased;_=session.cancel.cancelled()=>return Err(PluginError::new("CANCELLED","QR cancelled")),_=tokio::time::sleep_until(session.deadline)=>return Ok(json!({"status":"expired"})),_=tokio::time::sleep_until(session.next)=>{}}
        session.next = tokio::time::Instant::now() + Duration::from_secs(2);
        let poll_cancel = Cancellation::default();
        let source = self.source.clone();
        let call = source.call_raw(
            "auth.qrcode.poll",
            vec![session.state.clone()],
            json!({}),
            Some(&poll_cancel),
        );
        tokio::pin!(call);
        let value = tokio::select! {
            biased;
            _ = session.cancel.cancelled() => {
                poll_cancel.cancel();
                let _ = call.await;
                return Err(PluginError::new("CANCELLED", "QR cancelled"));
            }
            _ = tokio::time::sleep_until(session.deadline) => {
                poll_cancel.cancel();
                let _ = call.await;
                return Ok(json!({"status":"expired"}));
            }
            result = &mut call => result?,
        };
        if value["status"] == "done" {
            self.accept(value.clone()).await?;
        }
        Ok(value)
    }
    pub async fn commit(mut self) -> Result<Arc<Source>> {
        let host = self.host.clone();
        host.commit_login(&mut self).await
    }
}
