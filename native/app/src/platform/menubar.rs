//! A stable consumer owns status-item lifetime and line-boundary wakeups.
use crate::{
    core::CoreBridge,
    menubar::{Projection, project},
};
use haiku_macos::menubar_api::{self, Update};
use nsh_core::prelude::*;
use std::{rc::Rc, time::Duration};

pub(super) fn initialize(bridge: Rc<CoreBridge>, ready: Signal<bool>, native: bool) {
    if !native {
        return;
    }
    let p = crate::presentation::get().expect("Presentation precedes native integration");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let weak = Rc::downgrade(&bridge);
    let presentation = Rc::downgrade(&p);
    spawn(async move {
        let mut installed = false;
        let mut projection = Projection::default();
        let mut deadline = None;
        loop {
            // A single wakeup at the next line boundary, never frame/position polling.
            let wake = async {
                match deadline {
                    Some(at) => timer_until(at).await,
                    None => std::future::pending::<()>().await,
                }
            };
            tokio::select! {
                value=rx.recv()=>{if value.is_none(){return;} while rx.try_recv().is_ok() {}},
                _=wake=>{},
            }
            if !ready.get_untracked() {
                continue;
            }
            if !installed {
                installed = blocking(|| menubar_api::install("Haiku".into()))
                    .await
                    .unwrap_or(false);
                if !installed {
                    return;
                }
            }
            let (Some(bridge), Some(p)) = (weak.upgrade(), presentation.upgrade()) else {
                return;
            };
            let state = bridge.controller.state();
            let position = bridge.controller.position_snapshot();
            let track = p.track.get_untracked();
            let lyrics = p.lyrics.get_untracked();
            let settings = bridge.settings.get_untracked();
            let (view, next) = project(
                &state,
                track.as_ref(),
                &lyrics,
                &settings,
                &bridge.collections,
                position,
            );
            deadline =
                next.map(|secs| nsh_core::time::now() + Duration::from_secs_f64(secs.max(0.001)));
            let changes = projection.update(view.clone());
            let mut updates = vec![];
            if changes.highlight {
                updates.push(Update::Highlight(view.highlight));
            }
            if changes.visible {
                updates.push(Update::LyricsVisible(view.visible));
            }
            if changes.line {
                updates.push(Update::Line {
                    text: view.line.text,
                    words: view.line.words,
                    start: view.line.start,
                    position,
                    paused: !view.playing,
                    delay: view.line.delay,
                });
            }
            if changes.progress {
                updates.push(Update::Progress(position, !view.playing));
            }
            if changes.playing {
                updates.push(Update::Playing(view.playing));
            }
            if changes.favorite {
                updates.push(Update::Favorite(view.favorite));
            }
            if changes.menu {
                updates.push(Update::Menu(view.menu));
            }
            if !updates.is_empty() {
                let _ = blocking(move || {
                    menubar_api::apply(updates);
                    #[cfg(debug_assertions)]
                    if let Some(snapshot) = menubar_api::snapshot() {
                        log::info!("Menubar snapshot {snapshot}");
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
