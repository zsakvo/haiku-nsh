//! The only dock instance is mounted at the shell root; all commands go to Core.
use crate::{
    player_controls::{POPUP, control, hint, icon},
    presentation,
    seek_bar::seek_bar,
};
use haiku_playback::{Command, Mode, Status};
use nsh_base::{SliderValue, popover, slider, theme};
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
    let open = p.open;
    effect(move || cx.emit(PlayerDockEvent::OpenChange(open.get())));
    let line = p.line;
    effect(move || cx.emit(PlayerDockEvent::LyricChange(line.get().map(|i|i as u32))));
    let c = p.bridge.controller.clone();
    let empty = move || state.with(|s| s.tracks.is_empty());
    let volume_c = c.clone();
    let queue_c = c.clone();
    let mode_c = c.clone();
    let previous_c = c.clone();
    let next_c = c.clone();
    let seek_c = c.clone();
    view().absolute().left_6().right_6().bottom_6().z_index(30)
        .test_id(&identity).h_20().flex_row().items_center().gap_5().px_4()
        .rounded(move ||px(theme().tokens().number("radius-surface").unwrap_or(14.)))
        .bg(move ||theme().color("dock-surface").unwrap()).border_1().border_color(move ||theme().border).shadow_lg()
        .child(nsh_base::button().accessibility_label("打开正在播放")
            .disabled(empty).on_press(move ||open.set(true))
            .size_12().rounded(move ||theme().radius).overflow_hidden()
            .app_class("hover:opacity-80 active:opacity-60 focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40")
            .child(when(move || !art.get().is_empty(), move ||view().tooltip(hint("打开正在播放")).child(image(move ||art.get()).size_12())))
            .child(when(move || art.get().is_empty(), ||view().size_12().items_center().justify_center().bg(move ||theme().muted).child(icon("music",false)))))
        .child(view().w_40().min_w_0().gap_1()
            .child(text(move ||track.with(|t| t.as_ref().map(|t|t.title.clone()).unwrap_or_else(||"尚未播放".into())))
                .text_sm().truncate().text_color(move ||theme().foreground))
            .child(text(move ||track.with(|t| t.as_ref().map(haiku_library::service::artist_names).filter(|v|!v.is_empty()).map(|v|v.join(" / ")).unwrap_or_else(||"选择歌曲开始播放".into())))
                .text_xs().truncate().text_color(move ||theme().muted_foreground)))
        .child(view().flex_row().items_center().gap_2()
            .child(control("上一首","previous",false).disabled(empty).on_press(move ||send(&previous_c,Command::Previous)))
            .child(nsh_base::button()
                .accessibility_label("播放 / 暂停")
                .size_9().rounded_full().items_center().justify_center()
                .bg(move ||theme().primary).hover(|s|s.opacity(0.85)).active(|s|s.opacity(0.65))
                .app_class("focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40")
                .disabled(empty).on_press(move ||send(&c,Command::Toggle))
                .child(view().tooltip(hint("播放 / 暂停"))
                    .child(when(move ||state.with(|s| matches!(s.status,Status::Playing|Status::Loading)),||icon("pause",true)))
                    .child(when(move ||!state.with(|s| matches!(s.status,Status::Playing|Status::Loading)),||icon("play",true)))))
            .child(control("下一首","next",false).disabled(empty).on_press(move ||send(&next_c,Command::Next))))
        .child(seek_bar().position(p.bridge.position).buffered(p.bridge.buffered)
            .duration(move || track.with(|t|t.as_ref().and_then(|t|t.duration).unwrap_or(0.)))
            .now_playing(open)
            .on_seek(move |seconds: &f64| send(&seek_c,Command::Seek(*seconds)))
            .flex_1().min_w_0())
        .child(view().flex_row().items_center().gap_1()
            .child(control("切换播放模式","repeat",false).w_12().disabled(empty)
                .on_press(move ||send(&mode_c,Command::SetMode(match state.with_untracked(|s|s.mode) {
                    Mode::Sequential=>Mode::RepeatAll,Mode::RepeatAll=>Mode::RepeatOne,Mode::RepeatOne=>Mode::Shuffle,Mode::Shuffle=>Mode::Sequential,
                })))
                .child(text(move || match state.with(|s|s.mode) {
                    Mode::Sequential=>"顺序",Mode::RepeatAll=>"循环",Mode::RepeatOne=>"单曲",Mode::Shuffle=>"随机",
                }).text_xs().text_color(move ||theme().muted_foreground)))
            .child(popover().placement(Placement::Top).popup_class(POPUP)
                .trigger(control("音量","volume",false))
                .child(view().w_48().gap_3()
                    .child(text("音量").text_sm().text_color(move ||theme().popover_foreground))
                    .child(slider().min(0.).max(1.).step(0.01)
                        .value(move ||Some(SliderValue::Single(state.with(|s|s.volume))))
                        .accessibility_label("播放音量")
                        .track_class("h-1 rounded-full bg-muted")
                        .range_class("h-1 rounded-full bg-foreground")
                        .indicator_class("h-1 bg-foreground")
                        .thumb_class("size-3 rounded-full bg-foreground border border-border focus-visible:outline-2 focus-visible:outline-ring")
                        .on_change(move |v: &SliderValue|send(&volume_c,Command::SetVolume(v.start()))))))
            .child(popover().placement(Placement::Top).popup_class(POPUP)
                .trigger(control("播放队列","queue",false).disabled(empty))
                .child(view().w_80().gap_2()
                    .child(text("播放队列").text_sm().font_weight(FontWeight::SEMIBOLD).text_color(move ||theme().popover_foreground))
                    .child(virtual_list(move ||state.with(|s|s.tracks.len()),move |at| {
                        let jump_c=queue_c.clone(); let remove_c=queue_c.clone();
                        view().flex_row().items_center().h_12().gap_2()
                            .child(nsh_base::button().flex_1().min_w_0().h_full().px_2()
                                .accessibility_label(move ||Some(state.with(|s|s.tracks.get(at.index()).map(|t|format!("播放 {}",t.title)).unwrap_or_default()).into()))
                                .app_class("rounded-(--radius) hover:bg-accent active:bg-muted focus-visible:outline-2 focus-visible:outline-ring")
                                .on_press(move ||send(&jump_c,Command::Jump(at.index() as i32)))
                                .child(text(move ||state.with(|s|s.tracks.get(at.index()).map(|t|t.title.clone()).unwrap_or_default()))
                                    .text_sm().truncate().text_color(move ||theme().popover_foreground)))
                            .child(control("从队列移除","close",false).on_press(move ||send(&remove_c,Command::Remove(at.index() as i32))))
                    }).estimated_item_size(px(48.)).h_64())))
            .child(control("显示歌词","lyrics",false).disabled(empty).on_press(move ||open.set(true))))
}
