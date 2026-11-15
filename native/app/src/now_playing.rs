//! App-owned full-window presentation over retained navigation and dock.
use crate::{lyrics_view::lyrics_view, player_controls::close_on_dark, presentation};
use nsh_base::theme;
use nsh_core::{Hsla, JsonValue, Rgba, prelude::*};
use std::time::Duration;

const ENTER_MS: u64 = 320;
const EXIT_MS: u64 = 240;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Hidden,
    Starting,
    Opening,
    Open,
    Closing,
}

/// Presence changes only at the boundaries; native styles move the retained subtree.
#[derive(Clone, Copy)]
struct Transition {
    mounted: Signal<bool>,
    phase: Signal<Phase>,
    reduced: Signal<bool>,
    generation: Stored<u64>,
    surface: ElementRef,
    fade: AnimatedValue,
}

impl Transition {
    fn new(open: impl Fn() -> bool + 'static) -> Self {
        let this = Self {
            mounted: signal(false),
            phase: signal(Phase::Hidden),
            reduced: signal(false),
            generation: stored(0),
            surface: element_ref(),
            fade: animated_value(1.),
        };
        let open = computed(open);
        effect(move || {
            let (open, reduced) = (open.get(), reduce_motion());
            untracked(|| this.follow(open, reduced));
        });
        this
    }

    fn follow(self, open: bool, reduced: bool) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.replace(generation);
        self.reduced.set(reduced);
        if open && !self.mounted.get_untracked() {
            self.fade.set(if reduced { 0. } else { 1. });
            self.phase.set(Phase::Starting);
            self.mounted.set(true);
            // Commit the offscreen starting style before changing its target.
            let enter = move || {
                if self.generation.get() == generation
                    && self.phase.get_untracked() == Phase::Starting
                {
                    self.begin(true, reduced, generation);
                }
            };
            request_animation_frame(move |_| enter());
            set_timeout(Duration::from_millis(20), enter);
        } else if self.mounted.get_untracked() {
            // Keep the same view and its sampled native style when reversing.
            self.begin(open, reduced, generation);
        }
    }

    fn begin(self, open: bool, reduced: bool, generation: u64) {
        self.phase
            .set(if open { Phase::Opening } else { Phase::Closing });
        #[cfg(test)]
        global::<tests::TransitionStarts>().update(|record| {
            record.count += 1;
            record.current = Some(self);
        });
        log::info!("NowPlaying transition start: open={open}, reduced={reduced}");
        let deadline =
            nsh_core::time::now() + Duration::from_millis(if open { ENTER_MS } else { EXIT_MS });
        spawn(async move {
            if reduced {
                // Style transitions are suppressed by NSH under reduce_motion;
                // an explicit opacity value preserves the requested 150ms fade.
                let completed = self
                    .fade
                    .timing(
                        if open { 1. } else { 0. },
                        150.,
                        if open {
                            Easing::EaseOut
                        } else {
                            Easing::EaseIn
                        },
                    )
                    .await;
                if !completed {
                    return;
                }
            } else {
                self.fade.set(1.);
                // Anchor the lifetime to this request, not to when an async
                // style query or timer task happens to be polled.
                timer_until(deadline).await;
                // Let the native view draw its last transition frame before removal.
                next_frame().await;
            }
            if self.generation.get() == generation {
                self.phase
                    .set(if open { Phase::Open } else { Phase::Hidden });
                self.mounted.set(open);
                log::info!("NowPlaying transition complete: open={open}, reduced={reduced}");
            }
        });
    }

    fn view(self) -> nsh_core::element::View {
        view()
            .element_ref(&self.surface)
            .test_id("now-playing-overlay")
            .absolute()
            .inset_0()
            .z_index(10)
            .pointer_events(move || {
                if self.phase.get() == Phase::Closing {
                    nsh_core::PointerEvents::None
                } else {
                    nsh_core::PointerEvents::Auto
                }
            })
            .style_with(move |s| {
                let phase = self.phase.get();
                let reduced = self.reduced.get();
                let s = if reduced || phase == Phase::Starting {
                    s.transition_none()
                } else if phase == Phase::Closing {
                    s.transition_transform().duration(EXIT_MS as f32).ease_in()
                } else {
                    s.transition_transform()
                        .duration(ENTER_MS as f32)
                        .ease_out()
                };
                s.translate_y(relative(
                    if !reduced && matches!(phase, Phase::Starting | Phase::Closing) {
                        1.
                    } else {
                        0.
                    },
                ))
            })
            .bind_opacity(&self.fade)
    }
}

/// Window presentation controlled by the shell; this never pushes a navigation route.
#[derive(Clone, Copy, Debug, PartialEq, Props)]
pub struct NowPlayingProps {
    /// Open or close the window presentation.
    #[prop(default)]
    pub open: Prop<bool>,
}
/// Overlay lifecycle requests, independent of navigation history.
#[derive(Clone, Debug, PartialEq, Event)]
pub enum NowPlayingEvent {
    /// Open the shell's manual lyrics search dialog for the current track.
    FindLyrics,
    /// Open the shell's existing track editor.
    #[args(action, value)]
    Action(String, JsonValue),
    /// Follow dismissal from Escape or the close control.
    #[args(open)]
    OpenChange(bool),
}
/// Cover-derived dark background, artwork, metadata and virtual local lyrics.
#[component]
pub fn NowPlaying(props: NowPlayingProps, cx: Scope<NowPlayingEvent>) -> impl IntoElement {
    let p = presentation::get().expect("Core.open before NowPlaying");
    let open = p.open;
    let transition = Transition::new(move || props.open.get());
    let content = element_ref();
    let art = p.artwork;
    let track = p.track;
    let close = move || cx.emit(NowPlayingEvent::OpenChange(false));
    // The shell owns visibility; this is its only mirror for native consumers.
    effect(move || {
        let value = props.open.get();
        if open.get_untracked() != value {
            open.set(value);
        }
    });
    effect(move || {
        if props.open.get() {
            on_key_with(
                "escape",
                KeyOptions {
                    when: nsh_core::keymap::When::Always,
                    ..KeyOptions::default()
                },
                move |_| close(),
            );
        }
    });
    let cover_size = move || {
        px((window().width.0 * 0.28)
            .min(window().height.0 * 0.48)
            .min(360.)
            .max(190.))
    };
    let state = p.bridge.state;
    let paused = computed(move || state.with(|s| s.status == haiku_playback::Status::Paused));
    let background = cover_background();
    let keyboard = p.clone();
    effect(move || {
        if !props.open.get() {
            return;
        }
        for (key, command) in [
            ("space", haiku_playback::Command::Toggle),
            ("mod+left", haiku_playback::Command::Previous),
            ("mod+right", haiku_playback::Command::Next),
        ] {
            let c = keyboard.bridge.controller.clone();
            on_key(key, move |_| crate::player_dock::send(&c, command.clone()));
        }
        for (key, offset) in [("left", -5.), ("right", 5.)] {
            let p = keyboard.clone();
            on_key(key, move |_| {
                let duration = p
                    .track
                    .with_untracked(|t| t.as_ref().and_then(|t| t.duration).unwrap_or(0.));
                if duration > 0. {
                    crate::player_dock::send(
                        &p.bridge.controller,
                        haiku_playback::Command::Seek(
                            (p.bridge.position.get_untracked() + offset).clamp(0., duration),
                        ),
                    );
                }
            });
        }
        for (key, offset) in [("up", 0.05), ("down", -0.05)] {
            let p = keyboard.clone();
            on_key(key, move |_| {
                crate::player_dock::send(
                    &p.bridge.controller,
                    haiku_playback::Command::SetVolume(
                        (p.bridge.state.with_untracked(|s| s.volume) + offset).clamp(0., 1.),
                    ),
                )
            });
        }
    });
    view().absolute().inset_0().pointer_events(nsh_core::PointerEvents::BoxNone)
        .child(when(move || transition.mounted.get(), move || {
            transition.view().overflow_hidden()
                .child(view().absolute().inset_0().bg(move || Hsla::from(background.get()))
                    .pointer_events(nsh_core::PointerEvents::None))
                .child(view().absolute().inset_0().bg_linear_to_b()
                    .gradient_from(move || theme().color("now-playing-shade-top").unwrap())
                    .gradient_to(move || theme().color("now-playing-shade-bottom").unwrap())
                    .pointer_events(nsh_core::PointerEvents::None))
                .child(content_root(content).test_id("now-playing").absolute().inset_0()
                    .focusable(true).min_h_0()
                    .child(view().h(px(52.)).flex_shrink_0().pl(px(88.)).pr(px(24.))
                        .flex_row().items_center().window_drag(true)
                        .child(view().top(px(2.)).child(close_on_dark(close))))
                    .child(when(move || track.with(Option::is_none), || view().flex_1().items_center().justify_center()
                        .child(text("还没有正在播放的音乐").text_color(move || theme().color("now-playing-muted").unwrap()))))
                    .child(when(move || track.with(Option::is_some), { let p = p.clone(); move || view().test_id("now-playing-columns").flex_1().min_h_0().flex_row()
                        .gap(move || px(if window().width.0 < 1000. { 40. } else { 72. }))
                        .pl(px(56.)).pr(px(64.)).pt(px(38.)).pb(px(90.))
                        .child(view().test_id("now-playing-left").flex_1().min_w_0()
                            .items_center().justify_center().gap(px(28.))
                            .child(view().size(cover_size).flex_shrink_0()
                                .child(view().absolute().left(move || px(cover_size().0 * -0.04))
                                    .top(move || px(cover_size().0 * -0.04))
                                    .size(move || px(cover_size().0 * 1.08))
                                    .rounded_full().overflow_hidden()
                                    .pointer_events(nsh_core::PointerEvents::None)
                                    .style_with(move |s| {
                                        let s = if reduce_motion() { s.transition_none() }
                                            else { s.transition_transform().duration(400.).ease_in_out() };
                                        s.translate_x(px(if paused.get() { cover_size().0 * 0.232 } else { 0. }))
                                            .scale(if paused.get() { 0.8 } else { 1. })
                                    })
                                    .child(image("asset:/ui/player/desktop-disc.png").size_full()))
                                .child(view().absolute().inset_0().rounded(px(7.)).overflow_hidden()
                                    .bg(move || theme().color("now-playing-surface").unwrap())
                                    .items_center().justify_center()
                                    .style_with(move |s| {
                                        let s = if reduce_motion() { s.transition_none() }
                                            else { s.transition_transform().duration(400.).ease_in_out() };
                                        s.scale(if paused.get() { 0.8 } else { 1. })
                                    })
                                    .child(when(move || !art.get().is_empty(), move || image(move || art.get()).size_full().object_fit(nsh_core::ResizeMode::Cover)))
                                    .child(when(move || art.get().is_empty(), || text("♪").text_4xl()
                                        .text_color(move || theme().color("now-playing-foreground").unwrap())))))
                            .child(crate::now_playing_controls::controls(p.clone(), move || px((cover_size().0 * 0.8).min(280.)))))
                        .child(view().test_id("now-playing-lyrics").flex_1().min_w_0().min_h_0().gap(px(36.))
                            .child(crate::now_playing_controls::metadata(p.clone(), cx))
                            .child(lyrics_view(p.clone()))) })))
        }))
}

/// Effect-owned interpolation cancels on interruption and samples the visible RGB.
fn cover_background() -> Signal<Rgba> {
    let target = computed(move || color(theme().color("now-playing-background").unwrap()).to_rgb());
    let visible = signal(target.get_untracked());
    effect(move || {
        let target = target.get();
        let reduced = reduce_motion();
        let start = visible.get_untracked();
        if reduced || start == target {
            visible.set(target);
            return;
        }
        spawn(async move {
            let began = nsh_core::time::now();
            loop {
                let t = (nsh_core::time::now().duration_since(began).as_secs_f32() / 0.5).min(1.);
                let eased = t * t * (3. - 2. * t);
                let mix = |a: f32, b: f32| a + (b - a) * eased;
                visible.set(Rgba {
                    r: mix(start.r, target.r),
                    g: mix(start.g, target.g),
                    b: mix(start.b, target.b),
                    a: mix(start.a, target.a),
                });
                if t >= 1. {
                    break;
                }
                timer(Duration::from_millis(16)).await;
            }
        });
    });
    visible
}

fn content_root(handle: ElementRef) -> nsh_core::element::View {
    let root = view().element_ref(&handle);
    // Owned by this mounted content; focus does not wait for a display-frame future.
    effect(move || handle.focus());
    #[cfg(test)]
    let root = {
        root.on_layout(move |frame: &Bounds<Points>| {
            // This absolute root is laid out at the surface origin. Inspect the
            // layout event itself: a separate async measure can miss its first frame.
            *tests::FRAME.lock().unwrap() = Some((frame.origin.y.0, frame.size.height.0));
        })
    };
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsh_engine::{Clock, EngineConfig, Environment, Root};
    use nsh_testing::Headless;
    use std::sync::{Arc, Mutex};

    pub(super) static FRAME: Mutex<Option<(f32, f32)>> = Mutex::new(None);

    #[derive(Default, Clone)]
    pub(super) struct TransitionStarts {
        pub(super) count: u32,
        pub(super) current: Option<Transition>,
    }

    /// The same controlled props/events as AppShell with initialNowPlaying=true.
    #[component]
    fn InitialNowPlayingFixture() -> impl IntoElement {
        set_tokens(
            nsh_base::Theme::from_json(include_str!("../../../src/ui/theme/haiku.json")).unwrap(),
        );
        let now_playing_open = signal(true);
        let ready = signal(false);
        let report = signal(String::new());
        let starts = global::<TransitionStarts>();
        spawn(async move {
            crate::Core::open_isolated().await.expect("isolated Core");
            ready.set(true);
        });
        view().flex_1().child(when(
            move || ready.get(),
            move || {
                let shared_open = presentation::get().unwrap().open;
                view()
                    .child(text("Core ready"))
                    .child(
                        now_playing()
                            .open(move || now_playing_open.get())
                            .on_open_change(move |open: &bool| {
                                now_playing_open.set(*open);
                            }),
                    )
                    .child(
                        crate::player_dock::player_dock().on_open_change(move |open: &bool| {
                            now_playing_open.set(*open);
                        }),
                    )
                    .child(
                        nsh_base::button()
                            .child(text("Inspect transition"))
                            .on_press(move || {
                                let record = starts.get();
                                report.set(format!(
                                    "starts:{};phase:{:?};owner:{};shared:{}",
                                    record.count,
                                    record.current.unwrap().phase.get_untracked(),
                                    now_playing_open.get_untracked(),
                                    shared_open.get_untracked(),
                                ));
                            }),
                    )
                    .child(text(move || report.get()))
            },
        ))
    }

    #[test]
    fn initial_now_playing_starts_once_and_is_open_within_500ms() {
        let mut app = Headless::start(EngineConfig {
            viewport: Size::new(px(1200.), px(800.)),
            clock: Clock::manual(std::time::SystemTime::UNIX_EPOCH),
            ..EngineConfig::new(Root::of::<InitialNowPlayingFixture>())
        })
        .unwrap();
        app.wait_for("Core ready").unwrap();
        app.run_for(Duration::from_millis(500)).unwrap();
        app.press("Inspect transition").unwrap();
        assert!(app.shows("starts:1;phase:Open;owner:true;shared:true"));
        assert!(app.screen().matches("id:now-playing-overlay").unwrap());
    }

    #[component]
    fn Fixture() -> impl IntoElement {
        let ready = signal(false);
        spawn(async move {
            crate::Core::open_isolated().await.expect("isolated Core");
            ready.set(true);
        });
        view()
            .flex_1()
            .child(when(move || ready.get(), || now_playing().open(true)))
    }

    #[test]
    fn now_playing_fills_window_beneath_persistent_dock() {
        *FRAME.lock().unwrap() = None;
        let mut app = Headless::start(EngineConfig {
            viewport: Size::new(px(1200.), px(800.)),
            ..EngineConfig::new(Root::of::<Fixture>())
        })
        .unwrap();
        app.wait_for("id:now-playing").unwrap();
        app.wait_until(
            "NowPlaying layout",
            std::time::Duration::from_secs(5),
            |_| FRAME.lock().unwrap().is_some(),
        )
        .unwrap();
        let (top, height) = FRAME.lock().unwrap().unwrap();
        assert!(top.abs() <= 1., "top = {top}");
        assert!(
            (top + height - 800.).abs() <= 1.,
            "bottom = {}",
            top + height
        );
    }

    #[component]
    fn TransitionFixture() -> impl IntoElement {
        let open = signal(false);
        let clicks = signal(0);
        let transition = Transition::new(move || open.get());
        view()
            .flex_1()
            // The test's own controls stay above the full-window overlay (z 10), so presses reach them.
            .child(
                view()
                    .z_index(20)
                    .child(
                        nsh_base::button()
                            .on_press(move || open.set(true))
                            .child(text("Open")),
                    )
                    .child(
                        nsh_base::button()
                            .on_press(move || open.set(false))
                            .child(text("Close")),
                    ),
            )
            .child(text(move || format!("phase:{:?}", transition.phase.get())))
            .child(text(move || format!("clicks:{}", clicks.get())))
            .child(when(
                move || transition.mounted.get(),
                move || {
                    transition.view().child(
                        nsh_base::button()
                            .on_press(move || clicks.update(|n| *n += 1))
                            .child(text("Retained content")),
                    )
                },
            ))
    }

    #[derive(Default)]
    struct Recorded {
        id: Option<u32>,
        pointer_events: Option<nsh_core::PointerEvents>,
        styles: Vec<Box<nsh_core::ViewStyle>>,
        fades: Vec<f32>,
    }

    fn transition_app(reduced: bool) -> (Headless, Arc<Mutex<Recorded>>) {
        let mut app = Headless::start(EngineConfig {
            viewport: Size::new(px(1200.), px(800.)),
            environment: Environment {
                reduce_motion: reduced,
                ..Environment::default()
            },
            clock: Clock::manual(std::time::SystemTime::UNIX_EPOCH),
            ..EngineConfig::new(Root::of::<TransitionFixture>())
        })
        .unwrap();
        app.wait_for("text:phase:Hidden").unwrap();
        let recorded = Arc::new(Mutex::new(Recorded::default()));
        let output = recorded.clone();
        app.observe(move |_, batch| {
            use nsh_core::{
                MountItem,
                animation::{AnimationOp, Driver},
            };
            let mut output = output.lock().unwrap();
            for item in &batch.items {
                if let MountItem::ViewProps { id, props } = item
                    && props.test_id.as_deref() == Some("now-playing-overlay")
                {
                    if let Some(previous) = output.id {
                        assert_eq!(
                            previous, *id,
                            "interruption must retain the same native view"
                        );
                    }
                    output.id = Some(*id);
                    output.pointer_events = Some(props.pointer_events);
                }
            }
            for item in &batch.items {
                match item {
                    MountItem::ViewStyle { id, style } if output.id == Some(*id) => {
                        output.styles.push(style.clone());
                    }
                    MountItem::Animation(AnimationOp::Start {
                        driver: Driver::Timing { duration_ms, .. },
                        ..
                    }) => output.fades.push(*duration_ms),
                    _ => {}
                }
            }
        });
        (app, recorded)
    }

    fn y(style: &nsh_core::ViewStyle) -> f32 {
        style
            .transform
            .ops()
            .iter()
            .filter(|op| op.kind == nsh_core::TransformKind::TranslateY)
            .map(|op| op.value)
            .sum()
    }

    #[test]
    fn transition_finishes_opening_and_unmounts_only_after_exit_without_pointer_input() {
        let (mut app, recorded) = transition_app(false);
        app.press("Open").unwrap();
        app.wait_for("Retained content").unwrap();
        app.run_for(Duration::from_millis(400)).unwrap();
        assert!(app.shows("phase:Open"));
        app.press("Close").unwrap();
        assert!(app.shows("phase:Closing"));
        app.run_for(Duration::from_millis(100)).unwrap();
        assert!(app.shows("Retained content"));
        assert_eq!(
            recorded.lock().unwrap().pointer_events,
            Some(nsh_core::PointerEvents::None),
            "closing overlay must disable native pointer input"
        );
        assert!(app.shows("clicks:0"));
        app.run_for(Duration::from_millis(200)).unwrap();
        assert!(app.shows("phase:Hidden"));
        assert!(!app.shows("Retained content"));
        assert!(!app.screen().matches("id:now-playing-overlay").unwrap());
    }

    #[test]
    fn interrupted_exit_reverses_from_the_sampled_style_and_ignores_stale_removal() {
        use nsh_core::transition::{Live, restyle};
        let (mut app, recorded) = transition_app(false);
        app.press("Open").unwrap();
        app.run_for(Duration::from_millis(400)).unwrap();
        let mut style = recorded.lock().unwrap().styles.last().unwrap().clone();
        assert_eq!(y(&style), 0.);
        app.press("Close").unwrap();
        let mut closing = recorded.lock().unwrap().styles.last().unwrap().clone();
        assert_eq!(y(&closing), 800.);
        assert_eq!(closing.transition.unwrap().duration, 240.);
        // Run the very same transition engine as AppKit against the actual
        // Headless mount styles, rather than duplicating its interpolation.
        let mut live = Live::default();
        live.restyle(restyle(&style, &mut closing, false).unwrap());
        style = closing;
        let mut frame = Bounds::new(Default::default(), Size::new(px(1200.), px(800.)));
        live.step(0., &mut frame, &mut style);
        live.step(100., &mut frame, &mut style);
        let sampled = y(&style);
        assert!(sampled > 0. && sampled < 800.);
        app.run_for(Duration::from_millis(100)).unwrap();
        app.press("Open").unwrap();
        let mut reopening = recorded.lock().unwrap().styles.last().unwrap().clone();
        assert_eq!(reopening.transition.unwrap().duration, 320.);
        assert_eq!(y(&reopening), 0.);
        live.restyle(restyle(&style, &mut reopening, false).unwrap());
        style = reopening;
        assert_eq!(
            y(&style),
            sampled,
            "reverse must start at the current sample"
        );
        live.step(100., &mut frame, &mut style);
        live.step(200., &mut frame, &mut style);
        assert!(y(&style) < sampled);
        live.step(500., &mut frame, &mut style);
        assert_eq!(y(&style), 0.);
        app.run_for(Duration::from_millis(180)).unwrap();
        assert!(
            !app.shows("phase:Hidden"),
            "stale exit must not remove the new entrance"
        );
        assert!(app.shows("Retained content"));
        app.run_for(Duration::from_millis(200)).unwrap();
        assert!(app.shows("phase:Open"));
        assert!(app.shows("Retained content"));
    }

    #[test]
    fn reduced_motion_fades_for_150ms_without_any_translation() {
        let (mut app, recorded) = transition_app(true);
        app.press("Open").unwrap();
        app.wait_for("text:phase:Open").unwrap();
        assert!(app.shows("Retained content"));
        app.press("Close").unwrap();
        app.wait_for("text:phase:Hidden").unwrap();
        assert!(!app.shows("Retained content"));
        let recorded = recorded.lock().unwrap();
        assert!(!recorded.styles.is_empty());
        assert!(
            recorded
                .styles
                .iter()
                .all(|s| y(s) == 0. && s.transition.is_none())
        );
        assert_eq!(recorded.fades, vec![150., 150.]);
    }
}
