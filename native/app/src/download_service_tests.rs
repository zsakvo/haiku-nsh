use super::*;
use haiku_library::test_support::{minimal_flac, tiny_jpeg};
use haiku_model::{Album, Artist};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::atomic::{AtomicBool, Ordering},
};

struct Server {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(vec![]));
        let (stopped, recorded) = (stop.clone(), requests.clone());
        let thread = std::thread::spawn(move || {
            let mut interrupted = false;
            let mut handlers = vec![];
            while !stopped.load(Ordering::Relaxed) {
                let (mut socket, _) = match listener.accept() {
                    Ok(socket) => socket,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(_) => break,
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = String::new();
                let mut byte = [0];
                while !request.ends_with("\r\n\r\n") {
                    if socket.read(&mut byte).ok() != Some(1) {
                        break;
                    }
                    request.push(byte[0] as char);
                }
                recorded.lock().unwrap().push(request.clone());
                let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let disconnect = path == "/interrupt" && !interrupted;
                if disconnect {
                    interrupted = true;
                }
                handlers.push(std::thread::spawn(move || {
                    if path == "/bad" || !request.to_lowercase().contains("x-download: accepted") {
                        let _ = socket.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        return;
                    }
                    let body = if path == "/cover" { tiny_jpeg() } else { minimal_flac() };
                    let start = request.to_lowercase().lines()
                        .find_map(|line| line.strip_prefix("range: bytes="))
                        .and_then(|range| range.split('-').next()?.parse::<usize>().ok())
                        .unwrap_or(0);
                    let body = &body[start..];
                    let status = if start > 0 { "206 Partial Content" } else { "200 OK" };
                    let range = if start > 0 {
                        format!("Content-Range: bytes {start}-{}/{}\r\n", start + body.len() - 1, start + body.len())
                    } else { String::new() };
                    // All audio responses deliberately lie about their container.
                    let head = format!("HTTP/1.1 {status}\r\nContent-Type: audio/mpeg\r\nContent-Length: {}\r\n{range}Connection: close\r\n\r\n", body.len());
                    let _ = socket.write_all(head.as_bytes());
                    if disconnect {
                        let _ = socket.write_all(&body[..32]);
                    } else if path == "/slow" {
                        for chunk in body.chunks(16) {
                            if socket.write_all(chunk).is_err() { break; }
                            let _ = socket.flush();
                            std::thread::sleep(Duration::from_millis(350));
                        }
                    } else {
                        let _ = socket.write_all(body);
                    }
                }));
            }
            for handler in handlers {
                handler.join().unwrap();
            }
        });
        Self {
            url,
            requests,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}
async fn fixture() -> (tempfile::TempDir, Arc<Service>, Server) {
    let root = tempfile::tempdir().unwrap();
    let server = Server::start();
    let host = Host::open(root.path()).await.unwrap();
    let plugin = r#"
import {definePlugin} from "haiku";
export default definePlugin({
  id: "download.fixture", name: "Download fixture", version: "1.0.0",
  http: {headers: {"X-Download": "accepted"}},
  track: {
    async streams(id, ctx) {
      if (id === "never") await new Promise(() => {});
      const candidate = path => ({
        url: BASE + path, quality: ctx.quality, format: "audio/mpeg",
        headers: {"X-Candidate": "retained"},
      });
      return id === "fallback" ? [candidate("/bad"), candidate("/song")]
        : [candidate("/" + id)];
    },
    async lyrics() {
      return {text: "[00:01.00]Original", format: "lrc", translation: "[00:01.00]译文"};
    },
  },
  playlist: {
    async get(id, ctx) {
      return {info: {id, title: "Fixture playlist"}, tracks: {
        items: [{id: ctx.cursor ? "song2" : "song", title: "Same",
          artists: ["Artist"], album: {title: "Album"}}], next: ctx.cursor ? undefined : "page:two",
      }};
    },
  },
});
"#
    .replace("BASE", &json!(server.url).to_string());
    host.install_files(
        BTreeMap::from([("plugin.ts".into(), plugin.into_bytes())]),
        false,
    )
    .await
    .unwrap();
    host.activate("download.fixture").await.unwrap();
    let settings = Arc::new(SettingsStore::open(root.path()).unwrap());
    settings
        .update(json!({
            "downloadDir":root.path().join("output"), "downloadConcurrency":1,
            "downloadCoverMode":"both", "downloadLyricsMode":"both",
            "lyricsAutoSearch":false,
        }))
        .unwrap();
    let mut service = Service::open(root.path(), host, settings).unwrap();
    Arc::get_mut(&mut service).unwrap().retry_delays =
        vec![Duration::from_millis(30), Duration::from_millis(60)];
    (root, service, server)
}
fn track(id: &str, server: &Server) -> Track {
    let mut track = Track::local(id);
    track.source = "download.fixture".into();
    track.local_path = None;
    track.title = "Song / unsafe?".into();
    track.artists = vec![Artist::Name("Artist".into())];
    track.album = Some(Album {
        id: Some("album-id".into()),
        title: "Album".into(),
    });
    track.cover = Some(format!("{}/cover", server.url));
    track
}
async fn wait(service: &Service, condition: impl Fn(&[Item]) -> bool) {
    let mut events = service.subscribe();
    tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            let snapshot = events.borrow_and_update().clone();
            if condition(&snapshot) {
                return;
            }
            events.changed().await.unwrap();
        }
    })
    .await
    .unwrap_or_else(|_| panic!("download state: {:?}", service.snapshot()));
}

#[tokio::test]
async fn headers_fallback_sniff_tags_sidecars_collision_and_reference_roundtrip() {
    use lofty::{
        file::TaggedFileExt,
        tag::{Accessor, ItemKey},
    };
    let (root, service, server) = fixture().await;
    let tracks = [track("fallback", &server), track("song", &server)];
    service.enqueue(json!(tracks)).await.unwrap();
    wait(&service, |items| {
        items.len() == 2 && items.iter().all(|i| i.status == "completed")
    })
    .await;
    let items = service.snapshot();
    assert_ne!(items[0].path, items[1].path);
    assert!(items[1].path.as_ref().unwrap().contains("(1).flac"));
    for item in &items {
        let path = Path::new(item.path.as_ref().unwrap());
        assert_eq!(path.extension().unwrap(), "flac");
        assert_eq!(item.key, format!("download.fixture\0{}", item.track.id));
        assert_eq!(item.attempts, 1);
        assert!(item.warning.is_none(), "{:?}", item.warning);
        assert_eq!(item.sidecars.len(), 2);
        let lyrics = fs::read_to_string(path.with_extension("lrc")).unwrap();
        assert!(lyrics.contains("Original") && lyrics.contains("译文"));
        assert_eq!(fs::read(path.with_extension("jpg")).unwrap(), tiny_jpeg());
        let audio = lofty::read_from_path(path).unwrap();
        let tag = audio.primary_tag().unwrap();
        assert_eq!(tag.title().as_deref(), Some("Song / unsafe?"));
        assert!(
            tag.get_string(ItemKey::Lyrics)
                .unwrap()
                .contains("Original")
        );
        assert_eq!(tag.pictures()[0].data(), tiny_jpeg());
    }
    let requests = server.requests.lock().unwrap().clone();
    assert!(requests.iter().any(|r| r.starts_with("GET /bad ")));
    assert!(
        requests
            .iter()
            .filter(|r| !r.starts_with("GET /cover "))
            .all(|r| r.to_lowercase().contains("x-candidate: retained"))
    );
    let disk: Vec<Value> = json_file::read(&root.path().join("haiku_state/downloads.json"))
        .unwrap()
        .unwrap();
    assert_eq!(disk[0]["track"]["sourceId"], "download.fixture");
    assert_eq!(disk[0]["track"]["album"], "Album");
    assert_eq!(disk[0]["track"]["artists"], json!(["Artist"]));
    assert!(disk[0]["track"].get("source").is_none());
    service.shutdown().await.unwrap();
    let restored =
        Service::open(root.path(), service.host.clone(), service.settings.clone()).unwrap();
    assert_eq!(restored.snapshot()[0].track, items[0].track);
    restored.remove(&items[0].key, false).await.unwrap();
    assert!(Path::new(items[0].path.as_ref().unwrap()).exists());
    restored.remove(&items[1].key, true).await.unwrap();
    assert!(!Path::new(items[1].path.as_ref().unwrap()).exists());
    assert!(items[1].sidecars.iter().all(|p| !Path::new(p).exists()));
}

#[tokio::test]
async fn interrupted_transfer_retries_with_range_and_reuses_only_complete_same_quality_cache() {
    let (root, service, server) = fixture().await;
    let song = track("interrupt", &server);
    service.enqueue(json!([song])).await.unwrap();
    wait(&service, |items| items[0].status == "completed").await;
    assert_eq!(service.snapshot()[0].attempts, 2);
    assert!(
        server
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.to_lowercase().contains("range: bytes=32-"))
    );

    let cached = track("never", &server);
    let objects = root.path().join("media-cache/objects");
    fs::create_dir_all(&objects).unwrap();
    let cache = |quality: &str| {
        objects.join(haiku_audio::media_cache::file_id(
            &haiku_playback::backend::cache_key(&cached, quality).unwrap(),
        ))
    };
    fs::write(cache("high"), b"wrong quality").unwrap();
    fs::write(cache("lossless").with_extension("part"), b"partial").unwrap();
    service.enqueue(json!([cached.clone()])).await.unwrap();
    wait(&service, |items| items[1].status == "resolving").await;
    service.control(&key(&cached), "pause").unwrap();
    fs::write(cache("lossless"), minimal_flac()).unwrap();
    service.control(&key(&cached), "resume").unwrap();
    wait(&service, |items| items[1].status == "completed").await;
    assert_eq!(service.snapshot()[1].attempts, 1);
    assert!(
        !server
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.starts_with("GET /never "))
    );
    service.shutdown().await.unwrap();
}

#[tokio::test]
async fn pause_resume_cancellation_fences_stale_events_and_shutdown_restores_queue() {
    let (root, service, server) = fixture().await;
    let song = track("slow", &server);
    service
        .enqueue(json!([song.clone(), track("song", &server)]))
        .await
        .unwrap();
    wait(&service, |_| {
        service.state.lock().unwrap().items[0]
            .temp_path
            .as_ref()
            .and_then(|p| fs::metadata(p).ok())
            .is_some_and(|m| m.len() >= 16)
    })
    .await;
    let old_id = service.state.lock().unwrap().active[&key(&song)].id.clone();
    service.control(&key(&song), "pause").unwrap();
    assert_eq!(service.snapshot()[0].status, "paused");
    assert!(
        service
            .mutate(&key(&song), &old_id, |i| i.status = "completed".into())
            .is_err()
    );
    service.control(&key(&song), "resume").unwrap();
    wait(&service, |items| {
        items.iter().all(|i| i.status == "completed")
    })
    .await;
    assert!(
        server
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.to_lowercase().contains("range: bytes="))
    );

    service
        .enqueue(json!([track("never", &server)]))
        .await
        .unwrap();
    wait(&service, |items| items[2].status == "resolving").await;
    service.stop().unwrap();
    service.shutdown().await.unwrap();
    let restored =
        Service::open(root.path(), service.host.clone(), service.settings.clone()).unwrap();
    assert_eq!(restored.snapshot()[2].status, "queued");
    restored
        .control(&key(&track("never", &server)), "cancel")
        .unwrap();
    assert_eq!(restored.snapshot()[2].status, "canceled");
    fs::remove_file(restored.snapshot()[0].path.as_ref().unwrap()).unwrap();
    restored.verify().unwrap();
    assert_eq!(restored.snapshot()[0].status, "missing");
    restored.clear_completed().unwrap();
    assert_eq!(restored.snapshot().len(), 2);
    restored.shutdown().await.unwrap();
}

#[tokio::test]
async fn collection_pagination_and_reference_record_without_host_extensions() {
    let (root, service, server) = fixture().await;
    service
        .enqueue(json!({"source":"download.fixture","kind":"playlist","id":"list"}))
        .await
        .unwrap();
    wait(&service, |items| {
        items.len() == 2 && items.iter().all(|i| i.status == "completed")
    })
    .await;
    assert_eq!(
        service.snapshot()[0].playlist_name.as_deref(),
        Some("Fixture playlist")
    );
    service.shutdown().await.unwrap();
    let disk = json!([{
        "key":"download.fixture\u{0}reference","track":{
            "sourceId":"download.fixture","provider":"download.fixture","id":"reference",
            "title":"Reference","artists":["Artist"],"album":"Album","coverHeaders":{"x":"kept"}
        },
        "status":"downloading","quality":"lossless","received":32,"total":100,"fileSize":0,
        "attempts":1,"origin":"manual","enqueuedAt":1,"futureField":true
    }]);
    json_file::write(&root.path().join("haiku_state/downloads.json"), &disk).unwrap();
    let restored =
        Service::open(root.path(), service.host.clone(), service.settings.clone()).unwrap();
    assert_eq!(restored.snapshot()[0].status, "queued");
    let saved: Value = json_file::read(&root.path().join("haiku_state/downloads.json"))
        .unwrap()
        .unwrap();
    assert_eq!(saved[0]["track"]["coverHeaders"], json!({"x":"kept"}));
    assert_eq!(saved[0]["futureField"], true);
    // No real upstream is involved even for reference-format restoration.
    let _ = server;
    restored.shutdown().await.unwrap();
}

#[tokio::test]
async fn failed_retry_budget_and_retry_all() {
    let (_root, service, server) = fixture().await;
    let song = track("bad", &server);
    service.enqueue(json!([song.clone()])).await.unwrap();
    wait(&service, |items| items[0].status == "failed").await;
    assert_eq!(service.snapshot()[0].attempts, 3);
    service.retry_failed().unwrap();
    wait(&service, |items| items[0].status == "failed").await;
    assert_eq!(service.snapshot()[0].attempts, 3);
    service.remove(&key(&song), true).await.unwrap();
    assert!(service.snapshot().is_empty());
    service.shutdown().await.unwrap();
}

#[tokio::test]
async fn auto_download_watches_additions_favorite_refresh_and_reopen() {
    use haiku_store::collections::Collections;
    let (root, service, server) = fixture().await;
    let collections = Collections::open(root.path()).unwrap();
    let local = collections.create("自动歌单").unwrap();
    let song = track("song", &server);
    collections.add(&local, &[song.clone()]).unwrap();
    let favorite = collections
        .favorite("download.fixture", "remote", "Fixture", "收藏", "", "", &[])
        .unwrap();
    service
        .settings
        .update(json!({
            "autoDownloadPlaylists": [local, favorite],
            "downloadFollowPlaylistRemoval": true,
        }))
        .unwrap();
    service.watch_collections(collections.clone());
    wait(&service, |items| {
        items.len() == 1 && items[0].status == "completed"
    })
    .await;
    assert_eq!(service.snapshot()[0].origin, format!("auto:{local}"));
    assert_eq!(service.snapshot()[0].playlist_ids, vec![local.clone()]);
    let added = track("song2", &server);
    collections.add(&local, &[added.clone()]).unwrap();
    wait(&service, |items| {
        items.len() == 2 && items.iter().all(|i| i.status == "completed")
    })
    .await;
    let version = collections.refresh_version(&favorite).unwrap();
    collections
        .refresh_favorite(&favorite, version, "收藏", "", "", &[added.clone()])
        .unwrap();
    wait(&service, |items| {
        items
            .iter()
            .any(|i| i.track.id == "song2" && i.playlist_ids.contains(&favorite))
    })
    .await;
    collections.flush().unwrap();
    service.shutdown().await.unwrap();
    let restored =
        Service::open(root.path(), service.host.clone(), service.settings.clone()).unwrap();
    restored.watch_collections(Collections::open(root.path()).unwrap());
    assert_eq!(
        restored.settings.get().auto_download_playlists,
        vec![local.clone(), favorite]
    );
    assert!(restored.settings.get().download_follow_playlist_removal);
    assert_eq!(restored.snapshot().len(), 2);
    // Restart settings and records are reference-compatible, without converting auto to manual.
    let reopened = SettingsStore::open(root.path()).unwrap();
    assert_eq!(reopened.get().auto_download_playlists[0], local);
    assert!(
        restored
            .snapshot()
            .iter()
            .all(|i| i.origin.starts_with("auto:"))
    );
    restored.shutdown().await.unwrap();
}

#[tokio::test]
async fn auto_download_waits_for_settings_commit_and_collection_reload() {
    use haiku_store::collections::Collections;
    let (root, service, server) = fixture().await;
    let collections = Collections::open(root.path()).unwrap();
    let playlist = collections.create("提交唤醒").unwrap();
    collections.add(&playlist, &[track("song", &server)]).unwrap();
    service.watch_collections(collections.clone());
    // A commit after registration must wake an idle watcher, without a collection edit.
    service.settings.update(json!({"autoDownloadPlaylists":[playlist]})).unwrap();
    wait(&service, |items| items.len()==1 && items[0].status=="completed").await;
    collections.flush().unwrap();
    let other = Collections::open(root.path()).unwrap();
    other.add(&playlist, &[track("song2",&server)]).unwrap();
    other.flush().unwrap();
    collections.reload().unwrap();
    wait(&service, |items| items.len()==2 && items.iter().all(|i|i.status=="completed")).await;
    service.shutdown().await.unwrap();
}

#[tokio::test]
async fn auto_download_follow_removal_preserves_manual_and_other_auto_membership() {
    use haiku_store::collections::Collections;
    let (root, service, server) = fixture().await;
    let collections = Collections::open(root.path()).unwrap();
    let a = collections.create("A").unwrap();
    let b = collections.create("B").unwrap();
    let song = track("song", &server);
    collections.add(&a, &[song.clone()]).unwrap();
    collections.add(&b, &[song.clone()]).unwrap();
    service
        .settings
        .update(json!({
            "autoDownloadPlaylists": [a, b], "downloadFollowPlaylistRemoval": true,
        }))
        .unwrap();
    service.reconcile_auto(&collections).await.unwrap();
    wait(&service, |items| items[0].status == "completed").await;
    let downloaded = service.snapshot()[0].clone();
    let audio = downloaded.path.clone().unwrap();
    collections.remove(&a, &[song.entity_ref()]).unwrap();
    service.reconcile_auto(&collections).await.unwrap();
    assert!(Path::new(&audio).is_file());
    assert_eq!(service.snapshot()[0].playlist_ids, vec![b.clone()]);
    collections.remove(&b, &[song.entity_ref()]).unwrap();
    service.reconcile_auto(&collections).await.unwrap();
    assert!(service.snapshot().is_empty());
    assert!(!Path::new(&audio).exists());
    assert!(downloaded.sidecars.iter().all(|p| !Path::new(p).exists()));

    collections.add(&a, &[song.clone()]).unwrap();
    service.reconcile_auto(&collections).await.unwrap();
    wait(&service, |items| items[0].status == "completed").await;
    // An explicit manual enqueue claims an existing automatic job permanently.
    service.enqueue(json!(song)).await.unwrap();
    let manual = service.snapshot()[0].clone();
    collections.remove(&a, &[song.entity_ref()]).unwrap();
    service.reconcile_auto(&collections).await.unwrap();
    assert_eq!(service.snapshot()[0].origin, "manual");
    assert!(Path::new(manual.path.as_ref().unwrap()).exists());
    service.shutdown().await.unwrap();
}

#[tokio::test]
async fn auto_download_without_follow_keeps_file_and_disable_does_not_delete() {
    use haiku_store::collections::Collections;
    let (root, service, server) = fixture().await;
    let collections = Collections::open(root.path()).unwrap();
    let id = collections.create("保留").unwrap();
    let song = track("song", &server);
    collections.add(&id, &[song.clone()]).unwrap();
    service
        .settings
        .update(json!({"autoDownloadPlaylists": [id]}))
        .unwrap();
    service.reconcile_auto(&collections).await.unwrap();
    wait(&service, |items| items[0].status == "completed").await;
    collections.remove(&id, &[song.entity_ref()]).unwrap();
    service.reconcile_auto(&collections).await.unwrap();
    assert!(service.snapshot()[0].playlist_ids.is_empty());
    assert!(service.path(&key(&song)).unwrap().is_file());
    collections.add(&id, &[song]).unwrap();
    service.reconcile_auto(&collections).await.unwrap();
    service
        .settings
        .update(json!({
            "autoDownloadPlaylists": [], "downloadFollowPlaylistRemoval": true,
        }))
        .unwrap();
    service.reconcile_auto(&collections).await.unwrap();
    assert_eq!(service.snapshot().len(), 1);
    service.shutdown().await.unwrap();
}

#[tokio::test]
async fn directory_change_cancel_keep_and_move_preserve_records_and_sidecars() {
    let (root, service, server) = fixture().await;
    service
        .enqueue(json!(track("song", &server)))
        .await
        .unwrap();
    wait(&service, |items| items[0].status == "completed").await;
    let original = service.snapshot()[0].clone();
    let previous = service.settings.get().download_dir;
    let canceled = root.path().join("canceled");
    service
        .change_directory(canceled.display().to_string(), "cancel".into())
        .await
        .unwrap();
    assert!(!canceled.exists());
    assert_eq!(service.settings.get().download_dir, previous);
    let kept = root.path().join("kept");
    let report = service
        .change_directory(kept.display().to_string(), "keep".into())
        .await
        .unwrap();
    assert_eq!(report.moved, 0);
    let kept = kept.canonicalize().unwrap();
    assert_eq!(service.snapshot()[0].path, original.path);
    assert!(Path::new(original.path.as_ref().unwrap()).exists());
    service
        .enqueue(json!(track("song2", &server)))
        .await
        .unwrap();
    wait(&service, |items| {
        items.len() == 2 && items[1].status == "completed"
    })
    .await;
    let second = service.snapshot()[1].clone();
    assert!(Path::new(second.path.as_ref().unwrap()).starts_with(&kept));
    let moved = root.path().join("moved");
    let report = service
        .change_directory(moved.display().to_string(), "move".into())
        .await
        .unwrap();
    let moved = moved.canonicalize().unwrap();
    assert_eq!(report.moved, 3); // audio + cover + lyrics for the job in the current directory
    assert_eq!(report.errors.len(), 3); // retained older files remain at their prior location
    assert_eq!(report.completed, report.total);
    assert!(!report.running);
    let after = service.snapshot();
    assert_eq!(after[0].path, original.path);
    assert!(Path::new(after[1].path.as_ref().unwrap()).starts_with(&moved));
    assert!(!Path::new(second.path.as_ref().unwrap()).exists());
    assert!(after[1].sidecars.iter().all(|p| Path::new(p).is_file()));
    service.shutdown().await.unwrap();
    let restored =
        Service::open(root.path(), service.host.clone(), service.settings.clone()).unwrap();
    assert_eq!(restored.snapshot()[1].path, after[1].path);
    assert_eq!(
        SettingsStore::open(root.path()).unwrap().get().download_dir,
        moved.display().to_string()
    );
    restored.shutdown().await.unwrap();
}

#[tokio::test]
async fn directory_change_reports_partial_failure_without_overwriting() {
    let (root, service, server) = fixture().await;
    service
        .enqueue(json!(track("song", &server)))
        .await
        .unwrap();
    wait(&service, |items| items[0].status == "completed").await;
    let before = service.snapshot()[0].clone();
    let target = root.path().join("collision");
    fs::create_dir_all(&target).unwrap();
    let target = target.canonicalize().unwrap();
    let occupied = target.join(
        Path::new(before.path.as_ref().unwrap())
            .file_name()
            .unwrap(),
    );
    fs::write(&occupied, b"User file").unwrap();
    let report = service
        .change_directory(target.display().to_string(), "move".into())
        .await
        .unwrap();
    assert_eq!(report.moved, 2);
    assert_eq!(report.errors.len(), 1);
    assert!(report.errors[0].contains(&before.track.title));
    let after = service.snapshot()[0].clone();
    assert_eq!(after.path, before.path);
    assert!(Path::new(after.path.as_ref().unwrap()).is_file());
    assert_eq!(fs::read(occupied).unwrap(), b"User file");
    assert!(
        after
            .sidecars
            .iter()
            .all(|p| Path::new(p).starts_with(&target))
    );
    assert_eq!(service.directory_snapshot().errors, report.errors);
    service.shutdown().await.unwrap();
}

#[tokio::test]
async fn directory_change_drains_running_jobs_and_blocks_scheduler_until_done() {
    let (root, service, server) = fixture().await;
    service
        .enqueue(json!([track("slow", &server), track("song", &server)]))
        .await
        .unwrap();
    wait(&service, |items| {
        items[0].status == "downloading" && items[0].received > 0
    })
    .await;
    let migrating = service.clone();
    let target = root.path().join("new");
    let new_dir = target.display().to_string();
    let migration =
        tokio::spawn(async move { migrating.change_directory(new_dir, "move".into()).await });
    let mut progress = service.directory_subscribe();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !progress.borrow().running {
            progress.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    {
        let state = service.state.lock().unwrap();
        assert!(state.migrating);
        assert_eq!(state.items[1].attempts, 0);
    }
    // Even an explicit pump cannot start a queued task during the migration barrier.
    service.pump();
    assert_eq!(service.snapshot()[1].attempts, 0);
    migration.await.unwrap().unwrap();
    assert!(!service.state.lock().unwrap().migrating);
    service
        .control(&key(&track("slow", &server)), "pause")
        .unwrap();
    wait(&service, |items| items[1].status == "completed").await;
    assert_eq!(service.snapshot()[0].status, "paused");
    assert!(
        Path::new(service.snapshot()[1].path.as_ref().unwrap())
            .starts_with(target.canonicalize().unwrap())
    );
    service.shutdown().await.unwrap();
}
