//! 播放后端：rodio 的 `OutputStream`/`Sink` 不是 `Send`，因此独占一个线程。
//!
//! 内存关键：**边下边解码、有界缓冲 + 压缩前缀落盘**，绝不把整首歌一次性解成 PCM
//! 常驻内存，也不把整段压缩数据常驻内存。一首 4 分钟 44.1kHz 立体声 f32 整解约 85MB；
//! 改为流水线后常驻内存峰值 ≈ 有界 PCM 环(~1.5MB) + 少量按需 Range 离散段（各 ≤512KB）——
//! 整段压缩前缀改写入匿名临时文件（file-backed，可被系统回收），不再计入常驻内存。
//! 三级流水线：
//!
//! 1. [`SharedStream`] — 后台 tokio 任务把 HTTP 字节流**顺序追加落盘**到一个匿名临时
//!    文件（保留整段压缩数据以支持 symphonia 对 m4a `moov` 的 seek，但驻留在磁盘/页缓存
//!    而非进程堆）；读取走 `pread`（定位读，不移动游标）。
//! 2. 解码线程 — symphonia 从 [`StreamMediaSource`] 按 packet 懒解码，推进 [`Ring`]
//!    （有界 PCM 环，满了就背压阻塞，不会预解整首）。所有错误转“流结束”，绝不 panic
//!    （本工程 `panic = "abort"` 无法 catch_unwind）。
//! 3. [`RingSource`] — rodio `Source`，音频回调只从环里 pop 一个采样，不在回调里做解码。
//!
//! **网络抖动在本文件内部消化，绝不上交**（见 `specs/features/network-jitter-resilience.md`）：
//! 连接被掐断由 [`download_task`] 带 `Range` 续传重连；PCM 环枯竭时 [`Ring::pop_chunk`]
//! 立刻返回 [`RingRead::Underrun`] 让消费端吐静音（回调绝不阻塞），播放位置按
//! [`Ring::silence_samples`] 冻结。抖动一旦上交给 Dart，那边只能拆掉整条流水线重建，
//! 而重建只能靠 `start_secs` 重新定位——听感就是「倒回一两秒重播」，且响度状态
//! （[`TrackLoudness`]）跟着重置，增益从 1.0 重新爬一遍，表现为音量反复泵动。
//!
//! 移植自 cjrt-workspace `examples/music-cli/src/player.rs`（流式播放与内存优化文档同源）。

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::effects::{EffectsConfig, EffectsProcessor, EffectsSettings, FADE_SLEEP};
use crate::loudness::{LoudnessMeter, LoudnessReport, LoudnessSettings, TrackLoudness};
use crate::media_cache;
use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use rodio::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rodio::cpal::{self, FromSample, SizedSample};
use rodio::dynamic_mixer::{self, DynamicMixer, DynamicMixerController};
use rodio::source::{SeekError, Source};
use rodio::Sink;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

/// PCM 环形缓冲容量（交错 f32 采样数）。约 400k 采样 ≈ 1.5MB ≈ 立体声 48kHz 下 ~4.2s。
/// 解码线程预读到此上限即背压阻塞，从而限制常驻 PCM 内存。
const RING_CAP_SAMPLES: usize = 400_000;

/// 音频回调每次从环里批量搬运的采样数（立体声 44.1kHz 下 ≈ 46ms）。
/// rodio 逐采样调 `Iterator::next`，若每采样锁一次环 + 唤醒解码线程，44.1kHz 立体声
/// 是每秒 ~88k 次加锁/唤醒，两线程互相唤醒造成上下文切换风暴，音频回调错过 deadline
/// 即断续。故消费端持本地块缓冲，锁频率降为每块一次（~20 次/秒）。
const LOCAL_CHUNK_SAMPLES: usize = 4096;

/// 环枯竭到此时长仍无一个采样 → 判死。**这不是抖动阈值**：网络抖动由下载层的断点重连
/// 吸收（见 [`DOWNLOAD_RETRY_BACKOFF`]），欠载期间消费端吐静音、播放位置冻结、UI 显示
/// 缓冲中，流水线一动不动。走到这里意味着连重连都救不回来（最坏 ~27s：3 次重连各含
/// 8s 连接超时 + 退避），故取 45s——让下载层先把话说完，这里只防流水线彻底卡死。
const UNDERRUN_TIMEOUT: Duration = Duration::from_secs(45);

/// 欠载时一次吐出的静音采样数（立体声 44.1kHz 下 ≈ 12ms）。比 [`LOCAL_CHUNK_SAMPLES`] 短，
/// 让数据一到就能尽快接上；又足够长，使欠载期间的加锁频率保持在百赫兹量级。
const SILENCE_CHUNK_SAMPLES: usize = 1024;

/// 欠载恢复的迟滞：环里重新攒够这么多秒才继续出声。没有它，弱网下每进来一个 packet
/// 就出一小段声，听感是持续的断续「咔咔」，比干脆缓冲一秒再连续播放难听得多。
const RESUME_PREBUFFER_SECS: f64 = 1.0;

/// 已就绪、可交给播放线程的解码流。播放线程按需从 `ring` 造 [`RingSource`]，rodio 懒拉取。
pub struct DecodedStream {
    pub generation: u64,
    pub channels: u16,
    pub sample_rate: u32,
    /// 流在压缩数据层实际从此位置开始解码。播放线程用它校正 rodio 从 0 起算的位置。
    start_secs: f64,
    /// 当前流的共享缓冲句柄，供播放线程每 tick 读缓冲进度。
    shared: Arc<SharedStream>,
    /// PCM 环。播放线程据此按需构造 `RingSource`——输出重绑时换一个新的 source 接到
    /// **同一个环**上，下载与解码完全不受影响。
    ring: Arc<Ring>,
    /// 下载 / 解码流水线的取消守卫。**由播放线程持有**，不再塞进 `RingSource`：
    /// 否则重绑输出时 drop 掉旧 `Sink` 会连带取消整条流水线。
    guard: StreamGuard,
    /// 本曲的响度均衡状态：解码线程写测量、`RingSource` 读增益（见 [`crate::loudness`]）。
    loudness: Arc<TrackLoudness>,
}

/// 主循环 -> 播放线程。
pub enum PlayerCmd {
    /// 播放一段流式音频（替换当前播放）。
    Play(DecodedStream),
    PrepareNext(DecodedStream),
    ClearNext,
    SkipToNext(Sender<bool>),
    Pause,
    Resume,
    Stop,
    /// 跳转到指定播放位置（秒）。媒体中心拖动进度条触发。
    Seek(f64),
    SetVolume(f32),
    /// 中途改响度均衡设置（用户拨设置开关）。作用于当前曲目：增益按斜坡过渡到新值。
    /// 注意测量与否在开播时已定，中途开启只补施已知增益，不补测本曲。
    SetLoudness(LoudnessSettings),
    /// 改播放音效（均衡器 / 立体声 / 平衡）。**全局配置，不随曲目走**——与
    /// [`PlayerCmd::SetLoudness`] 的「只改当前曲目」是两回事。走命令通道而非让句柄
    /// 直接写共享配置，是为了与 Play/Pause 的淡入淡出排在同一条时间线上。
    SetAudioEffects(EffectsSettings),
    /// 系统媒体输出路由已改变：丢弃旧输出流并在新设备上重开，
    /// 解码流水线原样保留（见 [`run`] 中的处理）。
    RebindOutput,
    Quit,
}

/// 输出流的坏消息：由 cpal 的 error 回调（**系统的错误线程**）投给播放线程。
///
/// 回调里绝不做重开——`cpal::Stream` 不是 `Send`，输出必须在播放线程内开关。
/// 回调只做两件事：置 [`OutputSlot::dead`] 标记（同步可见的真相）、投一条本通知
/// （只为把原文带过来，供事件/日志显示）。
enum OutputNotice {
    /// `StreamError::DeviceNotAvailable` —— AAudio/CoreAudio 断开了流，必须重开。
    Lost(String),
    /// 其它后端错误：不一定致命，也可能高频。只记日志，不重开（避免重开风暴）。
    Warning(String),
}

/// 播放线程 -> 主循环。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerFailureKind {
    Network,
    Decode,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerFailure {
    pub kind: PlayerFailureKind,
    pub message: String,
}

impl PlayerFailure {
    fn network(message: impl Into<String>) -> Self {
        Self {
            kind: PlayerFailureKind::Network,
            message: message.into(),
        }
    }

    fn decode(message: impl Into<String>) -> Self {
        Self {
            kind: PlayerFailureKind::Decode,
            message: message.into(),
        }
    }

    fn output(message: impl Into<String>) -> Self {
        Self {
            kind: PlayerFailureKind::Output,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PlayerEvent {
    /// 当前曲目自然播放结束（用于自动下一首）。
    Finished,
    TrackChanged(String),
    Error(PlayerFailure),
    /// 输出流被系统断开，正在重开。**位置此刻是停住的**（音频回调已不再被调用），
    /// Dart 侧据此转「缓冲中」并停掉媒体会话的位置插值，否则通知栏会自己编进度。
    OutputLost(String),
    /// 输出已在新设备上重开，播放位置连续。
    OutputRestored,
}

/// Generation is captured by the engine thread when the event occurs, never by its consumer.
pub struct SessionEvent {
    pub generation: u64,
    pub event: PlayerEvent,
}
struct SessionSender {
    snapshot: Arc<Mutex<()>>,
    tx: UnboundedSender<SessionEvent>,
    generation: Arc<AtomicU64>,
}
impl SessionSender {
    fn send(
        &self,
        event: PlayerEvent,
    ) -> Result<(), tokio::sync::mpsc::error::SendError<SessionEvent>> {
        self.tx.send(SessionEvent {
            generation: self.generation.load(Ordering::Acquire),
            event,
        })
    }
}

/// 播放器句柄：克隆/持有命令发送端 + 当前播放位置（毫秒）。
#[derive(Clone)]
pub struct PlayerHandle {
    generation: Arc<AtomicU64>,
    pub snapshot: Arc<Mutex<()>>,
    tx: Sender<PlayerCmd>,
    /// 当前曲目已播放位置（毫秒），由播放线程每 tick 用 `Sink::get_pos()` 刷新；
    /// Play/Stop 时清零。供歌词同步读取。
    pos_ms: Arc<AtomicU64>,
    /// 当前曲目缓冲进度（0..=1 的 f64，按位存进 `AtomicU64`，模式同 `pos_ms`）。
    /// 播放线程每 tick 用当前流的 `SharedStream::buffered_fraction()` 刷新；Play/Stop 时清零。
    buffered: Arc<AtomicU64>,
    /// 当前音频消费者是否因 PCM 环耗尽而等待生产者。
    buffering: Arc<AtomicBool>,
    /// 当前曲目的响度状态。播放线程在 Play/Stop 时换插；FRB 轮询侧据此读测量快照
    /// 上报给 Dart 落盘。**曲终不清空**——解码线程比 sink 早几秒结束，要留着让最后
    /// 那次「已测完整首」的快照能被轮询取走。
    current_loudness: Arc<Mutex<Option<Arc<TrackLoudness>>>>,
}

impl PlayerHandle {
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
    /// 当前播放位置（秒）。无活跃曲目时为 0。
    pub fn position_secs(&self) -> f64 {
        self.pos_ms.load(Ordering::Relaxed) as f64 / 1000.0
    }

    /// 当前曲目缓冲进度（0..=1）。无活跃曲目时为 0。
    pub fn buffered_fraction(&self) -> f64 {
        f64::from_bits(self.buffered.load(Ordering::Relaxed))
    }

    pub fn is_buffering(&self) -> bool {
        self.buffering.load(Ordering::Relaxed)
    }

    pub fn play(&self, stream: DecodedStream) {
        let _ = self.tx.send(PlayerCmd::Play(stream));
    }
    pub fn prepare_next(&self, stream: DecodedStream) {
        let _ = self.tx.send(PlayerCmd::PrepareNext(stream));
    }
    pub fn clear_next(&self) {
        let _ = self.tx.send(PlayerCmd::ClearNext);
    }
    pub fn skip_to_next(&self) -> bool {
        let (tx, rx) = std::sync::mpsc::channel();
        if self.tx.send(PlayerCmd::SkipToNext(tx)).is_err() {
            return false;
        }
        rx.recv_timeout(Duration::from_secs(1)).unwrap_or(false)
    }
    pub fn pause(&self) {
        let _ = self.tx.send(PlayerCmd::Pause);
    }
    pub fn resume(&self) {
        let _ = self.tx.send(PlayerCmd::Resume);
    }
    pub fn stop(&self) {
        let _ = self.tx.send(PlayerCmd::Stop);
    }
    /// 跳转到 `secs` 秒处。无活跃曲目时为 no-op。
    pub fn seek(&self, secs: f64) {
        let _ = self.tx.send(PlayerCmd::Seek(secs.max(0.0)));
    }
    pub fn set_volume(&self, v: f32) {
        let _ = self.tx.send(PlayerCmd::SetVolume(v));
    }
    /// 改当前曲目的响度均衡设置（下一首由 `play` 时的参数决定）。
    pub fn set_loudness(&self, settings: LoudnessSettings) {
        let _ = self.tx.send(PlayerCmd::SetLoudness(settings));
    }
    /// 改播放音效。全局生效（不随切歌复位），当前曲目上按斜坡过渡，不会有阶跃爆音。
    pub fn set_audio_effects(&self, settings: EffectsSettings) {
        let _ = self.tx.send(PlayerCmd::SetAudioEffects(settings));
    }
    /// 当前曲目的响度测量快照 + 其曲目标识。尚无结果（未开启 / 刚起播 / 全静音）时为 `None`。
    pub fn loudness_report(&self) -> Option<(String, LoudnessReport)> {
        let guard = self.current_loudness.lock().ok()?;
        let loudness = guard.as_ref()?;
        Some((loudness.key().to_string(), loudness.report()?))
    }
    /// 通知播放线程系统输出路由已变，需要在新设备上重开输出。
    pub fn rebind_output(&self) {
        let _ = self.tx.send(PlayerCmd::RebindOutput);
    }
    pub fn quit(&self) {
        let _ = self.tx.send(PlayerCmd::Quit);
    }
}

/// 启动播放线程，返回句柄。`events` 用于回送 Finished/Error。
pub fn spawn(tx_events: UnboundedSender<SessionEvent>) -> PlayerHandle {
    let generation = Arc::new(AtomicU64::new(0));
    let snapshot = Arc::new(Mutex::new(()));
    let events = SessionSender {
        snapshot: snapshot.clone(),
        tx: tx_events,
        generation: generation.clone(),
    };
    let (tx, rx) = std::sync::mpsc::channel::<PlayerCmd>();
    let pos_ms = Arc::new(AtomicU64::new(0));
    let buffered = Arc::new(AtomicU64::new(0));
    let buffering = Arc::new(AtomicBool::new(false));
    let current_loudness: Arc<Mutex<Option<Arc<TrackLoudness>>>> = Arc::new(Mutex::new(None));
    let pos_for_thread = pos_ms.clone();
    let buffered_for_thread = buffered.clone();
    let buffering_for_thread = buffering.clone();
    let loudness_for_thread = current_loudness.clone();
    // 音效配置全局唯一：由播放线程持有，各 `RingSource` 只读共享（见 [`crate::effects`]）。
    let effects = Arc::new(EffectsConfig::new());
    thread::spawn(move || {
        run(
            rx,
            events,
            pos_for_thread,
            buffered_for_thread,
            buffering_for_thread,
            loudness_for_thread,
            effects,
        )
    });
    PlayerHandle {
        snapshot,
        generation,
        tx,
        pos_ms,
        buffered,
        buffering,
        current_loudness,
    }
}

/// 播放线程当前持有的曲目流水线。`guard` 在此结构被 drop 时取消下载与解码——
/// 因此只有 Stop / 切歌 / 退出才会拆流水线，重绑输出不会。
struct CurrentTrack {
    generation: u64,
    shared: Arc<SharedStream>,
    ring: Arc<Ring>,
    channels: u16,
    sample_rate: u32,
    loudness: Arc<TrackLoudness>,
    _guard: StreamGuard,
}

struct PendingNext {
    track: CurrentTrack,
    appended: bool,
}

impl CurrentTrack {
    /// 欠载静音已经占掉的输出时长（毫秒）。`Sink::get_pos()` 数的是本 source 吐出的
    /// 每一个采样，静音也在内——扣掉它，缓冲期间的播放位置才会真正停住。
    fn silence_ms(&self) -> u64 {
        silence_ms(self.ring.silence_samples(), self.sample_rate, self.channels)
    }
}

/// 交错采样数 → 毫秒。速率非法时返回 0（宁可不扣，也不能让位置乱跳）。
fn silence_ms(silence_samples: u64, sample_rate: u32, channels: u16) -> u64 {
    let per_sec = sample_rate as u64 * channels.max(1) as u64;
    if per_sec == 0 {
        return 0;
    }
    silence_samples * 1000 / per_sec
}

/// 输出重开的限流参数：设备真的没了（拔掉声卡、通话独占）时不能空转重试。
const REBIND_MIN_INTERVAL: Duration = Duration::from_millis(500);
const REBIND_WINDOW: Duration = Duration::from_secs(30);
const REBIND_MAX_IN_WINDOW: u32 = 5;

/// 位置停摆看门狗阈值。**这是兜底，不是主方案**（主方案是 [`OutputSlot::dead`] 那条
/// 事件驱动的路），只覆盖「流被断开却不回调 onError」的机型与桌面端设备静默消失。
/// 3s 而非更短：留足 AAudio 冷启动 + 解码首包的余量，也让事件驱动那条路先跑完
/// （正常情况它在一跳（150ms）内就收拾干净了）。
const OUTPUT_STALL_TIMEOUT: Duration = Duration::from_secs(3);

/// 停摆判定：位置**该走却不走**。三个条件都必须成立，缺一个就会误判：
///
/// - 未暂停：暂停时位置本来就不走；
/// - 未欠载：环空时是网络问题（已有 8 秒欠载超时那条路管），不是输出问题；
/// - 位置停走够久：见 [`OUTPUT_STALL_TIMEOUT`]。
fn output_stalled(pos_unchanged_for: Duration, paused: bool, buffering: bool) -> bool {
    !paused && !buffering && pos_unchanged_for >= OUTPUT_STALL_TIMEOUT
}

/// 播放线程持有的音频输出：cpal 流 + rodio 混音器 + 存活标记。
///
/// **为什么不用 `rodio::OutputStream`**：AAudio 在输出设备/配置变化时会**断开**
/// 已有流，并要求应用关掉旧流、开一条新的（oboe 的 `onErrorBeforeClose` 契约）。
/// cpal 忠实转发了这个错误（`host/oboe/output_callback.rs` → `StreamError::DeviceNotAvailable`），
/// 但 `rodio-0.20` 的 `OutputStream` 把 error 回调 `eprintln!` 掉了，也没有注入口
/// （0.22 才有 `with_error_callback`，但升级是全面破坏性改动）。于是自己持有 cpal 流：
/// 只重写「开输出」这一层，`Sink` / `Source` / seek / 音量 / 淡入淡出全部照旧。
///
/// 不实现契约的代价（真实事故）：流被断开后 `Sink::get_pos()` 定住（它数的是已被消费的
/// 采样，不是墙上时钟）、没有声音、PCM 环灌满后解码线程背压阻塞，而 `Play` 又会复用
/// 这条死流 —— 表现为「没声音、进度卡 0、切歌也救不回来，但通知栏进度照走」。
struct OutputSlot {
    /// cpal 流必须活着，drop 即停止回调。本结构只能在播放线程内持有（`Stream` !Send）。
    _stream: cpal::Stream,
    /// rodio 混音器的输入端：`Sink` 从这里接进输出（等价于 `OutputStreamHandle`）。
    mixer: Arc<DynamicMixerController<f32>>,
    /// 由 error 回调置位。**同步可见的真相**：命令通道可能正被一串命令占着，
    /// `Play` / `Resume` 的前置检查必须能直接看到它，不能等通知被处理。
    dead: Arc<AtomicBool>,
}

impl OutputSlot {
    fn is_alive(&self) -> bool {
        !self.dead.load(Ordering::Acquire)
    }
}

/// 打开默认输出设备。**惰性调用**：进程启动时可能还没有可用路由（Android 冷启动、
/// 桌面无声卡），此时不应让播放线程直接死掉，等到真正要出声时再试。
fn open_output(notices: &Sender<OutputNotice>) -> Result<OutputSlot, String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| "无法打开音频设备: 没有默认输出设备".to_string())?;
    let default_config = device
        .default_output_config()
        .map_err(|e| format!("无法打开音频设备: {e}"))?;
    // 默认配置开不起来时逐个试其它受支持的配置——`rodio::OutputStream::try_default`
    // 本来就有这个兜底（`try_new_output_stream_config`），自己接管输出后必须保留，
    // 否则某些设备/驱动上会从「能出声」退化成「打不开设备」。
    let mut last_err = String::new();
    for config in output_config_candidates(&device, default_config) {
        match open_output_with(&device, &config, notices) {
            Ok(slot) => return Ok(slot),
            Err(e) => last_err = e,
        }
    }
    Err(if last_err.is_empty() {
        "无法打开音频设备: 没有可用的输出配置".to_string()
    } else {
        last_err
    })
}

/// 候选输出配置：默认配置优先，其后按 cpal 的默认启发式排序，每个再取
/// 最高 / 44.1kHz / 最低三档采样率（与 rodio 的 `supported_output_formats` 同策略）。
fn output_config_candidates(
    device: &cpal::Device,
    default_config: cpal::SupportedStreamConfig,
) -> Vec<cpal::SupportedStreamConfig> {
    const HZ_44100: cpal::SampleRate = cpal::SampleRate(44_100);
    let mut out = vec![default_config];
    let Ok(supported) = device.supported_output_configs() else {
        return out;
    };
    let mut ranges: Vec<_> = supported.collect();
    ranges.sort_by(|a, b| b.cmp_default_heuristics(a));
    for range in ranges {
        let (min_rate, max_rate) = (range.min_sample_rate(), range.max_sample_rate());
        out.push(range.with_max_sample_rate());
        if HZ_44100 > min_rate && HZ_44100 < max_rate {
            out.push(range.with_sample_rate(HZ_44100));
        }
        out.push(range.with_sample_rate(min_rate));
    }
    out
}

fn open_output_with(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    notices: &Sender<OutputNotice>,
) -> Result<OutputSlot, String> {
    let (mixer, mixer_rx) = dynamic_mixer::mixer::<f32>(config.channels(), config.sample_rate().0);
    let dead = Arc::new(AtomicBool::new(false));

    // ★ AAudio/oboe 契约的实现点：流被断开就必须重开，否则它永远不会再被回调。
    let err_dead = dead.clone();
    let err_tx = notices.clone();
    let error_callback = move |err: cpal::StreamError| match err {
        cpal::StreamError::DeviceNotAvailable => {
            err_dead.store(true, Ordering::Release);
            let _ = err_tx.send(OutputNotice::Lost("音频输出流被系统断开".to_string()));
        }
        other => {
            let _ = err_tx.send(OutputNotice::Warning(other.to_string()));
        }
    };

    let stream = build_output_stream(device, config, mixer_rx, error_callback)
        .map_err(|e| format!("无法打开音频设备: {e}"))?;
    stream.play().map_err(|e| format!("无法启动音频流: {e}"))?;
    Ok(OutputSlot {
        _stream: stream,
        mixer,
        dead,
    })
}

/// 按设备的默认采样格式建流。分派逻辑与 `rodio-0.20` 的 `new_output_stream_with_format`
/// 一致（同一套 `Sample::from_sample` 转换 + 无数据时填 `EQUILIBRIUM` 静音）。
fn build_output_stream<E>(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    mixer_rx: DynamicMixer<f32>,
    error_callback: E,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    E: FnMut(cpal::StreamError) + Send + 'static,
{
    let cfg = config.config();
    match config.sample_format() {
        cpal::SampleFormat::F32 => {
            typed_output_stream::<f32, E>(device, &cfg, mixer_rx, error_callback)
        }
        cpal::SampleFormat::F64 => {
            typed_output_stream::<f64, E>(device, &cfg, mixer_rx, error_callback)
        }
        cpal::SampleFormat::I8 => {
            typed_output_stream::<i8, E>(device, &cfg, mixer_rx, error_callback)
        }
        cpal::SampleFormat::I16 => {
            typed_output_stream::<i16, E>(device, &cfg, mixer_rx, error_callback)
        }
        cpal::SampleFormat::I32 => {
            typed_output_stream::<i32, E>(device, &cfg, mixer_rx, error_callback)
        }
        cpal::SampleFormat::I64 => {
            typed_output_stream::<i64, E>(device, &cfg, mixer_rx, error_callback)
        }
        cpal::SampleFormat::U8 => {
            typed_output_stream::<u8, E>(device, &cfg, mixer_rx, error_callback)
        }
        cpal::SampleFormat::U16 => {
            typed_output_stream::<u16, E>(device, &cfg, mixer_rx, error_callback)
        }
        cpal::SampleFormat::U32 => {
            typed_output_stream::<u32, E>(device, &cfg, mixer_rx, error_callback)
        }
        cpal::SampleFormat::U64 => {
            typed_output_stream::<u64, E>(device, &cfg, mixer_rx, error_callback)
        }
        _ => Err(cpal::BuildStreamError::StreamConfigNotSupported),
    }
}

fn typed_output_stream<T, E>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut mixer_rx: DynamicMixer<f32>,
    error_callback: E,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: SizedSample + FromSample<f32>,
    E: FnMut(cpal::StreamError) + Send + 'static,
{
    device.build_output_stream::<T, _, _>(
        config,
        move |data: &mut [T], _| {
            for d in data.iter_mut() {
                *d = match mixer_rx.next() {
                    Some(sample) => T::from_sample(sample),
                    None => T::EQUILIBRIUM,
                };
            }
        },
        error_callback,
        None,
    )
}

/// 在给定输出上接一条新的播放通道。等价于 `Sink::try_new(&handle)`（后者就是
/// `new_idle` + 把队列加进同一个混音器），但我们直接持有混音器，故不会失败。
fn new_sink(slot: &OutputSlot) -> Sink {
    let (sink, queue) = Sink::new_idle();
    slot.mixer.add(queue);
    sink
}

/// 自动重开输出的限流。**只管自动路径**（流被断开、停摆看门狗）；
/// 用户驱动的 [`PlayerCmd::RebindOutput`]（路由切换）不受限——连着切五次输出设备
/// 是正常操作，限流会把声音掐死。
struct RebindLimiter {
    window_start: Instant,
    count: u32,
    last: Option<Instant>,
}

impl RebindLimiter {
    fn new() -> Self {
        Self {
            window_start: Instant::now(),
            count: 0,
            last: None,
        }
    }

    /// 允许则记账并返回 true。
    fn allow(&mut self, now: Instant) -> bool {
        if let Some(last) = self.last {
            if now.duration_since(last) < REBIND_MIN_INTERVAL {
                return false;
            }
        }
        if now.duration_since(self.window_start) > REBIND_WINDOW {
            self.window_start = now;
            self.count = 0;
        }
        if self.count >= REBIND_MAX_IN_WINDOW {
            return false;
        }
        self.count += 1;
        self.last = Some(now);
        true
    }

    /// 位置正常增长时调用：距**上次自动重开**已稳定播放超过一个窗口 → 归还额度。
    /// 用「上次重开」而不是「窗口起点」作基准：后者在进程刚起来时就已经很旧了，
    /// 会让第一次断开白拿一次额度。
    fn note_progress(&mut self, now: Instant) {
        if let Some(last) = self.last {
            if now.duration_since(last) > REBIND_WINDOW {
                self.reset();
            }
        }
    }

    /// 清零额度：偶发的一次断开不该把额度永久扣掉。
    fn reset(&mut self) {
        self.window_start = Instant::now();
        self.count = 0;
        self.last = None;
    }
}

/// 在环上再造一个消费者。重绑输出时旧 source 已退休（见 [`Ring::retire_sources`]），
/// 新 source 从环的当前位置继续消费——环里已缓冲的 PCM 不会丢。
fn new_source(
    ring: &Arc<Ring>,
    channels: u16,
    sample_rate: u32,
    loudness: &Arc<TrackLoudness>,
    effects: &Arc<EffectsConfig>,
) -> RingSource {
    let samples_per_sec = sample_rate as f64 * channels.max(1) as f64;
    RingSource {
        ring: ring.clone(),
        channels,
        sample_rate,
        local: Vec::with_capacity(LOCAL_CHUNK_SAMPLES),
        local_pos: 0,
        generation: ring.generation.load(Ordering::Acquire),
        source_epoch: ring.current_source_epoch(),
        loudness: loudness.clone(),
        samples_per_sec,
        prebuffer_samples: (samples_per_sec * RESUME_PREBUFFER_SECS) as usize,
        effects_config: effects.clone(),
        // 滤波器状态是本 source 私有的：重绑输出丢掉它无所谓（重新 ring-up + 自带淡入）。
        effects: EffectsProcessor::new(sample_rate, channels),
    }
}

/// 重绑输出：只换设备与 `Sink`，把新的 `RingSource` 接回**同一个环**。
///
/// 路由切换（[`PlayerCmd::RebindOutput`]）与「流被系统断开」共用这一条路：坏的只有输出端，
/// 下载缓冲、PCM 环、解码线程全都完好，所以不重播曲目——环里本就缓冲着约 4 秒 PCM，
/// 切换间隙只有几十毫秒，不重新下载、不重新 probe、不重新解码，播放位置连续。
///
/// **顺序要害，改动时不要动**：必须先 [`Ring::retire_sources`] 再 drop `Sink` / 输出流。
/// 旧 source 可能正阻塞在空环上等 PCM，不先解除，drop 输出时 cpal 收不回音频回调，
/// 直接死锁。（配套的另一条在 [`Ring::pop_chunk`]：退休检查排在取数据之前。）
///
/// 返回是否重开成功。无活跃曲目时丢掉旧输出即算成功（下次 `Play` 会惰性重开）。
#[allow(clippy::too_many_arguments)]
fn rebind_output(
    output: &mut Option<OutputSlot>,
    sink: &mut Option<Sink>,
    active: &mut bool,
    position_base_ms: &mut u64,
    current: Option<&CurrentTrack>,
    paused: bool,
    volume: f32,
    effects: &Arc<EffectsConfig>,
    pos_ms: &Arc<AtomicU64>,
    notices: &Sender<OutputNotice>,
    events: &SessionSender,
) -> bool {
    let Some(cur) = current else {
        // 无活跃曲目：丢掉旧输出即可，下次 Play 会在新设备上惰性重开。
        *sink = None;
        *output = None;
        *active = false;
        return true;
    };
    let resume_ms = pos_ms.load(Ordering::Relaxed);
    // 静音账随旧 Sink 一起作废：`resume_ms` 已经是扣过静音的位置，而新 Sink 的
    // `get_pos()` 从 0 起算，不清零就会把同一段静音扣第二遍（位置倒退）。
    cur.ring.reset_silence();
    cur.ring.retire_sources();
    if let Some(old) = sink.take() {
        old.stop();
    }
    // 显式先 drop 旧输出再开新的：被断开的流必须关掉才能拿到新路由。
    *output = None;

    match open_output(notices) {
        Ok(slot) => {
            let fresh = new_sink(&slot);
            fresh.set_volume(volume);
            fresh.append(new_source(
                &cur.ring,
                cur.channels,
                cur.sample_rate,
                &cur.loudness,
                effects,
            ));
            if paused {
                fresh.pause();
            }
            *output = Some(slot);
            *sink = Some(fresh);
            *active = true;
            // 新 Sink 的 get_pos() 从 0 起算，故把绝对位置整体放进基准。
            *position_base_ms = resume_ms;
            pos_ms.store(resume_ms, Ordering::Relaxed);
            true
        }
        Err(e) => {
            // 重开失败（切换途中设备短暂不可用）：保留流水线，
            // 下一次重绑 / Play / 看门狗还能再试。
            *active = false;
            let _ = events.send(PlayerEvent::Error(PlayerFailure::output(e)));
            false
        }
    }
}

fn run(
    rx: Receiver<PlayerCmd>,
    events: SessionSender,
    pos_ms: Arc<AtomicU64>,
    buffered: Arc<AtomicU64>,
    buffering: Arc<AtomicBool>,
    current_loudness: Arc<Mutex<Option<Arc<TrackLoudness>>>>,
    effects: Arc<EffectsConfig>,
) {
    // 输出流必须在本线程内存活，否则音频会被立即丢弃。惰性创建：见 `open_output`。
    let mut output: Option<OutputSlot> = None;
    let mut sink: Option<Sink> = None;
    let mut volume: f32 = 0.8;
    let mut active = false;
    // 暂停态由本线程记账：重绑输出后要在新 Sink 上把它还原出来。
    let mut paused = false;
    let mut position_base_ms = 0u64;
    // 当前曲目流水线：每 tick 读其缓冲进度写入 `buffered`；Stop/切歌/曲终清空。
    let mut current: Option<CurrentTrack> = None;
    let mut next: Option<PendingNext> = None;

    // 输出流坏消息的收件箱。**不复用命令通道**：那样播放线程就得自己持有一个 `Sender`，
    // 外部句柄全部 drop 后 `recv_timeout` 再也收不到 `Disconnected`，线程永不退出。
    let (notices, notice_rx) = std::sync::mpsc::channel::<OutputNotice>();
    let mut limiter = RebindLimiter::new();
    // 停摆看门狗的基准：位置每变一次就重新计时。
    let mut last_pos_ms = 0u64;
    let mut last_pos_at = Instant::now();

    loop {
        let command = rx.recv_timeout(Duration::from_millis(150));
        let _snapshot_guard = events.snapshot.lock().unwrap();
        match command {
            Ok(PlayerCmd::Play(stream)) => {
                events
                    .generation
                    .store(stream.generation, Ordering::Release);
                // 切歌前先淡出：`Sink::stop` 是在半个波形上直接截断，是整条链路里最响的
                // 一次咔哒。只有已经在出声时才值得等这 12ms——冷启动第一首不要平白多等。
                if sink.is_some() {
                    effects.begin_fade_out();
                    thread::sleep(FADE_SLEEP);
                }
                if let Some(old) = sink.take() {
                    old.stop();
                }
                // 先 drop 旧流水线守卫：取消旧解码线程与下载任务。
                current = None;
                next = None;
                active = false;
                paused = false;
                buffering.store(false, Ordering::Relaxed);
                let DecodedStream {
                    generation,
                    channels,
                    sample_rate,
                    start_secs,
                    shared,
                    ring,
                    guard,
                    loudness,
                } = stream;
                if let Ok(mut slot) = current_loudness.lock() {
                    *slot = Some(loudness.clone());
                }

                // 死流必须先关掉：`OutputSlot` 只有「有没有」不够，还得看「活着没有」。
                // 复用一条被系统断开的流，新 Sink 建在死掉的混音器上——没有声音、
                // 位置永不增长，切歌也救不回来（这正是本 bug 里「进度卡 0」的直接来源）。
                if output.as_ref().is_some_and(|o| !o.is_alive()) {
                    output = None;
                }
                // 惰性打开输出；路由切换后这里拿到的就是新设备。
                if output.is_none() {
                    match open_output(&notices) {
                        Ok(o) => output = Some(o),
                        Err(e) => {
                            let _ = events.send(PlayerEvent::Error(PlayerFailure::output(e)));
                            continue;
                        }
                    }
                }
                let slot = output.as_ref().expect("output just opened");

                // 重新开闸：上面的切歌淡出（或上一次 Pause/Stop）把闸门关到了 0，
                // 新 source 自带 8ms 淡入，这里必须把目标放回 1，否则新曲永远不出声。
                effects.end_fade_out();
                let fresh = new_sink(slot);
                fresh.set_volume(volume);
                ring.set_paused(false);
                fresh.append(new_source(
                    &ring,
                    channels,
                    sample_rate,
                    &loudness,
                    &effects,
                ));
                current = Some(CurrentTrack {
                    generation,
                    shared,
                    ring,
                    channels,
                    sample_rate,
                    loudness,
                    _guard: guard,
                });
                sink = Some(fresh);
                active = true;
                position_base_ms = (start_secs.max(0.0) * 1000.0) as u64;
                pos_ms.store(position_base_ms, Ordering::Relaxed);
                buffered.store(0.0f64.to_bits(), Ordering::Relaxed);
                buffering.store(false, Ordering::Relaxed);
                last_pos_ms = position_base_ms;
                last_pos_at = Instant::now();
            }
            Ok(PlayerCmd::PrepareNext(stream)) => {
                if let Some(old) = next.take() {
                    old.track.ring.retire_sources();
                }
                let DecodedStream {
                    generation,
                    channels,
                    sample_rate,
                    start_secs: _,
                    shared,
                    ring,
                    guard,
                    loudness,
                } = stream;
                ring.set_paused(paused);
                let track = CurrentTrack {
                    generation,
                    shared,
                    ring,
                    channels,
                    sample_rate,
                    loudness,
                    _guard: guard,
                };
                let can_append = current.as_ref().is_some_and(|cur| {
                    cur.channels == track.channels
                        && cur.sample_rate == track.sample_rate
                        && cur.shared.is_complete()
                });
                if can_append {
                    if let Some(s) = &sink {
                        s.append(new_source(
                            &track.ring,
                            track.channels,
                            track.sample_rate,
                            &track.loudness,
                            &effects,
                        ));
                    }
                }
                next = Some(PendingNext {
                    track,
                    appended: can_append,
                });
            }
            Ok(PlayerCmd::ClearNext) => {
                if let Some(old) = next.take() {
                    old.track.ring.retire_sources();
                }
            }
            Ok(PlayerCmd::SkipToNext(reply)) => {
                let advanced = if let Some(pending) = next.take() {
                    if let Some(cur) = &current {
                        cur.ring.retire_sources();
                    }
                    if !pending.appended {
                        if let Some(old) = sink.take() {
                            old.stop();
                        }
                        if let Some(slot) = &output {
                            let fresh = new_sink(slot);
                            fresh.set_volume(volume);
                            fresh.append(new_source(
                                &pending.track.ring,
                                pending.track.channels,
                                pending.track.sample_rate,
                                &pending.track.loudness,
                                &effects,
                            ));
                            if paused {
                                fresh.pause();
                            }
                            sink = Some(fresh);
                        }
                    }
                    let key = pending.track.loudness.key().to_string();
                    events
                        .generation
                        .store(pending.track.generation, Ordering::Release);
                    current = Some(pending.track);
                    media_cache::promote_next_pinned();
                    position_base_ms = 0;
                    pos_ms.store(0, Ordering::Relaxed);
                    buffered.store(0.0f64.to_bits(), Ordering::Relaxed);
                    if let Some(cur) = &current {
                        if let Ok(mut slot) = current_loudness.lock() {
                            *slot = Some(cur.loudness.clone());
                        }
                    }
                    active = true;
                    let _ = events.send(PlayerEvent::TrackChanged(key));
                    true
                } else {
                    false
                };
                let _ = reply.send(advanced);
            }
            Ok(PlayerCmd::Pause) => {
                // 先关闸再暂停：`Sink::pause` 同样是在任意相位上停住，直接停会「啪」一声。
                if sink.is_some() {
                    effects.begin_fade_out();
                    thread::sleep(FADE_SLEEP);
                }
                paused = true;
                if let Some(cur) = &current {
                    cur.ring.set_paused(true);
                }
                if let Some(pending) = &next {
                    pending.track.ring.set_paused(true);
                }
                if let Some(s) = &sink {
                    s.pause();
                }
            }
            Ok(PlayerCmd::Resume) => {
                // 暂停期间输出流可能已经被系统断开——这正是本 bug 最常见的入口：
                // 被别的 App 抢走音频焦点 → 我们暂停 → 对方改了输出配置 → AAudio 断开
                // 我们的流 → 焦点归还后我们对着一条尸体 resume（没声音、位置不动）。
                // 恢复出声前先在新设备上重绑，位置连续。
                if output.as_ref().is_some_and(|o| !o.is_alive()) {
                    let had_track = current.is_some();
                    if had_track {
                        let _ = events.send(PlayerEvent::OutputLost(
                            "音频输出流被系统断开（暂停期间）".to_string(),
                        ));
                    }
                    let ok = rebind_output(
                        &mut output,
                        &mut sink,
                        &mut active,
                        &mut position_base_ms,
                        current.as_ref(),
                        // 这一刻还是暂停态，但下面立刻就要 play()：直接按「不暂停」重绑，
                        // 省掉一次 pause→play 抖动。
                        false,
                        volume,
                        &effects,
                        &pos_ms,
                        &notices,
                        &events,
                    );
                    if ok {
                        if let Some(pending) = &next {
                            if pending.appended {
                                if let Some(s) = &sink {
                                    s.append(new_source(
                                        &pending.track.ring,
                                        pending.track.channels,
                                        pending.track.sample_rate,
                                        &pending.track.loudness,
                                        &effects,
                                    ));
                                }
                            }
                        }
                    }
                    if ok && had_track {
                        let _ = events.send(PlayerEvent::OutputRestored);
                    }
                }
                paused = false;
                if let Some(cur) = &current {
                    cur.ring.set_paused(false);
                }
                if let Some(pending) = &next {
                    pending.track.ring.set_paused(false);
                }
                if let Some(s) = &sink {
                    s.play();
                }
                last_pos_ms = pos_ms.load(Ordering::Relaxed);
                last_pos_at = Instant::now();
                // 开闸放在 play() 之后：闸门是逐样本推进的，先开闸再恢复会让淡入的头几毫秒
                // 空转在暂停态里，恢复瞬间反而是满幅。
                effects.end_fade_out();
            }
            Ok(PlayerCmd::Stop) => {
                if sink.is_some() {
                    effects.begin_fade_out();
                    thread::sleep(FADE_SLEEP);
                }
                if let Some(s) = sink.take() {
                    s.stop();
                }
                if let Ok(mut slot) = current_loudness.lock() {
                    *slot = None;
                }
                current = None;
                next = None;
                active = false;
                paused = false;
                position_base_ms = 0;
                pos_ms.store(0, Ordering::Relaxed);
                buffered.store(0.0f64.to_bits(), Ordering::Relaxed);
                buffering.store(false, Ordering::Relaxed);
                last_pos_ms = 0;
                last_pos_at = Instant::now();
            }
            Ok(PlayerCmd::RebindOutput) => {
                // 路由切换（用户驱动，Dart 侧已去抖 300ms）：**不过限流**——连着切五次
                // 输出设备是正常操作，被限流掐掉会直接没声音。也不发 OutputLost/Restored
                // 事件：位置连续、暂停仍是暂停，播放态一点没变，报给 UI 只会添乱。
                if let Some(pending) = &next {
                    if pending.appended {
                        pending.track.ring.retire_sources();
                    }
                }
                rebind_output(
                    &mut output,
                    &mut sink,
                    &mut active,
                    &mut position_base_ms,
                    current.as_ref(),
                    paused,
                    volume,
                    &effects,
                    &pos_ms,
                    &notices,
                    &events,
                );
                if let Some(pending) = &next {
                    if pending.appended {
                        if let Some(s) = &sink {
                            s.append(new_source(
                                &pending.track.ring,
                                pending.track.channels,
                                pending.track.sample_rate,
                                &pending.track.loudness,
                                &effects,
                            ));
                        }
                    }
                }
                last_pos_ms = pos_ms.load(Ordering::Relaxed);
                last_pos_at = Instant::now();
            }
            Ok(PlayerCmd::Seek(secs)) => {
                if active {
                    if let Some(s) = &sink {
                        // rodio 把 try_seek 转发到 RingSource::try_seek（冲刷环 + 通知解码线程
                        // seek），成功后内部 TrackPosition 会把计时基准设为 secs，故 get_pos()
                        // 随即从 secs 起算——下方刷新逻辑无需特判。先即时同步一次位置。
                        if s.try_seek(Duration::from_secs_f64(secs.max(0.0))).is_ok() {
                            position_base_ms = 0;
                            pos_ms.store((secs.max(0.0) * 1000.0) as u64, Ordering::Relaxed);
                        }
                    }
                }
            }
            Ok(PlayerCmd::SetVolume(v)) => {
                volume = v.clamp(0.0, 1.0);
                if let Some(s) = &sink {
                    s.set_volume(volume);
                }
            }
            Ok(PlayerCmd::SetLoudness(settings)) => {
                // 只改当前曲目：新目标增益由 RingSource 斜坡逼近，不会有阶跃。
                if let Some(cur) = &current {
                    cur.loudness.configure(settings);
                }
            }
            Ok(PlayerCmd::SetAudioEffects(settings)) => {
                // 全局配置：写进共享的 `EffectsConfig` 即可，与当前有没有曲目无关
                // （无曲目时写下的设置，下一首起播就直接生效）。消费端每块读一次版本号，
                // 变了才取快照，各参数在其上按斜坡过渡，不会有阶跃。
                effects.set(settings);
            }
            Ok(PlayerCmd::Quit) => break,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        // 输出流的坏消息（error 回调投来的）。非致命错误只记日志，不重开。
        let mut lost_message: Option<String> = None;
        while let Ok(notice) = notice_rx.try_recv() {
            match notice {
                OutputNotice::Lost(msg) => lost_message = Some(msg),
                OutputNotice::Warning(msg) => {
                    log::warn!("[player] 音频输出流报错（非致命）: {msg}")
                }
            }
        }

        // 停摆看门狗（兜底，见 OUTPUT_STALL_TIMEOUT 的注释）：位置该走却不走。
        let mut stalled = false;
        if active {
            // 当前流下载完整后可从准无缝升级为 rodio 同 Sink 串联（零采样间隙）。
            if let (Some(cur), Some(pending), Some(s)) = (&current, &mut next, &sink) {
                if !pending.appended
                    && cur.channels == pending.track.channels
                    && cur.sample_rate == pending.track.sample_rate
                    && cur.shared.is_complete()
                {
                    s.append(new_source(
                        &pending.track.ring,
                        pending.track.channels,
                        pending.track.sample_rate,
                        &pending.track.loudness,
                        &effects,
                    ));
                    pending.appended = true;
                }
            }
            if let (Some(s), Some(cur)) = (&sink, &current) {
                // 刷新播放位置（供歌词同步）。get_pos() 在暂停时停止增长；欠载吐出的
                // 静音要扣掉，否则「没声音但进度条照走」，缓冲结束后歌词全是错位的。
                let played = (s.get_pos().as_millis() as u64).saturating_sub(cur.silence_ms());
                let pos = position_base_ms.saturating_add(played);
                pos_ms.store(pos, Ordering::Relaxed);
                // 刷新缓冲进度（供进度条缓冲层）。done 后恒 1.0。
                buffered.store(cur.shared.buffered_fraction().to_bits(), Ordering::Relaxed);
                buffering.store(cur.ring.is_buffering(), Ordering::Relaxed);
                if pos != last_pos_ms {
                    last_pos_ms = pos;
                    last_pos_at = Instant::now();
                    // 稳定播放一段时间就把重开额度还回去。
                    limiter.note_progress(last_pos_at);
                } else {
                    // 位置定住、没暂停、环里有数据 —— 只剩「输出端不再消费采样」一种解释。
                    stalled =
                        output_stalled(last_pos_at.elapsed(), paused, cur.ring.is_buffering());
                }
                let boundary = if next.as_ref().is_some_and(|p| p.appended) {
                    cur.ring.is_drained()
                } else {
                    s.empty()
                };
                if boundary {
                    if let Some(failure) = cur.ring.terminal_failure() {
                        if let Some(pending) = next.take() {
                            pending.track.ring.retire_sources();
                        }
                        if let Some(s) = sink.take() {
                            s.stop();
                        }
                        active = false;
                        current = None;
                        buffering.store(false, Ordering::Relaxed);
                        let _ = events.send(PlayerEvent::Error(failure));
                    } else if let Some(pending) = next.take() {
                        if !pending.appended {
                            if let Some(old) = sink.take() {
                                old.stop();
                            }
                            if let Some(slot) = &output {
                                let fresh = new_sink(slot);
                                fresh.set_volume(volume);
                                fresh.append(new_source(
                                    &pending.track.ring,
                                    pending.track.channels,
                                    pending.track.sample_rate,
                                    &pending.track.loudness,
                                    &effects,
                                ));
                                if paused {
                                    fresh.pause();
                                }
                                sink = Some(fresh);
                            }
                        }
                        let key = pending.track.loudness.key().to_string();
                        events
                            .generation
                            .store(pending.track.generation, Ordering::Release);
                        current = Some(pending.track);
                        media_cache::promote_next_pinned();
                        position_base_ms = 0;
                        pos_ms.store(0, Ordering::Relaxed);
                        buffered.store(0.0f64.to_bits(), Ordering::Relaxed);
                        buffering.store(false, Ordering::Relaxed);
                        if let Some(cur) = &current {
                            if let Ok(mut slot) = current_loudness.lock() {
                                *slot = Some(cur.loudness.clone());
                            }
                        }
                        last_pos_ms = 0;
                        last_pos_at = Instant::now();
                        let _ = events.send(PlayerEvent::TrackChanged(key));
                    } else {
                        active = false;
                        current = None;
                        buffering.store(false, Ordering::Relaxed);
                        let _ = events.send(PlayerEvent::Finished);
                    }
                }
            }
        } else {
            // 无活跃曲目：基准跟着走，恢复播放时不会一上来就被判成停摆。
            last_pos_at = Instant::now();
        }

        // ── 输出流已死 → 在新设备上重开（AAudio/oboe 的契约）──
        let dead = output.as_ref().is_some_and(|o| !o.is_alive());
        if dead || stalled {
            let reason = if dead {
                lost_message.unwrap_or_else(|| "音频输出流被系统断开".to_string())
            } else {
                format!(
                    "音频输出停摆 {} 秒（位置停走、未暂停、缓冲充足）",
                    OUTPUT_STALL_TIMEOUT.as_secs()
                )
            };
            let had_track = current.is_some();
            if limiter.allow(Instant::now()) {
                if had_track {
                    let _ = events.send(PlayerEvent::OutputLost(reason));
                }
                let ok = rebind_output(
                    &mut output,
                    &mut sink,
                    &mut active,
                    &mut position_base_ms,
                    current.as_ref(),
                    paused,
                    volume,
                    &effects,
                    &pos_ms,
                    &notices,
                    &events,
                );
                if ok {
                    if let Some(pending) = &next {
                        if pending.appended {
                            if let Some(s) = &sink {
                                s.append(new_source(
                                    &pending.track.ring,
                                    pending.track.channels,
                                    pending.track.sample_rate,
                                    &pending.track.loudness,
                                    &effects,
                                ));
                            }
                        }
                    }
                }
                if ok && had_track {
                    let _ = events.send(PlayerEvent::OutputRestored);
                }
            } else if dead {
                // 限流命中（设备真的没了，或系统在反复断开）：不空转。丢掉死流让下一次
                // Play 还能再试一次，并把终局失败报给 Dart（那边转 error + toast）。
                sink = None;
                output = None;
                active = false;
                if had_track {
                    let _ = events.send(PlayerEvent::Error(PlayerFailure::output(format!(
                        "音频输出反复断开，已停止自动重开：{reason}"
                    ))));
                }
            }
            last_pos_ms = pos_ms.load(Ordering::Relaxed);
            last_pos_at = Instant::now();
        }
    }
}

// ===================== 第 1 级：网络字节流缓冲 =====================

/// 按需 Range 补块的对齐块大小（512KB）。`read_at` 落在未下载区域时按此对齐发起
/// `bytes=start-end` 请求（典型场景：m4a 尾部 `moov`、大幅前跳 seek）。
const RANGE_CHUNK: u64 = 512 * 1024;

/// 下载中途断开后的重连退避。**抖动必须在这一层被吸收**：往上交给 Dart 就只能拆掉整条
/// 流水线重建，而重建只能靠 `startSecs` 重新定位 —— 那正是「倒回一两秒重播」的来源。
/// 这里续传重连时播放位置一动不动，环里还有数秒 PCM 兜着，快速重连用户根本听不出来。
const DOWNLOAD_RETRY_BACKOFF: [Duration; 3] = [
    Duration::from_millis(300),
    Duration::from_millis(800),
    Duration::from_millis(2000),
];

/// 稳定推进这么多字节就归还重试额度：一首歌听十分钟，不该被开头几次抖动把额度用光。
const DOWNLOAD_RETRY_RESET_BYTES: u64 = 2 * 1024 * 1024;

/// 顺序追加写入前缀临时文件。只有下载任务这一个写者，游标顺序推进，`read_at` 走 pread
/// 不动游标，故无需额外同步（写在 `inner` 锁内，读写不并发访问同一游标）。
fn write_prefix(mut f: &File, chunk: &[u8]) -> io::Result<()> {
    f.write_all(chunk)
}

/// 从前缀临时文件的 `offset` 处定位读满 `buf`（pread 语义，不移动写入游标）。调用方保证
/// `offset + buf.len() <= prefix_len`，故读到的都是已落盘字节；不足即视为异常 EOF。
#[cfg(unix)]
fn read_prefix_at(f: &File, offset: u64, buf: &mut [u8]) -> io::Result<()> {
    use std::os::unix::fs::FileExt;
    f.read_exact_at(buf, offset)
}

/// Windows 版：`seek_read` 无 `read_exact` 变体，手动补齐短读（本机为 macOS，此分支仅
/// 保证将来 Windows 编译，不在本平台走）。
#[cfg(windows)]
fn read_prefix_at(f: &File, mut offset: u64, mut buf: &mut [u8]) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        match f.seek_read(buf, offset) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "前缀文件读到 EOF",
                ))
            }
            Ok(n) => {
                let tmp = buf;
                buf = &mut tmp[n..];
                offset += n as u64;
            }
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

struct StreamInner {
    /// 主下载从 0 顺序追加的连续前缀——**落盘到匿名临时文件**（`prefix_file`），内存不再
    /// 常驻整段压缩数据。写入用 `append`（游标顺序写），读取用 `read_at`（pread 定位读，
    /// 不移动游标，故读写互不干扰游标）。文件句柄 drop / 进程退出即自动回收。
    prefix_file: File,
    /// 已落盘前缀的字节长度（= 文件有效数据长度）。原 `prefix.len()` 的所有用途改用它。
    prefix_len: u64,
    /// 按需 Range 拉取的离散段：`起点 -> 字节`（典型：尾部 moov）。段长 ≤ [`RANGE_CHUNK`]。
    /// 数量少、各 ≤512KB，保持内存驻留（不落盘）。
    extra: BTreeMap<u64, Vec<u8>>,
    /// 正在拉取的块起点（去重，避免并发重复拉同一块）。
    pending: BTreeSet<u64>,
    /// 总长度：`download_task` 从响应 `Content-Length` 立即写入；无则为 None（chunked，回退等全量）。
    total_len: Option<u64>,
    /// Range 是否可用：遇非 206 响应 / Range 请求失败置 false，之后回退等主下载推进（只慢不坏）。
    range_ok: bool,
    /// 下载已结束（成功或失败）。指主下载（前缀）结束。
    done: bool,
    /// 下载错误（HTTP 非 2xx / 网络中断）。
    err: Option<String>,
    /// 消费方已取消（切歌/退出），下载任务与读取应尽快收尾。
    cancel: bool,
}

/// 后台下载任务写入、symphonia 读取的共享缓冲。前缀 + 稀疏补块模型：主下载**顺序落盘**到
/// `prefix_file`（匿名临时文件，见 [`StreamInner::prefix_file`]），未下载区域的读取按需发
/// Range 请求存入内存 `extra`，从而消除 m4a 尾部 moov 的全量等待。保留整段压缩数据以支持
/// seek，但前缀驻留磁盘/页缓存而非进程堆——常驻内存只剩少量 `extra` 离散段。
struct SharedStream {
    inner: Mutex<StreamInner>,
    cv: Condvar,
    // ---- Range 补块所需（`open_stream` 注入，创建后不可变）----
    client: reqwest::Client,
    url: String,
    headers: Vec<(String, String)>,
    /// 播放运行时句柄：`read_at`（阻塞的解码/探测线程）用它 `spawn` Range 请求。
    rt: tokio::runtime::Handle,
    /// 持久化缓存键；有值时前缀文件即 media_cache 的 `.part`，完成时 finalize。
    cache_key: Option<String>,
}

impl SharedStream {
    fn new(
        client: reqwest::Client,
        url: String,
        headers: Vec<(String, String)>,
        rt: tokio::runtime::Handle,
        cache_key: Option<String>,
    ) -> Result<Arc<Self>> {
        // 有 cache_key 且缓存可用时：前缀落到 media_cache 的 `.part`（可续传）。
        // 否则匿名临时文件（tempfile 语义，句柄 drop / 进程退出自动回收）。
        let (prefix_file, prefix_len, cache_key) = if let Some(key) = cache_key.as_ref() {
            match media_cache::open_part_writer(key) {
                Ok((f, size, _path)) => (f, size, Some(key.clone())),
                Err(_) => (
                    tempfile::tempfile().context("创建临时缓存文件失败")?,
                    0,
                    None,
                ),
            }
        } else {
            (
                tempfile::tempfile().context("创建临时缓存文件失败")?,
                0,
                None,
            )
        };
        Ok(Arc::new(Self {
            inner: Mutex::new(StreamInner {
                prefix_file,
                prefix_len,
                extra: BTreeMap::new(),
                pending: BTreeSet::new(),
                total_len: None,
                range_ok: true,
                done: false,
                err: None,
                cancel: false,
            }),
            cv: Condvar::new(),
            client,
            url,
            headers,
            rt,
            cache_key,
        }))
    }

    /// 顺序追加一段下载字节：落盘到 `prefix_file` 并推进 `prefix_len`。写在 `inner` 锁内
    /// 完成——每块很小（网络 chunk，数十 KB），同步写落页缓存代价可忽略，无需 `spawn_blocking`。
    /// 写失败则记录错误、不推进 `prefix_len`（读取端据此报错/EOF，语义与网络中断一致）。
    fn append(&self, chunk: &[u8]) {
        let mut g = self.inner.lock().unwrap();
        match write_prefix(&g.prefix_file, chunk) {
            Ok(()) => g.prefix_len += chunk.len() as u64,
            Err(e) => g.err = Some(format!("写入缓存文件失败: {e}")),
        }
        self.cv.notify_all();
    }

    /// 记录响应 `Content-Length`（仅首次生效）。使 `byte_len`/`SeekFrom::End` 不再等全量。
    fn set_total_len(&self, len: u64) {
        let mut g = self.inner.lock().unwrap();
        if g.total_len.is_none() {
            g.total_len = Some(len);
            if let Some(key) = self.cache_key.as_ref() {
                media_cache::update_part(key, g.prefix_len, Some(len));
            }
        }
        self.cv.notify_all();
    }

    fn finish(&self) {
        let mut g = self.inner.lock().unwrap();
        if !g.cancel && g.err.is_none() {
            if let Some(total) = g.total_len {
                if g.prefix_len < total {
                    g.err = Some(format!(
                        "音频流提前结束: 已接收 {} / {} 字节",
                        g.prefix_len, total
                    ));
                }
            }
        }
        g.done = true;
        // 持久化：主下载结束且无错误时，把 `.part` 晋升为完整缓存。
        if g.err.is_none() {
            if let Some(key) = self.cache_key.as_ref() {
                let size = g.prefix_len;
                let total = g.total_len;
                let complete = media_cache::is_complete_download(size, total);
                if complete {
                    drop(g);
                    media_cache::finalize(key, size, total, None);
                    self.cv.notify_all();
                    return;
                } else if size > 0 {
                    media_cache::update_part(key, size, total);
                }
            }
        }
        self.cv.notify_all();
    }

    fn set_err(&self, e: String) {
        let mut g = self.inner.lock().unwrap();
        g.err = Some(e);
        self.cv.notify_all();
    }

    fn cancel(&self) {
        let mut g = self.inner.lock().unwrap();
        g.cancel = true;
        self.cv.notify_all();
    }

    fn is_cancelled(&self) -> bool {
        self.inner.lock().unwrap().cancel
    }

    /// 已知总长（有 `Content-Length` 时立即可得，不阻塞）。
    fn known_total_len(&self) -> Option<u64> {
        self.inner.lock().unwrap().total_len
    }

    /// 已顺序落盘的前缀字节数。断点重连据此发 `Range: bytes={prefix_len}-`。
    fn prefix_len(&self) -> u64 {
        self.inner.lock().unwrap().prefix_len
    }

    /// 缓冲进度 0..=1：`total_len` 已知取 `prefix_len/total`（只算主前缀，不计尾部离散块）；
    /// done 后恒 1.0；总长未知且未 done 时为 0.0。
    fn buffered_fraction(&self) -> f64 {
        let g = self.inner.lock().unwrap();
        if g.done && g.err.is_none() {
            return 1.0;
        }
        match g.total_len {
            Some(total) if total > 0 => (g.prefix_len as f64 / total as f64).clamp(0.0, 1.0),
            _ => 0.0,
        }
    }

    fn is_complete(&self) -> bool {
        let g = self.inner.lock().unwrap();
        g.done && g.err.is_none()
    }

    /// 发起一次对齐块 Range 请求（`spawn` 到播放运行时，异步回填 `extra`）。调用方须已在
    /// `inner` 锁内把 `chunk_start` 标记进 `pending`（本函数不加锁）。请求返回后统一在一次
    /// 加锁里移除 pending 并回填结果 / 标记 range 不可用，再 `notify_all` 唤醒 `read_at`。
    fn spawn_range_fetch(self: &Arc<Self>, chunk_start: u64, chunk_end: u64) {
        let shared = Arc::clone(self);
        let client = self.client.clone();
        let url = self.url.clone();
        let headers = self.headers.clone();
        self.rt.spawn(async move {
            // 请求体全程不持 `inner` 锁（含 await），拿到结果后再一次性加锁回填。
            let outcome: Result<Vec<u8>, ()> = async {
                let mut req = client.get(&url);
                for (k, v) in &headers {
                    req = req.header(k.as_str(), v.as_str());
                }
                // 闭区间字节范围：bytes=start-(end-1)。
                req = req.header("Range", format!("bytes={}-{}", chunk_start, chunk_end - 1));
                let resp = req.send().await.map_err(|_| ())?;
                // 服务器不支持 Range 会回 200（全量）或其他状态，此处只接受 206。
                if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                    return Err(());
                }
                let bytes = resp.bytes().await.map_err(|_| ())?;
                Ok(bytes.to_vec())
            }
            .await;

            let mut g = shared.inner.lock().unwrap();
            g.pending.remove(&chunk_start);
            match outcome {
                Ok(bytes) => {
                    g.extra.insert(chunk_start, bytes);
                }
                // Range 不可用：置位后 `read_at` 回退为等待主下载推进。
                Err(()) => {
                    g.range_ok = false;
                }
            }
            shared.cv.notify_all();
        });
    }

    /// 从 `pos` 读取至多 `buf.len()` 字节；不足时阻塞，直到有更多数据 / 下载结束 / 被取消。
    /// 未命中前缀与离散段、且总长已知、且落在主下载前沿所在块「之后」的区域时，按需发 Range
    /// 补块（避免对主下载正在填充的当前块重复拉取）；Range 不可用则回退等待主下载。
    /// 返回读到的字节数；EOF 或取消返回 0。
    fn read_at(self: &Arc<Self>, pos: u64, buf: &mut [u8]) -> io::Result<usize> {
        let mut g = self.inner.lock().unwrap();
        loop {
            if g.cancel {
                return Ok(0);
            }
            // 1. 命中已落盘的连续前缀：从临时文件 pread 定位读（不移动写入游标）。
            if pos < g.prefix_len {
                let n = (buf.len() as u64).min(g.prefix_len - pos) as usize;
                read_prefix_at(&g.prefix_file, pos, &mut buf[..n])?;
                return Ok(n);
            }
            // 2. 命中某个离散段（取起点 ≤ pos 的最后一段，判是否覆盖 pos）。
            if let Some((&start, seg)) = g.extra.range(..=pos).next_back() {
                let end = start + seg.len() as u64;
                if pos >= start && pos < end {
                    let off = (pos - start) as usize;
                    let n = buf.len().min(seg.len() - off);
                    buf[..n].copy_from_slice(&seg[off..off + n]);
                    return Ok(n);
                }
            }
            // 3. 未命中。总长已知且 Range 可用、且请求块在主下载前沿块「之后」→ 发 Range 补块。
            //    「前沿块」= 主下载当前正在填充的对齐块；只对其之后的块用 Range，避免与顺序下载
            //    重复拉取（正常缓冲欠载只落在前沿块内，走下方等待即可，行为同 v1）。
            if g.range_ok {
                if let Some(total) = g.total_len {
                    if pos < total {
                        let chunk_start = (pos / RANGE_CHUNK) * RANGE_CHUNK;
                        let frontier_chunk = (g.prefix_len / RANGE_CHUNK) * RANGE_CHUNK;
                        if chunk_start > frontier_chunk {
                            if g.pending.contains(&chunk_start) {
                                // 同块已有请求在途：等它就绪后再读。
                                g = self.cv.wait(g).unwrap();
                                continue;
                            }
                            let chunk_end = (chunk_start + RANGE_CHUNK).min(total);
                            g.pending.insert(chunk_start);
                            self.spawn_range_fetch(chunk_start, chunk_end);
                            g = self.cv.wait(g).unwrap();
                            continue;
                        }
                    }
                }
            }
            // 4. 回退：到达已下载末尾且下载结束 → EOF / 报错；否则等主下载推进。
            if g.done {
                if let Some(e) = &g.err {
                    return Err(io::Error::new(io::ErrorKind::Other, e.clone()));
                }
                return Ok(0);
            }
            g = self.cv.wait(g).unwrap();
        }
    }

    /// 返回总长度（用于 `SeekFrom::End`）：有 `Content-Length` 立即返回；否则阻塞至下载结束
    /// 取前缀长度。取消则返回当前已知长度。
    fn wait_total_len(&self) -> u64 {
        let mut g = self.inner.lock().unwrap();
        if let Some(total) = g.total_len {
            return total;
        }
        while !g.done && !g.cancel {
            g = self.cv.wait(g).unwrap();
        }
        g.total_len.unwrap_or(g.prefix_len)
    }

    fn error(&self) -> Option<String> {
        self.inner.lock().unwrap().err.clone()
    }

    /// 测试辅助：顺序追加前缀（落盘 + 推进 `prefix_len`），替代旧测试直接 push `Vec<u8>`。
    #[cfg(test)]
    fn write_prefix_for_test(&self, bytes: &[u8]) {
        let mut g = self.inner.lock().unwrap();
        write_prefix(&g.prefix_file, bytes).unwrap();
        g.prefix_len += bytes.len() as u64;
        self.cv.notify_all();
    }
}

/// symphonia 的 `MediaSource`：从 [`SharedStream`] 顺序读，必要时阻塞等待下载跟上。
struct StreamMediaSource {
    shared: Arc<SharedStream>,
    pos: u64,
}

impl Read for StreamMediaSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.shared.read_at(self.pos, buf)?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for StreamMediaSource {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let new = match pos {
            SeekFrom::Start(p) => p,
            SeekFrom::Current(d) => (self.pos as i64 + d).max(0) as u64,
            // m4a `moov` 常在文件尾：需要总长度。有 Content-Length 立即得，尾部字节再由
            // read_at 的 Range 补块取，不再等全量；无 Content-Length 才回退阻塞至下载完成。
            SeekFrom::End(d) => {
                let total = self
                    .shared
                    .known_total_len()
                    .unwrap_or_else(|| self.shared.wait_total_len());
                (total as i64 + d).max(0) as u64
            }
        };
        self.pos = new;
        Ok(new)
    }
}

impl MediaSource for StreamMediaSource {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        // 仅 isomp4(m4a) 探测会调此方法——它需要总长度去定位文件尾部的 moov atom，且 symphonia
        // 要求 seekable 流必须给出 Some(len)，否则报 Unseekable。有 Content-Length 时立即返回
        // 真实长度（不再等全量），尾部 moov 字节由 read_at 的 Range 补块取；无 Content-Length
        // 才回退阻塞至下载完成。mp3/flac 探测不依赖 byte_len，流式不受影响。
        Some(
            self.shared
                .known_total_len()
                .unwrap_or_else(|| self.shared.wait_total_len()),
        )
    }
}

// ===================== 第 2/3 级：有界 PCM 环 =====================

struct RingInner {
    buf: VecDeque<f32>,
    /// 解码线程已结束（自然流末或带 [terminal_failure] 的失败）。
    done: bool,
    /// 消费方已停止，解码线程应退出。
    cancel: bool,
    /// 待处理的 seek 目标（秒）：由音频线程（`RingSource::try_seek`）置位，解码线程取出后
    /// 对 `FormatReader` 执行 seek。置位时同步冲刷环内残留 PCM，使旧位置音频立即停播。
    seek_to: Option<f64>,
    /// rodio 已暂停；空环等待在暂停期间不累计欠载超时。
    paused: bool,
    /// 空环正在等待生产者。仅状态级变化，由 FRB 低频轮询。
    buffering: bool,
    /// 正处于「欠载后重新攒缓冲」阶段：攒够 [`RESUME_PREBUFFER_SECS`] 才恢复出声。
    refilling: bool,
    /// 本轮连续欠载的起点。取到数据即清空；持续超过 [`Ring::underrun_timeout`] 判死。
    underrun_since: Option<Instant>,
    /// 非正常结束原因；自然 EOF 为 None。
    terminal_failure: Option<PlayerFailure>,
    /// 消费者代数。输出重绑时 +1，令旧 [`RingSource`] 立刻从 `pop_chunk` 返回 false
    /// 而**不**把环标记为 done/cancel——流水线继续生产，只是换了个消费者。
    /// 放在锁内（而非独立原子）是为了让 condvar 的等待谓词能无竞态地观察到它：
    /// 若放在锁外，「谓词判定 → 进入 wait」之间的退休通知会丢失，旧 source 永久阻塞，
    /// drop `OutputStream` 时音频回调就收不回来。
    source_epoch: u64,
}

/// 解码线程（生产者）与 rodio `Source`（消费者）之间的有界 PCM 队列。
struct Ring {
    inner: Mutex<RingInner>,
    /// 生产者（解码线程）在环满时的背压等待。**消费端没有对应的 condvar**：
    /// 它跑在音频回调上，欠载时吐静音立刻返回，从不等待。
    not_full: Condvar,
    cap: usize,
    /// seek 代数：`request_seek` 时 +1。消费端（[`RingSource`]）每采样无锁读取，
    /// 代数变化说明发生过 seek，其本地块缓冲里是 seek 前的旧采样，须立即丢弃。
    generation: AtomicU64,
    /// 消费端因欠载吐出的静音采样累计数。**不计入播放位置**：播放线程从 `Sink::get_pos()`
    /// 里扣掉它（`TrackPosition` 数的是本 source 吐出的每一个采样，静音也算），缓冲期间
    /// 进度条与歌词才会停在原地，而不是一边没声一边空走。seek / 重绑输出时清零。
    silence_samples: AtomicU64,
    /// None 用于完整本地文件；网络流使用 [`UNDERRUN_TIMEOUT`]，测试可注入更短值。
    underrun_timeout: Option<Duration>,
}

/// [`Ring::pop_chunk`] 的结局。**没有「阻塞等待」这一档**：本函数跑在音频回调线程上，
/// 挂住它会让 CoreAudio / AAudio 判过载甚至断流，反被当成设备故障去重绑输出。
enum RingRead {
    /// 取到 PCM，已覆盖写入 `out`。
    Data,
    /// 环空但流还活着：调用方应输出这么多个静音采样，位置不推进。
    Underrun(usize),
    /// 流结束（done / cancel 且环空）或本 source 已退休。
    End,
}

impl Ring {
    fn new(cap: usize) -> Arc<Self> {
        Self::new_with_timeout(cap, UNDERRUN_TIMEOUT)
    }

    fn new_with_timeout(cap: usize, underrun_timeout: Duration) -> Arc<Self> {
        Self::new_inner(cap, Some(underrun_timeout))
    }

    fn new_local(cap: usize) -> Arc<Self> {
        Self::new_inner(cap, None)
    }

    fn new_inner(cap: usize, underrun_timeout: Option<Duration>) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(RingInner {
                buf: VecDeque::new(),
                done: false,
                cancel: false,
                seek_to: None,
                paused: false,
                buffering: false,
                refilling: false,
                underrun_since: None,
                terminal_failure: None,
                source_epoch: 0,
            }),
            not_full: Condvar::new(),
            cap,
            generation: AtomicU64::new(0),
            silence_samples: AtomicU64::new(0),
            underrun_timeout,
        })
    }

    /// 推入一段采样；环满则背压阻塞，避免预解整首。被取消或有 seek 请求则丢弃剩余并返回，
    /// 让解码线程尽快回到循环顶部处理 seek（避免把 seek 前的旧采样继续灌进环）。
    ///
    /// 块级写入：每轮把「环内剩余空间」能装下的部分一次 `extend` 进去，装满才等待，
    /// 整段只唤醒消费者 O(块数) 次——而非逐采样加锁/唤醒。
    fn push_slice(&self, samples: &[f32]) {
        let mut g = self.inner.lock().unwrap();
        let mut rest = samples;
        while !rest.is_empty() {
            while g.buf.len() >= self.cap && !g.done && !g.cancel && g.seek_to.is_none() {
                // 满则背压等待。消费端从不等待（欠载即吐静音返回），故这里不必反向唤醒。
                g = self.not_full.wait(g).unwrap();
            }
            if g.done || g.cancel || g.seek_to.is_some() {
                return;
            }
            let n = (self.cap - g.buf.len()).min(rest.len());
            g.buf.extend(rest[..n].iter().copied());
            g.buffering = false;
            rest = &rest[n..];
        }
    }

    /// 请求 seek（音频线程调用）：置目标、冲刷环内残留 PCM、递增代数（让消费端丢弃
    /// 本地缓冲里的旧采样），并唤醒可能在背压等待的解码线程。
    /// 因 `push_slice` 持锁完成单次 `extend` 后才可能释放锁，清空 buf 时不会与正在进行的
    /// 推入交错，故清空后环必为空——seek 前的旧采样不会残留。
    fn request_seek(&self, secs: f64) {
        let mut g = self.inner.lock().unwrap();
        g.seek_to = Some(secs);
        g.done = false;
        g.buf.clear();
        // seek 后 rodio 的 `TrackPosition` 会把计时基准重设为目标位置，静音账也随之作废。
        // 顺带清掉欠载态：seek 造成的空环不是网络欠载，不该继承上一段的判死计时。
        g.refilling = false;
        g.underrun_since = None;
        self.silence_samples.store(0, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::Release);
        self.not_full.notify_all();
    }

    /// 累计一段欠载静音（消费端调用）。
    fn note_silence(&self, n: u64) {
        self.silence_samples.fetch_add(n, Ordering::Relaxed);
    }

    fn silence_samples(&self) -> u64 {
        self.silence_samples.load(Ordering::Relaxed)
    }

    /// 清零静音账。重绑输出时调用：新 `Sink` 的 `get_pos()` 从 0 起算，而扣减基准
    /// （`position_base_ms`）取的是已经扣过静音的位置，不清零会重复扣一遍。
    fn reset_silence(&self) {
        self.silence_samples.store(0, Ordering::Relaxed);
    }

    /// 解码线程取出待处理的 seek 目标。
    fn take_seek(&self) -> Option<f64> {
        self.inner.lock().unwrap().seek_to.take()
    }

    /// Keep the decoder available after natural EOF: the audio ring may still contain
    /// seconds of playback (or be paused), and a later seek must revive the producer.
    fn wait_at_eof(&self) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.seek_to.is_some() {
            return true;
        }
        g.done = true;
        g.buffering = false;
        self.not_full.notify_all();
        while !g.cancel && g.seek_to.is_none() {
            g = self.not_full.wait(g).unwrap();
        }
        !g.cancel
    }

    fn finish(&self) {
        let mut g = self.inner.lock().unwrap();
        g.done = true;
        g.buffering = false;
        self.not_full.notify_all();
    }

    fn finish_with_error(&self, failure: PlayerFailure) {
        let mut g = self.inner.lock().unwrap();
        if g.terminal_failure.is_none() {
            g.terminal_failure = Some(failure);
        }
        g.done = true;
        g.buffering = false;
        self.not_full.notify_all();
    }

    fn cancel(&self) {
        let mut g = self.inner.lock().unwrap();
        g.cancel = true;
        g.buffering = false;
        self.not_full.notify_all();
    }

    fn set_paused(&self, paused: bool) {
        let mut g = self.inner.lock().unwrap();
        g.paused = paused;
        if paused {
            g.buffering = false;
        }
    }

    fn is_buffering(&self) -> bool {
        self.inner.lock().unwrap().buffering
    }

    fn terminal_failure(&self) -> Option<PlayerFailure> {
        self.inner.lock().unwrap().terminal_failure.clone()
    }

    fn is_drained(&self) -> bool {
        let g = self.inner.lock().unwrap();
        (g.done || g.cancel) && g.buf.is_empty()
    }

    /// 让当前所有 `RingSource` 退休：下一次 `pop_chunk` 即返回「流结束」，但环、解码
    /// 线程、下载任务一概不动。播放线程重绑输出前调用。
    fn retire_sources(&self) {
        let mut g = self.inner.lock().unwrap();
        g.source_epoch += 1;
        g.buffering = false;
        self.not_full.notify_all();
    }

    fn current_source_epoch(&self) -> u64 {
        self.inner.lock().unwrap().source_epoch
    }

    /// 批量弹出至多 `max` 个采样填入 `out`（覆盖式）。
    ///
    /// **绝不阻塞**：本函数跑在音频回调线程上。环空而流还活着时返回
    /// [`RingRead::Underrun`]，由调用方吐等长静音、回调立刻返回——挂住回调会让
    /// CoreAudio / AAudio 判过载甚至断流，那条路最后会绕到「重绑输出」，把一次网络抖动
    /// 误诊成设备故障。
    ///
    /// 水位线唤醒：仅当环内剩余降到半容量以下才唤醒解码线程，让生产端一次补一大段，
    /// 而非每次搬运都触发线程唤醒。
    ///
    /// `align` = 声道数：返回长度必为它的整数倍（见 [`poppable`]）。
    /// `prebuffer` = 欠载恢复门限（交错采样数，见 [`RESUME_PREBUFFER_SECS`]）。
    fn pop_chunk(
        &self,
        out: &mut Vec<f32>,
        max: usize,
        epoch: u64,
        align: usize,
        prebuffer: usize,
    ) -> RingRead {
        let mut g = self.inner.lock().unwrap();
        // 退休检查必须排在取数据之前：重绑输出后旧 source 若再 drain 一次，
        // 那段 PCM 就被它吞掉且永远送不出声了。
        if g.source_epoch != epoch {
            return RingRead::End;
        }
        // 欠载恢复的迟滞门限。收尾（done/cancel）时不设门限——残料必须放完，
        // 否则最后不足一秒的尾巴会被永远扣在环里，曲终判定也跟着卡住。
        let gate = if g.refilling && !g.done && !g.cancel {
            prebuffer.min(self.cap / 2).max(align)
        } else {
            0
        };
        if g.buf.len() >= gate {
            let mut n = poppable(g.buf.len(), max, align);
            if n == 0 && (g.done || g.cancel) {
                // 收尾时残留不足一帧（生产者按帧推入，正常不会发生）：全取走。
                // 否则消费者会卡在「环非空但取不出」上。
                n = g.buf.len().min(max);
            }
            if n > 0 {
                out.clear();
                out.extend(g.buf.drain(..n));
                if g.buf.len() < self.cap / 2 {
                    self.not_full.notify_all();
                }
                g.buffering = false;
                g.refilling = false;
                g.underrun_since = None;
                return RingRead::Data;
            }
            // n == 0 且流还活着：不足一帧的残留留到下一块，落入下方欠载分支。
        }
        if g.done || g.cancel {
            g.buffering = false;
            return RingRead::End;
        }
        // 环空 / 正在攒 prebuffer：先唤醒可能在背压等待的生产者，再吐静音。
        self.not_full.notify_all();
        g.refilling = true;
        if g.paused {
            // 暂停时位置本就不走，rodio 的 `Pausable` 也不会拉我们（它自产静音）。
            // 不累计判死计时：暂停一小时不该被当成网络欠载。
            g.buffering = false;
            g.underrun_since = None;
        } else {
            g.buffering = true;
            let now = Instant::now();
            let since = *g.underrun_since.get_or_insert(now);
            if let Some(limit) = self.underrun_timeout {
                if now.duration_since(since) >= limit {
                    g.terminal_failure = Some(PlayerFailure::network(format!(
                        "网络缓冲超时（{} 秒无可播放数据）",
                        limit.as_secs()
                    )));
                    g.done = true;
                    g.buffering = false;
                    self.not_full.notify_all();
                    return RingRead::End;
                }
            }
        }
        // 静音同样必须帧对齐，否则左右声道会逐渐串位（见 [`poppable`]）。
        RingRead::Underrun(poppable(SILENCE_CHUNK_SAMPLES, max, align).max(align.min(max)))
    }
}

/// 本次可弹出的采样数：截到 `max`，再向下对齐到 `align`（声道数）的整数倍。
///
/// **必须帧对齐**：立体声级要成对的 L/R，块边界切在半帧上会让左右声道逐渐串位——
/// 症状是立体声像慢慢偏移，极难排查。`LOCAL_CHUNK_SAMPLES` 对 1/2/4/8 声道都整除，
/// 但对 6 声道（5.1）不整除，所以对齐不能省。
///
/// `max < align` 时不做对齐（否则永远取不出任何东西，消费者直接饿死）；实际调用里
/// `max = 4096`，这条只是防御。
fn poppable(len: usize, max: usize, align: usize) -> usize {
    let n = len.min(max);
    if align > 1 && align <= max {
        n - n % align
    } else {
        n
    }
}

#[cfg(test)]
fn terminal_player_event(ring: &Ring) -> PlayerEvent {
    match ring.terminal_failure() {
        Some(failure) => PlayerEvent::Error(failure),
        None => PlayerEvent::Finished,
    }
}

/// 取消守卫：`RingSource` 被 drop（切歌/退出）时，取消解码线程、下载缓冲与下载任务，
/// 防止解码线程在背压上永久阻塞、下载继续空跑。
struct StreamGuard {
    shared: Arc<SharedStream>,
    ring: Arc<Ring>,
    dl: tokio::task::AbortHandle,
}

impl Drop for StreamGuard {
    fn drop(&mut self) {
        self.shared.cancel();
        self.ring.cancel();
        self.dl.abort();
    }
}

/// rodio `Source`：音频回调仅从环里搬采样，不做解码。
///
/// 持本地块缓冲：`next()` 先从本地取；耗尽才锁一次环批量搬 [`LOCAL_CHUNK_SAMPLES`] 个，
/// 把音频回调路径的加锁频率从每采样一次降到每块一次。
struct RingSource {
    ring: Arc<Ring>,
    channels: u16,
    sample_rate: u32,
    /// 本地块缓冲 + 读游标。
    local: Vec<f32>,
    local_pos: usize,
    /// 上次同步到的 seek 代数；与 [`Ring::generation`] 不一致说明发生过 seek，
    /// 本地缓冲里是旧位置采样，须丢弃（每采样一次的 Relaxed/Acquire 原子读，无锁开销）。
    generation: u64,
    /// 本 source 的消费者代数。与环内当前代数不一致即已退休（输出被重绑），
    /// `pop_chunk` 直接返回 false，本 source 就此结束。
    source_epoch: u64,
    /// 响度均衡增益。每搬一块施加一次（块内斜坡），未启用时是纯粹的分支跳过。
    loudness: Arc<TrackLoudness>,
    /// 交错采样速率（采样率 × 声道），把块长换算成时长以限制增益变化速率。
    samples_per_sec: f64,
    /// 欠载恢复门限（交错采样数）= [`RESUME_PREBUFFER_SECS`] × `samples_per_sec`。
    prebuffer_samples: usize,
    /// 全局音效配置（所有 source 共享）。见 [`crate::effects::EffectsConfig`]。
    effects_config: Arc<EffectsConfig>,
    /// 本 source 私有的滤波器状态与斜坡值。
    effects: EffectsProcessor,
}

impl Iterator for RingSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        loop {
            let gen = self.ring.generation.load(Ordering::Acquire);
            if gen != self.generation {
                self.generation = gen;
                self.local.clear();
                self.local_pos = 0;
                // seek 是波形上的硬切换：留着旧位置喂出来的滤波器状态会 ringing，
                // 且切换点本身就是一次咔哒，故顺带重走一次 8ms 淡入。
                self.effects.reset();
            }
            if self.local_pos < self.local.len() {
                let s = self.local[self.local_pos];
                self.local_pos += 1;
                return Some(s);
            }
            match self.ring.pop_chunk(
                &mut self.local,
                LOCAL_CHUNK_SAMPLES,
                self.source_epoch,
                self.channels.max(1) as usize,
                self.prebuffer_samples,
            ) {
                RingRead::Data => {
                    // 响度增益在此施加：环里存的是**未加增益**的原始 PCM（测量也取原始信号），
                    // 出环即将送进 sink 时才乘上，故改增益立刻生效、不受环内 4 秒缓冲拖延。
                    self.loudness.apply(&mut self.local, self.samples_per_sec);
                    // 音效紧随其后（EQ / 立体声 / 淡入淡出 / 软削波），同样是出环即施加，
                    // 所以拖滑块的效果不会被环内的数秒缓冲拖延。
                    self.effects.process(&mut self.local, &self.effects_config);
                    self.local_pos = 0;
                }
                // 网络欠载：吐静音让音频回调**立刻返回**，流水线原地等下载续上。
                // 不过响度/音效——整块是零，处理它只会白白推进增益斜坡与滤波器状态。
                RingRead::Underrun(n) => {
                    self.local.clear();
                    self.local.resize(n, 0.0);
                    self.local_pos = 0;
                    self.ring.note_silence(n as u64);
                }
                // 流结束（done/cancel 且环空），或本 source 已退休。
                RingRead::End => return None,
            }
        }
    }
}

impl Source for RingSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        self.channels
    }
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }

    /// rodio `Sink::try_seek` 经 speed/pausable/track_position 链转发到这里。我们仅向解码线程
    /// 发出 seek 请求并冲刷环（实际的 `FormatReader::seek` 在解码线程异步完成，不阻塞音频回调），
    /// 随即返回 `Ok`。上游 `TrackPosition::try_seek` 据此把计时基准重置为目标位置，使 `get_pos()`
    /// 自动从新位置起算。冲刷后环短暂为空，音频回调会在 `pop` 上等待解码线程供给新位置采样
    /// （与正常网络欠载时的等待行为一致），即 seek 时的短暂静音。
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.ring.request_seek(pos.as_secs_f64());
        Ok(())
    }
}

// ===================== 打开流：下载 + 探测 + 解码线程 =====================

/// 打开一条流式播放：启动后台下载，探测格式并起解码线程，返回可直接播放的 [`DecodedStream`]。
/// 边下边解码——首包就绪即返回，不等整首下载完。
pub async fn open_stream(
    client: reqwest::Client,
    url: String,
    headers: Vec<(String, String)>,
    cache_key: Option<String>,
    start_secs: f64,
    loudness_settings: LoudnessSettings,
    loudness_key: String,
) -> Result<DecodedStream> {
    // Range 补块需在阻塞的解码/探测线程里发请求，故把 client/url/headers + 当前运行时句柄
    // 存进 SharedStream（open_stream 运行于播放运行时内，current() 即该运行时）。
    let shared = SharedStream::new(
        client,
        url,
        headers,
        tokio::runtime::Handle::current(),
        cache_key,
    )?;
    let ring = Ring::new(RING_CAP_SAMPLES);
    let loudness = Arc::new(TrackLoudness::new(loudness_settings, loudness_key));

    // 后台下载任务：HTTP 字节流 -> SharedStream（顺序写前缀）。
    let (head_tx, head_rx) = oneshot::channel();
    let dl = tokio::spawn(download_task(shared.clone(), head_tx));
    let dl_abort = dl.abort_handle();

    // 探测格式 + 解码首包（拿到 channels/sample_rate），并在内部 spawn 解码线程。
    // 与下载侧的响应头结果一起等：两者本就并发，`join!` 不引入额外延迟。
    let (spec, head) = {
        let shared = shared.clone();
        let ring = ring.clone();
        let loudness = loudness.clone();
        let prepare =
            tokio::task::spawn_blocking(move || prepare_decode(shared, ring, start_secs, loudness));
        tokio::join!(prepare, head_rx)
    };
    let spec = spec.context("解码准备任务异常")?;

    // 地址本身不可用（403 / 续传被拒 / 连不上）时必须向上报错，即使 `prepare_decode`
    // 靠残留 `.part` 前缀解出了首包——否则 Dart 侧会当作起播成功，不再尝试下一候选地址，
    // 而这个错误会等到解码读过前缀末尾才以播放中的 error 事件冒出来。
    let head_failed = match head {
        Ok(Ok(())) => None,
        Ok(Err(msg)) => Some(msg),
        // 发送端被丢弃（任务被取消）：交给 spec 判定，不额外造错。
        Err(_) => None,
    };

    let (channels, sample_rate) = match (spec, head_failed) {
        (_, Some(msg)) => {
            shared.cancel();
            ring.cancel();
            dl_abort.abort();
            return Err(anyhow!(msg));
        }
        (Ok(s), None) => s,
        (Err(e), None) => {
            // 准备失败（坏音频/下载错误）：取消下载与缓冲，向上报错以便尝试下一候选地址。
            shared.cancel();
            ring.cancel();
            dl_abort.abort();
            // 容器/编码根本解不了时把缓存对象删掉。局域网上 5MB 往往在探测失败前就下完并
            // finalize 了，留着的话下次播放会命中缓存 → 本地解码再失败一次 → 回落网络又下
            // 一遍，每次播放白跑一轮（2026-08-11 实测）。下载失败不走这里：`.part` 前缀要
            // 留给续传。
            if e.downcast_ref::<UnsupportedAudio>().is_some() {
                if let Some(key) = shared.cache_key.clone() {
                    media_cache::remove(&key);
                }
            }
            return Err(e);
        }
    };

    Ok(DecodedStream {
        generation: 0,
        channels,
        sample_rate,
        start_secs,
        guard: StreamGuard {
            shared: shared.clone(),
            ring: ring.clone(),
            dl: dl_abort,
        },
        shared,
        ring,
        loudness,
    })
}

/// 从本地完整文件起播（媒体缓存命中）。不经 HTTP，不写盘。
pub async fn open_local(
    path: String,
    loudness_settings: LoudnessSettings,
    loudness_key: String,
) -> Result<DecodedStream> {
    let path_buf = std::path::PathBuf::from(&path);
    if !path_buf.is_file() {
        return Err(anyhow!("本地缓存文件不存在: {path}"));
    }
    let ring = Ring::new_local(RING_CAP_SAMPLES);
    let loudness = Arc::new(TrackLoudness::new(loudness_settings, loudness_key));
    let spec = {
        let ring = ring.clone();
        let path = path.clone();
        let loudness = loudness.clone();
        tokio::task::spawn_blocking(move || prepare_decode_file(path, ring, loudness))
            .await
            .context("本地解码准备任务异常")??
    };
    let (channels, sample_rate) = spec;
    // 本地完整文件：构造一个「已 done」的 SharedStream 占位供缓冲进度恒 1.0。
    // 使用空 URL 的假共享流会很重；这里用内存空前缀 + done 标记的轻量路径：
    // 直接 tempfile + finish。
    let shared = SharedStream::new(
        reqwest::Client::new(),
        String::new(),
        Vec::new(),
        tokio::runtime::Handle::current(),
        None,
    )?;
    {
        let mut g = shared.inner.lock().unwrap();
        g.done = true;
        g.total_len = Some(0);
    }
    Ok(DecodedStream {
        generation: 0,
        channels,
        sample_rate,
        start_secs: 0.0,
        guard: StreamGuard {
            shared: shared.clone(),
            ring: ring.clone(),
            // 无下载任务：造一个立刻结束的 abort handle
            dl: tokio::spawn(async {}).abort_handle(),
        },
        shared,
        ring,
        loudness,
    })
}

/// 本地文件 MediaSource：标准 File 包装。
struct FileMediaSource {
    file: File,
    len: u64,
}

impl Read for FileMediaSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }
}

impl Seek for FileMediaSource {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.file.seek(pos)
    }
}

impl MediaSource for FileMediaSource {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.len)
    }
}

/// 「这条流我们根本解不了」——容器或编码不在编译进来的 symphonia feature 里。
///
/// 与下载失败严格区分：换下一个候选地址也是同样的结果，且**已经下到的字节不能留在
/// 媒体缓存里**（否则下次播放命中缓存，走一遍本地解码再失败一次，每次都白下一遍）。
/// 判定见 [`open_stream`] 的失败分支。
#[derive(Debug)]
pub struct UnsupportedAudio(pub String);

impl std::fmt::Display for UnsupportedAudio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UnsupportedAudio {}

/// 按头部魔数认容器，只为把报错说人话。返回 None = 没认出来（不代表能解）。
///
/// 为什么需要它：symphonia 的 probe 会在流里**搜索**同步字，喂给它一个不认识的容器时
/// 常常在某个偏移撞上假的 mp3/adts 同步字，于是错误冒出来的是「不支持的音频编码」，
/// 指不到真正的问题。2026-08-11 实例：Navidrome 库里一个从 YouTube 下来的
/// webm/opus 文件被改名成 `.mp3`，服务端信扩展名（既不转码也不报错），
/// 播放器只说「不支持的音频编码」，查了半天才发现是容器不对。
fn sniff_container(head: &[u8]) -> Option<&'static str> {
    if head.len() < 12 {
        return None;
    }
    let tag = &head[8..12];
    if head.starts_with(b"\x1a\x45\xdf\xa3") {
        // EBML：WebM / Matroska。YouTube 下载物几乎都是这个 + Opus。
        return Some("WebM/Matroska（通常是 Opus 编码）");
    }
    if head.starts_with(b"OggS") {
        // Ogg/Vorbis 现在解得了（symphonia 的 `ogg` + `vorbis` feature），失败多半是流被
        // 截断，不能当「不支持」把缓存删掉。同容器的 Opus / Speex 仍无解码器 —— 编码写在
        // 首页第一个包里，就在这 64 字节窗口内，认出来才报不支持。
        if head.windows(8).any(|w| w == b"OpusHead") {
            return Some("Ogg/Opus 编码");
        }
        if head.windows(8).any(|w| w == b"Speex   ") {
            return Some("Ogg/Speex 编码");
        }
        return None;
    }
    if head.starts_with(b"\x30\x26\xb2\x75") {
        return Some("ASF/WMA");
    }
    if head.starts_with(b"MAC ") {
        return Some("Monkey's Audio (APE)");
    }
    if head.starts_with(b"wvpk") {
        return Some("WavPack");
    }
    if head.starts_with(b"TTA1") {
        return Some("True Audio (TTA)");
    }
    if head.starts_with(b"DSD ") || head.starts_with(b"FRM8") {
        return Some("DSD (DSF/DFF)");
    }
    if head.starts_with(b"RIFF") && tag == b"WAVE" {
        return Some("WAV");
    }
    if head.starts_with(b"FORM") && (tag == b"AIFF" || tag == b"AIFC") {
        return Some("AIFF");
    }
    None
}

/// 把 symphonia 的失败换成一句能行动的话。[`head`] 是流的前 64 字节（拿不到就传空切片）。
/// 之所以要 64 而不是魔数那 4 字节：Ogg 得看到首页里的编码标识才分得清 Vorbis 与 Opus。
///
/// **只有认出容器时才返回 [`UnsupportedAudio`]**：没认出来的失败可能只是流被截断 /
/// 下载中断，那种情况必须保留 `.part` 前缀以便续传，不能当成「这文件解不了」删缓存。
fn decode_prepare_error(head: &[u8], detail: String) -> anyhow::Error {
    match sniff_container(head) {
        Some(c) => anyhow::Error::new(UnsupportedAudio(format!(
            "不支持的音频格式：{c}；文件扩展名与实际内容不符时，请在服务端转码后再播"
        ))),
        None => anyhow!(detail),
    }
}

fn prepare_decode_file(
    path: String,
    ring: Arc<Ring>,
    loudness: Arc<TrackLoudness>,
) -> Result<(u16, u32)> {
    let mut file = File::open(&path).with_context(|| format!("打开本地缓存失败: {path}"))?;
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    // 头 64 字节先留一份给 sniff_container（报错时才用），再把游标拨回 0 交给 symphonia。
    let head = {
        let mut b = [0u8; 64];
        let n = file.read(&mut b).unwrap_or(0);
        let _ = file.seek(SeekFrom::Start(0));
        b[..n].to_vec()
    };
    let mss = MediaSourceStream::new(Box::new(FileMediaSource { file, len }), Default::default());
    let probed = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            mss,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )
        .map_err(|e| decode_prepare_error(&head, format!("无法识别本地音频格式: {e}")))?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .context("找不到音频轨")?;
    let track_id = track.id;
    let codec = track.codec_params.codec;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| decode_prepare_error(&head, format!("不支持的音频编码 {codec:?}: {e}")))?;
    let (channels, sample_rate) = loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(_)) | Err(SymphoniaError::ResetRequired) => {
                return Err(anyhow!("本地音频流过短或为空"));
            }
            Err(e) => return Err(anyhow!("读取音频包失败: {e}")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let ch = spec.channels.count().max(1) as u16;
                let rate = spec.rate;
                let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                buf.copy_interleaved_ref(decoded);
                ring.push_slice(buf.samples());
                break (ch, rate);
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(_)) => return Err(anyhow!("本地音频流过短或为空")),
            Err(e) => return Err(anyhow!("解码失败: {e}")),
        }
    };
    thread::spawn(move || decode_loop(format, decoder, track_id, ring, None, loudness, true));
    Ok((channels, sample_rate))
}

/// 主下载任务。
///
/// [`head_tx`] 在拿到响应头（或确定拿不到）时立刻回报一次结果：`Ok(())` 表示这个地址可用，
/// `Err(msg)` 表示这次请求废了。[`open_stream`] 据此决定要不要向上报错——**必须有这条通道**：
/// 命中残留 `.part` 时 `prepare_decode` 光靠本地前缀就能解出首包并返回 Ok，下载侧的 403
/// 便会被掩盖到播放中途才以 error 事件冒出来，Dart 侧的候选地址回退也就永远轮不到。
async fn download_task(shared: Arc<SharedStream>, head_tx: oneshot::Sender<Result<(), String>>) {
    let mut head_tx = Some(head_tx);
    let mut attempt = 0usize;
    // 「稳定推进」基准：自上次归还额度以来又下了多少。长曲目不该被开头几次抖动耗光额度。
    let mut progress_mark = shared.prefix_len();

    loop {
        let first = head_tx.is_some();
        match download_once(&shared, &mut head_tx, first).await {
            DownloadOutcome::Complete => break,
            // 地址本身不可用（非 2xx / 不支持续传）：重连也是同样结果，交给上层换地址。
            DownloadOutcome::Fatal(msg) => {
                shared.set_err(msg);
                break;
            }
            // 中途断开（TCP reset / TLS 中断 / 读超时 / 响应体提前结束）。
            DownloadOutcome::Interrupted(msg) => {
                if shared.is_cancelled() {
                    break;
                }
                let advanced = shared.prefix_len();
                if advanced >= progress_mark.saturating_add(DOWNLOAD_RETRY_RESET_BYTES) {
                    progress_mark = advanced;
                    attempt = 0;
                }
                let Some(delay) = DOWNLOAD_RETRY_BACKOFF.get(attempt) else {
                    shared.set_err(msg);
                    break;
                };
                attempt += 1;
                log::warn!(
                    "[player] 下载中断（{msg}），{}ms 后从 {advanced} 字节处续传（第 {attempt} 次）",
                    delay.as_millis()
                );
                tokio::time::sleep(*delay).await;
                if shared.is_cancelled() {
                    break;
                }
            }
        }
    }
    shared.finish();
}

/// 一次下载连接的结局。
enum DownloadOutcome {
    /// 响应体读到自然结束，且（总长已知时）字节齐全。
    Complete,
    /// 地址级失败：重连没有意义，直接判死让上层换候选地址 / 重新问插件要 URL。
    Fatal(String),
    /// 中途断开：字节仍然有效，可从 `prefix_len` 处续传重连。
    Interrupted(String),
}

/// 发起一次（可能带 `Range` 续传的）下载连接，把响应体顺序追加进 `shared`。
///
/// `head_tx` 只在**首次**连接时回报：`open_stream` 靠它判断「这个地址能不能用」。
/// 重连发生在已经出过声之后，此时地址早已被判为可用，不再回报。
async fn download_once(
    shared: &Arc<SharedStream>,
    head_tx: &mut Option<oneshot::Sender<Result<(), String>>>,
    first: bool,
) -> DownloadOutcome {
    // 首次连接的失败要立刻回报，`open_stream` 才能及时换下一个候选地址。
    macro_rules! fail_head {
        ($msg:expr) => {{
            let msg: String = $msg;
            if let Some(tx) = head_tx.take() {
                let _ = tx.send(Err(msg.clone()));
            }
            return DownloadOutcome::Fatal(msg);
        }};
    }

    let resume_from = shared.prefix_len();
    let mut req = shared.client.get(&shared.url);
    for (k, v) in &shared.headers {
        req = req.header(k.as_str(), v.as_str());
    }
    if resume_from > 0 {
        req = req.header("Range", format!("bytes={resume_from}-"));
    }
    let resp = match req.send().await {
        Ok(r) => r,
        // 首次连不上 = 地址不可用（快速失败换地址）；重连连不上只是这一次没连上，可以再试。
        Err(e) if first => fail_head!(format!("发起请求失败: {e}")),
        Err(e) => return DownloadOutcome::Interrupted(format!("重连失败: {e}")),
    };
    let status = resp.status();
    // 续传：期望 206；若 200 且 resume_from>0，说明服务器忽略 Range——删残片全量重来太重，
    // 这里若 200 且已有前缀则放弃续传、报错让上层删 part 重试（由 finish 保留 incomplete）。
    // **只有首次连接才删 part**：重连时前缀是本次刚下好的有效数据，删掉是自毁。
    if resume_from > 0 && status.as_u16() == 200 {
        if first {
            if let Some(key) = shared.cache_key.clone() {
                media_cache::remove(&key);
            }
        }
        fail_head!("服务器不支持断点续传，请重试".into());
    }
    if resume_from > 0 && status.as_u16() == 416 {
        if first {
            if let Some(key) = shared.cache_key.clone() {
                media_cache::remove(&key);
            }
        }
        fail_head!("缓存续传范围无效，请重试".into());
    }
    if !status.is_success() {
        fail_head!(format!("CDN 返回 {status}"));
    }
    if let Some(tx) = head_tx.take() {
        let _ = tx.send(Ok(()));
    }
    // 总长：206 时 Content-Range: bytes a-b/total；200 时 Content-Length。
    if let Some(cr) = resp.headers().get(reqwest::header::CONTENT_RANGE) {
        if let Ok(s) = cr.to_str() {
            if let Some(total) = s.rsplit('/').next().and_then(|t| t.parse::<u64>().ok()) {
                shared.set_total_len(total);
            }
        }
    } else if let Some(len) = resp.content_length() {
        if resume_from == 0 {
            shared.set_total_len(len);
        } else {
            shared.set_total_len(resume_from + len);
        }
    }

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if shared.is_cancelled() {
            return DownloadOutcome::Complete;
        }
        match chunk {
            Ok(b) => shared.append(&b),
            Err(e) => return DownloadOutcome::Interrupted(format!("读取音频流失败: {e}")),
        }
    }
    // 响应体正常结束。总长已知却没收齐 = 被静默截断，按可续传处理（与 chunk 出错同等待遇）。
    match shared.known_total_len() {
        Some(total) if shared.prefix_len() < total => DownloadOutcome::Interrupted(format!(
            "音频流提前结束: 已接收 {} / {total} 字节",
            shared.prefix_len()
        )),
        _ => DownloadOutcome::Complete,
    }
}

/// 探测格式、建解码器、解首包拿到声道/采样率，然后把后续解码循环交给独立线程。
/// 返回 `(channels, sample_rate)`。
fn prepare_decode(
    shared: Arc<SharedStream>,
    ring: Arc<Ring>,
    start_secs: f64,
    loudness: Arc<TrackLoudness>,
) -> Result<(u16, u32)> {
    let mss = MediaSourceStream::new(
        Box::new(StreamMediaSource {
            shared: shared.clone(),
            pos: 0,
        }),
        Default::default(),
    );
    // 头 64 字节：只在报错时用来认容器（此刻前缀早已落盘，read_at 不会阻塞）。
    let head = {
        let mut b = [0u8; 64];
        let n = shared.read_at(0, &mut b).unwrap_or(0);
        b[..n].to_vec()
    };
    let probed = match symphonia::default::get_probe().format(
        &Hint::new(),
        mss,
        &FormatOptions {
            enable_gapless: true,
            ..Default::default()
        },
        &MetadataOptions::default(),
    ) {
        Ok(p) => p,
        // 下载层错误（HTTP 非 2xx / 网络中断）会被 probe 包成 IoError，优先回报真实原因。
        Err(e) => {
            if let Some(de) = shared.error() {
                return Err(anyhow!(de));
            }
            return Err(decode_prepare_error(
                &head,
                format!("无法识别音频格式: {e}"),
            ));
        }
    };

    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .context("找不到音频轨")?;
    let track_id = track.id;
    let codec = track.codec_params.codec;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| decode_prepare_error(&head, format!("不支持的音频编码 {codec:?}: {e}")))?;

    // 恢复播放必须在首包入环前完成定位。旧实现先起播再异步 seek，定位失败时声音从头播，
    // 但 Flutter 已把歌词和进度设到旧位置，三者会永久错位。
    if start_secs > 0.0 {
        format
            .seek(
                SeekMode::Coarse,
                SeekTo::Time {
                    time: Time::from(start_secs),
                    track_id: Some(track_id),
                },
            )
            .map_err(|e| anyhow!("恢复播放定位失败: {e}"))?;
        decoder.reset();
    }

    // 解码首包以确定声道/采样率（rodio 需在播放前知道），首包采样推入环。
    let (channels, sample_rate) = loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            // 认得出容器就是「解不了」，认不出才可能是流被截断——见 decode_prepare_error。
            Err(SymphoniaError::IoError(_)) | Err(SymphoniaError::ResetRequired) => {
                return Err(decode_prepare_error(&head, "音频流过短或为空".into()));
            }
            Err(e) => return Err(decode_prepare_error(&head, format!("读取音频包失败: {e}"))),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let ch = spec.channels.count().max(1) as u16;
                let rate = spec.rate;
                let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                buf.copy_interleaved_ref(decoded);
                ring.push_slice(buf.samples());
                break (ch, rate);
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(_)) => {
                return Err(decode_prepare_error(&head, "音频流过短或为空".into()))
            }
            Err(e) => return Err(decode_prepare_error(&head, format!("解码失败: {e}"))),
        }
    };

    // 后续 packet 在独立线程解码，背压由环控制。
    let decode_shared = shared.clone();
    thread::spawn(move || {
        decode_loop(
            format,
            decoder,
            track_id,
            ring,
            Some(decode_shared),
            loudness,
            start_secs <= 0.0,
        )
    });

    Ok((channels, sample_rate))
}

/// 解码循环（独立线程）：逐 packet 解码并推入环，直到流末尾 / 错误 / 被取消。
fn decode_loop(
    mut format: Box<dyn FormatReader>,
    mut decoder: Box<dyn Decoder>,
    track_id: u32,
    ring: Arc<Ring>,
    shared: Option<Arc<SharedStream>>,
    loudness: Arc<TrackLoudness>,
    started_from_beginning: bool,
) {
    // 复用的交错采样缓冲：仅当声道/采样率变化或所需容量超过现有容量时才重建，消除旧版
    // 「每 packet 一次 `SampleBuffer::new`」的数十次/秒 malloc 抖动（macOS 高频大块分配的
    // 页保留是 RSS 虚高来源之一）。容量足够时直接复用——`copy_interleaved_ref` 按本包实际
    // 帧数写入，`samples()` 只返回本次写入长度，缓冲偏大不影响正确性。
    let mut sample_buf: Option<SampleBuffer<f32>> = None;
    // 当前缓冲的 (采样率, 声道, 容量帧数)，用于判断能否复用。
    let mut buf_meta: Option<(u32, symphonia::core::audio::Channels, u64)> = None;
    let mut terminal_failure = None;
    // 响度表头：仅当本次播放要测量时才建（关闭功能 = 零开销）。规格变化时重建。
    // 测量的是**入环前的原始信号**，与出环时施加的增益互不影响。
    let measuring = loudness.should_measure();
    let mut meter: Option<LoudnessMeter> = None;
    // 被取消（切歌 / 退出）与自然流末的区别：前者的测量只是部分覆盖，不能标 complete。
    let mut cancelled = false;
    let mut seeked = !started_from_beginning;
    let mut reported_eof = false;
    loop {
        if ring.inner.lock().unwrap().cancel {
            cancelled = true;
            break;
        }
        // 处理待办 seek：对解复用器执行 seek，并按 symphonia 约定重置解码器。环已由
        // request_seek 冲刷，故 seek 后从新位置解出的采样会接续填入。seek 失败（容器不支持/
        // 越界）则忽略，从当前位置继续——避免错误地 reset 解码器。
        if let Some(target) = ring.take_seek() {
            seeked = true;
            reported_eof = false;
            meter = None;
            if format
                .seek(
                    SeekMode::Coarse,
                    SeekTo::Time {
                        time: Time::from(target),
                        track_id: Some(track_id),
                    },
                )
                .is_ok()
            {
                decoder.reset();
            }
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            // 网络层会把 HTTP/响应体失败记录在 SharedStream；无底层错误的 IoError 是
            // symphonia 在自然 EOF 上的既有表现。
            Err(SymphoniaError::IoError(_)) | Err(SymphoniaError::ResetRequired) => {
                if let Some(message) = shared.as_ref().and_then(|stream| stream.error()) {
                    terminal_failure = Some(PlayerFailure::network(message));
                    break;
                }
                if let Some(m) = meter.as_ref() {
                    let (lufs, peak, secs) = m.snapshot();
                    loudness.record(lufs, peak, secs, !seeked);
                }
                reported_eof = true;
                if ring.wait_at_eof() {
                    continue;
                }
                cancelled = true;
                break;
            }
            Err(error) => {
                terminal_failure = Some(PlayerFailure::decode(format!("读取音频包失败: {error}")));
                break;
            }
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let cap = decoded.capacity() as u64;
                // spec 变化或现有容量不足才重建；否则复用（见上）。
                let need_new = match buf_meta {
                    Some((rate, ch, c)) => rate != spec.rate || ch != spec.channels || c < cap,
                    None => true,
                };
                if need_new {
                    sample_buf = Some(SampleBuffer::<f32>::new(cap, spec));
                    buf_meta = Some((spec.rate, spec.channels, cap));
                }
                let buf = sample_buf.as_mut().unwrap();
                buf.copy_interleaved_ref(decoded);
                // 先测后推：`push_slice` 会在环满时背压阻塞，测量放在它前面，
                // 增益就能领先播放位置数秒确定下来（解码本就跑在 sink 前方）。
                if measuring {
                    let ch = spec.channels.count().max(1) as u16;
                    if meter.as_ref().is_none_or(|m| !m.matches(ch, spec.rate)) {
                        meter = LoudnessMeter::new(ch, spec.rate);
                    }
                    if let Some(m) = meter.as_mut() {
                        if let Some((lufs, peak, secs)) = m.push(buf.samples()) {
                            loudness.record(lufs, peak, secs, false);
                        }
                    }
                }
                ring.push_slice(buf.samples());
            }
            // 单包错误跳过，不影响整体。
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(error)) => {
                terminal_failure = Some(
                    shared
                        .as_ref()
                        .and_then(|stream| stream.error())
                        .map(PlayerFailure::network)
                        .unwrap_or_else(|| PlayerFailure::decode(format!("解码读取失败: {error}"))),
                );
                break;
            }
            Err(error) => {
                terminal_failure = Some(PlayerFailure::decode(format!("解码失败: {error}")));
                break;
            }
        }
    }
    // 收尾快照：即使中途被切歌，部分覆盖的测量也值得回报（Dart 按覆盖率决定采不采用），
    // 只有自然读到流末尾才算完整测量。
    if !reported_eof {
        if let Some(m) = meter.as_ref() {
            let (lufs, peak, secs) = m.snapshot();
            loudness.record(
                lufs,
                peak,
                secs,
                !seeked && !cancelled && terminal_failure.is_none(),
            );
        }
    }
    if let Some(failure) = terminal_failure {
        ring.finish_with_error(failure);
    } else {
        ring.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── 输出假死恢复（见 specs/features/android-audio-output-disconnect.md）──

    #[test]
    fn stall_watchdog_only_fires_when_position_should_be_moving() {
        let long = OUTPUT_STALL_TIMEOUT + Duration::from_millis(1);
        // 该走却不走：唯一该报的情形。
        assert!(output_stalled(long, false, false));
        // 暂停时位置本来就不走。
        assert!(!output_stalled(long, true, false));
        // 环空是网络欠载（另有 8 秒超时那条路），不是输出坏了。
        assert!(!output_stalled(long, false, true));
        // 没到阈值不报：AAudio 冷启动、解码首包都要时间。
        assert!(!output_stalled(
            OUTPUT_STALL_TIMEOUT - Duration::from_millis(1),
            false,
            false
        ));
    }

    /// 自建 cpal 输出 → 混音器 → `Sink` 的接线是否真的在消费采样。
    ///
    /// 接错线的表现就是**静音**，而这正是本次要修的故障，光靠 `cargo check` 看不出来。
    /// `Sink::get_pos()` 数的是已被消费的采样数，所以它增长 = 音频回调确实在拉数据。
    /// 需要真实输出设备，故默认忽略；本地验证：
    /// `cargo test --lib -- --ignored output_pipeline_consumes_samples`
    #[test]
    #[ignore = "需要真实音频输出设备"]
    fn output_pipeline_consumes_samples() {
        let (notices, _notice_rx) = std::sync::mpsc::channel();
        let slot = open_output(&notices).expect("打开默认输出设备");
        let sink = new_sink(&slot);
        // 1 秒静音（听不见，但一样要被消费掉）。
        sink.append(rodio::buffer::SamplesBuffer::new(
            2,
            44_100,
            vec![0.0f32; 44_100 * 2],
        ));
        thread::sleep(Duration::from_millis(300));
        assert!(
            sink.get_pos() >= Duration::from_millis(100),
            "输出没有消费采样（get_pos={:?}）——混音器接线坏了，表现就是没有声音",
            sink.get_pos()
        );
        assert!(slot.is_alive(), "播放过程中输出流被标记为已断开");
    }

    #[test]
    fn rebind_limiter_spaces_out_and_caps_automatic_reopens() {
        let mut limiter = RebindLimiter::new();
        let t0 = Instant::now();
        assert!(limiter.allow(t0));
        // 最小间隔内的第二次不放行（系统连发多个断开事件时只重开一次）。
        assert!(!limiter.allow(t0 + REBIND_MIN_INTERVAL / 2));
        // 过了最小间隔可以再来，直到窗口额度用尽。
        let mut at = t0;
        let mut allowed = 1;
        for _ in 1..REBIND_MAX_IN_WINDOW + 3 {
            at += REBIND_MIN_INTERVAL;
            if limiter.allow(at) {
                allowed += 1;
            }
        }
        assert_eq!(allowed, REBIND_MAX_IN_WINDOW);
        // 窗口过去后额度自然恢复（设备真的没了不空转，但也不能永久放弃）。
        assert!(limiter.allow(at + REBIND_WINDOW + Duration::from_secs(1)));
    }

    #[test]
    fn rebind_limiter_reset_restores_budget() {
        let mut limiter = RebindLimiter::new();
        let mut at = Instant::now();
        for _ in 0..REBIND_MAX_IN_WINDOW {
            assert!(limiter.allow(at));
            at += REBIND_MIN_INTERVAL;
        }
        assert!(!limiter.allow(at));
        // 稳定播放满一个窗口后归还额度（run() 里由「位置持续增长」每跳调用）。
        limiter.note_progress(at + Duration::from_secs(1));
        assert!(
            !limiter.allow(at + Duration::from_secs(1)),
            "还没稳到一个窗口就还额度"
        );
        limiter.note_progress(at + REBIND_WINDOW + Duration::from_secs(1));
        assert!(limiter.allow(at + REBIND_WINDOW + Duration::from_secs(1)));
    }

    /// 构造一个不实际发请求的 SharedStream（测试仅走 prefix/extra/EOF 分支，不触发 Range）。
    /// 运行时须在测试期间存活，故一并返回由调用方持有。前缀改用 `write_prefix_for_test`
    /// 落盘写入（不再直接 push `Vec<u8>`）。
    fn mk() -> (tokio::runtime::Runtime, Arc<SharedStream>) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let handle = rt.handle().clone();
        let s = SharedStream::new(
            reqwest::Client::new(),
            "http://invalid.invalid/x".into(),
            Vec::new(),
            handle,
            None,
        )
        .unwrap();
        (rt, s)
    }

    #[test]
    fn read_hits_prefix() {
        let (_rt, s) = mk();
        s.write_prefix_for_test(&[1, 2, 3, 4, 5]);
        let mut buf = [0u8; 3];
        let n = s.read_at(1, &mut buf).unwrap();
        assert_eq!(n, 3);
        assert_eq!(&buf, &[2, 3, 4]);
    }

    #[test]
    fn read_hits_extra_segment() {
        let (_rt, s) = mk();
        s.write_prefix_for_test(&[0, 0, 0, 0]); // 前缀 0..4
        {
            let mut g = s.inner.lock().unwrap();
            g.total_len = Some(1024);
            g.extra.insert(512, vec![9, 8, 7, 6]); // 离散段 512..516
        }
        // pos=513 命中离散段，从段内偏移 1 起读。
        let mut buf = [0u8; 2];
        let n = s.read_at(513, &mut buf).unwrap();
        assert_eq!(n, 2);
        assert_eq!(&buf, &[8, 7]);
    }

    #[test]
    fn read_extra_miss_falls_through_to_eof() {
        // pos 落在已知离散段之外、且无总长（不触发 Range）、下载已结束 → EOF。
        let (_rt, s) = mk();
        s.write_prefix_for_test(&[1, 2, 3]);
        {
            let mut g = s.inner.lock().unwrap();
            g.extra.insert(100, vec![7, 7]); // 100..102，不覆盖 pos=50
            g.done = true;
        }
        let mut buf = [0u8; 4];
        assert_eq!(s.read_at(50, &mut buf).unwrap(), 0);
    }

    #[test]
    fn read_clamps_at_prefix_boundary() {
        // 读请求起点在前缀内、但长度越过已落盘边界：只返回边界内的字节数（不越界读文件）。
        let (_rt, s) = mk();
        s.write_prefix_for_test(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]); // 前缀 0..10
        let mut buf = [0xEEu8; 5]; // 预置哨兵，验证只写入 2 字节
        let n = s.read_at(8, &mut buf).unwrap();
        assert_eq!(n, 2); // 只剩 8,9 两字节
        assert_eq!(&buf[..2], &[8, 9]);
        assert_eq!(&buf[2..], &[0xEE, 0xEE, 0xEE]); // 边界外未被触碰
    }

    #[test]
    fn read_returns_correct_bytes_after_spill() {
        // 前缀落盘后从中段 pread：应精确返回原字节（验证 spill + pread 定位读正确）。
        let (_rt, s) = mk();
        let data: Vec<u8> = (0..2000u32).map(|i| (i % 256) as u8).collect();
        s.write_prefix_for_test(&data);
        let mut buf = [0u8; 100];
        let n = s.read_at(1000, &mut buf).unwrap();
        assert_eq!(n, 100);
        assert_eq!(&buf[..], &data[1000..1100]);
    }

    /// 起一个只回一次固定响应的本地 HTTP 服务，返回其 URL（线程随连接结束自行退出）。
    fn serve_once(response: &'static str) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/song.mp3", listener.local_addr().unwrap());
        thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf);
                let _ = sock.write_all(response.as_bytes());
                let _ = sock.flush();
            }
        });
        url
    }

    /// 起一个按顺序应答 N 个连接的本地 HTTP 服务，用来演练断点重连。
    /// 返回 URL 与「收到的请求头文本」列表（供断言重连确实带上了 `Range`）。
    #[allow(clippy::type_complexity)]
    fn serve_sequence(responses: Vec<&'static str>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/song.mp3", listener.local_addr().unwrap());
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let recorder = seen.clone();
        thread::spawn(move || {
            for response in responses {
                let Ok((mut sock, _)) = listener.accept() else {
                    return;
                };
                let mut buf = [0u8; 1024];
                let n = sock.read(&mut buf).unwrap_or(0);
                recorder
                    .lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf[..n]).into_owned());
                let _ = sock.write_all(response.as_bytes());
                let _ = sock.flush();
                // drop(sock) 即关闭连接——响应体短于 Content-Length 时这就是「被截断」。
            }
        });
        (url, seen)
    }

    fn run_download(url: String) -> (Arc<SharedStream>, Result<(), String>) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let handle = rt.handle().clone();
        let shared =
            SharedStream::new(reqwest::Client::new(), url, Vec::new(), handle, None).unwrap();
        let head = rt.block_on(async {
            let (tx, rx) = oneshot::channel();
            download_task(shared.clone(), tx).await;
            rx.await.unwrap()
        });
        (shared, head)
    }

    #[test]
    fn download_head_reports_http_failure() {
        // 地址不可用时必须经 head 通道立刻回报，open_stream 才能向上报错让 Dart 换下一候选地址；
        // 只把错误写进 SharedStream 是不够的——有 `.part` 前缀时首包能从本地解出来，错误会被掩盖。
        let (shared, head) = run_download(serve_once(
            "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n",
        ));
        let msg = head.expect_err("403 应回报失败");
        assert!(msg.contains("403"), "{msg}");
        assert_eq!(
            shared.inner.lock().unwrap().err.as_deref(),
            Some(msg.as_str())
        );
    }

    #[test]
    fn download_head_reports_success() {
        let (shared, head) = run_download(serve_once(
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabc",
        ));
        assert!(head.is_ok());
        let g = shared.inner.lock().unwrap();
        assert_eq!(g.err, None);
        assert_eq!(g.prefix_len, 3);
    }

    /// **本次修复的核心保证**：连接被中途掐断时，下载层自己带着 `Range` 续上，
    /// 流水线一动不动。抖动只要上交到 Dart，就只能拆掉重建 → 倒回一两秒重播。
    #[test]
    fn download_resumes_after_a_truncated_body() {
        let (url, seen) = serve_sequence(vec![
            // 声明 6 字节却只给 3 个就断开。
            "HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\nabc",
            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 3-5/6\r\nContent-Length: 3\r\n\r\ndef",
        ]);
        let (shared, head) = run_download(url);

        assert!(head.is_ok(), "首连成功过，不该回报地址失败");
        let g = shared.inner.lock().unwrap();
        assert_eq!(g.err, None, "抖动被下载层吸收，不该冒到上层");
        assert_eq!(g.prefix_len, 6);
        drop(g);

        let mut buf = [0u8; 6];
        assert_eq!(shared.read_at(0, &mut buf).unwrap(), 6);
        assert_eq!(
            &buf, b"abcdef",
            "续传的字节必须接在断点后面，不能重叠或错位"
        );

        // hyper 发出的头名是小写的，故统一小写后再比。
        let seen: Vec<String> = seen
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.to_lowercase())
            .collect();
        assert_eq!(seen.len(), 2, "应当只重连一次");
        assert!(!seen[0].contains("range:"), "首连不带 Range");
        assert!(
            seen[1].contains("range: bytes=3-"),
            "重连必须从断点续传，实际请求：{}",
            seen[1]
        );
    }

    /// 重连撞上地址级失败（URL 过期 / 鉴权掉了）时不再空转：判死交给 Dart 重新问插件要地址。
    #[test]
    fn download_reconnect_stops_at_a_fatal_status() {
        let (url, _seen) = serve_sequence(vec![
            "HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\nabc",
            "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n",
        ]);
        let (shared, head) = run_download(url);

        assert!(head.is_ok(), "首连已经成功，head 不该被改写");
        let err = shared.error().expect("重连拿到 403 应判死");
        assert!(err.contains("403"), "{err}");
    }

    #[test]
    fn read_reports_error_after_failed_download() {
        let (_rt, s) = mk();
        {
            let mut g = s.inner.lock().unwrap();
            g.err = Some("boom".into());
            g.done = true;
        }
        let mut buf = [0u8; 4];
        assert!(s.read_at(0, &mut buf).is_err());
    }

    #[test]
    fn finish_marks_known_length_short_read_as_network_error() {
        let (_rt, s) = mk();
        s.set_total_len(10);
        s.write_prefix_for_test(&[1, 2, 3]);

        s.finish();

        let error = s.error().unwrap();
        assert!(error.contains("音频流提前结束"));
        assert!((s.buffered_fraction() - 0.3).abs() < 1e-9);
    }

    #[test]
    fn buffered_fraction_boundaries() {
        let (_rt, s) = mk();
        // 总长未知且未 done → 0.0。
        assert_eq!(s.buffered_fraction(), 0.0);
        {
            let mut g = s.inner.lock().unwrap();
            g.total_len = Some(1000);
        }
        s.write_prefix_for_test(&vec![0u8; 250]);
        // 部分：250 / 1000 = 0.25。
        assert!((s.buffered_fraction() - 0.25).abs() < 1e-9);
        {
            s.inner.lock().unwrap().done = true;
        }
        // done 后恒 1.0（即便前缀未达 total）。
        assert_eq!(s.buffered_fraction(), 1.0);
    }

    #[test]
    fn known_total_len_first_write_wins() {
        let (_rt, s) = mk();
        assert_eq!(s.known_total_len(), None);
        s.set_total_len(4096);
        assert_eq!(s.known_total_len(), Some(4096));
        // Content-Length 仅首次生效。
        s.set_total_len(9999);
        assert_eq!(s.known_total_len(), Some(4096));
    }

    /// 网络抖一下：吐静音、原地等，**流水线不能死**。死了就要上交 Dart 重建，
    /// 而重建只能靠 startSecs 重新定位——那正是「倒回一两秒重播」的来源。
    #[test]
    fn brief_underrun_recovers_in_place_without_ending_the_stream() {
        let ring = Ring::new_with_timeout(8, Duration::from_millis(200));
        let epoch = ring.current_source_epoch();
        let mut out = Vec::new();

        assert!(matches!(
            ring.pop_chunk(&mut out, 8, epoch, 1, 0),
            RingRead::Underrun(_)
        ));
        assert!(ring.is_buffering());

        ring.push_slice(&[0.25, 0.5]);
        assert!(matches!(
            ring.pop_chunk(&mut out, 8, epoch, 1, 0),
            RingRead::Data
        ));
        assert_eq!(out, vec![0.25, 0.5]);
        assert!(!ring.is_buffering());
        assert!(ring.terminal_failure().is_none());
        assert!(
            ring.inner.lock().unwrap().underrun_since.is_none(),
            "取到数据就该归零判死计时，下一次短欠载不能继承上一段的账"
        );
    }

    /// 欠载**绝不能阻塞**：`pop_chunk` 跑在音频回调线程上，挂住它会让 CoreAudio /
    /// AAudio 判过载甚至断流，最后绕到「重绑输出」，把网络抖动误诊成设备故障。
    #[test]
    fn underrun_returns_immediately_instead_of_blocking() {
        let ring = Ring::new_with_timeout(8, Duration::from_secs(60));
        let epoch = ring.current_source_epoch();
        let mut out = Vec::new();

        let started = Instant::now();
        assert!(matches!(
            ring.pop_chunk(&mut out, 8, epoch, 1, 0),
            RingRead::Underrun(_)
        ));
        assert!(
            started.elapsed() < Duration::from_millis(50),
            "欠载路径挂了 {:?}——音频回调被阻塞",
            started.elapsed()
        );
    }

    /// 只有**持续**欠载才判死。单次调用只是起表，抖一下不该让曲目直接报错。
    #[test]
    fn sustained_underrun_finally_reports_network_stall() {
        let ring = Ring::new_with_timeout(8, Duration::from_millis(40));
        let epoch = ring.current_source_epoch();
        let mut out = Vec::new();

        assert!(matches!(
            ring.pop_chunk(&mut out, 8, epoch, 1, 0),
            RingRead::Underrun(_)
        ));
        assert!(ring.terminal_failure().is_none(), "第一次只是起表");

        thread::sleep(Duration::from_millis(60));
        assert!(matches!(
            ring.pop_chunk(&mut out, 8, epoch, 1, 0),
            RingRead::End
        ));
        let failure = ring.terminal_failure().unwrap();
        assert_eq!(failure.kind, PlayerFailureKind::Network);
        assert!(failure.message.contains("缓冲超时"));
    }

    /// 恢复迟滞：没有它，弱网下每进来一个 packet 就出一小段声，听感是持续的断续「咔咔」。
    #[test]
    fn underrun_recovery_waits_for_the_prebuffer_gate() {
        let ring = Ring::new(64);
        let epoch = ring.current_source_epoch();
        let mut out = Vec::new();

        assert!(matches!(
            ring.pop_chunk(&mut out, 64, epoch, 2, 16),
            RingRead::Underrun(_)
        ));
        // 只补了半个门限：继续静音。
        ring.push_slice(&[0.5; 8]);
        assert!(matches!(
            ring.pop_chunk(&mut out, 64, epoch, 2, 16),
            RingRead::Underrun(_)
        ));
        // 攒够门限 → 恢复出声。
        ring.push_slice(&[0.5; 8]);
        assert!(matches!(
            ring.pop_chunk(&mut out, 64, epoch, 2, 16),
            RingRead::Data
        ));
        assert_eq!(out.len(), 16);
        assert!(!ring.is_buffering());
    }

    /// 收尾不设门限：最后不足一秒的尾巴必须放完，否则曲终判定跟着卡住。
    #[test]
    fn end_of_stream_drains_below_the_prebuffer_gate() {
        let ring = Ring::new(64);
        let epoch = ring.current_source_epoch();
        let mut out = Vec::new();

        assert!(matches!(
            ring.pop_chunk(&mut out, 64, epoch, 2, 32),
            RingRead::Underrun(_)
        ));
        ring.push_slice(&[0.5; 4]);
        ring.finish();

        assert!(matches!(
            ring.pop_chunk(&mut out, 64, epoch, 2, 32),
            RingRead::Data
        ));
        assert_eq!(out.len(), 4);
    }

    /// 静音必须记账并在 seek 时作废，否则「没声音但进度条照走」/「位置倒退」。
    #[test]
    fn completed_decoder_waits_for_seek_and_cancellation_without_false_eof() {
        let ring = Ring::new_local(64);
        let worker = ring.clone();
        let ready = std::sync::Arc::new(std::sync::Barrier::new(2));
        let started = ready.clone();
        let thread = std::thread::spawn(move || {
            started.wait();
            worker.wait_at_eof()
        });
        ready.wait();
        ring.request_seek(0.5);
        assert!(thread.join().unwrap());
        assert!(!ring.is_drained());
        assert_eq!(ring.take_seek(), Some(0.5));
        ring.push_slice(&[0.3; 8]);
        let mut out = Vec::new();
        assert!(matches!(
            ring.pop_chunk(&mut out, 8, ring.current_source_epoch(), 1, 0),
            RingRead::Data
        ));
        let worker = ring.clone();
        let thread = std::thread::spawn(move || worker.wait_at_eof());
        ring.cancel();
        assert!(!thread.join().unwrap());
    }

    #[test]
    fn silence_is_counted_and_cleared_on_seek() {
        let ring = Ring::new(64);
        let epoch = ring.current_source_epoch();
        let mut out = Vec::new();

        let RingRead::Underrun(n) = ring.pop_chunk(&mut out, 64, epoch, 2, 0) else {
            panic!("空环应吐静音");
        };
        assert_eq!(n % 2, 0, "静音也必须帧对齐，否则左右声道会串位");
        ring.note_silence(n as u64);
        assert_eq!(ring.silence_samples(), n as u64);

        // seek 后 rodio 把计时基准重设到目标位置，旧静音账随之作废。
        ring.request_seek(12.0);
        assert_eq!(ring.silence_samples(), 0);
        assert!(ring.inner.lock().unwrap().underrun_since.is_none());
    }

    #[test]
    fn silence_ms_converts_interleaved_samples() {
        // 立体声 44.1kHz：半秒静音 = 44100 个交错采样。
        assert_eq!(silence_ms(44_100, 44_100, 2), 500);
        assert_eq!(silence_ms(0, 44_100, 2), 0);
        // 速率非法时不扣（宁可不扣，也不能让位置乱跳）。
        assert_eq!(silence_ms(1000, 0, 2), 0);
    }

    /// 音频回调路径的端到端保证：环空时 `RingSource` 必须立刻给出采样。
    #[test]
    fn ring_source_yields_silence_instead_of_blocking_on_underrun() {
        let ring = Ring::new_with_timeout(64, Duration::from_secs(60));
        let loudness = Arc::new(TrackLoudness::new(
            LoudnessSettings::default(),
            "plugin\u{1f}track".into(),
        ));
        let effects = Arc::new(EffectsConfig::new());
        let mut src = new_source(&ring, 2, 44_100, &loudness, &effects);

        let started = Instant::now();
        let first: Vec<f32> = (0..8)
            .map(|_| src.next().expect("欠载不该终止流"))
            .collect();
        assert!(
            started.elapsed() < Duration::from_millis(50),
            "回调被挂住了"
        );
        assert!(first.iter().all(|s| *s == 0.0), "欠载应输出静音");
        assert!(
            ring.silence_samples() >= 8,
            "静音必须记账，否则缓冲期间进度条会空走"
        );
    }

    #[test]
    fn failed_stream_is_not_reported_as_finished() {
        let ring = Ring::new(8);
        ring.finish_with_error(PlayerFailure::network("读取音频流失败"));

        let event = terminal_player_event(&ring);
        assert!(matches!(
            event,
            PlayerEvent::Error(PlayerFailure {
                kind: PlayerFailureKind::Network,
                ..
            })
        ));
    }

    #[test]
    fn retired_source_ends_immediately_without_ending_the_pipeline() {
        // 重绑输出的核心保证：旧 source 立刻收敛，但环本身不能被标记成 done/cancel。
        let ring = Ring::new_with_timeout(8, Duration::from_secs(60));
        let stale = ring.current_source_epoch();
        ring.retire_sources();

        let mut out = Vec::new();
        assert!(matches!(
            ring.pop_chunk(&mut out, 8, stale, 1, 0),
            RingRead::End
        ));
        assert!(
            ring.terminal_failure().is_none(),
            "退休不是故障，不得记欠载"
        );
        let g = ring.inner.lock().unwrap();
        assert!(!g.done && !g.cancel, "流水线必须原样存活");
    }

    #[test]
    fn retired_source_does_not_steal_samples_from_its_successor() {
        // 旧 source 退休后若还能 drain 一次，那段 PCM 就被吞掉且永远发不出声。
        let ring = Ring::new(8);
        let stale = ring.current_source_epoch();
        ring.retire_sources();
        let fresh = ring.current_source_epoch();
        ring.push_slice(&[0.1, 0.2]);

        let mut out = Vec::new();
        assert!(matches!(
            ring.pop_chunk(&mut out, 8, stale, 1, 0),
            RingRead::End
        ));
        assert!(out.is_empty());

        assert!(matches!(
            ring.pop_chunk(&mut out, 8, fresh, 1, 0),
            RingRead::Data
        ));
        assert_eq!(out, vec![0.1, 0.2]);
    }

    #[test]
    fn paused_underrun_does_not_accrue_the_give_up_clock() {
        let ring = Ring::new_with_timeout(8, Duration::from_millis(20));
        ring.set_paused(true);
        let epoch = ring.current_source_epoch();
        let mut out = Vec::new();

        for _ in 0..3 {
            assert!(matches!(
                ring.pop_chunk(&mut out, 8, epoch, 1, 0),
                RingRead::Underrun(_)
            ));
            thread::sleep(Duration::from_millis(15));
        }
        assert!(!ring.is_buffering(), "暂停不是缓冲");
        assert!(
            ring.terminal_failure().is_none(),
            "暂停一小时也不该被判成网络欠载"
        );

        ring.set_paused(false);
        ring.push_slice(&[0.75]);
        assert!(matches!(
            ring.pop_chunk(&mut out, 8, epoch, 1, 0),
            RingRead::Data
        ));
        assert_eq!(out, vec![0.75]);
    }

    /// 帧对齐：块边界切在半帧上会让立体声左右声道逐渐串位（声像慢慢偏移，极难排查）。
    #[test]
    fn pop_chunk_returns_whole_frames() {
        let ring = Ring::new(64);
        let epoch = ring.current_source_epoch();
        let mut out = Vec::new();

        // 立体声：9 个采样只能取 8 个（4 帧），剩 1 个留给下一块。
        ring.push_slice(&[0.5f32; 9]);
        assert!(matches!(
            ring.pop_chunk(&mut out, 64, epoch, 2, 0),
            RingRead::Data
        ));
        assert_eq!(out.len(), 8, "align = 2 时返回长度必为偶数");

        // 5.1：20 个采样只能取 18 个（3 帧）。LOCAL_CHUNK_SAMPLES 对 6 不整除，
        // 这条路径在真实播放里会走到。
        let ring = Ring::new(64);
        let epoch = ring.current_source_epoch();
        ring.push_slice(&[0.5f32; 20]);
        assert!(matches!(
            ring.pop_chunk(&mut out, 64, epoch, 6, 0),
            RingRead::Data
        ));
        assert_eq!(out.len(), 18, "align = 6 时返回长度必为 6 的倍数");
    }

    #[test]
    fn pop_chunk_drains_partial_frame_at_end_of_stream() {
        // 流已结束时残留不足一帧仍要取完，否则消费者卡在「环非空但取不出」上活锁。
        let ring = Ring::new(64);
        let epoch = ring.current_source_epoch();
        ring.push_slice(&[0.1, 0.2, 0.3]);
        ring.finish();

        let mut out = Vec::new();
        assert!(matches!(
            ring.pop_chunk(&mut out, 64, epoch, 2, 0),
            RingRead::Data
        ));
        assert_eq!(out, vec![0.1, 0.2]);
        assert!(matches!(
            ring.pop_chunk(&mut out, 64, epoch, 2, 0),
            RingRead::Data
        ));
        assert_eq!(out, vec![0.3], "收尾的半帧也要取走");
        assert!(matches!(
            ring.pop_chunk(&mut out, 64, epoch, 2, 0),
            RingRead::End
        ));
    }

    #[test]
    fn partial_frame_yields_silence_instead_of_a_split_frame() {
        // 流还活着时，不足一帧不能返回半帧（会串位），而应吐静音等生产者补齐。
        let ring = Ring::new_with_timeout(64, Duration::from_secs(60));
        let epoch = ring.current_source_epoch();
        let mut out = Vec::new();

        ring.push_slice(&[0.1]);
        assert!(matches!(
            ring.pop_chunk(&mut out, 64, epoch, 2, 0),
            RingRead::Underrun(_)
        ));

        ring.push_slice(&[0.2]);
        assert!(matches!(
            ring.pop_chunk(&mut out, 64, epoch, 2, 0),
            RingRead::Data
        ));
        assert_eq!(out, vec![0.1, 0.2]);
    }

    /// Ogg 首页（27 字节页头 + 1 字节段表）后跟编码标识包，正是 sniff 认编码的位置。
    fn ogg_head(codec: &[u8]) -> Vec<u8> {
        let mut v = b"OggS\x00\x02".to_vec();
        v.extend_from_slice(&[0u8; 20]); // granule/serial/seq/crc
        v.push(1); // 段数
        v.push(codec.len() as u8); // 段表
        v.extend_from_slice(codec);
        v
    }

    /// 编译进来的解码器集合就是「我们支持什么格式」的事实来源：
    /// [`sniff_container`] 的判断、`local_scan` 的扫描白名单都以它为准，
    /// 谁把 `Cargo.toml` 的 symphonia feature 删了，这里先炸。
    #[test]
    fn compiled_codecs_match_supported_formats() {
        use symphonia::core::codecs::{
            CODEC_TYPE_AAC, CODEC_TYPE_FLAC, CODEC_TYPE_MP3, CODEC_TYPE_OPUS, CODEC_TYPE_VORBIS,
        };
        let codecs = symphonia::default::get_codecs();
        for c in [
            CODEC_TYPE_MP3,
            CODEC_TYPE_FLAC,
            CODEC_TYPE_AAC,
            CODEC_TYPE_VORBIS,
        ] {
            assert!(codecs.get_codec(c).is_some(), "缺解码器: {c:?}");
        }
        // symphonia 0.5 没有 Opus 解码器，sniff_container 报「不支持」正是基于此。
        assert!(codecs.get_codec(CODEC_TYPE_OPUS).is_none());
    }

    /// EBML 头（WebM）必须被认出来：这正是 2026-08-11 那首「扩展名是 .mp3、内容是
    /// YouTube webm/opus」的文件，symphonia 只会含糊地报「不支持的音频编码」。
    #[test]
    fn sniff_webm_and_friends() {
        let webm = b"\x1a\x45\xdf\xa3\x9f\x42\x86\x81\x01\x42\xf7\x81\x01\x42\xf2\x81";
        assert!(sniff_container(webm).unwrap().contains("WebM"));
        // Ogg 得看编码：Vorbis 解得了（None，失败按截断处理），Opus / Speex 才是不支持。
        assert!(sniff_container(&ogg_head(b"OpusHead"))
            .unwrap()
            .contains("Opus"));
        assert!(sniff_container(&ogg_head(b"Speex   "))
            .unwrap()
            .contains("Speex"));
        assert_eq!(sniff_container(&ogg_head(b"\x01vorbis")), None);
        assert_eq!(sniff_container(b"OggS\0\x02\0\0\0\0\0\0\0\0"), None);
        assert_eq!(sniff_container(b"RIFF\0\0\0\0WAVEfmt "), Some("WAV"));
        // 认不出来的（含真 mp3/flac）返回 None，交给原始报错。
        assert_eq!(sniff_container(b"fLaC\0\0\0\x22\x12\x00\x12\x00"), None);
        assert_eq!(sniff_container(b"\xff\xfb\x90\x64\0\0\0\0\0\0\0\0"), None);
        assert_eq!(sniff_container(b"short"), None);
    }

    /// 认出容器 → UnsupportedAudio（上层据此删缓存对象）；认不出 → 普通错误
    /// （可能只是流被截断，`.part` 前缀要留给续传）。
    #[test]
    fn only_recognized_containers_are_unsupported_audio() {
        let webm = b"\x1a\x45\xdf\xa3\x9f\x42\x86\x81\x01\x42\xf7\x81\x01\x42\xf2\x81";
        let e = decode_prepare_error(webm, "无法识别音频格式: x".into());
        assert!(e.downcast_ref::<UnsupportedAudio>().is_some());
        assert!(e.to_string().contains("WebM"));

        let e = decode_prepare_error(
            b"\xff\xfb\x90\x64\0\0\0\0\0\0\0\0",
            "音频流过短或为空".into(),
        );
        assert!(e.downcast_ref::<UnsupportedAudio>().is_none());
        assert_eq!(e.to_string(), "音频流过短或为空");
    }

    /// 本地缓存文件走同一套判定（`playLocal` 命中一个内容不对的对象时）。
    #[test]
    fn prepare_decode_file_reports_container() {
        let path = std::env::temp_dir().join(format!(
            "haiku_webm_probe_{}.mp3",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        // EBML 头 + 填充：足够让 probe 失败并让 sniff 认出容器。
        let mut bytes =
            b"\x1a\x45\xdf\xa3\x9f\x42\x86\x81\x01\x42\xf7\x81\x01\x42\xf2\x81".to_vec();
        bytes.extend(std::iter::repeat(0u8).take(64 * 1024));
        std::fs::write(&path, &bytes).unwrap();

        let err = prepare_decode_file(
            path.display().to_string(),
            Ring::new(1024),
            Arc::new(TrackLoudness::new(
                LoudnessSettings::default(),
                String::new(),
            )),
        )
        .unwrap_err();
        let _ = std::fs::remove_file(&path);

        assert!(err.downcast_ref::<UnsupportedAudio>().is_some(), "{err}");
        assert!(err.to_string().contains("WebM"), "{err}");
    }
}

#[cfg(test)]
mod session_transport_tests {
    use super::*;
    #[test]
    fn queued_events_keep_the_generation_at_emission() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let generation = Arc::new(AtomicU64::new(7));
        let sender = SessionSender {
            tx,
            generation: generation.clone(),
            snapshot: Arc::new(Mutex::new(())),
        };
        sender.send(PlayerEvent::Finished).unwrap();
        generation.store(8, Ordering::Release);
        sender
            .send(PlayerEvent::Error(PlayerFailure::network("new")))
            .unwrap();
        assert_eq!(rx.try_recv().unwrap().generation, 7);
        assert_eq!(rx.try_recv().unwrap().generation, 8);
    }
}
