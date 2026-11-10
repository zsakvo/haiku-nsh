//! Exit barriers belong to Core lifetimes, independently of playback and media sessions.
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
};
type Barrier = dyn Fn() -> Result<(), String> + Send + Sync;
fn barriers() -> &'static Mutex<BTreeMap<u64, Weak<Barrier>>> {
    static BARRIERS: OnceLock<Mutex<BTreeMap<u64, Weak<Barrier>>>> = OnceLock::new();
    BARRIERS.get_or_init(Default::default)
}
pub(crate) struct ExitGuard {
    id: u64,
    _barrier: Arc<Barrier>,
}
impl Drop for ExitGuard {
    fn drop(&mut self) {
        barriers().lock().unwrap().remove(&self.id);
    }
}
pub(crate) fn register(
    barrier: impl Fn() -> Result<(), String> + Send + Sync + 'static,
) -> ExitGuard {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let barrier: Arc<Barrier> = Arc::new(barrier);
    barriers()
        .lock()
        .unwrap()
        .insert(id, Arc::downgrade(&barrier));
    ExitGuard {
        id,
        _barrier: barrier,
    }
}
pub(crate) fn flush_all() -> Result<(), String> {
    let pending: Vec<_> = barriers()
        .lock()
        .unwrap()
        .values()
        .filter_map(Weak::upgrade)
        .collect();
    let errors: Vec<_> = pending.iter().filter_map(|flush| flush().err()).collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}
pub(crate) fn on_exit() {
    if let Err(error) = flush_all() {
        log::error!("Core exit flush: {error}");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn quit_flushes_collections_before_any_media_session_and_unregisters_core() {
        let root = tempfile::tempdir().unwrap();
        let store = haiku_store::collections::Collections::open(root.path()).unwrap();
        let writer = store.clone();
        let guard = super::register(move || writer.flush());
        let id = store.create("Exit before playback").unwrap();
        assert!(!root.path().join("playlists.loro").exists());
        super::flush_all().unwrap();
        let restored = haiku_store::collections::Collections::open(root.path()).unwrap();
        assert!(restored.list().iter().any(|p| p.id == id));
        let registration = guard.id;
        drop(guard);
        assert!(
            !super::barriers()
                .lock()
                .unwrap()
                .contains_key(&registration)
        );
    }
}
