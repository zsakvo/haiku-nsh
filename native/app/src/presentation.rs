//! Engine-owned local presentation. Async reads are cancelled by effect owners;
//! request versions also reject late blocking completions.
use crate::core::CoreBridge;
use base64::Engine;
use haiku_library::service::Library;
use haiku_lyrics::{Cursor, Lyrics};
use haiku_model::Track;
use haiku_palette::{CoverTheme, RequestVersion};
use nsh_core::prelude::*;
use std::{cell::RefCell, collections::VecDeque, io::Read, rc::Rc, sync::Arc};

#[derive(Clone)]
struct Cover {
    theme: CoverTheme,
    uri: String,
    path: String,
}

struct TrackCover {
    identity: String,
    key: String,
    theme: CoverTheme,
}

pub(crate) struct Presentation {
    _owner: Owner,
    pub bridge: Rc<CoreBridge>,
    pub track: Signal<Option<Track>>,
    pub artwork: Signal<String>,
    pub artwork_path: Signal<Option<String>>,
    pub lyrics: Signal<Arc<Lyrics>>,
    pub line: Signal<Option<usize>>,
    pub open: Signal<bool>,
    cover_theme: RefCell<Option<CoverTheme>>,
    cache: RefCell<VecDeque<(String, Cover)>>,
    /// Best cover known per track identity, so a switch to a prewarmed track resolves
    /// synchronously instead of waiting on a key that needs a plugin call to compute.
    /// The palette is locked to the first cover resolved for the track: the small and
    /// large renditions of one cover can quantize to different colors, and a second
    /// palette change for the same track reads as a glitch.
    by_track: RefCell<VecDeque<TrackCover>>,
    request: RefCell<RequestVersion>,
}
pub(crate) fn get() -> Option<Rc<Presentation>> {
    global::<Option<Rc<Presentation>>>().get()
}
pub(crate) fn apply_cover_into(tokens: &mut nsh_core::element::Tokens) {
    if let Some(p) = get() {
        let theme = p.cover_theme.borrow().clone();
        crate::player_theme::apply_into(tokens, theme.as_ref());
    }
}
fn cover_key(path: &str) -> String {
    let meta = std::fs::metadata(path.strip_prefix("file://").unwrap_or(path)).ok();
    format!(
        "{path}:{:?}:{:?}",
        meta.as_ref().and_then(|m| m.modified().ok()),
        meta.map(|m| m.len())
    )
}
fn load_cover(path: String) -> Option<Cover> {
    let theme = haiku_palette::extract_cover_theme_file(&path).ok()?;
    let mut bytes = Vec::new();
    std::fs::File::open(&path)
        .ok()?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 16 * 1024 * 1024 {
        return None;
    }
    let mime = if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.starts_with(b"RIFF") {
        "image/webp"
    } else {
        "image/jpeg"
    };
    // Native local media is app-owned. The image service receives bytes, never an
    // ungranted file URL; the same bounded cache prewarms artwork and palette.
    Some(Cover {
        theme,
        path,
        uri: format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ),
    })
}
pub(crate) fn initialize(bridge: Rc<CoreBridge>, library: Library) {
    let owner = Owner::detached();
    let p = owner.run(|| {
        Rc::new(Presentation {
            _owner: owner,
            bridge,
            track: signal(None),
            artwork: signal(String::new()),
            artwork_path: signal(None),
            lyrics: signal(Arc::new(Lyrics::default())),
            line: signal(None),
            open: signal(false),
            cover_theme: RefCell::new(None),
            cache: RefCell::new(VecDeque::new()),
            by_track: RefCell::new(VecDeque::new()),
            request: RefCell::new(RequestVersion::default()),
        })
    });
    global::<Option<Rc<Presentation>>>().set(Some(p.clone()));
    owner.run(|| {
        let revision = signal(library.subscribe().borrow().revision);
        let mut events = library.subscribe();
        spawn(async move {
            while events.changed().await.is_ok() {
                revision.set(events.borrow_and_update().revision);
            }
        });
        let state = p.bridge.state;
        let track = p.track;
        let library_for_track = library.clone();
        effect(move || {
            revision.get();
            let selected = state.with(|s| s.current.clone());
            let effective = selected.map(|t| {
                let fallback_duration = t.duration.filter(|d| d.is_finite() && *d > 0.);
                let mut resolved = library_for_track.track(&t.id).unwrap_or(t);
                if !resolved.duration.is_some_and(|d| d.is_finite() && d > 0.) {
                    resolved.duration = fallback_duration;
                }
                resolved
            });
            if track.get_untracked() != effective {
                track.set(effective);
            }
        });
        let weak = Rc::downgrade(&p);
        let reset = weak.clone();
        bind_lyrics(
            move || {
                let p = weak.upgrade()?;
                Some((
                    p.track.get(),
                    p.bridge.state.with(|s| s.generation),
                    serde_json::from_value(serde_json::to_value(p.bridge.settings.get()).unwrap())
                        .unwrap_or_default(),
                ))
            },
            p.bridge.lyrics.clone(),
            Arc::new(crate::lyrics::Sources(p.bridge.source_host.clone())),
            move || {
                if let Some(p) = reset.upgrade() {
                    p.lyrics.set(Arc::new(Lyrics::default()));
                    p.line.set(None);
                }
            },
        );
        let weak = Rc::downgrade(&p);
        let mut lyric_events = p.bridge.lyrics.subscribe();
        spawn(async move {
            while lyric_events.changed().await.is_ok() {
                let Some(p) = weak.upgrade() else { return };
                let state = lyric_events.borrow_and_update().clone();
                p.lyrics.set(Arc::new(state.lyrics.unwrap_or_default()));
            }
        });
        // Artwork depends on the track identity and its cover only. Duration arrives from
        // metadata and from the decoder while the stream buffers; rerunning on those updates
        // cancelled the pending cover and restarted it after buffering finished.
        let artwork_track = computed(move || {
            track.get().map(|mut t| {
                t.duration = None;
                t
            })
        });
        let weak = Rc::downgrade(&p);
        effect(move || {
            let selected = artwork_track.get();
            let Some(p) = weak.upgrade() else { return };
            let version = p.request.borrow_mut().advance();
            let Some(track) = selected else {
                p.clear_cover();
                return;
            };
            let identity = track_identity(&track);
            // The system media center must not keep the previous track's image.
            p.artwork_path.set(None);
            // A prewarmed successor switches in the same tick: artwork, palette and
            // tokens change together, and the background transitions from the old colors.
            // On a miss the previous cover stays visible until the new one is decoded,
            // never degrading to the fallback palette in between.
            let mut shown = p.cached_for(&identity).map(|(key, cover)| {
                p.show(&identity, key.clone(), cover);
                key
            });
            let weak = Rc::downgrade(&p);
            spawn(async move {
                let show = |key: String, cover: Cover| {
                    let Some(p) = weak.upgrade() else { return false };
                    if !p.request.borrow().accepts(version) {
                        return false;
                    }
                    p.show(&identity, key, cover);
                    true
                };
                // The track's own artwork needs no plugin call, so it is shown at once instead of
                // queueing behind stream resolution in the source's serial runtime.
                // A cached cover is already the best one prewarming found; going back to the
                // smaller direct artwork would visibly downgrade it.
                let mut found = shown.is_some();
                let direct = if found { None } else { direct_cover(weak.clone(), track.clone()).await };
                if let Some((key, cover)) = direct {
                    found = true;
                    if shown.as_ref() != Some(&key) {
                        let _ = prefetch_image(cover.uri.clone()).await;
                        if !show(key.clone(), cover) {
                            return;
                        }
                        shown = Some(key);
                    }
                }
                // A source may still provide larger artwork through track.cover.
                if let Some((key, cover)) = prepared_cover(weak.clone(), track).await {
                    if shown.as_ref() != Some(&key) {
                        let _ = prefetch_image(cover.uri.clone()).await;
                        show(key, cover);
                    }
                    return;
                }
                // Only a track that truly has no artwork falls back to the default palette.
                if !found {
                    if let Some(p) = weak.upgrade() {
                        if p.request.borrow().accepts(version) {
                            p.clear_cover();
                        }
                    }
                }
            });
        });
        // Cursor publishes line changes only. Word fills read the same Rust clock directly.
        let lyrics = p.lyrics;
        let line = p.line;
        let position = p.bridge.position;
        let cursor = stored(Cursor::default());
        let previous_lyrics = stored(Arc::new(Lyrics::default()));
        effect(move || {
            let lyrics = lyrics.get();
            let seconds = position.get();
            cursor.with(|cursor| {
                if !Arc::ptr_eq(&previous_lyrics.get(), &lyrics) {
                    previous_lyrics.replace(lyrics.clone());
                    *cursor = Cursor::default();
                }
                if cursor.update(&lyrics, seconds) {
                    line.set(cursor.current());
                    log::info!(
                        "M1-04 lyric cursor: {:?} at {:.3}",
                        cursor.current(),
                        seconds
                    );
                }
            });
        });
        // The controller supplies the actual planned successor for shuffle/repeat-one.
        // Every state publish (status, buffering, duration) used to rerun this and enqueue
        // another track.cover call; only a different successor needs prewarming.
        let next_track = computed(move || {
            revision.get();
            let next = state.with(|s| s.next_index.and_then(|i| s.tracks.get(i as usize).cloned()));
            next.map(|t| {
                let mut t = library.track(&t.id).unwrap_or(t);
                t.duration = None;
                t
            })
        });
        let weak = Rc::downgrade(&p);
        effect(move || {
            let Some(track) = next_track.get() else { return };
            let weak = weak.clone();
            // Prewarm exactly what the switch will look up: the track's own artwork, the
            // source's larger cover, and the decoded native image for each.
            spawn(async move {
                let identity = track_identity(&track);
                for cover in [
                    direct_cover(weak.clone(), track.clone()).await,
                    prepared_cover(weak.clone(), track).await,
                ] {
                    let Some((key, cover)) = cover else { continue };
                    let _ = prefetch_image(cover.uri.clone()).await;
                    let Some(p) = weak.upgrade() else { return };
                    p.remember(key.clone(), cover.clone());
                    p.associate(&identity, key, &cover.theme);
                }
            });
        });
    });
}
// Current and next share the same bounded decoded cache, keyed by source + final URL.
async fn prepared_cover(
    weak: std::rc::Weak<Presentation>,
    mut track: Track,
) -> Option<(String, Cover)> {
    let host = weak.upgrade()?.bridge.source_host.clone();
    let input = track.clone();
    let url = background(async move { crate::sources::playback_cover(&host, &input).await })
        .await
        .ok()??;
    track.cover = Some(url);
    load_artwork(weak, track).await
}
fn track_identity(track: &Track) -> String {
    format!("{}:{}", track.source, track.id)
}
/// The track's own artwork only, without asking the source for a larger image.
async fn direct_cover(weak: std::rc::Weak<Presentation>, mut track: Track) -> Option<(String, Cover)> {
    let image = track.cover.clone()?;
    let host = weak.upgrade()?.bridge.source_host.clone();
    track.cover = Some(crate::sources::cover_url(&host, &track.source, &image, 1600));
    load_artwork(weak, track).await
}
/// Fetch (or reuse) and decode `track.cover`, already resolved to its final URL.
async fn load_artwork(weak: std::rc::Weak<Presentation>, track: Track) -> Option<(String, Cover)> {
    let url = track.cover.clone()?;
    let p = weak.upgrade()?;
    let host = p.bridge.source_host.clone();
    let root = p.bridge.artwork_root.clone();
    drop(p);
    let key = format!("{}:{}", track.source, cover_key(&url));
    let cached = weak
        .upgrade()?
        .cache
        .borrow()
        .iter()
        .find(|(k, _)| k == &key)
        .map(|(_, c)| c.clone());
    if let Some(cover) = cached {
        return Some((key, cover));
    }
    let path = background(crate::sources::artwork(host, root, track))
        .await
        .ok()??;
    let cover = blocking(move || load_cover(path)).await.ok()??;
    Some((key, cover))
}

impl Presentation {
    pub(crate) fn duration(&self) -> f64 {
        let track_duration = self
            .track
            .with(|t| t.as_ref().and_then(|t| t.duration))
            .filter(|d| d.is_finite() && *d > 0.);
        track_duration.unwrap_or_else(|| {
            let decoded = self.bridge.duration.get();
            if decoded.is_finite() && decoded > 0. {
                decoded
            } else {
                0.
            }
        })
    }
    pub(crate) fn duration_untracked(&self) -> f64 {
        let track_duration = self
            .track
            .with_untracked(|t| t.as_ref().and_then(|t| t.duration))
            .filter(|d| d.is_finite() && *d > 0.);
        track_duration.unwrap_or_else(|| {
            let decoded = self.bridge.duration.get_untracked();
            if decoded.is_finite() && decoded > 0. {
                decoded
            } else {
                0.
            }
        })
    }
    fn show(&self, identity: &str, key: String, cover: Cover) {
        self.remember(key.clone(), cover.clone());
        let theme = self.associate(identity, key, &cover.theme);
        self.artwork.set(cover.uri);
        self.artwork_path.set(Some(cover.path));
        // Same palette (e.g. the larger rendition of the same track): swap only the image.
        if self.cover_theme.borrow().as_ref() == Some(&theme) {
            return;
        }
        crate::player_theme::apply(Some(&theme));
        *self.cover_theme.borrow_mut() = Some(theme);
    }
    fn clear_cover(&self) {
        self.artwork.set(String::new());
        self.artwork_path.set(None);
        if self.cover_theme.borrow_mut().take().is_some() {
            crate::player_theme::apply(None);
        }
    }
    fn cached_for(&self, identity: &str) -> Option<(String, Cover)> {
        let key = self
            .by_track
            .borrow()
            .iter()
            .find(|t| t.identity == identity)
            .map(|t| t.key.clone())?;
        let cover = self
            .cache
            .borrow()
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, c)| c.clone())?;
        Some((key, cover))
    }
    /// Points the track at its best cover (the later, larger one replaces the direct one)
    /// and returns the track's locked palette: the first one recorded for it wins.
    fn associate(&self, identity: &str, key: String, theme: &CoverTheme) -> CoverTheme {
        let mut tracks = self.by_track.borrow_mut();
        let theme = match tracks.iter().position(|t| t.identity == identity) {
            Some(i) => tracks.remove(i).unwrap().theme,
            None => theme.clone(),
        };
        tracks.push_front(TrackCover {
            identity: identity.to_owned(),
            key,
            theme: theme.clone(),
        });
        tracks.truncate(24);
        theme
    }
    fn remember(&self, key: String, cover: Cover) {
        let mut cache = self.cache.borrow_mut();
        cache.retain(|(k, _)| *k != key);
        cache.push_front((key, cover));
        cache.truncate(12);
    }
}

// Effect-owned tasks are canceled before an effect reruns. Compare request inputs in a
// memo so pause/progress or unrelated settings cannot cancel a pending local lyric read.
fn bind_lyrics(
    inputs: impl Fn() -> Option<(Option<Track>, u64, haiku_lyrics::service::Options)> + 'static,
    service: Arc<haiku_lyrics::service::Service>,
    origin: Arc<dyn haiku_lyrics::service::Origin>,
    reset: impl Fn() + 'static,
) {
    let request = computed(move || {
        inputs().map(|(track, generation, options)| {
            (track, generation, serde_json::to_value(options).unwrap())
        })
    });
    effect(move || {
        let Some((track, _, options)) = request.get() else {
            return;
        };
        let generation = service.clear(track.clone());
        reset();
        if let Some(track) = track {
            let service = service.clone();
            let origin = origin.clone();
            let options = serde_json::from_value(options).unwrap();
            spawn(async move {
                let _ = background(async move {
                    service
                        .load_generation(track, options, origin, generation)
                        .await;
                })
                .await;
            });
        }
    });
}

#[cfg(test)]
#[path = "presentation_tests.rs"]
mod tests;
