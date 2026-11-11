//! Account facade. The authority, credentials and requests live off the engine thread.
use haiku_services::account::Service;
use nsh_core::{
    JsonValue,
    element::{Trace, Tracer},
    prelude::*,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};

#[derive(Clone, Default)]
struct Listeners(Rc<RefCell<BTreeMap<u64, Callback<JsonValue>>>>);
unsafe impl Trace for Listeners {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        self.0.as_ref().trace(tracer);
    }
}

fn error(e: haiku_services::account::AccountError) -> JsError {
    JsError::Error(e.message)
        .with("code", e.code)
        .with("transport", e.transport)
}

/// Haiku account and informational subscription; membership never gates app features.
#[class]
pub struct AccountService {
    service: Arc<Service>,
    uploader: Arc<haiku_services::stats_upload::Service>,
    listeners: Listeners,
    next: u64,
    owner: Owner,
    _exit: crate::exit::ExitGuard,
}
impl AccountService {
    pub(crate) fn new(service: Arc<Service>, stats: haiku_store::stats::Stats) -> Instance<Self> {
        let uploader = haiku_services::stats_upload::Service::start(service.clone(), stats);
        let listeners = Listeners::default();
        let callbacks = listeners.clone();
        let mut events = service.subscribe();
        let upload = uploader.clone();
        let owner = Owner::detached();
        owner.run(|| {
            spawn(async move {
                while events.changed().await.is_ok() {
                    let mut value =
                        serde_json::to_value(events.borrow_and_update().clone()).unwrap();
                    value["upload"] = serde_json::to_value(upload.snapshot()).unwrap();
                    let callbacks = callbacks.0.borrow().values().cloned().collect::<Vec<_>>();
                    for callback in callbacks {
                        callback.post(value.clone());
                    }
                }
            });
        });
        let callbacks = listeners.clone();
        let account = service.clone();
        let mut sync_events = uploader.subscribe();
        owner.run(|| {
            spawn(async move {
                while sync_events.changed().await.is_ok() {
                    let mut value = serde_json::to_value(account.snapshot()).unwrap();
                    value["upload"] =
                        serde_json::to_value(sync_events.borrow_and_update().clone()).unwrap();
                    let callbacks = callbacks.0.borrow().values().cloned().collect::<Vec<_>>();
                    for callback in callbacks {
                        callback.post(value.clone());
                    }
                }
            });
        });
        // Startup restoration is independent of reactive settings effects.
        if service.snapshot().signed_in {
            let restored = service.clone();
            haiku_services::downloader::runtime().spawn(async move {
                let _ = restored.refresh().await;
            });
        }
        let sync_exit = uploader.clone();
        let exit = crate::exit::register(move || {
            sync_exit.stop();
            Ok(())
        });
        Instance::new(Self {
            service,
            uploader,
            listeners,
            next: 0,
            owner,
            _exit: exit,
        })
    }
    pub(crate) fn stop(&self) {
        self.uploader.stop();
    }
}
impl Drop for AccountService {
    fn drop(&mut self) {
        self.uploader.stop();
        self.owner.dispose();
    }
}
#[class]
impl AccountService {
    /// Cached user/subscription, sign-in state, server and email cooldown deadlines; no tokens.
    pub fn snapshot(&self) -> JsonValue {
        let mut value = serde_json::to_value(self.service.snapshot()).unwrap();
        value["upload"] = serde_json::to_value(self.uploader.snapshot()).unwrap();
        value
    }
    /// Send an email code; the authority enforces the case-insensitive 60-second cooldown.
    pub async fn send_code(this: Instance<Self>, email: String) -> Result<(), JsError> {
        let service = this.with(|s| s.service.clone());
        background(async move { service.send_code(email).await })
            .await
            .map_err(|e| JsError::Error(e.to_string()))?
            .map_err(error)
    }
    /// Verify an email code, automatically registering new accounts on the server.
    pub async fn login(
        this: Instance<Self>,
        email: String,
        code: String,
    ) -> Result<JsonValue, JsError> {
        let service = this.with(|s| s.service.clone());
        let snapshot = background(async move { service.login(email, code).await })
            .await
            .map_err(|e| JsError::Error(e.to_string()))?
            .map_err(error)?;
        Ok(serde_json::to_value(snapshot).unwrap())
    }
    /// Clear local credentials immediately, then best-effort revoke the server session.
    pub async fn logout(this: Instance<Self>) -> Result<(), JsError> {
        let service = this.with(|s| s.service.clone());
        background(async move { service.logout().await })
            .await
            .map_err(|e| JsError::Error(e.to_string()))?
            .map_err(error)
    }
    /// Refresh user and membership; transport failures retain the cached offline session.
    pub async fn refresh(this: Instance<Self>) -> Result<JsonValue, JsError> {
        let service = this.with(|s| s.service.clone());
        let snapshot = background(async move { service.refresh().await })
            .await
            .map_err(|e| JsError::Error(e.to_string()))?
            .map_err(error)?;
        Ok(serde_json::to_value(snapshot).unwrap())
    }
    /// Read subscription information with a five-minute cache.
    pub async fn subscription(this: Instance<Self>) -> Result<JsonValue, JsError> {
        let service = this.with(|s| s.service.clone());
        background(async move { service.subscription(false).await })
            .await
            .map_err(|e| JsError::Error(e.to_string()))?
            .map_err(error)
    }
    /// List active plans for display only; purchasing is not available.
    pub async fn plans(this: Instance<Self>) -> Result<JsonValue, JsError> {
        let service = this.with(|s| s.service.clone());
        background(async move { service.plans().await })
            .await
            .map_err(|e| JsError::Error(e.to_string()))?
            .map_err(error)
    }
    /// Persist a validated HTTP(S) server and invalidate the previous session and requests.
    pub async fn set_server(this: Instance<Self>, base: String) -> Result<(), JsError> {
        let service = this.with(|s| s.service.clone());
        blocking(move || service.set_server(base))
            .await
            .map_err(|e| JsError::Error(e.to_string()))?
            .map_err(error)
    }
    /// Subscribe to the initial snapshot and later account changes; Stop detaches.
    pub fn on_change(&mut self, #[args(change)] callback: Callback<JsonValue>) -> Stop {
        self.next += 1;
        let id = self.next;
        callback.post(self.snapshot());
        self.listeners.0.borrow_mut().insert(id, callback);
        let listeners = self.listeners.clone();
        Stop::new(move || {
            listeners.0.borrow_mut().remove(&id);
        })
    }
}
