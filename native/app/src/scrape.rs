//! Typed JS facade; Rust owns job lifetime, progress and durable review results.
use crate::scrape_service::{Options, Service};
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
/// Local metadata scraping; file tag writeback remains an explicit library edit operation.
#[class]
pub struct ScrapeService {
    service: Arc<Service>,
    listeners: Listeners,
    next: u64,
    owner: Owner,
}
impl ScrapeService {
    pub(crate) async fn close(this: Instance<Self>) -> Result<(), String> {
        let service = this.with(|s| s.service.clone());
        background(async move { service.shutdown().await })
            .await
            .map_err(|e| e.to_string())
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
        Instance::new(Self {
            service,
            listeners,
            next: 0,
            owner,
        })
    }
}
impl Drop for ScrapeService {
    fn drop(&mut self) {
        self.service.cancel();
        self.owner.dispose();
    }
}
#[class]
impl ScrapeService {
    /// Queue local track IDs. Options.mode is missing or replace; returns after scheduling.
    pub async fn start(
        this: Instance<Self>,
        ids: Vec<String>,
        options: JsonValue,
    ) -> Result<(), String> {
        let options: Options = serde_json::from_value(options).map_err(|e| e.to_string())?;
        let service = this.with(|s| s.service.clone());
        background(async move { service.start(ids, options) })
            .await
            .map_err(|e| e.to_string())?
    }
    /// Cancel queued/in-flight work while retaining every completed result.
    pub fn cancel(&self) {
        self.service.cancel();
    }
    /// Current running/total/completed state and persisted review items.
    pub fn jobs(&self) -> JsonValue {
        serde_json::to_value(self.service.snapshot()).unwrap()
    }
    /// Pending review and applied items, including candidates, scores and undo records.
    pub fn review(&self) -> JsonValue {
        serde_json::to_value(self.service.snapshot().items).unwrap()
    }
    /// Apply one candidate index from a stored item to sparse metadata overrides.
    pub async fn accept(this: Instance<Self>, id: String, candidate: u32) -> Result<(), String> {
        let service = this.with(|s| s.service.clone());
        blocking(move || service.accept(&id, candidate as usize))
            .await
            .map_err(|e| e.to_string())?
    }
    /// Persist a skipped review without changing metadata or audio files.
    pub async fn skip(this: Instance<Self>, id: String) -> Result<(), String> {
        let service = this.with(|s| s.service.clone());
        blocking(move || service.skip(&id))
            .await
            .map_err(|e| e.to_string())?
    }
    /// Restore the exact previous sparse override; reject conflicts with subsequent manual edits.
    pub async fn undo(this: Instance<Self>, id: String) -> Result<(), String> {
        let service = this.with(|s| s.service.clone());
        blocking(move || service.undo(&id))
            .await
            .map_err(|e| e.to_string())?
    }
    /// Search a review item again using a custom keyword; never silently apply the result.
    pub async fn retry(this: Instance<Self>, id: String, keyword: String) -> Result<(), String> {
        let service = this.with(|s| s.service.clone());
        background(async move { service.retry(&id, keyword).await })
            .await
            .map_err(|e| e.to_string())?
    }
    /// Initial snapshot and later progress/mutations. Stop unregisters this subscriber.
    pub fn on_change(&mut self, #[args(snapshot)] callback: Callback<JsonValue>) -> Stop {
        self.next += 1;
        let id = self.next;
        callback.post(self.jobs());
        self.listeners.0.borrow_mut().insert(id, callback);
        let listeners = self.listeners.clone();
        Stop::new(move || {
            listeners.0.borrow_mut().remove(&id);
        })
    }
}
