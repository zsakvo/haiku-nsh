//! Shared, cancellable complete-page refreshes. No partial page reaches the Loro mirror.
use futures::{
    FutureExt,
    future::{BoxFuture, Shared},
};
use haiku_model::{EntityRef, Track};
use haiku_plugins::{Cancellation, Host, PageArgs, SourceApi};
use haiku_store::collections::Collections;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

type Result<T> = std::result::Result<T, String>;
type Load =
    Arc<dyn Fn(EntityRef, Cancellation) -> BoxFuture<'static, Result<Snapshot>> + Send + Sync>;
type Pending = Shared<BoxFuture<'static, Result<bool>>>;

pub(crate) struct Snapshot {
    title: String,
    description: String,
    cover: String,
    tracks: Vec<Track>,
}
struct Flight {
    serial: u64,
    leases: BTreeSet<String>,
    cancel: Cancellation,
    future: Pending,
}
#[derive(Default)]
struct State {
    next: u64,
    flights: BTreeMap<String, Flight>,
    leases: BTreeMap<String, String>,
}
impl Drop for State {
    fn drop(&mut self) {
        for flight in self.flights.values() {
            flight.cancel.cancel();
        }
    }
}
#[derive(Clone)]
pub(crate) struct FavoriteRefresh {
    state: Arc<Mutex<State>>,
    store: Collections,
    load: Load,
}
impl FavoriteRefresh {
    pub fn new(host: Arc<Host>, store: Collections) -> Self {
        Self {
            state: Default::default(),
            store,
            load: Arc::new(move |reference, cancel| {
                let host = host.clone();
                async move {
                    let api =
                        SourceApi::new(host.source(&reference.source).map_err(|e| e.to_string())?);
                    let info = Arc::new(Mutex::new(None));
                    let tracks = SourceApi::collect_all(|cursor| {
                        let (api, cancel, id) = (api.clone(), cancel.clone(), reference.id.clone());
                        let info = info.clone();
                        async move {
                            let first = cursor.is_none();
                            let detail = api
                                .detail(
                                    "playlist",
                                    &id,
                                    PageArgs { cursor, limit: 40 },
                                    Some(&cancel),
                                )
                                .await?;
                            if first {
                                *info.lock().unwrap() = detail.info;
                            }
                            Ok(detail.tracks)
                        }
                    })
                    .await
                    .map_err(|e| e.to_string())?;
                    let info = info.lock().unwrap().take().ok_or("歌单信息缺失")?;
                    Ok(Snapshot {
                        title: info.title,
                        description: info.description.unwrap_or_default(),
                        cover: info.cover.unwrap_or_default(),
                        tracks,
                    })
                }
                .boxed()
            }),
        }
    }
    /// Reserve before starting async work, so departure can cancel even an unpolled request.
    pub fn reserve(&self, reference: EntityRef) -> Result<String> {
        if reference.kind != "playlist" {
            return Err("只能刷新收藏的远端歌单".into());
        }
        let id = format!("favorite:{}:{}", reference.source, reference.id);
        let version = self.store.refresh_version(&id)?;
        let key = reference.encode();
        let mut state = self.state.lock().unwrap();
        state.next += 1;
        let serial = state.next;
        let lease = serial.to_string();
        if !state.flights.contains_key(&key) {
            let cancel = Cancellation::default();
            let (store, load, weak, token, flight_key) = (
                self.store.clone(),
                self.load.clone(),
                Arc::downgrade(&self.state),
                cancel.clone(),
                key.clone(),
            );
            let future = async move {
                let snapshot = tokio::select! {
                    biased;
                    _ = token.cancelled() => return Err("刷新已取消".into()),
                    result = load(reference, token.clone()) => result?,
                };
                let state = weak.upgrade().ok_or("刷新已取消")?;
                // Cancellation and commit serialize on the same lock.
                let state = state.lock().unwrap();
                if token.is_cancelled()
                    || !state
                        .flights
                        .get(&flight_key)
                        .is_some_and(|flight| flight.serial == serial)
                {
                    return Err("刷新已取消".into());
                }
                store.refresh_favorite(
                    &id,
                    version,
                    &snapshot.title,
                    &snapshot.description,
                    &snapshot.cover,
                    &snapshot.tracks,
                )
            }
            .boxed()
            .shared();
            state.flights.insert(
                key.clone(),
                Flight {
                    serial,
                    leases: BTreeSet::new(),
                    cancel,
                    future,
                },
            );
        }
        state
            .flights
            .get_mut(&key)
            .unwrap()
            .leases
            .insert(lease.clone());
        state.leases.insert(lease.clone(), key);
        Ok(lease)
    }
    pub fn pending(&self, lease: &str) -> Result<Pending> {
        let state = self.state.lock().unwrap();
        let key = state.leases.get(lease).ok_or("刷新已取消")?;
        Ok(state.flights.get(key).ok_or("刷新已取消")?.future.clone())
    }
    /// Cancel only this consumer; the last departure cancels the underlying plugin calls.
    pub fn release(&self, lease: &str) {
        let mut state = self.state.lock().unwrap();
        let Some(key) = state.leases.remove(lease) else {
            return;
        };
        if let Some(flight) = state.flights.get_mut(&key) {
            flight.leases.remove(lease);
            if flight.leases.is_empty() {
                flight.cancel.cancel();
                state.flights.remove(&key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn reference() -> EntityRef {
        EntityRef {
            source: "fixture".into(),
            kind: "playlist".into(),
            id: "remote:/雪".into(),
        }
    }
    fn fixture(load: Load) -> (tempfile::TempDir, FavoriteRefresh, String) {
        let dir = tempfile::tempdir().unwrap();
        let store = Collections::open(dir.path()).unwrap();
        let r = reference();
        let id = store
            .favorite(
                &r.source,
                &r.id,
                "Fixture",
                "Offline",
                "",
                "",
                &[Track::local("/old")],
            )
            .unwrap();
        (
            dir,
            FavoriteRefresh {
                state: Default::default(),
                store,
                load,
            },
            id,
        )
    }
    fn snapshot() -> Snapshot {
        Snapshot {
            title: "Fresh".into(),
            description: "".into(),
            cover: "".into(),
            tracks: vec![Track::local("/new")],
        }
    }
    #[tokio::test]
    async fn favorite_refresh_complete_plugin_pages_and_monotonic_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        let host = Host::open(dir.path()).await.unwrap();
        host.install_files(
            BTreeMap::from([(
                "plugin.ts".into(),
                br#"
            import {definePlugin} from "haiku";
            export default definePlugin({
              id:"refresh.fixture",name:"Refresh",version:"1.0.0",
              playlist:{async get(id,ctx){
                return {info:ctx.cursor?undefined:{id,title:"Refreshed"},
                  tracks:{items:[{id:ctx.cursor||"first",title:ctx.cursor||"first",artists:[]}],
                    next:ctx.cursor?undefined:"opaque:/page-2"}};
              }}
            });
        "#
                .to_vec(),
            )]),
            false,
        )
        .await
        .unwrap();
        let source = host.activate("refresh.fixture").await.unwrap();
        let store = Collections::open(dir.path()).unwrap();
        let r = EntityRef {
            source: source.record.source_id.clone(),
            kind: "playlist".into(),
            id: "list".into(),
        };
        let id = store
            .favorite(&r.source, &r.id, "Fixture", "Offline", "", "", &[])
            .unwrap();
        let before = store
            .list()
            .into_iter()
            .find(|p| p.id == id)
            .unwrap()
            .refreshed_at;
        let service = FavoriteRefresh::new(host, store.clone());
        let lease = service.reserve(r).unwrap();
        assert!(service.pending(&lease).unwrap().await.unwrap());
        service.release(&lease);
        let current = store.list().into_iter().find(|p| p.id == id).unwrap();
        assert_eq!(current.title, "Refreshed");
        assert!(current.refreshed_at > before);
        assert_eq!(
            store
                .tracks(&id)
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "opaque:/page-2"]
        );
        store.flush().unwrap();
        assert_eq!(Collections::open(dir.path()).unwrap().tracks(&id).len(), 2);
    }
    #[tokio::test]
    async fn favorite_refresh_single_flight_failure_keeps_snapshot() {
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let (_dir, service, id) = fixture(Arc::new(move |_, _| {
            count.fetch_add(1, Ordering::SeqCst);
            async { Err("offline".into()) }.boxed()
        }));
        let a = service.reserve(reference()).unwrap();
        let b = service.reserve(reference()).unwrap();
        let before = service.store.export_snapshot();
        let (a_result, b_result) =
            futures::join!(service.pending(&a).unwrap(), service.pending(&b).unwrap());
        assert_eq!(a_result, Err("offline".into()));
        assert_eq!(a_result, b_result);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(service.store.export_snapshot(), before);
        assert_eq!(service.store.tracks(&id)[0].id, "/old");
        service.release(&a);
        service.release(&b);
    }
    #[tokio::test]
    async fn favorite_refresh_last_departure_cancels_and_reentry_is_fresh() {
        let gate = Arc::new(tokio::sync::Notify::new());
        let ready = gate.clone();
        let (_dir, service, id) = fixture(Arc::new(move |_, _| {
            let gate = ready.clone();
            async move {
                gate.notified().await;
                Ok(snapshot())
            }
            .boxed()
        }));
        let a = service.reserve(reference()).unwrap();
        let b = service.reserve(reference()).unwrap();
        let mut old = service.pending(&a).unwrap();
        assert!(futures::poll!(&mut old).is_pending());
        service.release(&a);
        assert!(futures::poll!(&mut old).is_pending());
        service.release(&b);
        assert!(old.await.is_err());
        assert_eq!(service.store.tracks(&id)[0].id, "/old");
        let next = service.reserve(reference()).unwrap();
        let mut pending = service.pending(&next).unwrap();
        assert!(futures::poll!(&mut pending).is_pending());
        gate.notify_one();
        assert!(pending.await.unwrap());
        service.release(&next);
        assert_eq!(service.store.tracks(&id)[0].id, "/new");
    }
    #[tokio::test]
    async fn favorite_refresh_late_version_and_deleted_mirror_are_discarded() {
        let gate = Arc::new(tokio::sync::Notify::new());
        let ready = gate.clone();
        let (_dir, service, id) = fixture(Arc::new(move |_, _| {
            let gate = ready.clone();
            async move {
                gate.notified().await;
                Ok(snapshot())
            }
            .boxed()
        }));
        for delete in [false, true] {
            let lease = service.reserve(reference()).unwrap();
            let mut pending = service.pending(&lease).unwrap();
            assert!(futures::poll!(&mut pending).is_pending());
            if delete {
                service.store.delete(&id).unwrap();
            } else {
                let r = reference();
                service
                    .store
                    .favorite(
                        &r.source,
                        &r.id,
                        "Fixture",
                        "Newer",
                        "",
                        "",
                        &[Track::local("/newer")],
                    )
                    .unwrap();
            }
            gate.notify_one();
            assert!(!pending.await.unwrap());
            service.release(&lease);
            if delete {
                assert!(!service.store.list().iter().any(|p| p.id == id));
            } else {
                assert_eq!(service.store.tracks(&id)[0].id, "/newer");
            }
        }
    }
}
