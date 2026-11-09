//! S6: fixed-height recycling rows; selection belongs to track IDs.

use std::collections::HashSet;

use nsh_base::{button, theme};
use nsh_core::prelude::*;

const COUNT: usize = 10_000;
const ROW_HEIGHT: f32 = 44.0;

#[derive(PartialEq)]
struct Track {
    id: usize,
    title: String,
    artist: String,
    duration: String,
}

fn action(label: &'static str, on_press: impl Fn() + 'static) -> impl IntoElement {
    button()
        .on_press(on_press)
        .h(px(36.0))
        .px_3()
        .justify_center()
        .bg(move || theme().secondary)
        .hover(|s| s.bg(move || theme().accent))
        .active(|s| s.opacity(0.8))
        .focus(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .child(
            text(label)
                .text_sm()
                .text_color(move || theme().secondary_foreground),
        )
}

/// A disposable 10,000-track Rust virtual list.
#[component]
pub fn SpikeTrackList() -> impl IntoElement {
    let tracks = signal(
        (1..=COUNT)
            .map(|id| Track {
                id,
                title: format!("曲目 {id:05}"),
                artist: format!("艺术家 {:03}", (id - 1) % 100 + 1),
                duration: format!("{}:{:02}", 3 + id % 3, id % 60),
            })
            .collect::<Vec<_>>(),
    );
    let selected = signal(HashSet::<usize>::new());
    let list = element_ref();

    view()
        .flex_1()
        .gap_2()
        .child(
            view()
                .flex_row()
                .gap_2()
                .child(action("回到顶部", move || {
                    list.scroll_to_index(0, percentage(0.0), px(0.0), false)
                }))
                .child(action("第 5000 行", move || {
                    list.scroll_to_index(4999, percentage(0.0), px(0.0), false)
                }))
                .child(action("第 9000 行", move || {
                    list.scroll_to_index(8999, percentage(0.0), px(0.0), false)
                }))
                .child(action("滚动到底部", move || list.scroll_to_end(false))),
        )
        .child(
            virtual_list(COUNT, move |at| {
                // Position is read at event/binding time, after a cell is recycled.
                let id = move || tracks.with(|rows| rows[at.index()].id);
                button()
                    .h(px(ROW_HEIGHT))
                    .px_3()
                    .flex_row()
                    .items_center()
                    .on_press(move || {
                        selected.update(|ids| {
                            if !ids.remove(&id()) {
                                ids.insert(id());
                            }
                        })
                    })
                    .selected(move || selected.with(|ids| ids.contains(&id())))
                    .bg(move || {
                        if selected.with(|ids| ids.contains(&id())) {
                            theme().accent
                        } else {
                            theme().background
                        }
                    })
                    .hover(|s| s.bg(move || theme().muted))
                    .active(|s| s.opacity(0.8))
                    .focus(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
                    .child(
                        text(move || {
                            let title = tracks.with(|rows| rows[at.index()].title.clone());
                            if selected.with(|ids| ids.contains(&id())) {
                                format!("{title} · 已选中")
                            } else {
                                title
                            }
                        })
                        .flex_1()
                        .text_sm()
                        .text_color(move || theme().foreground),
                    )
                    .child(
                        text(move || tracks.with(|rows| rows[at.index()].artist.clone()))
                            .w(px(160.0))
                            .text_sm()
                            .text_color(move || theme().muted_foreground),
                    )
                    .child(
                        text(move || tracks.with(|rows| rows[at.index()].duration.clone()))
                            .w(px(64.0))
                            .text_sm()
                            .text_color(move || theme().muted_foreground),
                    )
            })
            .keys(move || tracks.with(|rows| rows.iter().map(|row| row.id).collect::<Vec<_>>()))
            .estimated_item_size(px(ROW_HEIGHT))
            .initial_scroll_index(4999)
            .test_id("spike-tracks")
            .element_ref(&list)
            .flex_1(),
        )
}
