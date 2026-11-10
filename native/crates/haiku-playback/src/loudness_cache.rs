//! Measurements retain their own entity identity, including late reports after a skip.
use haiku_audio::{
    loudness::{LoudnessReport, gain_db},
    playback::LoudnessOptions,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};
#[derive(Clone, Serialize, Deserialize)]
struct Record {
    lufs: f64,
    peak: f64,
    secs: f64,
    complete: bool,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Records {
    schema_version: u32,
    records: BTreeMap<String, Record>,
}
pub struct LoudnessCache {
    path: Option<PathBuf>,
    records: Records,
}
impl LoudnessCache {
    pub fn new(path: Option<PathBuf>) -> Self {
        let records = path
            .as_ref()
            .and_then(|p| haiku_store::json_file::read(p).ok().flatten())
            .unwrap_or_default();
        Self { path, records }
    }
    pub fn options(&self, key: String, duration: f64, enabled: bool) -> LoudnessOptions {
        let known = self
            .records
            .records
            .get(&key)
            .filter(|r| r.complete || r.secs >= 60. || (duration > 0. && r.secs >= duration * 0.5))
            .and_then(|r| gain_db(r.lufs, r.peak, -18.));
        LoudnessOptions {
            enabled,
            target_lufs: -18.,
            known_gain_db: if enabled { known } else { None },
            measure: enabled && known.is_none(),
            track_key: key,
        }
    }
    pub fn record(&mut self, key: String, r: LoudnessReport) -> Result<(), String> {
        if key.is_empty()
            || !r.lufs.is_finite()
            || !r.peak.is_finite()
            || r.peak < 0.
            || !r.measured_secs.is_finite()
            || r.measured_secs < 0.
        {
            return Ok(());
        }
        if let Some(old) = self.records.records.get(&key) {
            if (old.complete && !r.complete)
                || (old.complete == r.complete && old.secs >= r.measured_secs)
            {
                return Ok(());
            }
        }
        self.records.schema_version = 1;
        self.records.records.insert(
            key,
            Record {
                lufs: r.lufs,
                peak: r.peak,
                secs: r.measured_secs,
                complete: r.complete,
            },
        );
        if let Some(path) = &self.path {
            haiku_store::json_file::write(path, &self.records).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coverage_trust_and_better_measurements_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("loudness.json");
        let mut c = LoudnessCache::new(Some(path.clone()));
        let mut r = LoudnessReport {
            lufs: -24.,
            peak: 0.3,
            measured_secs: 20.,
            complete: false,
        };
        c.record("a".into(), r).unwrap();
        assert!(c.options("a".into(), 100., true).known_gain_db.is_none());
        assert!(c.options("a".into(), 40., true).known_gain_db.is_some());
        r.measured_secs = 60.;
        c.record("a".into(), r).unwrap();
        assert!(c.options("a".into(), 500., true).known_gain_db.is_some());
        r.measured_secs = 5.;
        r.lufs = -8.;
        c.record("a".into(), r).unwrap();
        assert_eq!(c.options("a".into(), 500., true).known_gain_db, Some(6.));
        r.complete = true;
        c.record("a".into(), r).unwrap();
        r.complete = false;
        r.measured_secs = 200.;
        c.record("a".into(), r).unwrap();
        let c = LoudnessCache::new(Some(path));
        assert_eq!(c.options("a".into(), 500., true).known_gain_db, Some(-10.));
        assert!(c.options("b".into(), 1., true).measure);
    }
}
