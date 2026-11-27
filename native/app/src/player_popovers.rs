//! Dock popup state belongs to the dock; native controls only emit commands.
use crate::{
    player_controls::{POPUP, compact_surface_control, control, icon, small_control},
    player_dock::send,
    presentation,
};
use haiku_playback::Command;
use nsh_base::{PopoverEvent, SliderValue, popover, slider, theme};
use nsh_core::{
    element::{AnyElement, NativeEvent},
    geometry::Axis,
    prelude::*,
};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Clone, Copy, PartialEq)]
enum Popup {
    Volume,
    Queue,
}

pub(crate) fn controls() -> (AnyElement, AnyElement) {
    let p = presentation::get().expect("Core.open before dock popovers");
    let state = p.bridge.state;
    let open = signal(None::<Popup>);
    let expanded = p.open;
    effect(move || {
        if expanded.get() {
            open.set(None);
        }
    });
    effect(move || {
        if state.with(|s| s.tracks.is_empty()) && open.get_untracked() == Some(Popup::Queue) {
            open.set(None);
        }
    });
    #[cfg(target_os = "macos")]
    if haiku_macos::player_popover::available() {
        let volume_c = p.bridge.controller.clone();
        let volume = compact_surface_control("音量", "volume", false.into())
            .selected(move || open.get() == Some(Popup::Volume))
            .on_press(move || {
                open.update(|v| {
                    *v = if *v == Some(Popup::Volume) {
                        None
                    } else {
                        Some(Popup::Volume)
                    }
                })
            })
            .child(
                nsh_core::element::native("haiku.VolumePopover")
                    .absolute()
                    .inset_0()
                    .pointer_events(PointerEvents::None)
                    .prop("open", move || json!(open.get() == Some(Popup::Volume)))
                    .prop("dark", move || json!(nsh_core::element::dark()))
                    .prop("animate", move || json!(!reduce_motion()))
                    .prop("volume", move || json!(state.with(|s| s.volume)))
                    .on_native_event(move |e: &NativeEvent| match e.name.as_ref() {
                        "open" if e.data == false => {
                            if open.get_untracked() == Some(Popup::Volume) {
                                open.set(None);
                            }
                        }
                        "volume" => {
                            if let Some(v) = e.data.as_f64().filter(|v| v.is_finite()) {
                                send(&volume_c, Command::SetVolume(v.clamp(0., 1.)));
                            }
                        }
                        _ => {}
                    }),
            )
            .into_element();

        let queue_version = computed(move || state.with(|s| s.queue_version));
        let queue = computed(move || {
            queue_version.get();
            state.with_untracked(|s| json!({
            "version": s.queue_version,
            "tracks": s.tracks.iter().map(|t| json!({
                "identity": t.entity_ref().encode(), "title": t.title,
                "artist": haiku_library::service::artist_names(t).join(" / "),
            })).collect::<Vec<_>>(),
        }))
        });
        let artwork = signal(HashMap::<String, String>::new());
        let pending = stored(HashSet::<String>::new());
        let limit = Arc::new(tokio::sync::Semaphore::new(4));
        let queue_c = p.bridge.controller.clone();
        let queue = compact_surface_control("播放队列", "queue", false.into())
            .disabled(move || state.with(|s| s.tracks.is_empty()))
            .selected(move || open.get() == Some(Popup::Queue))
            .on_press(move || {
                open.update(|v| {
                    *v = if *v == Some(Popup::Queue) {
                        None
                    } else {
                        Some(Popup::Queue)
                    }
                })
            })
            .child(
                nsh_core::element::native("haiku.QueuePopover")
                    .absolute()
                    .inset_0()
                    .pointer_events(PointerEvents::None)
                    .prop("open", move || json!(open.get() == Some(Popup::Queue)))
                    .prop("dark", move || json!(nsh_core::element::dark()))
                    .prop("animate", move || json!(!reduce_motion()))
                    .prop("queue", move || queue.get())
                    .prop("current", move || json!(state.with(|s| s.index)))
                    .prop("artwork", move || json!(artwork.get()))
                    .prop("colors", move || {
                        let rgba = |c| {
                            let c = color(c).to_rgb();
                            json!([c.r, c.g, c.b, c.a])
                        };
                        json!({
                            "active": rgba(theme().primary),
                            "foreground": rgba(theme().popover_foreground),
                            "muted": rgba(theme().muted_foreground),
                        })
                    })
                    .on_native_event(move |e: &NativeEvent| {
                        if e.name.as_ref() == "open" {
                            if e.data == false && open.get_untracked() == Some(Popup::Queue) {
                                open.set(None);
                            }
                            return;
                        }
                        if e.name.as_ref() == "clear" {
                            if e.data["version"].as_u64()
                                == Some(state.with_untracked(|s| s.queue_version))
                            {
                                send(&queue_c, Command::Clear);
                            }
                            return;
                        }
                        let Some(index) = e.data["index"]
                            .as_u64()
                            .and_then(|v| usize::try_from(v).ok())
                        else {
                            return;
                        };
                        let Some(version) = e.data["version"].as_u64() else {
                            return;
                        };
                        let Some(identity) = e.data["identity"].as_str() else {
                            return;
                        };
                        let track = state.with_untracked(|s| {
                            (s.queue_version == version)
                                .then(|| s.tracks.get(index).cloned())
                                .flatten()
                                .filter(|t| t.entity_ref().encode() == identity)
                        });
                        let Some(mut track) = track else { return };
                        match e.name.as_ref() {
                            "jump" => send(
                                &queue_c,
                                Command::JumpIfQueue {
                                    version,
                                    index: index as i32,
                                    identity: identity.to_owned(),
                                },
                            ),
                            "remove" => send(&queue_c, Command::Remove(index as i32)),
                            "artwork" => {
                                let key = identity.to_owned();
                                if artwork.with_untracked(|a| a.contains_key(&key))
                                    || pending.with(|p| p.contains(&key))
                                {
                                    return;
                                }
                                pending.with(|p| {
                                    p.insert(key.clone());
                                });
                                let host = p.bridge.source_host.clone();
                                track.cover = track.cover.map(|url| {
                                    crate::sources::cover_url(&host, &track.source, &url, 80)
                                });
                                let root = p.bridge.artwork_root.clone();
                                let limit = limit.clone();
                                spawn(async move {
                                    let path = background(async move {
                                        let _permit = limit.acquire_owned().await.ok()?;
                                        crate::sources::artwork(host, root, track).await
                                    })
                                    .await
                                    .ok()
                                    .flatten();
                                    pending.with(|p| {
                                        p.remove(&key);
                                    });
                                    if state.with_untracked(|s| s.queue_version == version) {
                                        if let Some(path) = path {
                                            artwork.update(|a| {
                                                if a.len() >= 256 {
                                                    a.clear();
                                                }
                                                a.insert(key, path);
                                            });
                                        }
                                    }
                                });
                            }
                            _ => {}
                        }
                    }),
            )
            .into_element();
        return (volume, queue);
    }
    // The same controls remain usable in the headless runner and other targets.
    let volume_c = p.bridge.controller.clone();
    let volume = popover().placement(Placement::Top).popup_class(POPUP)
        .open(move || Some(open.get() == Some(Popup::Volume)))
        .on_open_change(move |e: &PopoverEvent| if let PopoverEvent::OpenChange(value, _) = e {
            open.set(value.then_some(Popup::Volume));
        })
        .trigger(compact_surface_control("音量", "volume", false.into()))
        .child(view().w_12().items_center().gap_3()
            .child(slider().axis(Axis::Vertical).h_36().w_6().min(0.).max(1.).step(0.01)
                .value(move || Some(SliderValue::Single(state.with(|s| s.volume))))
                .accessibility_label("播放音量")
                .track_class("w-1 rounded-full bg-muted")
                .range_class("w-1 rounded-full bg-foreground")
                .indicator_class("w-1 bg-foreground")
                .thumb_class("size-3 rounded-full bg-foreground border border-border focus-visible:outline-2 focus-visible:outline-ring")
                .on_change(move |v: &SliderValue| send(&volume_c, Command::SetVolume(v.start()))))
            .child(text(move || format!("{:.0}%", state.with(|s| s.volume) * 100.))
                .text_xs().text_color(move || theme().popover_foreground))).into_element();
    let list = element_ref();
    let queue_c = p.bridge.controller.clone();
    let clear_c = queue_c.clone();
    let host = p.bridge.source_host.clone();
    let queue = popover().placement(Placement::Top).align(Align::End).popup_class(POPUP)
        .open(move || Some(open.get() == Some(Popup::Queue)))
        .on_open_change(move |e: &PopoverEvent| if let PopoverEvent::OpenChange(value, _) = e {
            open.set(value.then_some(Popup::Queue));
        })
        .on_open(move || {
            let current = state.with_untracked(|s| s.index);
            if current >= 0 { list.scroll_to_index(current as u32, percentage(0.5), px(0.), false); }
        })
        .trigger(compact_surface_control("播放队列", "queue", false.into())
            .disabled(move || state.with(|s| s.tracks.is_empty())))
        .child(view().w_72().gap_3()
            .child(view().flex_row().items_center().gap_2()
                .child(text("播放列表").text_sm().font_weight(FontWeight::SEMIBOLD).text_color(move || theme().popover_foreground))
                .child(text(move || state.with(|s| s.tracks.len().to_string())).text_xs().text_color(move || theme().muted_foreground))
                .child(view().flex_1())
                .child(control("清空播放队列", "trash", false)
                    .disabled(move || state.with(|s| s.tracks.is_empty()))
                    .on_press(move || send(&clear_c, Command::Clear))))
            .child(virtual_list(move || state.with(|s| s.tracks.len()), move |at| {
                let jump = queue_c.clone();
                let remove = queue_c.clone();
                let track = computed(move || state.with(|s| s.tracks.get(at.index()).cloned()));
                let active = move || state.with(|s| s.index >= 0 && s.index as usize == at.index());
                let host = host.clone();
                let cover = computed(move || track.with(|t| t.as_ref().and_then(|t| t.cover.as_ref()
                    .map(|url| crate::sources::cover_url(&host, &t.source, url, 80))).unwrap_or_default()));
                view().h_14().flex_row().items_center().gap_2()
                    .child(nsh_base::button().flex_1().min_w_0().h_full().flex_row().items_center().gap_3().px_2()
                        .accessibility_label(move || Some(track.with(|t| t.as_ref().map(|t| format!("播放 {}", t.title)).unwrap_or_default()).into()))
                        .selected(active)
                        .app_class("rounded-(--radius) active:opacity-60 focus-visible:outline-2 focus-visible:outline-ring")
                        .on_press(move || send(&jump, Command::Jump(at.index() as i32)))
                        .child(view().size_10().flex_shrink_0().rounded(move || theme().radius).overflow_hidden()
                            .items_center().justify_center().bg(move || theme().muted)
                            .child(when(move || !cover.get().is_empty(), move || image(move || cover.get()).size_full().object_fit(ResizeMode::Cover)))
                            .child(when(move || cover.get().is_empty(), || icon("music", false))))
                        .child(view().flex_1().min_w_0().gap_0p5()
                            .child(text(move || track.with(|t| t.as_ref().map(|t| t.title.clone()).unwrap_or_default()))
                                .text_sm().truncate()
                                .text_color(move || if active() { theme().primary } else { theme().popover_foreground }))
                            .child(text(move || track.with(|t| t.as_ref().map(|t| haiku_library::service::artist_names(t).join(" / ")).unwrap_or_default()))
                                .text_xs().truncate().text_color(move || theme().muted_foreground))))
                    .child(small_control("从队列移除", "close").on_press(move || send(&remove, Command::Remove(at.index() as i32))))
            }).element_ref(&list).estimated_item_size(px(56.)).h(px(448.)))).into_element();
    (volume, queue)
}
