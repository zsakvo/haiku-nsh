//! Engine adapter for the Rust lyrics authority; TSX receives low-frequency snapshots only.
use crate::core::{CoreBridge, decode};
use futures::future::BoxFuture;
use haiku_lyrics::{
    providers::{Candidate, Query, Text},
    service::{Options, Origin},
};
use haiku_model::Track;
use nsh_core::{
    JsonValue,
    element::{Trace, Tracer},
    prelude::*,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc, time::Duration};

pub(crate) struct Sources(pub Arc<haiku_plugins::Host>);
impl Origin for Sources {
    fn local(&self, t: Track) -> BoxFuture<'_, Result<Option<Text>, String>> {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                t.local_path
                    .and_then(|p| haiku_library::local_scan::read_lyrics(std::path::Path::new(&p)))
                    .map(|l| Text {
                        text: l.text,
                        format: "lrc".into(),
                        translation: l.translation,
                        instrumental: false,
                    })
            })
            .await
            .map_err(|e| e.to_string())
        })
    }
    fn plugin(&self, t: Track) -> BoxFuture<'_, Result<Option<Text>, String>> {
        Box::pin(async move {
            if t.source == "local" {
                return Ok(None);
            }
            let source = self.0.source(&t.source).map_err(|e| e.to_string())?;
            if !source
                .descriptor()
                .capabilities
                .iter()
                .any(|c| c == "track.lyrics")
            {
                return Ok(None);
            }
            struct Cancel(haiku_plugins::Cancellation);
            impl Drop for Cancel {
                fn drop(&mut self) {
                    self.0.cancel();
                }
            }
            let cancel = Cancel(Default::default());
            haiku_plugins::SourceApi::new(source)
                .lyrics(&t.id, Some(&cancel.0))
                .await
                .map(|r| {
                    r.map(|l| Text {
                        text: l.text,
                        format: l.format,
                        translation: l.translation,
                        instrumental: false,
                    })
                })
                .map_err(|e| e.to_string())
        })
    }
}
pub(crate) fn options(b: &CoreBridge) -> Options {
    serde_json::from_value(serde_json::to_value(b.settings.get_untracked()).unwrap())
        .unwrap_or_default()
}
#[derive(Clone, Default)]
struct Listeners(Rc<RefCell<BTreeMap<u64, Callback<JsonValue>>>>);
unsafe impl Trace for Listeners {
    fn trace(&self, t: &mut Tracer<'_>) {
        self.0.as_ref().trace(t);
    }
}
/// Remote/local lyrics, manual selections and reference-compatible provider configuration.
#[class]
pub struct LyricsService {
    bridge: Rc<CoreBridge>,
    listeners: Listeners,
    next: u64,
}
impl LyricsService {
    pub(crate) fn new(bridge: Rc<CoreBridge>) -> Instance<Self> {
        let listeners = Listeners::default();
        let callbacks = listeners.clone();
        let service = bridge.lyrics.clone();
        let last = std::cell::RefCell::new(String::new());
        // Register under the engine owner; synchronous calls only mutate service state.
        set_interval(Duration::from_millis(100), move || {
            let value = serde_json::to_value(service.snapshot()).unwrap();
            let encoded = value.to_string();
            if *last.borrow() == encoded {
                return;
            }
            *last.borrow_mut() = encoded;
            for cb in callbacks.0.borrow().values() {
                cb.post(value.clone());
            }
        });
        Instance::new(Self {
            bridge,
            listeners,
            next: 0,
        })
    }
}
#[class]
impl LyricsService {
    /// Current loading/track/generation/lyrics/error/sourceErrors snapshot.
    pub fn snapshot(&self) -> JsonValue {
        serde_json::to_value(self.bridge.lyrics.snapshot()).unwrap()
    }
    /// Initial and subsequent low-frequency states; Stop removes this subscription.
    pub fn on_change(&mut self, #[args(state)] callback: Callback<JsonValue>) -> Stop {
        self.next += 1;
        let id = self.next;
        callback.post(self.snapshot());
        self.listeners.0.borrow_mut().insert(id, callback);
        let listeners = self.listeners.clone();
        Stop::new(move || {
            listeners.0.borrow_mut().remove(&id);
        })
    }
    /// Search {title,artist,album?,durationSec}; provider IDs optionally narrow enabled sources.
    pub async fn search(
        this: Instance<Self>,
        query: JsonValue,
        providers: Option<Vec<String>>,
    ) -> Result<JsonValue, JsError> {
        let q: Query = decode(query)?;
        let (service, o) = this.with(|s| (s.bridge.lyrics.clone(), options(&s.bridge)));
        background(async move {
            service
                .search(q, providers, o)
                .await
                .map(|r| serde_json::to_value(r).unwrap())
        })
        .await
        .map_err(|e| JsError::Error(e.to_string()))?
        .map_err(JsError::Error)
    }
    /// Resolve a candidate into the same parsed line/word structure used by the Rust view.
    pub async fn resolve(this: Instance<Self>, candidate: JsonValue) -> Result<JsonValue, JsError> {
        let c: Candidate = decode(candidate)?;
        let (service, o) = this.with(|s| (s.bridge.lyrics.clone(), options(&s.bridge)));
        background(async move {
            service
                .resolve(c, o)
                .await
                .map(|r| serde_json::to_value(r.parsed()).unwrap())
        })
        .await
        .map_err(|e| JsError::Error(e.to_string()))?
        .map_err(JsError::Error)
    }
    /// Persist a manual candidate even when media caching is disabled; reject stale selections.
    pub async fn adopt(this: Instance<Self>, candidate: JsonValue) -> Result<(), JsError> {
        let c: Candidate = decode(candidate)?;
        let (service, o) = this.with(|s| (s.bridge.lyrics.clone(), options(&s.bridge)));
        background(async move { service.adopt(c, o).await })
            .await
            .map_err(|e| JsError::Error(e.to_string()))?
            .map_err(JsError::Error)
    }
    /// Remove this track's override/cache, then restart the automatic source chain.
    pub async fn reset_to_auto(this: Instance<Self>) -> Result<(), String> {
        let (service, o, host) = this.with(|s| {
            (
                s.bridge.lyrics.clone(),
                options(&s.bridge),
                s.bridge.source_host.clone(),
            )
        });
        background(async move {
            let track = service.snapshot().track;
            service.reset()?;
            if let Some(t) = track {
                service.load(t, o, Arc::new(Sources(host))).await;
            }
            Ok::<_, String>(())
        })
        .await
        .map_err(|e| e.to_string())?
    }
}
