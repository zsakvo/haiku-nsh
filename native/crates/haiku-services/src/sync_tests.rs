use super::*;
use crate::sync_fixture::Server;
use haiku_model::Track;
use std::sync::atomic::Ordering;

fn service(root: &Path, server: &Server, fast: bool) -> Arc<Service> {
    let options = Options {
        github_base: server.url.clone(),
        debounce: if fast {
            Duration::from_millis(60)
        } else {
            Duration::from_secs(12)
        },
        poll: if fast {
            Duration::from_millis(180)
        } else {
            Duration::from_secs(120)
        },
        retry_base: Duration::from_millis(80),
        retry_cap: Duration::from_millis(160),
    };
    Service::open(
        root,
        Collections::open(root).unwrap(),
        Arc::new(SettingsStore::open(root).unwrap()),
        Arc::new(MemorySecrets::default()),
        options,
    )
    .unwrap()
}
async fn wait_for(mut f: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !f() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
fn count(server: &Server, method: &str) -> usize {
    server
        .data
        .lock()
        .unwrap()
        .calls
        .iter()
        .filter(|(m, _, _)| m == method)
        .count()
}
#[test]
fn sync_reference_codec_reads_legacy_and_v2_and_writes_reference_readable_gzip() {
    let bytes = include_bytes!("../../haiku-store/tests/fixtures/reference-playlists.loro");
    let reference = STANDARD
        .decode(include_str!("../tests/fixtures/reference-sync-v2.b64").trim())
        .unwrap();
    let (decoded, progress) = decode(&reference).unwrap();
    assert_eq!(decoded, bytes);
    assert_eq!(progress["album"]["trackId"], "song");
    assert_eq!(decode(bytes).unwrap().0, bytes);
    let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
    gzip.write_all(bytes).unwrap();
    assert_eq!(decode(&gzip.finish().unwrap()).unwrap().0, bytes);
    let packed = encode(bytes, &progress).unwrap();
    // Reference decodeSnapshot: optional gunzip, prefix, JSON v2, nested base64.
    let mut reference_input = vec![];
    GzDecoder::new(packed.as_slice())
        .read_to_end(&mut reference_input)
        .unwrap();
    assert!(reference_input.starts_with(b"HAIKU_SYNC_2\n"));
    let value: Value = serde_json::from_slice(&reference_input[PREFIX.len()..]).unwrap();
    assert_eq!(value["version"], 2);
    assert_eq!(
        STANDARD
            .decode(value["playlists"].as_str().unwrap())
            .unwrap(),
        bytes
    );
    assert_eq!(value["albumProgress"]["album"]["updatedAt"], 100);
    assert!(decode(b"HAIKU_SYNC_2\n{\"version\":9}").is_err());
    assert!(gist_id("https://evil.invalid/abc123").is_err());
    assert_eq!(
        gist_id("https://gist.github.com/listener/abc123").unwrap(),
        "abc123"
    );
}

#[tokio::test]
async fn sync_two_instances_merge_conflicting_edits_persist_and_import_idempotently() {
    let server = Server::start().unwrap();
    let (ra, rb) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let a = service(ra.path(), &server, false);
    a.set_token("fixture-token".into()).await.unwrap();
    let id = a.collections.create("Shared").unwrap();
    let tracks = [Track::local("A"), Track::local("B"), Track::local("C")];
    a.collections.add(&id, &tracks).unwrap();
    a.create_gist().await.unwrap();
    let b = service(rb.path(), &server, false);
    b.set_token("fixture-token".into()).await.unwrap();
    b.bind_gist("https://gist.github.com/user/abc123".into())
        .await
        .unwrap();
    a.collections
        .remove(&id, &[tracks[0].entity_ref()])
        .unwrap();
    b.collections.move_track(&id, 2, 0).unwrap();
    b.collections.add(&id, &[Track::local("D")]).unwrap();
    let (left, right) = (
        a.collections.export_snapshot(),
        b.collections.export_snapshot(),
    );
    let (rx, ry) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (x, y) = (
        Collections::open(rx.path()).unwrap(),
        Collections::open(ry.path()).unwrap(),
    );
    x.import(&left).unwrap();
    x.import(&right).unwrap();
    y.import(&right).unwrap();
    y.import(&left).unwrap();
    assert_eq!(x.tracks(&id), y.tracks(&id));
    a.sync_now(false).await.unwrap();
    b.sync_now(false).await.unwrap();
    a.sync_now(true).await.unwrap();
    assert_eq!(a.collections.tracks(&id), b.collections.tracks(&id));
    let names = a
        .collections
        .tracks(&id)
        .into_iter()
        .map(|t| t.id)
        .collect::<Vec<_>>();
    assert!(!names.contains(&"A".to_owned()));
    assert!(
        ["B", "C", "D"]
            .iter()
            .all(|id| names.contains(&id.to_string()))
    );
    assert_eq!(
        names
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        names.len()
    );
    let local = a.collections.subscribe_local();
    assert!(
        a.collections
            .import(&b.collections.export_snapshot())
            .unwrap()
            .is_empty()
    );
    assert!(!local.has_changed().unwrap());
    let restored = Collections::open(ra.path()).unwrap();
    assert_eq!(restored.tracks(&id), a.collections.tracks(&id));
    assert!(
        !std::fs::read_to_string(ra.path().join("settings.json"))
            .unwrap()
            .contains("fixture-token")
    );
    a.disconnect().unwrap();
    assert!(!a.snapshot().connected && a.snapshot().gist_id.is_empty());
}

#[tokio::test]
async fn sync_single_flight_and_switch_token_discards_late_response_and_shutdown_drains() {
    let server = Server::start().unwrap();
    let root = tempfile::tempdir().unwrap();
    let sync = service(root.path(), &server, false);
    sync.set_token("fixture-token".into()).await.unwrap();
    sync.create_gist().await.unwrap();
    let calls = count(&server, "PATCH");
    let futures = (0..12).map(|_| sync.sync_now(false));
    for result in futures::future::join_all(futures).await {
        result.unwrap();
    }
    assert_eq!(count(&server, "PATCH"), calls + 1);
    server.hold.store(true, Ordering::SeqCst);
    let before = count(&server, "GET");
    let pending = sync.clone();
    let task = tokio::spawn(async move { pending.sync_now(false).await });
    wait_for(|| count(&server, "GET") > before).await;
    sync.set_token("replacement-token".into()).await.unwrap();
    assert!(task.await.unwrap().is_err());
    assert_eq!(sync.snapshot().revision, json!(0));
    server.hold.store(false, Ordering::SeqCst);
    sync.start();
    sync.shutdown().await.unwrap();
    assert!(sync.sync_now(false).await.is_err());
}

#[tokio::test]
async fn sync_scheduler_debounces_pulls_retries_and_remote_import_never_triggers_push() {
    let server = Server::start().unwrap();
    let root = tempfile::tempdir().unwrap();
    let sync = service(root.path(), &server, true);
    sync.set_token("fixture-token".into()).await.unwrap();
    sync.create_gist().await.unwrap();
    sync.start();
    // Let the scheduler subscribe before mutating; readiness comes from a complete pull.
    wait_for(|| sync.snapshot().last_synced > 0).await;
    let patches = count(&server, "PATCH");
    sync.collections.create("Local wake").unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(count(&server, "PATCH"), patches);
    wait_for(|| count(&server, "PATCH") > patches).await;
    let remote_root = tempfile::tempdir().unwrap();
    let remote = Collections::open(remote_root.path()).unwrap();
    remote.create("Remote-only").unwrap();
    {
        let mut data = server.data.lock().unwrap();
        data.files["playlists.loro.b64"]["content"] =
            json!(STANDARD.encode(encode(&remote.export_snapshot(), &BTreeMap::new()).unwrap()));
        data.revision += 1;
    }
    let patches = count(&server, "PATCH");
    wait_for(|| {
        sync.collections
            .list()
            .iter()
            .any(|p| p.title == "Remote-only")
    })
    .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(count(&server, "PATCH"), patches);
    server.failure.store(500, Ordering::SeqCst);
    sync.collections.create("Retry me").unwrap();
    wait_for(|| sync.snapshot().retry_at.is_some()).await;
    let retry_at = sync.snapshot().retry_at.unwrap();
    assert!(retry_at <= crate::account::now() + 200);
    server.failure.store(0, Ordering::SeqCst);
    wait_for(|| sync.snapshot().error.is_empty() && !sync.snapshot().busy).await;
    sync.shutdown().await.unwrap();
    let count = server.data.lock().unwrap().calls.len();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(server.data.lock().unwrap().calls.len(), count);
}
