//! Reference JSON field names, independent of the richer SDK Track used by playback.
use crate::document::Document;
use chrono::{Datelike, Local, TimeZone};
use haiku_model::{Album, Artist, Track};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StoredTrack {
    #[serde(alias = "source")]
    pub source_id: String,
    pub id: String,
    pub provider: String,
    pub title: String,
    pub artists: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_headers: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl StoredTrack {
    pub fn key(&self) -> String {
        self.track().entity_ref().encode()
    }
    pub fn track(&self) -> Track {
        Track {
            source: self.source_id.clone(),
            id: self.id.clone(),
            title: self.title.clone(),
            artists: self.artists.iter().cloned().map(Artist::Name).collect(),
            album: self.album.as_ref().map(|title| Album {
                id: None,
                title: title.clone(),
            }),
            cover: self.cover.clone(),
            duration: self.duration,
            local_path: self.local_path.clone(),
            quality: None,
            explicit: None,
            extra: self.extra.get("extra").cloned(),
        }
    }
}
impl From<&Track> for StoredTrack {
    fn from(t: &Track) -> Self {
        Self {
            source_id: t.source.clone(),
            id: t.id.clone(),
            provider: t.source.clone(),
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
            duration: t.duration,
            local_path: t.local_path.clone(),
            extra: t
                .extra
                .as_ref()
                .map(|v| BTreeMap::from([("extra".into(), v.clone())]))
                .unwrap_or_default(),
            ..Default::default()
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackHistory {
    #[serde(flatten)]
    pub track: StoredTrack,
    pub played_at: i64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlaylistHistory {
    pub kind: String,
    pub source_id: String,
    pub id: String,
    pub title: String,
    pub cover: Option<String>,
    pub cover_headers: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_count: Option<u32>,
    pub played_at: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HistoryState {
    pub schema_version: u32,
    pub tracks: Vec<TrackHistory>,
    pub playlists: Vec<PlaylistHistory>,
    pub searches: Vec<String>,
}
impl Default for HistoryState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            tracks: vec![],
            playlists: vec![],
            searches: vec![],
        }
    }
}
#[derive(Clone)]
pub struct History(Document<HistoryState>);
impl History {
    pub fn open(root: &Path) -> Result<Self, String> {
        let this = Self(Document::open(root.join("history.json"))?);
        this.0.edit(normalize);
        Ok(this)
    }
    pub fn snapshot(&self) -> HistoryState {
        self.0.get()
    }
    pub fn revision(&self) -> u64 {
        self.0.revision()
    }
    pub fn recovery(&self) -> Option<String> {
        self.0.recovery()
    }
    pub fn flush(&self) -> Result<(), String> {
        self.0.flush()
    }
    pub fn error(&self) -> Option<String> {
        self.0.error()
    }
    pub fn record_track(&self, track: &Track, played_at: i64) {
        self.0.edit(|s| {
            s.tracks.insert(
                0,
                TrackHistory {
                    track: track.into(),
                    played_at,
                },
            );
            normalize(s);
        });
    }
    pub fn record_playlist(&self, entry: PlaylistHistory) {
        self.0.edit(|s| {
            s.playlists.insert(0, entry);
            normalize(s);
        });
    }
    pub fn search(&self, query: &str) {
        let query = query.trim();
        if query.is_empty() {
            return;
        }
        self.0.edit(|s| {
            s.searches.retain(|q| q != query);
            s.searches.insert(0, query.into());
            s.searches.truncate(30);
        });
    }
    pub fn remove(&self, kind: &str, key: &str) {
        self.0.edit(|s| match kind {
            "tracks" => s.tracks.retain(|t| t.track.key() != key),
            "playlists" => s.playlists.retain(|p| playlist_key(p) != key),
            "searches" => s.searches.retain(|q| q != key),
            _ => (),
        });
    }
    pub fn clear(&self, kind: &str) {
        self.0.edit(|s| match kind {
            "tracks" => s.tracks.clear(),
            "playlists" => s.playlists.clear(),
            "searches" => s.searches.clear(),
            _ => (),
        });
    }
    pub fn tracks(&self, search: &str, source: &str) -> Vec<TrackHistory> {
        let needle = search.to_lowercase();
        self.snapshot()
            .tracks
            .into_iter()
            .filter(|t| {
                (source.is_empty() || t.track.source_id == source)
                    && format!(
                        "{} {} {}",
                        t.track.title,
                        t.track.artists.join(" "),
                        t.track.album.as_deref().unwrap_or("")
                    )
                    .to_lowercase()
                    .contains(&needle)
            })
            .collect()
    }
}
pub fn playlist_key(p: &PlaylistHistory) -> String {
    serde_json::to_string(&(&p.kind, &p.source_id, &p.id)).unwrap()
}
fn normalize(s: &mut HistoryState) {
    let mut seen = BTreeSet::new();
    s.tracks.sort_by_key(|t| std::cmp::Reverse(t.played_at));
    s.tracks.retain(|t| {
        !t.track.id.is_empty() && !t.track.source_id.is_empty() && seen.insert(t.track.key())
    });
    s.tracks.truncate(500);
    seen.clear();
    s.playlists.sort_by_key(|p| std::cmp::Reverse(p.played_at));
    s.playlists
        .retain(|p| !p.id.is_empty() && seen.insert(playlist_key(p)));
    s.playlists.truncate(100);
    seen.clear();
    s.searches
        .retain(|q| !q.trim().is_empty() && seen.insert(q.clone()));
    s.searches.truncate(30);
}
pub fn group(epoch: i64, now: i64) -> &'static str {
    let date = |t| Local.timestamp_opt(t, 0).single().map(|d| d.date_naive());
    let (Some(day), Some(today)) = (date(epoch), date(now)) else {
        return "更早";
    };
    if day >= today {
        "今天"
    } else if Some(day) == today.pred_opt() {
        "昨天"
    } else if day >= today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64) {
        "本周"
    } else {
        "更早"
    }
}
