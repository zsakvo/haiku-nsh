//! The only dock instance is mounted at the shell root; all commands go to Core.
use crate::{
    player_controls::{
        compact_surface_control, hint, icon, surface_control, surface_icon,
    },
    presentation,
    seek_bar::seek_bar,
};
use haiku_playback::{Command, Mode, Status};
use nsh_base::theme;
use nsh_core::prelude::*;

#[derive(Default, Clone)]
struct DockMounts(u32);
pub(crate) fn send(controller: &haiku_playback::Controller, command: Command) {
    let c = controller.clone();
    spawn(async move {
        if let Err(e) = c.execute(command).await {
            log::error!("player control: {e}");
        }
    });
}
/// Root-owned player. No position or buffer values cross the TSX bridge.
#[derive(Clone, Debug, PartialEq, Event)]
pub enum PlayerDockEvent {
    /// Requests the shell's retained now-playing surface.
    #[args(open)]
    OpenChange(bool),
    /// Low-frequency lyric cursor change; no playback clock is exported.
    #[args(index)]
    LyricChange(Option<u32>),
}
/// Persistent root player controls bound directly to Core.
#[component]
pub fn PlayerDock(cx: Scope<PlayerDockEvent>) -> impl IntoElement {
    let mounts = global::<DockMounts>();
    mounts.update(|n| n.0 += 1);
    let identity = format!("player-dock-{}", mounts.with_untracked(|n| n.0));
    let p = presentation::get().expect("Core.open before PlayerDock");
    let state = p.bridge.state;
    let track = p.track;
    let art = p.artwork;
    let foreground = move || theme().foreground;
    let muted_foreground = move || theme().muted_foreground;
    let line = p.line;
    effect(move || cx.emit(PlayerDockEvent::LyricChange(line.get().map(|i| i as u32))));
    let c = p.bridge.controller.clone();
    let empty = move || state.with(|s| s.tracks.is_empty());
    let (volume, queue) = crate::player_popovers::controls();
    let mode_c = c.clone();
    let previous_c = c.clone();
    let next_c = c.clone();
    let seek_c = c.clone();
    let duration_p = p.clone();
    #[cfg(target_os = "macos")]
    let native_glass = haiku_macos::lifecycle::native_host();
    #[cfg(not(target_os = "macos"))]
    let native_glass = false;
    // Float flush over the bottom edge of the content panel, without reserving layout height.
    view().absolute()
        .left(px(0.))
        .right(px(0.))
        .bottom(px(0.))
        .test_id(&identity).h(move ||px(theme().tokens().number("dock-height").unwrap_or(64.)))
        .flex_row().items_center().px_3()
        .child(view().test_id("dock-material").absolute().inset_0()
            .pointer_events(nsh_core::PointerEvents::None)
            .overflow_hidden().backdrop_blur(px(if native_glass {0.} else {
                theme().tokens().number("dock-blur-radius").unwrap_or(20.)
            }))
            .child(when(move ||native_glass, move || {
                // AppKit owns the blur radius. Blend in a small amount of the
                // original backdrop to soften the material without fading controls.
                nsh_core::element::native("haiku.GlassSurface").test_id("dock-glass").absolute().inset_0()
                    .pointer_events(nsh_core::PointerEvents::None)
                    .opacity(move ||theme().tokens().number("dock-backdrop-opacity").unwrap_or(0.9))
                    .prop("radius", || serde_json::json!(0.0))
                    .prop("material", || serde_json::json!("header"))
                    .prop("dark",move ||serde_json::json!(nsh_core::element::dark()))
            }))
            .child(view().absolute().inset_0().bg(move ||
                theme().color("dock-surface").unwrap()
            ))
            .child(view().absolute().top_0().left(px(0.)).right(px(0.)).h(px(1.)).bg(move ||
                theme().color("navigation-backdrop-border").unwrap()
            )))
        .child(view().flex_1().min_w_0().flex_row().items_center().gap_3()
          .child(nsh_base::button().accessibility_label("打开正在播放")
            .disabled(empty).on_press(move ||cx.emit(PlayerDockEvent::OpenChange(true)))
            .size_12().flex_shrink_0().rounded(move ||theme().radius).overflow_hidden()
            .app_class("active:opacity-60 focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40")
            .child(when(move || !art.get().is_empty(), move ||view().tooltip(hint("打开正在播放")).child(image(move ||art.get()).size_12())))
            .child(when(move || art.get().is_empty(), ||view().size_12().items_center().justify_center().bg(move ||theme().muted).child(icon("music",false)))))
          .child(view().flex_1().min_w_0().flex_row().items_center().gap_4()
            .child(view().min_w_0().flex_shrink(1.).gap_1()
                .child(text(move ||track.with(|t| t.as_ref().map(|t|t.title.clone()).unwrap_or_else(||"尚未播放".into())))
                    .min_w_0().flex_shrink(1.).text_sm().truncate().test_id("dock-title").text_color(foreground))
                .child(text(move ||track.with(|t| t.as_ref().map(haiku_library::service::artist_names).filter(|v|!v.is_empty()).map(|v|v.join(" / ")).unwrap_or_else(||"选择歌曲开始播放".into())))
                    .text_xs().truncate().test_id("dock-artist").text_color(muted_foreground)))
            .child(crate::collections::compact_like_button(false))))
        .child(view().w(percentage(0.4)).max_w(move ||px(theme().tokens().number("dock-progress-width").unwrap_or(384.)))
          .flex_shrink_0()
          .child(view().test_id("dock-transport").relative().top(px(4.)).flex_row().items_center().justify_center().gap_4()
            .child(surface_control("上一首","previous-solid",false,false.into()).disabled(empty).on_press(move ||send(&previous_c,Command::Previous)))
            .child(nsh_base::button()
                .accessibility_label("播放 / 暂停")
                .size_7().rounded_full().items_center().justify_center()
                .active(|s|s.opacity(0.65))
                .app_class("focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40")
                .disabled(empty).on_press(move ||send(&c,Command::Toggle))
                .child(view().tooltip(hint("播放 / 暂停"))
                    .child(when(move ||state.with(|s| matches!(s.status,Status::Playing|Status::Loading)),move ||surface_icon("pause-solid",false,false.into()).size_6()))
                    .child(when(move ||!state.with(|s| matches!(s.status,Status::Playing|Status::Loading)),move ||surface_icon("play-solid",false,false.into()).size_6()))))
            .child(surface_control("下一首","next-solid",false,false.into()).disabled(empty).on_press(move ||send(&next_c,Command::Next))))
          .child(seek_bar().position(p.bridge.position).buffered(p.bridge.buffered)
            .duration(move || duration_p.duration())
            .now_playing(false).inline_times(true)
            .on_seek(move |seconds: &f64| send(&seek_c,Command::Seek(*seconds)))
            .w_full().min_w_0()))
        .child(view().flex_1().min_w_0().flex_row().items_center().justify_end().gap(px(10.))
            .child(compact_surface_control(move || -> SharedString {format!("切换播放模式：{}",match state.with(|s|s.mode) {
                    Mode::Sequential=>"顺序播放",Mode::RepeatAll=>"列表循环",Mode::RepeatOne=>"单曲循环",Mode::Shuffle=>"随机播放",
                }).into()},"repeat",false.into()).disabled(empty)
                .on_press(move ||send(&mode_c,Command::SetMode(match state.with_untracked(|s|s.mode) {
                    Mode::Sequential=>Mode::RepeatAll,Mode::RepeatAll=>Mode::RepeatOne,Mode::RepeatOne=>Mode::Shuffle,Mode::Shuffle=>Mode::Sequential,
                }))))
            .child(volume)
            .child(queue))
}
