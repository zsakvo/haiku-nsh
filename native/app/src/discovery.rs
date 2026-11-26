//! Per-source first-page home cache. Domain models only; no UI state or credentials.
use haiku_plugins::{Home, Host, Result, SourceApi};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
const TTL: u64 = 7 * 24 * 60 * 60;
#[derive(Serialize, Deserialize)]
struct Cached { version: u32, saved_at: u64, home: Home }
#[derive(Clone)]
pub struct HomeCache { root: PathBuf }
impl HomeCache {
    pub fn new(root: PathBuf) -> Self { Self { root } }
    fn path(&self, source: &str) -> PathBuf {
        self.root.join(format!("{}.json", haiku_audio::media_cache::file_id(source)))
    }
    fn read(&self, source: &str, now: u64) -> Result<Option<Home>> {
        let data: Option<Cached> = haiku_store::json_file::read(&self.path(source))?;
        Ok(data.filter(|c| c.version == 1 && now >= c.saved_at && now - c.saved_at < TTL).map(|c| bounded(c.home, None)))
    }
    fn write(&self, source: &str, now: u64, home: Home, old: Option<&Home>) -> Result<Home> {
        let home = bounded(home, old);
        haiku_store::json_file::write(&self.path(source), &Cached { version: 1, saved_at: now, home: home.clone() })?;
        Ok(home)
    }
    /// Cache lookup finishes before calling the loader. The caller starts a second refresh
    /// after displaying a cache hit, so slow/offline providers cannot block the first frame.
    async fn load<F, Fut>(&self, source: &str, refresh: bool, now: u64, fetch: F) -> Result<serde_json::Value>
    where F: FnOnce() -> Fut, Fut: std::future::Future<Output=Result<Home>> {
        let old = self.read(source, now)?;
        if !refresh && let Some(home) = &old { return Ok(presentation(home, true, None)); }
        // The disk cache never stores cursors; only a live first page can continue.
        let mut fetched = fetch().await?;
        let next = fetched.next.clone();
        retain_radio(&mut fetched, old.as_ref());
        fetched.shelves.retain(|s| !s.items.is_empty() || !s.entries.is_empty());
        self.write(source, now, fetched.clone(), None)?;
        Ok(presentation(&fetched, false, next.as_deref()))
    }
    pub async fn home(&self, host: Arc<Host>, source: String, refresh: bool) -> Result<serde_json::Value> {
        // Validate availability even when serving cached content.
        let api = SourceApi::new(host.source(&source)?);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        self.load(&source, refresh, now, || api.home(None, refresh, None)).await
    }
    /// Live pages keep every supported item. Only the disk snapshot is bounded.
    pub async fn page(
        &self, host: Arc<Host>, source: String, cursor: String,
        cancel: Option<&haiku_plugins::Cancellation>,
    ) -> Result<serde_json::Value> {
        let api = SourceApi::new(host.source(&source)?);
        let mut home = api.home(Some(&cursor), false, cancel).await?;
        let next = home.next.take();
        home.shelves.retain(|s| !s.items.is_empty() || !s.entries.is_empty());
        Ok(presentation(&home, false, next.as_deref()))
    }
}
fn trim(home: &mut Home) {
    home.shelves.retain(|s| !s.items.is_empty() || !s.entries.is_empty());
    if home.shelves.len() > 40 {
        let has_radio = |s: &haiku_plugins::Shelf| s.entries.iter().any(|e| e.to.reference.kind == "radio");
        let overflow = home.shelves.split_off(40);
        for shelf in overflow.into_iter().filter(has_radio) {
            if let Some(index) = home.shelves.iter().rposition(|s| !has_radio(s)) {
                home.shelves.remove(index);
                home.shelves.push(shelf);
            }
        }
    }
    for shelf in &mut home.shelves { shelf.items.truncate(60); shelf.entries.truncate(60); }
}
fn bounded(mut home: Home, old: Option<&Home>) -> Home {
    retain_radio(&mut home, old);
    trim(&mut home);
    home.next = None;
    home
}
fn retain_radio(home: &mut Home, old: Option<&Home>) {
    // Refreshes can omit radio entries. Retain just those shortcuts, without reviving
    // stale non-radio recommendations or replacing fresh entries with old copies.
    if let Some(old) = old {
        for shelf in &old.shelves {
            let entries: Vec<_> = shelf.entries.iter().filter(|e| e.to.reference.kind == "radio"
                && !home.shelves.iter().flat_map(|s| &s.entries).any(|fresh| fresh.to.reference == e.to.reference))
                .cloned().collect();
            if entries.is_empty() { continue; }
            if let Some(fresh) = home.shelves.iter_mut().find(|s| s.id == shelf.id && s.kind == "entries") {
                for entry in entries {
                    if !fresh.entries.iter().any(|e| e.to.reference == entry.to.reference) { fresh.entries.push(entry); }
                }
            } else {
                let mut retained = shelf.clone(); retained.items.clear(); retained.entries = entries;
                home.shelves.push(retained);
            }
        }
    }
}
fn presentation(home: &Home, cached: bool, next: Option<&str>) -> serde_json::Value {
    let mut value = serde_json::json!({"shelves":home.shelves,"cached":cached,"next":next});
    for shelf in value["shelves"].as_array_mut().unwrap() {
        if shelf["kind"] == "tracks" {
            for entity in shelf["items"].as_array_mut().unwrap() {
                entity["source"] = entity["reference"]["source"].clone();
                entity["id"] = entity["reference"]["id"].clone();
                entity["localPath"] = serde_json::Value::Null;
            }
        }
    }
    value
}
#[cfg(test)]
mod tests {
    use super::*;
    use haiku_plugins::{Entry, Link, Shelf};
    use haiku_model::EntityRef;
    fn home() -> Home {
        Home { shelves: vec![Shelf { id: "radio".into(), title: "FM".into(), kind: "entries".into(), items:vec![], entries:vec![Entry {title:"FM".into(),subtitle:None,cover:None,to:Link { reference:EntityRef {source:"s".into(),kind:"radio".into(),id:"r".into()} }}],more:None,layout:None }],next:None }
    }
    #[tokio::test]
    async fn home_cache_expiry_caps_radio_retention_and_cache_first() {
        let dir = tempfile::tempdir().unwrap();let cache=HomeCache::new(dir.path().into());
        cache.write("s", 10, home(), None).unwrap();
        let hit=cache.load("s",false,10+TTL-1,|| async { panic!("cache hit must not call a provider") }).await.unwrap();
        assert_eq!(hit["cached"],true);
        assert!(cache.read("s",10+TTL).unwrap().is_none());
        assert!(cache.read("other",10).unwrap().is_none());
        let old=home();let mut next=home();next.shelves[0].entries.clear();
        assert_eq!(bounded(next,Some(&old)).shelves[0].entries.len(),1);
        let mut renamed=home();renamed.shelves[0].id="radio:p1:s0".into();
        assert_eq!(bounded(renamed,Some(&old)).shelves.len(),1);
        let mut big=home(); big.shelves[0].entries=vec![big.shelves[0].entries[0].clone();70];big.shelves=vec![big.shelves[0].clone();45];
        let result=bounded(big,None);assert_eq!(result.shelves.len(),40);assert_eq!(result.shelves[0].entries.len(),60);
        let mut full=home();full.shelves[0].id="playlist".into();
        full.shelves[0].entries[0].to.reference.kind="playlist".into();
        full.shelves=vec![full.shelves[0].clone();45];
        assert!(bounded(full,Some(&old)).shelves.iter().flat_map(|s| &s.entries).any(|e| e.to.reference.kind=="radio"));
        let refreshed=cache.load("s",true,11,||async {Ok(Home{shelves:vec![],next:None})}).await.unwrap();
        assert_eq!(refreshed["shelves"][0]["entries"][0]["title"],"FM");
        assert_eq!(refreshed["cached"],false);
        let mut large=home();
        large.shelves[0].entries=vec![large.shelves[0].entries[0].clone();70];
        large.shelves=vec![large.shelves[0].clone();45];
        large.next=Some("continue".into());
        let live=cache.load("large",true,12,||async {Ok(large)}).await.unwrap();
        assert_eq!(live["shelves"].as_array().unwrap().len(),45);
        assert_eq!(live["shelves"][0]["entries"].as_array().unwrap().len(),70);
        assert_eq!(live["next"],"continue");
        let saved=cache.read("large",12).unwrap().unwrap();
        assert_eq!(saved.shelves.len(),40);
        assert_eq!(saved.shelves[0].entries.len(),60);
        assert!(saved.next.is_none());
    }
}
