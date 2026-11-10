//! Shared serial JSON documents: bounded debounce, explicit exit barrier, corrupt-file recovery.
use serde::{Serialize, de::DeserializeOwned};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread::JoinHandle,
    time::Duration,
};
struct State<T> {
    value: T,
    revision: u64,
    dirty: bool,
    error: Option<String>,
}
struct Inner<T: Serialize> {
    state: Arc<Mutex<State<T>>>,
    path: PathBuf,
    wake: mpsc::Sender<bool>,
    worker: Mutex<Option<JoinHandle<()>>>,
    pub recovery: Option<String>,
}
impl<T: Serialize> Drop for Inner<T> {
    fn drop(&mut self) {
        let _ = self.wake.send(false);
        if let Some(worker) = self.worker.get_mut().unwrap().take() {
            let _ = worker.join();
        }
    }
}
pub(crate) struct Document<T: Serialize>(Arc<Inner<T>>);
impl<T: Serialize> Clone for Document<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
fn save<T: Serialize>(state: &Mutex<State<T>>, path: &Path) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    if !s.dirty {
        return Ok(());
    }
    let result = crate::json_file::write(path, &s.value).map_err(|e| e.to_string());
    s.error = result.as_ref().err().cloned();
    if result.is_ok() {
        s.dirty = false;
    }
    result
}
impl<T: Serialize + DeserializeOwned + Default + Clone + Send + 'static> Document<T> {
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let (value, recovery) = match crate::json_file::read(&path) {
            Ok(v) => (v.unwrap_or_default(), None),
            Err(e) if path.is_file() => {
                let backup = path.with_extension(format!("json.corrupt-{}", uuid::Uuid::new_v4()));
                std::fs::rename(&path, &backup).map_err(|e| e.to_string())?;
                (
                    T::default(),
                    Some(format!(
                        "{}: {e}; 已备份到 {}",
                        path.display(),
                        backup.display()
                    )),
                )
            }
            Err(e) => return Err(e.to_string()),
        };
        let state = Arc::new(Mutex::new(State {
            value,
            revision: 0,
            dirty: false,
            error: None,
        }));
        let (wake, receiver) = mpsc::channel();
        let (disk, target) = (state.clone(), path.clone());
        let worker = std::thread::spawn(move || {
            while let Ok(mut keep) = receiver.recv() {
                // A burst never postpones persistence for longer than 500 ms.
                let deadline = std::time::Instant::now() + Duration::from_millis(500);
                while keep {
                    match receiver
                        .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                    {
                        Ok(next) => keep = next,
                        Err(_) => break,
                    }
                }
                let _ = save(&disk, &target);
                if !keep {
                    break;
                }
            }
            let _ = save(&disk, &target);
        });
        Ok(Self(Arc::new(Inner {
            state,
            path,
            wake,
            worker: Mutex::new(Some(worker)),
            recovery,
        })))
    }
    pub fn get(&self) -> T {
        self.0.state.lock().unwrap().value.clone()
    }
    pub fn edit(&self, edit: impl FnOnce(&mut T)) {
        let mut s = self.0.state.lock().unwrap();
        edit(&mut s.value);
        s.dirty = true;
        s.revision += 1;
        let _ = self.0.wake.send(true);
    }
    pub fn revision(&self) -> u64 {
        self.0.state.lock().unwrap().revision
    }
    pub fn flush(&self) -> Result<(), String> {
        save(&self.0.state, &self.0.path)
    }
    pub fn recovery(&self) -> Option<String> {
        self.0.recovery.clone()
    }
    pub fn error(&self) -> Option<String> {
        self.0.state.lock().unwrap().error.clone()
    }
}
