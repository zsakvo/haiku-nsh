//! Styled player controls share the shell's geometry, semantic colors and states.
use nsh_base::{ButtonElement, button, theme};
use nsh_core::{element::ImageSrc, prelude::*};

pub(crate) fn icon(name: &str, primary: bool) -> impl IntoElement {
    surface_icon(name, primary, false.into())
}
fn surface_icon(name: &str, primary: bool, dark: Prop<bool>) -> impl IntoElement {
    let markup = match name {
        "pause" => include_str!("../../../assets/icons/pause.svg"),
        "previous" => include_str!("../../../assets/icons/skip-prev.svg"),
        "next" => include_str!("../../../assets/icons/skip-next.svg"),
        "volume" => include_str!("../../../assets/icons/sound-high.svg"),
        "queue" => include_str!("../../../assets/icons/playlist.svg"),
        "shuffle" => include_str!("../../../assets/icons/shuffle.svg"),
        "repeat" => include_str!("../../../assets/icons/repeat.svg"),
        "lyrics" => include_str!("../../../assets/icons/closed-captions-tag.svg"),
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
pub(crate) fn surface_control(
    label: &'static str,
    icon_name: &'static str,
    primary: bool,
    dark: Prop<bool>,
) -> ButtonElement {
    button()
        .accessibility_label(label)
        .size(if primary { px(36.) } else { px(32.) })
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
        .hover(move |s| s.opacity(0.8))
        .active(|s| s.opacity(0.65))
        .app_class("focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-40")
        .child(view().tooltip(hint(label)).child(surface_icon(icon_name, primary, dark)))
}
pub(crate) fn hint(label: &'static str) -> AnyElement {
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
    button()
        .accessibility_label("收起正在播放")
        .on_press(on_press)
        .size_8()
        .rounded_full()
        .items_center()
        .justify_center()
        .hover(|s| s.bg(move || theme().color("now-playing-surface").unwrap()))
        .active(|s| s.opacity(0.65_f32))
        .app_class("focus-visible:outline-2 focus-visible:outline-lyric-active")
        .child(
            view().tooltip(hint("收起正在播放")).child(
                image(ImageSrc::svg_markup(include_str!(
                    "../../../assets/icons/xmark.svg"
                )))
                .size_5()
                .tint_color(move || theme().color("now-playing-foreground").unwrap()),
            ),
        )
}
pub(crate) const POPUP: &str =
    "rounded-(--radius-surface) border border-border bg-popover shadow-lg p-3";
