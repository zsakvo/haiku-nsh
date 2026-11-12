//! Real archive restoration drives a recording registration backend, including rejection.
use super::*;
use futures::future::BoxFuture;
use haiku_services::backup::{Hooks, Service};
use haiku_services::sync::MemorySecrets;

#[derive(Default)]
struct Backend {
    live: Mutex<Vec<Binding>>,
    reject: Mutex<bool>,
}
impl GlobalShortcuts for Backend {
    fn supported(&self) -> bool {
        true
    }
    fn replace(&self, bindings: &[Binding]) -> Result<(), String> {
        if *self.reject.lock().unwrap() {
            return Err("registration conflict".into());
        }
        *self.live.lock().unwrap() = bindings.to_vec();
        Ok(())
    }
}
struct Reload(Arc<Registration>);
impl Hooks for Reload {
    fn flush(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
    fn before_restore(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
    fn after_restore(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async {
            let _guard = self.0.serial.lock().await;
            self.0.store.reload()?;
            self.0.reload()
        })
    }
}
#[tokio::test]
async fn backup_restore_registers_new_group_and_rejection_rolls_back_preferences() {
    let root = tempfile::tempdir().unwrap();
    let store = Arc::new(SettingsStore::open(root.path()).unwrap());
    let backend = Arc::new(Backend::default());
    let registration = Registration::new(store.clone(), backend.clone());
    let service = Service::new(
        root.path().into(),
        Arc::new(MemorySecrets::default()),
        Arc::new(Reload(registration.clone())),
    );
    let old = shortcuts::defaults();
    let mut restored = old.clone();
    restored
        .iter_mut()
        .find(|b| b.scope == "global" && b.id == "togglePlayback")
        .unwrap()
        .chord = "control+alt+p".into();
    restored
        .iter_mut()
        .find(|b| b.scope == "app" && b.id == "openSettings")
        .unwrap()
        .chord = "meta+shift+,".into();
    registration.commit(&restored).unwrap();
    let path = root.path().join("fixture.haikubak");
    service.export(&path, false).await.unwrap();
    registration.commit(&old).unwrap();
    service.inspect(&path).unwrap();
    service.import(&path).await.unwrap();
    assert_eq!(*backend.live.lock().unwrap(), restored);
    assert_eq!(*registration.applied.lock().unwrap(), restored);

    registration.commit(&old).unwrap();
    *backend.reject.lock().unwrap() = true;
    service.inspect(&path).unwrap();
    let error = service.import(&path).await.err().unwrap();
    assert!(error.contains("registration conflict"));
    assert_eq!(*backend.live.lock().unwrap(), old);
    let s = SettingsStore::open(root.path()).unwrap().get();
    assert_eq!(shortcuts::effective(&s.shortcuts, &s.global_shortcuts), old);
}
