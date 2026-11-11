//! Hold a local lyric read across playback/settings changes; no sleeps or filesystem races.
use super::*;
use futures::future::BoxFuture;
use haiku_lyrics::{
    providers::Text,
    service::{Options, Origin, Service},
};
use haiku_playback::Status;
use haiku_store::settings::Settings;
use tokio::sync::{Notify, watch};

struct DelayedLocal {
    entered: watch::Sender<String>,
    release: Arc<Notify>,
}
impl Origin for DelayedLocal {
    fn local(&self, track: Track) -> BoxFuture<'_, Result<Option<Text>, String>> {
        Box::pin(async move {
            self.entered.send_replace(track.id.clone());
            self.release.notified().await;
            Ok(Some(Text {
                text: format!("[00:10]{} middle", track.id),
                format: "lrc".into(),
                translation: None,
                instrumental: false,
            }))
        })
    }
    fn plugin(&self, _: Track) -> BoxFuture<'_, Result<Option<Text>, String>> {
        Box::pin(async { Ok(None) })
    }
}
#[derive(Clone)]
struct Controls {
    track: Signal<Option<Track>>,
    playback: Signal<(u64, Status)>,
    settings: Signal<Settings>,
    release: Arc<Notify>,
}
#[component]
fn PendingLyricsFixture() -> impl IntoElement {
    let pending = signal(String::new());
    let rendered = signal(String::new());
    let controls = stored(None::<Controls>);
    spawn(async move {
        let core = crate::core::Core::open_isolated().await.unwrap();
        let path = core.with(|c| std::path::PathBuf::from(c.data_dir()).join("controlled-lyrics"));
        let service = Arc::new(Service::new(path, Default::default()));
        let track = signal(Some(Track::local("Alpha")));
        let playback = signal((1, Status::Playing));
        let settings = signal(Settings::default());
        let release = Arc::new(Notify::new());
        let (entered, mut entries) = watch::channel(String::new());
        let origin = Arc::new(DelayedLocal {
            entered,
            release: release.clone(),
        });
        let mut events = service.subscribe();
        bind_lyrics(
            move || {
                let options: Options =
                    serde_json::from_value(serde_json::to_value(settings.get()).unwrap()).unwrap();
                Some((track.get(), playback.with(|p| p.0), options))
            },
            service,
            origin,
            || {},
        );
        controls.replace(Some(Controls {
            track,
            playback,
            settings,
            release,
        }));
        spawn(async move {
            while entries.changed().await.is_ok() {
                pending.set(format!("pending {}", entries.borrow_and_update().clone()));
            }
        });
        spawn(async move {
            while events.changed().await.is_ok() {
                let snapshot = events.borrow_and_update().clone();
                rendered.set(
                    snapshot
                        .lyrics
                        .map(|l| l.lines[0].text.clone())
                        .unwrap_or_default(),
                );
            }
        });
    });
    view()
        .flex_col()
        .child(text(move || pending.get()))
        .child(text(move || rendered.get()))
        .child(
            nsh_base::button()
                .child(text("Pause and change download settings"))
                .on_press(move || {
                    if let Some(c) = controls.get() {
                        c.playback.update(|p| p.1 = Status::Paused);
                        c.settings.update(|s| s.download_concurrency = 5);
                    }
                }),
        )
        .child(
            nsh_base::button()
                .child(text("Switch track"))
                .on_press(move || {
                    if let Some(c) = controls.get() {
                        c.track.set(Some(Track::local("Beta")));
                        c.playback.update(|p| p.0 += 1);
                    }
                }),
        )
        .child(
            nsh_base::button()
                .child(text("Finish local read"))
                .on_press(move || {
                    if let Some(c) = controls.get() {
                        c.release.notify_one();
                    }
                }),
        )
}
#[test]
fn pending_local_lyrics_survive_pause_and_download_settings_then_switch_tracks() {
    use nsh_engine::{EngineConfig, Root};
    use nsh_testing::Headless;
    let mut app = Headless::start(EngineConfig::new(Root::of::<PendingLyricsFixture>())).unwrap();
    app.wait_for("pending Alpha").unwrap();
    app.press("Pause and change download settings").unwrap();
    app.press("Finish local read").unwrap();
    app.wait_for("Alpha middle").unwrap();
    app.press("Switch track").unwrap();
    app.wait_for("pending Beta").unwrap();
    app.press("Finish local read").unwrap();
    app.wait_for("Beta middle").unwrap();
    assert!(!app.screen().texts().iter().any(|text| *text == "Alpha middle"));
}
