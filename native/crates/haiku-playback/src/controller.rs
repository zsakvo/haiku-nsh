use crate::{
    AudioBackend, AudioEvent, AudioEventKind, Clock, Mode, Progress, Queue, TrackResolver,
};
use haiku_model::{Error, ErrorCode, Track};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    #[default]
    Idle,
    Loading,
    Playing,
    Paused,
    Error,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sleep {
    pub remaining_ms: Option<u64>,
    pub after_track: bool,
}
/// Low frequency snapshot. Position and buffered fraction intentionally live elsewhere.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    pub current: Option<Track>,
    /// The queue's actual planned successor, including shuffle/repeat-one.
    #[serde(skip)]
    pub next_index: Option<i32>,
    pub tracks: Arc<Vec<Track>>,
    pub index: i32,
    pub queue_version: u64,
    pub generation: u64,
    pub timeline_revision: u64,
    pub status: Status,
    pub buffering: bool,
    pub mode: Mode,
    pub volume: f64,
    pub sleep: Sleep,
    pub error: Option<Error>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub tracks: Vec<Track>,
    pub index: i32,
    pub mode: Mode,
    pub volume: f64,
    pub position: f64,
    #[serde(default)]
    pub session_tag: Option<String>,
}
#[derive(Clone, Debug)]
pub enum Command {
    Play(Vec<Track>, i32),
    Toggle,
    Pause,
    Resume,
    Stop,
    Next,
    Previous,
    Seek(f64),
    SetVolume(f64),
    SetMode(Mode),
    Append(Vec<Track>),
    PlayNext(Track),
    ReplaceAfterCurrent(Vec<Track>),
    Remove(i32),
    Clear,
    Move(i32, i32),
    Jump(i32),
    SetSleep(u32),
    SleepAfterTrack,
    ExtendSleep,
    CancelSleep,
    Tick,
    Flush,
    Shutdown,
}
struct Request {
    command: Command,
    reply: oneshot::Sender<Result<(), Error>>,
}
enum Completion {
    Open(u64, Result<(), Error>),
    Prepare(u64, Result<(), Error>),
}
#[derive(Clone)]
pub struct Controller {
    commands: mpsc::UnboundedSender<Request>,
    state: watch::Receiver<PlayerState>,
    frozen: Arc<Mutex<Progress>>,
    backend: Arc<dyn AudioBackend>,
}
impl Controller {
    /// Call from a Tokio worker. Initialization reads disk but never starts audio.
    pub fn start(
        backend: Arc<dyn AudioBackend>,
        resolver: Arc<dyn TrackResolver>,
        clock: Arc<dyn Clock>,
        events: mpsc::UnboundedReceiver<AudioEvent>,
        session_path: Option<PathBuf>,
    ) -> Result<Self, Error> {
        let mut queue = Queue::default();
        let mut position = 0.0;
        let mut volume = 1.0;
        if let Some(path) = &session_path {
            if let Some(session) = haiku_store::json_file::read::<Session>(path)
                .map_err(|e| Error::new(ErrorCode::CorruptData, e.to_string()))?
            {
                if session.session_tag.is_none() {
                    queue.replace(
                        session.tracks.into_iter().take(500).collect(),
                        session.index,
                    );
                    queue.set_mode(session.mode);
                    position = finite(session.position, 0.0).max(0.0);
                    if let Some(duration) = queue
                        .current()
                        .and_then(|t| t.duration)
                        .filter(|d| *d > 0.0)
                    {
                        position = position.min(duration);
                    }
                    volume = finite(session.volume, 1.0).clamp(0.0, 1.0);
                }
            }
        }
        let initial = PlayerState {
            current: queue.current().cloned(),
            next_index: queue.peek_next(true),
            tracks: Arc::new(queue.tracks.clone()),
            index: queue.index,
            queue_version: 0,
            generation: 0,
            timeline_revision: 0,
            status: if queue.current().is_some() {
                Status::Paused
            } else {
                Status::Idle
            },
            buffering: false,
            mode: queue.mode,
            volume,
            sleep: Sleep::default(),
            error: None,
        };
        let (states, state) = watch::channel(initial.clone());
        let buffered = queue
            .current()
            .and_then(|track| track.local_path.as_ref())
            .and_then(|path| std::fs::metadata(path).ok())
            .filter(|file| file.is_file() && file.len() > 0)
            .map(|_| 1.0)
            .unwrap_or(0.0);
        let frozen = Arc::new(Mutex::new(Progress {
            position,
            buffered,
            ..Progress::default()
        }));
        let (commands, requests) = mpsc::unbounded_channel();
        let (completed, completions) = mpsc::unbounded_channel();
        let actor = Actor {
            queue,
            state: initial,
            states,
            backend: backend.clone(),
            resolver,
            clock,
            frozen: frozen.clone(),
            session_path,
            last_save: 0,
            dirty: false,
            sequence: 0,
            opened: false,
            open_task: None,
            prepare_task: None,
            prepared: None,
            completed,
            pending: vec![],
            finished: None,
            sleep_deadline: None,
            failed: 0,
        };
        tokio::spawn(actor.run(requests, events, completions));
        Ok(Self {
            commands,
            state,
            frozen,
            backend,
        })
    }
    pub async fn execute(&self, command: Command) -> Result<(), Error> {
        let (reply, result) = oneshot::channel();
        self.commands
            .send(Request { command, reply })
            .map_err(|_| cancelled())?;
        result.await.map_err(|_| cancelled())?
    }
    pub fn shutdown(&self) {
        let (reply, _) = oneshot::channel();
        let _ = self.commands.send(Request {
            command: Command::Shutdown,
            reply,
        });
    }
    pub fn state(&self) -> PlayerState {
        self.state.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<PlayerState> {
        self.state.clone()
    }
    /// Rust consumers read the audio thread counters directly, never a JS mirror.
    pub fn progress(&self) -> Progress {
        let state = self.state.borrow();
        let audio = self.backend.progress();
        if state.status == Status::Playing && audio.generation == state.generation {
            audio
        } else {
            *self.frozen.lock().unwrap()
        }
    }
}
fn cancelled() -> Error {
    Error::new(ErrorCode::Cancelled, "Playback service closed")
}
fn finite(n: f64, fallback: f64) -> f64 {
    if n.is_finite() { n } else { fallback }
}
struct Prepared {
    index: i32,
    generation: u64,
    key: String,
}
struct Actor {
    queue: Queue,
    state: PlayerState,
    states: watch::Sender<PlayerState>,
    backend: Arc<dyn AudioBackend>,
    resolver: Arc<dyn TrackResolver>,
    clock: Arc<dyn Clock>,
    frozen: Arc<Mutex<Progress>>,
    session_path: Option<PathBuf>,
    last_save: u64,
    dirty: bool,
    sequence: u64,
    opened: bool,
    open_task: Option<JoinHandle<()>>,
    prepare_task: Option<JoinHandle<()>>,
    prepared: Option<Prepared>,
    completed: mpsc::UnboundedSender<Completion>,
    pending: Vec<AudioEvent>,
    finished: Option<u64>,
    sleep_deadline: Option<u64>,
    failed: usize,
}
impl Actor {
    async fn run(
        mut self,
        mut requests: mpsc::UnboundedReceiver<Request>,
        mut events: mpsc::UnboundedReceiver<AudioEvent>,
        mut completions: mpsc::UnboundedReceiver<Completion>,
    ) {
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        loop {
            tokio::select! {
                request = requests.recv() => match request {
                    Some(request) => {
                        let shutdown = matches!(request.command, Command::Shutdown);
                        let result = self.command(request.command);
                        let _ = request.reply.send(result);
                        if shutdown { break; }
                    }
                    None => { let _ = self.persist(true); break; }
                },
                Some(event) = events.recv() => self.event(event),
                Some(done) = completions.recv() => self.completion(done),
                _ = tick.tick() => { if let Err(e) = self.tick() { self.state.error = Some(e); self.publish(false); } },
            }
        }
        self.cancel_open();
        self.invalidate_prepared();
        self.backend.stop();
    }
    fn generation(&mut self) -> u64 {
        self.sequence = self.sequence.checked_add(1).expect("generation exhausted");
        self.sequence
    }
    fn publish(&mut self, queue_changed: bool) {
        if queue_changed {
            self.state.queue_version += 1;
            self.state.tracks = Arc::new(self.queue.tracks.clone());
        }
        self.state.current = self.queue.current().cloned();
        self.state.next_index = self.queue.peek_next(true);
        self.state.index = self.queue.index;
        self.state.mode = self.queue.mode;
        if *self.states.borrow() != self.state {
            self.states.send_replace(self.state.clone());
        }
    }
    fn sample(&mut self) {
        let audio = self.backend.progress();
        if self.state.status == Status::Playing && audio.generation == self.state.generation {
            let mut frozen = self.frozen.lock().unwrap();
            *frozen = Progress {
                position: finite(audio.position, frozen.position).max(0.0),
                ..audio
            };
            self.dirty = true;
        }
    }
    fn position(&self) -> f64 {
        self.frozen.lock().unwrap().position
    }
    fn set_position(&self, position: f64) {
        *self.frozen.lock().unwrap() = Progress {
            generation: self.state.generation,
            position,
            ..Progress::default()
        };
    }
    fn persist(&mut self, immediate: bool) -> Result<(), Error> {
        self.dirty = true;
        let now = self.clock.now_ms();
        if !immediate && now.saturating_sub(self.last_save) < 5_000 {
            return Ok(());
        }
        if let Some(path) = &self.session_path {
            let start = (self.queue.index.max(0) as usize).saturating_sub(499);
            let session = Session {
                tracks: self
                    .queue
                    .tracks
                    .iter()
                    .skip(start)
                    .take(500)
                    .cloned()
                    .collect(),
                index: if self.queue.index < 0 {
                    -1
                } else {
                    self.queue.index - start as i32
                },
                mode: self.queue.mode,
                volume: self.state.volume,
                position: self.position(),
                session_tag: None,
            };
            haiku_store::json_file::write(path, &session)
                .map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
        }
        self.last_save = now;
        self.dirty = false;
        Ok(())
    }
    fn cancel_open(&mut self) {
        if let Some(task) = self.open_task.take() {
            task.abort();
        }
        self.pending.clear();
    }
    fn invalidate_prepared(&mut self) {
        if let Some(task) = self.prepare_task.take() {
            task.abort();
        }
        self.prepared = None;
        self.backend.clear_next();
    }
    fn start_current(&mut self, position: f64) {
        self.cancel_open();
        self.invalidate_prepared();
        self.backend.stop();
        self.opened = false;
        if self.queue.current().is_none() {
            self.stop(false);
            return;
        }
        self.state.generation = self.generation();
        self.state.status = Status::Loading;
        self.state.buffering = false;
        self.state.error = None;
        self.set_position(position);
        self.publish(false);
        if let Err(e) = self.persist(true) {
            self.state.error = Some(e);
            self.publish(false);
        }
        let track = self.queue.current().unwrap().clone();
        let generation = self.state.generation;
        let backend = self.backend.clone();
        let resolver = self.resolver.clone();
        let completed = self.completed.clone();
        self.open_task = Some(tokio::spawn(async move {
            let result = async {
                let path = resolver.resolve(track).await?;
                backend.open(path, generation).await?;
                Ok(())
            }
            .await;
            let _ = completed.send(Completion::Open(generation, result));
        }));
    }
    fn prepare_next(&mut self) {
        self.invalidate_prepared();
        if !self.opened || self.state.status != Status::Playing || self.state.sleep.after_track {
            return;
        }
        let Some(index) = self.queue.peek_next(true) else {
            return;
        };
        let track = self.queue.tracks[index as usize].clone();
        let generation = self.generation();
        // Arm before resolving/opening: TrackChanged can precede the prepare response.
        self.prepared = Some(Prepared {
            index,
            generation,
            key: track.entity_ref().encode(),
        });
        let backend = self.backend.clone();
        let resolver = self.resolver.clone();
        let completed = self.completed.clone();
        self.prepare_task = Some(tokio::spawn(async move {
            let result = async {
                backend
                    .prepare(resolver.resolve(track).await?, generation)
                    .await
            }
            .await;
            let _ = completed.send(Completion::Prepare(generation, result));
        }));
    }
    fn stop(&mut self, clear: bool) {
        self.sample();
        self.cancel_open();
        self.invalidate_prepared();
        self.state.generation = self.generation();
        self.opened = false;
        if clear {
            self.queue.clear();
        }
        self.state.status = Status::Idle;
        self.state.buffering = false;
        self.state.error = None;
        self.set_position(0.0);
        self.publish(clear);
        self.backend.stop();
    }
    fn pause(&mut self) {
        if self.queue.current().is_none() {
            return;
        }
        self.sample();
        if self.state.status == Status::Loading {
            self.cancel_open();
            self.state.generation = self.generation();
            self.opened = false;
        }
        self.state.status = Status::Paused;
        self.state.buffering = false;
        self.publish(false); // Intent is visible before the backend command.
        self.invalidate_prepared();
        if self.opened {
            self.backend.pause();
        } else {
            self.backend.stop();
        }
    }
    fn resume(&mut self) {
        if self.queue.current().is_none() || self.state.status == Status::Playing {
            return;
        }
        if self.opened && self.state.status == Status::Paused {
            self.state.status = Status::Playing;
            self.state.error = None;
            self.publish(false);
            self.backend.resume();
            self.prepare_next();
        } else {
            self.start_current(self.position());
        }
    }
    fn next(&mut self, natural: bool) {
        if natural && self.state.sleep.after_track {
            self.pause();
            self.backend.stop();
            self.opened = false;
            self.state.generation = self.generation();
            self.set_position(0.0);
            self.state.sleep = Sleep::default();
            self.sleep_deadline = None;
            self.publish(false);
        } else if self.queue.next(natural).is_some() {
            self.start_current(0.0);
        } else {
            self.stop(false);
        }
    }
    fn fail(&mut self, error: Error) {
        self.cancel_open();
        self.invalidate_prepared();
        self.backend.stop();
        self.opened = false;
        self.state.status = Status::Error;
        self.state.buffering = false;
        self.state.error = Some(error);
        self.set_position(0.0);
        self.publish(false);
        // A bounded forward-only traversal, independent of repeat/shuffle.
        self.failed += 1;
        if self.failed < self.queue.tracks.len().min(20)
            && self.queue.index + 1 < self.queue.tracks.len() as i32
        {
            let _ = self.queue.jump(self.queue.index + 1);
            self.start_current(0.0);
        }
        if let Err(e) = self.persist(true) {
            self.state.error = Some(e);
            self.publish(false);
        }
    }
    fn completion(&mut self, done: Completion) {
        match done {
            Completion::Open(generation, result)
                if generation == self.state.generation && self.state.status == Status::Loading =>
            {
                self.open_task = None;
                match result {
                    Ok(()) => {
                        self.backend.set_volume(self.state.volume);
                        if self.position() > 0.0 {
                            self.backend.seek(self.position());
                        }
                        self.opened = true;
                        self.state.status = Status::Playing;
                        self.publish(false);
                        let pending = std::mem::take(&mut self.pending);
                        if pending.is_empty() {
                            self.prepare_next();
                        } else {
                            for event in pending {
                                self.event(event);
                            }
                        }
                    }
                    Err(e) => self.fail(e),
                }
            }
            Completion::Prepare(generation, Err(_))
                if self
                    .prepared
                    .as_ref()
                    .is_some_and(|p| p.generation == generation) =>
            {
                self.prepared = None;
                self.prepare_task = None;
            }
            _ => {}
        }
    }
    fn event(&mut self, event: AudioEvent) {
        if matches!(event.kind, AudioEventKind::TrackChanged) {
            if self
                .prepared
                .as_ref()
                .is_some_and(|p| p.generation == event.generation)
            {
                let prepared = self.prepared.take().unwrap();
                self.queue.next(true);
                if self.queue.index != prepared.index
                    || self
                        .queue
                        .current()
                        .map(|t| t.entity_ref().encode())
                        .as_deref()
                        != Some(&prepared.key)
                {
                    self.start_current(0.0);
                    return;
                }
                self.prepare_task = None;
                self.state.generation = prepared.generation;
                self.state.status = Status::Playing;
                self.state.error = None;
                self.state.buffering = false;
                self.failed = 0;
                self.set_position(0.0);
                self.publish(false);
                if let Err(e) = self.persist(true) {
                    self.state.error = Some(e);
                    self.publish(false);
                }
                self.prepare_next();
                return;
            }
            // The audio thread crossed a boundary just before ClearNext reached it.
            if event.generation > self.state.generation {
                if self.state.status == Status::Playing {
                    self.start_current(self.position());
                } else {
                    self.backend.stop();
                }
            }
            return;
        }
        if event.generation != self.state.generation {
            return;
        }
        if self.state.status == Status::Loading
            && matches!(
                event.kind,
                AudioEventKind::Finished | AudioEventKind::Error(_)
            )
        {
            self.pending.push(event);
            return;
        }
        match event.kind {
            AudioEventKind::Finished
                if self.state.status == Status::Playing
                    && self.finished != Some(event.generation) =>
            {
                self.finished = Some(event.generation);
                self.next(true);
                if let Err(e) = self.persist(true) {
                    self.state.error = Some(e);
                    self.publish(false);
                }
            }
            AudioEventKind::Error(error) if self.state.status == Status::Playing => {
                self.fail(error)
            }
            AudioEventKind::Buffering(value) if self.state.status == Status::Playing => {
                self.state.buffering = value;
                self.publish(false);
            }
            _ => {}
        }
    }
    fn tick(&mut self) -> Result<(), Error> {
        self.sample();
        if let Some(deadline) = self.sleep_deadline {
            let remaining = deadline.saturating_sub(self.clock.now_ms());
            if remaining == 0 {
                self.sleep_deadline = None;
                self.state.sleep = Sleep::default();
                self.pause();
                self.persist(true)?;
            } else {
                // Display seconds, avoiding a low-frequency state emission every 100 ms.
                self.state.sleep.remaining_ms = Some(remaining.div_ceil(1000) * 1000);
                self.publish(false);
            }
        }
        if self.dirty && self.clock.now_ms().saturating_sub(self.last_save) >= 5_000 {
            self.persist(false)?;
        }
        Ok(())
    }
    fn command(&mut self, command: Command) -> Result<(), Error> {
        let mut immediate = true;
        match command {
            Command::Play(tracks, index) => {
                self.failed = 0;
                self.queue.replace(tracks, index);
                self.publish(true);
                self.start_current(0.0);
            }
            Command::Toggle => {
                if matches!(self.state.status, Status::Playing | Status::Loading) {
                    self.pause();
                } else {
                    self.resume();
                }
            }
            Command::Pause => self.pause(),
            Command::Resume => self.resume(),
            Command::Stop => self.stop(false),
            Command::Clear => self.stop(true),
            Command::Next => {
                self.failed = 0;
                self.next(false);
            }
            Command::Previous => {
                self.failed = 0;
                if self.queue.index == 0 && self.queue.mode == Mode::Sequential {
                    return self.command(Command::Seek(0.0));
                }
                if self.queue.previous().is_some() {
                    self.start_current(0.0);
                }
            }
            Command::Seek(seconds) => {
                if !seconds.is_finite() || self.queue.current().is_none() {
                    return Ok(());
                }
                let duration = self
                    .queue
                    .current()
                    .and_then(|t| t.duration)
                    .filter(|d| *d > 0.0)
                    .unwrap_or(f64::INFINITY);
                let position = seconds.clamp(0.0, duration);
                self.invalidate_prepared();
                self.state.timeline_revision += 1;
                self.set_position(position);
                self.publish(false);
                if self.state.status == Status::Loading {
                    self.start_current(position);
                } else if self.opened {
                    self.backend.seek(position);
                    self.prepare_next();
                }
            }
            Command::SetVolume(volume) => {
                if !volume.is_finite() {
                    return Ok(());
                }
                self.state.volume = volume.clamp(0.0, 1.0);
                self.publish(false);
                self.backend.set_volume(self.state.volume);
            }
            Command::SetMode(mode) => {
                self.queue.set_mode(mode);
                self.publish(false);
                self.prepare_next();
            }
            Command::Append(tracks) => {
                self.queue.append(tracks);
                self.publish(true);
                self.prepare_next();
                immediate = false;
            }
            Command::PlayNext(track) => {
                self.queue.play_next(track);
                self.publish(true);
                self.prepare_next();
                immediate = false;
            }
            Command::ReplaceAfterCurrent(tracks) => {
                self.queue.replace_after_current(tracks);
                self.publish(true);
                self.prepare_next();
                immediate = false;
            }
            Command::Move(from, to) => {
                self.queue.move_item(from, to);
                self.publish(true);
                self.prepare_next();
                immediate = false;
            }
            Command::Jump(index) => {
                self.queue.jump(index)?;
                self.failed = 0;
                self.start_current(0.0);
            }
            Command::Remove(index) => {
                if index < 0 || index as usize >= self.queue.tracks.len() {
                    return Ok(());
                }
                let current = self.queue.index == index;
                self.queue.remove(index);
                self.publish(true);
                if self.queue.current().is_none() {
                    self.stop(false);
                } else if current {
                    self.failed = 0;
                    self.start_current(0.0);
                } else {
                    self.prepare_next();
                }
            }
            Command::SetSleep(minutes) => {
                if ![15, 30, 45, 60, 90].contains(&minutes) {
                    return Err(Error::new(
                        ErrorCode::InvalidArgument,
                        "Sleep minutes must be 15/30/45/60/90",
                    ));
                }
                let remaining = minutes as u64 * 60_000;
                self.sleep_deadline = Some(self.clock.now_ms().saturating_add(remaining));
                self.state.sleep = Sleep {
                    remaining_ms: Some(remaining),
                    after_track: false,
                };
                self.publish(false);
                self.prepare_next();
            }
            Command::ExtendSleep => {
                let deadline = self
                    .sleep_deadline
                    .unwrap_or(self.clock.now_ms())
                    .max(self.clock.now_ms())
                    .saturating_add(15 * 60_000);
                self.sleep_deadline = Some(deadline);
                self.state.sleep = Sleep {
                    remaining_ms: Some(deadline.saturating_sub(self.clock.now_ms())),
                    after_track: false,
                };
                self.publish(false);
                self.prepare_next();
            }
            Command::SleepAfterTrack => {
                self.sleep_deadline = None;
                self.state.sleep = Sleep {
                    remaining_ms: None,
                    after_track: true,
                };
                self.invalidate_prepared();
                self.publish(false);
            }
            Command::CancelSleep => {
                self.sleep_deadline = None;
                self.state.sleep = Sleep::default();
                self.publish(false);
                self.prepare_next();
            }
            Command::Tick => return self.tick(),
            Command::Flush => self.sample(),
            Command::Shutdown => {
                self.sample();
                self.pause();
            }
        }
        self.persist(immediate)
    }
}
