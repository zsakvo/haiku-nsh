//! Pure status-item projection and event mapping. AppKit lives in haiku-macos.
use haiku_lyrics::Lyrics;
use haiku_macos::menubar::{MenuBarEvent, MenuItemSpec};
use haiku_model::Track;
use haiku_playback::{Command, Controller, PlayerState, Status};
use haiku_store::{collections::Collections, settings::Settings};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Line {
    pub text: String,
    pub words: Vec<(f64, f64, String)>,
    pub delay: f64,
    pub start: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct View {
    pub line: Line,
    pub playing: bool,
    pub favorite: bool,
    pub visible: bool,
    pub highlight: Option<u32>,
    pub menu: Vec<MenuItemSpec>,
    // An explicit timeline revision changes on seek, even within the same lyric line.
    pub timeline: (u64, u64),
}
#[derive(Default, Debug, PartialEq)]
pub(crate) struct Changes {
    pub line: bool,
    pub progress: bool,
    pub playing: bool,
    pub favorite: bool,
    pub visible: bool,
    pub highlight: bool,
    pub menu: bool,
}
#[derive(Default)]
pub(crate) struct Projection(Option<View>);
impl Projection {
    pub fn update(&mut self, view: View) -> Changes {
        let old = self.0.as_ref();
        let line = old.is_none_or(|o| o.line != view.line);
        let playing = old.is_none_or(|o| o.playing != view.playing);
        let changes = Changes {
            line,
            playing,
            progress: !line && (playing || old.is_none_or(|o| o.timeline != view.timeline)),
            favorite: old.is_none_or(|o| o.favorite != view.favorite),
            visible: old.is_none_or(|o| o.visible != view.visible),
            highlight: old.is_none_or(|o| o.highlight != view.highlight),
            menu: old.is_none_or(|o| o.menu != view.menu),
        };
        self.0 = Some(view);
        changes
    }
}
fn item(id: impl Into<String>, title: impl Into<String>, enabled: bool) -> MenuItemSpec {
    MenuItemSpec {
        id: id.into(),
        title: title.into(),
        enabled,
        is_separator: false,
        children: vec![],
    }
}
fn title(t: &Track) -> String {
    let artists = t
        .artists
        .iter()
        .map(|a| match a {
            haiku_model::Artist::Name(n) | haiku_model::Artist::Entity { name: n, .. } => {
                n.as_str()
            }
        })
        .collect::<Vec<_>>()
        .join(" / ");
    if artists.is_empty() {
        t.title.clone()
    } else {
        format!("{} — {artists}", t.title)
    }
}
pub(crate) fn project(
    state: &PlayerState,
    track: Option<&Track>,
    lyrics: &Lyrics,
    settings: &Settings,
    collections: &Collections,
    position: f64,
) -> (View, Option<f64>) {
    let current = if state.current.is_some() && lyrics.synced {
        lyrics.lines.iter().rposition(|l| l.at <= position)
    } else {
        None
    };
    let line = current
        .map(|i| &lyrics.lines[i])
        .filter(|l| !l.text.is_empty())
        .map(|l| Line {
            text: l.text.clone(),
            words: l
                .words
                .iter()
                .map(|w| (w.start, w.end, w.text.clone()))
                .collect(),
            delay: (l.end - l.at).max(0.) / 2.,
            start: l.at,
        })
        .unwrap_or_else(|| Line {
            text: track
                .or(state.current.as_ref())
                .map(title)
                .unwrap_or_else(|| "Haiku".into()),
            words: vec![],
            delay: 1.5,
            start: 0.,
        });
    let playing = state.status == Status::Playing && !state.buffering;
    let next = if playing && lyrics.synced && state.current.is_some() {
        lyrics
            .lines
            .iter()
            .find(|l| l.at > position)
            .map(|l| l.at - position)
    } else {
        None
    };
    let mut queue = item(
        "queue",
        format!("播放队列（{}）", state.tracks.len()),
        !state.tracks.is_empty(),
    );
    queue.children = state
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let token =
                serde_json::to_string(&(state.queue_version, i as i32, t.entity_ref().encode()))
                    .unwrap();
            item(
                format!("queue:{token}"),
                format!(
                    "{}{}",
                    if state.index == i as i32 { "✓ " } else { "" },
                    title(t)
                ),
                true,
            )
        })
        .collect();
    let mut playlists = item("playlists", "加入歌单", state.current.is_some());
    playlists.children = collections
        .list()
        .into_iter()
        .filter(|c| c.kind == "user")
        .map(|c| {
            let token = serde_json::to_string(&(
                c.id,
                state.current.as_ref().map(|t| t.entity_ref().encode()),
            ))
            .unwrap();
            item(
                format!("playlist:{token}"),
                c.title,
                state.current.is_some(),
            )
        })
        .collect();
    if playlists.children.is_empty() {
        playlists.enabled = false;
    }
    let menu = vec![
        queue,
        playlists,
        item("window:show", "显示主窗口", true),
        item(
            "menubar:lyrics",
            if settings.menubar_lyrics_visible {
                "✓ 菜单栏歌词"
            } else {
                "菜单栏歌词"
            },
            true,
        ),
        item("lyrics:show",if settings.desktop_lyrics.visible {"隐藏桌面歌词"}else{"显示桌面歌词"},true),
        item("lyrics:lock",if settings.desktop_lyrics.locked {"解锁桌面歌词"}else{"锁定桌面歌词"},settings.desktop_lyrics.visible),
        item("app:quit", "退出 Haiku", true),
    ];
    let highlight = settings
        .menubar_lyric_highlight
        .strip_prefix('#')
        .and_then(|s| u32::from_str_radix(s, 16).ok());
    (
        View {
            line,
            playing,
            favorite: state
                .current
                .as_ref()
                .is_some_and(|t| collections.is_liked(&t.entity_ref())),
            visible: settings.menubar_lyrics_visible,
            highlight,
            menu,
            timeline: (state.generation, state.timeline_revision),
        },
        next,
    )
}
#[derive(Debug, PartialEq)]
pub(crate) enum HostAction {
    None,
    ShowWindow,
    ToggleLyrics,
    ToggleDesktopLyrics,
    ToggleDesktopLock,
    Quit,
}
/// Execute playback/collection actions via the same authorities as the UI.
pub(crate) async fn dispatch(
    controller: &Controller,
    collections: &Collections,
    event: MenuBarEvent,
) -> Result<HostAction, String> {
    let command = match event {
        MenuBarEvent::Prev => Command::Previous,
        MenuBarEvent::Toggle => Command::Toggle,
        MenuBarEvent::Next => Command::Next,
        MenuBarEvent::ToggleFavorite => {
            if let Some(t) = controller.state().current {
                collections.set_liked(&t, None)?;
            }
            return Ok(HostAction::None);
        }
        MenuBarEvent::MenuItem(id) => {
            match id.as_str() {
                "window:show" => return Ok(HostAction::ShowWindow),
                "menubar:lyrics" => return Ok(HostAction::ToggleLyrics),
                "lyrics:show"=>return Ok(HostAction::ToggleDesktopLyrics),
                "lyrics:lock"=>return Ok(HostAction::ToggleDesktopLock),
                "app:quit" => return Ok(HostAction::Quit),
                _ => {}
            }
            if let Some(token) = id.strip_prefix("queue:") {
                let (version, index, identity) =
                    serde_json::from_str(token).map_err(|_| "无效队列菜单项")?;
                Command::JumpIfQueue {
                    version,
                    index,
                    identity,
                }
            } else if let Some(token) = id.strip_prefix("playlist:") {
                let (id, identity): (String, Option<String>) =
                    serde_json::from_str(token).map_err(|_| "无效歌单菜单项")?;
                if let Some(t) = controller
                    .state()
                    .current
                    .filter(|t| Some(t.entity_ref().encode()) == identity)
                {
                    collections.add(&id, &[t])?;
                }
                return Ok(HostAction::None);
            } else {
                return Ok(HostAction::None);
            }
        }
    };
    controller.execute(command).await.map_err(|e| e.message)?;
    Ok(HostAction::None)
}

#[cfg(test)]
mod tests;
