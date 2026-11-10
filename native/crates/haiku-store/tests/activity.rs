use haiku_model::Track;
use haiku_store::{
    history::{History, PlaylistHistory},
    logs::{Filter, Logs},
    stats::Stats,
};

#[test]
fn history_reference_json_identity_limits_searches_and_groups() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("history.json"),
        include_bytes!("fixtures/reference-history.json"),
    )
    .unwrap();
    let h = History::open(dir.path()).unwrap();
    let reference = h.snapshot();
    assert_eq!(reference.tracks[0].track.title, "Reference Song");
    assert_eq!(
        reference.playlists[0].cover_headers.as_ref().unwrap()["Authorization"],
        "fixture-only"
    );
    assert_eq!(reference.playlists[0].track_count, None);
    for i in 0..510 {
        let mut t = Track::local(format!("song-{i}"));
        t.source = if i % 2 == 0 { "a" } else { "b" }.into();
        h.record_track(&t, i);
    }
    assert_eq!(h.snapshot().tracks.len(), 500);
    let mut t = Track::local("same");
    t.source = "a".into();
    h.record_track(&t, 9999);
    t.source = "b".into();
    h.record_track(&t, 10000);
    h.record_track(&t, 10001);
    assert_eq!(h.tracks("same", "").len(), 2);
    assert_eq!(h.tracks("same", "b")[0].played_at, 10001);
    for i in 0..105 {
        h.record_playlist(PlaylistHistory {
            kind: "local".into(),
            source_id: "collections".into(),
            id: i.to_string(),
            played_at: i,
            ..Default::default()
        });
        h.search(&format!("query-{i}"));
    }
    h.search(" query-1 ");
    assert_eq!(h.snapshot().playlists.len(), 100);
    assert_eq!(h.snapshot().searches.len(), 30);
    assert_eq!(h.snapshot().searches[0], "query-1");
    h.remove("tracks", &t.entity_ref().encode());
    assert_eq!(h.tracks("same", "").len(), 1);
    h.clear("playlists");
    h.flush().unwrap();
    assert!(
        History::open(dir.path())
            .unwrap()
            .snapshot()
            .playlists
            .is_empty()
    );
    let now = haiku_store::stats::wall_time().0;
    assert_eq!(haiku_store::history::group(now, now), "今天");
    assert_eq!(haiku_store::history::group(now - 86400, now), "昨天");
    assert_eq!(haiku_store::history::group(now - 30 * 86400, now), "更早");
}

#[test]
fn injected_clock_segments_exclude_pause_seek_gaps_and_accept_five_seconds() {
    let dir = tempfile::tempdir().unwrap();
    let s = Stats::open(dir.path()).unwrap();
    let t = Track::local("clock");
    let tick = |mono, pos, timeline, playing| {
        s.sample(Some(&t), 1, timeline, playing, pos, mono, 1794300000, 0)
    };
    tick(0, 0., 0, true);
    tick(4000, 4., 0, true);
    tick(4000, 4., 0, false);
    assert!(s.snapshot().events.is_empty());
    tick(5000, 4., 0, true);
    tick(9000, 8., 0, true);
    tick(10000, 40., 1, true);
    tick(14000, 44., 1, true);
    tick(30000, 60., 1, true);
    s.finish();
    assert!(s.snapshot().events.is_empty());
    tick(31000, 0., 2, true);
    tick(36000, 5., 2, true);
    s.finish();
    assert_eq!(s.snapshot().events.len(), 1);
    assert_eq!(s.snapshot().events[0].listened_sec, 5.);
    tick(37000, 0., 3, true);
    tick(41999, 4.999, 3, true);
    s.finish();
    assert_eq!(s.snapshot().events.len(), 1);
    tick(43000, 0., 4, true);
    tick(48000, 5., 4, true);
    s.flush().unwrap(); // active segment is included in the exit barrier
    assert_eq!(Stats::open(dir.path()).unwrap().snapshot().events.len(), 2);
}

#[test]
fn stats_reference_json_aggregates_rank_hours_calendar_streak_milestones_and_cap() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("stats.json"),
        include_bytes!("fixtures/reference-stats.json"),
    )
    .unwrap();
    let s = Stats::open(dir.path()).unwrap();
    let original = s.snapshot().events[0].clone();
    assert_eq!(original.track.source_id, "subsonic.music:test");
    assert_eq!(original.listened_sec, 5.);
    assert_eq!(original.event_id.len(), 40);
    let device = s.snapshot().device_id;
    s.clear();
    let now = 1794312000; // 2026-11-10 12:00 UTC
    for i in 0..52 {
        let mut e = original.clone();
        e.played_at = now - if i < 50 { 0 } else { (i - 49) * 86400 };
        e.listened_sec = if i < 50 { 5. } else { 100. };
        e.tz_offset_min = 0;
        e.track.id = if i < 50 { "frequent" } else { "long" }.into();
        s.record(e);
    }
    let a = s.aggregate("all", now);
    assert_eq!(a["count"], 52);
    assert_eq!(a["uniqueTracks"], 2);
    assert_eq!(a["uniqueArtists"], 1);
    assert_eq!(a["seconds"], 450.);
    assert_eq!(a["tracks"][0]["count"], 50);
    assert_eq!(a["longestStreak"], 3);
    assert_eq!(a["currentStreak"], 3);
    assert_eq!(a["milestones"][0]["reached"], true);
    assert_eq!(
        a["hours"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .sum::<f64>(),
        450.
    );
    assert_eq!(s.aggregate("day", now)["count"], 50);
    assert_eq!(s.aggregate("month", now)["count"], 52);
    assert_eq!(s.aggregate("year", now)["count"], 52);
    for _ in 0..5001 {
        s.record(original.clone());
    }
    assert_eq!(s.snapshot().events.len(), 5000);
    s.clear();
    assert_eq!(s.snapshot().device_id, device);
}

#[test]
fn logs_rotate_bound_redact_filter_and_notifications_dismiss() {
    let dir = tempfile::tempdir().unwrap();
    let logs = Logs::open(dir.path()).unwrap();
    logs.add(
        "plugin",
        "warn",
        "fixture",
        Some("http"),
        "https://user:secret@music.invalid/path?token=hidden#hidden",
        Some(&format!(
            "Cookie: secret\nAuthorization: Bearer hidden\n{{\"password\":\"hidden\"}}\n{}",
            "雪".repeat(9000)
        )),
    )
    .unwrap();
    let f = Filter {
        channel: "plugin".into(),
        source: "fixture".into(),
        level: "warn".into(),
        query: "music.invalid".into(),
    };
    let entries = logs.filter(&f);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].detail.as_ref().unwrap().chars().count(), 8000);
    let text = logs.text(&f);
    assert!(!text.contains("hidden") && !text.contains("secret") && !text.contains("user:"));
    assert!(text.contains("[REDACTED]"));
    logs.notify("error", "token=hidden", Some("Cookie: secret"));
    let n = logs.notices();
    assert!(!n[0].message.contains("hidden"));
    logs.clear_notice(Some(n[0].id));
    assert!(logs.notices().is_empty());
    let next = Logs::open(dir.path()).unwrap();
    assert!(dir.path().join("logs/app.previous.jsonl").exists());
    assert_eq!(next.filter(&f).len(), 1);
    next.clear("plugin").unwrap();
    assert!(next.filter(&f).is_empty());
    assert!(!next.filter(&Filter::default()).is_empty());
    next.clear("").unwrap();
    assert!(next.filter(&Filter::default()).is_empty());
    // Import oversized logs then normalize once, avoiding 1001 disk rewrites in the test.
    let mut short = entries[0].clone();
    short.detail = None;
    let entry = serde_json::to_string(&short).unwrap();
    std::fs::write(
        dir.path().join("logs/plugin.jsonl"),
        (entry + "\n").repeat(1002),
    )
    .unwrap();
    let bounded = Logs::open(dir.path()).unwrap();
    assert_eq!(
        bounded
            .filter(&Filter {
                channel: "plugin".into(),
                ..Default::default()
            })
            .len(),
        1000
    );
}
