//! Each host root has its own bounded, nonblocking diagnostics channel.
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, atomic::{AtomicBool, Ordering}},
};

/// Process-wide: plugin HTTP traffic is traced (with body previews) only while this is on,
/// i.e. while the developer tools window is open.
pub(crate) fn http_capture() -> Arc<AtomicBool> {
    static CAPTURE: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    CAPTURE.get_or_init(Default::default).clone()
}
pub fn set_http_capture(on: bool) {
    http_capture().store(on, Ordering::Relaxed);
}
use tokio::sync::broadcast;

pub(crate) fn sender(root: &Path) -> broadcast::Sender<Value> {
    static CHANNELS: OnceLock<Mutex<BTreeMap<PathBuf, broadcast::Sender<Value>>>> = OnceLock::new();
    let mut channels = CHANNELS.get_or_init(Default::default).lock().unwrap();
    // Do not retain abandoned hosts; a live receiver or runtime sender keeps its channel alive.
    channels.retain(|_, channel| channel.receiver_count() > 0 || channel.strong_count() > 1);
    channels
        .entry(root.to_path_buf())
        .or_insert_with(|| broadcast::channel(128).0)
        .clone()
}
