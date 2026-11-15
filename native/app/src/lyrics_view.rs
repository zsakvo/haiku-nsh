//! Only the current timed row owns highlight layers; all other rows are whole text.
use crate::presentation::Presentation;
use haiku_lyrics::{LyricLine, Lyrics};
use haiku_macos::karaoke::Clock;
use nsh_base::{button, theme};
use nsh_core::prelude::*;
use serde_json::json;
use std::{
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

fn line(lyrics: Signal<Arc<Lyrics>>, at: Position) -> Option<LyricLine> {
    lyrics.with(|l| l.lines.get(at.index()).cloned())
}
type ClockKey = (u64, u64, bool, Option<usize>);

/// Deduplicate low-frequency inputs before sampling the independent position signal.
fn lyric_clock(key: impl Fn() -> ClockKey + 'static, position: Signal<f64>) -> Memo<Clock> {
    let key = computed(key);
    let revision = stored(None::<(u64, u64)>);
    let epoch = stored(0_u64);
    computed(move || {
        let (generation, seek, playing, _) = key.get();
        let next = (generation, seek);
        if revision.get().is_some_and(|old| old != next) {
            epoch.replace(epoch.get().wrapping_add(1));
        }
        revision.replace(Some(next));
        Clock {
            position: position.get_untracked(),
            playing,
            epoch: epoch.get(),
        }
    })
}

/// Unknown settings survive the durable settings flatten map.
fn player_font() -> SharedString {
    let configured = crate::presentation::get().and_then(|p| {
        p.bridge.settings.with(|s| {
            s.extra
                .get("playerFont")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
        })
    });
    configured
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "Songti SC".into()
            } else {
                tokens()
                    .font("font-sans")
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "system-ui".into())
            }
        })
        .into()
}

fn native_line(
    words: impl Fn() -> serde_json::Value + 'static,
    clock: Memo<Clock>,
) -> impl IntoElement {
    let rgba = |name| {
        let c = color(theme().color(name).unwrap()).to_rgb();
        json!([c.r, c.g, c.b, c.a])
    };
    nsh_core::element::native("haiku.KaraokeLayer")
        .w_full()
        .prop("words", words)
        .prop("font", move || json!(player_font().as_ref()))
        .prop("size", move || json!(24. * nsh_core::element::font_scale()))
        .prop("weight", json!(500))
        .prop("color", move || rgba("lyric-active"))
        .prop("dim", move || rgba("lyric-inactive"))
        .prop("clock", move || json!(clock.get()))
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
                        .text_size(px(24.))
                        .font_family(player_font)
                        .line_height(nsh_core::LineHeight::Multiple(1.4))
                        .font_weight(FontWeight::MEDIUM)
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
                                .text_size(px(24.))
                                .font_family(player_font)
                                .line_height(nsh_core::LineHeight::Multiple(1.4))
                                .font_weight(FontWeight::MEDIUM)
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
    let state = p.bridge.state;
    let clock = lyric_clock(
        move || {
            let s = state.get();
            (
                s.generation,
                s.timeline_revision,
                s.status == haiku_playback::Status::Playing && !s.buffering,
                current.get(),
            )
        },
        p.bridge.position,
    );
    let native = haiku_macos::karaoke::available();
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

                    .py(px(10.)).justify_center().gap(px(6.))
                    .app_class("rounded-(--radius) hover:bg-now-playing-surface active:opacity-80 focus-visible:outline-2 focus-visible:outline-lyric-active")
                    .opacity(move || {
                        if !lyrics.with(|l| l.synced) { return 1_f32; }
                        match current.get() {
                            Some(i) if i == at.index() => 1_f32,
                            Some(i) => (0.78_f32 - i.abs_diff(at.index()) as f32 * 0.18).max(0.15),
                            None => 1_f32,
                        }
                    })
                    .on_press(move || if let Some(l) = line(lyrics,at) {
                        let c = c.clone();
                        spawn(async move { if let Err(e) = c.execute(haiku_playback::Command::Seek(l.at)).await { log::error!("lyric seek: {e}"); } });
                    })
                    .child(when(move ||current.get() == Some(at.index()) && line(lyrics,at).is_some_and(|l|!l.words.is_empty()), move || {
                        let p = p.clone();
                        view().w_full()
                            .child(when(move ||native, move ||native_line(
                                move ||json!(line(lyrics,at).map(|l|l.words).unwrap_or_default()), clock)))
                            .child(when(move ||!native, move ||karaoke_line(p.clone(),at)))
                    }))
                    .child(when(move ||current.get() != Some(at.index()) || line(lyrics,at).is_none_or(|l|l.words.is_empty()), move || {
                        text(move ||line(lyrics,at).map(|l|l.text).unwrap_or_default())
                            .text_size(px(24.)).font_family(player_font).line_height(nsh_core::LineHeight::Multiple(1.4)).font_weight(FontWeight::MEDIUM)
                            .text_color(move ||theme().color(if current.get() == Some(at.index()) {"lyric-active"} else {"lyric-inactive"}).unwrap())
                    }))
                    .child(when(move || line(lyrics,at).and_then(|l|l.translation).is_some_and(|s|!s.is_empty()), move ||
                        text(move ||line(lyrics,at).and_then(|l|l.translation).unwrap_or_default())
                            .text_size(px(19.)).font_family(player_font)
                            .text_color(move ||theme().color(if current.get() == Some(at.index()) {"lyric-active"} else {"now-playing-muted"}).unwrap())))
            })
            .estimated_item_size(px(90.))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[component]
    fn ClockFixture() -> impl IntoElement {
        set_tokens(
            nsh_base::Theme::from_json(include_str!("../../../src/ui/theme/haiku.json")).unwrap(),
        );
        let ready = signal(false);
        let position = signal(0.);
        // The second field models unrelated PlayerState publications, such as volume.
        let state = signal(((1_u64, 0_u64, true, Some(0_usize)), 0_u32));
        let clock = lyric_clock(move || state.with(|s| s.0), position);
        spawn(async move {
            // set_tokens is queued; mount the native props after that commit.
            timer(Duration::from_millis(16)).await;
            ready.set(true);
            loop {
                timer(Duration::from_millis(16)).await;
                position.update(|p| *p += 0.016);
                state.update(|s| s.1 += 1);
            }
        });
        view()
            .child(when(
                move || ready.get(),
                move || native_line(|| json!([{"text":"一整行","start":0.,"end":30.}]), clock),
            ))
            .child(text(move || format!("position {:.1}", position.get())))
            .child(
                button()
                    .accessibility_label("seek")
                    .child(text("seek"))
                    .on_press(move || {
                        position.set(10.);
                        state.update(|s| s.0.1 += 1);
                    }),
            )
            .child(
                button()
                    .accessibility_label("pause")
                    .child(text("pause"))
                    .on_press(move || state.update(|s| s.0.2 = false)),
            )
    }

    #[test]
    fn native_clock_stays_idle_for_three_seconds_then_syncs_seek_and_pause() {
        use nsh_core::MountItem;
        use nsh_engine::{EngineConfig, Root};
        use nsh_testing::Headless;
        let samples = Arc::new(Mutex::new(Vec::<Clock>::new()));
        let output = samples.clone();
        let mut app = Headless::start(EngineConfig::new(Root::of::<ClockFixture>())).unwrap();
        app.observe(move |_, batch| {
            for item in &batch.items {
                if let MountItem::ComponentProps { props, .. } = item {
                    for (name, value) in props {
                        if name.as_ref() == "clock" {
                            output
                                .lock()
                                .unwrap()
                                .push(serde_json::from_value(value.clone().unwrap()).unwrap());
                        }
                    }
                }
            }
        });
        app.wait_for("position 0.0").unwrap();
        app.wait(Duration::from_secs(3)).unwrap();
        let initial = samples.lock().unwrap().clone();
        assert_eq!(initial.len(), 1, "playing must not stream clock properties");
        assert!(initial[0].playing);
        assert!(
            app.screen().texts().iter().any(|s| s
                .strip_prefix("position ")
                .and_then(|s| s.parse::<f64>().ok())
                .is_some_and(|p| p > 2.)),
            "fixture must actually advance position"
        );
        app.press("seek").unwrap();
        let seek = samples.lock().unwrap().last().unwrap().clone();
        assert_eq!(seek.epoch, initial[0].epoch + 1);
        assert!((seek.position - 10.).abs() < 0.1);
        app.press("pause").unwrap();
        assert!(!samples.lock().unwrap().last().unwrap().playing);
    }
}
