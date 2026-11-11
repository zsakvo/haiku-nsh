//! Rust owns durable settings; TSX subscribes to a committed mirror.
use crate::core::CoreBridge;
use haiku_store::settings::{Settings, SettingsStore};
use nsh_core::{
    JsonValue,
    element::{Trace, Tracer},
    prelude::*,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};
#[derive(Clone, Default)]
struct Listeners(Rc<RefCell<BTreeMap<u64, Callback<JsonValue>>>>);
unsafe impl Trace for Listeners {
    fn trace(&self, t: &mut Tracer<'_>) {
        self.0.as_ref().trace(t);
    }
}
/// Durable settings with reference-compatible names and recursive field merges.
#[class]
pub struct SettingsService {
    store: Arc<SettingsStore>,
    downloads: Arc<crate::download_service::Service>,
    bridge: Rc<CoreBridge>,
    serial: Arc<tokio::sync::Mutex<()>>,
    listeners: Listeners,
    next: u64,
}
impl SettingsService {
    pub(crate) fn new(
        store: Arc<SettingsStore>,
        bridge: Rc<CoreBridge>,
        downloads: Arc<crate::download_service::Service>,
    ) -> Instance<Self> {
        Instance::new(Self {
            store,
            downloads,
            bridge,
            serial: Arc::new(tokio::sync::Mutex::new(())),
            listeners: Listeners::default(),
            next: 0,
        })
    }
}
#[class]
impl SettingsService {
    /// Read the latest durable settings, including fields unknown to this desktop version.
    pub fn get(&self) -> JsonValue {
        serde_json::to_value(self.store.get()).unwrap()
    }
    /// Atomically merge fields, clamp valid numeric values, apply DSP/cache settings, then notify mirrors.
    pub async fn update(this: Instance<Self>, patch: JsonValue) -> Result<JsonValue, String> {
        let (store, bridge, serial, listeners, downloads) = this.with(|s| {
            (
                s.store.clone(),
                s.bridge.clone(),
                s.serial.clone(),
                s.listeners.clone(),
                s.downloads.clone(),
            )
        });
        let _guard = serial.lock().await;
        let settings = blocking(move || {
            let previous = store.get().download_concurrency;
            let next = store.update(patch)?;
            if next.download_concurrency != previous {
                downloads.pump();
            }
            Ok::<_, String>(next)
        })
        .await
        .map_err(|e| e.to_string())??;
        let cache = settings.clone();
        blocking(move || apply_cache(&cache))
            .await
            .map_err(|e| e.to_string())?;
        bridge
            .controller
            .execute(haiku_playback::Command::Configure(settings.clone()))
            .await
            .map_err(|e| e.message)?;
        bridge.settings.set(settings.clone());
        let value = serde_json::to_value(settings).unwrap();
        for callback in listeners.0.borrow().values() {
            callback.post(value.clone());
        }
        Ok(value)
    }
    /// Hear the initial committed settings and later successful updates. Stop unregisters the listener.
    pub fn on_change(&mut self, #[args(settings)] callback: Callback<JsonValue>) -> Stop {
        self.next += 1;
        let id = self.next;
        callback.post(self.get());
        self.listeners.0.borrow_mut().insert(id, callback);
        let listeners = self.listeners.clone();
        Stop::new(move || {
            listeners.0.borrow_mut().remove(&id);
        })
    }
    /// Disk cache usage in bytes and complete entry count; partial files count toward bytes.
    pub async fn cache_usage() -> Result<JsonValue, String> {
        blocking(||{let u=haiku_audio::media_cache::usage();serde_json::json!({"usedBytes":u.used_bytes,"maxBytes":u.max_bytes,"entryCount":u.entry_count})}).await.map_err(|e|e.to_string())
    }
    /// Clear only media-cache objects after the caller's confirmation; original local files are untouched.
    pub async fn clear_cache() -> Result<(), String> {
        blocking(haiku_audio::media_cache::clear)
            .await
            .map_err(|e| e.to_string())?
    }
}
pub(crate) fn apply_cache(s: &Settings) {
    haiku_audio::media_cache::set_enabled(s.media_cache_enabled);
    haiku_audio::media_cache::set_max_bytes(s.media_cache_max_bytes);
}
