use crate::{Cancellation, PluginError, Result, Source};
use haiku_model::{Album, Artist, EntityRef, Track};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PageArgs {
    pub cursor: Option<String>,
    pub limit: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<String>,
    pub total: Option<u64>,
}
#[derive(Clone, Copy, Debug)]
pub enum SearchKind {
    Track,
    Album,
    Artist,
    Playlist,
    Program,
}
impl SearchKind {
    fn name(self) -> &'static str {
        match self {
            Self::Track => "track",
            Self::Album => "album",
            Self::Artist => "artist",
            Self::Playlist => "playlist",
            Self::Program => "program",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entity {
    pub reference: EntityRef,
    pub title: String,
    pub cover: Option<String>,
    pub artists: Vec<Artist>,
    pub album: Option<Album>,
    pub duration: Option<f64>,
    pub description: Option<String>,
    pub owned: Option<bool>,
    pub year: Option<u32>,
    pub track_count: Option<u32>,
    pub extra: Option<Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Detail {
    pub info: Option<Entity>,
    pub tracks: Page<Track>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stream {
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    pub format: Option<String>,
    pub quality: Option<String>,
    pub bitrate: Option<u64>,
    pub size: Option<u64>,
    pub expires_at: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lyrics {
    pub format: String,
    pub text: String,
    pub translation: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibraryCard {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub entity: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub title: String,
    pub cover: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CategoryGroup {
    pub title: String,
    pub items: Vec<Category>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapeCandidate {
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub year: Option<u32>,
    pub cover: Option<String>,
    pub duration: Option<f64>,
    pub track: Option<u32>,
    pub extra: Option<Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScrapeQuery {
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub duration: Option<f64>,
}
#[derive(Clone, Debug)]
pub struct Shelf {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub items: Vec<Entity>,
}
#[derive(Clone, Debug)]
pub struct Home {
    pub shelves: Vec<Shelf>,
    pub next: Option<String>,
}

/// Only this typed boundary should be consumed by content/playback services.
#[derive(Clone)]
pub struct SourceApi {
    source: Arc<Source>,
    warnings: Arc<Mutex<Vec<String>>>,
    cover_size: u32,
}
impl SourceApi {
    pub fn new(source: Arc<Source>) -> Self {
        Self {
            source,
            warnings: Arc::default(),
            cover_size: 512,
        }
    }
    pub fn with_cover_size(mut self, size: u32) -> Self {
        self.cover_size = size;
        self
    }
    pub fn take_warnings(&self) -> Vec<String> {
        std::mem::take(&mut *self.warnings.lock().unwrap())
    }
    fn warn(&self, message: String) {
        log::warn!("{}: {}", self.source.record.source_id, message);
        self.warnings.lock().unwrap().push(message);
    }
    fn cover(&self, image: Option<&str>) -> Option<String> {
        image.map(|s| s.replace("{size}", &self.cover_size.to_string()))
    }
    fn entity(&self, kind: &str, value: Value) -> Option<Entity> {
        let id = value["id"].as_str();
        let title = value[if kind == "artist" { "name" } else { "title" }].as_str();
        let (Some(id), Some(title)) = (id, title) else {
            self.warn(format!("Dropped {kind}: missing id/title"));
            return None;
        };
        Some(Entity {
            reference: EntityRef {
                source: self.source.record.source_id.clone(),
                kind: kind.into(),
                id: id.into(),
            },
            title: title.into(),
            cover: self.cover(value["cover"].as_str()),
            artists: serde_json::from_value(value["artists"].clone()).unwrap_or_default(),
            album: serde_json::from_value(value["album"].clone()).ok(),
            duration: value["duration"].as_f64(),
            description: value["description"].as_str().map(str::to_owned),
            owned: value["owned"].as_bool(),
            year: value["year"].as_u64().and_then(|n| n.try_into().ok()),
            track_count: value["trackCount"].as_u64().and_then(|n| n.try_into().ok()),
            extra: value.get("extra").cloned(),
        })
    }
    fn track_value(&self, mut value: Value) -> Option<Track> {
        if let Some(object) = value.as_object_mut() {
            object.insert("source".into(), json!(self.source.record.source_id));
        }
        match serde_json::from_value::<Track>(value) {
            Ok(mut track) => {
                track.cover = self.cover(track.cover.as_deref());
                Some(track)
            }
            Err(_) => {
                self.warn("Dropped track: missing required fields".into());
                None
            }
        }
    }
    fn page(&self, kind: &str, value: Value) -> Result<Page<Entity>> {
        let items = value["items"]
            .as_array()
            .ok_or_else(|| PluginError::new("PARSE_ERROR", "Page.items must be an array"))?
            .iter()
            .cloned()
            .filter_map(|v| self.entity(kind, v))
            .collect();
        Ok(Page {
            items,
            next: value["next"].as_str().map(str::to_owned),
            total: value["total"].as_u64(),
        })
    }
    fn tracks(&self, value: Value) -> Result<Page<Track>> {
        let items = value["items"]
            .as_array()
            .ok_or_else(|| PluginError::new("PARSE_ERROR", "Page.items must be an array"))?
            .iter()
            .cloned()
            .filter_map(|v| self.track_value(v))
            .collect();
        Ok(Page {
            items,
            next: value["next"].as_str().map(str::to_owned),
            total: value["total"].as_u64(),
        })
    }
    async fn invoke(
        &self,
        path: &str,
        args: Vec<Value>,
        extra: Value,
        cancel: Option<&Cancellation>,
    ) -> Result<Value> {
        self.source.invoke(path, args, extra, cancel).await
    }
    pub async fn search(
        &self,
        kind: SearchKind,
        keyword: &str,
        page: PageArgs,
        cancel: Option<&Cancellation>,
    ) -> Result<Page<Entity>> {
        let value = self
            .invoke(
                &format!("search.{}", kind.name()),
                vec![json!({"keyword":keyword,"cursor":page.cursor,"limit":page.limit})],
                json!({}),
                cancel,
            )
            .await?;
        self.page(kind.name(), value)
    }
    pub async fn track(&self, id: &str, cancel: Option<&Cancellation>) -> Result<Track> {
        let value = self
            .invoke("track.get", vec![json!(id)], json!({}), cancel)
            .await?;
        self.track_value(value)
            .ok_or_else(|| PluginError::new("PARSE_ERROR", "Invalid track"))
    }
    pub async fn streams(
        &self,
        id: &str,
        quality: &str,
        format: Option<&str>,
        cancel: Option<&Cancellation>,
    ) -> Result<Vec<Stream>> {
        let args = vec![json!(id)];
        let extra = json!({"quality":quality,"format":format});
        let first = self
            .invoke("track.streams", args.clone(), extra.clone(), cancel)
            .await;
        let value = if first.as_ref().is_err_and(|e| e.code == "PLAYBACK_EXPIRED") {
            self.invoke("track.streams", args, extra, cancel).await?
        } else {
            first?
        };
        let mut streams: Vec<Stream> = serde_json::from_value(value)?;
        let defaults = self
            .source
            .descriptor()
            .http
            .and_then(|v| v.get("headers").cloned())
            .and_then(|v| serde_json::from_value::<BTreeMap<String, String>>(v).ok())
            .unwrap_or_default();
        for stream in &mut streams {
            let mut headers = defaults.clone();
            headers.append(&mut stream.headers);
            stream.headers = headers;
        }
        Ok(streams)
    }
    pub async fn lyrics(&self, id: &str, cancel: Option<&Cancellation>) -> Result<Option<Lyrics>> {
        Ok(serde_json::from_value(
            self.invoke("track.lyrics", vec![json!(id)], json!({}), cancel)
                .await?,
        )?)
    }
    pub async fn detail(
        &self,
        kind: &str,
        id: &str,
        page: PageArgs,
        cancel: Option<&Cancellation>,
    ) -> Result<Detail> {
        if !["album", "artist", "playlist", "toplist", "program"].contains(&kind) {
            return Err(PluginError::new(
                "INVALID_ARGUMENT",
                "Unsupported detail kind",
            ));
        }
        let value = self
            .invoke(
                &format!("{kind}.get"),
                vec![json!(id)],
                serde_json::to_value(page)?,
                cancel,
            )
            .await?;
        let info = value
            .get("info")
            .cloned()
            .and_then(|v| self.entity(kind, v));
        let tracks = if value["tracks"].is_null() {
            Page {
                items: vec![],
                next: None,
                total: None,
            }
        } else {
            self.tracks(value["tracks"].clone())?
        };
        Ok(Detail { info, tracks })
    }
    pub async fn artist_albums(
        &self,
        id: &str,
        page: PageArgs,
        cancel: Option<&Cancellation>,
    ) -> Result<Page<Entity>> {
        let value = self
            .invoke(
                "artist.albums",
                vec![json!(id)],
                serde_json::to_value(page)?,
                cancel,
            )
            .await?;
        self.page("album", value)
    }
    pub async fn artist_tracks(
        &self,
        id: &str,
        page: PageArgs,
        cancel: Option<&Cancellation>,
    ) -> Result<Page<Track>> {
        let value = self
            .invoke(
                "artist.tracks",
                vec![json!(id)],
                serde_json::to_value(page)?,
                cancel,
            )
            .await?;
        self.tracks(value)
    }
    pub fn library_cards(&self) -> Result<Vec<LibraryCard>> {
        Ok(serde_json::from_value(json!(
            self.source.descriptor().library_cards
        ))?)
    }
    pub async fn browse(
        &self,
        card: &str,
        kind: &str,
        page: PageArgs,
        sort: Option<&str>,
        cancel: Option<&Cancellation>,
    ) -> Result<Page<Entity>> {
        let value = self
            .invoke(
                "library.browse",
                vec![json!(card)],
                json!({"cursor":page.cursor,"limit":page.limit,"sort":sort}),
                cancel,
            )
            .await?;
        self.page(kind, value)
    }
    pub async fn toplists(&self, cancel: Option<&Cancellation>) -> Result<Vec<Entity>> {
        let value = self
            .invoke("toplist.list", vec![], json!({}), cancel)
            .await?;
        Ok(value
            .as_array()
            .ok_or_else(|| PluginError::new("PARSE_ERROR", "Expected list"))?
            .iter()
            .cloned()
            .filter_map(|v| self.entity("toplist", v))
            .collect())
    }
    pub async fn categories(&self, cancel: Option<&Cancellation>) -> Result<Vec<CategoryGroup>> {
        let value = self
            .invoke("categories.list", vec![], json!({}), cancel)
            .await?;
        Ok(serde_json::from_value(value["groups"].clone())?)
    }
    pub async fn category_playlists(
        &self,
        id: &str,
        page: PageArgs,
        sort: Option<&str>,
        cancel: Option<&Cancellation>,
    ) -> Result<Page<Entity>> {
        let value = self
            .invoke(
                "categories.playlists",
                vec![json!(id)],
                json!({"cursor":page.cursor,"limit":page.limit,"sort":sort}),
                cancel,
            )
            .await?;
        self.page("playlist", value)
    }
    pub async fn me_playlists(
        &self,
        page: PageArgs,
        cancel: Option<&Cancellation>,
    ) -> Result<Page<Entity>> {
        let value = self
            .invoke("me.playlists", vec![], serde_json::to_value(page)?, cancel)
            .await?;
        self.page("playlist", value)
    }
    pub async fn liked_tracks(
        &self,
        page: PageArgs,
        cancel: Option<&Cancellation>,
    ) -> Result<Page<Track>> {
        let value = self
            .invoke(
                "me.likedTracks",
                vec![],
                serde_json::to_value(page)?,
                cancel,
            )
            .await?;
        self.tracks(value)
    }
    pub async fn radio_channels(&self, cancel: Option<&Cancellation>) -> Result<Vec<Category>> {
        Ok(serde_json::from_value(
            self.invoke("radio.channels", vec![], json!({}), cancel)
                .await?,
        )?)
    }
    pub async fn radio_next(
        &self,
        id: &str,
        history: &[Track],
        count: u32,
        cancel: Option<&Cancellation>,
    ) -> Result<Vec<Track>> {
        let value = self
            .invoke(
                "radio.next",
                vec![json!(id)],
                json!({"history":history,"count":count}),
                cancel,
            )
            .await?;
        Ok(value
            .as_array()
            .ok_or_else(|| PluginError::new("PARSE_ERROR", "Expected tracks"))?
            .iter()
            .cloned()
            .filter_map(|v| self.track_value(v))
            .collect())
    }
    pub async fn home(
        &self,
        cursor: Option<&str>,
        refresh: bool,
        cancel: Option<&Cancellation>,
    ) -> Result<Home> {
        let value = self
            .invoke(
                "home",
                vec![],
                json!({"cursor":cursor,"refresh":refresh}),
                cancel,
            )
            .await?;
        let mut shelves = vec![];
        for shelf in value["shelves"]
            .as_array()
            .ok_or_else(|| PluginError::new("PARSE_ERROR", "Expected shelves"))?
        {
            let kind = shelf["type"].as_str().unwrap_or("tracks");
            let items = shelf["items"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .cloned()
                        .filter_map(|v| self.entity(kind.trim_end_matches('s'), v))
                        .collect()
                })
                .unwrap_or_default();
            shelves.push(Shelf {
                id: shelf["id"].as_str().unwrap_or("").into(),
                title: shelf["title"].as_str().unwrap_or("").into(),
                kind: kind.into(),
                items,
            });
        }
        Ok(Home {
            shelves,
            next: value["next"].as_str().map(str::to_owned),
        })
    }
    pub async fn scrape(
        &self,
        query: ScrapeQuery,
        cancel: Option<&Cancellation>,
    ) -> Result<Vec<ScrapeCandidate>> {
        Ok(serde_json::from_value(
            self.invoke(
                "scrape.search",
                vec![serde_json::to_value(query)?],
                json!({}),
                cancel,
            )
            .await?,
        )?)
    }
    pub async fn scrape_lyrics(
        &self,
        candidate: ScrapeCandidate,
        cancel: Option<&Cancellation>,
    ) -> Result<Option<Lyrics>> {
        Ok(serde_json::from_value(
            self.invoke(
                "scrape.lyrics",
                vec![serde_json::to_value(candidate)?],
                json!({}),
                cancel,
            )
            .await?,
        )?)
    }
}
