//! Download queue facade; all network and filesystem mutations run off the engine thread.
use crate::download_service::Service;
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
/// Persistent offline downloads with cancellable attempts and resumable partial files.
#[class]
pub struct DownloadService {
    service: Arc<Service>,
    listeners: Listeners,
    next: u64,
    owner: Owner,
    _exit: crate::exit::ExitGuard,
}
impl DownloadService {
    pub(crate) async fn close(this: Instance<Self>) -> Result<(), String> {
        let service = this.with(|s| s.service.clone());
        background(async move { service.shutdown().await })
            .await
            .map_err(|e| e.to_string())?
    }
    pub(crate) fn new(service: Arc<Service>) -> Instance<Self> {
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
            })
        });
        let scheduler = service.clone();
        haiku_services::downloader::runtime().spawn(async move {
            scheduler.pump();
        });
        let exit_service = service.clone();
        let exit = crate::exit::register(move || exit_service.stop());
        Instance::new(Self {
            service,
            listeners,
            next: 0,
            owner,
            _exit: exit,
        })
    }
}
impl Drop for DownloadService {
    fn drop(&mut self) {
        let _ = self.service.stop();
        self.owner.dispose();
    }
}
#[class]
impl DownloadService {
    /// Enqueue SDK track(s) or {source,kind,id} for an album/playlist/toplist; returns after scheduling.
    pub async fn enqueue(this: Instance<Self>, tracks: JsonValue) -> Result<(), String> {
        let service = this.with(|s| s.service.clone());
        background(async move { service.enqueue(tracks).await })
            .await
            .map_err(|e| e.to_string())?
    }
    /// Pause a task; keep its partial file for resume.
    pub async fn pause(this: Instance<Self>, key: String) -> Result<(), String> {
        let service = this.with(|s| s.service.clone());
        blocking(move || service.control(&key, "pause"))
            .await
            .map_err(|e| e.to_string())?
    }
    /// Resume a paused/canceled/failed/missing task with a fresh retry budget.
    pub async fn resume(this: Instance<Self>, key: String) -> Result<(), String> {
        let service = this.with(|s| s.service.clone());
        blocking(move || service.control(&key, "resume"))
            .await
            .map_err(|e| e.to_string())?
    }
    /// Retry a failed or missing task with a fresh retry budget.
    pub async fn retry(this: Instance<Self>, key: String) -> Result<(), String> {
        let service = this.with(|s| s.service.clone());
        blocking(move || service.control(&key, "retry"))
            .await
            .map_err(|e| e.to_string())?
    }
    /// Cancel an unfinished task; retain the record and partial file.
    pub async fn cancel(this: Instance<Self>, key: String) -> Result<(), String> {
        let service = this.with(|s| s.service.clone());
        blocking(move || service.control(&key, "cancel"))
            .await
            .map_err(|e| e.to_string())?
    }
    /// Remove a record after its worker drains; withFile also deletes its audio, partial and sidecars.
    pub async fn remove(this: Instance<Self>, key: String, with_file: bool) -> Result<(), String> {
        let service = this.with(|s| s.service.clone());
        background(async move { service.remove(&key, with_file).await })
            .await
            .map_err(|e| e.to_string())?
    }
    /// Clear completed records while retaining files on disk.
    pub async fn clear_completed(this: Instance<Self>) -> Result<(), String> {
        let service = this.with(|s| s.service.clone());
        blocking(move || service.clear_completed())
            .await
            .map_err(|e| e.to_string())?
    }
    /// Retry all failed and missing records.
    pub async fn retry_failed(this: Instance<Self>) -> Result<(), String> {
        let service = this.with(|s| s.service.clone());
        blocking(move || service.retry_failed())
            .await
            .map_err(|e| e.to_string())?
    }
    /// Latest task array: state, progress, bytes, files, warnings and errors.
    pub fn snapshot(&self) -> JsonValue {
        serde_json::to_value(self.service.snapshot()).unwrap()
    }
    /// Mark completed records missing when their audio file no longer exists.
    pub async fn verify(this: Instance<Self>) -> Result<(), String> {
        let service = this.with(|s| s.service.clone());
        blocking(move || service.verify())
            .await
            .map_err(|e| e.to_string())?
    }
    /// Reveal the downloaded file in the platform file manager.
    pub async fn reveal(this: Instance<Self>, key: String) -> Result<(), String> {
        let service = this.with(|s| s.service.clone());
        let path = blocking(move || service.path(&key))
            .await
            .map_err(|e| e.to_string())??;
        reveal_path(path).await.map_err(|e| e.to_string())
    }
    /// Subscribe to the initial snapshot and later mutations/progress; Stop unregisters the listener.
    pub fn on_change(&mut self, #[args(snapshot)] callback: Callback<JsonValue>) -> Stop {
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
