//! Stable native-panel worker: changed values and one timer for the next lyric line.
use crate::{
    core::CoreBridge,
    desktop_lyrics::{Projection, project},
};
use haiku_macos::desktop_lyrics_api::{self, Update};
use nsh_core::prelude::*;
use std::{rc::Rc, time::Duration};
pub(super) fn initialize(bridge: Rc<CoreBridge>, ready: Signal<bool>, native: bool) {
    if !native {
        return;
    }
    let p = crate::presentation::get().unwrap();
    let root = bridge.data_root().to_owned();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let weak = Rc::downgrade(&bridge);
    let presentation = Rc::downgrade(&p);
    cleanup(|| desktop_lyrics_api::hide());
    spawn(async move {
        let mut installed = false;
        let mut projection = Projection::default();
        let mut deadline = None;
        loop {
            let wake = async {
                match deadline {
                    Some(at) => timer_until(at).await,
                    None => std::future::pending::<()>().await,
                }
            };
            tokio::select! {value=rx.recv()=>{if value.is_none(){return;}while rx.try_recv().is_ok(){}},_=wake=>{}}
            if !ready.get_untracked() {
                continue;
            }
            let (Some(bridge), Some(p)) = (weak.upgrade(), presentation.upgrade()) else {
                return;
            };
            let settings = bridge.settings.get_untracked();
            let d = &settings.desktop_lyrics;
            if !installed && !d.visible {
                continue;
            }
            if !installed {
                let path = root.join("haiku_state/desktop-lyrics-position.json");
                installed = blocking(move || {
                    let position = match haiku_store::json_file::read(&path) {
                        Ok(p) => p,
                        Err(e) => {
                            log::warn!("Desktop lyrics position: {e}");
                            None
                        }
                    };
                    desktop_lyrics_api::install(position)
                })
                .await
                .unwrap_or(false);
                if !installed {
                    return;
                }
            }
            let s = bridge.controller.state();
            let pos = bridge.controller.position_snapshot();
            let (v, next) = project(
                &s,
                p.track.get_untracked().as_ref(),
                &p.lyrics.get_untracked(),
                d,
                &bridge.collections,
                pos,
            );
            deadline = next.map(|s| nsh_core::time::now() + Duration::from_secs_f64(s.max(0.001)));
            let changes = projection.update(v.clone());
            let mut updates = vec![];
            // Hidden panels need only a visibility update; reopening reapplies all state.
            if v.visible {
                if changes.style {
                    updates.push(Update::Style(v.style));
                }
                if changes.lines {
                    updates.push(Update::Lines {
                        current: v.current,
                        next: v.next,
                        start: v.start,
                        position: pos,
                        paused: !v.playing,
                    });
                }
                if changes.progress {
                    updates.push(Update::Progress(pos, !v.playing));
                }
                if changes.playing {
                    updates.push(Update::Playing(v.playing));
                }
                if changes.favorite {
                    updates.push(Update::Favorite(v.favorite));
                }
                if changes.locked {
                    updates.push(Update::Locked(v.locked));
                }
            }
            if changes.visible {
                updates.push(Update::Show(v.visible));
            }
            if !updates.is_empty() {
                let _ = blocking(move || {
                    desktop_lyrics_api::apply(updates);
                    #[cfg(debug_assertions)]
                    if let Some(snapshot) = desktop_lyrics_api::snapshot() {
                        log::info!("Desktop lyrics snapshot {snapshot}");
                    }
                })
                .await;
            }
        }
    });
    let state = bridge.state;
    let settings = bridge.settings;
    let revision = bridge.collections_revision;
    effect(move || {
        ready.get();
        state.get();
        settings.get();
        revision.get();
        p.track.get();
        p.lyrics.get();
        let _ = tx.send(());
    });
}
