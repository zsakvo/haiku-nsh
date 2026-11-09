//! Serialized callers own ordering; replacement is atomic and durable on disk.
use serde::{Serialize, de::DeserializeOwned};
use std::{io::Write, path::Path};

pub fn read<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, std::io::Error> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(std::io::Error::other),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
pub fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), std::io::Error> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("Missing parent directory"))?;
    std::fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut temp, value).map_err(std::io::Error::other)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    std::fs::File::open(parent)?.sync_all()
}
