//! 本机曲库扫描的 FRB 暴露面。
//!
//! [`crate::local_scan`] 之上的薄壳：把「给一组目录，还我一串带标签的音轨」压成
//! FRB 友好的接口。**不理解音乐语义** —— 分组成专辑/歌手、排序、过滤、索引落盘
//! 统统在 Dart（见 `specs/features/mobile-local-library.md §4`）。
//!
//! 进度流照抄 参考版 downloads 接口：一个进程级 `EventSink`，Dart 侧重新订阅
//! （热重载 / provider 重建）只需再调一次 [`local_scan_events`]，进行中的扫描不受影响。

use std::path::Path;
use std::sync::Mutex;

use crate::local_scan;
use crate::tags::{write_tag, AudioTags};
use haiku_model::events::EventSink;

/// 一条扫描出的音轨。
///
/// 可选字段一律用 `Option` 而不是「空串/0 当哨兵」：这份数据每轮扫描只过一次桥，
/// 多几个 Option 的开销无关紧要，读代码的人少猜一遍才要紧。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct LocalTrackInfo {
    /// 绝对路径。Dart 侧同时拿它当 `trackId`（见规范 §4.1）。
    pub path: String,
    /// 无标题标签时已回落为文件名（不含扩展名），绝不为空。
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track_no: Option<u32>,
    pub disc_no: Option<u32>,
    pub year: Option<u32>,
    pub duration_sec: Option<u32>,
    pub size: u64,
    /// Unix 秒；取不到为 0。
    pub mtime: i64,
    /// 已落盘的封面路径；None = 无内嵌封面。同一张专辑的多首歌共用一个文件。
    pub cover_path: Option<String>,
}

/// 扫描进度。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct LocalScanProgress {
    pub scanned: u32,
    /// 待扫总数（枚举完目录后才确定，故第一条事件之前 UI 只能显示「正在查找」）。
    pub total: u32,
    /// 当前文件名（不含目录），用于进度条下方那行小字。
    pub current: String,
}

/// 一轮扫描的产物。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct LocalScanReport {
    pub tracks: Vec<LocalTrackInfo>,
    /// 扩展名在白名单内、但打不开或解不出标签的文件数。
    pub skipped: u32,
    /// 不存在或没权限的扫描根，原样回传供 UI 标红。
    pub unreadable_roots: Vec<String>,
}

/// 当前活跃的 Dart 进度流。重复订阅即替换（热重载后恢复接收）。
static EVENTS_SINK: Mutex<Option<EventSink<LocalScanProgress>>> = Mutex::new(None);

/// 订阅扫描进度流。可重复订阅：新流替换旧流成为推送目标。
pub fn local_scan_events(sink: EventSink<LocalScanProgress>) {
    *EVENTS_SINK.lock().unwrap() = Some(sink);
}

/// 扫描 [`roots`] 下的音频文件，内嵌封面抽到 [`cover_dir`]。
///
/// 是普通的 `pub fn`（非 `#[frb(sync)]`），FRB 会把它派到 Rust 线程池上跑 ——
/// 遍历几千个文件、逐个读标签的这几秒钟不会卡住 Dart 的 UI isolate。
pub fn scan_local_media(roots: Vec<String>, cover_dir: String) -> LocalScanReport {
    // 进度事件按「每 64 个文件或最后一个」节流：逐文件推的话，扫一个五千首的库要往
    // 桥上灌五千条消息，光是 Dart 侧的 setState 就够卡一阵了。
    const STRIDE: u32 = 64;
    let report = local_scan::scan(&roots, Path::new(&cover_dir), |scanned, total, current| {
        if scanned % STRIDE == 0 || scanned == total {
            emit(LocalScanProgress {
                scanned,
                total,
                current: current.to_string(),
            });
        }
    });
    LocalScanReport {
        tracks: report.tracks.into_iter().map(to_info).collect(),
        skipped: report.skipped,
        unreadable_roots: report.unreadable_roots,
    }
}

fn emit(progress: LocalScanProgress) {
    if let Ok(g) = EVENTS_SINK.lock() {
        if let Some(sink) = g.as_ref() {
            let _ = sink.add(progress);
        }
    }
}

fn to_info(t: local_scan::ScannedTrack) -> LocalTrackInfo {
    LocalTrackInfo {
        path: t.path,
        title: t.title,
        artists: t.artists,
        album: t.album,
        album_artist: t.album_artist,
        track_no: t.track_no,
        disc_no: t.disc_no,
        year: t.year,
        duration_sec: t.duration_sec,
        size: t.size,
        mtime: t.mtime,
        cover_path: t.cover_path,
    }
}

// ===================== 刮削结果写回文件标签 =====================

/// 一次「把刮削 / 手动编辑的结果写回音频文件」请求。
///
/// **结构体名不能与内部实现的 [`AudioTags`] 同名** —— FRB codegen 会报
/// "multiple objects with same key" 并随机挑一个（`mobile-local-library.md §3`
/// 记过这个坑）。故 FRB 面叫 `LocalTagWrite`，内部沿用 `AudioTags`。
///
/// 空的可选字段 = **不动那一项**（不是「清空」）：覆盖层本身是稀疏的，
/// Dart 侧传过来的就是合并后的最终值，没有的项应当保持文件里原来的样子。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct LocalTagWrite {
    pub path: String,
    pub title: Option<String>,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<u32>,
    pub track_no: Option<u32>,
    pub disc_no: Option<u32>,
    /// 本地封面文件路径；None = 不动封面。
    pub cover_path: Option<String>,
    /// 逐行 LRC。逐字（KRC/YRC）必须由 Dart 侧先降级 —— 标签容器只认纯文本。
    pub lyrics: Option<String>,
}

/// 把元信息写回音频文件。
///
/// 返回**空串 = 成功**；非空 = 警告（文件不存在 / 不可写 / 容器不支持标签），
/// **不是致命错**：调用方按 warning 收集，覆盖层不回滚（Android 分区存储、
/// 只读介质上写失败是预期内的，见 `specs/features/local-scraping.md`）。
///
/// 普通 `pub fn`（非 `#[frb(sync)]`）：写标签要重写整个文件头，几百 KB 的搬运
/// 不该压在 UI isolate 上，交给 FRB 的 Rust 线程池。
pub fn write_local_tags(req: LocalTagWrite) -> String {
    let path = Path::new(&req.path);
    if !path.is_file() {
        return format!("文件不存在：{}", req.path);
    }
    let meta = AudioTags {
        // `write_tag` 对空标题是「不写」，正好对上「None = 不动那一项」。
        title: req.title.unwrap_or_default(),
        artists: req.artists,
        album: req.album,
        album_artist: req.album_artist,
        track_no: req.track_no,
        disc_no: req.disc_no,
        year: req.year,
        cover_path: req.cover_path,
        lyrics: req.lyrics,
    };
    match write_tag(path, &meta) {
        Ok(()) => String::new(),
        Err(e) => format!("{}：{e}", file_name_of(&req.path)),
    }
}

// ===================== 读文件自带的歌词 =====================

/// 一首本地曲目自带的歌词（侧车 `.lrc` 或内嵌标签）。
///
/// **不与内部实现 [`local_scan::LocalLyrics`] 同名**：FRB codegen 按名字合并结构体,
/// 撞名会随机挑一个（同 [`LocalTagWrite`] 那条注释，`mobile-local-library.md §3`）。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct LocalFileLyrics {
    /// LRC 或纯文本正文。非空。
    pub text: String,
    /// 平行译文（双语单文件拆出来的一半，或独立译文侧车）。
    pub translation: Option<String>,
    /// `sidecar` = 同名 `.lrc`；`embedded` = 文件标签。
    pub origin: String,
}

/// 读一首本地音频文件自带的歌词。两处（侧车 / 内嵌标签）都没有则返回 None。
///
/// 普通 `pub fn`（非 `#[frb(sync)]`）：要 stat 几个侧车候选、可能还要解一次文件头，
/// 这些盘上的等待不该压在 Dart 的 UI isolate 上。
///
/// 这条路**对未登录/非会员开放** —— 文件就在盘上，读盘不产生任何服务成本
/// （见 `specs/features/local-file-lyrics.md`）。
pub fn read_local_lyrics(path: String) -> Option<LocalFileLyrics> {
    local_scan::read_lyrics(Path::new(&path)).map(|l| LocalFileLyrics {
        text: l.text,
        translation: l.translation,
        origin: l.origin.to_string(),
    })
}

fn file_name_of(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::minimal_flac;
    use std::fs;

    /// 写回 → 用扫描侧读回来核对。
    ///
    /// **这是唯一能真正验证 lofty ItemKey 映射对不对的方式**：`AlbumArtist` /
    /// `RecordingDate` / `set_disk` 三个是这一版新加的，写错了不会报错，只是读回来
    /// 是空的 —— 不做往返就发现不了。
    #[test]
    fn write_then_scan_round_trips_every_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.flac");
        fs::write(&path, minimal_flac()).unwrap();

        let warning = write_local_tags(LocalTagWrite {
            path: path.to_string_lossy().into_owned(),
            title: Some("晴天".into()),
            artists: vec!["周杰伦".into(), "方文山".into()],
            album: Some("叶惠美".into()),
            album_artist: Some("周杰伦".into()),
            year: Some(2003),
            track_no: Some(3),
            disc_no: Some(1),
            cover_path: None,
            lyrics: Some("[00:01.00]故事的小黄花\n".into()),
        });
        assert!(warning.is_empty(), "写回不该有警告：{warning}");

        let report = local_scan::scan(
            &[dir.path().to_string_lossy().into_owned()],
            &dir.path().join("covers"),
            |_, _, _| {},
        );
        let t = report.tracks.first().expect("应扫到刚写的那首");
        assert_eq!(t.title, "晴天");
        assert_eq!(t.artists, vec!["周杰伦".to_string(), "方文山".to_string()]);
        assert_eq!(t.album.as_deref(), Some("叶惠美"));
        assert_eq!(t.album_artist.as_deref(), Some("周杰伦"));
        assert_eq!(t.year, Some(2003));
        assert_eq!(t.track_no, Some(3));
        assert_eq!(t.disc_no, Some(1));
    }

    /// 空的可选字段 = 不动那一项，而不是清空。
    #[test]
    fn absent_fields_leave_existing_tags_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.flac");
        fs::write(&path, minimal_flac()).unwrap();
        let p = path.to_string_lossy().into_owned();

        write_local_tags(LocalTagWrite {
            path: p.clone(),
            title: Some("晴天".into()),
            artists: vec!["周杰伦".into()],
            album: Some("叶惠美".into()),
            album_artist: None,
            year: Some(2003),
            track_no: None,
            disc_no: None,
            cover_path: None,
            lyrics: None,
        });
        // 第二次只改标题，别的都不传。
        write_local_tags(LocalTagWrite {
            path: p,
            title: Some("晴天 (Live)".into()),
            artists: Vec::new(),
            album: None,
            album_artist: None,
            year: None,
            track_no: None,
            disc_no: None,
            cover_path: None,
            lyrics: None,
        });

        let report = local_scan::scan(
            &[dir.path().to_string_lossy().into_owned()],
            &dir.path().join("covers"),
            |_, _, _| {},
        );
        let t = report.tracks.first().unwrap();
        assert_eq!(t.title, "晴天 (Live)");
        assert_eq!(t.album.as_deref(), Some("叶惠美"), "没传的字段不该被清掉");
        assert_eq!(t.artists, vec!["周杰伦".to_string()]);
        assert_eq!(t.year, Some(2003));
    }

    /// 文件不存在 / 不是音频 → 只给警告，不 panic（写回失败是预期内的）。
    #[test]
    fn failures_are_warnings_not_panics() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope.flac").to_string_lossy().into_owned();
        assert!(!write_local_tags(blank(missing)).is_empty());

        let junk = dir.path().join("junk.mp3");
        fs::write(&junk, b"not audio at all").unwrap();
        let w = write_local_tags(blank(junk.to_string_lossy().into_owned()));
        assert!(!w.is_empty());
        assert!(w.contains("junk.mp3"), "警告应带上文件名：{w}");
    }

    fn blank(path: String) -> LocalTagWrite {
        LocalTagWrite {
            path,
            title: Some("x".into()),
            artists: Vec::new(),
            album: None,
            album_artist: None,
            year: None,
            track_no: None,
            disc_no: None,
            cover_path: None,
            lyrics: None,
        }
    }
}
