//! 播放音效：10 段图示均衡器 + 立体声宽度/声道平衡 + 软削波 + 防爆音淡入淡出。
//!
//! 与 [`crate::loudness`] 同挂在 `RingSource` 的出环路径上（解码后、进 sink 前的交错
//! f32 PCM），但两者定位正交：响度均衡是**每曲**的静态增益（测量驱动），本模块是
//! **全局**的用户偏好（设置驱动）。链路顺序固定：
//!
//! ```text
//! [响度增益] → [前置增益] → [10 段 biquad] → [立体声宽度/平衡] → [淡入淡出] → [软削波] → sink
//! ```
//!
//! 顺序不可换：前置增益必须在 EQ 之前，否则 EQ 的中间态已经溢出，再压回来也救不了；
//! 淡入淡出必须在软削波之前（fade 是最后一道闸门，且淡出末端的极小值不该再走一遍
//! tanh 分支）；软削波在最末端，保证送进 sink 的采样绝对值 ≤ 1.0（sink 上的用户音量
//! 只会再变小）。
//!
//! 三条约束贯穿本模块的所有设计：
//!
//! 1. **音频回调路径绝不上锁**：配置走「原子 version + Mutex 快照」协议
//!    （见 [`EffectsConfig`]），稳态下消费端每块只做一次 Relaxed 原子读。
//! 2. **绝不跳变**：所有用户可调参数都对**目标值**做斜坡，宽度/平衡的最终增益再在块内
//!    按帧线性插值，杜绝 zipper noise。
//! 3. **绝不 panic**：热路径不做可失败的索引与 unwrap（锁中毒也按 `into_inner` 兜住）。

use std::f64::consts::PI;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// 均衡器段数。
pub const BAND_COUNT: usize = 10;

/// 固定 ISO 频点。首末段用搁架、中间用峰值：31Hz 用 peaking 在 44.1k 下 Q 太窄，
/// 只顶起一条极窄的低频，听感不像「低音」；16k 则接近部分采样率的 Nyquist。
pub const BAND_FREQS: [f32; BAND_COUNT] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];

/// 单段增益范围（dB）。
pub const BAND_GAIN_MIN_DB: f32 = -12.0;
/// 单段增益范围（dB）。
pub const BAND_GAIN_MAX_DB: f32 = 12.0;

/// 峰值段的 Q：1 倍频程带宽（`Q = √(2^N)/(2^N−1)`，N=1）——与相邻频点的 1 倍频程
/// 间隔匹配，10 段拼起来才是一条连续可控的曲线而非 10 个孤立的尖峰。
const PEAK_Q: f64 = std::f64::consts::SQRT_2;

/// 增益视为「中性」的阈值（dB）。低于此值的段直接置 `None` 直通，省掉 5 次乘加；
/// 也是斜坡收敛后判定旁路的依据。
const GAIN_EPS_DB: f32 = 0.01;

/// 每段增益 / 前置增益的斜坡速率（dB/s）。拖满 ±12 约 0.5s 到位，听感是「平滑推上去」。
const GAIN_SLEW_DB_PER_SEC: f32 = 24.0;
/// 宽度 / 平衡的斜坡速率（每秒的无量纲变化量）。0→2 约 0.5s。
const STEREO_SLEW_PER_SEC: f32 = 4.0;

/// 斜坡进行中，一次最多处理多少帧就重算一遍系数。
///
/// **这是踩出来的坑**：TDF-II 的状态字 `z1/z2` 是与当时的系数绑定的中间量，直接换一组
/// 系数会让下一个样本的输出立刻跳到 `b0_new·x + z1_old`——与新旧任一稳态都不同。
/// 按整块（4096 帧 ≈ 93ms）重算时，24 dB/s 的斜坡一块就走 2.2 dB，实测在 0.3 的直流上
/// 造成 0.15 的单样本落差（约 3.5 dB 的台阶），拖滑块时听得见「咔咔」。
/// 切到 128 帧（≈2.9ms）后每次只走 0.07 dB，落差降到 5e-3 量级，听不出来。
///
/// 只在**参数确实在动**的那零点几秒里生效（每 2.9ms 20 次 sin/cos，折合 <0.1% CPU），
/// 稳态一次都不多算。
const RAMP_SPAN_FRAMES: usize = 128;

/// 淡入淡出全程时长（秒）。8ms 足以消掉半波截断的咔哒，又短到听不出「渐弱」。
const FADE_SECS: f32 = 0.008;

/// 播放线程置 `fade_target = 0` 后、执行 pause/stop/切歌前应等待的时长。
///
/// 比 [`FADE_SECS`] 略长：音频回调是按块推进 fade 的，留一块的余量才能保证闸门确实关到底。
/// 这个 sleep 落在**播放线程**（本就是 150ms 轮询节奏），不是音频回调，无副作用；
/// 代价是暂停时丢掉约 12ms 音频（已作为渐弱送出去了），不可闻。
pub const FADE_SLEEP: Duration = Duration::from_millis(12);

/// 软削波阈值。低于此值完全线性——绝大多数采样走这条分支，零代价。
const CLIP_THRESH: f32 = 0.95;

/// 非规格化数（denormal）冲刷阈值。淡出到静音后滤波器状态会指数衰减进 denormal 区，
/// 某些平台上会触发极慢的浮点微码路径（实测可让音频回调错过 deadline）。
/// 每块 20 次比较即可根除，**不要**用注入 DC 微噪的做法（那是往信号里掺垃圾）。
const DENORMAL_FLOOR: f64 = 1e-20;

// ===================== biquad =====================

/// 归一化后的双二阶系数（已除以 `a0`）。
///
/// 全部用 f64：31Hz @ 48kHz 的极点极靠近单位圆，f32 系数与状态会有可闻的量化噪声本底，
/// 而 f64 的额外开销在 20 个 biquad 的规模下测不出来。
#[derive(Clone, Copy, Debug)]
struct Coeffs {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

/// Transposed Direct Form II 的两个状态字。
#[derive(Default, Clone, Copy, Debug)]
struct BiquadState {
    z1: f64,
    z2: f64,
}

/// TDF-II 单样本推进。相比 DF-I 只需两个状态字，且对系数量化更不敏感。
#[inline]
fn tick(c: &Coeffs, s: &mut BiquadState, x: f64) -> f64 {
    let y = c.b0 * x + s.z1;
    s.z1 = c.b1 * x - c.a1 * y + s.z2;
    s.z2 = c.b2 * x - c.a2 * y;
    y
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BandKind {
    LowShelf,
    Peaking,
    HighShelf,
}

fn band_kind(index: usize) -> BandKind {
    if index == 0 {
        BandKind::LowShelf
    } else if index + 1 >= BAND_COUNT {
        BandKind::HighShelf
    } else {
        BandKind::Peaking
    }
}

/// RBJ Audio EQ Cookbook 系数设计。返回 `None` 表示该段直通：
///
/// - 增益小于 [`GAIN_EPS_DB`]（听不出来的量，不值得每样本 5 次乘加）；
/// - **Nyquist 守卫**：`f0 > 0.45·Fs`。32kHz 采样率下 16kHz 正好是 Nyquist，
///   不守卫会算出退化系数（`cos(w0) = −1`）导致滤波器发散出啸叫；
/// - 数值异常（`a0` 近零 / 非有限）——理论上不会发生，兜住以免把 NaN 送进 sink。
fn design(kind: BandKind, f0: f64, gain_db: f64, fs: f64) -> Option<Coeffs> {
    if !gain_db.is_finite() || gain_db.abs() < GAIN_EPS_DB as f64 {
        return None;
    }
    // NaN 采样率/频点会让下面所有比较都为假，故用「必须落在合法区间」的正向判断，
    // 而不是排除法。
    if !(fs.is_finite() && fs > 0.0 && f0 > 0.0 && f0 <= 0.45 * fs) {
        return None;
    }
    let a = 10f64.powf(gain_db / 40.0);
    let w0 = 2.0 * PI * f0 / fs;
    let (sin_w0, cos_w0) = w0.sin_cos();

    let (b0, b1, b2, a0, a1, a2) = match kind {
        BandKind::Peaking => {
            let alpha = sin_w0 / (2.0 * PEAK_Q);
            (
                1.0 + alpha * a,
                -2.0 * cos_w0,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos_w0,
                1.0 - alpha / a,
            )
        }
        // 搁架用 S = 1（`alpha = sinw0/2 · √2`）：最大坡度而不过冲。
        BandKind::LowShelf => {
            let alpha = sin_w0 / 2.0 * std::f64::consts::SQRT_2;
            let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
                a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
                (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
            )
        }
        BandKind::HighShelf => {
            let alpha = sin_w0 / 2.0 * std::f64::consts::SQRT_2;
            let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
                a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
                (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
            )
        }
    };

    if !a0.is_finite() || a0.abs() < 1e-12 {
        return None;
    }
    let c = Coeffs {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    };
    if !(c.b0.is_finite()
        && c.b1.is_finite()
        && c.b2.is_finite()
        && c.a1.is_finite()
        && c.a2.is_finite())
    {
        return None;
    }
    Some(c)
}

// ===================== 末端非线性 =====================

/// 软削波：阈值以下完全线性，之上按 tanh 压缩，输出上界恰为 1.0。
///
/// 刻意**不做真·限幅器**（快 attack / 慢 release）：那会引入动态压缩，与「响度均衡
/// 不压缩曲内动态」的既定理念冲突。这里只兜住自动前置增益留不住的那点余量（见
/// [`auto_preamp_db`]）。
#[inline]
fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= CLIP_THRESH {
        return x;
    }
    let over = (a - CLIP_THRESH) / (1.0 - CLIP_THRESH);
    x.signum() * (CLIP_THRESH + (1.0 - CLIP_THRESH) * over.tanh())
}

/// 声道平衡增益。**只衰减、不提升**，因此不引入任何削波风险。
#[inline]
fn balance_gains(balance: f32) -> (f32, f32) {
    let l = if balance > 0.0 { 1.0 - balance } else { 1.0 };
    let r = if balance < 0.0 { 1.0 + balance } else { 1.0 };
    (l, r)
}

/// 自动前置增益（dB）：把最高的那段提升整体压回来。
///
/// 保守但**不保证绝对够**：相邻段（1 倍频程间隔、Q=1.414）有重叠，两段同为 +12 时
/// 在中间频点可叠到约 +14 dB，仍会超出约 2 dB，由末端软削波兜住。这是刻意的取舍——
/// 按最坏情况留余量会让常见的单段提升白白损失 3~4 dB 响度。用户可关掉自动手调。
fn auto_preamp_db(gains: &[f32; BAND_COUNT]) -> f32 {
    let mut max = 0.0f32;
    for g in gains {
        if g.is_finite() && *g > max {
            max = *g;
        }
    }
    -max
}

fn db_to_linear(db: f32) -> f32 {
    if db == 0.0 {
        return 1.0;
    }
    10f32.powf(db / 20.0)
}

// ===================== 配置 =====================

/// 一份音效配置。**全局，不是每曲的**。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectsSettings {
    /// 总开关。关闭时所有目标值折算成中性，斜坡走完即进入零开销旁路。
    pub enabled: bool,
    /// 各段增益（dB），下标对应 [`BAND_FREQS`]。
    pub band_gains_db: [f32; BAND_COUNT],
    /// 自动前置增益：由各段增益推出 `preamp_db`（见 [`auto_preamp_db`]）。
    pub auto_preamp: bool,
    /// 前置增益（dB）。`auto_preamp` 为真时由 [`EffectsSettings::resolved`] 覆写。
    pub preamp_db: f32,
    /// 0..2，1 = 原样。`mono` 已在构造时折算进来（mono → 0）。
    pub stereo_width: f32,
    /// -1..1，0 = 居中。
    pub balance: f32,
}

impl Default for EffectsSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            band_gains_db: [0.0; BAND_COUNT],
            auto_preamp: true,
            preamp_db: 0.0,
            stereo_width: 1.0,
            balance: 0.0,
        }
    }
}

impl EffectsSettings {
    /// 规整成可直接当目标值用的形态：clamp 到合法区间、折算自动前置增益、
    /// 关闭时整份归中性。**非有限值（NaN/Inf）一律退回中性**——设置文件跨版本升级
    /// 或 Dart 侧算错都不该让音频里出现 NaN。
    ///
    /// 放在写入侧（[`EffectsConfig::set`]）做一次，消费端拿到的快照即已规整，
    /// 音频回调路径上不再有 clamp 分支。
    pub fn resolved(mut self) -> Self {
        if !self.enabled {
            return Self {
                enabled: false,
                ..Self::default()
            };
        }
        for g in self.band_gains_db.iter_mut() {
            *g = if g.is_finite() {
                g.clamp(BAND_GAIN_MIN_DB, BAND_GAIN_MAX_DB)
            } else {
                0.0
            };
        }
        self.stereo_width = if self.stereo_width.is_finite() {
            self.stereo_width.clamp(0.0, 2.0)
        } else {
            1.0
        };
        self.balance = if self.balance.is_finite() {
            self.balance.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        self.preamp_db = if self.auto_preamp {
            auto_preamp_db(&self.band_gains_db)
        } else if self.preamp_db.is_finite() {
            self.preamp_db.clamp(BAND_GAIN_MIN_DB, BAND_GAIN_MAX_DB)
        } else {
            0.0
        };
        self
    }

    /// 是否完全中性（等价于「什么都不做」）。用于快速旁路判定。
    fn is_neutral(&self) -> bool {
        self.band_gains_db.iter().all(|g| g.abs() < GAIN_EPS_DB)
            && self.preamp_db.abs() < GAIN_EPS_DB
            && (self.stereo_width - 1.0).abs() < 1e-4
            && self.balance.abs() < 1e-4
    }
}

/// 播放线程/FRB 侧写、音频消费端读的共享音效配置。
///
/// **音频回调路径绝不能上锁**，故用「原子 version + Mutex 快照」协议：消费端每块
/// Acquire 读一次 `version`，没变就什么都不做（**不加锁**）；变了才锁一次把
/// [`EffectsSettings`] 拷走。用户拖滑块时也就每块（~46ms）锁一次，绝对安全。
///
/// 滤波器状态则放在 [`EffectsProcessor`]（`RingSource` 本地），音频线程因此永远不碰
/// 锁保护的数据；重绑输出换新 source 时丢掉滤波器状态也无所谓（重新 ring-up，加上
/// 自带的 8ms 淡入，听不出来）。
pub struct EffectsConfig {
    /// 每次 [`set`](Self::set) 自增。
    version: AtomicU64,
    /// 快速旁路：EQ/立体声任一非中性即 true。稳态下消费端只读这一个原子。
    active: AtomicBool,
    /// 0.0 / 1.0（f32 位模式）。播放线程在 pause/stop/切歌前置 0，恢复时置 1。
    fade_target: AtomicU32,
    settings: Mutex<EffectsSettings>,
}

impl Default for EffectsConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectsConfig {
    pub fn new() -> Self {
        Self {
            // 从 1 起算：消费端的 `cached_version` 从 0 起算，故第一块必然同步一次快照，
            // 无需为「source 建于 set 之前还是之后」分情况。
            version: AtomicU64::new(1),
            active: AtomicBool::new(false),
            fade_target: AtomicU32::new(1.0f32.to_bits()),
            settings: Mutex::new(EffectsSettings::default()),
        }
    }

    /// 写入新配置。由播放线程调用（每次用户改设置一次）。
    pub fn set(&self, settings: EffectsSettings) {
        let resolved = settings.resolved();
        {
            // 锁中毒只可能来自某个 panic 的写者，配置本身是 Copy 的纯数据，
            // 沿用旧值继续写即可，绝不能在这里 panic 把播放线程带走。
            let mut g = match self.settings.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            *g = resolved;
        }
        self.active.store(!resolved.is_neutral(), Ordering::Relaxed);
        // version 必须最后写且用 Release：消费端 Acquire 读到新版本号后才去取锁，
        // 从而保证一定看得到上面写进去的 settings。
        self.version.fetch_add(1, Ordering::Release);
    }

    /// 是否有任何非中性参数。稳态旁路只读这一个原子。
    #[inline]
    pub fn active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    /// 关闸：pause / stop / 切歌**之前**调用，随后 sleep [`FADE_SLEEP`] 再执行原动作。
    pub fn begin_fade_out(&self) {
        self.fade_target.store(0.0f32.to_bits(), Ordering::Relaxed);
    }

    /// 开闸：resume 与新建 source（起播 / 重绑输出）前调用。
    pub fn end_fade_out(&self) {
        self.fade_target.store(1.0f32.to_bits(), Ordering::Relaxed);
    }

    #[inline]
    fn fade_target(&self) -> f32 {
        f32::from_bits(self.fade_target.load(Ordering::Relaxed))
    }

    /// 版本变了才返回新快照（并更新 `cached`）；没变返回 `None` 且**不加锁**。
    ///
    /// 若 `set` 恰好插在「读 version」与「取锁」之间，这里会拿到更新的 settings 配上
    /// 旧一号的 version，下一块因而再取一次锁——多一次锁而已，值不会错。
    fn snapshot_if_changed(&self, cached: &mut u64) -> Option<EffectsSettings> {
        let version = self.version.load(Ordering::Acquire);
        if version == *cached {
            return None;
        }
        let settings = match self.settings.lock() {
            Ok(g) => *g,
            Err(poisoned) => *poisoned.into_inner(),
        };
        *cached = version;
        Some(settings)
    }
}

// ===================== 处理器 =====================

/// 斜坡推进一个标量，返回是否发生了变化。
#[inline]
fn ramp(cur: &mut f32, target: f32, max_step: f32) -> bool {
    let diff = target - *cur;
    if diff == 0.0 {
        return false;
    }
    if diff.abs() <= max_step {
        *cur = target;
    } else {
        *cur += max_step.copysign(diff);
    }
    true
}

/// 挂在 `RingSource` 上的音效处理器：持滤波器状态与斜坡中的当前值。
///
/// 每个 source 一份；输出重绑会重建（状态归零 + 自带淡入）。
pub struct EffectsProcessor {
    sample_rate: u32,
    channels: usize,
    /// 上次同步到的配置版本，见 [`EffectsConfig::snapshot_if_changed`]。
    cached_version: u64,
    /// 目标值（来自最近一次快照，已 [`EffectsSettings::resolved`]）。
    target: EffectsSettings,
    cur_bands_db: [f32; BAND_COUNT],
    cur_preamp_db: f32,
    cur_width: f32,
    cur_balance: f32,
    /// 输出闸门 0..1。新建即 0 → 每首歌开头自带 8ms 淡入，顺带消掉起播的咔哒声。
    fade: f32,
    coeffs: [Option<Coeffs>; BAND_COUNT],
    coeffs_dirty: bool,
    /// 每声道一组滤波器状态。仅在 [`Self::new`] 分配一次，处理路径上无分配。
    states: Vec<[BiquadState; BAND_COUNT]>,
}

impl EffectsProcessor {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        let channels = channels.max(1) as usize;
        Self {
            sample_rate: sample_rate.max(1),
            channels,
            cached_version: 0,
            target: EffectsSettings::default(),
            cur_bands_db: [0.0; BAND_COUNT],
            cur_preamp_db: 0.0,
            cur_width: 1.0,
            cur_balance: 0.0,
            fade: 0.0,
            coeffs: [None; BAND_COUNT],
            coeffs_dirty: true,
            states: vec![[BiquadState::default(); BAND_COUNT]; channels],
        }
    }

    /// seek 后调用：丢掉滤波器状态，避免旧位置的信号在滤波器里留下 ringing；
    /// 顺带重走一次 8ms 淡入——seek 是波形上的硬切换，本就是一次咔哒的来源。
    pub fn reset(&mut self) {
        for st in self.states.iter_mut() {
            *st = [BiquadState::default(); BAND_COUNT];
        }
        self.fade = 0.0;
    }

    /// 当前斜坡值是否已收敛到中性（= 这一块什么都不用做）。
    fn is_at_neutral(&self) -> bool {
        self.cur_bands_db.iter().all(|g| g.abs() < GAIN_EPS_DB)
            && self.cur_preamp_db.abs() < GAIN_EPS_DB
            && (self.cur_width - 1.0).abs() < 1e-4
            && self.cur_balance.abs() < 1e-4
    }

    fn recompute_coeffs(&mut self) {
        let fs = self.sample_rate as f64;
        for (i, slot) in self.coeffs.iter_mut().enumerate() {
            // BAND_FREQS / cur_bands_db 与 coeffs 等长，i 必在界内。
            let f0 = BAND_FREQS[i] as f64;
            let gain = self.cur_bands_db[i] as f64;
            *slot = design(band_kind(i), f0, gain, fs);
        }
    }

    /// 任一参数仍在往目标值爬。斜坡进行中要把块切细，见 [`RAMP_SPAN_FRAMES`]。
    fn is_ramping(&self) -> bool {
        self.cur_bands_db
            .iter()
            .zip(self.target.band_gains_db.iter())
            .any(|(c, t)| c != t)
            || self.cur_preamp_db != self.target.preamp_db
            || self.cur_width != self.target.stereo_width
            || self.cur_balance != self.target.balance
    }

    /// 给一块交错 PCM 施加音效（原地）。由音频消费端每块调用一次。
    ///
    /// 稳态（功能关闭、fade 已到 1）下的全部开销就是三次无竞争的原子读
    /// （version / active / fade_target）加十来次浮点比较，**不加锁、不分配、不 panic**。
    pub fn process(&mut self, buf: &mut [f32], config: &EffectsConfig) {
        if self.channels == 0 {
            return;
        }
        // 帧对齐：`Ring::pop_chunk` 已保证 `buf.len()` 是声道数的整数倍，这里再兜一层——
        // 半帧会让左右声道逐渐串位（症状是立体声像慢慢偏移），极难排查，宁可丢掉残帧。
        let frames = buf.len() / self.channels;
        if frames == 0 {
            return;
        }

        if let Some(settings) = config.snapshot_if_changed(&mut self.cached_version) {
            self.target = settings;
        }
        let fade_target = config.fade_target();

        // --- 旁路快路径 ---
        // 四个条件缺一不可：配置中性还不够，斜坡可能仍在往中性收敛（`is_at_neutral`），
        // 淡入淡出也可能正在进行（fade 两项）。全中性时斜坡与 fade 都是空操作，
        // 故这个判断可以安全地前置到斜坡之前，稳态下连比较都少做几次。
        if !config.active() && self.is_at_neutral() && self.fade >= 1.0 && fade_target >= 1.0 {
            return;
        }

        // 斜坡进行中把块切成 ≤ [`RAMP_SPAN_FRAMES`] 的子块逐段推进，理由见该常量。
        let span_frames = if self.is_ramping() {
            RAMP_SPAN_FRAMES.min(frames)
        } else {
            frames
        };
        let span_samples = span_frames * self.channels;
        let end = frames * self.channels;
        for span in buf[..end].chunks_mut(span_samples) {
            self.process_span(span, fade_target);
        }
    }

    /// 处理一个子块：推进一次斜坡 → 必要时重算系数 → 逐帧跑完整条链。
    fn process_span(&mut self, buf: &mut [f32], fade_target: f32) {
        let frames = buf.len() / self.channels;
        if frames == 0 {
            return;
        }
        let dt = frames as f32 / self.sample_rate as f32;

        // --- 斜坡：记下块首值，块尾值即推进后的当前值，两端之间按帧插值 ---
        let width_from = self.cur_width;
        let balance_from = self.cur_balance;
        let preamp_from = self.cur_preamp_db;

        let gain_step = GAIN_SLEW_DB_PER_SEC * dt;
        for (cur, target) in self
            .cur_bands_db
            .iter_mut()
            .zip(self.target.band_gains_db.iter())
        {
            if ramp(cur, *target, gain_step) {
                self.coeffs_dirty = true;
            }
        }
        ramp(&mut self.cur_preamp_db, self.target.preamp_db, gain_step);
        let stereo_step = STEREO_SLEW_PER_SEC * dt;
        ramp(&mut self.cur_width, self.target.stereo_width, stereo_step);
        ramp(&mut self.cur_balance, self.target.balance, stereo_step);

        if self.coeffs_dirty {
            // ≤20 次 sin/cos；稳态下 `coeffs_dirty` 恒 false，一次都不算。
            self.recompute_coeffs();
            self.coeffs_dirty = false;
        }

        let preamp_a = db_to_linear(preamp_from);
        let preamp_b = db_to_linear(self.cur_preamp_db);
        let inv_frames = 1.0 / frames as f32;
        let d_preamp = (preamp_b - preamp_a) * inv_frames;
        let d_width = (self.cur_width - width_from) * inv_frames;
        let d_balance = (self.cur_balance - balance_from) * inv_frames;
        // 立体声级只在双声道下有意义；且 width 恰为 1、balance 恰为 0 时必须整段跳过——
        // M/S 往返 `(l+r)/2 ± (l-r)/2` 在浮点下不保证逐位还原 l/r，白掉 1 ulp。
        let stereo = self.channels == 2
            && !(width_from == 1.0
                && self.cur_width == 1.0
                && balance_from == 0.0
                && self.cur_balance == 0.0);
        let fade_step = 1.0 / (FADE_SECS * self.sample_rate as f32);

        let mut fade = self.fade;
        let coeffs = &self.coeffs;
        let states = &mut self.states;

        for (i, frame) in buf.chunks_exact_mut(self.channels).enumerate() {
            let t = i as f32;
            let preamp = (preamp_a + d_preamp * t) as f64;
            // 前置增益 + 10 段级联。states 与 frame 等长（都是 channels），zip 不会越界。
            for (x, state) in frame.iter_mut().zip(states.iter_mut()) {
                let mut v = *x as f64 * preamp;
                for (c, s) in coeffs.iter().zip(state.iter_mut()) {
                    if let Some(c) = c {
                        v = tick(c, s, v);
                    }
                }
                *x = v as f32;
            }
            if stereo {
                let width = width_from + d_width * t;
                let (gain_l, gain_r) = balance_gains(balance_from + d_balance * t);
                let l = frame[0];
                let r = frame[1];
                let mid = (l + r) * 0.5;
                let side = (l - r) * 0.5;
                frame[0] = (mid + side * width) * gain_l;
                frame[1] = (mid - side * width) * gain_r;
            }
            // fade 按**帧**推进而非按交错采样推进：后者会让同一帧的 L/R 拿到不同增益，
            // 相当于淡入淡出期间给声像加了一点抖动。代价为零，没理由不这么做。
            fade = if fade < fade_target {
                (fade + fade_step).min(fade_target)
            } else if fade > fade_target {
                (fade - fade_step).max(fade_target)
            } else {
                fade
            };
            for x in frame.iter_mut() {
                *x = soft_clip(*x * fade);
            }
        }
        self.fade = fade;

        // denormal 冲刷：20 次比较/块，可忽略；不做的话淡出后的指数拖尾会把状态带进
        // denormal 区，某些平台上的浮点微码路径慢到能让音频回调错过 deadline。
        for state in self.states.iter_mut() {
            for s in state.iter_mut() {
                if s.z1.abs() < DENORMAL_FLOOR {
                    s.z1 = 0.0;
                }
                if s.z2.abs() < DENORMAL_FLOOR {
                    s.z2 = 0.0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 确定性伪随机 PCM（-0.5..0.5）。测试要可复现，故不用真随机。
    fn noise(len: usize) -> Vec<f32> {
        let mut state = 0x1234_5678u32;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 8) as f32 / (1 << 24) as f32 - 0.5
            })
            .collect()
    }

    fn sine(freq: f64, rate: u32, len: usize, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|i| (amp as f64 * (2.0 * PI * freq * i as f64 / rate as f64).sin()) as f32)
            .collect()
    }

    fn rms(block: &[f32]) -> f64 {
        if block.is_empty() {
            return 0.0;
        }
        (block.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>() / block.len() as f64).sqrt()
    }

    fn enabled(gains: [f32; BAND_COUNT], auto_preamp: bool) -> EffectsSettings {
        EffectsSettings {
            enabled: true,
            band_gains_db: gains,
            auto_preamp,
            ..EffectsSettings::default()
        }
    }

    /// 跑 `count` 块，返回最后一块的输出。每块都用同一份输入。
    fn run_blocks(
        p: &mut EffectsProcessor,
        cfg: &EffectsConfig,
        block: &[f32],
        count: usize,
    ) -> Vec<f32> {
        let mut out = block.to_vec();
        for _ in 0..count {
            out.copy_from_slice(block);
            p.process(&mut out, cfg);
        }
        out
    }

    /// §7.1 旁路零改动：关闭时输出与输入**逐位相同**。
    #[test]
    fn disabled_is_bit_exact_passthrough() {
        let cfg = EffectsConfig::new();
        let mut p = EffectsProcessor::new(44_100, 2);
        let input = noise(4096);
        // 第一块把起播自带的 8ms 淡入走完（4096 采样 ≈ 46ms ≫ 8ms）。
        let mut warm = input.clone();
        p.process(&mut warm, &cfg);
        assert_eq!(p.fade, 1.0);

        let mut out = input.clone();
        p.process(&mut out, &cfg);
        assert_eq!(out, input, "关闭时必须逐位原样输出");
    }

    /// 稳态下单频点的 RMS 增益。
    fn steady_gain(cfg: &EffectsConfig, freq: f64) -> f64 {
        let rate = 44_100u32;
        let mut p = EffectsProcessor::new(rate, 1);
        let block = sine(freq, rate, 4096, 0.2);
        // 30 块 ≈ 2.8s，远超 24 dB/s 斜坡走完 12 dB 所需的 0.5s 与滤波器暂态。
        let out = run_blocks(&mut p, cfg, &block, 30);
        rms(&out) / rms(&block)
    }

    /// §7.2 频响正确：单段 +6 dB 处增益 ≈ ×2，相隔 3 个倍频程处不受影响。
    #[test]
    fn single_band_boost_hits_target_and_does_not_bleed() {
        let cfg = EffectsConfig::new();
        let mut s = enabled([0.0; BAND_COUNT], false);
        s.band_gains_db[5] = 6.0; // 1 kHz
        cfg.set(s);

        let at_center = steady_gain(&cfg, 1000.0);
        assert!(
            (at_center - 2.0).abs() < 0.1,
            "1kHz 应 ≈ ×2，实测 {at_center}"
        );
        // 125 Hz = 中心频点下方 3 个倍频程，Q=1.414（1 倍频程带宽）下应几乎无影响。
        let far = steady_gain(&cfg, 125.0);
        assert!((far - 1.0).abs() < 0.05, "125Hz 不应被带起来，实测 {far}");
    }

    /// §7.3 Nyquist 守卫：32kHz 下 16k 段必须整段跳过，输出保持有界（不发散、不出 NaN）。
    #[test]
    fn nyquist_guard_skips_degenerate_band() {
        let cfg = EffectsConfig::new();
        let mut s = enabled([0.0; BAND_COUNT], false);
        s.band_gains_db[9] = 12.0;
        cfg.set(s);

        let mut p = EffectsProcessor::new(32_000, 1);
        let block = noise(2048);
        let out = run_blocks(&mut p, &cfg, &block, 40);
        assert!(
            p.coeffs[9].is_none(),
            "16k @ 32kHz 已越过 0.45·Fs，必须置 None"
        );
        assert!(
            out.iter().all(|x| x.is_finite() && x.abs() < 2.0),
            "退化系数会让滤波器发散出啸叫"
        );
    }

    /// §7.4 软削波上界：全段 +12 且关掉自动前置增益，满幅方波输出仍 ≤ 1.0。
    #[test]
    fn soft_clip_bounds_output_at_unity() {
        let cfg = EffectsConfig::new();
        cfg.set(enabled([12.0; BAND_COUNT], false));

        let mut p = EffectsProcessor::new(44_100, 2);
        let block: Vec<f32> = (0..4096)
            .map(|i| if (i / 32) % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        for _ in 0..40 {
            let mut b = block.clone();
            p.process(&mut b, &cfg);
            assert!(
                b.iter().all(|x| x.abs() <= 1.0 + 1e-6),
                "软削波的输出上界恰为 1.0"
            );
        }
    }

    /// §7.5 立体声宽度：0 = 完全下混（L == R）。
    #[test]
    fn zero_width_collapses_to_mono() {
        let cfg = EffectsConfig::new();
        cfg.set(EffectsSettings {
            enabled: true,
            stereo_width: 0.0,
            ..EffectsSettings::default()
        });

        let mut p = EffectsProcessor::new(44_100, 2);
        // L/R 差异明显的一块（左 0.4、右 -0.2）。
        let block: Vec<f32> = (0..4096)
            .map(|i| if i % 2 == 0 { 0.4 } else { -0.2 })
            .collect();
        // 宽度斜坡 4/s：1 → 0 需 0.25s，20 块 ≈ 1.9s 足够。
        let out = run_blocks(&mut p, &cfg, &block, 20);
        for pair in out.chunks_exact(2) {
            assert!((pair[0] - pair[1]).abs() < 1e-6, "{pair:?}");
        }
        // 下混后是两声道的均值。
        assert!((out[0] - 0.1).abs() < 1e-6, "{}", out[0]);
    }

    /// §7.5 立体声宽度：1 = 原样（逐位）。
    #[test]
    fn unit_width_is_bit_exact() {
        let cfg = EffectsConfig::new();
        cfg.set(EffectsSettings {
            enabled: true,
            stereo_width: 1.0,
            ..EffectsSettings::default()
        });

        let mut p = EffectsProcessor::new(44_100, 2);
        let input = noise(4096);
        let mut warm = input.clone();
        p.process(&mut warm, &cfg);
        let mut out = input.clone();
        p.process(&mut out, &cfg);
        assert_eq!(out, input, "width = 1 必须逐位原样");
    }

    /// §7.6 平衡只衰减：任意 balance 下逐样本 `|out| <= |in|`。
    #[test]
    fn balance_only_attenuates() {
        for balance in [-1.0f32, -0.5, 0.0, 0.3, 1.0] {
            let cfg = EffectsConfig::new();
            cfg.set(EffectsSettings {
                enabled: true,
                balance,
                ..EffectsSettings::default()
            });
            let mut p = EffectsProcessor::new(44_100, 2);
            let block = noise(2048);
            for _ in 0..20 {
                let mut out = block.clone();
                p.process(&mut out, &cfg);
                for (o, i) in out.iter().zip(block.iter()) {
                    // 1e-6 的余量留给 M/S 往返的 1 ulp 舍入，不是给「提升」留的口子。
                    assert!(
                        o.abs() <= i.abs() + 1e-6,
                        "balance={balance} out={o} in={i}"
                    );
                }
            }
        }
    }

    /// §7.7 参数斜坡不跳变：全段 0 → +12 的突变，输出逐样本差分无阶跃。
    #[test]
    fn gain_jump_never_steps_the_output() {
        let cfg = EffectsConfig::new();
        let mut p = EffectsProcessor::new(44_100, 1);
        // 用 DC 输入，这样输出的逐样本差分**只反映参数变化**，不含信号本身的斜率。
        let block = vec![0.3f32; 4096];
        let warm = run_blocks(&mut p, &cfg, &block, 2);
        let mut prev = *warm.last().unwrap();

        cfg.set(enabled([12.0; BAND_COUNT], true));
        let mut max_diff = 0.0f32;
        for _ in 0..40 {
            let mut out = block.clone();
            p.process(&mut out, &cfg);
            for x in &out {
                max_diff = max_diff.max((x - prev).abs());
                prev = *x;
            }
        }
        // 阈值 0.02 的来由：DC 0.3 上，一次「+12 dB 直接跳变」会造成约 0.9 的单样本落差；
        // 现在实测 5.2e-3（全部来自子块边界重算系数的拐点，见 RAMP_SPAN_FRAMES），取 0.02
        // 留了近 4 倍余量，既能揪出任何真·阶跃，又不会被平台间的浮点差异误伤。
        // 注：按整块重算系数时这里是 0.15——那正是这条测试要挡住的东西。
        assert!(max_diff < 0.02, "逐样本差分过大: {max_diff}");
    }

    /// §7.8 denormal 冲刷：淡出到 0 后连续跑若干块，滤波器状态最终精确为 0.0。
    #[test]
    fn denormals_are_flushed_after_fade_out() {
        let cfg = EffectsConfig::new();
        let mut s = enabled([0.0; BAND_COUNT], false);
        s.band_gains_db[5] = 12.0;
        cfg.set(s);

        let mut p = EffectsProcessor::new(44_100, 1);
        let block = noise(1024);
        run_blocks(&mut p, &cfg, &block, 10);
        assert!(
            p.states[0].iter().any(|st| st.z1 != 0.0 || st.z2 != 0.0),
            "先得把滤波器状态充上电，否则这条测试什么都没验证"
        );

        cfg.begin_fade_out();
        let silence = vec![0.0f32; 1024];
        run_blocks(&mut p, &cfg, &silence, 200);
        assert_eq!(p.fade, 0.0);
        assert!(
            p.states[0].iter().all(|st| st.z1 == 0.0 && st.z2 == 0.0),
            "指数拖尾必须被冲刷成精确 0，否则会掉进 denormal 的慢路径"
        );
    }

    /// 自动前置增益按最高的那段压回来；关闭总开关则整份归中性。
    #[test]
    fn resolved_folds_auto_preamp_and_disabled_state() {
        let mut s = enabled([0.0; BAND_COUNT], true);
        s.band_gains_db[0] = 9.0;
        s.band_gains_db[3] = -6.0;
        let r = s.resolved();
        assert_eq!(r.preamp_db, -9.0);

        let off = EffectsSettings {
            enabled: false,
            ..s
        }
        .resolved();
        assert!(off.is_neutral());
    }

    /// 越界 / NaN 的配置必须被规整，绝不能让 NaN 流进音频。
    #[test]
    fn resolved_clamps_out_of_range_values() {
        let mut s = enabled([99.0; BAND_COUNT], false);
        s.band_gains_db[1] = f32::NAN;
        s.stereo_width = 7.0;
        s.balance = -3.0;
        s.preamp_db = f32::INFINITY;
        let r = s.resolved();
        assert_eq!(r.band_gains_db[0], BAND_GAIN_MAX_DB);
        assert_eq!(r.band_gains_db[1], 0.0);
        assert_eq!(r.stereo_width, 2.0);
        assert_eq!(r.balance, -1.0);
        assert_eq!(r.preamp_db, 0.0);
    }

    /// 快照协议：版本没变就不返回（消费端据此免锁）。
    #[test]
    fn snapshot_only_on_version_change() {
        let cfg = EffectsConfig::new();
        let mut cached = 0u64;
        assert!(cfg.snapshot_if_changed(&mut cached).is_some());
        assert!(cfg.snapshot_if_changed(&mut cached).is_none());
        cfg.set(enabled([3.0; BAND_COUNT], true));
        let s = cfg
            .snapshot_if_changed(&mut cached)
            .expect("改过就该有新快照");
        assert!(s.enabled);
        assert!(cfg.active());
        assert!(cfg.snapshot_if_changed(&mut cached).is_none());
    }

    /// seek 后重置：状态归零并重新走一次淡入。
    #[test]
    fn reset_clears_state_and_refades() {
        let cfg = EffectsConfig::new();
        let mut s = enabled([0.0; BAND_COUNT], false);
        s.band_gains_db[5] = 6.0;
        cfg.set(s);
        let mut p = EffectsProcessor::new(44_100, 1);
        run_blocks(&mut p, &cfg, &noise(1024), 5);
        assert_eq!(p.fade, 1.0);

        p.reset();
        assert_eq!(p.fade, 0.0);
        assert!(p.states[0].iter().all(|st| st.z1 == 0.0 && st.z2 == 0.0));
    }
}
