//! Engine-thread subscriptions over the Rust Gist synchronization authority.
use haiku_services::sync::Service;
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

/// Reference-compatible Gist sync, with a single writer and cancellable requests.
#[class]
pub struct SyncService {
    service: Arc<Service>,
    isolated: bool,
    fixture: Option<Arc<haiku_services::sync_fixture::Server>>,
    listeners: Listeners,
    next: u64,
    owner: Owner,
    _exit: crate::exit::ExitGuard,
}
impl SyncService {
    pub(crate) fn authority(&self) -> Arc<Service> { self.service.clone() }
    pub(crate) fn new(service: Arc<Service>, isolated: bool) -> Instance<Self> {
        let listeners = Listeners::default();
        let callbacks = listeners.clone();
        let mut events = service.subscribe();
        let owner = Owner::detached();
        owner.run(|| {
            spawn(async move {
                while events.changed().await.is_ok() {
                    let value = serde_json::to_value(events.borrow_and_update().clone()).unwrap();
                    let callbacks = callbacks.0.borrow().values().cloned().collect::<Vec<_>>();
                    for callback in callbacks {
                        callback.post(value.clone());
                    }
                }
            });
        });
        service.start();
        let on_exit = service.clone();
        let exit = crate::exit::register(move || {
            on_exit.stop();
            Ok(())
        });
        Instance::new(Self {
            service,
            isolated,
            fixture: None,
            listeners,
            next: 0,
            owner,
            _exit: exit,
        })
    }
    pub(crate) async fn close(this: Instance<Self>) -> Result<(), String> {
        let service = this.with(|s| s.service.clone());
        background(async move { service.shutdown().await })
            .await
            .map_err(|e| e.to_string())?
    }
}
impl Drop for SyncService {
    fn drop(&mut self) {
        self.service.stop();
        self.owner.dispose();
    }
}
#[class]
impl SyncService {
    /// Start a loopback GitHub API only for an isolated Core with memory credentials.
    pub async fn start_fixture(this: Instance<Self>) -> Result<(), JsError> {
        let (isolated, service) = this.with(|s| (s.isolated, s.service.clone()));
        if !isolated {
            return Err(JsError::Error("同步夹具仅限隔离 Core".into()));
        }
        let server = blocking(move || {
            let server = Arc::new(haiku_services::sync_fixture::Server::start()?);
            service.set_fixture_base(server.url.clone())?;
            Ok::<_, String>(server)
        })
        .await
        .map_err(|e| JsError::Error(e.to_string()))?
        .map_err(JsError::Error)?;
        this.with_mut(|s| s.fixture = Some(server));
        Ok(())
    }
    /// Current connection, Gist, version, timestamps and retry state; never exposes the PAT.
    pub fn snapshot(&self) -> JsonValue {
        serde_json::to_value(self.service.snapshot()).unwrap()
    }
    /// Validate a GitHub PAT and save it in the credential backend.
    pub async fn set_token(this: Instance<Self>, token: String) -> Result<(), JsError> {
        let service = this.with(|s| s.service.clone());
        background(async move { service.set_token(token).await })
            .await
            .map_err(|e| JsError::Error(e.to_string()))?
            .map_err(JsError::Error)
    }
    /// Clear the credential while retaining the Gist binding.
    pub async fn clear_token(this: Instance<Self>) -> Result<(), JsError> {
        let service = this.with(|s| s.service.clone());
        blocking(move || service.clear_token())
            .await
            .map_err(|e| JsError::Error(e.to_string()))?
            .map_err(JsError::Error)
    }
    /// Create a private Gist containing the current Loro snapshot.
    pub async fn create_gist(this: Instance<Self>) -> Result<String, JsError> {
        let service = this.with(|s| s.service.clone());
        background(async move { service.create_gist().await })
            .await
            .map_err(|e| JsError::Error(e.to_string()))?
            .map_err(JsError::Error)
    }
    /// Bind a Gist ID or gist.github.com URL, then merge and push.
    pub async fn bind_gist(this: Instance<Self>, id_or_url: String) -> Result<(), JsError> {
        let service = this.with(|s| s.service.clone());
        background(async move { service.bind_gist(id_or_url).await })
            .await
            .map_err(|e| JsError::Error(e.to_string()))?
            .map_err(JsError::Error)
    }
    /// Remove the PAT and Gist binding; local and remote collections are retained.
    pub async fn disconnect(this: Instance<Self>) -> Result<(), JsError> {
        let service = this.with(|s| s.service.clone());
        blocking(move || service.disconnect())
            .await
            .map_err(|e| JsError::Error(e.to_string()))?
            .map_err(JsError::Error)
    }
    /// Pull, merge, persist and push in a single shared synchronization cycle.
    pub async fn sync_now(this: Instance<Self>) -> Result<(), JsError> {
        let service = this.with(|s| s.service.clone());
        background(async move { service.sync_now(false).await })
            .await
            .map_err(|e| JsError::Error(e.to_string()))?
            .map_err(JsError::Error)
    }
    /// Subscribe to the initial status and later changes; Stop detaches the callback.
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
