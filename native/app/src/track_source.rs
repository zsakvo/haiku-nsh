//! Engine-local data-source handles. Recycled rows always resolve their current id.
use haiku_model::Track;
use nsh_core::prelude::*;
use std::{collections::{BTreeMap, BTreeSet}, rc::Rc};

/// A reusable Rust source; pages keep only its engine-local handle.
pub trait TrackDataSource {
    fn ids(&self) -> Vec<String>;
    fn track(&self, id: &str) -> Option<Track>;
}
#[derive(Clone)]
pub struct Source {
    pub provider: Rc<dyn TrackDataSource>,
    pub ids: Signal<Vec<String>>,
    pub selected: Signal<BTreeSet<String>>,
    pub anchor: Signal<Option<String>>,
    pub focused: Signal<Option<String>>,
    pub revision: Signal<u64>,
    pub scroll: Signal<usize>,
}
impl Source {
    pub fn refresh(&self) { self.ids.set(self.provider.ids()); self.revision.update(|r| *r += 1); }
    pub fn select(&self, index: usize, toggle: bool, range: bool) {
        let ids = self.ids.get_untracked(); let Some(id) = ids.get(index) else { return; };
        if range {
            let anchor = self.anchor.get_untracked().and_then(|id| ids.iter().position(|v| v == &id)).unwrap_or(index);
            self.selected.set(ids[anchor.min(index)..=anchor.max(index)].iter().cloned().collect());
        } else if toggle { self.selected.update(|s| { if !s.remove(id) { s.insert(id.clone()); } }); self.anchor.set(Some(id.clone())); }
        else { self.selected.set(BTreeSet::from([id.clone()])); self.anchor.set(Some(id.clone())); }
        self.focused.set(Some(id.clone()));
    }
    pub fn selected_ids(&self) -> Vec<String> { self.selected.with_untracked(|s| s.iter().cloned().collect()) }
}
#[derive(Default)]
struct Registry { next: u32, sources: BTreeMap<u32,Source> }
pub fn register(provider: Rc<dyn TrackDataSource>) -> u32 {
    let source = Source { ids:signal(provider.ids()),provider,selected:signal(BTreeSet::new()),anchor:signal(None),focused:signal(None),revision:signal(0),scroll:signal(0) };
    global::<Registry>().update_returning(|r| { r.next += 1; r.sources.insert(r.next,source); r.next })
}
pub fn get(handle: u32) -> Option<Source> { global::<Registry>().with_untracked(|r| r.sources.get(&handle).cloned()) }
pub fn remove(handle: u32) { global::<Registry>().update(|r| { r.sources.remove(&handle); }); }
