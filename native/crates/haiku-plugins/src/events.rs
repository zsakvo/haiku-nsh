//! Each host root has its own bounded, nonblocking diagnostics channel.
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
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
