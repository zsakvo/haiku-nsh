//! Reference-compatible collections with serialized mutations, automatic persistence and events.
use crate::playlist_core::{LIKED_PLAYLIST_ID, PlaylistCore, PlaylistMeta, TrackEntry};
use haiku_model::{Album, Artist, EntityRef, Track};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// A committed change. IDs include deleted collections; consumers reread their views.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub revision: u64,
    pub playlist_ids: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: String,
    pub title: String,
    pub description: String,
    pub kind: String,
    pub position: u32,
    pub track_count: u32,
    pub created_at: i64,
    pub source_id: Option<String>,
    pub remote_id: Option<String>,
    pub source_label: Option<String>,
    pub cover: Option<String>,
    pub refreshed_at: i64,
    pub favorited_at: i64,
}
impl From<PlaylistMeta> for Collection {
    fn from(p: PlaylistMeta) -> Self {
        Self {
            id: p.id,
            title: p.title,
            description: p.description,
            kind: p.kind,
            position: p.position,
            track_count: p.track_count,
            created_at: p.created_at,
            source_id: p.source_plugin_id,
            remote_id: p.source_playlist_id,
            source_label: p.source_label,
            cover: p.cover_url,
            refreshed_at: p.refreshed_at,
            favorited_at: p.favorited_at,
        }
    }
}
struct State {
    core: PlaylistCore,
    revision: u64,
    dirty: bool,
    listeners: Vec<mpsc::Sender<Change>>,
    error: Option<String>,
}
impl State {
    fn changed(&mut self, ids: Vec<String>) {
        if ids.is_empty() {
            return;
        }
        self.dirty = true;
        self.revision += 1;
        let event = Change {
            revision: self.revision,
            playlist_ids: ids,
        };
        self.listeners
            .retain(|listener| listener.send(event.clone()).is_ok());
    }
    fn writable(&self, id: &str, liked: bool) -> Result<(), String> {
        match self.core.playlists().into_iter().find(|p| p.id == id) {
            Some(p) if p.kind == "user" || (liked && p.kind == "liked") => Ok(()),
            Some(_) => Err("收藏镜像只读；喜欢请使用专用操作".into()),
            None => Err("歌单不存在".into()),
        }
    }
}
enum Wake {
    Changed,
    Shutdown,
}
struct Inner {
    state: Arc<Mutex<State>>,
    path: PathBuf,
    tx: mpsc::Sender<Wake>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl Drop for Inner {
    fn drop(&mut self) {
        let _ = self.tx.send(Wake::Shutdown);
        if let Some(worker) = self.worker.get_mut().unwrap().take() {
            let _ = worker.join();
        }
    }
}
/// Shared service. The final handle flushes and joins its writer; `flush` is an exit barrier.
#[derive(Clone)]
pub struct Collections(Arc<Inner>);
fn save(state: &Mutex<State>, path: &Path) -> Result<(), String> {
    let mut state = state.lock().unwrap();
    if !state.dirty {
        return Ok(());
    }
    let bytes = state.core.export_snapshot();
    let result = (|| {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("loro.tmp");
        std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(tmp, path).map_err(|e| e.to_string())
    })();
    state.error = result.as_ref().err().cloned();
    if result.is_ok() {
        state.dirty = false;
    }
    result
}
impl Collections {
    /// Open only this application's data root. Preserve corrupt input for diagnosis.
    pub fn open(root: &Path) -> Result<Self, String> {
        let path = root.join("playlists.loro");
        let core = match std::fs::read(&path) {
            Ok(bytes) => match PlaylistCore::from_snapshot(&bytes) {
                Ok(core) => core,
                Err(_) => {
                    let stamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis();
                    std::fs::rename(&path, path.with_extension(format!("loro.corrupt-{stamp}")))
                        .map_err(|e| e.to_string())?;
                    PlaylistCore::new()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => PlaylistCore::new(),
            Err(e) => return Err(e.to_string()),
        };
        let dirty = core.ensure_liked_playlist("喜欢");
        let state = Arc::new(Mutex::new(State {
            core,
            dirty,
            revision: 0,
            listeners: vec![],
            error: None,
        }));
        let (tx, rx) = mpsc::channel();
        let disk_state = state.clone();
        let disk_path = path.clone();
        let worker = thread::Builder::new()
            .name("haiku-collections".into())
            .spawn(move || {
                loop {
                    match rx.recv() {
                        Ok(Wake::Changed) => loop {
                            match rx.recv_timeout(Duration::from_millis(500)) {
                                Ok(Wake::Changed) => continue,
                                Ok(Wake::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                                    let _ = save(&disk_state, &disk_path);
                                    return;
                                }
                                Err(mpsc::RecvTimeoutError::Timeout) => {
                                    let _ = save(&disk_state, &disk_path);
                                    break;
                                }
                            }
                        },
                        Ok(Wake::Shutdown) | Err(_) => {
                            let _ = save(&disk_state, &disk_path);
                            return;
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let service = Self(Arc::new(Inner {
            state,
            path,
            tx,
            worker: Mutex::new(Some(worker)),
        }));
        if dirty {
            let _ = service.0.tx.send(Wake::Changed);
        }
        Ok(service)
    }
    fn mutate<T>(
        &self,
        f: impl FnOnce(&mut State) -> Result<(T, Vec<String>), String>,
    ) -> Result<T, String> {
        let mut state = self.0.state.lock().unwrap();
        let (result, ids) = f(&mut state)?;
        let changed = !ids.is_empty();
        state.changed(ids);
        if changed {
            let _ = self.0.tx.send(Wake::Changed);
        }
        Ok(result)
    }
    /// Current summaries in stored display order.
    pub fn list(&self) -> Vec<Collection> {
        self.0
            .state
            .lock()
            .unwrap()
            .core
            .playlists()
            .into_iter()
            .map(Into::into)
            .collect()
    }
    /// SDK tracks; likes are ordered by addition time, newest first, with stable ties.
    pub fn tracks(&self, id: &str) -> Vec<Track> {
        let mut entries = self.0.state.lock().unwrap().core.playlist_tracks(id);
        if id == LIKED_PLAYLIST_ID {
            entries.sort_by_key(|t| std::cmp::Reverse(t.added_at));
        }
        entries.into_iter().map(from_entry).collect()
    }
    /// Subscribe to committed mutations. Dropping the receiver stops delivery.
    pub fn subscribe(&self) -> mpsc::Receiver<Change> {
        let (tx, rx) = mpsc::channel();
        self.0.state.lock().unwrap().listeners.push(tx);
        rx
    }
    pub fn revision(&self) -> u64 {
        self.0.state.lock().unwrap().revision
    }
    /// Most recent background write failure, cleared by a successful retry.
    pub fn persistence_error(&self) -> Option<String> {
        self.0.state.lock().unwrap().error.clone()
    }
    /// Synchronous persistence barrier for exit/backup. Normal callers never need it.
    pub fn flush(&self) -> Result<(), String> {
        save(&self.0.state, &self.0.path)
    }
    pub fn is_liked(&self, key: &EntityRef) -> bool {
        key.kind == "track"
            && self
                .0
                .state
                .lock()
                .unwrap()
                .core
                .playlist_tracks(LIKED_PLAYLIST_ID)
                .iter()
                .any(|t| t.plugin_id == key.source && t.track_id == key.id)
    }
    /// Dedicated, atomic like/unlike path; toggle uses the same serialized transaction.
    pub fn set_liked(&self, track: &Track, value: Option<bool>) -> Result<bool, String> {
        self.mutate(|state| {
            let old = state
                .core
                .playlist_tracks(LIKED_PLAYLIST_ID)
                .iter()
                .any(|t| t.plugin_id == track.source && t.track_id == track.id);
            let liked = value.unwrap_or(!old);
            let ids = if old == liked {
                vec![]
            } else if liked {
                state.core.add_track(LIKED_PLAYLIST_ID, &to_entry(track))
            } else {
                state
                    .core
                    .remove_track(LIKED_PLAYLIST_ID, &track.source, &track.id)
            };
            Ok((liked, ids))
        })
    }
    pub fn create(&self, title: &str) -> Result<String, String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("歌单名称不能为空".into());
        }
        self.mutate(|s| {
            let id = s.core.create_playlist(title);
            Ok((id.clone(), vec![id]))
        })
    }
    pub fn rename(&self, id: &str, title: &str, description: &str) -> Result<(), String> {
        if title.trim().is_empty() {
            return Err("歌单名称不能为空".into());
        }
        self.mutate(|s| {
            s.writable(id, false)?;
            Ok(((), s.core.rename_playlist(id, title.trim(), description)))
        })
    }
    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.mutate(|s| {
            if id == LIKED_PLAYLIST_ID {
                return Err("不能删除喜欢歌单".into());
            }
            if !s.core.playlists().iter().any(|p| p.id == id) {
                return Err("歌单不存在".into());
            }
            Ok(((), s.core.delete_playlist(id)))
        })
    }
    pub fn add(&self, id: &str, tracks: &[Track]) -> Result<(), String> {
        self.mutate(|s| {
            s.writable(id, false)?;
            let mut ids = vec![];
            for track in tracks {
                ids.extend(s.core.add_track(id, &to_entry(track)));
            }
            ids.sort();
            ids.dedup();
            Ok(((), ids))
        })
    }
    pub fn remove(&self, id: &str, keys: &[EntityRef]) -> Result<(), String> {
        self.mutate(|s| {
            s.writable(id, true)?;
            let mut ids = vec![];
            for key in keys.iter().filter(|k| k.kind == "track") {
                ids.extend(s.core.remove_track(id, &key.source, &key.id));
            }
            ids.sort();
            ids.dedup();
            Ok(((), ids))
        })
    }
    pub fn move_track(&self, id: &str, from: u32, to: u32) -> Result<(), String> {
        self.mutate(|s| {
            s.writable(id, false)?;
            Ok(((), s.core.move_track(id, from as usize, to as usize)))
        })
    }
    pub fn reorder(&self, order: &[String]) -> Result<(), String> {
        self.mutate(|s| {
            let mut order = order.to_vec();
            order.retain(|id| id != LIKED_PLAYLIST_ID);
            order.insert(0, LIKED_PLAYLIST_ID.into());
            Ok(((), s.core.reorder_playlists(&order)))
        })
    }
    /// Offline immutable remote mirror. Source ID is the installed source instance identity.
    pub fn favorite(
        &self,
        source: &str,
        remote: &str,
        label: &str,
        title: &str,
        description: &str,
        cover: &str,
        tracks: &[Track],
    ) -> Result<String, String> {
        self.mutate(|s| {
            let entries = tracks.iter().map(to_entry).collect::<Vec<_>>();
            let id = s.core.favorite_plugin_playlist(
                source,
                remote,
                label,
                title,
                description,
                cover,
                &entries,
            );
            Ok((id.clone(), vec![id]))
        })
    }
    pub fn export_snapshot(&self) -> Vec<u8> {
        self.0.state.lock().unwrap().core.export_snapshot()
    }
    pub fn import(&self, bytes: &[u8]) -> Result<Vec<String>, String> {
        self.mutate(|s| {
            let ids = s.core.import(bytes)?;
            Ok((ids.clone(), ids))
        })
    }
}
fn to_entry(t: &Track) -> TrackEntry {
    TrackEntry {
        sdk_track: Some(t.clone()),
        plugin_id: t.source.clone(),
        track_id: t.id.clone(),
        provider: String::new(),
        title: t.title.clone(),
        artists: t
            .artists
            .iter()
            .map(|a| match a {
                Artist::Name(n) | Artist::Entity { name: n, .. } => n.clone(),
            })
            .collect(),
        album: t.album.as_ref().map(|a| a.title.clone()),
        cover: t.cover.clone(),
        duration: t.duration.map(|v| v as i64),
        added_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64,
    }
}
fn from_entry(t: TrackEntry) -> Track {
    if let Some(track) = t.sdk_track {
        return track;
    }
    let mut track = Track::local(&t.track_id);
    track.source = t.plugin_id;
    track.local_path = if track.source == "local" {
        Some(t.track_id.clone())
    } else {
        None
    };
    track.title = t.title;
    track.artists = t.artists.into_iter().map(Artist::Name).collect();
    track.album = t.album.map(|title| Album { id: None, title });
    track.cover = t.cover;
    track.duration = t.duration.map(|v| v as f64);
    track
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn debounce_flush_restart_identity_crud_and_events() {
        let dir = tempfile::tempdir().unwrap();
        let service = Collections::open(dir.path()).unwrap();
        service.flush().unwrap();
        let initial = std::fs::read(dir.path().join("playlists.loro")).unwrap();
        let events = service.subscribe();
        let mut a = Track::local("/a.mp3");
        a.title = "A".into();
        let mut b = a.clone();
        b.source = "subsonic#雪".into();
        b.local_path = None;
        assert!(service.set_liked(&a, None).unwrap());
        assert!(service.set_liked(&b, None).unwrap());
        assert!(!service.set_liked(&a, None).unwrap());
        assert!(service.is_liked(&b.entity_ref()));
        assert_eq!(
            std::fs::read(dir.path().join("playlists.loro")).unwrap(),
            initial
        );
        let id = service.create(" Mix ").unwrap();
        service
            .add(&id, &[a.clone(), b.clone(), a.clone()])
            .unwrap();
        assert_eq!(service.tracks(&id).len(), 2);
        service.move_track(&id, 0, 1).unwrap();
        assert_eq!(service.tracks(&id)[0], b);
        service.rename(&id, "Renamed", "Description").unwrap();
        assert!(service.rename(LIKED_PLAYLIST_ID, "No", "").is_err());
        assert!(service.add(LIKED_PLAYLIST_ID, &[a.clone()]).is_err());
        let second = service.create("Second").unwrap();
        service.reorder(&[second.clone(), id.clone()]).unwrap();
        assert_eq!(service.list()[1].id, second);
        service.remove(&id, &[b.entity_ref()]).unwrap();
        assert_eq!(service.tracks(&id), vec![a.clone()]);
        service.delete(&second).unwrap();
        assert!(!service.list().iter().any(|p| p.id == second));
        assert!(events.try_iter().any(|e| e.playlist_ids.contains(&id)));
        drop(events);
        assert!(service.set_liked(&a, Some(true)).unwrap());
        // Wait on observable persistence, bounded by a generous deadline (no repeated builds).
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while std::fs::read(dir.path().join("playlists.loro")).unwrap() == initial {
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(Duration::from_millis(20));
        }
        let reopened = Collections::open(dir.path()).unwrap();
        assert_eq!(reopened.tracks(&id), vec![a.clone()]);
        drop(reopened);
        service.rename(&id, "Exit flush", "").unwrap();
        drop(service);
        let reopened = Collections::open(dir.path()).unwrap();
        assert!(reopened.list().iter().any(|p| p.title == "Exit flush"));
        assert!(reopened.is_liked(&a.entity_ref()));
        assert!(reopened.is_liked(&b.entity_ref()));
    }
    #[test]
    fn reads_reference_binary_fixture() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("playlists.loro"),
            include_bytes!("../tests/fixtures/reference-playlists.loro"),
        )
        .unwrap();
        let service = Collections::open(root.path()).unwrap();
        let track = &service.tracks(LIKED_PLAYLIST_ID)[0];
        assert_eq!(track.source, "subsonic.music#参考");
        assert_eq!(track.id, "id:/?#雪");
        assert_eq!(track.title, "Reference song");
        assert_eq!(track.duration, Some(123.));
        let pl = service
            .list()
            .into_iter()
            .find(|p| p.kind == "user")
            .unwrap();
        assert_eq!(pl.title, "参考版歌单");
        assert_eq!(pl.track_count, 1);
        assert_eq!(pl.description, "由参考版 dist/haiku-native 生成");
        let reread = PlaylistCore::from_snapshot(&service.export_snapshot()).unwrap();
        assert_eq!(
            reread.playlist_tracks(LIKED_PLAYLIST_ID)[0].plugin_id,
            "subsonic.music#参考"
        );
    }
    #[test]
    fn two_services_merge_idempotently_and_mirrors_are_read_only() {
        let a_dir = tempfile::tempdir().unwrap();
        let b_dir = tempfile::tempdir().unwrap();
        let a = Collections::open(a_dir.path()).unwrap();
        let b = Collections::open(b_dir.path()).unwrap();
        let id = a.create("Shared").unwrap();
        b.import(&a.export_snapshot()).unwrap();
        a.add(&id, &[Track::local("/a")]).unwrap();
        b.add(&id, &[Track::local("/b")]).unwrap();
        a.import(&b.export_snapshot()).unwrap();
        b.import(&a.export_snapshot()).unwrap();
        assert_eq!(a.tracks(&id), b.tracks(&id));
        assert_eq!(a.tracks(&id).len(), 2);
        assert!(a.import(&b.export_snapshot()).unwrap().is_empty());
        let remote = a
            .favorite(
                "source#1",
                "remote:/雪",
                "Server",
                "Remote",
                "",
                "",
                &[Track::local("/a")],
            )
            .unwrap();
        assert_eq!(remote, "favorite:source#1:remote:/雪");
        assert!(a.add(&remote, &[Track::local("/b")]).is_err());
        assert!(a.rename(&remote, "No", "").is_err());
        a.delete(&remote).unwrap();
    }
}
