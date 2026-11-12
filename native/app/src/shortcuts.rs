//! One durable shortcut authority; native callbacks enter the existing playback controller.
#[cfg(test)]
mod tests;
use crate::{core::CoreBridge, settings::SettingsService};
use haiku_macos::shortcuts::GlobalShortcuts;
use haiku_store::{
    settings::SettingsStore,
    shortcuts::{self, Binding},
};
use nsh_core::{
    JsonValue,
    element::{Trace, Tracer},
    prelude::*,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{Arc, Mutex},
};

/// Shared by live edits and backup reloads so both use the same registration rollback.
pub(crate) struct Registration {
    store: Arc<SettingsStore>,
    backend: Arc<dyn GlobalShortcuts>,
    applied: Mutex<Vec<Binding>>,
    pub serial: Arc<tokio::sync::Mutex<()>>,
}
impl Registration {
    pub(crate) fn new(store: Arc<SettingsStore>, backend: Arc<dyn GlobalShortcuts>) -> Arc<Self> {
        let s = store.get();
        Arc::new(Self {
            applied: Mutex::new(shortcuts::effective(&s.shortcuts, &s.global_shortcuts)),
            store,
            backend,
            serial: Arc::new(tokio::sync::Mutex::new(())),
        })
    }
    fn commit(&self, next: &[Binding]) -> Result<(), String> {
        let next = shortcuts::validate(next)?;
        let mut previous = self.applied.lock().unwrap();
        self.backend.replace(&next)?;
        if let Err(error) = self.store.replace_shortcuts(&next) {
            self.backend
                .replace(&previous)
                .map_err(|restore| format!("{error}; 注册回滚失败：{restore}"))?;
            return Err(error);
        }
        *previous = next;
        Ok(())
    }
    pub(crate) fn reload(&self) -> Result<(), String> {
        let s = self.store.get();
        let next = shortcuts::effective(&s.shortcuts, &s.global_shortcuts);
        if let Err(error) = self.commit(&next) {
            // Restore only shortcut preferences; keep the other restored settings.
            self.store
                .replace_shortcuts(&self.applied.lock().unwrap())
                .map_err(|restore| format!("{error}; 设置回滚失败：{restore}"))?;
            return Err(error);
        }
        Ok(())
    }
}
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
    registration: Arc<Registration>,
    listeners: Listeners,
    next: u64,
}
impl ShortcutsService {
    pub(crate) fn new(
        store: Arc<SettingsStore>,
        settings: Instance<SettingsService>,
        bridge: Rc<CoreBridge>,
        registration: Arc<Registration>,
    ) -> Instance<Self> {
        Instance::new(Self {
            store,
            settings,
            bridge,
            registration,
            listeners: Listeners::default(),
            next: 0,
        })
    }
    pub(crate) fn current(&self) -> Vec<Binding> {
        let s = self.store.get();
        shortcuts::effective(&s.shortcuts, &s.global_shortcuts)
    }
    pub(crate) fn refresh(this: &Instance<Self>) {
        let snapshot = this.with(|s| s.snapshot());
        this.with(|s| {
            for cb in s.listeners.0.borrow().values() {
                cb.post(snapshot.clone());
            }
        });
    }
    async fn replace(
        this: Instance<Self>,
        reset: Option<Option<String>>,
        bindings: Option<Vec<Binding>>,
    ) -> Result<JsonValue, String> {
        let (registration, settings) = this.with(|s| (s.registration.clone(), s.settings.clone()));
        let serial = registration.serial.clone();
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
        blocking(move || registration.commit(&next))
            .await
            .map_err(|e| e.to_string())??;
        SettingsService::refresh(settings).await?;
        let snapshot = this.with(|s| s.snapshot());
        Self::refresh(&this);
        Ok(snapshot)
    }
}
#[class]
impl ShortcutsService {
    /// Current complete bindings, defaults and macOS support. App IDs retain reference spelling.
    pub fn snapshot(&self) -> JsonValue {
        serde_json::json!({"supported":self.registration.backend.supported(),"bindings":self.current(),"defaults":shortcuts::defaults(),"labels":shortcuts::ACTIONS.iter().map(|(id,label,_)|(*id,*label)).chain([("showMainWindow","显示主窗口")]).collect::<BTreeMap<_,_>>()})
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
        let (bridge, settings) = this.with(|s| (s.bridge.clone(), s.settings.clone()));
        dispatch(&bridge, &id, settings).await
    }
}
pub(crate) async fn dispatch(
    bridge: &Rc<CoreBridge>,
    id: &str,
    settings: Instance<SettingsService>,
) -> Result<(), String> {
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
        "toggleDesktopLyrics" => {
            let visible = bridge.settings.get_untracked().desktop_lyrics.visible;
            SettingsService::update(
                settings,
                serde_json::json!({"desktopLyrics":{"visible":!visible}}),
            )
            .await?;
            return Ok(());
        }
        _ => return Err("未知快捷键操作".into()),
    };
    bridge
        .controller
        .execute(command)
        .await
        .map_err(|e| e.message)
}
