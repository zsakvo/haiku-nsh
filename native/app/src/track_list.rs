//! A reusable compact table. Cells bind their current position, never row-local state.
use crate::track_source::{self, Source};
use haiku_library::service::artist_names;
use nsh_base::{context_menu, menu, menu_item, submenu, theme};
use nsh_core::{PressEvent, prelude::*};
use std::collections::BTreeSet;

/// A TrackList's Rust data source and optional columns.
#[derive(Clone, Copy, Debug, PartialEq, Props)]
pub struct TrackListProps {
    /// Engine-local TrackDataSource handle, owned by the retained page.
    pub source: Prop<u32>,
    /// Show the album column (default true).
    #[prop(default = true)]
    pub album_column: Prop<bool>,
    /// Show the duration column (default true).
    #[prop(default = true)]
    pub duration_column: Prop<bool>,
    /// Current playing ID, from the low-frequency player state.
    pub current_id: Prop<Option<SharedString>>,
    /// Register keyboard commands only while this list is the active surface.
    #[prop(default = true)]
    pub active: Prop<bool>,
}
/// Commands and selection requests from the list; the consumer owns playback/editor actions.
#[derive(Clone, Debug, PartialEq, Event)]
pub enum TrackListEvent {
    /// Double-click/Return/play menu requests the complete source starting at this ID.
    #[args(source, id)]
    Play(u32, String),
    /// Existing row commands: next, append, edit or reveal; IDs follow query order.
    #[args(command, ids)]
    Action(String, Vec<String>),
    /// Selection changed, keyed by stable track IDs (not recycled cells).
    #[args(ids)]
    SelectionChange(Vec<String>),
}
fn source(props: TrackListProps) -> Option<Source> {
    track_source::get(props.source.get())
}
fn id_at(props: TrackListProps, at: Position) -> Option<String> {
    source(props)?.ids.with(|ids| ids.get(at.index()).cloned())
}
fn selection(props: TrackListProps, cx: Scope<TrackListEvent>) {
    if let Some(s) = source(props) {
        cx.emit(TrackListEvent::SelectionChange(s.selected_ids()));
    }
}
fn play(props: TrackListProps, cx: Scope<TrackListEvent>, id: String) {
    cx.emit(TrackListEvent::Play(props.source.get_untracked(), id));
}
fn targets(props: TrackListProps, at: Position) -> Vec<String> {
    let Some(s) = source(props) else {
        return vec![];
    };
    let Some(id) = id_at(props, at) else {
        return vec![];
    };
    if s.selected.with_untracked(|set| set.contains(&id)) {
        s.selected_ids()
    } else {
        vec![id]
    }
}
fn command(
    props: TrackListProps,
    at: Position,
    cx: Scope<TrackListEvent>,
    name: &'static str,
    label: &'static str,
) -> impl IntoElement {
    menu_item()
        .on_select(move || {
            if name == "play" {
                if let Some(id) = id_at(props, at) { play(props, cx, id); }
            } else {
                cx.emit(TrackListEvent::Action(name.into(), targets(props, at)));
            }
        })
        .app_class("rounded-(--radius) min-h-8 px-3 py-1.5 text-sm text-popover-foreground hover:bg-accent active:bg-muted focus:bg-accent")
        .child(text(label))
}
fn selected_tracks(props: TrackListProps, at: Position) -> Vec<haiku_model::Track> {
    let Some(s) = source(props) else {
        return vec![];
    };
    targets(props, at)
        .iter()
        .filter_map(|id| s.provider.track(id))
        .collect()
}
fn commands(props: TrackListProps, at: Position, cx: Scope<TrackListEvent>) -> impl IntoElement {
    let bridge = crate::presentation::get().map(|p| p.bridge.clone());
    let store = bridge.as_ref().map(|b| b.collections.clone());
    let read = store.clone();
    let write = store.clone();
    let add = store.clone();
    let revision = bridge.as_ref().map(|b| b.collections_revision);
    let all_liked = move || {
        if let Some(r) = revision {
            r.get();
        }
        let tracks = selected_tracks(props, at);
        !tracks.is_empty()
            && read
                .as_ref()
                .is_some_and(|s| tracks.iter().all(|t| s.is_liked(&t.entity_ref())))
    };
    view()
        .child(command(props, at, cx, "play", "播放"))
        .child(command(props, at, cx, "next", "下一首播放"))
        .child(command(props, at, cx, "append", "加入队列"))
        .child(when(
            move || revision.is_some(),
            move || {
                let all_liked = all_liked.clone();
                let write = write.clone();
                menu_item()
                    .app_class(ITEM)
                    .child(text(move || {
                        if all_liked() {
                            "取消喜欢"
                        } else {
                            "喜欢"
                        }
                    }))
                    .on_select(move || {
                        if let Some(store) = &write {
                            let tracks = selected_tracks(props, at);
                            let value = !tracks.iter().all(|t| store.is_liked(&t.entity_ref()));
                            for track in tracks {
                                if let Err(e) = store.set_liked(&track, Some(value)) {
                                    log::error!("Like: {e}");
                                }
                            }
                        }
                    })
            },
        ))
        .child(
            submenu()
                .trigger(view().app_class(ITEM).child(text("加入歌单 ▸")))
                .child(each(
                    move || {
                        if let Some(r) = revision {
                            r.get();
                        }
                        store
                            .as_ref()
                            .map(|s| {
                                s.list()
                                    .into_iter()
                                    .filter(|p| p.kind == "user")
                                    .map(|p| (p.id, p.title))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default()
                    },
                    |p| p.0.clone(),
                    move |item, _| {
                        let add = add.clone();
                        menu_item()
                            .app_class(ITEM)
                            .child(text(move || item.get().1))
                            .on_select(move || {
                                if let Some(store) = &add {
                                    if let Err(e) = store
                                        .add(&item.get_untracked().0, &selected_tracks(props, at))
                                    {
                                        log::error!("Add playlist: {e}");
                                    }
                                }
                            })
                    },
                ))
                .child(command(props, at, cx, "new-playlist", "新建歌单…")),
        )
        .child(when(
            move || {
                source(props)
                    .and_then(|s| s.provider.collection_id())
                    .is_some_and(|id| id != "__liked__")
            },
            move || {
                view()
                    .child(command(props, at, cx, "move-up", "上移"))
                    .child(command(props, at, cx, "move-down", "下移"))
            },
        ))
        .child(when(
            move || {
                source(props)
                    .and_then(|s| s.provider.collection_id())
                    .is_some()
            },
            move || command(props, at, cx, "remove", "从此列表移除"),
        ))
        .child(when(
            move || {
                selected_tracks(props, at)
                    .iter()
                    .all(|t| t.source == "local")
            },
            move || {
                view()
                    .child(command(props, at, cx, "edit", "编辑信息…"))
                    .child(when(
                        move || source(props).and_then(|s| s.provider.collection_id()).is_none(),
                        move || command(props, at, cx, "scrape", "刮削…"),
                    ))
                    .child(command(props, at, cx, "reveal", "在 Finder 中显示"))
            },
        ))
}
const ITEM: &str = "rounded-(--radius) min-h-8 px-3 py-1.5 text-sm text-popover-foreground hover:bg-accent active:bg-muted focus:bg-accent";
const POPUP: &str =
    "rounded-(--radius-surface) border border-border bg-popover shadow-lg min-w-48 p-1";

/// Virtualized compact tracks with ID selection, keyboard commands and context menus.
/// The source supplies IDs and resolves effective metadata; no full track array crosses TSX.
#[component]
pub fn TrackList(props: TrackListProps, cx: Scope<TrackListEvent>) -> impl IntoElement {
    let list = element_ref();
    // Filtering and rescanning can change the visible selection without an input event.
    effect(move || {
        if let Some(s) = source(props) {
            let ids = s.ids.get();
            let selected = s.selected.get();
            cx.emit(TrackListEvent::SelectionChange(
                ids.into_iter().filter(|id| selected.contains(id)).collect(),
            ));
        }
    });
    effect(move || {
        if !props.active.get() {
            return;
        }
        on_key("mod+a", move |_| {
            if let Some(s) = source(props) {
                s.selected.set(
                    s.ids
                        .with_untracked(|ids| ids.iter().cloned().collect::<BTreeSet<_>>()),
                );
                selection(props, cx);
            }
        });
        on_key("enter", move |_| {
            if let Some(s) = source(props) {
                if let Some(id) = s
                    .focused
                    .get_untracked()
                    .or_else(|| s.selected_ids().first().cloned())
                {
                    play(props, cx, id);
                }
            }
        });
        for (key, delta) in [
            ("up", -1isize),
            ("down", 1),
            ("home", isize::MIN),
            ("end", isize::MAX),
        ] {
            on_key(key, move |_| {
                if let Some(s) = source(props) {
                    let ids = s.ids.get_untracked();
                    if ids.is_empty() {
                        return;
                    }
                    let current = s
                        .focused
                        .get_untracked()
                        .and_then(|id| ids.iter().position(|v| *v == id))
                        .unwrap_or(0);
                    let next = if delta == isize::MIN {
                        0
                    } else if delta == isize::MAX {
                        ids.len() - 1
                    } else {
                        current.saturating_add_signed(delta).min(ids.len() - 1)
                    };
                    s.select(next, false, false);
                    list.scroll_to_index(next as u32, percentage(0.0), px(0.0), false);
                    selection(props, cx);
                }
            });
        }
    });
    let initial = source(props).map(|s| s.scroll.get_untracked()).unwrap_or(0);
    view()
        .flex_1()
        .min_h_0()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(move || theme().border)
                .child(
                    text("#")
                        .w_8()
                        .text_xs()
                        .text_color(move || theme().muted_foreground),
                )
                .child(
                    text("标题 / 艺术家")
                        .flex_1()
                        .text_xs()
                        .text_color(move || theme().muted_foreground),
                )
                .child(when(
                    move || props.album_column.get(),
                    || {
                        text("专辑")
                            .w_48()
                            .text_xs()
                            .text_color(move || theme().muted_foreground)
                    },
                ))
                .child(when(
                    move || props.duration_column.get(),
                    || {
                        text("时长")
                            .w_12()
                            .text_xs()
                            .text_color(move || theme().muted_foreground)
                    },
                ))
                .child(view().w_8()),
        )
        .child(
            virtual_list(
                move || source(props).map(|s| s.ids.with(Vec::len)).unwrap_or(0),
                move |at| {
                    let track = computed(move || {
                        let s = source(props)?;
                        s.revision.get();
                        s.provider.track(&id_at(props, at)?)
                    });
                    let row = view()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .h(move || px(theme().tokens().number("track-row-height").unwrap_or(48.0)))
                        .rounded(move || theme().radius)
                        .role(nsh_core::AccessibilityRole::Row)
                        .focusable(true)
                        .label(move || {
                            track.with(|t| t.as_ref().map(|t| t.title.clone()).unwrap_or_default())
                        })
                        .accessibility_state(move || nsh_core::AccessibilityState {
                            selected: source(props).is_some_and(|s| {
                                id_at(props, at)
                                    .is_some_and(|id| s.selected.with(|set| set.contains(&id)))
                            }),
                            ..Default::default()
                        })
                        .bg(move || {
                            let selected = source(props).is_some_and(|s| {
                                id_at(props, at)
                                    .is_some_and(|id| s.selected.with(|set| set.contains(&id)))
                            });
                            if selected {
                                theme().color("selection").unwrap_or(theme().accent)
                            } else {
                                theme().background
                            }
                        })
                        .hover(|s| s.bg(move || theme().accent))
                        .active(|s| s.bg(move || theme().muted))
                        .app_class("focus-visible:outline-2 focus-visible:outline-ring")
                        .on_press(move |event: &PressEvent| {
                            if let Some(s) = source(props) {
                                s.select(at.index(), event.modifiers.cmd, event.modifiers.shift);
                                selection(props, cx);
                                if event.clicks == 2 {
                                    if let Some(id) = id_at(props, at) {
                                        play(props, cx, id);
                                    }
                                }
                            }
                        })
                        .child(
                            text(move || {
                                if id_at(props, at).is_some_and(|id| {
                                    props.current_id.get().is_some_and(|v| v.as_str() == id)
                                }) {
                                    "▶".into()
                                } else {
                                    (at.index() + 1).to_string()
                                }
                            })
                            .w_8()
                            .text_xs()
                            .text_color(move || {
                                if id_at(props, at).is_some_and(|id| {
                                    props.current_id.get().is_some_and(|v| v.as_str() == id)
                                }) {
                                    theme().primary
                                } else {
                                    theme().muted_foreground
                                }
                            }),
                        )
                        .child(
                            view()
                                .flex_1()
                                .min_w_0()
                                .gap_1()
                                .child(
                                    text(move || {
                                        track.with(|t| {
                                            t.as_ref().map(|t| t.title.clone()).unwrap_or_default()
                                        })
                                    })
                                    .text_sm()
                                    .truncate()
                                    .text_color(move || theme().foreground),
                                )
                                .child(
                                    text(move || {
                                        track.with(|t| {
                                            t.as_ref()
                                                .map(artist_names)
                                                .filter(|v| !v.is_empty())
                                                .map(|v| v.join(" / "))
                                                .unwrap_or_else(|| "未知艺术家".into())
                                        })
                                    })
                                    .text_xs()
                                    .truncate()
                                    .text_color(move || theme().muted_foreground),
                                ),
                        )
                        .child(when(
                            move || props.album_column.get(),
                            move || {
                                text(move || {
                                    track.with(|t| {
                                        t.as_ref()
                                            .and_then(|t| t.album.as_ref())
                                            .map(|a| a.title.clone())
                                            .unwrap_or_else(|| "未知专辑".into())
                                    })
                                })
                                .w_48()
                                .text_sm()
                                .truncate()
                                .text_color(move || theme().muted_foreground)
                            },
                        ))
                        .child(when(
                            move || props.duration_column.get(),
                            move || {
                                text(move || {
                                    track.with(|t| {
                                        let seconds = t
                                            .as_ref()
                                            .and_then(|t| t.duration)
                                            .unwrap_or(0.)
                                            .max(0.)
                                            as u64;
                                        format!("{}:{:02}", seconds / 60, seconds % 60)
                                    })
                                })
                                .w_12()
                                .text_xs()
                                .text_color(move || theme().muted_foreground)
                            },
                        ))
                        .child(when(
                            move || crate::presentation::get().is_some(),
                            move || {
                                let p = crate::presentation::get().unwrap();
                                let revision = p.bridge.collections_revision;
                                let store = p.bridge.collections.clone();
                                text(move || {
                                    revision.get();
                                    if track.with(|t| {
                                        t.as_ref().is_some_and(|t| store.is_liked(&t.entity_ref()))
                                    }) {
                                        "♥"
                                    } else {
                                        ""
                                    }
                                })
                                .w_5()
                                .text_sm()
                                .text_color(move || theme().primary)
                            },
                        ))
                        .child(
                            menu()
                                .popup_class(POPUP)
                                .trigger(
                                    nsh_base::button()
                                        .accessibility_label("更多曲目操作")
                                        .size_8()
                                        .rounded(move || theme().radius)
                                        .hover(|s| s.bg(move || theme().muted))
                                        .child(
                                            text("…").text_color(move || theme().muted_foreground),
                                        ),
                                )
                                .child(commands(props, at, cx)),
                        );
                    view()
                        .child(when(
                            move || {
                                id_at(props, at)
                                    .and_then(|id| source(props)?.provider.group(&id))
                                    .is_some()
                            },
                            move || {
                                text(move || {
                                    id_at(props, at)
                                        .and_then(|id| source(props)?.provider.group(&id))
                                        .unwrap_or_default()
                                })
                                .app_class("px-3 pt-3 pb-1 text-xs text-muted-foreground")
                            },
                        ))
                        .child(
                            context_menu()
                                .popup_class(POPUP)
                                .on_open_change(move |event: &nsh_base::MenuEvent| {
                                    if matches!(event, nsh_base::MenuEvent::OpenChange(true, _)) {
                                        if let Some(s) = source(props) {
                                            if !id_at(props, at).is_some_and(|id| {
                                                s.selected.with_untracked(|set| set.contains(&id))
                                            }) {
                                                s.select(at.index(), false, false);
                                                selection(props, cx);
                                            }
                                        }
                                    }
                                })
                                .trigger(row)
                                .child(commands(props, at, cx)),
                        )
                },
            )
            .keys(move || source(props).map(|s| s.ids.get()).unwrap_or_default())
            .initial_scroll_index(initial)
            .estimated_item_size(move || {
                px(theme().tokens().number("track-row-height").unwrap_or(48.0))
            })
            .empty(
                text("没有符合条件的歌曲")
                    .p_5()
                    .text_sm()
                    .text_color(move || theme().muted_foreground),
            )
            .on_scroll(move |event: &ScrollEvent| {
                if let Some(s) = source(props) {
                    s.scroll.set(
                        (event.offset.y.0
                            / theme().tokens().number("track-row-height").unwrap_or(48.0))
                        .max(0.) as usize,
                    );
                }
            })
            .element_ref(&list)
            .flex_1()
            .min_h_0(),
        )
}
