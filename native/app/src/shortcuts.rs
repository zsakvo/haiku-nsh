//! One durable shortcut authority; native callbacks enter the existing playback controller.
use crate::{core::CoreBridge, settings::SettingsService};
use haiku_macos::shortcuts::{GlobalShortcuts, SystemShortcuts};
use haiku_store::{
    settings::SettingsStore,
    shortcuts::{self, Binding},
};
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
/// App and global shortcut groups. Empty global chords disable registration.
#[class]
pub struct ShortcutsService {
    store: Arc<SettingsStore>,
    settings: Instance<SettingsService>,
    bridge: Rc<CoreBridge>,
    backend: Arc<SystemShortcuts>,
    serial: Arc<tokio::sync::Mutex<()>>,
    listeners: Listeners,
    next: u64,
}
impl ShortcutsService {
    pub(crate) fn new(
        store: Arc<SettingsStore>,
        settings: Instance<SettingsService>,
        bridge: Rc<CoreBridge>,
        native: bool,
    ) -> Instance<Self> {
        Instance::new(Self {
            store,
            settings,
            bridge,
            backend: Arc::new(SystemShortcuts { native }),
            serial: Arc::new(tokio::sync::Mutex::new(())),
            listeners: Listeners::default(),
            next: 0,
        })
    }
    pub(crate) fn current(&self) -> Vec<Binding> {
        let s = self.store.get();
        shortcuts::effective(&s.shortcuts, &s.global_shortcuts)
    }
    async fn replace(
        this: Instance<Self>,
        reset: Option<Option<String>>,
        bindings: Option<Vec<Binding>>,
    ) -> Result<JsonValue, String> {
        let (serial, store, backend, settings) = this.with(|s| {
            (
                s.serial.clone(),
                s.store.clone(),
                s.backend.clone(),
                s.settings.clone(),
            )
        });
        let _guard = serial.lock().await;
        let previous = this.with(|s| s.current());
        let next = match reset {
            None => bindings.ok_or("缺少快捷键组")?,
            Some(None) => shortcuts::defaults(),
            Some(Some(id)) => {
                let (scope, action) = id
                    .split_once(':')
                    .ok_or("使用 app:操作 或 global:操作 ID")?;
                let mut next = previous.clone();
                let default = shortcuts::defaults()
                    .into_iter()
                    .find(|b| b.scope == scope && b.id == action)
                    .ok_or("未知快捷键操作")?;
                *next
                    .iter_mut()
                    .find(|b| b.scope == scope && b.id == action)
                    .ok_or("未知快捷键操作")? = default;
                next
            }
        };
        let next = shortcuts::validate(&next)?;
        blocking(move || {
            backend.replace(&next)?;
            if let Err(error) = store.replace_shortcuts(&next) {
                backend
                    .replace(&previous)
                    .map_err(|restore| format!("{error}; 注册回滚失败：{restore}"))?;
                return Err(error);
            }
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())??;
        SettingsService::refresh(settings).await?;
        let snapshot = this.with(|s| s.snapshot());
        this.with(|s| {
            for cb in s.listeners.0.borrow().values() {
                cb.post(snapshot.clone());
            }
        });
        Ok(snapshot)
    }
}
#[class]
impl ShortcutsService {
    /// Current complete bindings, defaults and macOS support. App IDs retain reference spelling.
    pub fn snapshot(&self) -> JsonValue {
        serde_json::json!({"supported":self.backend.supported(),"bindings":self.current(),"defaults":shortcuts::defaults(),"labels":shortcuts::ACTIONS.iter().map(|(id,label,_)|(*id,*label)).chain([("showMainWindow","显示主窗口")]).collect::<BTreeMap<_,_>>()})
    }
    /// Validate and atomically replace the complete app/global group; rejection retains previous settings.
    pub async fn apply(this: Instance<Self>, bindings: JsonValue) -> Result<JsonValue, String> {
        let bindings =
            serde_json::from_value(bindings).map_err(|e| format!("快捷键格式错误：{e}"))?;
        Self::replace(this, None, Some(bindings)).await
    }
    /// Restore one qualified ID, such as app:togglePlayback or global:nextTrack.
    pub async fn reset_one(this: Instance<Self>, id: String) -> Result<JsonValue, String> {
        Self::replace(this, Some(Some(id)), None).await
    }
    /// Restore all app defaults and remove every global binding in one transaction.
    pub async fn reset_all(this: Instance<Self>) -> Result<JsonValue, String> {
        Self::replace(this, Some(None), None).await
    }
    /// Hear the initial committed snapshot and successful replacements. Stop unregisters once.
    pub fn on_change(&mut self, #[args(snapshot)] listener: Callback<JsonValue>) -> Stop {
        self.next += 1;
        let id = self.next;
        listener.post(self.snapshot());
        self.listeners.0.borrow_mut().insert(id, listener);
        let listeners = self.listeners.clone();
        Stop::new(move || {
            listeners.0.borrow_mut().remove(&id);
        })
    }
    /// Dispatch an app binding through the same Controller/collections path as global events and UI.
    pub async fn trigger(this: Instance<Self>, id: String) -> Result<(), String> {
        let bridge = this.with(|s| s.bridge.clone());
        dispatch(&bridge, &id).await
    }
}
pub(crate) async fn dispatch(bridge: &Rc<CoreBridge>, id: &str) -> Result<(), String> {
    use haiku_playback::Command;
    let command = match id {
        "togglePlayback" => Command::Toggle,
        "previousTrack" => Command::Previous,
        "nextTrack" => Command::Next,
        "volumeUp" => Command::SetVolume((bridge.controller.state().volume + 0.05).min(1.)),
        "volumeDown" => Command::SetVolume((bridge.controller.state().volume - 0.05).max(0.)),
        "toggleLiked" => {
            if let Some(t) = bridge.controller.state().current {
                bridge.collections.set_liked(&t, None)?;
                bridge
                    .collections_revision
                    .set(bridge.collections.revision());
            }
            return Ok(());
        }
        "showMainWindow" => {
            #[cfg(target_os = "macos")]
            haiku_macos::lifecycle::show_main_window();
            return Ok(());
        }
        "toggleDesktopLyrics" => return Ok(()),
        _ => return Err("未知快捷键操作".into()),
    };
    bridge
        .controller
        .execute(command)
        .await
        .map_err(|e| e.message)
}
