//! Small native helpers for plugin settings; no platform UI is recreated in TSX.
use base64::Engine;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
    path::Path,
};

pub(crate) fn qr_image(url: &str) -> Result<String, String> {
    let code = qrcode::QrCode::new(url.as_bytes()).map_err(|e| e.to_string())?;
    let image = code
        .render::<image::Luma<u8>>()
        .min_dimensions(256, 256)
        .build();
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes.into_inner())
    ))
}

pub(crate) fn logo(root: &Path, logo: Option<&str>) -> Option<String> {
    let logo = logo?;
    if logo.starts_with("https://") {
        return Some(logo.into());
    }
    let root = root.canonicalize().ok()?;
    let path = root.join(logo).canonicalize().ok()?;
    if !path.starts_with(&root) || std::fs::metadata(&path).ok()?.len() > 512 * 1024 {
        return None;
    }
    let mime = match path.extension()?.to_str()? {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        _ => return None,
    };
    let bytes = std::fs::read(path).ok()?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// Fixed SDK signatures become fields on each exported descriptor.
pub(crate) fn parameters(capabilities: &[String]) -> BTreeMap<String, Value> {
    capabilities.iter().map(|path| {
        let mut fields = Vec::new();
        if path.starts_with("search.") {
            fields.push(json!({"key":"keyword","label":"关键词","target":"object","index":0,"default":""}));
        } else if path == "radio.next" {
            fields.push(json!({"key":"input","label":"频道参数（[] 为私人 FM）","target":"args","type":"json","default":"[]"}));
        } else if path.ends_with(".get") || path.ends_with(".streams") || path.ends_with(".lyrics") && path == "track.lyrics"
            || path == "categories.playlists" || path == "library.browse" {
            fields.push(json!({"key":"id","label":"原始 ID","target":"arg","index":0,"default":""}));
        } else if path.starts_with("scrape.") {
            fields.push(json!({"key":"query","label":if path == "scrape.search" {"刮削查询"} else {"候选元数据"},"target":"arg","index":0,"type":"json",
                "default":if path == "scrape.search" {r#"{"title":"","artists":[]}"#} else {r#"{"id":"","title":"","artists":[]}"#}}));
        } else if path.starts_with("auth.") && path != "auth.user" && path != "auth.refresh" && path != "auth.logout" {
            fields.push(json!({"key":"input","label":"认证参数（JSON）","target":"args","type":"json","default":"[]"}));
        }
        if path.starts_with("search.") || path.ends_with(".get") && path != "track.get"
            || path.starts_with("me.") || path == "artist.tracks" || path == "artist.albums" || path == "categories.playlists" || path == "library.browse" {
            let target = if path.starts_with("search.") {"object"} else {"context"};
            fields.push(json!({"key":"limit","label":"每页数量","target":target,"index":0,"type":"number","default":"20"}));
            fields.push(json!({"key":"cursor","label":"游标（可选）","target":target,"index":0,"default":""}));
        }
        if path == "track.streams" {
            fields.push(json!({"key":"quality","label":"音质","target":"context","default":"high"}));
        }
        if path == "home" {
            fields.push(json!({"key":"cursor","label":"游标（可选）","target":"context","default":""}));
            fields.push(json!({"key":"refresh","label":"刷新","target":"context","type":"json","default":"false"}));
        }
        if path == "radio.next" {
            fields.push(json!({"key":"history","label":"已播放曲目（JSON）","target":"context","type":"json","default":"[]"}));
            fields.push(json!({"key":"count","label":"数量","target":"context","type":"number","default":"20"}));
        }
        (path.clone(), json!(fields))
    }).collect()
}

/// Content hashes detect replacements even when editors preserve file timestamps.
pub(crate) fn directory_hash(root: &Path) -> Result<u64, String> {
    fn visit(
        path: &Path,
        hash: &mut std::hash::DefaultHasher,
        count: &mut usize,
    ) -> Result<(), String> {
        let mut entries = std::fs::read_dir(path)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') || name == "node_modules" || name == "target"
            {
                continue;
            }
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                visit(&entry.path(), hash, count)?;
            } else if kind.is_file() {
                *count += 1;
                if *count > 2048
                    || entry.metadata().map_err(|e| e.to_string())?.len() > 10 * 1024 * 1024
                {
                    return Err("Plugin directory exceeds development limits".into());
                }
                entry.path().hash(hash);
                std::fs::read(entry.path())
                    .map_err(|e| e.to_string())?
                    .hash(hash);
            }
        }
        Ok(())
    }
    let mut hash = std::hash::DefaultHasher::new();
    visit(root, &mut hash, &mut 0)?;
    Ok(hash.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qr_url_generates_square_png_with_quiet_zone() {
        let data = qr_image("https://fixture.invalid/login?token=fake-only").unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data.split_once(',').unwrap().1)
            .unwrap();
        let image = image::load_from_memory(&bytes).unwrap().to_luma8();
        assert_eq!(image.width(), image.height());
        assert!(image.width() >= 256);
        assert_eq!(image.get_pixel(0, 0).0, [255]);
        assert!(image.pixels().any(|p| p.0 == [0]));
        assert!(qr_image(&"x".repeat(10000)).is_err());
    }
    #[test]
    fn signatures_cover_standard_search_and_stream_arguments() {
        let schemas = parameters(&["search.track".into(), "track.streams".into()]);
        assert_eq!(schemas["search.track"][0]["key"], "keyword");
        assert_eq!(schemas["track.streams"][1]["key"], "quality");
    }
}
