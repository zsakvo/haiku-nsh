//! Pure desktop-lyrics projection and native-event mapping. No AppKit in this module.
use haiku_lyrics::Lyrics;
use haiku_macos::desktop_lyrics::{DesktopLyricsEvent, LyricLine, Style, geometry::Position};
use haiku_model::Track;
use haiku_playback::{Controller, PlayerState, Status};
use haiku_store::{collections::Collections, desktop_lyrics::DesktopLyrics};
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct View {
    pub current: Option<LyricLine>,
    pub next: Option<LyricLine>,
    pub start: f64,
    pub style: Style,
    pub visible: bool,
    pub locked: bool,
    pub playing: bool,
    pub favorite: bool,
    pub timeline: (u64, u64),
}
#[derive(Default, Debug, PartialEq)]
pub(crate) struct Changes {
    pub lines: bool,
    pub progress: bool,
    pub style: bool,
    pub visible: bool,
    pub locked: bool,
    pub playing: bool,
    pub favorite: bool,
}
#[derive(Default)]
pub(crate) struct Projection(Option<View>);
impl Projection {
    pub fn update(&mut self, v: View) -> Changes {
        let old = self.0.as_ref();
        let showing = old.is_none_or(|o| o.visible != v.visible) && v.visible;
        let lines = showing
            || old.is_none_or(|o| {
                o.current != v.current
                    || o.next != v.next
                    || o.start != v.start
                    || o.timeline.0 != v.timeline.0
            });
        let playing = old.is_none_or(|o| o.playing != v.playing);
        let c = Changes {
            lines,
            progress: !lines
                && (showing || playing || old.is_none_or(|o| o.timeline != v.timeline)),
            style: showing || old.is_none_or(|o| o.style != v.style),
            visible: old.is_none_or(|o| o.visible != v.visible),
            locked: showing || old.is_none_or(|o| o.locked != v.locked),
            playing: showing || playing,
            favorite: showing || old.is_none_or(|o| o.favorite != v.favorite),
        };
        self.0 = Some(v);
        c
    }
}
pub(crate) fn style(d: &DesktopLyrics) -> Style {
    let rgb = |s: &str| {
        s.strip_prefix('#')
            .and_then(|s| u32::from_str_radix(s, 16).ok())
            .map(|v| {
                (
                    ((v >> 16) & 255) as f64 / 255.,
                    ((v >> 8) & 255) as f64 / 255.,
                    (v & 255) as f64 / 255.,
                )
            })
    };
    Style {
        text_color: rgb(&d.text_color),
        highlight_color: rgb(&d.highlight_color),
        outline: d.outline,
        shadow: d.shadow.unwrap_or(d.outline),
        background: d.background.clone(),
        font_size: d.font_size,
        font_family: if d.font == "system" {
            String::new()
        } else {
            d.font.clone()
        },
        font_name: d.font_name.clone(),
        font_weight: d.weight(),
        layout: d.layout.clone(),
        show_translation: d.show_translation,
    }
}
pub(crate) fn project(
    s: &PlayerState,
    track: Option<&Track>,
    lyrics: &Lyrics,
    d: &DesktopLyrics,
    collections: &Collections,
    pos: f64,
) -> (View, Option<f64>) {
    let at = if s.current.is_some() && lyrics.synced {
        lyrics.lines.iter().rposition(|l| l.at <= pos)
    } else {
        None
    };
    let map = |l: &haiku_lyrics::LyricLine| LyricLine {
        text: l.text.clone(),
        translation: l.translation.clone().unwrap_or_default(),
        words: l
            .words
            .iter()
            .map(|w| (w.start, w.end, w.text.clone()))
            .collect(),
    };
    let current = at
        .map(|i| &lyrics.lines[i])
        .filter(|l| !l.text.trim().is_empty());
    let (current, next, start) = if let Some(l) = current {
        (
            Some(map(l)),
            at.and_then(|i| lyrics.lines.get(i + 1)).map(map),
            l.at,
        )
    } else {
        let t = track
            .filter(|t| {
                s.current
                    .as_ref()
                    .is_some_and(|c| c.entity_ref() == t.entity_ref())
            })
            .or(s.current.as_ref());
        (
            Some(LyricLine {
                text: t.map(|t| t.title.clone()).unwrap_or_else(|| "Haiku".into()),
                translation: t
                    .map(|t| {
                        t.artists
                            .iter()
                            .map(|a| match a {
                                haiku_model::Artist::Name(n)
                                | haiku_model::Artist::Entity { name: n, .. } => n.as_str(),
                            })
                            .collect::<Vec<_>>()
                            .join(" / ")
                    })
                    .unwrap_or_default(),
                words: vec![],
            }),
            None,
            0.,
        )
    };
    let playing = s.status == Status::Playing && !s.buffering;
    let wake = if d.visible && playing && lyrics.synced && s.current.is_some() {
        lyrics.lines.iter().find(|l| l.at > pos).map(|l| l.at - pos)
    } else {
        None
    };
    (
        View {
            current,
            next,
            start,
            style: style(d),
            visible: d.visible,
            locked: d.locked,
            playing,
            favorite: s
                .current
                .as_ref()
                .is_some_and(|t| collections.is_liked(&t.entity_ref())),
            timeline: (s.generation, s.timeline_revision),
        },
        wake,
    )
}
#[derive(Debug, PartialEq)]
pub(crate) enum Action {
    None,
    Patch(serde_json::Value),
    ShowApp,
    Settings,
    Position(Position),
}
pub(crate) async fn dispatch(
    c: &Controller,
    collections: &Collections,
    event: DesktopLyricsEvent,
) -> Result<Action, String> {
    use DesktopLyricsEvent::*;
    let event = match event {
        Prev => haiku_macos::menubar::MenuBarEvent::Prev,
        Toggle => haiku_macos::menubar::MenuBarEvent::Toggle,
        Next => haiku_macos::menubar::MenuBarEvent::Next,
        ToggleFavorite => haiku_macos::menubar::MenuBarEvent::ToggleFavorite,
        Lock => {
            return Ok(Action::Patch(
                serde_json::json!({"desktopLyrics":{"locked":true}}),
            ));
        }
        Unlock => {
            return Ok(Action::Patch(
                serde_json::json!({"desktopLyrics":{"locked":false}}),
            ));
        }
        Close => {
            return Ok(Action::Patch(
                serde_json::json!({"desktopLyrics":{"visible":false}}),
            ));
        }
        ShowApp => return Ok(Action::ShowApp),
        Settings => return Ok(Action::Settings),
        Position(p) => return Ok(Action::Position(p)),
    };
    crate::menubar::dispatch(c, collections, event).await?;
    Ok(Action::None)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_values_only_and_show_reanchors_hidden_state() {
        let d = DesktopLyrics::default();
        let view = View {
            current: Some(LyricLine {
                text: "one".into(),
                translation: "译".into(),
                words: vec![],
            }),
            next: None,
            start: 0.,
            style: style(&d),
            visible: true,
            locked: false,
            playing: true,
            favorite: false,
            timeline: (1, 0),
        };
        let mut projection = Projection::default();
        assert!(projection.update(view.clone()).lines);
        assert_eq!(projection.update(view.clone()), Changes::default());
        let mut v = view.clone();
        v.playing = false;
        let changes = projection.update(v.clone());
        assert!(changes.progress && changes.playing && !changes.lines);
        v.timeline.1 += 1;
        assert!(projection.update(v.clone()).progress);
        v.visible = false;
        assert!(projection.update(v.clone()).visible);
        v.visible = true;
        assert!(projection.update(v.clone()).lines);
        v.current.as_mut().unwrap().text = "two".into();
        assert!(projection.update(v.clone()).lines);
        v.timeline.0 += 1;
        assert!(projection.update(v).lines);
    }
}
