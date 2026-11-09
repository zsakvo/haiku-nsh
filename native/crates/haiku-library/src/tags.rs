//! 本地曲库和下载产物共用的标签写入。

use std::{fs, path::Path};

/// 待写入音频文件的标签。空值一律跳过，不会清掉文件里已有的同名字段以外的东西。
#[derive(Debug, Clone, Default)]
pub struct AudioTags {
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    /// 专辑艺术家。合辑里每首歌的 artist 不同，专辑的署名得单列。
    pub album_artist: Option<String>,
    pub track_no: Option<u32>,
    pub disc_no: Option<u32>,
    pub year: Option<u32>,
    /// 本地封面文件路径（Dart 先下好），`None` = 不内嵌。
    pub cover_path: Option<String>,
    /// 内嵌歌词文本（LRC），`None` = 不内嵌。
    pub lyrics: Option<String>,
}

/// 把 [`AudioTags`] 写进音频文件。
///
/// 跨 crate 共享：下载完成后写标签、以及本地曲库的「刮削结果写回文件标签」
/// （`api/local_library.rs::write_local_tags`）共用这一份，避免两套 ItemKey 映射
/// 各写各的然后慢慢跑偏。
pub fn write_tag(path: &Path, meta: &AudioTags) -> Result<(), String> {
    use lofty::config::WriteOptions;
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::picture::{Picture, PictureType};
    use lofty::probe::Probe;
    use lofty::tag::{Accessor, ItemKey, Tag};

    let mut tagged = Probe::open(path)
        .map_err(|e| e.to_string())?
        .guess_file_type()
        .map_err(|e| e.to_string())?
        .read()
        .map_err(|e| e.to_string())?;

    if tagged.primary_tag().is_none() {
        let kind = tagged.primary_tag_type();
        tagged.insert_tag(Tag::new(kind));
    }
    // 极少数裸流容器压根没有主标签类型，此时放弃写标签（调用方按 warning 处理）。
    let is_id3v2 = tagged.primary_tag_type() == lofty::tag::TagType::Id3v2;
    let tag = tagged
        .primary_tag_mut()
        .ok_or_else(|| String::from("该容器不支持标签"))?;

    if !meta.title.is_empty() {
        tag.set_title(meta.title.clone());
    }
    // 多值艺人：先清空再逐个 push（`insert` 是覆盖语义，push 才追加）。
    let artists: Vec<&String> = meta.artists.iter().filter(|a| !a.is_empty()).collect();
    if !artists.is_empty() {
        tag.remove_key(ItemKey::TrackArtist);
        for a in artists {
            tag.push(lofty::tag::TagItem::new(
                ItemKey::TrackArtist,
                lofty::tag::ItemValue::Text(a.clone()),
            ));
        }
    }
    if let Some(album) = meta.album.as_ref().filter(|s| !s.is_empty()) {
        tag.set_album(album.clone());
    }
    if let Some(v) = meta.album_artist.as_ref().filter(|s| !s.is_empty()) {
        tag.insert_text(ItemKey::AlbumArtist, v.clone());
    }
    if let Some(no) = meta.track_no.filter(|n| *n > 0) {
        tag.set_track(no);
    }
    if let Some(no) = meta.disc_no.filter(|n| *n > 0) {
        tag.set_disk(no);
    }
    if let Some(year) = meta.year.filter(|y| *y >= 1000 && *y <= 9999) {
        // 与扫描侧 `local_scan::year_of` 的读取顺序对齐：先 RecordingDate（ID3v2.4
        // 的 TDRC / Vorbis 的 DATE），该容器不认时回落 Year（ID3v2.3 的 TYER）。
        if !tag.insert_text(ItemKey::RecordingDate, year.to_string()) {
            tag.insert_text(ItemKey::Year, year.to_string());
        }
    }
    if let Some(lyrics) = meta.lyrics.as_ref().filter(|s| !s.trim().is_empty()) {
        // ID3v2 不接受 `ItemKey::Lyrics`（它被同步/非同步两种语义重载），必须用 USLT。
        let key = if is_id3v2 {
            ItemKey::UnsyncLyrics
        } else {
            ItemKey::Lyrics
        };
        if !tag.insert_text(key, lyrics.clone()) {
            tag.insert_text(ItemKey::UnsyncLyrics, lyrics.clone());
        }
    }
    if let Some(cover) = meta.cover_path.as_ref().filter(|s| !s.is_empty()) {
        match fs::File::open(cover)
            .map_err(|e| e.to_string())
            .and_then(|mut f| Picture::from_reader(&mut f).map_err(|e| e.to_string()))
        {
            Ok(mut pic) => {
                pic.set_pic_type(PictureType::CoverFront);
                tag.remove_picture_type(PictureType::CoverFront);
                tag.push_picture(pic);
            }
            // 封面坏了不该毁掉整次标签写入，静默跳过。
            Err(_) => {}
        }
    }

    tagged
        .save_to_path(path, WriteOptions::default())
        .map_err(|e| e.to_string())
}
