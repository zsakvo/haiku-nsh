//! System callbacks return through a channel before entering the normal Controller.
use crate::core::CoreBridge;
use haiku_playback::{Command, Mode, Status};
use nsh_core::prelude::*;
use std::{rc::Rc, time::Duration};

pub(super) fn initialize(bridge: Rc<CoreBridge>) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let native = haiku_macos::events::subscribe();
    std::thread::spawn(move || {
        while let Ok(event) = native.recv() {
            if tx.send(event).is_err() {
                break;
            }
        }
    });
    let controller = bridge.controller.clone();
    let collections = bridge.collections.clone();
    spawn(async move {
        while let Some(event) = rx.recv().await {
            if event.name == "media.event" {
                if event.data["kind"].as_str() == Some("favorite") {
                    if let Some(track) = controller.state().current {
                        let _ = collections.set_liked(&track, None);
                    }
                    continue;
                }
                if let Some(command) = remote_command(&event.data, controller.state().mode) {
                    if let Err(e) = controller.execute(command).await {
                        log::warn!("Media command: {}", e.message);
                    }
                }
            }
        }
    });
    let ready = signal(haiku_macos::lifecycle::native_host());
    set_interval(Duration::from_millis(200), move || {
        if !ready.get_untracked() && haiku_macos::lifecycle::native_host() {
            ready.set(true);
        }
    });
    // One consumer preserves main-queue ordering, including rapid pause/seek/close.
    enum Update {
        Info(Option<haiku_macos::system::MediaInfo>),
        Awake(bool),
    }
    let (updates, mut pending) = tokio::sync::mpsc::unbounded_channel();
    spawn(async move {
        while let Some(update) = pending.recv().await {
            let _ = blocking(move || match update {
                Update::Info(Some(info)) => {
                    haiku_macos::system::media_update(info);
                    #[cfg(debug_assertions)]
                    log::info!(
                        "Media center snapshot {}",
                        haiku_macos::system::media_snapshot()
                    );
                }
                Update::Info(None) => haiku_macos::system::media_clear(),
                Update::Awake(awake) => {
                    if let Err(e) = haiku_macos::system::keep_awake(awake) {
                        log::warn!("Display sleep: {e}");
                    }
                }
            })
            .await;
        }
    });
    let state = bridge.state;
    let collections = bridge.collections.clone();
    let revision = bridge.collections_revision;
    let controller = bridge.controller.clone();
    let last = stored(None);
    let info_updates = updates.clone();
    effect(move || {
        if !ready.get() {
            return;
        }
        let s = state.get();
        revision.get();
        let favorite = s
            .current
            .as_ref()
            .is_some_and(|t| collections.is_liked(&t.entity_ref()));
        let key = (
            s.current.clone(),
            s.status,
            s.buffering,
            s.timeline_revision,
            s.next_index,
            s.mode,
            favorite,
        );
        if last.with(|old| old.as_ref() == Some(&key)) {
            return;
        }
        last.replace(Some(key));
        let info = s.current.map(|track| haiku_macos::system::MediaInfo {
            title: track.title,
            artist: track
                .artists
                .iter()
                .map(|a| match a {
                    haiku_model::Artist::Name(n) | haiku_model::Artist::Entity { name: n, .. } => {
                        n.as_str()
                    }
                })
                .collect::<Vec<_>>()
                .join(" / "),
            album: track.album.map(|a| a.title).unwrap_or_default(),
            duration: track.duration.unwrap_or(0.),
            position: controller.position_snapshot(),
            playing: s.status == Status::Playing && !s.buffering,
            artwork_path: track
                .cover
                .filter(|p| !p.starts_with("http"))
                .map(|p| p.strip_prefix("file://").unwrap_or(&p).to_owned()),
            can_next: s.next_index.is_some(),
            can_previous: s.index >= 0,
            favorite,
        });
        let _ = info_updates.send(Update::Info(info));
    });
    let settings = bridge.settings;
    effect(move || {
        if !ready.get() {
            return;
        }
        let enabled = settings.get().prevent_screen_sleep_on_player;
        let open = crate::presentation::get()
            .map(|p| p.open.get())
            .unwrap_or(false);
        let awake = enabled && open && lifecycle() != nsh_core::platform::Lifecycle::Background;
        let _ = updates.send(Update::Awake(awake));
    });
}
fn remote_command(data: &serde_json::Value, mode: Mode) -> Option<Command> {
    Some(match data["kind"].as_str()? {
        "play" => Command::Resume,
        "pause" => Command::Pause,
        "toggle" => Command::Toggle,
        "stop" => Command::Stop,
        "next" => Command::Next,
        "previous" => Command::Previous,
        "seek" => Command::Seek(data["position"].as_f64()?),
        "repeat" => Command::SetMode(match data["mode"].as_i64()? {
            1 => Mode::RepeatOne,
            2 => Mode::RepeatAll,
            _ => Mode::Sequential,
        }),
        "shuffle" => Command::SetMode(if data["mode"].as_i64()? == 0 {
            if mode == Mode::Shuffle {
                Mode::Sequential
            } else {
                mode
            }
        } else {
            Mode::Shuffle
        }),
        _ => return None,
    })
}
