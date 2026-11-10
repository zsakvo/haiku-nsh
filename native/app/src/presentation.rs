//! Engine-owned local presentation. Async reads are cancelled by effect owners;
//! request versions also reject late blocking completions.
use crate::core::CoreBridge;
use base64::Engine;
use haiku_library::service::Library;
use haiku_lyrics::{Cursor, Lyrics};
use haiku_model::Track;
use haiku_palette::{Color, RequestVersion};
use nsh_core::prelude::*;
use std::{cell::RefCell, collections::VecDeque, io::Read, path::PathBuf, rc::Rc, sync::Arc};

#[derive(Clone)]
struct Cover {
    palette: Vec<Color>,
    uri: String,
}

pub(crate) struct Presentation {
    _owner: Owner,
    pub bridge: Rc<CoreBridge>,
    pub track: Signal<Option<Track>>,
    pub artwork: Signal<String>,
    pub lyrics: Signal<Arc<Lyrics>>,
    pub line: Signal<Option<usize>>,
    pub open: Signal<bool>,
    palette: RefCell<Option<Vec<Color>>>,
    cache: RefCell<VecDeque<(String, Cover)>>,
    request: RefCell<RequestVersion>,
}
pub(crate) fn get() -> Option<Rc<Presentation>> {
    global::<Option<Rc<Presentation>>>().get()
}
pub(crate) fn reapply_theme() {
    if let Some(p) = get() {
        let palette = p
            .palette
            .borrow()
            .clone()
            .unwrap_or_else(crate::player_theme::accent_palette);
        crate::player_theme::apply(&palette);
    }
}
fn cover_path(track: &Track) -> Option<String> {
    track
        .cover
        .as_ref()
        .filter(|p| !p.starts_with("http"))
        .map(|p| p.strip_prefix("file://").unwrap_or(p).to_owned())
}
fn cover_key(path: &str) -> String {
    let meta = std::fs::metadata(path).ok();
    format!(
        "{path}:{:?}:{:?}",
        meta.as_ref().and_then(|m| m.modified().ok()),
        meta.map(|m| m.len())
    )
}
fn load_cover(path: String) -> Option<Cover> {
    let palette = haiku_palette::extract_file(&path).ok()?;
    let mut bytes = Vec::new();
    std::fs::File::open(&path).ok()?.take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes).ok()?;
    if bytes.len() > 16 * 1024 * 1024 { return None; }
    let mime = if bytes.starts_with(b"\x89PNG") { "image/png" }
        else if bytes.starts_with(b"GIF8") { "image/gif" }
        else if bytes.starts_with(b"RIFF") { "image/webp" }
        else { "image/jpeg" };
    // Native local media is app-owned. The image service receives bytes, never an
    // ungranted file URL; the same bounded cache prewarms artwork and palette.
    Some(Cover { palette, uri: format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)) })
}
fn parse_local(path: Option<String>) -> Lyrics {
    let Some(local) = path.and_then(|p| haiku_library::local_scan::read_lyrics(&PathBuf::from(p)))
    else {
        return Lyrics::default();
    };
    let mut parsed = haiku_lyrics::parse(&local.text);
    if let Some(translation) = local.translation {
        haiku_lyrics::merge_translation(&mut parsed, &translation);
    }
    parsed
}
pub(crate) fn initialize(bridge: Rc<CoreBridge>, library: Library) {
    let owner = Owner::detached();
    let p = owner.run(|| {
        Rc::new(Presentation {
            _owner: owner,
            bridge,
            track: signal(None),
            artwork: signal(String::new()),
            lyrics: signal(Arc::new(Lyrics::default())),
            line: signal(None),
            open: signal(false),
            palette: RefCell::new(None),
            cache: RefCell::new(VecDeque::new()),
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
            let effective = selected.map(|t| library_for_track.track(&t.id).unwrap_or(t));
            if track.get_untracked() != effective {
                track.set(effective);
            }
        });
        let weak = Rc::downgrade(&p);
        effect(move || {
            let selected = track.get();
            let Some(p) = weak.upgrade() else { return };
            let version = p.request.borrow_mut().advance();
            p.lyrics.set(Arc::new(Lyrics::default()));
            p.line.set(None);
            let art = selected.as_ref().and_then(cover_path);
            let key = art.as_deref().map(cover_key);
            let cached = key.as_ref().and_then(|key| {
                p.cache
                    .borrow()
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.clone())
            });
            let cold = cached.is_none();
            p.artwork.set(cached.as_ref().map(|c|c.uri.clone()).unwrap_or_default());
            let palette = cached.map(|c|c.palette);
            *p.palette.borrow_mut() = palette.clone();
            crate::player_theme::apply(&palette.unwrap_or_else(crate::player_theme::accent_palette));
            let path = selected.and_then(|t| t.local_path);
            let weak = Rc::downgrade(&p);
            spawn(async move {
                let result = blocking(move || {
                    let lyrics = parse_local(path);
                    let cover = art.filter(|_|cold).and_then(load_cover);
                    (lyrics, cover)
                })
                .await;
                let Some(p) = weak.upgrade() else { return };
                if !p.request.borrow().accepts(version) {
                    return;
                }
                if let Ok((lyrics, cover)) = result {
                    log::info!("M1-04 local lyrics loaded: {} lines", lyrics.lines.len());
                    p.lyrics.set(Arc::new(lyrics));
                    if let Some(cover) = cover {
                        if let Some(key) = key {
                            p.remember(key, cover.clone());
                        }
                        p.artwork.set(cover.uri);
                        *p.palette.borrow_mut() = Some(cover.palette.clone());
                        crate::player_theme::apply(&cover.palette);
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
        let weak = Rc::downgrade(&p);
        effect(move || {
            revision.get();
            let next = state.with(|s| s.next_index.and_then(|i| s.tracks.get(i as usize).cloned()));
            let next = next.map(|t| library.track(&t.id).unwrap_or(t));
            let Some(path) = next.as_ref().and_then(cover_path) else {
                return;
            };
            let key = cover_key(&path);
            let Some(p) = weak.upgrade() else { return };
            if p.cache.borrow().iter().any(|(k, _)| *k == key) {
                return;
            }
            let weak = Rc::downgrade(&p);
            spawn(async move {
                if let Ok(Some(cover)) = blocking(move || load_cover(path)).await {
                    if let Some(p) = weak.upgrade() {
                        p.remember(key, cover);
                    }
                }
            });
        });
    });
}
impl Presentation {
    fn remember(&self, key: String, cover: Cover) {
        let mut cache = self.cache.borrow_mut();
        cache.retain(|(k, _)| *k != key);
        cache.push_front((key, cover));
        cache.truncate(12);
    }
}
