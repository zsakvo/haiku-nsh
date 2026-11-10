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
