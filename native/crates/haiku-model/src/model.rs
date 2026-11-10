use serde::{Deserialize, Serialize};

/// Host identity; JSON tuples avoid delimiter and URL escaping collisions.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityRef {
    pub source: String,
    pub kind: String,
    pub id: String,
}

impl EntityRef {
    pub fn encode(&self) -> String {
        serde_json::to_string(&(&self.source, &self.kind, &self.id)).expect("strings serialize")
    }
    pub fn decode(encoded: &str) -> Result<Self, Error> {
        let (source, kind, id) = serde_json::from_str(encoded)
            .map_err(|e| Error::new(ErrorCode::CorruptData, format!("Invalid entity: {e}")))?;
        Ok(Self { source, kind, id })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Artist {
    Name(String),
    Entity {
        id: Option<String>,
        name: String,
        cover: Option<String>,
        description: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Album {
    pub id: Option<String>,
    pub title: String,
}

/// SDK §3.2 track plus the host-owned source and optional complete local file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub source: String,
    pub id: String,
    pub title: String,
    pub artists: Vec<Artist>,
    pub album: Option<Album>,
    pub cover: Option<String>,
    pub duration: Option<f64>,
    pub quality: Option<Vec<String>>,
    pub explicit: Option<bool>,
    pub extra: Option<serde_json::Value>,
    pub local_path: Option<String>,
}

impl Track {
    pub fn local(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            source: "local".into(),
            id: path.clone(),
            title: std::path::Path::new(&path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            artists: vec![],
            album: None,
            cover: None,
            duration: None,
            quality: None,
            explicit: None,
            extra: None,
            local_path: Some(path),
        }
    }
    pub fn entity_ref(&self) -> EntityRef {
        EntityRef {
            source: self.source.clone(),
            kind: "track".into(),
            id: self.id.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    Io,
    CorruptData,
    UnsupportedFormat,
    InvalidArgument,
    Cancelled,
    PlaybackError,
    PlaybackExpired,
    CapabilityUnsupported,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
    pub source_id: Option<String>,
    pub retriable: bool,
    pub detail: Option<serde_json::Value>,
}
impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            source_id: None,
            retriable: false,
            detail: None,
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entity_encoding_is_lossless_and_unambiguous() {
        for source in ["library#user", "x\u{1f}y", "来源/一"] {
            let entity = EntityRef {
                source: source.into(),
                kind: "track".into(),
                id: "https://a/b?#x=%2F\"雪\n".into(),
            };
            assert_eq!(EntityRef::decode(&entity.encode()).unwrap(), entity);
        }
        assert!(EntityRef::decode("a#b#c").is_err());
        let mut track = Track::local("/a.m4a");
        track.artists.push(Artist::Name("Artist".into()));
        track.extra = Some(serde_json::json!({"raw": ["#?/", 42]}));
        assert_eq!(
            serde_json::from_str::<Track>(&serde_json::to_string(&track).unwrap()).unwrap(),
            track
        );
    }
}
