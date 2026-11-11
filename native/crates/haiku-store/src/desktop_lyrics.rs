//! Reference-compatible desktopLyrics settings; visibility/lock join the same authority.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DesktopLyrics {
    pub visible: bool,
    pub locked: bool,
    pub text_color: String,
    pub highlight_color: String,
    pub outline: bool,
    /// Absent in reference settings: retain the old outline/shadow linkage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow: Option<bool>,
    pub background: String,
    pub font_size: f64,
    pub font: String,
    pub font_name: String,
    pub font_weight: String,
    pub layout: String,
    pub show_translation: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl Default for DesktopLyrics {
    fn default() -> Self {
        Self {
            visible: false,
            locked: false,
            text_color: "#FFFFFF".into(),
            highlight_color: "#AC5E4F".into(),
            outline: true,
            shadow: None,
            background: "none".into(),
            font_size: 28.,
            font: String::new(),
            font_name: String::new(),
            font_weight: "regular".into(),
            layout: "classic".into(),
            show_translation: true,
            extra: BTreeMap::new(),
        }
    }
}
pub const WEIGHTS: [(&str, f64); 5] = [
    ("light", -0.4),
    ("regular", 0.),
    ("medium", 0.23),
    ("semibold", 0.3),
    ("bold", 0.4),
];
pub fn nearest_weight(weight: f64) -> &'static str {
    WEIGHTS
        .iter()
        .min_by(|a, b| (a.1 - weight).abs().total_cmp(&(b.1 - weight).abs()))
        .unwrap()
        .0
}
pub fn valid_color(color: &str) -> bool {
    color == "system"
        || (color.len() == 7
            && color.starts_with('#')
            && color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit))
}
impl DesktopLyrics {
    pub fn weight(&self) -> f64 {
        WEIGHTS
            .iter()
            .find(|(id, _)| *id == self.font_weight)
            .map(|(_, w)| *w)
            .unwrap_or(0.)
    }
    pub fn normalize(&mut self) {
        if !valid_color(&self.text_color) {
            self.text_color = "#FFFFFF".into();
        }
        if !valid_color(&self.highlight_color) {
            self.highlight_color = "#AC5E4F".into();
        }
        self.font_size = if self.font_size.is_finite() {
            self.font_size.clamp(14., 60.)
        } else {
            28.
        };
        if self.background == "blur" {
            self.background = "liquid".into();
        }
        if !["none", "solid", "liquid"].contains(&self.background.as_str()) {
            self.background = "none".into();
        }
        if !["classic", "focus", "left", "stage"].contains(&self.layout.as_str()) {
            self.layout = if self.extra.get("columns").and_then(Value::as_str) == Some("single") {
                "focus"
            } else {
                "classic"
            }
            .into();
        }
        if !WEIGHTS.iter().any(|(id, _)| *id == self.font_weight) {
            self.font_weight = "regular".into();
        }
    }
}
#[cfg(test)]
mod tests {
    use crate::settings::SettingsStore;
    use serde_json::json;
    #[test]
    fn reference_fields_merge_reopen_and_keep_unknown_values() {
        let dir = tempfile::tempdir().unwrap();
        let s = SettingsStore::open(dir.path()).unwrap();
        s.update(json!({"desktopLyrics":{"visible":true,"locked":true,"font":"PingFang SC","fontName":"PingFangSC-Semibold","fontSize":90,"fontWeight":"semibold","background":"blur","layout":"stage","outline":false,"showTranslation":false,"textColor":"#abCDef","highlightColor":"system","future":17}})).unwrap();
        s.update(json!({"desktopLyrics":{"shadow":true}})).unwrap();
        let d = SettingsStore::open(dir.path())
            .unwrap()
            .get()
            .desktop_lyrics;
        assert!(d.visible && d.locked);
        assert_eq!(d.font_size, 60.);
        assert_eq!(d.background, "liquid");
        assert_eq!(d.layout, "stage");
        assert_eq!(d.weight(), 0.3);
        assert_eq!(d.font, "PingFang SC");
        assert_eq!(d.font_name, "PingFangSC-Semibold");
        assert!(!d.outline && !d.show_translation);
        assert_eq!(d.shadow, Some(true));
        assert_eq!(d.extra["future"], 17);
        assert_eq!(d.text_color, "#abCDef");
        assert_eq!(d.highlight_color, "system");
        let d=s.update(json!({"desktopLyrics":{"fontSize":1,"layout":"bad","columns":"single","fontWeight":"bad","textColor":"bad"}})).unwrap().desktop_lyrics;
        assert_eq!(d.font_size, 14.);
        assert_eq!(d.layout, "focus");
        assert_eq!(d.font_weight, "regular");
        assert_eq!(d.text_color, "#FFFFFF");
    }
}
