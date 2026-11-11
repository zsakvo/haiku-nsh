//! Reference-compatible settings, serialized merges and durable publication.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AudioEffects {
    pub enabled: bool,
    pub band_gains_db: [f32; 10],
    pub preset_id: String,
    pub custom_presets: Vec<EqPreset>,
    pub auto_preamp: bool,
    pub preamp_db: f32,
    pub stereo_width: f32,
    pub balance: f32,
    pub mono: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EqPreset {
    pub id: String,
    pub name: String,
    pub gains: [f32; 10],
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl Default for AudioEffects {
    fn default() -> Self {
        Self {
            enabled: false,
            band_gains_db: [0.; 10],
            preset_id: "flat".into(),
            custom_presets: vec![],
            auto_preamp: true,
            preamp_db: 0.,
            stereo_width: 1.,
            balance: 0.,
            mono: false,
            extra: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub schema_version: u32,
    pub theme_mode: String,
    pub brand_color: String,
    pub audio_effects: AudioEffects,
    pub volume_normalization: bool,
    pub prevent_screen_sleep_on_player: bool,
    pub media_cache_enabled: bool,
    pub media_cache_max_bytes: u64,
    pub audio_quality: String,
    pub download_dir: String,
    pub download_quality: String,
    pub download_naming: String,
    pub download_layout: String,
    pub download_cover_mode: String,
    pub download_cover_hd: bool,
    pub download_lyrics_mode: String,
    pub download_lyrics_translation: bool,
    pub download_concurrency: u32,
    pub download_wifi_only: bool,
    pub download_reuse_media_cache: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            theme_mode: "light".into(),
            brand_color: "orange".into(),
            audio_effects: AudioEffects::default(),
            volume_normalization: true,
            prevent_screen_sleep_on_player: false,
            media_cache_enabled: true,
            media_cache_max_bytes: 2 * 1024 * 1024 * 1024,
            audio_quality: "high".into(),
            download_dir: String::new(),
            download_quality: "lossless".into(),
            download_naming: "{artist} - {title}".into(),
            download_layout: "flat".into(),
            download_cover_mode: "embed".into(),
            download_cover_hd: true,
            download_lyrics_mode: "both".into(),
            download_lyrics_translation: true,
            download_concurrency: 3,
            download_wifi_only: true,
            download_reuse_media_cache: true,
            extra: BTreeMap::new(),
        }
    }
}
fn clamp(v: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(min, max)
    } else {
        fallback
    }
}
impl Settings {
    pub fn normalize(&mut self) {
        self.schema_version = self.schema_version.max(1);
        if !["light", "dark", "system"].contains(&self.theme_mode.as_str()) {
            self.theme_mode = "light".into();
        }
        if ![
            "red", "orange", "amber", "yellow", "lime", "green", "emerald", "teal", "cyan", "sky",
            "blue", "indigo", "violet", "purple", "fuchsia", "pink", "rose",
        ]
        .contains(&self.brand_color.as_str())
        {
            self.brand_color = "orange".into();
        }
        if !["standard", "high", "lossless", "hires"].contains(&self.audio_quality.as_str()) {
            self.audio_quality = "high".into();
        }
        self.download_concurrency = self.download_concurrency.clamp(1, 5);
        if !["standard", "high", "lossless", "hires"].contains(&self.download_quality.as_str()) {
            self.download_quality = "lossless".into();
        }
        if !["flat", "artist", "artist-album", "playlist", "provider"]
            .contains(&self.download_layout.as_str())
        {
            self.download_layout = "flat".into();
        }
        for mode in [
            &mut self.download_cover_mode,
            &mut self.download_lyrics_mode,
        ] {
            if !["none", "embed", "sidecar", "both"].contains(&mode.as_str()) {
                *mode = "none".into();
            }
        }
        if self.download_naming.trim().is_empty() {
            self.download_naming = "{artist} - {title}".into();
        }
        self.media_cache_max_bytes = self
            .media_cache_max_bytes
            .clamp(512 * 1024 * 1024, 20 * 1024 * 1024 * 1024);
        let e = &mut self.audio_effects;
        e.band_gains_db
            .iter_mut()
            .for_each(|v| *v = clamp(*v, -12., 12., 0.));
        for preset in &mut e.custom_presets {
            preset
                .gains
                .iter_mut()
                .for_each(|v| *v = clamp(*v, -12., 12., 0.));
        }
        e.preamp_db = clamp(e.preamp_db, -12., 12., 0.);
        e.stereo_width = clamp(e.stereo_width, 0., 2., 1.);
        e.balance = clamp(e.balance, -1., 1., 0.);
    }
}
fn merge(target: &mut Value, patch: Value) {
    if let (Some(dst), Value::Object(src)) = (target.as_object_mut(), &patch) {
        for (key, value) in src {
            if let Some(old) = dst.get_mut(key) {
                merge(old, value.clone());
            } else {
                dst.insert(key.clone(), value.clone());
            }
        }
    } else {
        *target = patch;
    }
}
// Clamp before deserializing unsigned fields so negative JSON numbers remain recoverable preferences.
fn clamp_unsigned_fields(raw: &mut Value) {
    if let Some(n) = raw.get("downloadConcurrency").and_then(Value::as_f64) {
        raw["downloadConcurrency"] = Value::from(n.round().clamp(1., 5.) as u32);
    }
    if let Some(bytes) = raw.get("mediaCacheMaxBytes").and_then(Value::as_f64) {
        raw["mediaCacheMaxBytes"] =
            Value::from(bytes.clamp(512. * 1024. * 1024., 20. * 1024. * 1024. * 1024.) as u64);
    }
}
pub struct SettingsStore {
    path: PathBuf,
    value: Mutex<Settings>,
    recovery: Option<String>,
}
impl SettingsStore {
    pub fn open(root: &Path) -> Result<Self, String> {
        let path = root.join("settings.json");
        let mut recovery = None;
        let mut value = match crate::json_file::read::<Value>(&path).and_then(|raw| {
            raw.map(|mut raw| {
                clamp_unsigned_fields(&mut raw);
                serde_json::from_value::<Settings>(raw).map_err(std::io::Error::other)
            })
            .transpose()
        }) {
            Ok(value) => value.unwrap_or_default(),
            Err(error) => {
                let suffix = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                std::fs::rename(&path, path.with_extension(format!("json.corrupt-{suffix}")))
                    .map_err(|e| e.to_string())?;
                eprintln!("settings.json corrupt: {error}; original backed up");
                recovery = Some(format!("settings.json 损坏，原文件已备份：{error}"));
                Settings::default()
            }
        };
        value.normalize();
        Ok(Self {
            path,
            value: Mutex::new(value),
            recovery,
        })
    }
    pub fn recovery_warning(&self) -> Option<String> {
        self.recovery.clone()
    }
    pub fn get(&self) -> Settings {
        self.value.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    /// Merge against the latest committed value; publish only after atomic replacement succeeds.
    pub fn update(&self, patch: Value) -> Result<Settings, String> {
        if !patch.is_object() {
            return Err("Settings patch must be an object".into());
        }
        let mut guard = self.value.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = serde_json::to_value(&*guard).map_err(|e| e.to_string())?;
        merge(&mut next, patch);
        clamp_unsigned_fields(&mut next);
        let mut next: Settings = serde_json::from_value(next).map_err(|e| e.to_string())?;
        next.normalize();
        crate::json_file::write(&self.path, &next).map_err(|e| e.to_string())?;
        *guard = next.clone();
        Ok(next)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn atomic_merge_unknown_fields_clamp_and_reopen() {
        let root = tempfile::tempdir().unwrap();
        let store = SettingsStore::open(root.path()).unwrap();
        store.update(json!({"glassAutoInvert":true,"downloadWifiOnly":false,"future":{"x":1},"audioEffects":{"enabled":true,"preampDb":99,"mobile":7},"mediaCacheMaxBytes":-100})).unwrap();
        store
            .update(json!({"audioEffects":{"balance":-8}}))
            .unwrap();
        let s = SettingsStore::open(root.path()).unwrap().get();
        assert_eq!(s.audio_effects.preamp_db, 12.);
        assert_eq!(s.audio_effects.balance, -1.);
        assert!(s.audio_effects.enabled);
        assert_eq!(s.extra["future"], json!({"x":1}));
        assert_eq!(s.audio_effects.extra["mobile"], 7);
        assert_eq!(s.media_cache_max_bytes, 512 * 1024 * 1024);
        let before = std::fs::read(root.path().join("settings.json")).unwrap();
        assert!(
            store
                .update(json!({"audioEffects":{"enabled":"invalid"}}))
                .is_err()
        );
        assert_eq!(
            before,
            std::fs::read(root.path().join("settings.json")).unwrap()
        );
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
