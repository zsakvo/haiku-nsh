//! AppKit producers only send. This module drains on NSH's engine thread.
use haiku_macos::{
    events::{self, NativeEvent},
    menubar_api::{self, MenubarEvent, MenubarEventKind},
    system::{self, MediaInfo},
};
use haiku_model::events::EventSink;
use std::{cell::RefCell, sync::mpsc::Receiver};

thread_local! {
    static INPUTS: RefCell<Option<(Receiver<NativeEvent>, Receiver<haiku_model::events::Event<MenubarEvent>>)>> = const { RefCell::new(None) };
}

pub fn install() -> Result<String, String> {
    let lifecycle = haiku_macos::lifecycle::install()?;
    INPUTS.with(|inputs| {
        if inputs.borrow().is_some() {
            return;
        }
        let media = events::subscribe();
        menubar_api::menubar_install("Haiku".into());
        let (tx, rx) = std::sync::mpsc::channel();
        menubar_api::menubar_events(EventSink::new(tx));
        menubar_api::menubar_set_text("M0-05 local fixture".into(), 0.0);
        *inputs.borrow_mut() = Some((media, rx));
    });
    Ok(lifecycle)
}

fn command(kind: &str) {
    eprintln!("S4 engine-thread command={kind}");
    match kind {
        "play" => super::audio::resume(),
        "pause" => super::audio::pause(),
        "toggle" => {
            if super::audio::playing() {
                super::audio::pause();
            } else {
                super::audio::resume();
            }
        }
        "next" => super::audio::next(),
        _ => {}
    }
}

pub fn tick() -> String {
    INPUTS.with(|inputs| {
        if let Some((media, menu)) = inputs.borrow().as_ref() {
            for event in media.try_iter() {
                if event.name == "media.event" {
                    command(event.data["kind"].as_str().unwrap_or(""));
                }
            }
            for event in menu.try_iter() {
                match event.data.kind {
                    MenubarEventKind::Toggle => command("toggle"),
                    MenubarEventKind::Next => command("next"),
                    _ => {}
                }
            }
        }
    });
    let playing = super::audio::playing();
    system::media_update(MediaInfo {
        title: "M0-05 local fixture".into(),
        // Acceptance uses the existing 35-second M0-04 local FLAC.
        duration: 35.0,
        position: super::audio::position(),
        playing,
        can_next: true,
        ..Default::default()
    });
    menubar_api::menubar_set_playing(playing);
    system::media_snapshot().to_string()
}
