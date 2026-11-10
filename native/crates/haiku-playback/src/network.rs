//! Controller-owned resolution: local files, complete same-quality cache, then ordered streams.
use crate::{AudioBackend, Clock, TrackResolver, backend::cache_key};
use futures::future::BoxFuture;
use haiku_audio::media_cache::{self, CacheStatus};
use haiku_model::{Error, ErrorCode, Track};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug, PartialEq)]
pub enum Candidate {
    Local(PathBuf),
    Network {
        url: String,
        headers: BTreeMap<String, String>,
        format: Option<String>,
        quality: Option<String>,
        expires_at: Option<u64>,
    },
}
impl Candidate {
    fn supported(&self) -> bool {
        match self {
            Self::Local(_) => true,
            Self::Network { format, .. } => format.as_deref().is_none_or(|f| {
                ["mp3", "flac", "m4a", "ogg"].contains(&f.to_ascii_lowercase().as_str())
            }),
        }
    }
    fn expired(&self, now: u64) -> bool {
        matches!(self, Self::Network { expires_at: Some(until), .. } if *until <= now)
    }
}

/// Shared between the current open and its preload; source addresses never cross into TSX.
#[derive(Clone, Default)]
pub(crate) struct Addresses(Arc<Mutex<BTreeMap<String, Vec<Candidate>>>>);
impl Addresses {
    pub fn expire(&self, track: &Track, quality: &str) {
        if let Some(key) = cache_key(track, quality) {
            self.0.lock().unwrap().remove(&key);
        }
    }
    pub fn open(
        &self,
        backend: Arc<dyn AudioBackend>,
        resolver: Arc<dyn TrackResolver>,
        clock: Arc<dyn Clock>,
        track: Track,
        quality: String,
        generation: u64,
        prepare: bool,
        position: f64,
    ) -> BoxFuture<'static, Result<(), Error>> {
        let addresses = self.clone();
        Box::pin(async move {
            // A local library file can never fall through to a plugin or the network.
            if track.local_path.is_some() {
                let path = resolver.resolve(track.clone()).await?;
                return backend
                    .open_candidate(
                        track,
                        Candidate::Local(path),
                        generation,
                        prepare,
                        position,
                        None,
                    )
                    .await;
            }
            let key = cache_key(&track, &quality).unwrap();
            if let CacheStatus::Complete { path, .. } = media_cache::status(&key) {
                match backend
                    .open_candidate(
                        track.clone(),
                        Candidate::Local(path),
                        generation,
                        prepare,
                        position,
                        Some(key.clone()),
                    )
                    .await
                {
                    Ok(()) => {
                        media_cache::touch(&key);
                        return Ok(());
                    }
                    Err(_) => media_cache::remove(&key),
                }
            }
            let mut last = Error::new(ErrorCode::UnsupportedFormat, "来源没有可播放的音频候选");
            // One bounded refresh for Expired/expired freshly fetched candidates.
            for refresh in 0..2 {
                let cached = addresses.0.lock().unwrap().get(&key).cloned();
                let candidates =
                    match cached.filter(|v| !v.iter().any(|c| c.expired(clock.unix_seconds()))) {
                        Some(v) => v,
                        None => match resolver.candidates(track.clone(), quality.clone()).await {
                            Ok(v) => {
                                addresses.0.lock().unwrap().insert(key.clone(), v.clone());
                                v
                            }
                            Err(e) if e.code == ErrorCode::PlaybackExpired && refresh == 0 => {
                                addresses.0.lock().unwrap().remove(&key);
                                continue;
                            }
                            Err(e) => return Err(e),
                        },
                    };
                let mut expired = false;
                for candidate in candidates {
                    if !candidate.supported() {
                        continue;
                    }
                    if candidate.expired(clock.unix_seconds()) {
                        expired = true;
                        continue;
                    }
                    match backend
                        .open_candidate(
                            track.clone(),
                            candidate,
                            generation,
                            prepare,
                            position,
                            Some(key.clone()),
                        )
                        .await
                    {
                        Ok(()) => return Ok(()),
                        Err(e) => {
                            expired |= e.code == ErrorCode::PlaybackExpired;
                            last = e;
                        }
                    }
                }
                if !expired {
                    break;
                }
                addresses.0.lock().unwrap().remove(&key);
            }
            Err(last)
        })
    }
}
