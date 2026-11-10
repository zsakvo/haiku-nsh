//! Pointer seeking captures geometry once and keeps preview separate from the clock.
use nsh_base::theme;
use nsh_core::{
    AccessibilityRole, AccessibilityValue, PointerEvents, TouchEvent, TouchPhase, prelude::*,
};

/// A clock-driven seek control; position and buffered are Rust bindings in production.
#[derive(Clone, Copy, Debug, PartialEq, Props)]
pub struct SeekBarProps {
    /// Current position in seconds.
    #[prop(default)]
    pub position: Prop<f64>,
    /// Known duration in seconds; zero/unknown disables seeking.
    #[prop(default)]
    pub duration: Prop<f64>,
    /// Buffered fraction, independent of played position.
    #[prop(default)]
    pub buffered: Prop<f64>,
    /// Use the cover-derived progress roles.
    #[prop(default)]
    pub now_playing: Prop<bool>,
}
/// User intent; emitted once at the end of an activated primary drag.
#[derive(Clone, Debug, PartialEq, Event)]
pub enum SeekBarEvent {
    /// Seek to a clamped position in seconds.
    #[args(seconds)]
    Seek(f64),
}
#[derive(Clone, Copy)]
struct Drag {
    down: f32,
    left: f32,
    width: f32,
    duration: f64,
    active: bool,
}
fn time(seconds: f64) -> String {
    let seconds = if seconds.is_finite() {
        seconds.max(0.) as u64
    } else {
        0
    };
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
/// Independent buffer/played layers with a one-point activation threshold.
#[component]
pub fn SeekBar(props: SeekBarProps, cx: Scope<SeekBarEvent>) -> impl IntoElement {
    let width = stored(0_f32);
    let drag = stored(None::<Drag>);
    let preview = signal(None::<f64>);
    let focused = signal(false);
    let valid = move || props.duration.get().is_finite() && props.duration.get() > 0.;
    effect(move || {
        if !focused.get() || !valid() {
            return;
        }
        for (key, offset) in [("left", -5.), ("right", 5.)] {
            on_key(key, move |_| {
                cx.emit(SeekBarEvent::Seek(
                    (props.position.get_untracked() + offset)
                        .clamp(0., props.duration.get_untracked()),
                ))
            });
        }
    });
    view()
        .gap_1()
        .child(
            view()
                .test_id("seek-track")
                .h_6()
                .justify_center()
                .focusable(true)
                .role(AccessibilityRole::Adjustable)
                .label("播放进度")
                .accessibility_value(move || {
                    Some(AccessibilityValue {
                        min: Some(0.),
                        max: Some(props.duration.get() as f32),
                        now: Some(preview.get().unwrap_or(props.position.get()) as f32),
                        text: Some(time(preview.get().unwrap_or(props.position.get())).into()),
                    })
                })
                .on_focus(move || focused.set(true))
                .on_blur(move || focused.set(false))
                .app_class("focus-visible:outline-2 focus-visible:outline-ring")
                .opacity(move || if valid() { 1_f32 } else { 0.4_f32 })
                .on_layout(move |b: &Bounds<Points>| {
                    width.replace(b.size.width.0);
                })
                .on_touch(move |event: &TouchEvent| {
                    let Some(point) = event.points.iter().find(|p| p.changed) else {
                        return;
                    };
                    match event.phase {
                        TouchPhase::Start => {
                            if valid() && width.get() > 0. {
                                drag.replace(Some(Drag {
                                    down: point.page.x.0,
                                    left: point.page.x.0 - point.location.x.0,
                                    width: width.get(),
                                    duration: props.duration.get_untracked(),
                                    active: false,
                                }));
                            }
                        }
                        TouchPhase::Move => {
                            drag.with(|held| {
                                if let Some(d) = held {
                                    d.active |= (point.page.x.0 - d.down).abs() >= 1.;
                                    if d.active {
                                        preview.set(Some(
                                            ((point.page.x.0 - d.left) / d.width).clamp(0., 1.)
                                                as f64
                                                * d.duration,
                                        ));
                                    }
                                }
                            });
                        }
                        TouchPhase::End => {
                            if let Some(d) = drag.replace(None) {
                                if d.active {
                                    let seconds =
                                        ((point.page.x.0 - d.left) / d.width).clamp(0., 1.) as f64
                                            * d.duration;
                                    cx.emit(SeekBarEvent::Seek(seconds));
                                }
                            }
                            preview.set(None);
                        }
                        TouchPhase::Cancel => {
                            drag.replace(None);
                            preview.set(None);
                        }
                    }
                })
                .child(
                    view()
                        .h_1()
                        .rounded_full()
                        .overflow_hidden()
                        .bg(move || theme().muted)
                        .pointer_events(PointerEvents::None)
                        .child(
                            view()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left_0()
                                .w(move || percentage(props.buffered.get().clamp(0., 1.) as f32))
                                .bg(move || {
                                    theme()
                                        .color(if props.now_playing.get() {
                                            "progress-now-playing-buffered"
                                        } else {
                                            "progress-buffered"
                                        })
                                        .unwrap_or(theme().muted)
                                }),
                        )
                        .child(
                            view()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left_0()
                                .w(move || {
                                    percentage(if valid() {
                                        (preview.get().unwrap_or(props.position.get())
                                            / props.duration.get())
                                        .clamp(0., 1.)
                                            as f32
                                    } else {
                                        0.
                                    })
                                })
                                .bg(move || {
                                    theme()
                                        .color(if props.now_playing.get() {
                                            "progress-now-playing-played"
                                        } else {
                                            "progress-played"
                                        })
                                        .unwrap_or(theme().primary)
                                }),
                        ),
                ),
        )
        .child(
            view()
                .flex_row()
                .justify_between()
                .child(
                    text(move || time(preview.get().unwrap_or(props.position.get())))
                        .test_id("seek-time")
                        .text_xs()
                        .text_color(move || {
                            if props.now_playing.get() {
                                theme().color("now-playing-muted").unwrap()
                            } else {
                                theme().muted_foreground
                            }
                        }),
                )
                .child(
                    text(move || time(props.duration.get()))
                        .text_xs()
                        .text_color(move || {
                            if props.now_playing.get() {
                                theme().color("now-playing-muted").unwrap()
                            } else {
                                theme().muted_foreground
                            }
                        }),
                ),
        )
}
