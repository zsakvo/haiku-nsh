//! Visible lyric rows only. Layout measurements belong to word views, not lyric data.
use crate::presentation::Presentation;
use haiku_lyrics::{LyricLine, Lyrics};
use nsh_base::{button, theme};
use nsh_core::prelude::*;
use std::{
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

fn line(lyrics: Signal<Arc<Lyrics>>, at: Position) -> Option<LyricLine> {
    lyrics.with(|l| l.lines.get(at.index()).cloned())
}
/// Word-by-word clipping reads the Rust clock; no timer or JS position mirror.
fn karaoke_line(p: Rc<Presentation>, at: Position) -> impl IntoElement {
    let lyrics = p.lyrics;
    let position = p.bridge.position;
    view().flex_row().flex_wrap().child(each(
        move || (0..line(lyrics, at).map_or(0, |l| l.words.len())).collect::<Vec<_>>(),
        |index| *index,
        move |word, _| {
            let width = signal(0_f32);
            let content = move || {
                line(lyrics, at)
                    .and_then(|l| l.words.get(word.get()).cloned())
                    .map(|w| w.text)
                    .unwrap_or_default()
            };
            view()
                .child(
                    text(content)
                        .text_xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || theme().color("lyric-inactive").unwrap())
                        .on_layout(move |b: &Bounds<Points>| width.set(b.size.width.0)),
                )
                .child(
                    view()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .overflow_hidden()
                        .w(move || {
                            let fraction = line(lyrics, at)
                                .and_then(|l| l.words.get(word.get()).cloned())
                                .map_or(0., |w| {
                                    let now = position.get();
                                    if w.end <= w.start {
                                        if now >= w.start { 1. } else { 0. }
                                    } else {
                                        ((now - w.start) / (w.end - w.start)).clamp(0., 1.)
                                    }
                                });
                            px(width.get() * fraction as f32)
                        })
                        .child(
                            text(content)
                                .w(move || px(width.get()))
                                .number_of_lines(1_u32)
                                .text_xl()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(move || theme().color("lyric-active").unwrap()),
                        ),
                )
        },
    ))
}
/// Virtual lyrics follow the cursor except during a four-second user browsing interval.
pub(crate) fn lyrics_view(p: Rc<Presentation>) -> impl IntoElement {
    let lyrics = p.lyrics;
    let current = p.line;
    let list = element_ref();
    let deadline = stored(Instant::now());
    let programmatic_until = stored(Instant::now());
    let last_follow = stored(None::<usize>);
    let follow = move || {
        if Instant::now() < deadline.get() {
            return;
        }
        if let Some(at) = current.get_untracked() {
            if last_follow.get() != Some(at) {
                last_follow.replace(Some(at));
                programmatic_until.replace(Instant::now() + Duration::from_millis(200));
                list.scroll_to_index(at as u32, percentage(0.5), px(0.), false);
            }
        }
    };
    effect(move || {
        current.get();
        lyrics.get();
        follow();
    });
    spawn(async move {
        loop {
            timer(Duration::from_millis(250)).await;
            follow();
        }
    });
    let controller = p.bridge.controller.clone();
    view().flex_1().min_h_0()
        .child(virtual_list(
            move ||lyrics.with(|l| l.lines.len()),
            move |at| {
                let c = controller.clone();
                let p = p.clone();
                button()
                    .accessibility_label(move ||Some(line(lyrics,at).map(|l|l.text).unwrap_or_default().into()))
                    .disabled(move ||!lyrics.with(|l| l.synced))
                    
                    .h_24().px_3().justify_center().gap_2()
                    .app_class("rounded-(--radius) hover:bg-now-playing-surface active:opacity-80 focus-visible:outline-2 focus-visible:outline-lyric-active")
                    .opacity(move || {
                        if !lyrics.with(|l| l.synced) { return 1_f32; }
                        match current.get() {
                            Some(i) if i == at.index() => 1_f32,
                            Some(i) if i.abs_diff(at.index()) == 1 => 0.65_f32,
                            _ => 0.4_f32,
                        }
                    })
                    .on_press(move || if let Some(l) = line(lyrics,at) {
                        let c = c.clone();
                        spawn(async move { if let Err(e) = c.execute(haiku_playback::Command::Seek(l.at)).await { log::error!("lyric seek: {e}"); } });
                    })
                    .child(when(move ||line(lyrics,at).is_some_and(|l|!l.words.is_empty()), move ||karaoke_line(p.clone(),at)))
                    .child(when(move ||line(lyrics,at).is_none_or(|l|l.words.is_empty()), move || {
                        text(move ||line(lyrics,at).map(|l|l.text).unwrap_or_default())
                            .text_xl().font_weight(FontWeight::SEMIBOLD).number_of_lines(2_u32)
                            .text_color(move ||theme().color(if current.get() == Some(at.index()) {"lyric-active"} else {"lyric-inactive"}).unwrap())
                    }))
                    .child(text(move ||line(lyrics,at).and_then(|l|l.translation).unwrap_or_default())
                        .text_sm().number_of_lines(1_u32).text_color(move ||theme().color("now-playing-muted").unwrap()))
            })
            .estimated_item_size(px(96.))
            .element_ref(&list).flex_1().min_h_0()
            .on_touch(move |event: &nsh_core::TouchEvent| if matches!(event.phase,nsh_core::TouchPhase::Move) {
                deadline.replace(Instant::now()+Duration::from_secs(4)); last_follow.replace(None);
            })
            .on_scroll(move |_: &ScrollEvent| {
                // A native wheel/trackpad scroll has no primary touch sequence.
                // Ignore only the immediate event from our nonanimated follow.
                if Instant::now() >= programmatic_until.get() {
                    deadline.replace(Instant::now()+Duration::from_secs(4));
                    last_follow.replace(None);
                }
            })
            .on_mouse_move(move |e: &nsh_core::MouseEvent| if e.pressed_button.is_some() {
                deadline.replace(Instant::now()+Duration::from_secs(4)); last_follow.replace(None);
            })
            .empty(text("暂无本地歌词").text_sm().p_6().text_color(move ||theme().color("now-playing-muted").unwrap())))
}
