//! Playback samples carry monotonic time; wall time is used only for dates and event identity.
use crate::{document::Document, history::StoredTrack};
use chrono::{Datelike, Local, TimeZone, Timelike, Utc};
use haiku_model::Track;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Arc, Mutex},
};
pub fn wall_time() -> (i64, i32) {
    let now = Local::now();
    (now.timestamp(), now.offset().local_minus_utc() / 60)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenEvent {
    #[serde(flatten)]
    pub track: StoredTrack,
    pub played_at: i64,
    pub listened_sec: f64,
    pub tz_offset_min: i32,
    pub event_id: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StatsState {
    pub schema_version: u32,
    pub events: Vec<ListenEvent>,
    pub device_id: String,
    pub watermark: i64,
    pub revision: u64,
    pub account_id: String,
}
impl Default for StatsState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            events: vec![],
            device_id: uuid::Uuid::new_v4().to_string(),
            watermark: 0,
            revision: 0,
            account_id: String::new(),
        }
    }
}
struct Segment {
    event: ListenEvent,
    identity: (String, u64, u64),
    previous: (u64, f64),
}
#[derive(Clone)]
pub struct Stats {
    doc: Document<StatsState>,
    current: Arc<Mutex<Option<Segment>>>,
}
impl Stats {
    pub fn open(root: &Path) -> Result<Self, String> {
        let doc: Document<StatsState> = Document::open(root.join("stats.json"))?;
        doc.edit(|s| {
            s.events
                .retain(|e| e.listened_sec.is_finite() && e.listened_sec >= 5.);
            if s.events.len() > 5000 {
                s.events.drain(..s.events.len() - 5000);
            }
        });
        Ok(Self {
            doc,
            current: Arc::default(),
        })
    }
    pub fn revision(&self) -> u64 {
        self.doc.revision()
    }
    pub fn recovery(&self) -> Option<String> {
        self.doc.recovery()
    }
    pub fn error(&self) -> Option<String> {
        self.doc.error()
    }
    pub fn snapshot(&self) -> StatsState {
        self.doc.get()
    }
    /// Flush the active segment too, so process exit never depends on an audio session.
    pub fn flush(&self) -> Result<(), String> {
        self.finish();
        self.doc.flush()
    }
    fn commit(&self, segment: Option<Segment>) {
        if let Some(s) = segment.filter(|s| s.event.listened_sec >= 5.) {
            self.record(s.event);
        }
    }
    pub fn finish(&self) {
        let s = self.current.lock().unwrap().take();
        self.commit(s);
    }
    pub fn record(&self, event: ListenEvent) {
        if !event.listened_sec.is_finite() || event.listened_sec < 5. {
            return;
        }
        self.doc.edit(|s| {
            s.events.push(event);
            if s.events.len() > 5000 {
                s.events.drain(..s.events.len() - 5000);
            }
        });
    }
    pub fn clear(&self) {
        self.current.lock().unwrap().take();
        self.doc.edit(|s| {
            s.events.clear();
            s.revision = 0;
            s.watermark = 0;
        });
    }
    /// A pause, seek, generation change or anomalous gap ends a continuous segment.
    #[allow(clippy::too_many_arguments)]
    pub fn sample(
        &self,
        track: Option<&Track>,
        generation: u64,
        timeline: u64,
        playing: bool,
        position: f64,
        mono_ms: u64,
        epoch: i64,
        offset: i32,
    ) {
        let mut guard = self.current.lock().unwrap();
        if !playing || !position.is_finite() || track.is_none() {
            self.commit(guard.take());
            return;
        }
        let track = track.unwrap();
        let identity = (track.entity_ref().encode(), generation, timeline);
        let valid = guard.as_ref().is_some_and(|s| {
            let wall = mono_ms.saturating_sub(s.previous.0) as f64 / 1000.;
            let delta = position - s.previous.1;
            s.identity == identity
                && mono_ms >= s.previous.0
                && wall <= 10.
                && delta >= 0.
                && delta <= (wall + 1.5).min(10.)
        });
        if !valid {
            self.commit(guard.take());
        }
        if let Some(s) = guard.as_mut() {
            let wall = (mono_ms - s.previous.0) as f64 / 1000.;
            let delta = position - s.previous.1;
            if wall > 0. {
                s.event.listened_sec += delta.min(wall + 0.25);
            }
            s.previous = (mono_ms, position);
        } else {
            let id = format!(
                "{}|{}|{}|{}|{}",
                self.doc.get().device_id,
                track.source,
                track.id,
                epoch,
                offset
            );
            *guard = Some(Segment {
                identity,
                previous: (mono_ms, position),
                event: ListenEvent {
                    track: track.into(),
                    played_at: epoch,
                    listened_sec: 0.,
                    tz_offset_min: offset,
                    event_id: format!("{:x}", Sha1::digest(id.as_bytes())),
                    extra: BTreeMap::new(),
                },
            });
        }
    }
    pub fn aggregate(&self, range: &str, now: i64) -> Value {
        let today = Local
            .timestamp_opt(now, 0)
            .single()
            .unwrap_or_else(Local::now)
            .date_naive();
        let events = self
            .snapshot()
            .events
            .into_iter()
            .filter(|e| {
                let day = Local
                    .timestamp_opt(e.played_at, 0)
                    .single()
                    .map(|d| d.date_naive());
                day.is_some_and(|d| match range {
                    "day" => d == today,
                    "month" => d.year() == today.year() && d.month() == today.month(),
                    "year" => d.year() == today.year(),
                    _ => true,
                })
            })
            .collect::<Vec<_>>();
        let mut tracks = BTreeMap::<String, Value>::new();
        let mut artists = BTreeMap::<String, Value>::new();
        let mut days = BTreeMap::<String, f64>::new();
        let mut months = BTreeMap::<String, f64>::new();
        let mut dates = BTreeSet::new();
        let mut hours = [0.; 24];
        let mut seconds = 0.;
        for e in &events {
            seconds += e.listened_sec;
            let v = tracks
                .entry(e.track.key())
                .or_insert_with(|| json!({"track":e.track,"count":0,"seconds":0.}));
            add(v, e.listened_sec);
            for name in &e.track.artists {
                add(
                    artists
                        .entry(name.clone())
                        .or_insert_with(|| json!({"name":name,"count":0,"seconds":0.})),
                    e.listened_sec,
                );
            }
            if let Some(local) = Utc
                .timestamp_opt(e.played_at + e.tz_offset_min as i64 * 60, 0)
                .single()
            {
                let day = local.format("%Y-%m-%d").to_string();
                *days.entry(day.clone()).or_default() += e.listened_sec;
                *months.entry(day[..7].into()).or_default() += e.listened_sec;
                hours[local.hour() as usize] += e.listened_sec;
                dates.insert(local.date_naive());
            }
        }
        let (mut longest, mut streak, mut previous) = (0, 0, None);
        for day in &dates {
            streak = if previous.is_some_and(|p| *day - chrono::Duration::days(1) == p) {
                streak + 1
            } else {
                1
            };
            longest = longest.max(streak);
            previous = Some(*day);
        }
        let mut current = 0;
        let mut check = today;
        if !dates.contains(&check) {
            check -= chrono::Duration::days(1);
        }
        while dates.contains(&check) {
            current += 1;
            check -= chrono::Duration::days(1);
        }
        let ranked = |map: BTreeMap<String, Value>| {
            let mut values = map.into_values().collect::<Vec<_>>();
            values.sort_by(|a, b| {
                b["count"].as_u64().cmp(&a["count"].as_u64()).then_with(|| {
                    b["seconds"]
                        .as_f64()
                        .partial_cmp(&a["seconds"].as_f64())
                        .unwrap()
                })
            });
            values
        };
        let milestones = [50, 200, 500, 1000, 2000, 5000]
            .map(|count| json!({"count":count,"reached":events.len()>=count}));
        json!({"seconds":seconds,"count":events.len(),"uniqueTracks":tracks.len(),"uniqueArtists":artists.len(),
            "tracks":ranked(tracks),"artists":ranked(artists),"days":days,"months":months,"hours":hours,
            "longestStreak":longest,"currentStreak":current,"milestones":milestones})
    }
}
fn add(v: &mut Value, seconds: f64) {
    v["count"] = json!(v["count"].as_u64().unwrap() + 1);
    v["seconds"] = json!(v["seconds"].as_f64().unwrap() + seconds);
}
