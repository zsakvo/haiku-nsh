//! Only the current timed row owns highlight layers; all other rows are whole text.
use crate::presentation::Presentation;
use haiku_lyrics::{LyricLine, Lyrics};
use haiku_macos::karaoke::Clock;
use nsh_base::{button, theme};
use nsh_core::prelude::*;
use nsh_core::time::{Instant, now};
use serde_json::json;
use std::{collections::BTreeMap, rc::Rc, sync::Arc, time::Duration};

/// One page-owned animation; recycled rows only register their measured geometry.
#[derive(Clone, Copy)]
struct LyricScroll {
    list: ElementRef,
    owner: Owner,
    window_centered: bool,
    rows: Stored<BTreeMap<u64, (Position, ElementRef)>>,
    next_row: Stored<u64>,
    task: Stored<Option<Spawned<()>>>,
    active: Stored<bool>,
    quiet_until: Stored<Instant>,
}

impl LyricScroll {
    fn new(list: ElementRef, window_centered: bool) -> Self {
        Self {
            list,
            owner: Owner::current(),
            window_centered,
            rows: stored(BTreeMap::new()),
            next_row: stored(0),
            task: stored(None),
            active: stored(false),
            quiet_until: stored(now()),
        }
    }

    fn row(self, at: Position) -> ElementRef {
        let row = element_ref();
        let id = self.next_row.get();
        self.next_row.replace(id + 1);
        self.rows.with(|rows| rows.insert(id, (at, row)));
        cleanup(move || {
            self.rows.try_with(|rows| rows.remove(&id));
        });
        row
    }

    fn cancel(self) {
        if let Some(task) = self.task.replace(None) {
            task.cancel();
        }
        self.active.replace(false);
        self.quiet_until.replace(now());
    }

    fn is_programmatic(self) -> bool {
        self.active.get() || now() < self.quiet_until.get()
    }

    fn locate(self, index: usize) {
        self.cancel();
        self.active.replace(true);
        let task = self.owner.run(|| {
            spawn(async move {
                // Seeking swaps the active text renderer. Measure after its layout settles.
                next_frame().await;
                next_frame().await;
                let row = self.rows.with(|rows| {
                    rows.iter()
                        .find(|(_, (at, _))| at.index() == index)
                        .map(|(&id, (_, row))| (id, *row))
                });
                let geometry = if let Some((id, row)) = row {
                    let (viewport, row) = futures::join!(self.list.measure(), row.measure());
                    viewport.zip(row).filter(|_| {
                        self.rows
                            .try_with(|rows| {
                                rows.get(&id).is_some_and(|(at, _)| at.index() == index)
                            })
                            .unwrap_or(false)
                    })
                } else {
                    None
                };
                if let Some((viewport, row)) = geometry.filter(|_| !reduce_motion()) {
                    let from = row.page.y.0 - viewport.page.y.0;
                    let to = if self.window_centered {
                        (window().height.0 * 0.5 - viewport.page.y.0 - row.frame.size.height.0 * 0.5)
                            .clamp(
                                0.,
                                (viewport.frame.size.height.0 - row.frame.size.height.0).max(0.),
                            )
                    } else {
                        (viewport.frame.size.height.0 - row.frame.size.height.0) * 0.5
                    };
                    let started = now();
                    loop {
                        let t = (now().duration_since(started).as_secs_f32() / 0.48).min(1.);
                        let eased = t * t * t * (t * (t * 6. - 15.) + 10.);
                        // Preserve the row's current visible position on the first frame.
                        // Each frame has one concrete offset, with no competing native tween.
                        self.list.scroll_to_index(
                            index as u32,
                            percentage(0.),
                            px(from + (to - from) * eased),
                            false,
                        );
                        if t >= 1. {
                            break;
                        }
                        next_frame().await;
                    }
                } else {
                    let offset = if self.window_centered {
                        self.list
                            .measure()
                            .await
                            .map(|viewport| {
                                let viewport_center =
                                    viewport.page.y.0 + viewport.frame.size.height.0 * 0.5;
                                let max_shift = viewport.frame.size.height.0 * 0.4;
                                (window().height.0 * 0.5 - viewport_center)
                                    .clamp(-max_shift, max_shift)
                            })
                            .unwrap_or(0.)
                    } else {
                        0.
                    };
                    self.list
                        .scroll_to_index(index as u32, percentage(0.5), px(offset), false);
                }
                self.quiet_until.replace(now() + Duration::from_millis(80));
                self.active.replace(false);
            })
        });
        self.task.replace(Some(task));
    }
}

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
            tokens()
                .font("font-sans")
                .map(|s| s.to_string())
                .unwrap_or_else(|| "system-ui".into())
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
/// Virtual lyrics resume following on the first line change after manual scrolling stops.
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
    let scroll = LyricScroll::new(list, true);
    let scrolling_until = stored(now());
    let dragging = stored(false);
    let last_follow = stored(None::<usize>);
    let pending_seek = stored(None::<(u64, usize)>);
    let seek_revision = stored(0_u64);
    let previous_lyrics = stored(lyrics.get_untracked());
    effect(move || {
        current.get();
        let content = lyrics.get();
        if !Arc::ptr_eq(&previous_lyrics.get(), &content) {
            previous_lyrics.replace(content);
            scroll.cancel();
            pending_seek.replace(None);
            last_follow.replace(None);
        }
        if pending_seek.get().is_none() && !dragging.get() && now() >= scrolling_until.get() {
            if let Some(at) = current.get_untracked() {
                if last_follow.get() != Some(at) {
                    last_follow.replace(Some(at));
                    scroll.locate(at);
                }
            }
        }
    });
    let browse = move || {
        scroll.cancel();
        pending_seek.replace(None);
        scrolling_until.replace(now() + Duration::from_millis(260));
        last_follow.replace(None);
    };
    let controller = p.bridge.controller.clone();
    view().relative().flex_1().min_h_0()
        .child(virtual_list(
            move ||lyrics.with(|l| l.lines.len()),
            move |at| {
                let c = controller.clone();
                let p = p.clone();
                let row = scroll.row(at);
                view().element_ref(&row).child(button()
                    .accessibility_label(move ||Some(line(lyrics,at).map(|l|l.text).unwrap_or_default().into()))
                    .disabled(move ||!lyrics.with(|l| l.synced))

                    .px_3().py(px(10.)).justify_center().gap(px(6.))
                    .app_class("rounded-(--radius) active:opacity-80 focus-visible:outline-2 focus-visible:outline-lyric-active")
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
                        let target = at.index();
                        let request = seek_revision.get().wrapping_add(1);
                        seek_revision.replace(request);
                        pending_seek.replace(Some((request, target)));
                        scroll.cancel();
                        dragging.replace(false);
                        scroll.owner.run(|| spawn(async move {
                            let result = c.execute(haiku_playback::Command::Seek(l.at)).await;
                            if pending_seek.get() != Some((request, target)) {
                                return;
                            }
                            pending_seek.replace(None);
                            match result {
                                Ok(()) => {
                                    scrolling_until.replace(now());
                                    last_follow.replace(Some(target));
                                    scroll.locate(target);
                                }
                                Err(e) => log::error!("lyric seek: {e}"),
                            }
                        }));
                    })
                    // Keep the text's layout while the current row gains its native overlay.
                    .child(view().relative().w_full()
                        .child(text(move ||line(lyrics,at).map(|l|l.text).unwrap_or_default())
                            .text_size(px(24.)).font_family(player_font).line_height(nsh_core::LineHeight::Multiple(1.4)).font_weight(FontWeight::MEDIUM)
                            .opacity(move || if current.get() == Some(at.index()) && line(lyrics,at).is_some_and(|l|!l.words.is_empty()) {0_f32} else {1_f32})
                            .text_color(move ||theme().color(if current.get() == Some(at.index()) {"lyric-active"} else {"lyric-inactive"}).unwrap())
                        )
                        .child(when(move ||current.get() == Some(at.index()) && line(lyrics,at).is_some_and(|l|!l.words.is_empty()), move || {
                            let p = p.clone();
                            view().absolute().inset_0().pointer_events(nsh_core::PointerEvents::None)
                                .child(when(move ||native, move ||native_line(
                                    move ||json!(line(lyrics,at).map(|l|l.words).unwrap_or_default()), clock)))
                                .child(when(move ||!native, move ||karaoke_line(p.clone(),at)))
                        }))
                    )
                    .child(when(move || line(lyrics,at).and_then(|l|l.translation).is_some_and(|s|!s.is_empty()), move ||
                        text(move ||line(lyrics,at).and_then(|l|l.translation).unwrap_or_default())
                            .text_size(px(19.)).font_family(player_font)
                            .text_color(move ||theme().color(if current.get() == Some(at.index()) {"lyric-active"} else {"now-playing-muted"}).unwrap()))))
            })
            .test_id("lyrics-list")
            .scrollbar(false)
            .estimated_item_size(px(90.))
            .element_ref(&list).flex_1().min_h_0()
            .on_wheel(move |_: &nsh_core::WheelEvent| browse())
            .on_scroll_drag_start(move |_: &Point<Points>| {
                dragging.replace(true);
                browse();
            })
            .on_scroll_drag_end(move |_: &Point<Points>| {
                dragging.replace(false);
                scrolling_until.replace(now() + Duration::from_millis(260));
            })
            .on_scroll_end(move |_: &Point<Points>| {
                dragging.replace(false);
                scrolling_until.replace(now());
            })
            .on_scroll(move |_: &ScrollEvent| {
                // Keep browsing active through momentum; ignore our own positioning.
                if !scroll.is_programmatic() {
                    browse();
                }
            })
            .empty(text("暂无本地歌词").text_sm().p_6().text_color(move ||theme().color("now-playing-muted").unwrap())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[component]
    fn ScrollFixture() -> impl IntoElement {
        let list = element_ref();
        let scroll = LyricScroll::new(list, false);
        view()
            .w(px(400.))
            .h(px(500.))
            .child(
                button()
                    .accessibility_label("browse")
                    .child(text("browse"))
                    .on_press(move || {
                        scroll.cancel();
                        list.scroll_to_index(5, percentage(0.), px(0.), false);
                    }),
            )
            .child(
                button()
                    .accessibility_label("locate")
                    .child(text("locate"))
                    .on_press(move || scroll.locate(8)),
            )
            .child(
                button()
                    .accessibility_label("interrupt")
                    .child(text("interrupt"))
                    .on_press(move || {
                        scroll.locate(8);
                        set_timeout(Duration::from_millis(180), move || {
                            scroll.cancel();
                            list.scroll_to_index(5, percentage(0.), px(0.), false);
                        });
                    }),
            )
            .child(
                virtual_list(
                    || 30_usize,
                    move |at| {
                        let row = scroll.row(at);
                        view()
                            .element_ref(&row)
                            .h(move || px(if at.index() % 2 == 0 { 60. } else { 90. }))
                            .child(text(move || format!("row {}", at.index())))
                    },
                )
                .element_ref(&list)
                .h(px(300.))
                .flex_shrink_0()
                .estimated_item_size(px(75.)),
            )
    }

    #[test]
    fn lyric_scroll_emits_intermediate_positions_and_stops_when_interrupted() {
        use nsh_core::{Command, MountItem};
        use nsh_engine::{EngineConfig, Root};
        use nsh_testing::Headless;
        let samples = Arc::new(Mutex::new(Vec::<(f32, bool)>::new()));
        let output = samples.clone();
        let mut app = Headless::start(EngineConfig::new(Root::of::<ScrollFixture>())).unwrap();
        app.observe(move |_, batch| {
            for item in &batch.items {
                if let MountItem::Command {
                    command: Command::ScrollTo { y, animated, .. },
                    ..
                } = item
                {
                    output.lock().unwrap().push((y.0, *animated));
                }
            }
        });
        app.wait_for("browse").unwrap();
        app.press("browse").unwrap();
        app.wait(Duration::from_millis(80)).unwrap();
        samples.lock().unwrap().clear();
        app.press("locate").unwrap();
        app.wait(Duration::from_millis(80)).unwrap();
        let frames = samples.lock().unwrap().clone();
        assert!(
            frames.len() > 8,
            "positioning needs actual intermediate frames: {frames:?}"
        );
        assert!(frames.iter().all(|(_, animated)| !animated));
        let first = frames.first().unwrap().0;
        let last = frames.last().unwrap().0;
        let middle = frames[frames.len() / 2].0;
        assert!(
            (first - 360.).abs() < 1.,
            "first frame must keep the browsed position: {first}"
        );
        assert!(
            first < middle && middle < last,
            "{first} < {middle} < {last}"
        );
        assert!(
            frames.windows(2).all(|w| w[0].0 <= w[1].0 + 0.5),
            "{frames:?}"
        );
        assert!(
            (last - 480.).abs() < 1.,
            "variable-height target must finish centered: {last}"
        );

        app.press("browse").unwrap();
        app.wait(Duration::from_millis(80)).unwrap();
        samples.lock().unwrap().clear();
        app.press("interrupt").unwrap();
        app.wait(Duration::from_millis(40)).unwrap();
        let stopped = samples.lock().unwrap().len();
        assert!(stopped > 3, "interrupt must occur after animation started");
        assert!((samples.lock().unwrap().last().unwrap().0 - 360.).abs() < 1.);
        app.wait(Duration::from_millis(600)).unwrap();
        assert_eq!(
            samples.lock().unwrap().len(),
            stopped,
            "cancelled frames must not scroll back"
        );
    }

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
