//! App-owned window presentation over retained navigation and below the shared dock.
use crate::{lyrics_view::lyrics_view, player_controls::close_on_dark, presentation};
use nsh_base::theme;
use nsh_core::prelude::*;

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
    /// Follow dismissal from Escape or the close control.
    #[args(open)]
    OpenChange(bool),
}
/// Cover-derived dark background, artwork, metadata and virtual local lyrics.
#[component]
pub fn NowPlaying(props: NowPlayingProps, cx: Scope<NowPlayingEvent>) -> impl IntoElement {
    let p = presentation::get().expect("Core.open before NowPlaying");
    let open = p.open;
    let visible = signal(false);
    let content = element_ref();
    let track = p.track;
    let art = p.artwork;
    let close = move || {
        open.set(false);
        cx.emit(NowPlayingEvent::OpenChange(false));
    };
    effect(move || {
        let value = props.open.get();
        if open.get_untracked() != value {
            open.set(value);
        }
        // Present deterministically until native-host animation timing is resolved.
        visible.set(value);
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
    view().absolute().inset_0().pointer_events(nsh_core::PointerEvents::BoxNone)
        .child(when(move ||visible.get(), || {
            view().absolute().inset_0().z_index(10)
                .bg(move ||theme().color("now-playing-background").unwrap())
                .bg_linear_to_b()
                .gradient_from(move ||theme().color("now-playing-shade-top").unwrap())
                .gradient_to(move ||theme().color("now-playing-shade-bottom").unwrap())
        }))
        .child(when(move || visible.get(), move || {
            content_root(content).test_id("now-playing")
                .absolute().top_0().left_0().right_0().bottom_28().z_index(20)
                .focusable(true)
                .min_h_0().px_8().pt_8().gap_6()
                .child(view().flex_row().justify_end().child(close_on_dark(close)))
                .child(view().flex_1().min_h_0().flex_row().items_center().gap_12().px_8()
                    .child(view().flex_1().min_w_0().items_center().justify_center()
                        .child(view().w_80().h_80()
                            .child(view().absolute().size_72().right(px(-32.)).top_4().rounded_full()
                                .bg(move ||theme().color("now-playing-surface").unwrap())
                                .border_1().border_color(move ||theme().color("now-playing-muted").unwrap())
                                .items_center().justify_center()
                                .child(view().size_20().rounded_full().bg(move ||theme().color("now-playing-background").unwrap())))
                            .child(view().size_80().rounded(move ||px(theme().tokens().number("radius-panel").unwrap_or(10.))).overflow_hidden().shadow_lg()
                                .bg(move ||theme().color("now-playing-surface").unwrap())
                                .child(when(move ||!art.get().is_empty(),move ||image(move ||art.get()).size_80()))
                                .child(when(move ||art.get().is_empty(),||text("♪").text_4xl().text_color(move ||theme().color("now-playing-foreground").unwrap()))))))
                    .child(view().flex_1().min_w_0().h_full().gap_4()
                        .child(view().gap_2()
                            .child(text(move ||track.with(|t|t.as_ref().map(|t|t.title.clone()).unwrap_or_else(||"尚未播放".into())))
                                .text_2xl().font_weight(FontWeight::SEMIBOLD).truncate().text_color(move ||theme().color("now-playing-foreground").unwrap()))
                            .child(text(move ||track.with(|t|t.as_ref().map(haiku_library::service::artist_names).map(|v|v.join(" / ")).unwrap_or_default()))
                                .text_sm().text_color(move ||theme().color("now-playing-muted").unwrap()))
                            .child(text(move ||track.with(|t|t.as_ref().and_then(|t|t.album.as_ref()).map(|a|a.title.clone()).unwrap_or_default()))
                                .text_sm().text_color(move ||theme().color("now-playing-muted").unwrap())))
                        .child(view().flex_row().items_center().gap_3()
                            .child(crate::collections::like_button(true))
                            .child(nsh_base::button()
                                .accessibility_label("查找歌词…")
                                .app_class("min-h-8 px-3 py-1.5 justify-center rounded-(--radius) hover:bg-now-playing-surface active:opacity-80 focus-visible:outline-2 focus-visible:outline-ring")
                                .on_press(move || cx.emit(NowPlayingEvent::FindLyrics))
                                .child(text("查找歌词…").text_sm().text_color(move || theme().color("now-playing-foreground").unwrap()))))
                        .child(lyrics_view(p.clone()))))
        }))
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
    use nsh_engine::{EngineConfig, Root};
    use nsh_testing::Headless;
    use std::sync::Mutex;

    pub(super) static FRAME: Mutex<Option<(f32, f32)>> = Mutex::new(None);

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
    fn now_playing_fills_window_above_persistent_dock() {
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
            (top + height - 688.).abs() <= 1.,
            "bottom = {}",
            top + height
        );
    }
}
