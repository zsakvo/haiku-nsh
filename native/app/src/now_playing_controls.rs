//! Reference now-playing composition: three transport buttons, progress, and metadata.
use crate::{
    now_playing::NowPlayingEvent, player_controls::now_playing_control, player_dock::send,
    presentation::Presentation, seek_bar::seek_bar,
};
use haiku_playback::{Command, Status};
use nsh_base::theme;
use nsh_core::prelude::*;
use std::rc::Rc;

pub(crate) fn metadata(p: Rc<Presentation>, cx: Scope<NowPlayingEvent>) -> impl IntoElement {
    let track = p.track;
    view()
        .px_3()
        .gap_2()
        .flex_shrink_0()
        .child(
            text(move || track.with(|t| t.as_ref().map(|t| t.title.clone()).unwrap_or_default()))
                .test_id("now-playing-title")
                .text_size(px(26.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || theme().color("now-playing-foreground").unwrap()),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .child(
                    text(move || {
                        track.with(|t| {
                            t.as_ref()
                                .map(|t| {
                                    let artists = haiku_library::service::artist_names(t);
                                    format!(
                                        "专辑：{}   歌手：{}",
                                        t.album
                                            .as_ref()
                                            .map(|a| a.title.as_str())
                                            .unwrap_or("未知专辑"),
                                        if artists.is_empty() {
                                            "未知歌手".into()
                                        } else {
                                            artists.join(" / ")
                                        }
                                    )
                                })
                                .unwrap_or_default()
                        })
                    })
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.))
                    .text_color(move || theme().color("now-playing-muted").unwrap()),
                )
                .child(
                    view()
                        .flex_row()
                        .flex_shrink_0()
                        .gap(px(2.))
                        .child(
                            now_playing_control("编辑歌曲信息", "edit", 26., 15., true)
                                .disabled(move || {
                                    track.with(|t| t.as_ref().is_none_or(|t| t.source != "local"))
                                })
                                .on_press(move || {
                                    if let Some(track) = track.get_untracked() {
                                        cx.emit(NowPlayingEvent::Action(
                                            "edit".into(),
                                            serde_json::to_value(track).unwrap(),
                                        ));
                                    }
                                }),
                        )
                        .child(
                            now_playing_control("匹配歌词", "lyrics", 26., 15., true)
                                .disabled(move || track.with(Option::is_none))
                                .on_press(move || cx.emit(NowPlayingEvent::FindLyrics)),
                        ),
                ),
        )
}

pub(crate) fn controls(
    p: Rc<Presentation>,
    width: impl Fn() -> Points + 'static,
) -> impl IntoElement {
    let track = p.track;
    let state = p.bridge.state;
    let empty = move || track.with(Option::is_none);
    let playing = computed(move || state.with(|s| s.status == Status::Playing));
    let previous = p.bridge.controller.clone();
    let toggle = previous.clone();
    let next = previous.clone();
    let seek = previous.clone();
    view()
        .test_id("now-playing-controls")
        .items_center()
        .gap_3()
        .flex_shrink_0()
        .child(
            view()
                .test_id("now-playing-transport")
                .flex_row()
                .items_center()
                .justify_center()
                .gap_6()
                .child(
                    now_playing_control("上一首 · ⌘←", "previous-solid", 44., 26., false)
                        .accessibility_label("上一首")
                        .disabled(empty)
                        .on_press(move || send(&previous, Command::Previous)),
                )
                .child(
                    crate::player_controls::now_playing_transport(playing)
                        .disabled(empty)
                        .on_press(move || send(&toggle, Command::Toggle)),
                )
                .child(
                    now_playing_control("下一首 · ⌘→", "next-solid", 44., 26., false)
                        .accessibility_label("下一首")
                        .disabled(empty)
                        .on_press(move || send(&next, Command::Next)),
                ),
        )
        .child(
            seek_bar()
                .position(p.bridge.position)
                .buffered(p.bridge.buffered)
                .duration(move || track.with(|t| t.as_ref().and_then(|t| t.duration).unwrap_or(0.)))
                .now_playing(true)
                .immersive(true)
                .w(width)
                .flex_shrink_0()
                .on_seek(move |s: &f64| send(&seek, Command::Seek(*s))),
        )
}
