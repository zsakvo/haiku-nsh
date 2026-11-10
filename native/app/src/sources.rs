//! Typed plugin adapters; playback owns resolution/recovery and these adapters only fetch data.
use futures::future::BoxFuture;
use haiku_model::{Error, ErrorCode, Track};
use haiku_playback::{Candidate, TrackResolver};
use haiku_plugins::{Host, SourceApi};
use nsh_core::prelude::*;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};

pub struct SourceResolver(pub Arc<Host>);
impl TrackResolver for SourceResolver {
    fn candidates(
        &self,
        track: Track,
        quality: String,
    ) -> BoxFuture<'_, Result<Vec<Candidate>, Error>> {
        Box::pin(async move {
            let source = self.0.source(&track.source).map_err(playback_error)?;
            struct CancelOnDrop(haiku_plugins::Cancellation);
            impl Drop for CancelOnDrop {
                fn drop(&mut self) {
                    self.0.cancel();
                }
            }
            let cancel = CancelOnDrop(Default::default());
            let streams = SourceApi::new(source)
                .streams(&track.id, &quality, None, Some(&cancel.0))
                .await
                .map_err(playback_error)?;
            Ok(streams
                .into_iter()
                .map(|s| Candidate::Network {
                    url: s.url,
                    headers: s.headers,
                    format: s.format,
                    quality: s.quality,
                    expires_at: s.expires_at,
                })
                .collect())
        })
    }
}
pub struct SourceRadio { pub host: Arc<Host>, pub source: String, pub channel: Option<String> }
impl std::fmt::Debug for SourceRadio {
    fn fmt(&self, f:&mut std::fmt::Formatter<'_>)->std::fmt::Result { f.debug_struct("SourceRadio").field("source",&self.source).field("channel",&self.channel).finish() }
}
impl haiku_playback::RadioProvider for SourceRadio {
    fn next(&self, history: Vec<Track>, count:u32)->BoxFuture<'_,Result<Vec<Track>,Error>> {
        Box::pin(async move {
            struct Cancel(haiku_plugins::Cancellation);
            impl Drop for Cancel {fn drop(&mut self){self.0.cancel();}}
            let cancel=Cancel(Default::default());
            SourceApi::new(self.host.source(&self.source).map_err(playback_error)?)
                .radio_next(self.channel.as_deref(),&history,count,Some(&cancel.0)).await.map_err(playback_error)
        })
    }
}
fn playback_error(e: haiku_plugins::PluginError) -> Error {
    let mut error = Error::new(
        if e.code == "PLAYBACK_EXPIRED" {
            ErrorCode::PlaybackExpired
        } else if matches!(e.code.as_str(), "UPSTREAM_ERROR" | "TIMEOUT") {
            ErrorCode::PlaybackError
        } else {
            ErrorCode::CapabilityUnsupported
        },
        e.message,
    );
    error.retriable = matches!(
        e.code.as_str(),
        "UPSTREAM_ERROR" | "TIMEOUT" | "PLAYBACK_EXPIRED"
    );
    error
}

pub struct RemoteTracks(pub RefCell<Vec<Track>>);
impl crate::track_source::TrackDataSource for RemoteTracks {
    fn ids(&self) -> Vec<String> {
        self.0
            .borrow()
            .iter()
            .map(|t| t.entity_ref().encode())
            .collect()
    }
    fn track(&self, id: &str) -> Option<Track> {
        self.0
            .borrow()
            .iter()
            .find(|t| t.entity_ref().encode() == id)
            .cloned()
    }
}
pub struct RemoteSources {
    pub owner: Owner,
    pub tracks: RefCell<BTreeMap<u32, Rc<RemoteTracks>>>,
}
impl Default for RemoteSources {
    fn default() -> Self {
        Self {
            owner: Owner::detached(),
            tracks: RefCell::default(),
        }
    }
}
impl Drop for RemoteSources {
    fn drop(&mut self) {
        // Native class GC may run outside the reactive graph. Pages explicitly release
        // their handles; the engine-local registry itself is freed with its engine.
        self.owner.dispose();
    }
}

/// Bounded source artwork cache shared by native presentation and the media center.
pub async fn artwork(host: Arc<Host>, root: std::path::PathBuf, track: Track) -> Option<String> {
    use futures::StreamExt;
    let cover = track.cover?;
    if !cover.starts_with("http") {
        return Some(cover.strip_prefix("file://").unwrap_or(&cover).to_owned());
    }
    let path = root.join(haiku_audio::media_cache::file_id(
        &serde_json::json!([track.source, cover]).to_string(),
    ));
    if path.is_file() {
        return Some(path.to_string_lossy().into_owned());
    }
    let mut request = reqwest::Client::new()
        .get(&cover)
        .timeout(std::time::Duration::from_secs(10));
    if let Ok(source) = host.source(&track.source) {
        if let Some(headers) = source
            .descriptor()
            .http
            .and_then(|h| h["headers"].as_object().cloned())
        {
            for (key, value) in headers {
                if let Some(v) = value.as_str() {
                    request = request.header(key, v);
                }
            }
        }
    }
    let response = request.send().await.ok()?.error_for_status().ok()?;
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk.ok()?);
        if bytes.len() > 16 * 1024 * 1024 {
            return None;
        }
    }
    tokio::fs::create_dir_all(&root).await.ok()?;
    let temp = tempfile::NamedTempFile::new_in(&root).ok()?;
    tokio::fs::write(temp.path(), &bytes).await.ok()?;
    temp.persist(&path).ok()?;
    Some(path.to_string_lossy().into_owned())
}

/// One SDK page. Only source-owned cursors cross this boundary.
pub async fn search(
    host: Arc<Host>,
    source: String,
    kind: String,
    query: String,
    cursor: Option<String>,
) -> haiku_plugins::Result<serde_json::Value> {
    use haiku_plugins::{PageArgs, PluginError, SearchKind};
    use serde_json::json;
    let source = host.source(&source)?;
    if source.needs_login() {
        return Err(PluginError::new("AUTH_REQUIRED", "请先在设置中登录此来源"));
    }
    let api = SourceApi::new(source);
    let page = PageArgs { cursor, limit: 40 };
    let keyword = query.trim();
    if kind == "track" {
        return Ok(json!(api.search_tracks(keyword, page, None).await?));
    }
    let kind = match kind.as_str() {
        "album" => SearchKind::Album,
        "artist" => SearchKind::Artist,
        "playlist" => SearchKind::Playlist,
        "program" => SearchKind::Program,
        _ => {
            return Err(PluginError::new(
                "CAPABILITY_UNSUPPORTED",
                "不支持的搜索类型",
            ));
        }
    };
    Ok(json!(api.search(kind, keyword, page, None).await?))
}

async fn preview_with<F, Fut>(sources: Vec<(String, String)>, load: F) -> serde_json::Value
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = haiku_plugins::Result<serde_json::Value>>,
{
    use serde_json::json;
    let jobs = sources.into_iter().map(|(id,title)| {
        let work = load(id.clone());
        async move {
            match work.await {
                Ok(mut group) => { group["source"] = json!(id); group["title"] = json!(title); group },
                Err(e) => json!({"source":id,"title":title,"tracks":[],"playlists":[],"errors":[e.to_string()]}),
            }
        }
    });
    json!(futures::future::join_all(jobs).await)
}

pub async fn search_preview(host: Arc<Host>, query: String) -> serde_json::Value {
    use serde_json::json;
    if query.trim().is_empty() {
        return json!([]);
    }
    let sources = host
        .records()
        .into_iter()
        .filter(|r| r.enabled && host.package_enabled(&r.plugin_id))
        .filter_map(|r| {
            let source = host.source(&r.source_id).ok()?;
            source
                .descriptor()
                .capabilities
                .iter()
                .any(|c| c == "search.track" || c == "search.playlist")
                .then(|| {
                    (
                        r.source_id,
                        r.display_name.unwrap_or_else(|| source.descriptor().name),
                    )
                })
        })
        .collect();
    preview_with(sources, |id| {
        let host = host.clone();
        let query = query.clone();
        async move {
            let caps = host.source(&id)?.descriptor().capabilities;
            let fetch = |kind: &'static str| {
                let host = host.clone();
                let id = id.clone();
                let query = query.clone();
                let supported = caps.iter().any(|c| c == &format!("search.{kind}"));
                async move {
                    if supported {
                        search(host, id, kind.into(), query, None).await
                    } else {
                        Ok(json!({"items":[]}))
                    }
                }
            };
            let (tracks, playlists) = futures::join!(fetch("track"), fetch("playlist"));
            let mut errors = vec![];
            let mut items = |r: haiku_plugins::Result<serde_json::Value>| match r {
                Ok(p) => p["items"].clone(),
                Err(e) => {
                    errors.push(e.to_string());
                    json!([])
                }
            };
            let tracks = items(tracks);
            let playlists = items(playlists);
            Ok(json!({"tracks":tracks,"playlists":playlists,"errors":errors}))
        }
    })
    .await
}

#[cfg(test)]
mod search_tests {
    use super::*;
    #[tokio::test]
    async fn preview_runs_sources_concurrently_and_isolates_errors_in_source_order() {
        use serde_json::json;
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            preview_with(
                vec![
                    ("good".into(), "正常来源".into()),
                    ("bad".into(), "离线来源".into()),
                ],
                |id| {
                    let barrier = barrier.clone();
                    async move {
                        barrier.wait().await;
                        if id == "bad" {
                            Err(haiku_plugins::PluginError::new("UPSTREAM_ERROR", "offline"))
                        } else {
                            Ok(json!({"tracks":[1,2,3,4,5],"playlists":[],"errors":[]}))
                        }
                    }
                },
            ),
        )
        .await
        .expect("both sources must enter before either completes");
        assert_eq!(result[0]["source"], "good");
        assert_eq!(result[0]["tracks"].as_array().unwrap().len(), 5);
        assert_eq!(result[1]["source"], "bad");
        assert!(result[1]["errors"][0].as_str().unwrap().contains("offline"));
    }
}
