//! Account-fenced listening sync. Only local events wake the upload debounce.
use crate::account::{self, Identity};
use haiku_store::stats::{ListenEvent, Stats};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{sync::watch, time::Instant};

const DEBOUNCE: Duration = Duration::from_secs(12);
const PULL: Duration = Duration::from_secs(120);

#[derive(Clone, Default, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub syncing: bool,
    pub last_sync: Option<u64>,
    pub error: Option<String>,
    pub retry_at: Option<u64>,
}
pub struct Service {
    events: watch::Sender<Status>,
    stop: watch::Sender<bool>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}
impl Service {
    pub fn start(account: Arc<account::Service>, stats: Stats) -> Arc<Self> {
        let (events, _) = watch::channel(Status::default());
        let (stop, stopped) = watch::channel(false);
        let updates = events.clone();
        let task = crate::downloader::runtime().spawn(run(account, stats, updates, stopped));
        Arc::new(Self {
            events,
            stop,
            task: Mutex::new(Some(task)),
        })
    }
    pub fn snapshot(&self) -> Status {
        self.events.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<Status> {
        self.events.subscribe()
    }
    pub fn stop(&self) {
        self.stop.send_replace(true);
        if let Some(task) = self.task.lock().unwrap().take() {
            task.abort();
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.stop();
    }
}
fn backoff(failures: u32) -> Duration {
    Duration::from_secs(
        15u64
            .saturating_mul(1u64 << failures.saturating_sub(1).min(10))
            .min(1800),
    )
}
async fn identity_changed(
    account: &account::Service,
    events: &mut watch::Receiver<account::Snapshot>,
    identity: &Identity,
) {
    while account.is_current(identity) {
        if events.changed().await.is_err() {
            break;
        }
        events.borrow_and_update();
    }
}
async fn run(
    account: Arc<account::Service>,
    stats: Stats,
    updates: watch::Sender<Status>,
    mut stopped: watch::Receiver<bool>,
) {
    let mut accounts = account.subscribe();
    let mut local = stats.subscribe_local();
    let mut active = None;
    let mut due = Instant::now();
    let mut pull_due = due;
    let mut retry = Instant::now();
    let mut failures = 0;
    let mut initialized = false;
    loop {
        if *stopped.borrow() {
            return;
        }
        let identity = account.identity();
        if !initialized || active != identity {
            initialized = true;
            let server = account.snapshot().base_url;
            stats.set_account(
                identity
                    .as_ref()
                    .map(|i| i.account_id.as_str())
                    .unwrap_or(""),
                if identity.is_some() { &server } else { "" },
            );
            active = identity;
            failures = 0;
            due = Instant::now();
            pull_due = due;
            retry = due;
            updates.send_replace(Status::default());
        }
        let Some(identity) = active.clone() else {
            tokio::select! {
                _ = stopped.changed() => return,
                _ = accounts.changed() => { accounts.borrow_and_update(); },
                _ = local.changed() => { local.borrow_and_update(); },
            }
            continue;
        };
        tokio::select! {
            _ = stopped.changed() => return,
            _ = accounts.changed() => { accounts.borrow_and_update(); continue; },
            _ = local.changed() => {
                local.borrow_and_update();
                due = (Instant::now() + DEBOUNCE).min(pull_due).max(retry);
                continue;
            },
            _ = tokio::time::sleep_until(due) => {},
        }
        updates.send_modify(|status| {
            status.syncing = true;
            status.error = None;
        });
        let result = tokio::select! {
            _ = stopped.changed() => return,
            _ = identity_changed(&account, &mut accounts, &identity) => continue,
            result = sync_once(&account, &stats, &identity) => result,
        };
        match result {
            Ok(()) => {
                failures = 0;
                retry = Instant::now();
                due = retry + PULL;
                pull_due = due;
                updates.send_replace(Status {
                    last_sync: Some(account::now()),
                    ..Default::default()
                });
            }
            Err(error) => {
                failures += 1;
                let delay = backoff(failures);
                retry = Instant::now() + delay;
                due = retry;
                updates.send_modify(|status| {
                    status.syncing = false;
                    status.error = Some(error);
                    status.retry_at = Some(account::now() + delay.as_millis() as u64);
                });
            }
        }
    }
}
fn commit(
    account: &account::Service,
    stats: &Stats,
    identity: &Identity,
    watermark: i64,
    revision: u64,
    remote: Vec<ListenEvent>,
) -> Result<(), String> {
    account
        .with_current(identity, || stats.sync_commit(watermark, revision, remote))
        .ok_or_else(|| "账号会话已更改".to_string())?
}
/// One upload/pull transaction; all requests and commits retain the captured account epoch.
pub async fn sync_once(
    account: &account::Service,
    stats: &Stats,
    identity: &Identity,
) -> Result<(), String> {
    let snapshot = stats.snapshot();
    if snapshot.account_id != identity.account_id {
        return Err("统计账号已更改".into());
    }
    let request = |method, path: String, body| async move {
        account
            .request_for(identity, method, &path, body)
            .await
            .map_err(|e| e.to_string())
    };
    let pending = snapshot
        .events
        .iter()
        .filter(|e| {
            e.extra.get("isRemote") != Some(&Value::Bool(true))
                && e.extra.get("isDemo") != Some(&Value::Bool(true))
                && e.track.extra.get("isDemo") != Some(&Value::Bool(true))
                && e.track.extra.get("extra").and_then(|v| v.get("isDemo"))
                    != Some(&Value::Bool(true))
                && e.played_at >= snapshot.watermark
        })
        .collect::<Vec<_>>();
    for batch in pending.chunks(500) {
        let events = batch.iter().map(|e| json!({
            "event_id":e.event_id, "plugin_id":e.track.source_id, "track_id":e.track.id,
            "provider":e.track.provider, "title":e.track.title, "artists":e.track.artists,
            "album":e.track.album, "cover":e.track.cover, "duration":e.track.duration,
            "played_at":e.played_at, "listened_sec":e.listened_sec, "tz_offset_min":e.tz_offset_min,
        })).collect::<Vec<_>>();
        let result = request(
            "POST",
            "/listening/events".into(),
            Some(json!({
                "device_id":snapshot.device_id, "events":events,
            })),
        )
        .await?;
        if result["rejected"]["invalid"].as_u64().unwrap_or(0) > 0 {
            return Err("统计服务器拒绝了无效记录".into());
        }
        let watermark = batch.iter().map(|e| e.played_at).max().unwrap_or(0);
        commit(account, stats, identity, watermark, 0, vec![])?;
    }
    let meta = request("GET", "/listening/meta".into(), None).await?;
    if meta["exists"] != true || meta["revision"].as_u64().unwrap_or(0) <= snapshot.revision {
        return Ok(());
    }
    let mut cursor = String::new();
    let mut seen = BTreeSet::new();
    let mut remote = vec![];
    let mut revision = snapshot.revision;
    for _ in 0..100 {
        let mut query = reqwest::Url::parse("http://localhost/").unwrap();
        query.query_pairs_mut().append_pair("limit", "1000");
        if !cursor.is_empty() {
            query.query_pairs_mut().append_pair("cursor", &cursor);
        } else if snapshot.revision > 0 {
            query
                .query_pairs_mut()
                .append_pair("since", &snapshot.revision.to_string());
        }
        let path = format!("/listening/events?{}", query.query().unwrap());
        let page = request("GET", path, None).await?;
        let events = page["events"].as_array().ok_or("统计分页响应异常")?;
        for event in events {
            remote.push(remote_event(event)?);
        }
        revision = revision.max(page["revision"].as_u64().unwrap_or(0));
        if page["has_more"] == false {
            return commit(account, stats, identity, 0, revision, remote);
        }
        cursor = page["next_cursor"].as_str().unwrap_or("").into();
        if cursor.is_empty() || !seen.insert(cursor.clone()) {
            return Err("统计分页游标无效".into());
        }
    }
    Err("统计分页超过安全上限".into())
}
fn remote_event(event: &Value) -> Result<ListenEvent, String> {
    let text = |field| {
        event[field]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| "统计事件响应异常".to_string())
    };
    let listened_sec = event["listened_sec"]
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 5.)
        .ok_or("统计时长响应异常")?;
    Ok(ListenEvent {
        track: haiku_store::history::StoredTrack {
            source_id: text("plugin_id")?,
            id: text("track_id")?,
            provider: event["provider"].as_str().unwrap_or("").into(),
            title: event["title"].as_str().unwrap_or("").into(),
            artists: serde_json::from_value(event["artists"].clone()).unwrap_or_default(),
            album: event["album"].as_str().map(str::to_owned),
            cover: event["cover"].as_str().map(str::to_owned),
            duration: event["duration"].as_f64(),
            ..Default::default()
        },
        event_id: text("event_id")?,
        played_at: event["played_at"].as_i64().ok_or("统计时间响应异常")?,
        listened_sec,
        tz_offset_min: event["tz_offset_min"].as_i64().unwrap_or(0) as i32,
        extra: [("isRemote".into(), json!(true))].into(),
    })
}

#[cfg(test)]
#[path = "stats_upload_tests.rs"]
mod tests;
