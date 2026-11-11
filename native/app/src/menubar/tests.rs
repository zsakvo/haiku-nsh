use super::*;
use futures::future::BoxFuture;
use haiku_model::Error;
use haiku_playback::{AudioBackend, Progress, SystemClock, TrackResolver};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Default)]
struct Fake {
    progress: Mutex<Progress>,
    calls: Mutex<Vec<&'static str>>,
}
impl Fake {
    fn record(&self, s: &'static str) {
        self.calls.lock().unwrap().push(s);
    }
}
impl AudioBackend for Fake {
    fn open(&self, _: PathBuf, generation: u64) -> BoxFuture<'_, Result<(), Error>> {
        self.record("open");
        *self.progress.lock().unwrap() = Progress {
            generation,
            ..Default::default()
        };
        Box::pin(async { Ok(()) })
    }
    fn prepare(&self, _: PathBuf, _: u64) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async { Ok(()) })
    }
    fn clear_next(&self) {}
    fn pause(&self) {
        self.record("pause");
    }
    fn resume(&self) {
        self.record("resume");
    }
    fn stop(&self) {}
    fn seek(&self, v: f64) {
        self.progress.lock().unwrap().position = v;
    }
    fn set_volume(&self, _: f64) {}
    fn progress(&self) -> Progress {
        *self.progress.lock().unwrap()
    }
}
struct Resolver;
impl TrackResolver for Resolver {
    fn resolve(&self, t: Track) -> BoxFuture<'_, Result<PathBuf, Error>> {
        Box::pin(async move { Ok(t.local_path.unwrap().into()) })
    }
}
fn controller() -> (Controller, Arc<Fake>) {
    let backend = Arc::new(Fake::default());
    let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
    (
        Controller::start(
            backend.clone(),
            Arc::new(Resolver),
            Arc::new(SystemClock::default()),
            rx,
            None,
        )
        .unwrap(),
        backend,
    )
}
async fn playing(c: &Controller, index: i32) {
    let mut rx = c.subscribe();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let s = rx.borrow_and_update().clone();
            if s.status == Status::Playing && s.index == index {
                break;
            }
            rx.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn native_events_use_controller_and_collections_and_reject_stale_queue() {
    let root = tempfile::tempdir().unwrap();
    let collections = Collections::open(root.path()).unwrap();
    let list = collections.create("可写歌单").unwrap();
    let (c, audio) = controller();
    c.execute(Command::Play(
        vec![Track::local("/one.wav"), Track::local("/two.wav")],
        0,
    ))
    .await
    .unwrap();
    playing(&c, 0).await;
    let old = c.state();
    let (view, _) = project(
        &old,
        None,
        &Lyrics::default(),
        &Settings::default(),
        &collections,
        0.,
    );
    assert_eq!(view.menu[1].children.len(), 1); // excludes liked and plugin mirrors
    dispatch(&c, &collections, MenuBarEvent::Toggle)
        .await
        .unwrap();
    assert_eq!(c.state().status, Status::Paused);
    dispatch(&c, &collections, MenuBarEvent::Toggle)
        .await
        .unwrap();
    playing(&c, 0).await;
    dispatch(&c, &collections, MenuBarEvent::Next)
        .await
        .unwrap();
    playing(&c, 1).await;
    dispatch(&c, &collections, MenuBarEvent::Prev)
        .await
        .unwrap();
    playing(&c, 0).await;
    assert!(audio.calls.lock().unwrap().contains(&"pause"));
    assert!(audio.calls.lock().unwrap().contains(&"resume"));
    dispatch(&c, &collections, MenuBarEvent::ToggleFavorite)
        .await
        .unwrap();
    assert!(collections.is_liked(&old.current.as_ref().unwrap().entity_ref()));
    dispatch(
        &c,
        &collections,
        MenuBarEvent::MenuItem(view.menu[1].children[0].id.clone()),
    )
    .await
    .unwrap();
    assert_eq!(collections.tracks(&list).len(), 1);
    // A fresh queue item is accepted.
    let (fresh, _) = project(
        &c.state(),
        None,
        &Lyrics::default(),
        &Settings::default(),
        &collections,
        0.,
    );
    dispatch(
        &c,
        &collections,
        MenuBarEvent::MenuItem(fresh.menu[0].children[1].id.clone()),
    )
    .await
    .unwrap();
    playing(&c, 1).await;
    // Same identity at the same index, but mutated queue version: ignore, don't restart audio.
    c.execute(Command::Append(vec![Track::local("/three.wav")]))
        .await
        .unwrap();
    let generation = c.state().generation;
    dispatch(
        &c,
        &collections,
        MenuBarEvent::MenuItem(fresh.menu[0].children[0].id.clone()),
    )
    .await
    .unwrap();
    assert_eq!(c.state().generation, generation);
    assert_eq!(c.state().index, 1);
    // Current version with mismatched track identity is also ignored in the actor.
    c.execute(Command::JumpIfQueue {
        version: c.state().queue_version,
        index: 0,
        identity: "wrong".into(),
    })
    .await
    .unwrap();
    assert_eq!(c.state().index, 1);
    // A playlist selection made for the previous song cannot add the new current song.
    dispatch(
        &c,
        &collections,
        MenuBarEvent::MenuItem(view.menu[1].children[0].id.clone()),
    )
    .await
    .unwrap();
    assert_eq!(collections.tracks(&list).len(), 1);
    for (id, action) in [
        ("window:show", HostAction::ShowWindow),
        ("menubar:lyrics", HostAction::ToggleLyrics),
        ("app:quit", HostAction::Quit),
    ] {
        assert_eq!(
            dispatch(&c, &collections, MenuBarEvent::MenuItem(id.into()))
                .await
                .unwrap(),
            action
        );
    }
    c.execute(Command::Shutdown).await.unwrap();
}
#[tokio::test]
async fn projection_deduplicates_and_reanchors_only_line_pause_seek_changes() {
    use haiku_lyrics::{LyricLine, Word};
    let root = tempfile::tempdir().unwrap();
    let collections = Collections::open(root.path()).unwrap();
    let (c, _) = controller();
    let mut state = c.state();
    state.current = Some(Track::local("/one.wav"));
    state.tracks = Arc::new(vec![state.current.clone().unwrap()]);
    state.index = 0;
    state.status = Status::Playing;
    let lyrics = Lyrics {
        synced: true,
        lines: vec![
            LyricLine {
                at: 1.,
                end: 5.,
                text: "第一行".into(),
                words: vec![Word {
                    start: 1.,
                    end: 5.,
                    text: "第一行".into(),
                }],
                translation: None,
            },
            LyricLine {
                at: 5.,
                end: 9.,
                text: "第二行".into(),
                words: vec![],
                translation: None,
            },
        ],
    };
    let settings = Settings::default();
    let make = |s: &PlayerState, p| project(s, None, &lyrics, &settings, &collections, p);
    let (view, next) = make(&state, 2.);
    assert_eq!(next, Some(3.));
    assert_eq!(view.line.text, "第一行");
    let mut projection = Projection::default();
    assert!(projection.update(view.clone()).line);
    assert_eq!(projection.update(view), Changes::default());
    assert_eq!(projection.update(make(&state, 3.).0), Changes::default()); // position alone is not a push
    state.volume = 0.3;
    assert_eq!(projection.update(make(&state, 3.).0), Changes::default());
    state.status = Status::Paused;
    let (view, next) = make(&state, 3.);
    assert_eq!(next, None);
    let changed = projection.update(view);
    assert!(changed.progress && changed.playing);
    assert!(!changed.line);
    state.timeline_revision += 1;
    assert!(projection.update(make(&state, 4.).0).progress);
    state.status = Status::Playing;
    assert!(projection.update(make(&state, 4.).0).progress);
    assert!(projection.update(make(&state, 5.).0).line);
    assert_eq!(make(&state, 5.).0.line.start, 5.);
    c.execute(Command::Shutdown).await.unwrap();
}

#[tokio::test]
async fn desktop_toolbar_uses_same_controller_collections_and_returns_durable_actions() {
    use haiku_macos::desktop_lyrics::{DesktopLyricsEvent as E,geometry::Position};
    use crate::desktop_lyrics::{self,Action};
    let root=tempfile::tempdir().unwrap();let collections=Collections::open(root.path()).unwrap();
    let (c,audio)=controller();
    c.execute(Command::Play(vec![Track::local("/one.wav"),Track::local("/two.wav")],0)).await.unwrap();playing(&c,0).await;
    desktop_lyrics::dispatch(&c,&collections,E::Toggle).await.unwrap();assert_eq!(c.state().status,Status::Paused);
    desktop_lyrics::dispatch(&c,&collections,E::Toggle).await.unwrap();playing(&c,0).await;
    desktop_lyrics::dispatch(&c,&collections,E::Next).await.unwrap();playing(&c,1).await;
    desktop_lyrics::dispatch(&c,&collections,E::Prev).await.unwrap();playing(&c,0).await;
    desktop_lyrics::dispatch(&c,&collections,E::ToggleFavorite).await.unwrap();assert!(collections.is_liked(&c.state().current.unwrap().entity_ref()));
    assert!(audio.calls.lock().unwrap().contains(&"pause"));
    for (event,expected) in [(E::Lock,serde_json::json!({"desktopLyrics":{"locked":true}})),(E::Unlock,serde_json::json!({"desktopLyrics":{"locked":false}})),(E::Close,serde_json::json!({"desktopLyrics":{"visible":false}}))] {
        assert_eq!(desktop_lyrics::dispatch(&c,&collections,event).await.unwrap(),Action::Patch(expected));
    }
    assert_eq!(desktop_lyrics::dispatch(&c,&collections,E::Settings).await.unwrap(),Action::Settings);
    assert_eq!(desktop_lyrics::dispatch(&c,&collections,E::ShowApp).await.unwrap(),Action::ShowApp);
    let position=Position{center_x:500.,y:50.,width:600.};
    let event=serde_json::from_value(serde_json::to_value(E::Position(position)).unwrap()).unwrap();
    assert_eq!(desktop_lyrics::dispatch(&c,&collections,event).await.unwrap(),Action::Position(position));
    let file=root.path().join("haiku_state/desktop-lyrics-position.json");haiku_store::json_file::write(&file,&position).unwrap();
    assert_eq!(haiku_store::json_file::read::<Position>(&file).unwrap(),Some(position));
    let (a,_)=project(&c.state(),None,&Lyrics::default(),&Settings::default(),&collections,0.);
    let item=a.menu.iter().find(|i|i.id=="lyrics:show").unwrap();
    assert_eq!(dispatch(&c,&collections,MenuBarEvent::MenuItem(item.id.clone())).await.unwrap(),HostAction::ToggleDesktopLyrics);
    assert_eq!(dispatch(&c,&collections,MenuBarEvent::MenuItem("lyrics:lock".into())).await.unwrap(),HostAction::ToggleDesktopLock);
    c.execute(Command::Shutdown).await.unwrap();
}
#[tokio::test]
async fn desktop_projection_falls_back_to_metadata_then_follows_lyrics_and_pause() {
    use crate::desktop_lyrics;
    let root=tempfile::tempdir().unwrap();let collections=Collections::open(root.path()).unwrap();let (c,_)=controller();
    let mut s=c.state();let mut track=Track::local("/one.wav");track.title="歌名".into();track.artists=vec![haiku_model::Artist::Name("艺术家".into())];s.current=Some(track);s.status=Status::Playing;
    let mut d=haiku_store::desktop_lyrics::DesktopLyrics::default();d.visible=true;
    let (v,wake)=desktop_lyrics::project(&s,None,&Lyrics::default(),&d,&collections,0.);assert_eq!(wake,None);
    assert_eq!(v.current.unwrap().translation,"艺术家");
    let lyrics=haiku_lyrics::parse("[00:01]<00:01>第一<00:03>行<00:04>\n[00:05]第二行");
    let (v,wake)=desktop_lyrics::project(&s,None,&lyrics,&d,&collections,2.);assert_eq!(wake,Some(3.));
    assert_eq!(v.current.unwrap().text,"第一行");assert_eq!(v.next.unwrap().text,"第二行");
    s.status=Status::Paused;assert_eq!(desktop_lyrics::project(&s,None,&lyrics,&d,&collections,2.).1,None);
    d.visible=false;s.status=Status::Playing;assert_eq!(desktop_lyrics::project(&s,None,&lyrics,&d,&collections,2.).1,None);
    c.execute(Command::Shutdown).await.unwrap();
}
