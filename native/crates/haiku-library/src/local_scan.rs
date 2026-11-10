//! 本机音频文件扫描：遍历目录 → 读标签 → 抽封面。
//!
//! 只做「文件系统 + 标签」这一层，不理解音乐语义（分组成专辑/歌手、排序、过滤统统在
//! Dart）。FRB 暴露面在 [`crate::local_library`]，见
//! `specs/features/mobile-local-library.md`。
//!
//! 标签库复用 lofty —— 下载完成后给文件打标签用的就是它（`downloader.rs`），
//! 读写覆盖 mp3(ID3v2) / flac(Vorbis comments) / m4a(ilst) / ogg(Vorbis comments)，
//! 正好是我们扫的那几种格式。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::prelude::ItemKey;
use lofty::probe::Probe;
use lofty::tag::Accessor;

/// 扫描白名单。
///
/// **与播放引擎 symphonia 已开启的解码器严格一致**（`Cargo.toml` 的
/// `aac` / `isomp4` / `mp3` / `flac` / `ogg`+`vorbis`）。列出一个点下去放不了的文件，
/// 从用户角度就是 bug —— 宁可它不出现。要加格式先加 symphonia feature，见
/// `specs/features/mobile-local-library.md §2`。
///
/// `ogg` / `oga` 收的是 **Ogg/Vorbis**；同为 Ogg 容器的 Opus 解不了，靠下面
/// [`SUPPORTED_TYPES`] 的内容探测拦掉（扩展名区分不了两者）。`.opus` 不进白名单。
const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "m4a", "aac", "ogg", "oga"];

/// 递归深度上限。音乐库再怎么套也不会有 32 层，超过多半是链环或者选错了根目录。
const MAX_DEPTH: usize = 32;

/// 一条扫描结果。
///
/// 与 FRB 面的 `api::local_library::LocalTrackInfo` **刻意不同名**：同名的两个
/// `pub struct` 会让 codegen 在「按名字合并」时随机挑一个（2026-08-08 撞过），
/// 生成的 Dart 绑定就可能挂到内部结构上。
pub struct ScannedTrack {
    pub path: String,
    /// 无标题标签时回落文件名（不含扩展名）。绝不为空。
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track_no: Option<u32>,
    pub disc_no: Option<u32>,
    pub year: Option<u32>,
    pub duration_sec: Option<u32>,
    pub size: u64,
    /// Unix 秒。取不到时为 0。
    pub mtime: i64,
    /// 已落盘的内嵌封面，或直接引用的外置封面路径。
    pub cover_path: Option<String>,
}

/// 一轮扫描的产物。
pub struct ScanOutcome {
    pub tracks: Vec<ScannedTrack>,
    /// 扩展名在白名单内、但打不开或解不出标签的文件数。
    pub skipped: u32,
    /// 不存在或没权限的扫描根。
    pub unreadable_roots: Vec<String>,
    pub errors: Vec<String>,
    pub cancelled: bool,
}

/// 扫一轮。[`on_progress`] 每处理完一个文件调一次（已扫数, 总数, 当前文件名）。
///
/// 单个文件出任何错都只计入 `skipped`，**绝不中断整轮** —— 一个坏文件不该让五千首
/// 歌都扫不出来。
pub fn scan(
    roots: &[String],
    cover_dir: &Path,
    on_progress: impl FnMut(u32, u32, &str),
) -> ScanOutcome {
    scan_cancellable(roots, cover_dir, || false, on_progress)
}

/// Scan with cooperative cancellation during both traversal and metadata reads.
pub fn scan_cancellable(
    roots: &[String], cover_dir: &Path, cancelled: impl Fn() -> bool,
    mut on_progress: impl FnMut(u32, u32, &str),
) -> ScanOutcome {
    let mut unreadable_roots = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    // 两个根互为父子（用户加了 ~/Music 又加了 ~/Music/Live）时同一个文件会被走两遍。
    // 按规范化路径去重，否则曲库里会出现两条一模一样的歌。
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for root in roots {
        let path = Path::new(root);
        if !path.is_dir() {
            unreadable_roots.push(root.clone());
            continue;
        }
        let before = files.len();
        collect(path, 0, &mut files, &mut seen, &cancelled);
        // 目录在、但一个文件都没读出来且它本身读不动（权限），也算读不到。
        if files.len() == before && fs::read_dir(path).is_err() {
            unreadable_roots.push(root.clone());
        }
    }

    let mut errors: Vec<String> = unreadable_roots.iter().map(|r| format!("无法读取文件夹：{r}")).collect();
    let total = files.len() as u32;
    let mut tracks = Vec::with_capacity(files.len());
    let mut skipped = 0u32;
    let mut dir_cover_cache: HashMap<PathBuf, Option<PathBuf>> = HashMap::new();

    for (i, file) in files.iter().enumerate() {
        if cancelled() { break; }
        let name = file
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        match read_track(file, cover_dir, &mut dir_cover_cache) {
            Some(t) => tracks.push(t),
            None => { skipped += 1; errors.push(format!("无法读取或不支持的音频：{}", file.display())); },
        }
        on_progress(i as u32 + 1, total, &name);
    }

    ScanOutcome {
        tracks,
        skipped,
        unreadable_roots,
        errors,
        cancelled: cancelled(),
    }
}

/// 递归收集白名单扩展名的文件。
///
/// **不跟随符号链接**：一个指向祖先目录的软链就是无限循环，`MAX_DEPTH` 只能兜住
/// 深度、兜不住宽度爆炸。以 `.` 开头的目录一律跳过（含下载半成品 `.haiku-incomplete`
/// 与各类 `.git` / `.Trash`）。
fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, cancelled: &impl Fn() -> bool) {
    if depth >= MAX_DEPTH || cancelled() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if cancelled() { break; }
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        // `DirEntry::metadata` 不跟随链接，故这里能识别出软链本身。
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            let hidden = path
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.starts_with('.'));
            if !hidden {
                collect(&path, depth + 1, out, seen, cancelled);
            }
            continue;
        }
        if !is_audio(&path) {
            continue;
        }
        let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if seen.insert(key) {
            out.push(fs::canonicalize(&path).unwrap_or(path));
        }
    }
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.as_str()))
}

/// 扫描认得的容器：与播放引擎 symphonia 已开启的解码器一一对应
/// （`Cargo.toml` 的 `mp3` / `flac` / `isomp4` / `aac` / `ogg`+`vorbis`）。
///
/// 拿它做**内容**过滤，而不是只信 [`AUDIO_EXTENSIONS`]：扩展名对得上、内容却是
/// 别的容器的文件，放进曲库也是点下去放不响。反过来也成立 —— QQ 音乐解密工具
/// 产出的 `xxx.mgg2.flac` 内容其实是 Ogg/Vorbis，开了 vorbis 解码器后它就该收。
///
/// [`FileType::Opus`] / [`FileType::Speex`] **刻意不在表里**：同样是 Ogg 容器，
/// 但 symphonia 没有对应解码器，收进来就是放不响的死条目。
const SUPPORTED_TYPES: &[FileType] = &[
    FileType::Mpeg,
    FileType::Flac,
    FileType::Mp4,
    FileType::Aac,
    FileType::Vorbis,
];

fn read_track(
    path: &Path,
    cover_dir: &Path,
    dir_cover_cache: &mut HashMap<PathBuf, Option<PathBuf>>,
) -> Option<ScannedTrack> {
    let meta = fs::metadata(path).ok()?;
    // 先按**内容**定容器（`guess_file_type` 探不出时保留扩展名的猜测），再交给
    // lofty 解析 —— `Probe::open` 只按扩展名认，遇到「.flac 里装着 OGG」这种
    // 名实不符的文件会直接解析失败，整首歌被无声跳过。播放引擎那边本来就是纯内容
    // 探测（`Hint::new()`），扫描跟着对齐，两边对「这是什么文件」的判断才一致。
    let probe = match Probe::open(path) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("扫描跳过 {}：打不开（{e}）", path.display());
            return None;
        }
    };
    let probe = match probe.guess_file_type() {
        Ok(p) => p,
        Err(e) => {
            log::warn!("扫描跳过 {}：读不出文件头（{e}）", path.display());
            return None;
        }
    };
    let file_type = probe.file_type();
    if !file_type.is_some_and(|ft| SUPPORTED_TYPES.contains(&ft)) {
        log::warn!(
            "扫描跳过 {}：内容是 {:?}，不在可播放格式内（扩展名与内容不符？）",
            path.display(),
            file_type,
        );
        return None;
    }
    let tagged = match probe.read() {
        Ok(t) => t,
        Err(e) => {
            log::warn!(
                "扫描跳过 {}：{:?} 解析失败（{e}）",
                path.display(),
                file_type
            );
            return None;
        }
    };
    let properties = tagged.properties();
    let duration = properties.duration().as_secs();

    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

    let title = tag
        .and_then(|t| t.title())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        // 没有标题标签的文件多得是（尤其是自己转码的）。回落文件名，
        // 总好过在列表里显示一行空白。
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "未知曲目".to_string())
        });

    let artists = tag.map(split_artists).unwrap_or_default();
    let album = tag.and_then(|t| t.album()).and_then(non_empty);
    let album_artist = tag
        .and_then(|t| t.get_string(ItemKey::AlbumArtist))
        .and_then(|s| non_empty(std::borrow::Cow::Borrowed(s)));

    // 内嵌 > 同名侧车 > 目录级封面。
    let cover_path = tag
        .and_then(|t| {
            extract_cover(
                t,
                cover_dir,
                &cover_key(album_artist.as_deref(), album.as_deref(), path),
            )
        })
        .or_else(|| sidecar_cover(path))
        .or_else(|| directory_cover(path, dir_cover_cache));

    Some(ScannedTrack {
        path: path.to_string_lossy().into_owned(),
        title,
        artists,
        album,
        album_artist,
        track_no: tag.and_then(|t| t.track()),
        disc_no: tag.and_then(|t| t.disk()),
        year: tag.and_then(year_of),
        duration_sec: (duration > 0).then_some(duration as u32),
        size: meta.len(),
        mtime: mtime_secs(&meta),
        cover_path,
    })
}

// ===================== 本地曲目自带的歌词 =====================

/// 单份歌词文本的读取上限。一份歌词几十 KB 顶天，超过这个数多半是同名的别的东西
/// （改过扩展名的文本、导出的日志），读进来只是白占内存。
const MAX_LYRICS_BYTES: u64 = 1 << 20;

/// 侧车译文的文件名后缀（`song.zh.lrc` 之类）。按顺序取第一个存在的。
const TRANSLATION_SUFFIXES: &[&str] = &["zh", "trans", "translation"];

/// 一首本地曲目**自带**的歌词：文件旁边的 `.lrc` 侧车，或文件内嵌的歌词标签。
///
/// 与联网歌词的关键区别是**它不产生任何服务成本**，因此对未登录/非会员一律开放
/// （见 `specs/features/local-file-lyrics.md`）。
pub struct LocalLyrics {
    /// LRC 或纯文本正文（[`haiku_lyrics::parse`] 两者都吃）。绝不为空。
    pub text: String,
    /// 平行译文：双语单文件拆出来的那一半，或独立的译文侧车。
    pub translation: Option<String>,
    /// `sidecar` = 同名 `.lrc`；`embedded` = 文件标签。只用于日志与来源标签。
    pub origin: &'static str,
}

/// 读一首本地音频文件自带的歌词。取不到（两处都没有）返回 None。
///
/// **侧车优先于内嵌**：侧车是用户/下载器后放的，通常比文件里那份旧标签更新更准，
/// 而且用户想换歌词时改一个文本文件比重打标签容易得多。
pub fn read_lyrics(path: &Path) -> Option<LocalLyrics> {
    let (raw, origin) = match read_sidecar(path) {
        Some(text) => (text, "sidecar"),
        None => (embedded_lyrics(path)?, "embedded"),
    };
    if raw.trim().is_empty() {
        return None;
    }

    let (text, mut translation) = haiku_lyrics::split_bilingual(raw.trim());
    if text.trim().is_empty() {
        return None;
    }
    // 双语单文件已经自带译文时不再去找侧车译文：同一份译文来两次只会互相打架。
    if translation.is_none() {
        translation = read_translation_sidecar(path);
    }
    Some(LocalLyrics {
        text,
        translation,
        origin,
    })
}

/// 找主歌词侧车。`song.mp3` → `song.lrc`，另兼容部分工具写的 `song.mp3.lrc`。
///
/// 大小写两种扩展名都试：macOS/Windows 的文件系统不分大小写，Linux/Android 分，
/// 只试一种会让同一个曲库换平台就少一半歌词。
fn read_sidecar(path: &Path) -> Option<String> {
    for candidate in sidecar_candidates(path) {
        if let Some(text) = read_text_file(&candidate) {
            if !text.trim().is_empty() {
                return Some(text);
            }
        }
    }
    None
}

fn sidecar_candidates(path: &Path) -> Vec<PathBuf> {
    let Some(dir) = path.parent() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    // `file_stem` 对 `a.b.mp3` 给的是 `a.b`，正是我们要的（不能用
    // `with_extension`：那会把 `a.b` 的 `.b` 当扩展名替换掉）。
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        out.push(dir.join(format!("{stem}.lrc")));
        out.push(dir.join(format!("{stem}.LRC")));
    }
    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
        out.push(dir.join(format!("{name}.lrc")));
    }
    out
}

/// 找独立的译文侧车（`song.zh.lrc` / `song.trans.lrc` / `song.translation.lrc`）。
fn read_translation_sidecar(path: &Path) -> Option<String> {
    let dir = path.parent()?;
    let stem = path.file_stem().and_then(|s| s.to_str())?;
    for suffix in TRANSLATION_SUFFIXES {
        let candidate = dir.join(format!("{stem}.{suffix}.lrc"));
        if let Some(text) = read_text_file(&candidate) {
            if !text.trim().is_empty() {
                return Some(text);
            }
        }
    }
    None
}

/// 读一个文本文件并解码。不存在 / 过大 / 读不动都返回 None（歌词缺失不是错误）。
fn read_text_file(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_LYRICS_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    if bytes.len() > MAX_LYRICS_BYTES as usize { return None; }
    Some(decode_text(&bytes))
}

/// 字节 → 文本。
///
/// 老曲库里的 `.lrc` **大量是 GBK/GB18030**（很多还是 UTF-16），照 UTF-8 硬解出来
/// 满屏乱码。顺序：BOM（UTF-8 / UTF-16 两端）→ 合法 UTF-8 → chardetng 猜。
///
/// `encoding_rs` / `chardetng` 已经由 CJRT 的 `llrt_fetch` 拖进依赖图并链进产物，
/// 在这里声明成直接依赖不额外增加二进制体积（`Cargo.toml` 里记了这一条）。
fn decode_text(bytes: &[u8]) -> String {
    if let Some(body) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(body).into_owned();
    }
    if let Some(body) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return encoding_rs::UTF_16LE.decode(body).0.into_owned();
    }
    if let Some(body) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return encoding_rs::UTF_16BE.decode(body).0.into_owned();
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    detector.guess(None, true).decode(bytes).0.into_owned()
}

/// 读文件内嵌的歌词标签。
///
/// 两个键都试且**遍历所有 tag**：ID3v2 只认 `USLT`（`UnsyncLyrics`），Vorbis /
/// MP4 认 `Lyrics`（写入侧 `downloader::write_tag` 就是按这个分叉的），而一个文件
/// 里同时挂 ID3v2 和 APE 两套标签也不罕见 —— 只看 primary tag 会漏。
fn embedded_lyrics(path: &Path) -> Option<String> {
    let probe = Probe::open(path).ok()?.guess_file_type().ok()?;
    let tagged = probe.read().ok()?;
    for tag in tagged.tags() {
        for key in [ItemKey::Lyrics, ItemKey::UnsyncLyrics] {
            if let Some(v) = tag.get_string(key) {
                if !v.trim().is_empty() && v.len() <= MAX_LYRICS_BYTES as usize {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// 多艺术家拆分。
///
/// 标签里的写法五花八门：ID3v2 习惯 `/`，很多刮削器写 `;` 或 `feat.`，
/// Vorbis 则规范地存多个 ARTIST 帧。先取多帧（规范做法），只有单帧时才去猜分隔符。
fn split_artists(tag: &lofty::tag::Tag) -> Vec<String> {
    let values: Vec<String> = tag
        .get_strings(ItemKey::TrackArtist)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if values.len() > 1 {
        return values;
    }
    let Some(single) = values.into_iter().next() else {
        return Vec::new();
    };
    let parts: Vec<String> = single
        .split(['/', ';', '、'])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        vec![single]
    } else {
        parts
    }
}

/// 年份。
///
/// 不走 lofty 的 `Accessor::date()`（那要把整串解析成 `Timestamp`，遇到
/// `2019-00-00` 这类刮削器产物会整条丢掉）。直接取前四位数字：`2019` 与
/// `2019-05-01` 都能拿到，拿不到就没有。
fn year_of(tag: &lofty::tag::Tag) -> Option<u32> {
    tag.get_string(ItemKey::RecordingDate)
        .or_else(|| tag.get_string(ItemKey::Year))
        .and_then(|s| s.get(..4))
        .and_then(|y| y.parse::<u32>().ok())
}

fn non_empty(s: std::borrow::Cow<'_, str>) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn mtime_secs(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 封面去重键：同一张专辑的 N 首歌共用一个文件名。
///
/// 专辑名为空时回落到**文件所在目录** —— 散曲同目录共用一张，仍比每首一张省得多。
fn cover_key(album_artist: Option<&str>, album: Option<&str>, path: &Path) -> String {
    let raw = match album {
        Some(a) => format!("{}\x1f{}", album_artist.unwrap_or(""), a),
        None => path
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned()),
    };
    format!("{:016x}", fnv1a64(raw.to_lowercase().as_bytes()))
}

/// FNV-1a 64。只用来给封面文件起名，不需要抗碰撞强度 —— 为此引一个 sha 依赖不值。
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// 把内嵌封面写到 `<cover_dir>/<key>.<ext>`，返回路径。
///
/// **文件已存在就直接返回**：重扫一张 300 首的专辑只写一次盘。
fn extract_cover(tag: &lofty::tag::Tag, cover_dir: &Path, key: &str) -> Option<String> {
    let picture = tag.pictures().first()?;
    let ext = match picture.mime_type().map(|m| m.as_str()) {
        Some("image/png") => "png",
        _ => "jpg",
    };
    let out = cover_dir.join(format!("{key}.{ext}"));
    if out.is_file() {
        return Some(out.to_string_lossy().into_owned());
    }
    fs::create_dir_all(cover_dir).ok()?;
    // 先写临时文件再 rename：扫描中途被杀不会留下半张图，下次扫描却当它有效。
    let tmp = cover_dir.join(format!("{key}.{ext}.part"));
    {
        let mut f = fs::File::create(&tmp).ok()?;
        f.write_all(picture.data()).ok()?;
    }
    fs::rename(&tmp, &out).ok()?;
    Some(out.to_string_lossy().into_owned())
}

const COVER_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp"];
const COVER_BASENAMES: &[&str] = &[
    "cover", "folder", "front", "专辑", "封面", "album", "artwork", "thumb",
];

fn is_cover_image(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|ext| COVER_EXTENSIONS.contains(&ext.as_str()))
}

fn sidecar_cover(path: &Path) -> Option<String> {
    let dir = path.parent()?;
    let stem = path.file_stem().and_then(|s| s.to_str())?;
    if stem.is_empty() {
        return None;
    }
    for ext in COVER_EXTENSIONS {
        let candidate = dir.join(format!("{stem}.{ext}"));
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
        let upper = dir.join(format!("{stem}.{}", ext.to_uppercase()));
        if upper.is_file() {
            return Some(upper.to_string_lossy().into_owned());
        }
    }
    None
}

fn directory_cover(path: &Path, cache: &mut HashMap<PathBuf, Option<PathBuf>>) -> Option<String> {
    let dir = path.parent()?;
    cache
        .entry(dir.to_path_buf())
        .or_insert_with(|| find_directory_cover(dir))
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned())
}

fn find_directory_cover(dir: &Path) -> Option<PathBuf> {
    for name in COVER_BASENAMES {
        for ext in COVER_EXTENSIONS {
            for variant in [*name, name.to_uppercase().as_str()] {
                let candidate = dir.join(format!("{variant}.{ext}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    // 非约定命名仅在目录中只有一张图片时采用，避免猜错。
    let mut only_image = None;
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() || !is_cover_image(&path) {
            continue;
        }
        if only_image.is_some() {
            return None;
        }
        only_image = Some(path);
    }
    only_image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_key_groups_by_album_not_by_file() {
        let a = cover_key(Some("VA"), Some("精选"), Path::new("/m/1.mp3"));
        let b = cover_key(Some("VA"), Some("精选"), Path::new("/m/2.mp3"));
        assert_eq!(a, b);
    }

    #[test]
    fn cover_key_falls_back_to_directory_when_album_missing() {
        let a = cover_key(None, None, Path::new("/m/live/1.mp3"));
        let b = cover_key(None, None, Path::new("/m/live/2.mp3"));
        let c = cover_key(None, None, Path::new("/m/other/1.mp3"));
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn extension_whitelist_matches_symphonia_decoders() {
        assert!(is_audio(Path::new("/m/a.FLAC")));
        assert!(is_audio(Path::new("/m/a.m4a")));
        assert!(is_audio(Path::new("/m/a.ogg")));
        assert!(is_audio(Path::new("/m/a.oga")));
        // 播放引擎解不了 opus / wav，故扫描也不收（Ogg 里装 Opus 的那种由内容探测拦）。
        assert!(!is_audio(Path::new("/m/a.opus")));
        assert!(!is_audio(Path::new("/m/a.wav")));
        assert!(!is_audio(Path::new("/m/cover.jpg")));
    }

    /// 最小可解析的 Ogg/Vorbis：ident + comment + setup 三个包，够 lofty 认容器读标签
    /// （CRC 不参与读取校验，故不算）。
    fn minimal_ogg_vorbis() -> Vec<u8> {
        fn page(header_type: u8, seq: u32, packets: &[&[u8]]) -> Vec<u8> {
            let mut v = b"OggS\x00".to_vec();
            v.push(header_type);
            v.extend_from_slice(&0u64.to_le_bytes()); // granule
            v.extend_from_slice(&1u32.to_le_bytes()); // serial
            v.extend_from_slice(&seq.to_le_bytes());
            v.extend_from_slice(&0u32.to_le_bytes()); // crc（读取侧不校验）
            v.push(packets.len() as u8);
            for p in packets {
                v.push(p.len() as u8); // 每个包都 < 255 字节，段表一格搞定
            }
            for p in packets {
                v.extend_from_slice(p);
            }
            v
        }

        let mut ident = b"\x01vorbis".to_vec();
        ident.extend_from_slice(&0u32.to_le_bytes()); // version
        ident.push(2); // channels
        ident.extend_from_slice(&44100u32.to_le_bytes());
        ident.extend_from_slice(&0i32.to_le_bytes()); // bitrate max
        ident.extend_from_slice(&128_000i32.to_le_bytes()); // bitrate nominal
        ident.extend_from_slice(&0i32.to_le_bytes()); // bitrate min
        ident.push(0xB8); // blocksize
        ident.push(1); // framing

        let mut comment = b"\x03vorbis".to_vec();
        comment.extend_from_slice(&0u32.to_le_bytes()); // vendor 长度
        comment.extend_from_slice(&0u32.to_le_bytes()); // 注释条数
        comment.push(1); // framing

        let setup = b"\x05vorbis\x00\x00".to_vec();

        let mut out = page(0x02, 0, &[&ident]);
        out.extend_from_slice(&page(0x04, 1, &[&comment, &setup]));
        out
    }

    /// 内容说了算：扩展名与内容不符时按内容判。
    ///
    /// - FLAC 内容顶着 `.mp3` 的名字 → 照样扫进来（旧实现按扩展名当 MP3 解，失败跳过）。
    /// - Ogg/Vorbis 内容顶着 `.flac` 的名字（QQ 音乐解密工具产出的 `*.mgg2.flac` 就是
    ///   这样）→ 开了 vorbis 解码器后同样收进来，因为点下去真放得响。
    /// - 内容压根不是音频的 → 跳过。
    #[test]
    fn content_decides_format_not_extension() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("misnamed.mp3"),
            crate::test_support::minimal_flac(),
        )
        .unwrap();
        fs::write(dir.path().join("actually-ogg.flac"), minimal_ogg_vorbis()).unwrap();
        fs::write(dir.path().join("not-audio.ogg"), b"OggS-ish junk").unwrap();

        let report = scan(
            &[dir.path().to_string_lossy().into_owned()],
            &dir.path().join("covers"),
            |_, _, _| {},
        );
        let mut names: Vec<_> = report
            .tracks
            .iter()
            .map(|t| {
                Path::new(&t.path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        assert_eq!(names, vec!["actually-ogg.flac", "misnamed.mp3"]);
        assert_eq!(report.skipped, 1);
    }

    #[test]
    fn scan_reports_unreadable_root() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope").to_string_lossy().into_owned();
        let report = scan(&[missing.clone()], dir.path(), |_, _, _| {});
        assert_eq!(report.unreadable_roots, vec![missing]);
        assert!(report.tracks.is_empty());
    }

    #[test]
    fn scan_skips_hidden_dirs_and_non_audio() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".haiku-incomplete")).unwrap();
        fs::write(dir.path().join(".haiku-incomplete/x.mp3"), b"junk").unwrap();
        fs::write(dir.path().join("notes.txt"), b"hi").unwrap();
        // 白名单内但内容不是音频 → 计入 skipped，不进 tracks，也不中断。
        fs::write(dir.path().join("broken.mp3"), b"junk").unwrap();

        let report = scan(
            &[dir.path().to_string_lossy().into_owned()],
            &dir.path().join("covers"),
            |_, _, _| {},
        );
        assert!(report.tracks.is_empty());
        assert_eq!(report.skipped, 1);
        assert!(report.unreadable_roots.is_empty());
    }

    // ── 自带歌词 ──────────────────────────────────────────

    /// 建一个真的 flac（内嵌歌词得靠 lofty 写进去才有意义）。
    fn flac_at(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, crate::test_support::minimal_flac()).unwrap();
        path
    }

    #[test]
    fn reads_sidecar_lrc_next_to_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let song = flac_at(dir.path(), "song.flac");
        fs::write(dir.path().join("song.lrc"), "[00:01.00]窗外的麻雀\n").unwrap();

        let got = read_lyrics(&song).expect("同名 .lrc 应被读到");
        assert_eq!(got.origin, "sidecar");
        assert!(got.text.contains("窗外的麻雀"));
        assert!(got.translation.is_none());
    }

    #[test]
    fn utf16_both_endians_and_exact_one_mib_sidecar_limit() {
        let dir = tempfile::tempdir().unwrap();
        let song = flac_at(dir.path(), "bounded.flac");
        let path = dir.path().join("bounded.lrc");
        let text = "[00:01.00]窗外的麻雀\n";
        for little in [true, false] {
            let mut bytes = if little { vec![0xff, 0xfe] } else { vec![0xfe, 0xff] };
            for unit in text.encode_utf16() {
                bytes.extend(if little { unit.to_le_bytes() } else { unit.to_be_bytes() });
            }
            fs::write(&path, bytes).unwrap();
            assert_eq!(read_lyrics(&song).unwrap().text.trim(), text.trim());
        }
        let mut exact = b"[00:01.00]".to_vec();
        exact.resize(MAX_LYRICS_BYTES as usize, b'a');
        fs::write(&path, &exact).unwrap();
        assert!(read_lyrics(&song).is_some());
        exact.push(b'a');
        fs::write(&path, exact).unwrap();
        assert!(read_lyrics(&song).is_none());
        fs::remove_file(&path).unwrap();
        let mut tags = crate::tags::AudioTags {
            title: "bounded".into(), artists: vec![], album: None, album_artist: None,
            track_no: None, disc_no: None, year: None, cover_path: None,
            lyrics: Some("x".repeat(MAX_LYRICS_BYTES as usize + 1)),
        };
        crate::tags::write_tag(&song, &tags).unwrap();
        assert!(read_lyrics(&song).is_none());
        tags.lyrics = Some("[00:01]embedded fallback".into());
        crate::tags::write_tag(&song, &tags).unwrap();
        fs::write(&path, vec![b'x'; MAX_LYRICS_BYTES as usize+1]).unwrap();
        assert_eq!(read_lyrics(&song).unwrap().origin, "embedded");
    }

    /// `song.mp3.lrc` 这种「整个文件名再加后缀」的写法也要认。
    #[test]
    fn reads_sidecar_with_full_name_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let song = flac_at(dir.path(), "song.flac");
        fs::write(dir.path().join("song.flac.lrc"), "[00:02.00]hello\n").unwrap();

        let got = read_lyrics(&song).expect("song.flac.lrc 应被读到");
        assert!(got.text.contains("hello"));
    }

    /// 多点文件名不能把 `a.b.flac` 的侧车算成 `a.lrc`（`with_extension` 的坑）。
    #[test]
    fn sidecar_stem_keeps_inner_dots() {
        let dir = tempfile::tempdir().unwrap();
        let song = flac_at(dir.path(), "01. Yesterday.flac");
        fs::write(dir.path().join("01. Yesterday.lrc"), "[00:01.00]y\n").unwrap();
        fs::write(dir.path().join("01.lrc"), "[00:01.00]错的那份\n").unwrap();

        let got = read_lyrics(&song).expect("应读到 `01. Yesterday.lrc`");
        assert!(got.text.contains('y'), "读到的是：{}", got.text);
        assert!(!got.text.contains("错的那份"));
    }

    /// 没有侧车时读内嵌标签；写入侧用的就是 `write_tag`，这里做一次往返。
    #[test]
    fn falls_back_to_embedded_tag() {
        let dir = tempfile::tempdir().unwrap();
        let song = flac_at(dir.path(), "embedded.flac");
        crate::tags::write_tag(
            &song,
            &crate::tags::AudioTags {
                title: "晴天".into(),
                artists: vec!["周杰伦".into()],
                album: None,
                album_artist: None,
                track_no: None,
                disc_no: None,
                year: None,
                cover_path: None,
                lyrics: Some("[00:03.00]故事的小黄花\n".into()),
            },
        )
        .unwrap();

        let got = read_lyrics(&song).expect("内嵌歌词应被读到");
        assert_eq!(got.origin, "embedded");
        assert!(got.text.contains("故事的小黄花"));
    }

    /// 侧车优先于内嵌：用户后放的那份更新更准。
    #[test]
    fn sidecar_wins_over_embedded() {
        let dir = tempfile::tempdir().unwrap();
        let song = flac_at(dir.path(), "both.flac");
        crate::tags::write_tag(
            &song,
            &crate::tags::AudioTags {
                title: "x".into(),
                artists: Vec::new(),
                album: None,
                album_artist: None,
                track_no: None,
                disc_no: None,
                year: None,
                cover_path: None,
                lyrics: Some("[00:01.00]内嵌的\n".into()),
            },
        )
        .unwrap();
        fs::write(dir.path().join("both.lrc"), "[00:01.00]侧车的\n").unwrap();

        let got = read_lyrics(&song).unwrap();
        assert_eq!(got.origin, "sidecar");
        assert!(got.text.contains("侧车的"));
    }

    /// 双语单文件拆出译文（下载器写的 `.lrc` 就是这个形状）。
    #[test]
    fn splits_bilingual_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let song = flac_at(dir.path(), "bi.flac");
        fs::write(
            dir.path().join("bi.lrc"),
            "[00:01.00]Yesterday\n[00:01.00]昨日\n[00:05.00]love was such an easy game\n[00:05.00]爱曾如此简单\n",
        )
        .unwrap();

        let got = read_lyrics(&song).unwrap();
        let translation = got.translation.expect("应拆出译文");
        assert!(got.text.contains("Yesterday"));
        assert!(!got.text.contains("昨日"), "主歌词不该还留着译文");
        assert!(translation.contains("昨日"));
    }

    /// 独立译文侧车（`song.zh.lrc`）。
    #[test]
    fn reads_translation_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let song = flac_at(dir.path(), "t.flac");
        fs::write(dir.path().join("t.lrc"), "[00:01.00]hello\n").unwrap();
        fs::write(dir.path().join("t.zh.lrc"), "[00:01.00]你好\n").unwrap();

        let got = read_lyrics(&song).unwrap();
        assert_eq!(
            got.translation.as_deref().map(str::trim),
            Some("[00:01.00]你好")
        );
    }

    /// 主歌词侧车自己就是 `song.zh.lrc` 时不能被当成主歌词的译文（会与自己合并）。
    #[test]
    fn translation_sidecar_is_not_picked_as_main() {
        let dir = tempfile::tempdir().unwrap();
        let song = flac_at(dir.path(), "only-zh.flac");
        fs::write(dir.path().join("only-zh.zh.lrc"), "[00:01.00]只有译文\n").unwrap();
        // 没有 `only-zh.lrc`、也没有内嵌 → 什么都读不到，而不是把译文当主歌词。
        assert!(read_lyrics(&song).is_none());
    }

    /// 空文件 / 只有空白的侧车不算「有歌词」，要继续往内嵌回落。
    #[test]
    fn blank_sidecar_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let song = flac_at(dir.path(), "blank.flac");
        fs::write(dir.path().join("blank.lrc"), "   \n\n").unwrap();
        assert!(read_lyrics(&song).is_none());
    }

    /// GBK 编码的 `.lrc`（老曲库里遍地都是）不能读成乱码。
    #[test]
    fn decodes_gbk_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let song = flac_at(dir.path(), "gbk.flac");
        let (bytes, _, _) = encoding_rs::GBK.encode("[00:01.00]窗外的麻雀在电线杆上多嘴\n");
        fs::write(dir.path().join("gbk.lrc"), &bytes[..]).unwrap();

        let got = read_lyrics(&song).expect("GBK 侧车应被读到");
        assert!(
            got.text.contains("窗外的麻雀在电线杆上多嘴"),
            "解出来是：{}",
            got.text
        );
    }

    /// 带 BOM 的 UTF-8 不能把 BOM 留在第一行（`[` 前多个字符，整行就解不出时间戳）。
    #[test]
    fn strips_utf8_bom() {
        let dir = tempfile::tempdir().unwrap();
        let song = flac_at(dir.path(), "bom.flac");
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("[00:01.00]有 BOM\n".as_bytes());
        fs::write(dir.path().join("bom.lrc"), bytes).unwrap();

        let got = read_lyrics(&song).unwrap();
        assert!(got.text.starts_with("[00:01"), "首行是：{}", got.text);
    }

    /// 两处都没有歌词 → None（不是空字符串，也不该 panic）。
    #[test]
    fn returns_none_when_nothing_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let song = flac_at(dir.path(), "bare.flac");
        assert!(read_lyrics(&song).is_none());
        // 文件压根不存在也只是 None。
        assert!(read_lyrics(&dir.path().join("ghost.flac")).is_none());
    }
}
