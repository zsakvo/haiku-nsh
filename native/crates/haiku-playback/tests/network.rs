use futures::future::BoxFuture;
use haiku_model::{Error, ErrorCode, Track};
use haiku_playback::*;
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::mpsc;

#[derive(Default)]
struct Time(AtomicU64);
impl Clock for Time {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
    fn unix_seconds(&self) -> u64 {
        100 + self.now_ms() / 1000
    }
}
impl Time {
    fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}
fn remote(id: &str) -> Track {
    let mut t = Track::local(id);
    t.source = "fixture#雪:source".into();
    t.local_path = None;
    t.duration = Some(100.);
    t
}
fn candidate(url: &str, format: &str, expires_at: Option<u64>) -> Candidate {
    Candidate::Network {
        url: url.into(),
        headers: BTreeMap::new(),
        format: Some(format.into()),
        quality: Some("high".into()),
        expires_at,
    }
}
fn failure() -> Error {
    let mut e = Error::new(ErrorCode::PlaybackError, "network");
    e.retriable = true;
    e
}
struct Resolver {
    calls: AtomicU64,
    answers: Mutex<VecDeque<Result<Vec<Candidate>, Error>>>,
    qualities: Mutex<Vec<String>>,
}
impl Resolver {
    fn new(answers: Vec<Result<Vec<Candidate>, Error>>) -> Self {
        Self {
            calls: AtomicU64::new(0),
            answers: Mutex::new(answers.into()),
            qualities: Mutex::new(vec![]),
        }
    }
}
impl TrackResolver for Resolver {
    fn candidates(
        &self,
        _: Track,
        quality: String,
    ) -> BoxFuture<'_, Result<Vec<Candidate>, Error>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.qualities.lock().unwrap().push(quality);
        let value = self
            .answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Ok(vec![candidate("good", "flac", None)]));
        Box::pin(async move { value })
    }
}
#[derive(Default)]
struct Audio {
    calls: Mutex<Vec<(String, u64, bool)>>,
    fail: AtomicBool,
    progress: Mutex<Progress>,
}
impl Audio {
    fn count(&self) -> usize {
        self.calls.lock().unwrap().iter().filter(|c| !c.2).count()
    }
    fn position(&self, generation: u64, position: f64) {
        *self.progress.lock().unwrap() = Progress {
            generation,
            position,
            buffered: 0.5,
            buffering: false,
        };
    }
}
impl AudioBackend for Audio {
    fn open(&self, _: PathBuf, _: u64) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async { Ok(()) })
    }
    fn prepare(&self, _: PathBuf, _: u64) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async { Ok(()) })
    }
    fn open_candidate(
        &self,
        _: Track,
        c: Candidate,
        g: u64,
        prepare: bool,
        position: f64,
        _: Option<String>,
    ) -> BoxFuture<'_, Result<(), Error>> {
        let name = match c {
            Candidate::Local(_) => "local".into(),
            Candidate::Network { url, .. } => url,
        };
        self.calls.lock().unwrap().push((name.clone(), g, prepare));
        let bad = name == "bad" || self.fail.load(Ordering::SeqCst);
        Box::pin(async move {
            if bad {
                Err(failure())
            } else {
                if !prepare {
                    self.position(g, position);
                }
                Ok(())
            }
        })
    }
    fn clear_next(&self) {}
    fn pause(&self) {}
    fn resume(&self) {}
    fn stop(&self) {}
    fn seek(&self, p: f64) {
        self.progress.lock().unwrap().position = p;
    }
    fn set_volume(&self, _: f64) {}
    fn progress(&self) -> Progress {
        *self.progress.lock().unwrap()
    }
}
struct Harness {
    player: Controller,
    audio: Arc<Audio>,
    resolver: Arc<Resolver>,
    clock: Arc<Time>,
    events: mpsc::UnboundedSender<AudioEvent>,
}
impl Harness {
    fn new(answers: Vec<Result<Vec<Candidate>, Error>>) -> Self {
        let audio = Arc::new(Audio::default());
        let clock = Arc::new(Time::default());
        let resolver = Arc::new(Resolver::new(answers));
        let (events, rx) = mpsc::unbounded_channel();
        let player =
            Controller::start(audio.clone(), resolver.clone(), clock.clone(), rx, None).unwrap();
        Self {
            player,
            audio,
            resolver,
            clock,
            events,
        }
    }
    async fn command(&self, c: Command) {
        self.player.execute(c).await.unwrap();
    }
    async fn settle(&self) {
        for _ in 0..40 {
            tokio::task::yield_now().await;
        }
        self.command(Command::Tick).await;
    }
    async fn advance(&self, ms: u64) {
        self.clock.advance(ms);
        self.command(Command::Tick).await;
        self.settle().await;
    }
    async fn status(&self, status: Status) {
        let mut state = self.player.subscribe();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if state.borrow_and_update().status == status {
                    break;
                }
                state.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
    }
    async fn play(&self, id: &str) {
        self.command(Command::Play(vec![remote(id)], 0)).await;
        self.settle().await;
    }
    async fn error(&self) {
        self.events
            .send(AudioEvent {
                generation: self.player.state().generation,
                kind: AudioEventKind::Error(failure()),
            })
            .unwrap();
        self.settle().await;
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.player.shutdown();
    }
}

#[tokio::test]
async fn candidates_fallback_skip_unsupported_before_open_and_reuse_unexpired_addresses() {
    let h = Harness::new(vec![Ok(vec![
        candidate("opus", "opus", None),
        candidate("bad", "flac", None),
        candidate("good", "mp3", Some(200)),
    ])]);
    h.play("fallback").await;
    h.status(Status::Playing).await;
    assert_eq!(
        h.audio
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(|c| c.0.as_str())
            .collect::<Vec<_>>(),
        ["bad", "good"]
    );
    h.play("fallback").await;
    h.status(Status::Playing).await;
    assert_eq!(h.resolver.calls.load(Ordering::SeqCst), 1);
    assert_eq!(*h.resolver.qualities.lock().unwrap(), ["high"]);
}
#[tokio::test]
async fn expired_error_and_expiration_time_refetch_streams() {
    let h = Harness::new(vec![
        Err(Error::new(ErrorCode::PlaybackExpired, "Expired")),
        Ok(vec![candidate("fresh", "flac", Some(102))]),
        Ok(vec![candidate("renewed", "flac", Some(200))]),
    ]);
    h.play("expiry").await;
    h.status(Status::Playing).await;
    assert_eq!(h.resolver.calls.load(Ordering::SeqCst), 2);
    h.advance(2000).await;
    h.play("expiry").await;
    h.status(Status::Playing).await;
    assert_eq!(h.resolver.calls.load(Ordering::SeqCst), 3);
}
#[tokio::test]
async fn retries_have_exact_1_2_4_second_budget_and_are_cancelled_by_intent() {
    let h = Harness::new(vec![]);
    h.audio.fail.store(true, Ordering::SeqCst);
    h.play("retry").await;
    assert_eq!(h.audio.count(), 1);
    for (delay, expected) in [(1000, 2), (2000, 3), (4000, 4)] {
        h.advance(delay - 1).await;
        assert_eq!(h.audio.count(), expected - 1);
        h.advance(1).await;
        assert_eq!(h.audio.count(), expected);
    }
    h.status(Status::Error).await;
    h.advance(10_000).await;
    assert_eq!(h.audio.count(), 4);
    for command in [
        Command::Pause,
        Command::Stop,
        Command::Seek(12.),
        Command::Play(vec![], 0),
    ] {
        let h = Harness::new(vec![]);
        h.audio.fail.store(true, Ordering::SeqCst);
        h.play("cancel").await;
        h.command(command).await;
        h.advance(10_000).await;
        assert_eq!(h.audio.count(), 1, "cancelled retry must never reopen");
    }
}
#[tokio::test]
async fn thirty_seconds_of_real_progress_resets_budget_and_early_eof_recovers_position() {
    let h = Harness::new(vec![]);
    h.play("stability").await;
    h.status(Status::Playing).await;
    h.error().await;
    h.advance(1000).await;
    h.status(Status::Playing).await;
    let generation = h.player.state().generation;
    for second in 1..=31 {
        h.audio.position(generation, second as f64);
        h.advance(1000).await;
    }
    h.error().await;
    let count = h.audio.count();
    h.advance(999).await;
    assert_eq!(h.audio.count(), count);
    h.advance(1).await;
    h.status(Status::Playing).await;
    assert_eq!(h.audio.count(), count + 1);
    let generation = h.player.state().generation;
    h.audio.position(generation, 40.);
    h.events
        .send(AudioEvent {
            generation,
            kind: AudioEventKind::Finished,
        })
        .unwrap();
    h.settle().await;
    assert_eq!(h.player.state().status, Status::Loading);
    h.advance(2000).await;
    h.status(Status::Playing).await;
    assert_eq!(h.player.position_snapshot(), 40.);
    assert_eq!(
        h.resolver.calls.load(Ordering::SeqCst),
        1,
        "early EOF reuses an unexpired URL"
    );
}
#[tokio::test]
async fn complete_cache_is_quality_specific_and_local_failure_never_calls_streams() {
    use haiku_audio::media_cache as cache;
    use std::io::Write;
    let root = tempfile::tempdir().unwrap();
    cache::set_root(root.path().display().to_string()).unwrap();
    let track = remote("cache");
    let key = haiku_playback::backend::cache_key(&track, "high").unwrap();
    let (mut file, _, _) = cache::open_part_writer(&key).unwrap();
    file.write_all(b"audio").unwrap();
    drop(file);
    cache::finalize(&key, 5, Some(5), None);
    let h = Harness::new(vec![]);
    h.play("cache").await;
    h.status(Status::Playing).await;
    assert_eq!(h.resolver.calls.load(Ordering::SeqCst), 0);
    assert_eq!(h.audio.calls.lock().unwrap()[0].0, "local");
    assert_ne!(
        key,
        haiku_playback::backend::cache_key(&track, "lossless").unwrap()
    );
    assert!(matches!(
        cache::status(&haiku_playback::backend::cache_key(&track, "lossless").unwrap()),
        cache::CacheStatus::None
    ));
    let h = Harness::new(vec![]);
    h.command(Command::Play(
        vec![Track::local("/missing/fixture.flac")],
        0,
    ))
    .await;
    h.status(Status::Error).await;
    assert_eq!(h.resolver.calls.load(Ordering::SeqCst), 0);
}
