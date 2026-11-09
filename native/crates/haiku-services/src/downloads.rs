//! 离线下载的 FRB 暴露面。
//!
//! 这是 [`crate::downloader`] 之上的薄壳：把「给定候选地址 + 落点 + 标签就下一个文件」
//! 压成 FRB 友好的接口。**不理解音乐语义**——取播放地址、下封面、查歌词、套文件名模板、
//! 队列/重试/持久化统统在 Dart（见 `specs/features/downloads.md` §4.1）。
//!
//! 事件流模式同 参考版 menubar 接口：一个进程级 `EventSink`，Dart 侧重新订阅
//! （热重载 / provider 重建）只需再调一次 [`download_events`]，进行中的任务不受影响。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::downloader::{self, Update};
use haiku_model::events::EventSink;

/// 一个候选下载地址。按数组顺序尝试，**响应头非 2xx 立刻换下一个**。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct DownloadCandidate {
    pub url: String,
    /// `{ "Referer": "...", "User-Agent": "..." }`；坏 JSON 视为无 header。
    pub headers_json: String,
    /// 插件回报的格式提示。`audio/flac` 或裸扩展名 `flac` 都接受，`None` = 交给探测。
    pub mime_type: Option<String>,
}

/// 写进音频文件的标签。空字段跳过。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct TrackTag {
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub track_no: Option<u32>,
    /// 本地封面文件路径（Dart 先下好），`None` = 不内嵌。
    pub cover_path: Option<String>,
    /// 内嵌歌词文本（LRC），`None` = 不内嵌。
    pub lyrics: Option<String>,
}

/// 一次下载请求。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct DownloadRequest {
    /// Dart 生成，约定为 `pluginId\x1ftrackId`。[`download_cancel`] 用它定位。
    pub task_id: String,
    pub candidates: Vec<DownloadCandidate>,
    /// 半成品路径，约定 `<root>/.haiku-incomplete/<file_id>.part`。
    /// 必须与 `final_path` 同卷，否则 rename 退化成 copy。已有字节即为续传起点。
    pub temp_path: String,
    /// 最终落点，已由 Dart 套好模板并去重（含扩展名）。父目录不存在会自动创建。
    pub final_path: String,
    pub tag: Option<TrackTag>,
}

/// 事件类型。无字段枚举，FRB 生成普通 Dart enum。
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadEventKind {
    /// 进度推进（节流：字节增量 ≥ 256KiB 或距上次 ≥ 200ms）。
    Progress,
    /// 已带标签落到 `final_path`。**终态**。
    Done,
    /// 失败或被取消（看 `cancelled`）。`.part` 保留供续传。**终态**。
    Error,
}

/// 推给 Dart 的下载事件。用「类型 + 载荷字段」而非带数据枚举，避免 FRB 走 freezed。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct DownloadEvent {
    pub task_id: String,
    pub kind: DownloadEventKind,
    pub received: u64,
    /// 服务端给出的总字节；0 = 未知（chunked）。
    pub total: u64,
    /// `Error` 时的原因；其余为空串。
    pub message: String,
    /// `Done` 时的实际落点；其余为空串。
    pub final_path: String,
    /// `Done` 时探测出的**真实**扩展名（不含点，探测不出为空串）。
    ///
    /// `final_path` 由 Dart 传入且已含扩展名；两者不符时由 Dart 决定是否改名，
    /// Rust 绝不自作主张改路径。
    pub actual_extension: String,
    /// `Done` 时的标签写入警告；空串 = 无警告。**打标签失败不算下载失败**。
    pub warning: String,
    /// `Error` 时区分「用户取消」与「真失败」，Dart 据此决定要不要重试。
    pub cancelled: bool,
}

/// [`download_tag_and_finalize`] 的结果。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct DownloadFinalizeResult {
    pub final_path: String,
    /// 探测出的真实扩展名（不含点），探测不出为空串。
    pub actual_extension: String,
    /// 标签写入警告；空串 = 无警告。
    pub warning: String,
}

/// 当前活跃的 Dart 事件流。重复订阅即替换（热重载后恢复接收）。
static EVENTS_SINK: Mutex<Option<EventSink<DownloadEvent>>> = Mutex::new(None);

/// 订阅下载事件流。可重复订阅：新流替换旧流成为推送目标。
pub fn download_events(sink: EventSink<DownloadEvent>) {
    *EVENTS_SINK.lock().unwrap() = Some(sink);
}

fn emit(event: DownloadEvent) {
    if let Ok(g) = EVENTS_SINK.lock() {
        if let Some(sink) = g.as_ref() {
            let _ = sink.add(event);
        }
    }
}

fn to_event(task_id: &str, update: Update) -> DownloadEvent {
    match update {
        Update::Progress { received, total } => DownloadEvent {
            task_id: task_id.to_string(),
            kind: DownloadEventKind::Progress,
            received,
            total,
            message: String::new(),
            final_path: String::new(),
            actual_extension: String::new(),
            warning: String::new(),
            cancelled: false,
        },
        Update::Done {
            final_path,
            received,
            total,
            actual_extension,
            warning,
        } => DownloadEvent {
            task_id: task_id.to_string(),
            kind: DownloadEventKind::Done,
            received,
            total,
            message: String::new(),
            final_path,
            actual_extension,
            warning,
            cancelled: false,
        },
        Update::Error {
            message,
            received,
            total,
            cancelled,
        } => DownloadEvent {
            task_id: task_id.to_string(),
            kind: DownloadEventKind::Error,
            received,
            total,
            message,
            final_path: String::new(),
            actual_extension: String::new(),
            warning: String::new(),
            cancelled,
        },
    }
}

/// 坏 JSON 视为无 header（仍尝试裸请求），与 `api/playback.rs` 一致。
fn parse_headers(json: &str) -> Vec<(String, String)> {
    serde_json::from_str::<HashMap<String, String>>(json)
        .map(|m| m.into_iter().collect())
        .unwrap_or_default()
}

fn to_tag(tag: TrackTag) -> downloader::AudioTags {
    downloader::AudioTags {
        title: tag.title,
        artists: tag.artists,
        album: tag.album,
        // 下载链路的 `TrackTag` 目前没有这三项（插件的 TrackSummary 里就没有）。
        album_artist: None,
        track_no: tag.track_no,
        disc_no: None,
        year: None,
        cover_path: tag.cover_path,
        lyrics: tag.lyrics,
    }
}

/// 启动一次下载。**立即返回**（不等下载完成）；成败与进度全部经 [`download_events`] 上报。
///
/// 返回 `Err` 只表示请求本身不合法（没有候选 / 没有 task_id），此时不会有任何事件。
/// 同 `task_id` 重复调用会各自独立跑，Dart 侧应自行保证不重复入队。
pub fn download_start(req: DownloadRequest) -> Result<(), String> {
    if req.task_id.is_empty() {
        return Err("task_id 不能为空".into());
    }
    if req.candidates.is_empty() {
        return Err("没有可用的下载地址".into());
    }
    let job = downloader::Job {
        candidates: req
            .candidates
            .into_iter()
            .map(|c| downloader::Candidate {
                url: c.url,
                headers: parse_headers(&c.headers_json),
                mime_type: c.mime_type,
            })
            .collect(),
        temp_path: PathBuf::from(req.temp_path),
        final_path: PathBuf::from(req.final_path),
        tag: req.tag.map(to_tag),
    };
    let task_id = req.task_id;
    let cancel = downloader::register(&task_id);
    let emit_id = task_id.clone();
    downloader::runtime().spawn(async move {
        downloader::run(job, cancel, move |u| emit(to_event(&emit_id, u))).await;
        downloader::unregister(&task_id);
    });
    Ok(())
}

/// 取消进行中的任务。`.part` **保留**供下次续传。未知 id 静默忽略。
///
/// 取消是轮询式的（≤150ms），故卡死的连接也能被中断；随后会推一条
/// `Error{ cancelled: true }` 事件。
pub fn download_cancel(task_id: String) {
    downloader::cancel(&task_id);
}

/// 取消全部进行中的任务（退出 / 全局暂停 / 切到非 Wi-Fi 时用）。
pub fn download_cancel_all() {
    downloader::cancel_all();
}

/// 只做「打标签 + 原子改名」，不联网。
///
/// 用于复用媒体缓存（先 `File.copy` 到 `temp_path`）与重打标签。
/// 标签写失败不算失败：返回值的 `warning` 非空，文件仍然落到 `final_path`。
///
/// `mime_type` 为格式提示（`audio/flac` 或裸 `flac`），`None` = 全靠魔数 / lofty 探测。
pub fn download_tag_and_finalize(
    temp_path: String,
    final_path: String,
    tag: Option<TrackTag>,
    mime_type: Option<String>,
) -> Result<DownloadFinalizeResult, String> {
    let owned = tag.map(to_tag);
    let done = downloader::finalize(
        std::path::Path::new(&temp_path),
        std::path::Path::new(&final_path),
        owned.as_ref(),
        mime_type.as_deref(),
        None,
    )?;
    Ok(DownloadFinalizeResult {
        final_path: done.final_path,
        actual_extension: done.actual_extension,
        warning: done.warning,
    })
}

/// 探测已落盘文件的真实扩展名（不含点）。探测不出返回空串。
///
/// Dart 在「复用媒体缓存」路径上先用它定扩展名，再拼 `final_path`。
pub fn download_detect_extension(path: String, mime_type: Option<String>) -> String {
    downloader::detect_extension(std::path::Path::new(&path), mime_type.as_deref(), None)
        .unwrap_or_default()
}
