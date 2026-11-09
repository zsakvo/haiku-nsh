//! 离线下载：单文件搬运工。
//!
//! 职责严格限定为「传输 + 打标签 + 原子落盘」，**不含队列/重试/持久化**——那些全在 Dart
//! （见 `specs/features/downloads.md` §4）。因此本模块无全局状态可言，只有一张
//! `task_id -> CancelToken` 的取消登记表，任何一次下载都是一个独立的 [`Job`]。
//!
//! 关键约定：
//! - **候选回退拿响应头就判成败**：非 2xx（尤其 403/416）立刻切下一个候选，绝不等到写盘
//!   中途才报错。这是 `specs/features/streaming-playback-v2.md` §5 记录的血泪教训——
//!   死节点会把回退循环骗停。
//! - **断点续传**：`temp_path` 已有 N 字节则请求 `Range: bytes=N-`；服务端回 200（忽略
//!   Range）就当全量重写；回 416 则删残片重来一次。
//! - **顺序**：`.part` 写满 → 打标签 → 原子 rename 到 `final_path`。用户在成品目录里
//!   永远看不到半成品或没标签的文件。
//! - **打标签失败不算下载失败**：音频文件照样落地，警告随 `Done` 一起回给 Dart。

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use futures::StreamExt;
pub use haiku_library::tags::{write_tag, AudioTags};

/// 进度节流：字节增量达到这个数就推一次。
const PROGRESS_MIN_BYTES: u64 = 256 * 1024;
/// 进度节流：距上次推送超过这个时长就推一次（慢速下载也有心跳）。
const PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(200);
/// 取消轮询步长。兼作「连接卡死时仍能被取消」的保底——不靠 chunk 到达来发现取消。
const CANCEL_POLL: Duration = Duration::from_millis(150);
/// 建连超时，与 `player.rs` 的 client 保持一致。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

// ===================== 输入 =====================

/// 一个候选下载地址。多个候选按序尝试，第一个响应头 2xx 的胜出。
#[derive(Debug, Clone)]
pub struct Candidate {
    pub url: String,
    /// 已解析好的请求头（Referer / UA / Cookie 等）。
    pub headers: Vec<(String, String)>,
    /// 插件回报的格式提示（`audio/flac` 或裸扩展名 `flac` 都接受）。
    pub mime_type: Option<String>,
}

/// 一次下载任务。
#[derive(Debug, Clone)]
pub struct Job {
    pub candidates: Vec<Candidate>,
    /// 半成品路径（`<root>/.haiku-incomplete/<file_id>.part`）。与 `final_path` 同卷才能原子 rename。
    pub temp_path: PathBuf,
    /// 最终落点，已由 Dart 套好模板并去重（含扩展名）。
    pub final_path: PathBuf,
    pub tag: Option<AudioTags>,
}

// ===================== 输出 =====================

/// 下载过程回报。由调用方（`api/downloads.rs`）转成 FRB 事件推给 Dart。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    /// 进度。`total` 为 0 表示服务端没给总长（chunked）。
    Progress { received: u64, total: u64 },
    /// 完成。文件已带标签、已在 `final_path`。
    Done {
        final_path: String,
        received: u64,
        total: u64,
        /// 探测出的真实扩展名（不含点）。与 `final_path` 后缀不符时由 **Dart** 决定是否改名，
        /// Rust 绝不自作主张改路径。探测不出时为空串。
        actual_extension: String,
        /// 标签写入警告；空串 = 无警告。**有警告也是成功**。
        warning: String,
    },
    /// 失败或被取消。`.part` 一律保留供续传。
    Error {
        message: String,
        received: u64,
        total: u64,
        /// 用户主动取消（Dart 侧据此不重试）。
        cancelled: bool,
    },
}

/// [`finalize`] 的结果。
#[derive(Debug, Clone)]
pub struct Finalized {
    pub final_path: String,
    pub actual_extension: String,
    pub warning: String,
}

// ===================== 取消 =====================

/// 取消信号。轮询式（`CANCEL_POLL`），不依赖数据到达，故卡死的连接也能被中断。
#[derive(Debug, Default)]
pub struct CancelToken(AtomicBool);

impl CancelToken {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(AtomicBool::new(false)))
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

async fn wait_cancelled(token: &CancelToken) {
    loop {
        if token.is_cancelled() {
            return;
        }
        tokio::time::sleep(CANCEL_POLL).await;
    }
}

fn registry() -> &'static Mutex<HashMap<String, Arc<CancelToken>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<CancelToken>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 登记任务并拿到取消句柄。同 id 重复登记会顶掉旧句柄（旧任务随后自然结束）。
pub fn register(task_id: &str) -> Arc<CancelToken> {
    let token = CancelToken::new();
    if let Ok(mut g) = registry().lock() {
        g.insert(task_id.to_string(), token.clone());
    }
    token
}

/// 注销（任务收尾时调用）。
pub fn unregister(task_id: &str) {
    if let Ok(mut g) = registry().lock() {
        g.remove(task_id);
    }
}

/// 取消某个进行中的任务。`.part` 保留供续传。未知 id 静默忽略。
pub fn cancel(task_id: &str) {
    if let Ok(g) = registry().lock() {
        if let Some(t) = g.get(task_id) {
            t.cancel();
        }
    }
}

/// 取消全部进行中的任务。
pub fn cancel_all() {
    if let Ok(g) = registry().lock() {
        for t in g.values() {
            t.cancel();
        }
    }
}

// ===================== 运行时 / client =====================

/// 下载专用运行时，与播放运行时、插件 host 运行时互不干扰。
pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("创建下载运行时失败")
    })
}

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .unwrap_or_default()
    })
}

// ===================== 主流程 =====================

/// 单个候选尝试的失败分类。
enum AttemptErr {
    /// 用户取消，立刻收敛整个 job。
    Cancelled,
    /// 残片不可用（416 / 长度对不上）：删掉 `.part` 用**同一候选**重来一次。
    Restart(String),
    /// 本候选不可用：切下一个候选。
    Failed(String),
}

/// 跑一个下载任务，全过程通过 `emit` 回报。函数返回即任务终结（成功 / 失败 / 取消）。
///
/// `emit` 会在下载线程上被同步调用，实现方需自行保证廉价且线程安全。
pub async fn run<F>(job: Job, cancel: Arc<CancelToken>, emit: F)
where
    F: Fn(Update),
{
    if let Some(parent) = job.temp_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            emit(Update::Error {
                message: format!("创建半成品目录失败: {e}"),
                received: 0,
                total: 0,
                cancelled: false,
            });
            return;
        }
    }

    let mut known_total: Option<u64> = None;
    let mut last_error = String::from("没有可用的下载地址");

    for cand in &job.candidates {
        if cancel.is_cancelled() {
            emit(cancelled_event(&job, known_total));
            return;
        }
        // 每个候选最多两轮：第二轮只在残片被判定不可用（Restart）后触发。
        let mut restarted = false;
        loop {
            match attempt(&job, cand, &cancel, &emit, &mut known_total).await {
                Ok(content_type) => {
                    let received = file_len(&job.temp_path);
                    match finalize(
                        &job.temp_path,
                        &job.final_path,
                        job.tag.as_ref(),
                        cand.mime_type.as_deref(),
                        content_type.as_deref(),
                    ) {
                        Ok(done) => emit(Update::Done {
                            final_path: done.final_path,
                            received,
                            total: known_total.unwrap_or(received),
                            actual_extension: done.actual_extension,
                            warning: done.warning,
                        }),
                        Err(e) => emit(Update::Error {
                            message: e,
                            received,
                            total: known_total.unwrap_or(0),
                            cancelled: false,
                        }),
                    }
                    return;
                }
                Err(AttemptErr::Cancelled) => {
                    emit(cancelled_event(&job, known_total));
                    return;
                }
                Err(AttemptErr::Restart(msg)) => {
                    let _ = fs::remove_file(&job.temp_path);
                    known_total = None;
                    if restarted {
                        last_error = msg;
                        break;
                    }
                    restarted = true;
                    continue;
                }
                Err(AttemptErr::Failed(msg)) => {
                    last_error = msg;
                    break;
                }
            }
        }
    }

    emit(Update::Error {
        message: last_error,
        received: file_len(&job.temp_path),
        total: known_total.unwrap_or(0),
        cancelled: false,
    });
}

fn cancelled_event(job: &Job, total: Option<u64>) -> Update {
    Update::Error {
        message: "已取消".into(),
        received: file_len(&job.temp_path),
        total: total.unwrap_or(0),
        cancelled: true,
    }
}

fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// 尝试一个候选。成功时 `.part` 已写满，返回响应的 Content-Type（供格式探测）。
async fn attempt<F>(
    job: &Job,
    cand: &Candidate,
    cancel: &CancelToken,
    emit: &F,
    known_total: &mut Option<u64>,
) -> Result<Option<String>, AttemptErr>
where
    F: Fn(Update),
{
    let have = file_len(&job.temp_path);

    let mut req = client().get(&cand.url);
    for (k, v) in &cand.headers {
        req = req.header(k.as_str(), v.as_str());
    }
    if have > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }

    let sent = tokio::select! {
        biased;
        _ = wait_cancelled(cancel) => return Err(AttemptErr::Cancelled),
        r = req.send() => r,
    };
    let resp = match sent {
        Ok(r) => r,
        Err(e) => return Err(AttemptErr::Failed(format!("发起请求失败: {e}"))),
    };

    // ⚠️ 响应头就是判决点。非 2xx 一律立刻放弃本候选，不进入写盘循环。
    let status = resp.status();
    if !status.is_success() {
        if have > 0 && status.as_u16() == 416 {
            return Err(AttemptErr::Restart("续传范围无效（416），残片作废".into()));
        }
        return Err(AttemptErr::Failed(format!("下载地址返回 {status}")));
    }

    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // 服务端认了 Range 才算续传；回 200 说明它忽略了 Range，响应体是完整文件，从 0 重写。
    let resuming = have > 0 && status.as_u16() == 206;

    let total = if let Some(range) = resp
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
    {
        range.rsplit('/').next().and_then(|t| t.trim().parse().ok())
    } else {
        resp.content_length()
            .map(|len| if resuming { have + len } else { len })
    };

    // 候选之间总长不一致 = 残片来自另一个文件，混着写会得到坏音频。
    if let (Some(t), Some(k)) = (total, *known_total) {
        if t != k && have > 0 {
            return Err(AttemptErr::Restart(
                "候选文件长度与已有残片不符，残片作废".into(),
            ));
        }
    }
    if total.is_some() {
        *known_total = total;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&job.temp_path)
        .map_err(|e| AttemptErr::Failed(format!("打开半成品文件失败: {e}")))?;
    let mut received = if resuming {
        file.seek(SeekFrom::Start(have))
            .map_err(|e| AttemptErr::Failed(format!("定位半成品文件失败: {e}")))?;
        have
    } else {
        file.set_len(0)
            .and_then(|()| file.seek(SeekFrom::Start(0)).map(|_| ()))
            .map_err(|e| AttemptErr::Failed(format!("重置半成品文件失败: {e}")))?;
        0
    };

    let total_u = total.unwrap_or(0);
    emit(Update::Progress {
        received,
        total: total_u,
    });
    let mut sent_bytes = received;
    let mut sent_at = Instant::now();

    let mut stream = resp.bytes_stream();
    loop {
        let next = tokio::select! {
            biased;
            _ = wait_cancelled(cancel) => return Err(AttemptErr::Cancelled),
            n = stream.next() => n,
        };
        let Some(chunk) = next else { break };
        match chunk {
            Ok(bytes) => {
                file.write_all(&bytes)
                    .map_err(|e| AttemptErr::Failed(format!("写入半成品文件失败: {e}")))?;
                received += bytes.len() as u64;
                if received - sent_bytes >= PROGRESS_MIN_BYTES
                    || sent_at.elapsed() >= PROGRESS_MIN_INTERVAL
                {
                    sent_bytes = received;
                    sent_at = Instant::now();
                    emit(Update::Progress {
                        received,
                        total: total_u,
                    });
                }
            }
            Err(e) => return Err(AttemptErr::Failed(format!("读取下载流失败: {e}"))),
        }
    }
    file.flush()
        .map_err(|e| AttemptErr::Failed(format!("落盘失败: {e}")))?;
    drop(file);

    // 已知总长却短读 = 连接被截断，当作本候选失败（`.part` 保留，下个候选可续）。
    if let Some(t) = total {
        if t > 0 && received < t {
            return Err(AttemptErr::Failed(format!(
                "下载提前结束（{received}/{t}）"
            )));
        }
    }
    if received == 0 {
        return Err(AttemptErr::Failed("下载内容为空".into()));
    }
    emit(Update::Progress {
        received,
        total: total_u,
    });
    Ok(content_type)
}

// ===================== 打标签 + 原子落盘 =====================

/// 打标签 + 原子 rename。不联网，供「复用媒体缓存」与「重打标签」直接调用。
///
/// 标签写失败只记 warning，音频文件照样落地——这是设计约定（§4.4）。
pub fn finalize(
    temp: &Path,
    final_path: &Path,
    tag: Option<&AudioTags>,
    mime_hint: Option<&str>,
    content_type: Option<&str>,
) -> Result<Finalized, String> {
    if !temp.is_file() {
        return Err(format!("半成品文件不存在: {}", temp.display()));
    }
    let actual_extension = detect_extension(temp, mime_hint, content_type).unwrap_or_default();

    let mut warning = String::new();
    if let Some(t) = tag {
        if let Err(e) = write_tag(temp, t) {
            warning = format!("标签写入失败: {e}");
        }
    }

    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建下载目录失败: {e}"))?;
    }
    move_file(temp, final_path)?;

    Ok(Finalized {
        final_path: final_path.display().to_string(),
        actual_extension,
        warning,
    })
}

/// 同卷用 rename（原子）；跨卷回落 copy + delete。
fn move_file(from: &Path, to: &Path) -> Result<(), String> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to).map_err(|e| format!("移动下载文件失败: {e}"))?;
    let _ = fs::remove_file(from);
    Ok(())
}

// ===================== 格式探测 =====================

/// 定真实扩展名（不含点）。顺序：插件 mime 提示 → Content-Type → 魔数 → lofty 兜底。
pub fn detect_extension(
    path: &Path,
    mime_hint: Option<&str>,
    content_type: Option<&str>,
) -> Option<String> {
    if let Some(e) = mime_hint.and_then(ext_from_hint) {
        return Some(e);
    }
    if let Some(e) = content_type.and_then(ext_from_hint) {
        return Some(e);
    }
    if let Some(e) = ext_from_magic(path) {
        return Some(e);
    }
    ext_from_probe(path)
}

/// 接受 `audio/flac`、`audio/mpeg; charset=x`，也接受插件常给的裸扩展名 `flac` / `.m4a`。
fn ext_from_hint(hint: &str) -> Option<String> {
    let head = hint.split(';').next()?.trim().to_ascii_lowercase();
    if head.is_empty() {
        return None;
    }
    if !head.contains('/') {
        let bare = head.trim_start_matches('.');
        return match bare {
            "mp3" | "flac" | "m4a" | "aac" | "ogg" | "opus" | "wav" | "ape" | "wv" => {
                Some(bare.to_string())
            }
            "mp4" => Some("m4a".into()),
            _ => None,
        };
    }
    let ext = match head.as_str() {
        "audio/mpeg" | "audio/mp3" | "audio/x-mpeg" | "audio/mpeg3" | "audio/x-mpeg-3" => "mp3",
        "audio/flac" | "audio/x-flac" => "flac",
        "audio/mp4" | "audio/m4a" | "audio/x-m4a" | "video/mp4" => "m4a",
        "audio/aac" | "audio/aacp" | "audio/x-aac" => "aac",
        "audio/ogg" | "application/ogg" | "audio/vorbis" | "audio/x-vorbis+ogg" => "ogg",
        "audio/opus" => "opus",
        "audio/wav" | "audio/x-wav" | "audio/wave" | "audio/vnd.wave" => "wav",
        "audio/ape" | "audio/x-ape" | "audio/x-monkeys-audio" => "ape",
        // `application/octet-stream` 之类的通用类型不可信，交给魔数。
        _ => return None,
    };
    Some(ext.to_string())
}

fn ext_from_magic(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut buf = [0u8; 12];
    let mut filled = 0usize;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(_) => break,
        }
    }
    let b = &buf[..filled];
    if b.len() >= 4 && &b[..4] == b"fLaC" {
        return Some("flac".into());
    }
    if b.len() >= 4 && &b[..4] == b"OggS" {
        return Some("ogg".into());
    }
    if b.len() >= 8 && &b[4..8] == b"ftyp" {
        return Some("m4a".into());
    }
    if b.len() >= 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WAVE" {
        return Some("wav".into());
    }
    if b.len() >= 4 && &b[..4] == b"MAC " {
        return Some("ape".into());
    }
    if b.len() >= 3 && &b[..3] == b"ID3" {
        return Some("mp3".into());
    }
    // 裸 MPEG 帧同步字（11 bit 全 1）。
    if b.len() >= 2 && b[0] == 0xFF && (b[1] & 0xE0) == 0xE0 {
        return Some("mp3".into());
    }
    None
}

fn ext_from_probe(path: &Path) -> Option<String> {
    let probe = lofty::probe::Probe::open(path)
        .ok()?
        .guess_file_type()
        .ok()?;
    ext_from_file_type(probe.file_type()?)
}

fn ext_from_file_type(ft: lofty::file::FileType) -> Option<String> {
    use lofty::file::FileType;
    let ext = match ft {
        FileType::Aac => "aac",
        FileType::Aiff => "aiff",
        FileType::Ape => "ape",
        FileType::Flac => "flac",
        FileType::Mpeg => "mp3",
        FileType::Mp4 => "m4a",
        FileType::Mpc => "mpc",
        FileType::Opus => "opus",
        FileType::Vorbis => "ogg",
        FileType::Speex => "spx",
        FileType::Wav => "wav",
        FileType::WavPack => "wv",
        _ => return None,
    };
    Some(ext.to_string())
}

// ===================== lofty 写标签 =====================

// ===================== 测试 =====================

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use haiku_library::test_support::{minimal_flac, tiny_jpeg};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;

    fn tmp_dir(name: &str) -> PathBuf {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("haiku_dl_{name}_{ms}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 起一个本地 HTTP 服务，按序对每个连接回一份响应；把收到的请求原文送回 channel。
    /// 不引入任何测试依赖（照抄 `player.rs` 的 `serve_once` 做法）。
    fn serve(responses: Vec<Vec<u8>>) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/song", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for resp in responses {
                let Ok((mut sock, _)) = listener.accept() else {
                    return;
                };
                let mut req = String::new();
                let mut byte = [0u8; 1];
                while !req.ends_with("\r\n\r\n") {
                    match sock.read(&mut byte) {
                        Ok(1) => req.push(byte[0] as char),
                        _ => break,
                    }
                }
                let _ = tx.send(req);
                let _ = sock.write_all(&resp);
                let _ = sock.flush();
            }
        });
        (url, rx)
    }

    /// 起一个只发响应头 + 一小段字节、随后挂住不发的服务（模拟卡死连接）。
    fn serve_stalling(head: &'static str, body: &'static [u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/song", listener.local_addr().unwrap());
        thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut req = String::new();
                let mut byte = [0u8; 1];
                while !req.ends_with("\r\n\r\n") {
                    match sock.read(&mut byte) {
                        Ok(1) => req.push(byte[0] as char),
                        _ => break,
                    }
                }
                let _ = sock.write_all(head.as_bytes());
                let _ = sock.write_all(body);
                let _ = sock.flush();
                thread::sleep(Duration::from_secs(10));
            }
        });
        url
    }

    fn candidate(url: &str) -> Candidate {
        Candidate {
            url: url.to_string(),
            headers: Vec::new(),
            mime_type: None,
        }
    }

    fn drive(job: Job) -> Vec<Update> {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        rt.block_on(run(job, CancelToken::new(), move |u| {
            sink.lock().unwrap().push(u)
        }));
        let out = events.lock().unwrap().clone();
        out
    }

    fn last_error(updates: &[Update]) -> (&str, bool) {
        match updates.last().expect("至少要有一个事件") {
            Update::Error {
                message, cancelled, ..
            } => (message.as_str(), *cancelled),
            other => panic!("期望 Error，实得 {other:?}"),
        }
    }

    #[test]
    fn resume_requests_range_from_existing_part_size() {
        let dir = tmp_dir("resume");
        let temp = dir.join("x.part");
        fs::write(&temp, b"HELLO").unwrap();
        let (url, reqs) = serve(vec![concat!(
            "HTTP/1.1 206 Partial Content\r\n",
            "Content-Range: bytes 5-9/10\r\n",
            "Content-Length: 5\r\n",
            "Connection: close\r\n\r\n",
            "WORLD"
        )
        .as_bytes()
        .to_vec()]);

        let final_path = dir.join("out.bin");
        let updates = drive(Job {
            candidates: vec![candidate(&url)],
            temp_path: temp.clone(),
            final_path: final_path.clone(),
            tag: None,
        });

        let req = reqs.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            req.contains("range: bytes=5-") || req.contains("Range: bytes=5-"),
            "{req}"
        );
        assert!(
            matches!(updates.last(), Some(Update::Done { .. })),
            "{updates:?}"
        );
        assert_eq!(fs::read(&final_path).unwrap(), b"HELLOWORLD");
        assert!(!temp.exists(), "落盘后半成品应已 rename 走");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fresh_download_sends_no_range_header() {
        let dir = tmp_dir("fresh");
        let (url, reqs) = serve(vec![
            b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc".to_vec(),
        ]);
        let updates = drive(Job {
            candidates: vec![candidate(&url)],
            temp_path: dir.join("x.part"),
            final_path: dir.join("out.bin"),
            tag: None,
        });
        let req = reqs.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!req.to_ascii_lowercase().contains("range:"), "{req}");
        assert!(matches!(updates.last(), Some(Update::Done { .. })));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_2xx_switches_to_next_candidate_immediately() {
        let dir = tmp_dir("fallback");
        let (bad, _r1) = serve(vec![
            b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ]);
        let (good, _r2) = serve(vec![
            b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nBYTES".to_vec(),
        ]);
        let final_path = dir.join("out.bin");
        let updates = drive(Job {
            candidates: vec![candidate(&bad), candidate(&good)],
            temp_path: dir.join("x.part"),
            final_path: final_path.clone(),
            tag: None,
        });
        assert!(
            matches!(updates.last(), Some(Update::Done { .. })),
            "{updates:?}"
        );
        assert_eq!(fs::read(&final_path).unwrap(), b"BYTES");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn all_candidates_failing_reports_error() {
        let dir = tmp_dir("allfail");
        let (a, _r1) = serve(vec![
            b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ]);
        let (b, _r2) = serve(vec![
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ]);
        let final_path = dir.join("out.bin");
        let updates = drive(Job {
            candidates: vec![candidate(&a), candidate(&b)],
            temp_path: dir.join("x.part"),
            final_path: final_path.clone(),
            tag: None,
        });
        let (msg, cancelled) = last_error(&updates);
        assert!(msg.contains("404"), "{msg}");
        assert!(!cancelled);
        assert!(!final_path.exists(), "失败不得在成品目录留下文件");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancel_keeps_part_file_for_resume() {
        let dir = tmp_dir("cancel");
        let temp = dir.join("x.part");
        let final_path = dir.join("out.bin");
        let url = serve_stalling(
            "HTTP/1.1 200 OK\r\nContent-Length: 4096\r\nConnection: close\r\n\r\n",
            b"0123456789",
        );
        let job = Job {
            candidates: vec![candidate(&url)],
            temp_path: temp.clone(),
            final_path: final_path.clone(),
            tag: None,
        };
        let token = CancelToken::new();
        let worker = token.clone();
        let handle = thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let events = Arc::new(Mutex::new(Vec::new()));
            let sink = events.clone();
            rt.block_on(run(job, worker, move |u| sink.lock().unwrap().push(u)));
            let out = events.lock().unwrap().clone();
            out
        });
        thread::sleep(Duration::from_millis(600));
        token.cancel();
        let updates = handle.join().unwrap();

        let (msg, cancelled) = last_error(&updates);
        assert!(cancelled, "{msg}");
        assert!(temp.is_file(), "取消必须保留 .part 供续传");
        assert_eq!(fs::metadata(&temp).unwrap().len(), 10);
        assert!(!final_path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn finalize_creates_missing_parent_dirs() {
        let dir = tmp_dir("mkdir");
        let temp = dir.join("x.part");
        fs::write(&temp, b"fLaC-ish").unwrap();
        let final_path = dir.join("周杰伦").join("叶惠美").join("一路向北.flac");
        let out = finalize(&temp, &final_path, None, None, None).unwrap();
        assert_eq!(out.final_path, final_path.display().to_string());
        assert!(final_path.is_file());
        assert!(!temp.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_extension_prefers_hint_then_content_type_then_magic() {
        let dir = tmp_dir("detect");
        let flac = dir.join("a.bin");
        fs::write(&flac, minimal_flac()).unwrap();
        // 提示优先。
        assert_eq!(
            detect_extension(&flac, Some("audio/mpeg"), None).as_deref(),
            Some("mp3")
        );
        // 裸扩展名提示也认。
        assert_eq!(
            detect_extension(&flac, Some("m4a"), None).as_deref(),
            Some("m4a")
        );
        // 提示不可信 → Content-Type。
        assert_eq!(
            detect_extension(
                &flac,
                Some("application/octet-stream"),
                Some("audio/x-flac")
            )
            .as_deref(),
            Some("flac")
        );
        // 都不可信 → 魔数。
        assert_eq!(
            detect_extension(&flac, None, Some("binary/unknown")).as_deref(),
            Some("flac")
        );

        let mp3 = dir.join("b.bin");
        fs::write(&mp3, b"ID3\x04\x00\x00\x00\x00\x00\x00").unwrap();
        assert_eq!(detect_extension(&mp3, None, None).as_deref(), Some("mp3"));

        let m4a = dir.join("c.bin");
        fs::write(&m4a, b"\0\0\0\x20ftypM4A \0\0\0\0").unwrap();
        assert_eq!(detect_extension(&m4a, None, None).as_deref(), Some("m4a"));

        let ogg = dir.join("d.bin");
        fs::write(&ogg, b"OggS\0\x02\0\0\0\0\0\0").unwrap();
        assert_eq!(detect_extension(&ogg, None, None).as_deref(), Some("ogg"));

        let wav = dir.join("e.bin");
        fs::write(&wav, b"RIFF\x24\0\0\0WAVEfmt ").unwrap();
        assert_eq!(detect_extension(&wav, None, None).as_deref(), Some("wav"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tag_round_trip_writes_title_artists_album_cover_and_lyrics() {
        use lofty::file::TaggedFileExt;
        use lofty::picture::PictureType;
        use lofty::tag::{Accessor, ItemKey};

        let dir = tmp_dir("tag");
        let temp = dir.join("x.part");
        fs::write(&temp, minimal_flac()).unwrap();
        let cover = dir.join("cover.jpg");
        fs::write(&cover, tiny_jpeg()).unwrap();
        let final_path = dir.join("out.flac");

        let out = finalize(
            &temp,
            &final_path,
            Some(&AudioTags {
                title: "一路向北".into(),
                artists: vec!["周杰伦".into(), "方文山".into()],
                album: Some("叶惠美".into()),
                album_artist: Some("周杰伦".into()),
                track_no: Some(7),
                disc_no: Some(1),
                year: Some(2003),
                cover_path: Some(cover.display().to_string()),
                lyrics: Some("[00:01.00]窗外的麻雀\n".into()),
            }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(out.warning, "", "标签应写入成功");
        assert_eq!(out.actual_extension, "flac");

        let read = lofty::read_from_path(&final_path).unwrap();
        let tag = read.primary_tag().expect("应有主标签");
        assert_eq!(tag.title().as_deref(), Some("一路向北"));
        assert_eq!(tag.album().as_deref(), Some("叶惠美"));
        assert_eq!(tag.track(), Some(7));
        let artists: Vec<&str> = tag.get_strings(ItemKey::TrackArtist).collect();
        assert_eq!(artists, vec!["周杰伦", "方文山"]);
        assert_eq!(
            tag.get_string(ItemKey::Lyrics),
            Some("[00:01.00]窗外的麻雀\n")
        );
        let pic = tag
            .get_picture_type(PictureType::CoverFront)
            .expect("应有封面");
        assert_eq!(pic.data(), tiny_jpeg().as_slice());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tag_failure_still_lands_the_audio_file() {
        // 非音频内容 → lofty 探测失败 → 只记 warning，文件照样落到成品目录。
        let dir = tmp_dir("tagfail");
        let temp = dir.join("x.part");
        fs::write(&temp, b"not an audio file at all").unwrap();
        let final_path = dir.join("out.mp3");
        let out = finalize(
            &temp,
            &final_path,
            Some(&AudioTags {
                title: "x".into(),
                ..Default::default()
            }),
            None,
            None,
        )
        .unwrap();
        assert!(out.warning.contains("标签写入失败"), "{}", out.warning);
        assert!(final_path.is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancel_registry_targets_only_the_named_task() {
        let a = register("task-a");
        let b = register("task-b");
        cancel("task-a");
        assert!(a.is_cancelled());
        assert!(!b.is_cancelled());
        cancel_all();
        assert!(b.is_cancelled());
        unregister("task-a");
        unregister("task-b");
    }
}
