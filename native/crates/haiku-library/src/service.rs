//! Persistent local library. The index is disposable; sparse overrides are authoritative.
use crate::{local_scan, tags::{AudioTags, write_tag}};
use haiku_model::{Album, Artist, EntityRef, Track};
use haiku_store::json_file;
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeMap, BTreeSet}, path::{Path, PathBuf}, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}};
use tokio::sync::watch;

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    #[serde(default, skip_serializing_if = "Option::is_none")] pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub artists: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub album: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub cover: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub album_artist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub year: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub track_no: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub disc_no: Option<u32>,
}
impl Patch {
    fn validate(&self) -> Result<()> {
        if [&self.title, &self.album, &self.album_artist, &self.cover].into_iter().flatten().any(|s| s.trim().is_empty())
            || self.artists.as_ref().is_some_and(|a| a.is_empty() || a.iter().any(|a| a.trim().is_empty())) {
            return Err("字段不能为空；请用重置恢复文件标签".into());
        }
        Ok(())
    }
    fn apply(&self, t: &mut Track) {
        if let Some(v) = &self.title { t.title = v.clone(); }
        if let Some(v) = &self.artists { t.artists = v.iter().cloned().map(Artist::Name).collect(); }
        if let Some(v) = &self.album { t.album = Some(Album { id: None, title: v.clone() }); }
        if let Some(v) = &self.cover { t.cover = Some(v.clone()); }
        let extra = t.extra.get_or_insert_with(|| serde_json::json!({}));
        for (key, value) in [("album_artist", self.album_artist.as_ref().map(|v| serde_json::json!(v))), ("year", self.year.map(|v| serde_json::json!(v))), ("track_no", self.track_no.map(|v| serde_json::json!(v))), ("disc_no", self.disc_no.map(|v| serde_json::json!(v)))] {
            if let Some(v) = value { extra[key] = v; }
        }
    }
    fn merge_sparse(&mut self, patch: &Self, original: &Track) {
        macro_rules! field { ($name:ident, $value:expr) => { if let Some(v) = &patch.$name { self.$name = (Some(v.clone()) != $value).then(|| v.clone()); } }; }
        field!(title, Some(original.title.clone()));
        field!(artists, Some(artist_names(original)));
        field!(album, original.album.as_ref().map(|a| a.title.clone()));
        field!(cover, original.cover.clone());
        field!(album_artist, extra_string(original, "album_artist"));
        field!(year, extra_number(original, "year"));
        field!(track_no, extra_number(original, "track_no"));
        field!(disc_no, extra_number(original, "disc_no"));
    }
}

pub fn artist_names(t: &Track) -> Vec<String> { t.artists.iter().map(|a| match a { Artist::Name(n) | Artist::Entity { name:n, .. } => n.clone() }).collect() }
fn extra_string(t: &Track, key: &str) -> Option<String> { t.extra.as_ref()?.get(key)?.as_str().map(str::to_owned) }
fn extra_number(t: &Track, key: &str) -> Option<u32> { t.extra.as_ref()?.get(key)?.as_u64().and_then(|n| u32::try_from(n).ok()) }
fn from_scan(t: local_scan::ScannedTrack) -> Track {
    let mut track = Track::local(&t.path);
    track.title = t.title;
    track.artists = t.artists.into_iter().map(Artist::Name).collect();
    track.album = t.album.map(|title| Album { id: None, title });
    track.cover = t.cover_path;
    track.duration = t.duration_sec.map(f64::from);
    track.extra = Some(serde_json::json!({"album_artist":t.album_artist,"year":t.year,"track_no":t.track_no,"disc_no":t.disc_no,"size":t.size,"mtime":t.mtime}));
    track
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Progress { pub scanned: u32, pub total: u32, pub current: String }
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Report { pub count: usize, pub skipped: u32, pub errors: Vec<String>, pub cancelled: bool }
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot { pub revision: u64, pub scanning: bool, pub progress: Progress, pub report: Report }
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Query { pub search: String, pub sort: Sort, pub descending: bool, pub group: Option<String> }
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Sort { #[default] Title, Artist, Album, Track, Duration, Path }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Group { pub key: String, pub title: String, pub subtitle: String, pub cover: Option<String>, pub count: usize }

#[derive(Default)]
struct Data { roots: Vec<String>, index: BTreeMap<String, Track>, overrides: BTreeMap<String, Patch> }
struct Inner { dir: PathBuf, data: Mutex<Data>, cancelled: AtomicBool, scanning: AtomicBool, events: watch::Sender<Snapshot> }
#[derive(Clone)]
pub struct Library(Arc<Inner>);
impl Library {
    pub fn open(root: &Path) -> Result<Self> {
        let dir = root.join("library");
        let roots = json_file::read(&dir.join("roots.json")).map_err(|e| e.to_string())?.unwrap_or_default();
        let overrides = json_file::read(&dir.join("overrides.json")).map_err(|e| e.to_string())?.unwrap_or_default();
        // Corrupt index may be rebuilt. Never apply this fallback to authoritative files.
        let index = json_file::read(&dir.join("index.json")).unwrap_or(None).unwrap_or_default();
        let (events, _) = watch::channel(Snapshot::default());
        Ok(Self(Arc::new(Inner { dir, data: Mutex::new(Data { roots, index, overrides }), cancelled: AtomicBool::new(false), scanning: AtomicBool::new(false), events })))
    }
    pub fn snapshot(&self) -> Snapshot { self.0.events.borrow().clone() }
    pub fn subscribe(&self) -> watch::Receiver<Snapshot> { self.0.events.subscribe() }
    fn changed(&self) { self.0.events.send_modify(|s| s.revision += 1); }
    pub fn roots(&self) -> Vec<String> { self.0.data.lock().unwrap().roots.clone() }
    pub fn add_roots(&self, roots: Vec<String>) -> Result<()> {
        let mut data = self.0.data.lock().unwrap();
        let mut next: BTreeSet<_> = data.roots.iter().cloned().collect();
        for root in roots { let path = std::fs::canonicalize(&root).map_err(|e| format!("{root}：{e}"))?; if !path.is_dir() { return Err("请选择文件夹".into()); } next.insert(path.to_string_lossy().into_owned()); }
        let next: Vec<_> = next.into_iter().collect();
        json_file::write(&self.0.dir.join("roots.json"), &next).map_err(|e| e.to_string())?;
        data.roots = next; drop(data); self.changed(); Ok(())
    }
    pub fn remove_root(&self, root: &str) -> Result<()> {
        let mut data = self.0.data.lock().unwrap();
        let next: Vec<_> = data.roots.iter().filter(|r| *r != root).cloned().collect();
        json_file::write(&self.0.dir.join("roots.json"), &next).map_err(|e| e.to_string())?;
        data.roots = next; drop(data); self.changed(); Ok(())
    }
    pub fn cancel(&self) { self.0.cancelled.store(true, Ordering::Release); }
    pub fn scan(&self) -> Result<Report> {
        if self.0.scanning.swap(true, Ordering::AcqRel) { return Err("扫描已在进行".into()); }
        self.0.cancelled.store(false, Ordering::Release);
        self.0.events.send_modify(|s| { s.scanning = true; s.progress = Progress::default(); });
        let result = self.scan_inner();
        self.0.scanning.store(false, Ordering::Release);
        self.0.events.send_modify(|s| { s.scanning = false; if let Ok(r) = &result { s.report = r.clone(); } else if let Err(e) = &result { s.report.errors = vec![e.clone()]; } s.revision += 1; });
        result
    }
    fn scan_inner(&self) -> Result<Report> {
        let outcome = local_scan::scan_cancellable(&self.roots(), &self.0.dir.join("covers"), || self.0.cancelled.load(Ordering::Acquire), |scanned,total,current| {
            if scanned % 64 == 0 || scanned == total { self.0.events.send_modify(|s| s.progress = Progress { scanned, total, current: current.into() }); }
        });
        let report = Report { count: outcome.tracks.len(), skipped: outcome.skipped, errors: outcome.errors, cancelled: outcome.cancelled };
        // Cancelling never replaces the previous usable index with a partial scan.
        if !report.cancelled {
            let mut data = self.0.data.lock().unwrap();
            let index: BTreeMap<_,_> = outcome.tracks.into_iter().map(from_scan).map(|t| (t.id.clone(),t)).collect();
            json_file::write(&self.0.dir.join("index.json"), &index).map_err(|e| e.to_string())?;
            data.index = index;
        }
        Ok(report)
    }
    pub fn track(&self, id: &str) -> Option<Track> { let data = self.0.data.lock().unwrap(); Self::effective(&data,id) }
    fn effective(data: &Data, id: &str) -> Option<Track> { let mut t = data.index.get(id)?.clone(); if let Some(p) = data.overrides.get(id) { p.apply(&mut t); } Some(t) }
    pub fn edit(&self, ids: &[String], patch: Patch) -> Result<()> {
        patch.validate()?;
        let mut data = self.0.data.lock().unwrap(); let mut next = data.overrides.clone();
        for id in ids {
            let original = data.index.get(id).ok_or_else(|| format!("未找到本地曲目：{id}"))?;
            let p = next.entry(id.clone()).or_default(); p.merge_sparse(&patch, original);
            if *p == Patch::default() { next.remove(id); }
        }
        json_file::write(&self.0.dir.join("overrides.json"), &next).map_err(|e| e.to_string())?;
        data.overrides = next; drop(data); self.changed(); Ok(())
    }
    pub fn reset(&self, ids: &[String]) -> Result<()> {
        let mut data = self.0.data.lock().unwrap(); let mut next = data.overrides.clone();
        for id in ids { next.remove(id); }
        json_file::write(&self.0.dir.join("overrides.json"), &next).map_err(|e| e.to_string())?;
        data.overrides = next; drop(data); self.changed(); Ok(())
    }
    pub fn write_tags(&self, ids: &[String]) -> Vec<String> {
        let mut errors = Vec::new();
        // Serialize with edits/scans so a late write cannot erase a newer override.
        let mut data = self.0.data.lock().unwrap();
        for id in ids {
            let Some(t) = Self::effective(&data,id) else { errors.push(format!("未找到曲目：{id}")); continue; };
            let tags = AudioTags { title:t.title.clone(), artists:artist_names(&t), album:t.album.as_ref().map(|a| a.title.clone()), album_artist:extra_string(&t,"album_artist"), year:extra_number(&t,"year"), track_no:extra_number(&t,"track_no"), disc_no:extra_number(&t,"disc_no"), cover_path:t.cover.clone(), lyrics:None };
            if let Err(e) = write_tag(Path::new(id), &tags) { errors.push(format!("{id}：{e}")); }
        }
        // Overrides stay authoritative even when the container silently rejects a field.
        drop(data); self.changed(); errors
    }
    pub fn import_cover(&self, path: &Path) -> Result<String> {
        if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 20*1024*1024 { return Err("封面不能超过 20 MB".into()); }
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let ext = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") { "png" } else if bytes.starts_with(b"\xff\xd8\xff") { "jpg" } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") { "webp" } else { return Err("请选择 PNG、JPEG 或 WebP 图片".into()); };
        let hash = bytes.iter().fold(0xcbf29ce484222325u64,|h,b| (h ^ u64::from(*b)).wrapping_mul(0x100000001b3));
        let dir = self.0.dir.join("metadata-covers"); std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let out = dir.join(format!("{hash:016x}.{ext}")); std::fs::write(&out,bytes).map_err(|e| e.to_string())?; Ok(out.to_string_lossy().into_owned())
    }
    pub fn query(&self, query: &Query) -> Vec<String> {
        let data = self.0.data.lock().unwrap(); let q = query.search.trim().to_lowercase();
        let mut tracks: Vec<_> = data.index.keys().filter_map(|id| Self::effective(&data,id)).filter(|t| {
            let group_matches = query.group.as_ref().is_none_or(|key| group_ref(t,"albums") == *key || group_ref(t,"artists") == *key);
            group_matches && (q.is_empty() || format!("{} {} {} {}",t.title,artist_names(t).join(" "),t.album.as_ref().map_or("",|a| a.title.as_str()),t.id).to_lowercase().contains(&q))
        }).collect();
        tracks.sort_by(|a,b| {
            let cmp = match query.sort {
                Sort::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
                Sort::Artist => artist_names(a).join(" / ").to_lowercase().cmp(&artist_names(b).join(" / ").to_lowercase()),
                Sort::Album => a.album.as_ref().map(|a| a.title.to_lowercase()).cmp(&b.album.as_ref().map(|a| a.title.to_lowercase())),
                Sort::Path => a.id.cmp(&b.id),
                Sort::Duration => a.duration.unwrap_or(0.).total_cmp(&b.duration.unwrap_or(0.)),
                Sort::Track => (extra_number(a,"disc_no").unwrap_or(0),extra_number(a,"track_no").unwrap_or(0)).cmp(&(extra_number(b,"disc_no").unwrap_or(0),extra_number(b,"track_no").unwrap_or(0))).then_with(|| a.title.cmp(&b.title)),
            }.then_with(|| a.id.cmp(&b.id));
            if query.descending { cmp.reverse() } else { cmp }
        });
        tracks.into_iter().map(|t| t.id).collect()
    }
    pub fn groups(&self, kind: &str, query: &Query, offset: usize, limit: usize) -> (usize,Vec<Group>) {
        let data = self.0.data.lock().unwrap(); let mut groups = BTreeMap::<String,(Group,BTreeSet<String>)>::new();
        for t in data.index.keys().filter_map(|id| Self::effective(&data,id)) {
            let artist = artist_names(&t).first().cloned().unwrap_or_else(|| "未知艺术家".into());
            let album_artist = extra_string(&t,"album_artist").filter(|a| !a.is_empty()).unwrap_or_else(|| artist.clone());
            let key = group_ref(&t,kind);
            let (g,albums) = groups.entry(key.clone()).or_insert_with(|| (Group { key, title:if kind == "artists" { artist } else { t.album.as_ref().map_or("未知专辑".into(),|a| a.title.clone()) }, subtitle:if kind == "artists" { String::new() } else { album_artist }, cover:t.cover.clone(),count:0 },BTreeSet::new()));
            g.count += 1; if g.cover.is_none() { g.cover = t.cover.clone(); }
            albums.insert(t.album.as_ref().map(|a| a.title.clone()).unwrap_or_else(|| Path::new(&t.id).parent().unwrap_or(Path::new("")).to_string_lossy().into_owned()));
        }
        let search = query.search.trim().to_lowercase();
        let mut groups: Vec<_> = groups.into_values().map(|(mut g,albums)| { if kind == "artists" { g.subtitle = format!("{} 张专辑",albums.len()); } g }).filter(|g| format!("{} {}",g.title,g.subtitle).to_lowercase().contains(&search)).collect();
        groups.sort_by(|a,b| a.title.to_lowercase().cmp(&b.title.to_lowercase()).then_with(|| a.key.cmp(&b.key)));
        if query.descending { groups.reverse(); }
        (groups.len(),groups.into_iter().skip(offset).take(limit.min(100)).collect())
    }
}
fn group_ref(t: &Track, kind: &str) -> String {
    let artist = artist_names(t).first().cloned().unwrap_or_else(|| "未知艺术家".into());
    let key = if kind == "artists" { artist } else if let Some(album) = &t.album { format!("album\x1f{}\x1f{}",extra_string(t,"album_artist").filter(|a| !a.is_empty()).unwrap_or(artist),album.title) } else { format!("dir\x1f{}",Path::new(&t.id).parent().unwrap_or(Path::new("")).to_string_lossy()) };
    EntityRef { source:"local".into(),kind:if kind == "artists" { "artist" } else { "album" }.into(),id:key.to_lowercase() }.encode()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(root: &Path, name: &str, title: &str, album: &str, artist: &str, album_artist: &str, no: u32) -> String {
        std::fs::create_dir_all(root).unwrap();
        let path = root.join(name);
        std::fs::write(&path,crate::test_support::minimal_flac()).unwrap();
        write_tag(&path,&AudioTags { title:title.into(),artists:vec![artist.into()],album:Some(album.into()),album_artist:Some(album_artist.into()),track_no:Some(no),..Default::default() }).unwrap();
        std::fs::canonicalize(path).unwrap().to_string_lossy().into_owned()
    }
    #[test]
    fn roots_persist_and_overlapping_roots_deduplicate() {
        let temp = tempfile::tempdir().unwrap(); let root = temp.path().join("music");
        let a = fixture(&root.join("album"),"one.flac","One","Shared","Singer","Various",1);
        let lib = Library::open(&temp.path().join("data")).unwrap();
        lib.add_roots(vec![root.to_string_lossy().into_owned(),root.join("album").to_string_lossy().into_owned()]).unwrap();
        let reopened = Library::open(&temp.path().join("data")).unwrap(); assert_eq!(reopened.roots().len(),2);
        assert_eq!(reopened.scan().unwrap().count,1);
        assert_eq!(reopened.query(&Query::default()),vec![a.clone()]);
        assert_eq!(reopened.track(&a).unwrap().source,"local");
        reopened.remove_root(&reopened.roots()[0]).unwrap();
        assert_eq!(Library::open(&temp.path().join("data")).unwrap().roots().len(),1);
    }
    #[test]
    fn rescan_and_missing_files_keep_authoritative_sparse_overrides() {
        let temp = tempfile::tempdir().unwrap(); let root = temp.path().join("music");
        let a = fixture(&root,"one.flac","Original","Album","Singer","Singer",1);
        let data = temp.path().join("data"); let lib = Library::open(&data).unwrap();
        lib.add_roots(vec![root.to_string_lossy().into_owned()]).unwrap(); lib.scan().unwrap();
        lib.edit(std::slice::from_ref(&a),Patch { title:Some("Edited".into()),..Default::default() }).unwrap();
        lib.scan().unwrap(); assert_eq!(lib.track(&a).unwrap().title,"Edited");
        std::fs::rename(&root, temp.path().join("unmounted")).unwrap(); lib.scan().unwrap();
        assert!(lib.track(&a).is_none());
        let overrides: BTreeMap<String,Patch> = json_file::read(&data.join("library/overrides.json")).unwrap().unwrap();
        assert_eq!(overrides[&a].title.as_deref(),Some("Edited"));
        std::fs::rename(temp.path().join("unmounted"),&root).unwrap();
        let lib = Library::open(&data).unwrap(); lib.scan().unwrap(); assert_eq!(lib.track(&a).unwrap().title,"Edited");
        lib.reset(std::slice::from_ref(&a)).unwrap(); assert_eq!(lib.track(&a).unwrap().title,"Original");
    }
    #[test]
    fn batch_only_changes_submitted_fields_and_queries_sort_effective_metadata() {
        let temp = tempfile::tempdir().unwrap(); let root = temp.path().join("music");
        let a = fixture(&root,"a.flac","Zed","Shared","Singer A","Various",2);
        let b = fixture(&root,"b.flac","Alpha","Shared","Singer B","Various",1);
        let data = temp.path().join("data"); let lib = Library::open(&data).unwrap();
        lib.add_roots(vec![root.to_string_lossy().into_owned()]).unwrap(); lib.scan().unwrap();
        lib.edit(std::slice::from_ref(&a),Patch { year:Some(2020),..Default::default() }).unwrap();
        lib.edit(&[a.clone(),b.clone()],Patch { album:Some("Edited album".into()),..Default::default() }).unwrap();
        assert_eq!(lib.track(&a).unwrap().title,"Zed"); assert_eq!(lib.track(&b).unwrap().title,"Alpha");
        assert_eq!(extra_number(&lib.track(&a).unwrap(),"year"),Some(2020)); assert_eq!(extra_number(&lib.track(&b).unwrap(),"year"),None);
        let overrides: serde_json::Value = json_file::read(&data.join("library/overrides.json")).unwrap().unwrap();
        assert_eq!(overrides[&b],serde_json::json!({"album":"Edited album"}));
        assert_eq!(lib.query(&Query::default()),vec![b.clone(),a.clone()]);
        assert_eq!(lib.query(&Query { sort:Sort::Title, descending:true,..Default::default() }),vec![a.clone(),b.clone()]);
        assert_eq!(lib.query(&Query { search:"singer b".into(),..Default::default() }),vec![b.clone()]);
        assert_eq!(lib.query(&Query { search:"edited album".into(),sort:Sort::Track,..Default::default() }),vec![b.clone(),a.clone()]);
        let (n,groups) = lib.groups("albums",&Query::default(),0,100); assert_eq!(n,1); assert_eq!(groups[0].count,2); assert_eq!(groups[0].subtitle,"Various");
        assert_eq!(lib.groups("artists",&Query::default(),0,100).0,2);
        assert_eq!(lib.query(&Query { group:Some(groups[0].key.clone()),..Default::default() }).len(),2);
        let cancel = AtomicBool::new(false);
        let report = local_scan::scan_cancellable(&lib.roots(), &data.join("covers"), || cancel.load(Ordering::Acquire), |_,_,_| { cancel.store(true,Ordering::Release); });
        assert!(report.cancelled); assert_eq!(report.tracks.len(),1);
        // Failure on one file is reported, and never removes either override.
        std::fs::remove_file(&b).unwrap(); let errors = lib.write_tags(&[a.clone(),b.clone()]); assert_eq!(errors.len(),1);
        assert_eq!(lib.track(&b).unwrap().album.as_ref().unwrap().title,"Edited album");
    }
}
