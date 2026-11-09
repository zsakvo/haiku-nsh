//! Haiku 的音频播放 FRB 暴露面。
//!
//! 这是 [`crate::player`]（纯 Rust 流式播放引擎：symphonia 解码 + rodio 输出）之上的薄壳：
//! 把「给定 URL + headers 就播放」的能力压成 FRB 友好的接口。**不理解音乐语义**——
//! `getPlayback` 取地址 / `getLyrics` 取文本仍由 Dart 侧通过插件 host 完成，这里只负责
//! 解码播放音频字节，以及把 LRC/KRC 歌词文本解析成带（逐字）时间轴的结构供 Dart 渲染。
//!
//! 自带一个独立的多线程 tokio 运行时承载下载/解码任务，与插件 host 的运行时互不干扰。

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use std::time::Duration;

use tokio::sync::mpsc::UnboundedReceiver;

use haiku_model::events::EventSink;

use crate::effects::{self, EffectsSettings};
use crate::loudness::{self, LoudnessReport, LoudnessSettings};
use crate::media_cache;
use crate::player::{
    self, PlayerEvent, PlayerFailure, PlayerFailureKind as EngineFailureKind, PlayerHandle,
};

/// 播放事件类型。无字段枚举，FRB 生成普通 Dart enum（不依赖 freezed）。
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackEventKind {
    /// 位置上报（约每 200ms 一次；暂停时位置停止增长）。
    Position,
    /// 缓冲进度上报（0..=1；与上次差 ≥ 0.01 或到 1.0 才推，去重）。
    Buffer,
    /// PCM 环耗尽，当前流正在等待更多可播放数据。
    BufferingStarted,
    /// 原流重新产出 PCM，短暂缓冲已自行恢复。
    BufferingEnded,
    /// 音频输出流被系统断开（AAudio disconnect 等），引擎正在新设备上重开。
    /// **此刻位置是停住的**（音频回调已不再被调用），Dart 侧据此转「缓冲中」并让
    /// 媒体会话停止位置插值，否则通知栏会自己把进度编到曲终。
    OutputLost,
    /// 输出已重开，播放位置连续。
    OutputRestored,
    /// 当前曲目自然播放结束。
    Finished,
    /// 已预取曲目在音频引擎内完成交接；message 为曲目缓存 key。
    TrackChanged,
    /// 播放出错（音频设备 / 解码 / 网络）。
    Error,
    /// 响度测量结果（约每 15 秒音频一次 + 曲终一次）。Dart 落盘，下次播这首歌直接用。
    Loudness,
}

/// 失败分类。非 Error 事件使用 None。
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackFailureKind {
    None,
    Network,
    Decode,
    Output,
}

/// 推给 Dart 的播放事件流元素。用「类型 + 载荷字段」而非带数据枚举，避免 FRB 走 freezed。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PlaybackEvent {
    pub kind: PlaybackEventKind,
    /// `Position` 时为播放秒数；其余为 0。
    pub position: f64,
    /// `Buffer` 时为缓冲进度（0..=1）；其余为 0。
    pub buffered: f64,
    /// `Error` 时为错误信息；`Loudness` 时为曲目标识（`pluginId\x1ftrackId`）；其余为空串。
    pub message: String,
    /// `Error` 时标记可恢复的网络错误或不可恢复的解码/输出错误。
    pub failure_kind: PlaybackFailureKind,
    /// `Loudness` 时为整合响度（LUFS）；其余为 0。
    pub loudness_lufs: f64,
    /// `Loudness` 时为采样峰值（线性，1.0 = 0 dBFS）；其余为 0。
    pub loudness_peak: f64,
    /// `Loudness` 时为已测音频秒数（Dart 据此与时长比对判断可信度）；其余为 0。
    pub loudness_secs: f64,
    /// `Loudness` 时表示已测完整首（解码抵达流末尾）；其余为 false。
    pub loudness_complete: bool,
}

impl PlaybackEvent {
    fn position(secs: f64) -> Self {
        PlaybackEvent {
            kind: PlaybackEventKind::Position,
            position: secs,
            buffered: 0.0,
            message: String::new(),
            failure_kind: PlaybackFailureKind::None,
            loudness_lufs: 0.0,
            loudness_peak: 0.0,
            loudness_secs: 0.0,
            loudness_complete: false,
        }
    }
    fn buffer(fraction: f64) -> Self {
        PlaybackEvent {
            kind: PlaybackEventKind::Buffer,
            position: 0.0,
            buffered: fraction,
            message: String::new(),
            failure_kind: PlaybackFailureKind::None,
            loudness_lufs: 0.0,
            loudness_peak: 0.0,
            loudness_secs: 0.0,
            loudness_complete: false,
        }
    }
    fn buffering(started: bool) -> Self {
        PlaybackEvent {
            kind: if started {
                PlaybackEventKind::BufferingStarted
            } else {
                PlaybackEventKind::BufferingEnded
            },
            position: 0.0,
            buffered: 0.0,
            message: String::new(),
            failure_kind: PlaybackFailureKind::None,
            loudness_lufs: 0.0,
            loudness_peak: 0.0,
            loudness_secs: 0.0,
            loudness_complete: false,
        }
    }
    fn finished() -> Self {
        PlaybackEvent {
            kind: PlaybackEventKind::Finished,
            position: 0.0,
            buffered: 0.0,
            message: String::new(),
            failure_kind: PlaybackFailureKind::None,
            loudness_lufs: 0.0,
            loudness_peak: 0.0,
            loudness_secs: 0.0,
            loudness_complete: false,
        }
    }
    fn track_changed(key: String) -> Self {
        PlaybackEvent {
            kind: PlaybackEventKind::TrackChanged,
            position: 0.0,
            buffered: 0.0,
            message: key,
            failure_kind: PlaybackFailureKind::None,
            loudness_lufs: 0.0,
            loudness_peak: 0.0,
            loudness_secs: 0.0,
            loudness_complete: false,
        }
    }
    fn output_lost(message: String) -> Self {
        PlaybackEvent {
            kind: PlaybackEventKind::OutputLost,
            position: 0.0,
            buffered: 0.0,
            message,
            failure_kind: PlaybackFailureKind::None,
            loudness_lufs: 0.0,
            loudness_peak: 0.0,
            loudness_secs: 0.0,
            loudness_complete: false,
        }
    }
    fn output_restored() -> Self {
        PlaybackEvent {
            kind: PlaybackEventKind::OutputRestored,
            position: 0.0,
            buffered: 0.0,
            message: String::new(),
            failure_kind: PlaybackFailureKind::None,
            loudness_lufs: 0.0,
            loudness_peak: 0.0,
            loudness_secs: 0.0,
            loudness_complete: false,
        }
    }
    fn error(failure: PlayerFailure) -> Self {
        PlaybackEvent {
            kind: PlaybackEventKind::Error,
            position: 0.0,
            buffered: 0.0,
            message: failure.message,
            failure_kind: match failure.kind {
                EngineFailureKind::Network => PlaybackFailureKind::Network,
                EngineFailureKind::Decode => PlaybackFailureKind::Decode,
                EngineFailureKind::Output => PlaybackFailureKind::Output,
            },
            loudness_lufs: 0.0,
            loudness_peak: 0.0,
            loudness_secs: 0.0,
            loudness_complete: false,
        }
    }
    fn loudness(key: String, report: LoudnessReport) -> Self {
        PlaybackEvent {
            kind: PlaybackEventKind::Loudness,
            position: 0.0,
            buffered: 0.0,
            message: key,
            failure_kind: PlaybackFailureKind::None,
            loudness_lufs: report.lufs,
            loudness_peak: report.peak,
            loudness_secs: report.measured_secs,
            loudness_complete: report.complete,
        }
    }
}

/// 响度均衡参数（每次 `play` / `play_local` 传入，见 [`crate::loudness`]）。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct LoudnessOptions {
    /// 总开关。关闭时增益恒 1.0 且不测量（零开销）。
    pub enabled: bool,
    /// 目标响度（LUFS）。取 [`loudness_default_target_lufs`]。
    pub target_lufs: f64,
    /// 上次测量算出的增益（dB）。`None` = 这首歌还没测过，开播即用不上。
    pub known_gain_db: Option<f64>,
    /// 本次是否测量。已有高覆盖率记录时传 `false` 省掉重复测量；
    /// 传 `true` 时同时启用自适应（即便已有 `known_gain_db` 作起点也会随测量修正）。
    pub measure: bool,
    /// 曲目标识（`pluginId\x1ftrackId`）。测量结果按此 key 回传，Dart 据此落盘。
    pub track_key: String,
}

impl LoudnessOptions {
    fn split(self) -> (LoudnessSettings, String) {
        (
            LoudnessSettings {
                enabled: self.enabled,
                target_lufs: self.target_lufs,
                known_gain_db: self.known_gain_db,
                measure: self.measure,
            },
            self.track_key,
        )
    }
}

/// 播放音效参数（**全局，非每曲**；见 `specs/features/audio-effects.md`）。
///
/// 与 [`LoudnessOptions`] 的关键区别：响度均衡随每次 `play` 传入、作用于当前曲目，
/// 音效则是一份常驻的用户偏好，设一次一直生效（切歌不复位）。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct AudioEffectsOptions {
    /// 总开关。关闭时整条 DSP 链走零开销旁路（输出与输入逐位相同）。
    pub enabled: bool,
    /// 各段增益（dB），长度应为 [`audio_effects_band_frequencies`] 的长度。
    /// 长度不符时按位截断 / 缺失补 0，不报错——设置文件跨版本升级要能容错。
    pub band_gains_db: Vec<f32>,
    /// 自动前置增益：由各段增益推出 `preamp_db`，把 EQ 提升带来的整体电平压回安全区。
    pub auto_preamp: bool,
    /// 手动前置增益（dB）。`auto_preamp` 为真时忽略本字段。
    pub preamp_db: f32,
    /// 0..2，1 = 原样。
    pub stereo_width: f32,
    /// -1..1，0 = 居中。只衰减不提升，故不引入削波风险。
    pub balance: f32,
    /// 单声道下混（等价于 `stereo_width = 0`）。开启时覆盖 `stereo_width`。
    pub mono: bool,
}

impl AudioEffectsOptions {
    /// 折算成引擎侧参数。长度容错、`mono` 折算、clamp 都在这里做完，
    /// 引擎拿到的就是一份已规整的配置（`EffectsSettings::resolved` 再兜一次底）。
    fn into_settings(self) -> EffectsSettings {
        let mut band_gains_db = [0.0f32; effects::BAND_COUNT];
        for (slot, value) in band_gains_db.iter_mut().zip(self.band_gains_db.iter()) {
            *slot = *value;
        }
        EffectsSettings {
            enabled: self.enabled,
            band_gains_db,
            auto_preamp: self.auto_preamp,
            preamp_db: self.preamp_db,
            stereo_width: if self.mono { 0.0 } else { self.stereo_width },
            balance: self.balance,
        }
        .resolved()
    }
}

/// 均衡器的固定频点（Hz）。**单一事实来源在 Rust**，Dart 侧不要硬编码
/// （与 [`loudness_default_target_lufs`] 同一思路）。
pub fn audio_effects_band_frequencies() -> Vec<f32> {
    effects::BAND_FREQS.to_vec()
}

/// 单段增益的可调范围（dB）：`(下限, 上限)`。
pub fn audio_effects_band_gain_range() -> (f32, f32) {
    (effects::BAND_GAIN_MIN_DB, effects::BAND_GAIN_MAX_DB)
}

/// 响度测量上报节流：每多测这么多秒音频推一次事件（曲终另推一次）。
/// 这样中途切歌也只丢失最后不到这个数的覆盖量，而不必把落盘绑到播放生命周期上。
const LOUDNESS_REPORT_STEP_SECS: f64 = 15.0;

/// 目标响度默认值（LUFS）。Dart 用它算已知增益，保证与 Rust 内的自适应同一基准。
pub fn loudness_default_target_lufs() -> f64 {
    loudness::DEFAULT_TARGET_LUFS
}

/// 由测量结果换算每曲增益（dB）。含防削波封顶与上下限，`None` = 测量不可用。
///
/// 与 Rust 内自适应共用同一实现，避免 Dart 侧另写一份算法而与引擎跑偏。
pub fn loudness_gain_db(lufs: f64, peak: f64, target_lufs: f64) -> Option<f64> {
    loudness::gain_db(lufs, peak, target_lufs)
}

/// 媒体缓存条目状态（FRB 友好扁平结构）。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MediaCacheStatus {
    /// `none` | `partial` | `complete`
    pub kind: String,
    /// 本地路径（partial/complete）；none 时为空。
    pub path: String,
    pub size: u64,
    /// partial 时可能有；未知为 0。
    pub total_len: u64,
}

/// 媒体缓存用量快照。
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MediaCacheUsage {
    pub used_bytes: u64,
    pub max_bytes: u64,
    pub entry_count: u64,
}

/// 设置媒体缓存根目录（`<AppSupport>/haiku_media_cache`），加载索引。
pub fn media_cache_set_root(path: String) -> Result<(), String> {
    media_cache::set_root(path)
}

/// 启用/关闭媒体缓存（关时 lookup/写盘均 no-op）。
pub fn media_cache_set_enabled(enabled: bool) {
    media_cache::set_enabled(enabled);
}

/// 设置最大缓存字节数（会 clamp 到 0.5–20 GiB 并触发 LRU）。
pub fn media_cache_set_max_bytes(max_bytes: u64) {
    media_cache::set_max_bytes(max_bytes);
}

/// 查询缓存状态。
pub fn media_cache_status(key: String) -> MediaCacheStatus {
    match media_cache::status(&key) {
        media_cache::CacheStatus::None => MediaCacheStatus {
            kind: "none".into(),
            path: String::new(),
            size: 0,
            total_len: 0,
        },
        media_cache::CacheStatus::Partial {
            path,
            size,
            total_len,
        } => MediaCacheStatus {
            kind: "partial".into(),
            path: path.display().to_string(),
            size,
            total_len: total_len.unwrap_or(0),
        },
        media_cache::CacheStatus::Complete { path, size } => MediaCacheStatus {
            kind: "complete".into(),
            path: path.display().to_string(),
            size,
            total_len: size,
        },
    }
}

/// 当前用量。
pub fn media_cache_usage() -> MediaCacheUsage {
    let u = media_cache::usage();
    MediaCacheUsage {
        used_bytes: u.used_bytes,
        max_bytes: u.max_bytes,
        entry_count: u.entry_count,
    }
}

/// 清空所有缓存文件。
pub fn media_cache_clear() -> Result<(), String> {
    media_cache::clear()
}

/// 缓存根路径（未 set_root 时为空串）。
pub fn media_cache_root() -> String {
    media_cache::root_path().unwrap_or_default()
}

/// 标记完整命中访问（更新 LRU）。
pub fn media_cache_touch(key: String) {
    media_cache::touch(&key);
}

/// 固定当前播放 key（淘汰时跳过）；空串清除。
pub fn media_cache_set_pinned(key: String) {
    if key.is_empty() {
        media_cache::set_current_pinned(None);
    } else {
        media_cache::set_current_pinned(Some(key));
    }
}

/// 删除损坏/失败条目。
pub fn media_cache_remove(key: String) {
    media_cache::remove(&key);
}

/// 一个音频播放器实例（持有 rodio 输出线程 + 流式解码运行时）。FRB opaque。
pub struct Player {
    rt: std::mem::ManuallyDrop<tokio::runtime::Runtime>,
    request_epoch: AtomicU64,
    request_generation: AtomicU64,
    request_lock: Mutex<()>,
    prepare_epoch: AtomicU64,
    handle: PlayerHandle,
    client: reqwest::Client,
    /// 播放线程的事件接收端，等 [`Player::events`] 取走转发给 Dart。
    events_rx: Mutex<Option<UnboundedReceiver<player::SessionEvent>>>,
}

impl Player {
    /// Coherent audio-thread counters for the core's Rust-only display clock.
    /// Reading these does not allocate or publish a JS event.
    pub fn progress(&self) -> (u64, f64, f64, bool) {
        let _snapshot = self.handle.snapshot.lock().unwrap();
        (self.handle.generation(), self.handle.position_secs(),
            self.handle.buffered_fraction(), self.handle.is_buffering())
    }

    /// 创建播放器：起 rodio 输出线程与流式解码运行时。
    pub fn open() -> Result<Player, String> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| format!("创建播放运行时失败: {e}"))?;

        let (ev_tx, ev_rx) = tokio::sync::mpsc::unbounded_channel();
        let handle = player::spawn(ev_tx);

        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(8))
            // 每次「读到一段响应体」之间的上限（成功读取即重置）。没有它，连接被中间设备
            // 静默挂起时既不报错也不给数据，下载层的断点重连永远不会被触发，只能干等到
            // PCM 环耗尽——这正是弱网下「卡住不动」的来源。
            .read_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| format!("创建 http client 失败: {e}"))?;

        Ok(Player {
            rt: std::mem::ManuallyDrop::new(rt),
            request_epoch: AtomicU64::new(0),
            request_generation: AtomicU64::new(0),
            request_lock: Mutex::new(()),
            prepare_epoch: AtomicU64::new(0),
            handle,
            client,
            events_rx: Mutex::new(Some(ev_rx)),
        })
    }

    /// 订阅播放事件流（位置 / 结束 / 错误）。Dart 侧得到 `Stream<PlaybackEvent>`。
    /// 仅应订阅一次；重复订阅返回的流不再产出事件。
    pub fn events(&self, sink: EventSink<PlaybackEvent>) {
        let Some(mut rx) = self.events_rx.lock().unwrap().take() else {
            return;
        };
        let poll = self.handle.clone();
        self.rt.spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(200));
            // 位置去重：暂停/空闲时 position 不变，不再推事件——否则 Dart 侧全程 5Hz
            // 收事件、扇出重建。seek/播放中位置必然变化，事件照常。
            let mut last_sent = f64::NAN;
            // 缓冲去重：与上次差 ≥ 0.01 或首帧或跨越 1.0 才推，避免下载期间常驻扇出。
            let mut last_buf = f64::NAN;
            let mut last_buffering = false;
            // 响度上报去重：每首歌每测够 LOUDNESS_REPORT_STEP_SECS 推一次，曲终（已测完整首）
            // 再推一次。中途切歌时最后一次部分测量已经推过，Dart 按覆盖率决定要不要采用。
            let mut loudness_key = String::new();
            let mut loudness_sent_secs = 0.0f64;
            let mut loudness_sent_complete = false;
            loop {
                tokio::select! {
                    _ = tick.tick() => {
                        let _snapshot_guard = poll.snapshot.lock().unwrap();
                        let generation = poll.generation();
                        let sink = sink.with_generation(generation);
                        let pos = poll.position_secs();
                        if (pos - last_sent).abs() >= 0.001 || last_sent.is_nan() {
                            last_sent = pos;
                            if sink.add(PlaybackEvent::position(pos)).is_err() {
                                break; // Dart 端已关闭流
                            }
                        }
                        let buf = poll.buffered_fraction();
                        let reached_full = buf >= 1.0 && last_buf < 1.0;
                        if last_buf.is_nan() || (buf - last_buf).abs() >= 0.01 || reached_full {
                            last_buf = buf;
                            if sink.add(PlaybackEvent::buffer(buf)).is_err() {
                                break; // Dart 端已关闭流
                            }
                        }
                        let buffering = poll.is_buffering();
                        if buffering != last_buffering {
                            last_buffering = buffering;
                            if sink.add(PlaybackEvent::buffering(buffering)).is_err() {
                                break;
                            }
                        }
                        if let Some((key, report)) = poll.loudness_report() {
                            if key != loudness_key {
                                loudness_key = key.clone();
                                loudness_sent_secs = 0.0;
                                loudness_sent_complete = false;
                            }
                            let grew = report.measured_secs - loudness_sent_secs
                                >= LOUDNESS_REPORT_STEP_SECS;
                            let finished = report.complete && !loudness_sent_complete;
                            if !key.is_empty() && (grew || finished) {
                                loudness_sent_secs = report.measured_secs;
                                loudness_sent_complete = report.complete;
                                if sink.add(PlaybackEvent::loudness(key, report)).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    ev = rx.recv() => {
                        let generation = ev.as_ref().map(|e|e.generation).unwrap_or(0);
                        let sink = sink.with_generation(generation);
                        match ev.map(|e|e.event) {
                        Some(PlayerEvent::Finished) => {
                            let _ = sink.add(PlaybackEvent::finished());
                        }
                        Some(PlayerEvent::TrackChanged(key)) => {
                            let _ = sink.add(PlaybackEvent::track_changed(key));
                        }
                        Some(PlayerEvent::Error(failure)) => {
                            let _ = sink.add(PlaybackEvent::error(failure));
                        }
                        Some(PlayerEvent::OutputLost(message)) => {
                            let _ = sink.add(PlaybackEvent::output_lost(message));
                        }
                        Some(PlayerEvent::OutputRestored) => {
                            let _ = sink.add(PlaybackEvent::output_restored());
                        }
                        None => break, // 播放线程已退出
                    }},
                }
            }
        });
    }

    /// 播放给定地址（替换当前播放）。`headers_json` 为 `{ "Referer": "...", ... }` 字符串。
    /// `cache_key` 非空且缓存启用时边下边写本地；完整后入库。
    /// `start_secs` 在解码首包前定位，用于网络恢复时让声音与界面从同一位置继续。
    /// 边下边解码：仅等首包就绪即返回，整首在播放期间流式解码。
    pub async fn play(
        &self,
        url: String,
        headers_json: String,
        cache_key: Option<String>,
        start_secs: f64,
        loudness: LoudnessOptions,
        generation: u64,
    ) -> Result<(), String> {
        let epoch = {
            let _request_guard = self.request_lock.lock().unwrap();
            let newest = self.request_generation.load(Ordering::Acquire);
            if generation < newest {
                return Err("Superseded playback generation".into());
            }
            self.request_generation.store(generation, Ordering::Release);
            let epoch = self.request_epoch.fetch_add(1, Ordering::AcqRel) + 1;
            self.prepare_epoch.fetch_add(1, Ordering::AcqRel);
            self.handle.clear_next();
            epoch
        };
        let (loudness_settings, loudness_key) = loudness.split();
        let headers = parse_headers(&headers_json);
        if let Some(ref k) = cache_key {
            media_cache::set_pinned(Some(k.clone()), None);
        } else {
            media_cache::set_pinned(None, None);
        }
        let mut stream = player::open_stream(
            self.client.clone(),
            url,
            headers,
            cache_key,
            start_secs.max(0.0),
            loudness_settings,
            loudness_key,
        )
        .await
        .map_err(|e| e.to_string())?;
        let _request_guard = self.request_lock.lock().unwrap();
        if self.request_epoch.load(Ordering::Acquire) != epoch {
            return Err("Superseded playback request".into());
        }
        stream.generation = generation;
        self.handle.play(stream);
        Ok(())
    }

    /// 播放本地完整缓存文件。
    pub async fn play_local(
        &self,
        path: String,
        cache_key: Option<String>,
        loudness: LoudnessOptions,
        generation: u64,
    ) -> Result<(), String> {
        let epoch = {
            let _request_guard = self.request_lock.lock().unwrap();
            let newest = self.request_generation.load(Ordering::Acquire);
            if generation < newest {
                return Err("Superseded playback generation".into());
            }
            self.request_generation.store(generation, Ordering::Release);
            let epoch = self.request_epoch.fetch_add(1, Ordering::AcqRel) + 1;
            self.prepare_epoch.fetch_add(1, Ordering::AcqRel);
            self.handle.clear_next();
            epoch
        };
        let (loudness_settings, loudness_key) = loudness.split();
        if let Some(ref k) = cache_key {
            media_cache::set_pinned(Some(k.clone()), None);
            media_cache::touch(k);
        } else {
            media_cache::set_pinned(None, None);
        }
        let mut stream = player::open_local(path, loudness_settings, loudness_key)
            .await
            .map_err(|e| e.to_string())?;
        let _request_guard = self.request_lock.lock().unwrap();
        if self.request_epoch.load(Ordering::Acquire) != epoch {
            return Err("Superseded playback request".into());
        }
        stream.generation = generation;
        self.handle.play(stream);
        Ok(())
    }

    /// 提前打开并解码下一首，但不立即出声。
    pub async fn prepare_next(
        &self,
        url: String,
        headers_json: String,
        cache_key: Option<String>,
        loudness: LoudnessOptions,
        generation: u64,
    ) -> Result<(), String> {
        let epoch = self.prepare_epoch.fetch_add(1, Ordering::AcqRel) + 1;
        let play_epoch = self.request_epoch.load(Ordering::Acquire);
        let (loudness_settings, loudness_key) = loudness.split();
        let headers = parse_headers(&headers_json);
        media_cache::set_next_pinned(cache_key.clone());
        let mut stream = player::open_stream(
            self.client.clone(),
            url,
            headers,
            cache_key,
            0.0,
            loudness_settings,
            loudness_key,
        )
        .await
        .map_err(|e| e.to_string())?;
        let _request_guard = self.request_lock.lock().unwrap();
        if self.prepare_epoch.load(Ordering::Acquire) != epoch
            || self.request_epoch.load(Ordering::Acquire) != play_epoch
        {
            return Err("Superseded playback request".into());
        }
        stream.generation = generation;
        self.handle.prepare_next(stream);
        Ok(())
    }

    /// 提前打开本地下一首。
    pub async fn prepare_next_local(
        &self,
        path: String,
        cache_key: Option<String>,
        loudness: LoudnessOptions,
        generation: u64,
    ) -> Result<(), String> {
        let epoch = self.prepare_epoch.fetch_add(1, Ordering::AcqRel) + 1;
        let play_epoch = self.request_epoch.load(Ordering::Acquire);
        let (loudness_settings, loudness_key) = loudness.split();
        media_cache::set_next_pinned(cache_key.clone());
        if let Some(ref key) = cache_key {
            media_cache::touch(key);
        }
        let mut stream = player::open_local(path, loudness_settings, loudness_key)
            .await
            .map_err(|e| e.to_string())?;
        let _request_guard = self.request_lock.lock().unwrap();
        if self.prepare_epoch.load(Ordering::Acquire) != epoch
            || self.request_epoch.load(Ordering::Acquire) != play_epoch
        {
            return Err("Superseded playback request".into());
        }
        stream.generation = generation;
        self.handle.prepare_next(stream);
        Ok(())
    }

    pub fn clear_next(&self) {
        let _request_guard = self.request_lock.lock().unwrap();
        self.prepare_epoch.fetch_add(1, Ordering::AcqRel);
        media_cache::set_next_pinned(None);
        self.handle.clear_next();
    }

    pub fn skip_to_next(&self) -> bool {
        self.handle.skip_to_next()
    }

    pub fn pause(&self) {
        self.handle.pause();
    }
    pub fn resume(&self) {
        self.handle.resume();
    }
    pub fn stop(&self) {
        let _request_guard = self.request_lock.lock().unwrap();
        self.request_epoch.fetch_add(1, Ordering::AcqRel);
        self.prepare_epoch.fetch_add(1, Ordering::AcqRel);
        media_cache::set_pinned(None, None);
        self.handle.stop();
    }
    /// 跳转到 `secs` 秒处。
    pub fn seek(&self, secs: f64) {
        self.handle.seek(secs);
    }
    /// 音量 0.0..=1.0。
    pub fn set_volume(&self, volume: f32) {
        self.handle.set_volume(volume);
    }
    /// 中途改响度均衡设置（用户在设置里拨开关）：当前曲目的增益按斜坡过渡到新值，
    /// 不会有阶跃爆音。`track_key` 在此无用（测量归属在开播时已定），传空串即可。
    ///
    /// 注意本曲的「测不测量」在开播时就定了：关着开播的曲目中途打开不会补测，
    /// 但若 Dart 手里已有该曲记录，`known_gain_db` 会立刻生效。
    pub fn set_loudness(&self, loudness: LoudnessOptions) {
        let (settings, _) = loudness.split();
        self.handle.set_loudness(settings);
    }
    /// 设置播放音效（均衡器 / 立体声宽度 / 声道平衡）。**全局生效，切歌不复位**。
    ///
    /// 各参数在引擎里按斜坡过渡（每段增益 24 dB/s，宽度/平衡 4/s），因此拖滑块途中
    /// 可以高频调用，不会有阶跃爆音；关闭时整条链走零开销旁路。
    pub fn set_audio_effects(&self, effects: AudioEffectsOptions) {
        self.handle.set_audio_effects(effects.into_settings());
    }
    /// 系统媒体输出路由已改变（切扬声器 / 插拔耳机 / 蓝牙切换），在新设备上重开输出。
    ///
    /// Android 的 AAudio 会在路由切换时断开旧流：流对象还活着、`get_pos()` 照走，
    /// 但采样送不到当前设备——表现为进度在动却没声音。cpal 不会自行重建，必须由此
    /// 显式重绑。解码流水线与 PCM 环原样保留，故不重新下载也不重新解码，
    /// 播放位置连续、暂停态保持不变；无活跃曲目时只是丢弃旧输出。
    pub fn rebind_output(&self) {
        self.handle.rebind_output();
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.handle.quit();
        // Runtime may be dropped on an RPC worker; shutdown_background never blocks it.
        unsafe { std::mem::ManuallyDrop::take(&mut self.rt) }.shutdown_background();
    }
}

/// `play` 内部用：解析 headers JSON。坏 JSON 视为无 header（仍尝试裸请求）。
fn parse_headers(json: &str) -> Vec<(String, String)> {
    serde_json::from_str::<HashMap<String, String>>(json)
        .map(|m| m.into_iter().collect())
        .unwrap_or_default()
}
