//! Reference-compatible shortcut identities and platform-independent validation.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub const ACTIONS: &[(&str, &str, &str)] = &[
    ("closeWindow", "关闭窗口", "meta+w"),
    ("openCommandPalette", "命令面板", "meta+k"),
    ("navigateBack", "后退", "meta+["),
    ("navigateForward", "前进", "meta+]"),
    ("openSettings", "打开设置", "meta+,"),
    ("togglePlayback", "播放 / 暂停", "space"),
    ("nextTrack", "下一首", "meta+arrowright"),
    ("previousTrack", "上一首", "meta+arrowleft"),
    ("volumeUp", "增大音量", "meta+arrowup"),
    ("volumeDown", "减小音量", "meta+arrowdown"),
    ("toggleLiked", "喜欢歌曲", "meta+l"),
    ("toggleDesktopLyrics", "显示 / 隐藏桌面歌词", "meta+r"),
];
pub const GLOBAL_ACTIONS: &[&str] = &[
    "togglePlayback",
    "previousTrack",
    "nextTrack",
    "volumeUp",
    "volumeDown",
    "toggleLiked",
    "showMainWindow",
    "toggleDesktopLyrics",
];
/// One binding in an entire replacement group. Empty global chords mean unbound.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub id: String,
    pub scope: String,
    pub chord: String,
}

/// Canonical reference spelling; arrow aliases and modifier aliases remain compatible.
pub fn normalize(chord: &str) -> Result<String, String> {
    let mut mods = HashSet::new();
    let mut key = None;
    for word in chord.trim().to_lowercase().split('+') {
        let word = match word.trim() {
            "cmd" | "command" | "super" | "win" => "meta",
            "ctrl" => "control",
            "opt" | "option" => "alt",
            "mod" => {
                if cfg!(target_os = "macos") {
                    "meta"
                } else {
                    "control"
                }
            }
            "left" => "arrowleft",
            "right" => "arrowright",
            "up" => "arrowup",
            "down" => "arrowdown",
            "esc" => "escape",
            "return" => "enter",
            "plus" => "=",
            "minus" => "-",
            other => other,
        };
        if ["meta", "control", "alt", "shift"].contains(&word) {
            mods.insert(word.to_owned());
        } else if word.is_empty() || key.replace(word.to_owned()).is_some() {
            return Err("快捷键必须包含一个按键".into());
        }
    }
    let key = key.ok_or("快捷键缺少按键")?;
    if key_code(&key).is_none() {
        return Err("不支持此按键".into());
    }
    let mut parts: Vec<String> = ["meta", "control", "alt", "shift"]
        .into_iter()
        .filter(|m| mods.contains(*m))
        .map(str::to_owned)
        .collect();
    parts.push(key);
    Ok(parts.join("+"))
}
/// Carbon US-layout virtual key code. F1 through F20 have explicit physical codes.
pub fn key_code(key: &str) -> Option<u32> {
    let letters = [
        ("a", 0),
        ("s", 1),
        ("d", 2),
        ("f", 3),
        ("h", 4),
        ("g", 5),
        ("z", 6),
        ("x", 7),
        ("c", 8),
        ("v", 9),
        ("b", 11),
        ("q", 12),
        ("w", 13),
        ("e", 14),
        ("r", 15),
        ("y", 16),
        ("t", 17),
        ("1", 18),
        ("2", 19),
        ("3", 20),
        ("4", 21),
        ("6", 22),
        ("5", 23),
        ("=", 24),
        ("9", 25),
        ("7", 26),
        ("-", 27),
        ("8", 28),
        ("0", 29),
        ("]", 30),
        ("o", 31),
        ("u", 32),
        ("[", 33),
        ("i", 34),
        ("p", 35),
        ("enter", 36),
        ("l", 37),
        ("j", 38),
        ("'", 39),
        ("k", 40),
        (";", 41),
        ("\\", 42),
        (",", 43),
        ("/", 44),
        ("n", 45),
        ("m", 46),
        (".", 47),
        ("tab", 48),
        ("space", 49),
        ("`", 50),
        ("backspace", 51),
        ("escape", 53),
        ("delete", 117),
        ("arrowleft", 123),
        ("arrowright", 124),
        ("arrowdown", 125),
        ("arrowup", 126),
    ];
    if let Some(f) = key
        .strip_prefix('f')
        .and_then(|n| n.parse::<usize>().ok())
        .filter(|n| (1..=20).contains(n))
    {
        return Some(
            [
                122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111, 105, 107, 113, 106, 64, 79,
                80, 90,
            ][f - 1],
        );
    }
    letters.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}
pub fn defaults() -> Vec<Binding> {
    ACTIONS
        .iter()
        .map(|(id, _, chord)| Binding {
            id: (*id).into(),
            scope: "app".into(),
            chord: if cfg!(target_os = "macos") {
                (*chord).into()
            } else {
                chord.replace("meta+", "control+")
            },
        })
        .chain(GLOBAL_ACTIONS.iter().map(|id| Binding {
            id: (*id).into(),
            scope: "global".into(),
            chord: String::new(),
        }))
        .collect()
}
pub fn effective(
    app: &BTreeMap<String, String>,
    global: &BTreeMap<String, String>,
) -> Vec<Binding> {
    defaults()
        .into_iter()
        .map(|mut b| {
            if let Some(c) = if b.scope == "app" { app } else { global }.get(&b.id) {
                b.chord = c.clone();
            }
            b
        })
        .collect()
}
/// Validate the complete group before any live registrations change.
pub fn validate(bindings: &[Binding]) -> Result<Vec<Binding>, String> {
    let expected = defaults();
    if bindings.len() != expected.len() {
        return Err("必须替换整组快捷键".into());
    }
    let mut ids = HashSet::new();
    let mut chords = HashSet::new();
    let mut result = Vec::new();
    for b in bindings {
        if !expected.iter().any(|e| e.id == b.id && e.scope == b.scope)
            || !ids.insert((b.scope.clone(), b.id.clone()))
        {
            return Err("未知或重复的快捷键操作".into());
        }
        let mut b = b.clone();
        if b.chord.is_empty() && b.scope == "global" {
            result.push(b);
            continue;
        }
        b.chord = normalize(&b.chord)?;
        let parts: Vec<_> = b.chord.split('+').collect();
        if b.scope == "global"
            && !parts.iter().any(|p| ["meta", "control", "alt"].contains(p))
            && !parts
                .last()
                .is_some_and(|k| k.starts_with('f') && k[1..].parse::<u8>().is_ok())
        {
            return Err("全局快捷键需要 Cmd、Ctrl、Option 或 F1–F20".into());
        }
        if [
            "meta+q",
            "meta+h",
            "meta+alt+h",
            "meta+m",
            "meta+control+f",
            "control+q",
            "alt+f4",
            "meta+c",
            "meta+x",
            "meta+v",
            "meta+a",
            "meta+z",
            "meta+shift+z",
        ]
        .contains(&b.chord.as_str())
        {
            return Err("快捷键冲突：此组合由系统或编辑菜单使用".into());
        }
        if !chords.insert(b.chord.clone()) {
            return Err("快捷键冲突：组合已被应用内或全局操作使用".into());
        }
        result.push(b);
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validation_and_reference_aliases() {
        let mut b = defaults();
        let at = b.iter().position(|b| b.scope == "global").unwrap();
        for c in ["a", "shift+a"] {
            b[at].chord = c.into();
            assert!(validate(&b).is_err());
        }
        for f in 1..=20 {
            b[at].chord = format!("f{f}");
            assert!(validate(&b).is_ok());
        }
        b[at].chord = "cmd+k".into();
        assert!(validate(&b).is_err());
        b[at].chord = "control+alt+p".into();
        b[at + 1].chord = "ctrl+option+p".into();
        assert!(validate(&b).is_err());
        assert_eq!(normalize("cmd+right").unwrap(), "meta+arrowright");
        b = defaults();
        b[2].chord = "alt+arrowleft".into();
        assert!(validate(&b).is_ok());
    }
}
