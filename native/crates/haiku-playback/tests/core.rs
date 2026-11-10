use futures::future::BoxFuture;
use haiku_model::{Error, ErrorCode, Track};
use haiku_playback::*;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, mpsc};

fn tracks(count: usize) -> Vec<Track> {
    (0..count)
        .map(|i| Track::local(format!("/{i}.flac")))
        .collect()
}
// Reference queue.test.ts, all four original cases.
#[test]
fn ref_queue_sequential_stops_repeat_one_only_changes_natural_finish() {
    let mut q = Queue::with_seed(1);
    q.replace(tracks(5), 4);
    assert_eq!(q.next(false), None);
    q.set_mode(Mode::RepeatOne);
    assert_eq!(q.next(true), Some(4));
    assert_eq!(q.next(false), Some(0));
    assert_eq!(q.previous(), Some(4));
}
#[test]
fn ref_queue_shuffle_has_no_repetition_and_keeps_actual_history() {
    let mut q = Queue::with_seed(4);
    q.replace(tracks(5), 0);
    q.set_mode(Mode::Shuffle);
    let mut heard = vec![q.index];
    for _ in 1..5 {
        heard.push(q.next(false).unwrap());
    }
    assert_eq!(
        heard.iter().collect::<std::collections::HashSet<_>>().len(),
        5
    );
    assert_eq!(q.previous(), Some(heard[3]));
    assert_eq!(q.next(false), Some(heard[4]));
    assert_ne!(q.next(false), Some(heard[4]));
}
#[test]
fn ref_queue_play_next_and_reorder_preserve_current_identity() {
    let mut q = Queue::with_seed(1);
    q.replace(tracks(5), 0);
    q.set_mode(Mode::Shuffle);
    q.play_next(Track::local("/next"));
    assert_eq!(q.next(false), Some(1));
    q.move_item(1, 4);
    assert_eq!(q.current().unwrap().id, "/next");
    q.remove(0);
    assert_eq!(q.current().unwrap().id, "/next");
}
#[test]
fn ref_queue_empty_and_duplicate_positions_remain_distinct() {
    let mut q = Queue::default();
    assert_eq!(q.next(false), None);
    let track = Track::local("/same");
    q.replace(vec![track.clone(), track], 0);
    q.move_item(0, 1);
    assert_eq!(q.index, 1);
    q.remove(1);
    q.remove(0);
    assert_eq!(q.index, -1);
    assert_eq!(q.previous(), None);
}
#[test]
fn queue_append_suffix_clamping_removal_and_invalid_operations() {
    let mut q = Queue::with_seed(9);
    assert!(q.jump(0).is_err());
    q.replace(tracks(4), 999);
    assert_eq!(q.index, 3);
    q.replace(tracks(4), -3);
    assert_eq!(q.index, 0);
    q.set_mode(Mode::Shuffle);
    q.next(false);
    let current = q.current().cloned();
    q.append(vec![Track::local("/added")]);
    assert_eq!(q.current(), current.as_ref());
    q.replace_after_current(vec![Track::local("/tail")]);
    assert_eq!(q.current(), current.as_ref());
    let before = q.tracks.clone();
    q.move_item(-1, 0);
    q.remove(999);
    assert_eq!(q.tracks, before);
    q.clear();
    q.append(tracks(2));
    assert_eq!(q.index, 0);
    q.remove(0);
    assert_eq!(q.index, 0);
    assert_eq!(q.current().unwrap().id, "/1.flac");
    q.set_mode(Mode::RepeatAll);
    assert_eq!(q.next(true), Some(0));
}

#[derive(Default)]
struct FakeClock(AtomicU64);
impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}
impl FakeClock {
    fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}
#[derive(Default)]
struct Fake {
    calls: Mutex<Vec<(String, u64)>>,
    settings: Mutex<haiku_store::settings::Settings>,
    opened_settings: Mutex<Vec<haiku_store::settings::Settings>>,
    progress: Mutex<Progress>,
    gate: Mutex<Option<Arc<Notify>>>,
    errors: Mutex<bool>,
    terminal_during_open: Mutex<Option<AudioEventKind>>,
    events: Mutex<Option<mpsc::UnboundedSender<AudioEvent>>>,
}
impl Fake {
    fn record(&self, method: &str, generation: u64) {
        self.calls.lock().unwrap().push((method.into(), generation));
    }
    fn count(&self, method: &str) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.0 == method)
            .count()
    }
    fn position(&self, generation: u64, position: f64) {
        *self.progress.lock().unwrap() = Progress {
            generation,
            position,
            buffered: 1.0,
            buffering: false,
        };
    }
}
impl AudioBackend for Fake {
    fn open(&self, path: PathBuf, generation: u64) -> BoxFuture<'_, Result<(), Error>> {
        self.record(&format!("open:{}", path.display()), generation);
        self.opened_settings
            .lock()
            .unwrap()
            .push(self.settings.lock().unwrap().clone());
        Box::pin(async move {
            let gate = self.gate.lock().unwrap().clone();
            if let Some(gate) = gate {
                gate.notified().await;
            }
            if *self.errors.lock().unwrap() {
                return Err(Error::new(ErrorCode::Io, "bad file"));
            }
            self.position(generation, 0.0);
            let terminal = self.terminal_during_open.lock().unwrap().take();
            if let Some(kind) = terminal {
                self.events
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .send(AudioEvent { generation, kind })
                    .unwrap();
                // Wait for the actor to observe a terminal before opening completes.
                tokio::task::yield_now().await;
            }
            Ok(())
        })
    }
    fn prepare(&self, path: PathBuf, generation: u64) -> BoxFuture<'_, Result<(), Error>> {
        self.record(&format!("prepare:{}", path.display()), generation);
        self.opened_settings
            .lock()
            .unwrap()
            .push(self.settings.lock().unwrap().clone());
        Box::pin(async { Ok(()) })
    }
    fn configure(&self, settings: haiku_store::settings::Settings) {
        *self.settings.lock().unwrap() = settings;
    }
    fn configuration(&self) -> haiku_store::settings::Settings {
        self.settings.lock().unwrap().clone()
    }
    fn clear_next(&self) {
        self.record("clear_next", 0);
    }
    fn pause(&self) {
        self.record("pause", 0);
    }
    fn resume(&self) {
        self.record("resume", 0);
    }
    fn stop(&self) {
        self.record("stop", 0);
    }
    fn seek(&self, seconds: f64) {
        self.record("seek", seconds as u64);
        self.progress.lock().unwrap().position = seconds;
    }
    fn set_volume(&self, volume: f64) {
        self.record("volume", (volume * 100.0) as u64);
    }
    fn progress(&self) -> Progress {
        *self.progress.lock().unwrap()
    }
}
struct Resolver;
impl TrackResolver for Resolver {
    fn resolve(&self, track: Track) -> BoxFuture<'_, Result<PathBuf, Error>> {
        Box::pin(async move { Ok(PathBuf::from(track.local_path.unwrap())) })
    }
}
struct Harness {
    player: Controller,
    audio: Arc<Fake>,
    clock: Arc<FakeClock>,
    events: mpsc::UnboundedSender<AudioEvent>,
}
impl Harness {
    fn new(path: Option<PathBuf>) -> Self {
        let audio = Arc::new(Fake::default());
        let clock = Arc::new(FakeClock::default());
        let (events, rx) = mpsc::unbounded_channel();
        *audio.events.lock().unwrap() = Some(events.clone());
        let player =
            Controller::start(audio.clone(), Arc::new(Resolver), clock.clone(), rx, path).unwrap();
        Self {
            player,
            audio,
            clock,
            events,
        }
    }
    async fn command(&self, command: Command) {
        self.player.execute(command).await.unwrap();
    }
    async fn state(&self, condition: impl Fn(&PlayerState) -> bool) -> PlayerState {
        let mut states = self.player.subscribe();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let state = states.borrow_and_update().clone();
                if condition(&state) {
                    return state;
                }
                states.changed().await.unwrap();
            }
        })
        .await
        .expect("expected core state")
    }
    async fn play(&self, count: usize) -> PlayerState {
        self.command(Command::Play(tracks(count), 0)).await;
        self.state(|s| s.status == Status::Playing).await
    }
    fn emit(&self, generation: u64, kind: AudioEventKind) {
        self.events.send(AudioEvent { generation, kind }).unwrap();
    }
    async fn settle(&self) {
        for _ in 0..12 {
            tokio::task::yield_now().await;
        }
        self.command(Command::Tick).await;
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.player.shutdown();
    }
}

#[tokio::test]
async fn ref_playback_restore_paused_resume_at_saved_position() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("session.json");
    haiku_store::json_file::write(
        &path,
        &Session {
            tracks: tracks(1),
            index: 0,
            mode: Mode::Sequential,
            volume: 0.5,
            position: 37.0,
            session_tag: None,
        },
    )
    .unwrap();
    let h = Harness::new(Some(path));
    assert_eq!(h.player.state().status, Status::Paused);
    assert_eq!(h.audio.calls.lock().unwrap().len(), 0);
    assert_eq!(h.player.progress().position, 37.0);
    h.command(Command::Resume).await;
    h.state(|s| s.status == Status::Playing).await;
    assert!(h.audio.calls.lock().unwrap().contains(&("seek".into(), 37)));
    assert_eq!(h.player.state().volume, 0.5);
}
#[tokio::test]
async fn ref_playback_pause_resume_keeps_generation_position_events() {
    let h = Harness::new(None);
    let state = h.play(1).await;
    h.command(Command::Pause).await;
    h.command(Command::Resume).await;
    h.audio.position(state.generation, 12.0);
    assert_eq!(h.player.progress().position, 12.0);
    assert_eq!(h.player.state().generation, state.generation);
}
#[tokio::test]
async fn ref_playback_sleep_after_current_intercepts_repeat() {
    let h = Harness::new(None);
    let state = h.play(2).await;
    h.command(Command::SetMode(Mode::RepeatAll)).await;
    h.command(Command::SleepAfterTrack).await;
    h.emit(state.generation, AudioEventKind::Finished);
    let state = h.state(|s| s.status == Status::Paused).await;
    assert_eq!(state.index, 0);
    assert!(!state.sleep.after_track);
    assert_eq!(h.player.progress().position, 0.0);
}
#[tokio::test]
async fn ref_playback_progress_preserves_queue_snapshot_and_seek_revises_timeline() {
    let h = Harness::new(None);
    let state = h.play(1).await;
    let snapshot = state.tracks.clone();
    h.audio.position(state.generation, 1.0);
    h.command(Command::Tick).await;
    assert!(Arc::ptr_eq(&snapshot, &h.player.state().tracks));
    h.command(Command::Append(tracks(1))).await;
    assert!(!Arc::ptr_eq(&snapshot, &h.player.state().tracks));
    assert_eq!(snapshot.len(), 1);
    h.command(Command::Seek(1.0)).await;
    h.command(Command::Seek(1.0)).await;
    assert_eq!(h.player.state().timeline_revision, 2);
}
#[tokio::test]
async fn ref_playback_gapless_handoff_accepts_prepared_generation_updates_queue() {
    let h = Harness::new(None);
    let state = h.play(2).await;
    h.settle().await;
    let prepared = h
        .audio
        .calls
        .lock()
        .unwrap()
        .iter()
        .find(|c| c.0 == "prepare:/1.flac")
        .cloned()
        .unwrap();
    assert!(prepared.1 > state.generation);
    h.emit(prepared.1, AudioEventKind::TrackChanged);
    let state = h.state(|s| s.index == 1).await;
    assert_eq!(state.generation, prepared.1);
    assert_eq!(state.status, Status::Playing);
    // The old finished event must not skip the newly audible stream.
    h.emit(state.generation - 1, AudioEventKind::Finished);
    h.settle().await;
    assert_eq!(h.player.state().index, 1);
}
#[tokio::test]
async fn cancellation_stale_completion_and_short_finished_are_safe() {
    let h = Harness::new(None);
    let gate = Arc::new(Notify::new());
    *h.audio.gate.lock().unwrap() = Some(gate.clone());
    h.command(Command::Play(tracks(1), 0)).await;
    h.state(|s| s.status == Status::Loading).await;
    let old = h.player.state().generation;
    h.command(Command::Pause).await;
    assert_eq!(h.player.state().status, Status::Paused);
    *h.audio.gate.lock().unwrap() = None;
    gate.notify_one();
    h.emit(old, AudioEventKind::Finished);
    *h.audio.terminal_during_open.lock().unwrap() = Some(AudioEventKind::Finished);
    h.command(Command::Play(tracks(1), 0)).await;
    h.state(|s| s.status == Status::Idle).await;
    assert!(h.player.state().generation > old);
}
#[tokio::test]
async fn loading_open_is_cancelled_by_new_selection_and_stale_errors_are_ignored() {
    let h = Harness::new(None);
    let gate = Arc::new(Notify::new());
    *h.audio.gate.lock().unwrap() = Some(gate.clone());
    h.command(Command::Play(tracks(2), 0)).await;
    let old = h.player.state().generation;
    *h.audio.gate.lock().unwrap() = None;
    h.command(Command::Jump(1)).await;
    let current = h.state(|s| s.status == Status::Playing).await;
    gate.notify_one();
    h.emit(
        old,
        AudioEventKind::Error(Error::new(ErrorCode::Io, "obsolete")),
    );
    h.settle().await;
    assert_eq!(h.player.state().index, 1);
    assert_eq!(h.player.state().generation, current.generation);
}
#[tokio::test]
async fn queue_head_previous_seeks_zero_without_reopening() {
    let h = Harness::new(None);
    let state = h.play(2).await;
    h.audio.position(state.generation, 12.0);
    h.command(Command::Previous).await;
    assert_eq!(h.audio.count("open:/0.flac"), 1);
    assert_eq!(h.player.progress().position, 0.0);
}
#[tokio::test]
async fn natural_repeat_one_manual_next_and_sequential_end() {
    let h = Harness::new(None);
    let state = h.play(2).await;
    h.command(Command::SetMode(Mode::RepeatOne)).await;
    h.emit(state.generation, AudioEventKind::Finished);
    let repeated = h
        .state(|s| s.generation > state.generation && s.status == Status::Playing)
        .await;
    assert_eq!(repeated.index, 0);
    h.command(Command::Next).await;
    let last = h
        .state(|s| s.index == 1 && s.status == Status::Playing)
        .await;
    h.command(Command::SetMode(Mode::Sequential)).await;
    h.emit(last.generation, AudioEventKind::Finished);
    let end = h.state(|s| s.status == Status::Idle).await;
    assert_eq!(end.index, 1);
    assert_eq!(end.tracks.len(), 2);
}
#[tokio::test]
async fn deleting_current_opens_successor_clear_stops_and_keeps_mode() {
    let h = Harness::new(None);
    h.play(3).await;
    h.command(Command::Remove(0)).await;
    let state = h
        .state(|s| s.status == Status::Playing && s.tracks.len() == 2)
        .await;
    assert_eq!(state.current.unwrap().id, "/1.flac");
    h.command(Command::SetMode(Mode::RepeatAll)).await;
    h.command(Command::Clear).await;
    assert_eq!(h.player.state().index, -1);
    assert_eq!(h.player.state().status, Status::Idle);
    assert_eq!(h.player.state().mode, Mode::RepeatAll);
}
#[tokio::test]
async fn preloading_follows_each_mode_and_seek_or_queue_change_invalidates() {
    for mode in [
        Mode::Sequential,
        Mode::RepeatAll,
        Mode::RepeatOne,
        Mode::Shuffle,
    ] {
        let h = Harness::new(None);
        h.play(3).await;
        h.command(Command::SetMode(mode)).await;
        h.settle().await;
        let calls = h.audio.calls.lock().unwrap().clone();
        let previous = calls
            .iter()
            .rev()
            .find(|c| c.0.starts_with("prepare:"))
            .unwrap()
            .clone();
        if mode == Mode::RepeatOne {
            assert_eq!(previous.0, "prepare:/0.flac");
        }
        if matches!(mode, Mode::Sequential | Mode::RepeatAll) {
            assert_eq!(previous.0, "prepare:/1.flac");
        }
        h.command(Command::Seek(10.0)).await;
        h.settle().await;
        let next = h
            .audio
            .calls
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|c| c.0.starts_with("prepare:"))
            .cloned()
            .unwrap();
        assert!(next.1 > previous.1);
        h.command(Command::Append(vec![Track::local("/added")]))
            .await;
        h.settle().await;
        assert!(h.audio.count("clear_next") >= 4);
    }
}
#[tokio::test]
async fn invalidated_gapless_boundary_is_repaired_and_sleep_never_advances() {
    let h = Harness::new(None);
    h.play(2).await;
    h.settle().await;
    let prepared = h
        .audio
        .calls
        .lock()
        .unwrap()
        .iter()
        .find(|c| c.0.starts_with("prepare:"))
        .unwrap()
        .1;
    h.command(Command::SleepAfterTrack).await;
    h.emit(prepared, AudioEventKind::TrackChanged);
    h.settle().await;
    assert_eq!(h.player.state().index, 0);
    h.command(Command::Pause).await;
    h.emit(prepared, AudioEventKind::TrackChanged);
    h.settle().await;
    assert_eq!(h.player.state().status, Status::Paused);
}
#[tokio::test]
async fn errors_skip_forward_with_bound_and_never_wrap_even_in_repeat() {
    let h = Harness::new(None);
    *h.audio.errors.lock().unwrap() = true;
    h.command(Command::SetMode(Mode::RepeatAll)).await;
    h.command(Command::Play(tracks(25), 5)).await;
    let end = h
        .state(|s| s.status == Status::Error && s.index == 24)
        .await;
    assert_eq!(end.index, 24);
    assert_eq!(h.audio.count("open:/0.flac"), 0);
    assert_eq!(
        h.audio
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.0.starts_with("open:"))
            .count(),
        20
    );
}
#[tokio::test]
async fn sleep_presets_extension_cancel_and_deadline_really_pause() {
    let h = Harness::new(None);
    h.play(1).await;
    for minutes in [15, 30, 45, 60, 90] {
        h.command(Command::SetSleep(minutes)).await;
        assert_eq!(
            h.player.state().sleep.remaining_ms,
            Some(minutes as u64 * 60_000)
        );
    }
    assert!(h.player.execute(Command::SetSleep(16)).await.is_err());
    h.command(Command::SetSleep(15)).await;
    h.clock.advance(60_000);
    h.command(Command::Tick).await;
    assert_eq!(h.player.state().sleep.remaining_ms, Some(14 * 60_000));
    h.command(Command::ExtendSleep).await;
    assert_eq!(h.player.state().sleep.remaining_ms, Some(29 * 60_000));
    h.command(Command::CancelSleep).await;
    assert_eq!(h.player.state().sleep, Sleep::default());
    h.command(Command::ExtendSleep).await;
    h.clock.advance(15 * 60_000);
    h.command(Command::Tick).await;
    assert_eq!(h.player.state().status, Status::Paused);
    assert!(h.audio.count("pause") > 0);
    assert_eq!(h.player.state().sleep, Sleep::default());
}
#[tokio::test]
async fn session_atomic_write_cap_throttle_critical_flush_and_radio_restore() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("session.json");
    let h = Harness::new(Some(path.clone()));
    h.command(Command::Append(tracks(510))).await;
    assert!(!path.exists());
    h.clock.advance(4_999);
    h.command(Command::Tick).await;
    assert!(!path.exists());
    h.clock.advance(1);
    h.command(Command::Tick).await;
    let session: Session = haiku_store::json_file::read(&path).unwrap().unwrap();
    assert_eq!(session.tracks.len(), 500);
    h.command(Command::SetVolume(0.7)).await;
    let session: Session = haiku_store::json_file::read(&path).unwrap().unwrap();
    assert_eq!(session.volume, 0.7);
    h.command(Command::Seek(17.0)).await;
    assert_eq!(
        haiku_store::json_file::read::<Session>(&path)
            .unwrap()
            .unwrap()
            .position,
        17.0
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    h.command(Command::Shutdown).await;
    let restored = Harness::new(Some(path.clone()));
    assert_eq!(restored.player.state().status, Status::Paused);
    assert_eq!(restored.player.progress().position, 17.0);
    restored.command(Command::Shutdown).await;
    let mut radio = session;
    radio.session_tag = Some("radio".into());
    haiku_store::json_file::write(&path, &radio).unwrap();
    let restored_radio = Harness::new(Some(path));
    assert_eq!(restored_radio.player.state().status, Status::Idle);
    assert!(restored_radio.player.state().tracks.is_empty());
}
#[tokio::test]
async fn local_resolver_rejects_missing_empty_directory_and_remote_without_fallback() {
    let root = tempfile::tempdir().unwrap();
    let empty = root.path().join("empty");
    std::fs::write(&empty, "").unwrap();
    for path in [root.path().join("missing"), empty, root.path().into()] {
        assert!(
            LocalResolver
                .resolve(Track::local(path.to_string_lossy()))
                .await
                .is_err()
        );
    }
    let mut remote = Track::local("/x");
    remote.local_path = None;
    assert_eq!(
        LocalResolver.resolve(remote).await.unwrap_err().code,
        ErrorCode::CapabilityUnsupported
    );
    let file = root.path().join("valid");
    std::fs::write(&file, "audio").unwrap();
    assert_eq!(
        LocalResolver
            .resolve(Track::local(file.to_string_lossy()))
            .await
            .unwrap(),
        file
    );
}
#[tokio::test]
async fn paused_progress_is_frozen_position_not_in_low_frequency_state() {
    let h = Harness::new(None);
    let state = h.play(1).await;
    let mut changes = h.player.subscribe();
    changes.borrow_and_update();
    h.audio.position(state.generation, 4.0);
    h.command(Command::Tick).await;
    assert!(!changes.has_changed().unwrap());
    assert_eq!(h.player.progress().position, 4.0);
    h.command(Command::Pause).await;
    h.audio.position(state.generation, 90.0);
    assert_eq!(h.player.progress().position, 4.0);
    let encoded = serde_json::to_value(h.player.state()).unwrap();
    assert!(encoded.get("position").is_none());
    assert!(encoded.get("buffered").is_none());
    h.command(Command::SetVolume(f64::NAN)).await;
    assert_eq!(h.player.state().volume, 1.0);
    h.command(Command::SetVolume(2.0)).await;
    assert_eq!(h.player.state().volume, 1.0);
}

#[tokio::test]
async fn effects_configuration_survives_controller_next_and_preload() {
    let h = Harness::new(None);
    let mut s = haiku_store::settings::Settings::default();
    s.audio_effects.enabled = true;
    s.audio_effects.band_gains_db = [3.; 10];
    s.audio_effects.balance = -0.4;
    s.audio_effects.stereo_width = 1.4;
    h.command(Command::Configure(s.clone())).await;
    h.play(3).await;
    h.settle().await;
    h.command(Command::Next).await;
    h.state(|p| p.index == 1 && p.status == Status::Playing)
        .await;
    h.settle().await;
    assert_eq!(h.player.configuration(), s);
    let opens = h.audio.opened_settings.lock().unwrap();
    assert!(opens.len() >= 3);
    assert!(opens.iter().all(|actual| actual == &s));
}
