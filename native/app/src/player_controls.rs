//! Styled player controls share the shell's geometry, semantic colors and states.
use nsh_base::{ButtonElement, button, theme};
use nsh_core::{element::ImageSrc, prelude::*};

pub(crate) fn icon(name: &str, primary: bool) -> impl IntoElement {
    surface_icon(name, primary, false.into())
}
pub(crate) fn surface_icon(
    name: &str,
    primary: bool,
    dark: Prop<bool>,
) -> nsh_core::element::Image {
    let markup = match name {
        "pause" => include_str!("../../../assets/icons/pause.svg"),
        "previous" => include_str!("../../../assets/icons/skip-prev.svg"),
        "next" => include_str!("../../../assets/icons/skip-next.svg"),
        "play-solid" => include_str!("../../../assets/icons/play-solid.svg"),
        "pause-solid" => include_str!("../../../assets/icons/pause-solid.svg"),
        "previous-solid" => include_str!("../../../assets/icons/skip-prev-solid.svg"),
        "next-solid" => include_str!("../../../assets/icons/skip-next-solid.svg"),
        "volume" => include_str!("../../../assets/icons/sound-high.svg"),
        "queue" => include_str!("../../../assets/icons/playlist.svg"),
        "shuffle" => include_str!("../../../assets/icons/shuffle.svg"),
        "repeat" => include_str!("../../../assets/icons/repeat.svg"),
        "lyrics" => include_str!("../../../assets/icons/closed-captions-tag.svg"),
        "edit" => include_str!("../../../assets/icons/edit-pencil.svg"),
        "down" => include_str!("../../../assets/icons/nav-arrow-down.svg"),
        "clock" => include_str!("../../../assets/icons/clock.svg"),
        "settings" => include_str!("../../../assets/icons/settings.svg"),
        "desktop" => include_str!("../../../assets/icons/music-double-note.svg"),
        "close" => include_str!("../../../assets/icons/xmark.svg"),
        "music" => include_str!("../../../assets/icons/music-double-note.svg"),
        _ => include_str!("../../../assets/icons/play.svg"),
    };
    image(ImageSrc::svg_markup(markup))
        .size_5()
        .tint_color(move || {
            if primary {
                theme().primary_foreground
            } else if dark.get() {
                theme().color("dock-on-dark-foreground").unwrap()
            } else {
                theme().foreground
            }
        })
}
pub(crate) fn control(
    label: &'static str,
    icon_name: &'static str,
    primary: bool,
) -> ButtonElement {
    surface_control(label, icon_name, primary, false.into())
}
pub(crate) fn compact_surface_control(
    label: impl IntoReactive<SharedString>,
    icon_name: &'static str,
    dark: Prop<bool>,
) -> ButtonElement {
    sized_surface_control(label, icon_name, false, dark, true)
}
pub(crate) fn surface_control(
    label: impl IntoReactive<SharedString>,
    icon_name: &'static str,
    primary: bool,
    dark: Prop<bool>,
) -> ButtonElement {
    sized_surface_control(label, icon_name, primary, dark, false)
}
fn sized_surface_control(
    label: impl IntoReactive<SharedString>,
    icon_name: &'static str,
    primary: bool,
    dark: Prop<bool>,
    compact: bool,
) -> ButtonElement {
    let label = Prop::<SharedString>::from(label.into_reactive());
    button()
        .accessibility_label(move || Some(label.get()))
        .size(px(if compact {
            32.
        } else if primary {
            36.
        } else {
            32.
        }))
        .rounded_full()
        .items_center()
        .justify_center()
        .bg(move || {
            if primary {
                theme().primary
            } else {
                theme().color("transparent").unwrap()
            }
        })
        .hover(move |s| {
            if compact {
                s.bg(move || {
                    if dark.get() {
                        theme().color("now-playing-surface").unwrap()
                    } else {
                        theme().color("dock-control-hover").unwrap()
                    }
                })
            } else {
                s.opacity(0.8)
            }
        })
        .active(|s| s.opacity(0.65))
        .selected_style(|s| s.bg(move || theme().color("dock-on-dark-surface").unwrap()))
        .app_class("focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40")
        .child(view().tooltip(hint(move || label.get())).child(
            surface_icon(icon_name, primary, dark).size(px(if compact { 16. } else { 20. })),
        ))
}
pub(crate) fn hint(label: impl IntoReactive<SharedString>) -> AnyElement {
    nsh_base::tooltip()
        .app_class("rounded-(--radius-surface) border border-border bg-popover shadow-lg px-2 py-1")
        .child(
            text(label)
                .text_xs()
                .text_color(move || theme().popover_foreground),
        )
        .into_element()
}
pub(crate) fn close_on_dark(on_press: impl Fn() + 'static) -> ButtonElement {
    now_playing_control("收起正在播放 · Esc", "down", 36., 20., false)
        .accessibility_label("收起正在播放")
        .on_press(on_press)
}

/// Reference sizes and foregrounds for immersive transport and metadata actions.
pub(crate) fn now_playing_control(
    label: impl IntoReactive<SharedString>,
    icon_name: &'static str,
    size: f32,
    icon_size: f32,
    metadata: bool,
) -> ButtonElement {
    let label = Prop::<SharedString>::from(label.into_reactive());
    button()
        .accessibility_label(move || Some(label.get()))
        .size(px(size))
        .flex_shrink_0()
        .rounded(px(if metadata { 6. } else { 7. }))
        .items_center()
        .justify_center()
        .hover(move |s| s.bg(move || theme().color("now-playing-surface").unwrap()))
        .active(|s| s.opacity(0.65))
        .app_class("focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40")
        .child(
            view().tooltip(hint(move || label.get())).child(
                surface_icon(icon_name, false, true.into())
                    .size(px(icon_size))
                    .tint_color(move || {
                        theme()
                            .color(if metadata {
                                "now-playing-muted"
                            } else {
                                "now-playing-foreground"
                            })
                            .unwrap()
                    }),
            ),
        )
}

pub(crate) fn now_playing_transport(playing: Memo<bool>) -> ButtonElement {
    button()
        .accessibility_label(move || Some(if playing.get() { "暂停" } else { "播放" }.into()))
        .size_14()
        .flex_shrink_0()
        .rounded(px(7.))
        .items_center()
        .justify_center()
        .hover(|s| s.bg(move || theme().color("now-playing-surface").unwrap()))
        .active(|s| s.opacity(0.65))
        .app_class("focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40")
        .child(
            view()
                .tooltip(hint("播放 / 暂停 · 空格"))
                .child(when(
                    move || playing.get(),
                    || {
                        surface_icon("pause", false, true.into())
                            .size(px(34.))
                            .tint_color(move || theme().color("now-playing-foreground").unwrap())
                    },
                ))
                .child(when(
                    move || !playing.get(),
                    || {
                        surface_icon("play-solid", false, true.into())
                            .size(px(34.))
                            .tint_color(move || theme().color("now-playing-foreground").unwrap())
                    },
                )),
        )
}

pub(crate) const POPUP: &str =
    "rounded-(--radius-surface) border border-border bg-popover shadow-lg p-3";
