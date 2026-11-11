use super::*;
use crate::account::{tests::Server, MemoryCredentials};
use std::sync::atomic::Ordering;

async fn fixture() -> (tempfile::TempDir, Arc<account::Service>, Stats, Server) {
    let root = tempfile::tempdir().unwrap();
    let server = Server::start();
    let account =
        account::Service::open(root.path(), Arc::new(MemoryCredentials::default())).unwrap();
    account.set_server(server.url.clone()).unwrap();
    account
        .login("test@example.com".into(), "123456".into())
        .await
        .unwrap();
    let stats = Stats::open(root.path()).unwrap();
    stats.set_account("7", &server.url);
    (root, account, stats, server)
}
fn event(id: &str, played_at: i64) -> ListenEvent {
    serde_json::from_value(json!({
        "sourceId":"fixture.music","id":id,"provider":"fixture.music","title":id,
        "artists":["Fixture"],"eventId":id,"playedAt":played_at,"listenedSec":10.,
        "tzOffsetMin":480,
    }))
    .unwrap()
}
fn remote(id: &str) -> Value {
    json!({"event_id":id,"plugin_id":"remote.music","track_id":id,"provider":"remote.music",
        "title":"Remote","artists":["Remote"],"played_at":9,"listened_sec":12.,"tz_offset_min":0})
}
fn posts(server: &Server) -> Vec<Value> {
    server
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(p, b)| p == "/api/v1/listening/events" && b.is_object())
        .map(|(_, b)| b.clone())
        .collect()
}
async fn wait_for(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn stats_upload_watermark_remote_dedup_and_demo_exclusion() {
    let (_root, account, stats, server) = fixture().await;
    stats.record(event("first", 10));
    let mut demo = event("demo", 1000);
    demo.extra.insert("isDemo".into(), json!(true));
    stats.record(demo);
    server.remote.lock().unwrap().push(remote("remote-1"));
    let identity = account.identity().unwrap();
    sync_once(&account, &stats, &identity).await.unwrap();
    assert_eq!(stats.snapshot().watermark, 10);
    assert_eq!(stats.snapshot().revision, 2);
    assert_eq!(posts(&server)[0]["events"].as_array().unwrap().len(), 1);
    assert_eq!(posts(&server)[0]["events"][0]["plugin_id"], "fixture.music");
    assert_eq!(posts(&server)[0]["device_id"], stats.snapshot().device_id);
    stats.record(event("later", 20));
    sync_once(&account, &stats, &identity).await.unwrap();
    sync_once(&account, &stats, &identity).await.unwrap();
    let batches = posts(&server);
    assert_eq!(batches[1]["events"].as_array().unwrap().len(), 2); // Timestamp ties are replayable.
    assert_eq!(batches[2]["events"].as_array().unwrap().len(), 1);
    assert_eq!(batches[2]["events"][0]["event_id"], "later");
    assert_eq!(
        stats
            .snapshot()
            .events
            .iter()
            .filter(|e| e.event_id == "remote-1")
            .count(),
        1
    );
    assert!(server
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(p, _)| p.contains("/listening/"))
        .all(|(_, b)| !b.to_string().contains("access_token")));
}

#[tokio::test]
async fn stats_upload_batches_and_failed_pagination_never_advances_revision() {
    let (_root, account, stats, server) = fixture().await;
    for index in 0..501 {
        stats.record(event(&index.to_string(), index));
    }
    server.remote.lock().unwrap().push(remote("uncommitted"));
    server.repeat_cursor.store(true, Ordering::SeqCst);
    assert!(sync_once(&account, &stats, &account.identity().unwrap())
        .await
        .unwrap_err()
        .contains("游标"));
    let batches = posts(&server);
    assert_eq!(batches[0]["events"].as_array().unwrap().len(), 500);
    assert_eq!(batches[1]["events"].as_array().unwrap().len(), 1);
    assert_eq!(stats.snapshot().watermark, 500);
    assert_eq!(stats.snapshot().revision, 0);
    assert!(!stats
        .snapshot()
        .events
        .iter()
        .any(|e| e.event_id == "uncommitted"));
}

#[tokio::test]
async fn stats_upload_logout_discards_late_response_and_account_switch_resets_remote() {
    let (_root, account, stats, server) = fixture().await;
    stats.record(event("local", 10));
    server.hold_stats.store(true, Ordering::SeqCst);
    let (pending_account, pending_stats, identity) =
        (account.clone(), stats.clone(), account.identity().unwrap());
    let pending =
        tokio::spawn(async move { sync_once(&pending_account, &pending_stats, &identity).await });
    wait_for(|| !posts(&server).is_empty()).await;
    account.logout().await.unwrap();
    server.hold_stats.store(false, Ordering::SeqCst);
    assert!(pending.await.unwrap().is_err());
    assert_eq!(stats.snapshot().watermark, 0);
    stats
        .sync_commit(0, 2, vec![remote_event(&remote("old-account")).unwrap()])
        .unwrap();
    account
        .login("other@example.com".into(), "123456".into())
        .await
        .unwrap();
    stats.set_account("8", &server.url);
    assert_eq!(stats.snapshot().events.len(), 1);
    assert_eq!(stats.snapshot().events[0].event_id, "local");
    assert_eq!(stats.snapshot().revision, 0);
}

#[tokio::test]
async fn stats_upload_scheduler_debounces_stops_and_reports_backoff() {
    let (_root, account, stats, server) = fixture().await;
    let service = Service::start(account.clone(), stats.clone());
    wait_for(|| service.snapshot().last_sync.is_some()).await;
    stats.record(event("debounced", 11));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(posts(&server).is_empty());
    tokio::time::sleep(DEBOUNCE).await;
    wait_for(|| !posts(&server).is_empty()).await;
    wait_for(|| !service.snapshot().syncing).await;
    account.logout().await.unwrap();
    wait_for(|| stats.snapshot().account_id.is_empty()).await;
    let calls = server.calls.lock().unwrap().len();
    stats.record(event("logged-out", 12));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(server.calls.lock().unwrap().len(), calls);
    server.fail_stats.store(true, Ordering::SeqCst);
    account
        .login("other@example.com".into(), "123456".into())
        .await
        .unwrap();
    wait_for(|| service.snapshot().error.is_some()).await;
    assert!(service.snapshot().retry_at.unwrap() > account::now());
    assert_eq!(stats.snapshot().account_id, "8");
    service.stop();
}

#[test]
fn stats_upload_event_id_matches_reference_sha1_and_backoff_is_bounded() {
    let root = tempfile::tempdir().unwrap();
    haiku_store::json_file::write(
        &root.path().join("stats.json"),
        &json!({
            "deviceId":"fixture-device"
        }),
    )
    .unwrap();
    let stats = Stats::open(root.path()).unwrap();
    let track = haiku_model::Track::local("song");
    stats.sample(Some(&track), 0, 0, true, 0., 0, 1700000000, 480);
    stats.sample(Some(&track), 0, 0, true, 6., 6000, 1700000000, 480);
    stats.finish();
    // Node createHash('sha1').update('fixture-device|local|song|1700000000|480').
    assert_eq!(
        stats.snapshot().events[0].event_id,
        "97cdb2ebe3e552789b4c1f3482e0c776f81cd11f"
    );
    assert_eq!(backoff(1), Duration::from_secs(15));
    assert_eq!(backoff(2), Duration::from_secs(30));
    assert_eq!(backoff(8), Duration::from_secs(1800));
    assert_eq!(backoff(u32::MAX), Duration::from_secs(1800));
}
