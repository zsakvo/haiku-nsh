//! 响度均衡（EBU R128）：**边播边测 → 落盘 → 下次开播即用**。
//!
//! 不同来源的曲目母带响度差异可达十几 dB，固定音量下听感忽大忽小。本模块按 EBU R128
//! 测出每首歌的整合响度（integrated LUFS）与采样峰值，换算成一个**每曲静态增益**：
//! 只整体抬升/压低，不压缩曲内动态（与 AGC / dynaudnorm 的本质区别——后者会把安静的
//! 前奏顶起来、把渐强抹平）。
//!
//! 三条链路：
//!
//! 1. **测量**：[`LoudnessMeter`] 挂在解码线程上，直接吃 [`crate::player::decode_loop`]
//!    本来就要解出的 PCM（pre-gain 原始信号），额外开销只有 K 计权双二阶滤波 + 分块均方，
//!    相对 AAC/FLAC 解码可忽略；门限积分走直方图，内存恒定。
//! 2. **施加**：[`TrackLoudness::current_gain`] 由 `RingSource` 每块读一次，逐样本斜坡
//!    趋近目标增益，因此改增益不会有阶跃爆音。增益乘在 PCM 上，与 rodio sink 上的用户
//!    音量正交（最终 = 用户音量 × 本增益）。
//! 3. **持久化**：测量快照经 FRB 事件回给 Dart 落 MMKV，下次播这首歌直接把算好的
//!    `known_gain_db` 传进来，**出声第一帧就是对的**。
//!
//! 防削波：正增益不得让采样峰值越过 [`PEAK_HEADROOM_DB`]，故无需限制器，也不会削顶。
//! 代价是「安静但峰值满」的曲目只能补偿到余量为止（与 ReplayGain 的 prevent-clipping 同策）。

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use ebur128::{EbuR128, Mode};

/// 默认目标响度（LUFS）。取 ReplayGain 2.0 的 -18：现代母带普遍在 -10 ~ -8 LUFS，
/// 以此为目标时绝大多数曲目走**衰减**路径，天然不会削波；用户觉得整体偏轻可调高系统音量。
pub const DEFAULT_TARGET_LUFS: f64 = -18.0;

/// 增益下限（dB）。测量异常（近乎静音的曲目）时兜底。
const MIN_GAIN_DB: f64 = -20.0;
/// 增益上限（dB）。
const MAX_GAIN_DB: f64 = 12.0;
/// 削波余量（dB）：施加增益后采样峰值不得超过 -1 dBFS。
const PEAK_HEADROOM_DB: f64 = 1.0;

/// 自适应起测门槛（秒）：测够这么多音频才敢给第一版增益，避免拿前奏当全曲。
const ADAPT_MIN_SECS: f64 = 3.0;
/// 自适应快速收敛窗口（按**已测秒数**计）。解码跑在播放前方数秒，故这段时间里
/// 听到的音频不过刚起头，用较快速率把增益推到位，听感接近一次轻微淡入。
const ADAPT_FAST_WINDOW_SECS: f64 = 8.0;
/// 快速窗口内**向下**（衰减）的增益变化速率（dB/s）。见 [`TrackLoudness::record`]
/// 里对方向的说明：抬升永远走慢速率。
const ADAPT_SLEW_FAST_DB_PER_SEC: f32 = 6.0;
/// 常规增益变化速率（dB/s）。慢到人耳基本察觉不到「音量在动」。
const ADAPT_SLEW_DB_PER_SEC: f32 = 1.0;
/// 用户中途改设置（拨开关）时的过渡速率（dB/s）：快到像「立刻生效」，
/// 又仍然是斜坡而非阶跃，不会有爆音。
const TOGGLE_SLEW_DB_PER_SEC: f32 = 12.0;

/// 测量结果上报间隔（秒，按已测音频计）。
const MEASURE_UPDATE_SECS: f64 = 0.5;

/// 由整合响度与采样峰值换算每曲增益（dB）。
///
/// - `lufs` 非有限（全静音 / 无门限通过块）视为测不出，返回 `None`；
/// - 正增益按 `peak` 留 [`PEAK_HEADROOM_DB`] 余量封顶，故施加后必不削波；
/// - 最终 clamp 到 [`MIN_GAIN_DB`]..=[`MAX_GAIN_DB`]。
pub fn gain_db(lufs: f64, peak: f64, target_lufs: f64) -> Option<f64> {
    if !lufs.is_finite() {
        return None;
    }
    let mut gain = target_lufs - lufs;
    // 峰值封顶只约束「抬升」；衰减方向永远安全。
    if peak.is_finite() && peak > 0.0 {
        let ceiling = -20.0 * peak.log10() - PEAK_HEADROOM_DB;
        if gain > ceiling {
            gain = ceiling;
        }
    }
    Some(gain.clamp(MIN_GAIN_DB, MAX_GAIN_DB))
}

fn db_to_linear(db: f64) -> f32 {
    10f64.powf(db / 20.0) as f32
}

fn linear_to_db(linear: f32) -> f64 {
    20.0 * (linear.max(1e-6) as f64).log10()
}

/// 一次播放的响度均衡参数。由 Dart 在每次 `play` 时给出。
#[derive(Clone, Copy, Debug)]
pub struct LoudnessSettings {
    /// 总开关。关闭时增益恒 1.0 且不测量（零开销）。
    pub enabled: bool,
    /// 目标响度（LUFS）。
    pub target_lufs: f64,
    /// 已知增益（dB）：来自上次测量的落盘记录。`None` = 这首歌还没测过。
    pub known_gain_db: Option<f64>,
    /// 是否测量本次播放。已有高覆盖率记录时 Dart 传 `false`，省掉重复测量；
    /// 传 `true` 时同时启用自适应——即便有 `known_gain_db` 作起点，也会随测量结果修正。
    pub measure: bool,
}

impl Default for LoudnessSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            target_lufs: DEFAULT_TARGET_LUFS,
            known_gain_db: None,
            measure: false,
        }
    }
}

/// 一次测量的快照，回给 Dart 落盘。
#[derive(Clone, Copy, Debug)]
pub struct LoudnessReport {
    /// 整合响度（LUFS，负值）。
    pub lufs: f64,
    /// 采样峰值（线性，1.0 = 0 dBFS）。
    pub peak: f64,
    /// 已测音频秒数。Dart 据此与曲目时长比对，判断记录可信度。
    pub measured_secs: f64,
    /// 解码已抵达流末尾（本次测量不会再增长）。
    pub complete: bool,
}

/// 单曲共享的响度状态：解码线程写测量、音频消费端读增益、播放线程改配置。
///
/// 全原子无锁——音频回调路径（`RingSource` 每块一次）绝不能上锁。f32/f64 按位存进
/// `AtomicU32`/`AtomicU64`，模式与 `PlayerHandle` 的 `pos_ms`/`buffered` 一致。
pub struct TrackLoudness {
    /// 曲目标识（`pluginId\x1ftrackId`），随测量结果一起回给 Dart。
    ///
    /// 测量结果必须自带归属：轮询侧读到快照时，Dart 可能正在切下一首，靠「当前播放的是谁」
    /// 推断会把上一首的测量写到下一首头上。key 跟着 [`TrackLoudness`] 走就没有这个窗口。
    key: String,
    /// 目标线性增益。1.0 = 不改变。
    target_gain: AtomicU32,
    /// 当前已施加的线性增益（斜坡的落点）。放在共享态而非 `RingSource` 里，
    /// 是为了重绑输出换新 source 时增益接得上，不会跳回 1.0 再重新爬。
    current_gain: AtomicU32,
    /// 当前允许的增益变化速率（dB/s）。见 [`ADAPT_FAST_WINDOW_SECS`]。
    slew_db_per_sec: AtomicU32,

    /// 最近一次测量：整合响度（LUFS）。NaN = 尚无结果。
    lufs: AtomicU64,
    /// 最近一次测量：采样峰值（线性）。
    peak: AtomicU64,
    /// 已测毫秒数。
    measured_ms: AtomicU64,
    /// 解码已抵达流末尾。
    complete: AtomicBool,

    /// 配置（可被 `SetLoudness` 中途改写）。
    enabled: AtomicBool,
    measure: AtomicBool,
    target_lufs: AtomicU64,
    /// 已知增益（dB）。NaN = 无。
    known_gain_db: AtomicU64,
}

impl TrackLoudness {
    pub fn new(settings: LoudnessSettings, key: String) -> Self {
        let this = Self {
            key,
            target_gain: AtomicU32::new(1.0f32.to_bits()),
            current_gain: AtomicU32::new(1.0f32.to_bits()),
            slew_db_per_sec: AtomicU32::new(ADAPT_SLEW_FAST_DB_PER_SEC.to_bits()),
            lufs: AtomicU64::new(f64::NAN.to_bits()),
            peak: AtomicU64::new(f64::NAN.to_bits()),
            measured_ms: AtomicU64::new(0),
            complete: AtomicBool::new(false),
            enabled: AtomicBool::new(false),
            measure: AtomicBool::new(false),
            target_lufs: AtomicU64::new(DEFAULT_TARGET_LUFS.to_bits()),
            known_gain_db: AtomicU64::new(f64::NAN.to_bits()),
        };
        this.configure(settings);
        // 开播即用：已知增益直接落在 current_gain 上（不走斜坡，此刻还没出声）。
        let g = f32::from_bits(this.target_gain.load(Ordering::Relaxed));
        this.current_gain.store(g.to_bits(), Ordering::Relaxed);
        this
    }

    /// 写入配置并重算目标增益。中途改设置（用户拨开关）时由播放线程调用，
    /// 此时 `current_gain` 保持不动，由 `RingSource` 斜坡过渡过去。
    pub fn configure(&self, settings: LoudnessSettings) {
        self.enabled.store(settings.enabled, Ordering::Relaxed);
        self.measure
            .store(settings.enabled && settings.measure, Ordering::Relaxed);
        self.target_lufs
            .store(settings.target_lufs.to_bits(), Ordering::Relaxed);
        self.known_gain_db.store(
            settings.known_gain_db.unwrap_or(f64::NAN).to_bits(),
            Ordering::Relaxed,
        );
        self.recompute_target();
        // 用户动作要「立刻」看到效果，两个方向都用快速率过渡。
        self.slew_db_per_sec
            .store(TOGGLE_SLEW_DB_PER_SEC.to_bits(), Ordering::Relaxed);
    }

    /// 是否需要在解码线程上挂表头。
    pub fn should_measure(&self) -> bool {
        self.measure.load(Ordering::Relaxed)
    }

    /// 解码线程推入一次测量结果。
    pub fn record(&self, lufs: f64, peak: f64, measured_secs: f64, complete: bool) {
        self.lufs.store(lufs.to_bits(), Ordering::Relaxed);
        self.peak.store(peak.to_bits(), Ordering::Relaxed);
        self.measured_ms
            .store((measured_secs * 1000.0) as u64, Ordering::Relaxed);
        if complete {
            self.complete.store(true, Ordering::Relaxed);
        }
        let target = self.recompute_target();
        // 收敛速率**看方向**：
        // - 衰减（目标更小）在起播窗口内走快速率——响母带越早压下去越好，压过头也不刺耳；
        // - 抬升恒走慢速率——早期测量若正好撞上安静前奏会高估增益，猛抬上去再退回来
        //   就是一次明显的「忽大忽小」，恰恰是本功能要消灭的东西。
        let rushing_down = measured_secs < ADAPT_FAST_WINDOW_SECS
            && target < f32::from_bits(self.current_gain.load(Ordering::Relaxed));
        let slew = if rushing_down {
            ADAPT_SLEW_FAST_DB_PER_SEC
        } else {
            ADAPT_SLEW_DB_PER_SEC
        };
        self.slew_db_per_sec
            .store(slew.to_bits(), Ordering::Relaxed);
    }

    /// 依「配置 + 最新测量」定目标增益，返回新的线性目标。测量优先于 `known_gain_db`
    /// （测量是这一次听到的真实信号，`known_gain_db` 只是上次的结论）；测量还不够则
    /// 退回已知值，再退回 1.0。
    fn recompute_target(&self) -> f32 {
        if !self.enabled.load(Ordering::Relaxed) {
            self.target_gain.store(1.0f32.to_bits(), Ordering::Relaxed);
            return 1.0;
        }
        let target_lufs = f64::from_bits(self.target_lufs.load(Ordering::Relaxed));
        let measured_secs = self.measured_ms.load(Ordering::Relaxed) as f64 / 1000.0;
        let db = if measured_secs >= ADAPT_MIN_SECS && self.measure.load(Ordering::Relaxed) {
            let lufs = f64::from_bits(self.lufs.load(Ordering::Relaxed));
            let peak = f64::from_bits(self.peak.load(Ordering::Relaxed));
            gain_db(lufs, peak, target_lufs)
                .or_else(|| self.known_gain_db_opt())
                .unwrap_or(0.0)
        } else {
            self.known_gain_db_opt().unwrap_or(0.0)
        };
        let target = db_to_linear(db);
        self.target_gain.store(target.to_bits(), Ordering::Relaxed);
        target
    }

    fn known_gain_db_opt(&self) -> Option<f64> {
        let v = f64::from_bits(self.known_gain_db.load(Ordering::Relaxed));
        v.is_finite().then_some(v)
    }

    /// 曲目标识（见 [`key`](Self::key)）。
    pub fn key(&self) -> &str {
        &self.key
    }

    /// 当前测量快照。无任何结果时返回 `None`。
    pub fn report(&self) -> Option<LoudnessReport> {
        let lufs = f64::from_bits(self.lufs.load(Ordering::Relaxed));
        if !lufs.is_finite() {
            return None;
        }
        Some(LoudnessReport {
            lufs,
            peak: f64::from_bits(self.peak.load(Ordering::Relaxed)),
            measured_secs: self.measured_ms.load(Ordering::Relaxed) as f64 / 1000.0,
            complete: self.complete.load(Ordering::Relaxed),
        })
    }

    /// 给一块交错 PCM 施加增益（原地）。由音频消费端每块调用一次。
    ///
    /// 块内从当前增益线性过渡到本块落点，落点与当前值之差按 [`slew_db_per_sec`] 限速，
    /// 因此无论目标怎么跳变，听到的都是平滑变化。当前与目标都是 1.0 时整块跳过。
    ///
    /// `samples_per_sec` 为**交错采样**速率（采样率 × 声道数），用于把块长换算成时长。
    ///
    /// [`slew_db_per_sec`]: Self::slew_db_per_sec
    pub fn apply(&self, block: &mut [f32], samples_per_sec: f64) {
        let target = f32::from_bits(self.target_gain.load(Ordering::Relaxed));
        let current = f32::from_bits(self.current_gain.load(Ordering::Relaxed));
        // 快路径：功能关闭 / 增益恰为 1（绝大多数「未启用」场景），一次乘法都不做。
        if (target - 1.0).abs() < 1e-6 && (current - 1.0).abs() < 1e-6 {
            return;
        }
        if block.is_empty() {
            return;
        }
        let next = if (target - current).abs() < 1e-6 {
            target
        } else {
            let slew = f32::from_bits(self.slew_db_per_sec.load(Ordering::Relaxed));
            let block_secs = if samples_per_sec > 0.0 {
                block.len() as f64 / samples_per_sec
            } else {
                0.0
            };
            let max_step = (slew as f64 * block_secs).max(0.0);
            let delta = (linear_to_db(target) - linear_to_db(current)).clamp(-max_step, max_step);
            db_to_linear(linear_to_db(current) + delta)
        };

        let n = block.len() as f32;
        let step = (next - current) / n;
        for (i, s) in block.iter_mut().enumerate() {
            *s *= current + step * i as f32;
        }
        self.current_gain.store(next.to_bits(), Ordering::Relaxed);
    }
}

/// EBU R128 表头：吃交错 f32，按节拍吐出整合响度 + 采样峰值。
///
/// 每 [`MEASURE_UPDATE_SECS`] 秒音频出一次结果——`loudness_global` 只是扫一遍固定
/// 大小的直方图，与已测时长无关，故长曲目也不会越测越慢。
pub struct LoudnessMeter {
    inner: EbuR128,
    channels: u16,
    rate: u32,
    frames: u64,
    next_update_frames: u64,
}

impl LoudnessMeter {
    /// 建表头。声道/采样率非法时返回 `None`（调用方据此静默跳过测量，不影响播放）。
    pub fn new(channels: u16, rate: u32) -> Option<Self> {
        if channels == 0 || rate == 0 {
            return None;
        }
        let inner = EbuR128::new(channels as u32, rate, Mode::I | Mode::SAMPLE_PEAK).ok()?;
        Some(Self {
            inner,
            channels,
            rate,
            frames: 0,
            next_update_frames: (rate as f64 * MEASURE_UPDATE_SECS) as u64,
        })
    }

    /// 当前表头是否匹配这段 PCM 的规格（不匹配则调用方重建）。
    pub fn matches(&self, channels: u16, rate: u32) -> bool {
        self.channels == channels && self.rate == rate
    }

    /// 已测秒数。
    pub fn measured_secs(&self) -> f64 {
        self.frames as f64 / self.rate as f64
    }

    /// 推入一段交错采样。到达上报节拍时返回 `(lufs, peak, measured_secs)`。
    pub fn push(&mut self, samples: &[f32]) -> Option<(f64, f64, f64)> {
        let ch = self.channels as usize;
        // add_frames_f32 要求整帧；解码器给的本就是整帧，截断只是防御。
        let usable = samples.len() - samples.len() % ch;
        if usable == 0 {
            return None;
        }
        if self.inner.add_frames_f32(&samples[..usable]).is_err() {
            return None;
        }
        self.frames += (usable / ch) as u64;
        if self.frames < self.next_update_frames {
            return None;
        }
        self.next_update_frames = self.frames + (self.rate as f64 * MEASURE_UPDATE_SECS) as u64;
        Some(self.snapshot())
    }

    /// 立即取一次结果（解码结束时调用，不等节拍）。
    pub fn snapshot(&self) -> (f64, f64, f64) {
        let lufs = self.inner.loudness_global().unwrap_or(f64::NEG_INFINITY);
        let mut peak = 0.0f64;
        for c in 0..self.channels as u32 {
            if let Ok(p) = self.inner.sample_peak(c) {
                if p > peak {
                    peak = p;
                }
            }
        }
        (lufs, peak, self.measured_secs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_pushes_quiet_track_up_and_loud_track_down() {
        // 安静曲目（-24 LUFS，峰值有余量）→ 抬升 6 dB。
        let up = gain_db(-24.0, 0.3, -18.0).unwrap();
        assert!((up - 6.0).abs() < 1e-9, "{up}");
        // 现代响母带（-8 LUFS）→ 衰减 10 dB。
        let down = gain_db(-8.0, 1.0, -18.0).unwrap();
        assert!((down + 10.0).abs() < 1e-9, "{down}");
    }

    #[test]
    fn gain_never_clips() {
        // 安静但峰值满刻度：本该 +8 dB，被余量封到 -1 dB。
        let g = gain_db(-26.0, 1.0, -18.0).unwrap();
        assert!((g + 1.0).abs() < 1e-9, "{g}");
        // 施加后峰值仍在 0 dBFS 以下。
        assert!(1.0 * db_to_linear(g) < 1.0);
    }

    #[test]
    fn gain_unknown_for_silence() {
        assert!(gain_db(f64::NEG_INFINITY, 0.0, -18.0).is_none());
    }

    #[test]
    fn gain_is_clamped() {
        // 极端安静且峰值极小：不至于放大到失控。
        let g = gain_db(-90.0, 1e-6, -18.0).unwrap();
        assert!((g - MAX_GAIN_DB).abs() < 1e-9, "{g}");
    }

    #[test]
    fn disabled_leaves_samples_untouched() {
        let loud = TrackLoudness::new(LoudnessSettings::default(), String::new());
        let mut block = vec![0.5f32; 64];
        loud.apply(&mut block, 44100.0);
        assert!(block.iter().all(|s| (*s - 0.5).abs() < f32::EPSILON));
    }

    #[test]
    fn known_gain_applies_from_first_block() {
        let loud = TrackLoudness::new(
            LoudnessSettings {
                enabled: true,
                target_lufs: -18.0,
                known_gain_db: Some(-6.0),
                measure: false,
            },
            String::new(),
        );
        let mut block = vec![1.0f32; 64];
        loud.apply(&mut block, 44100.0);
        // -6 dB ≈ 0.501：开播即到位，不经斜坡。
        assert!((block[0] - 0.501).abs() < 0.01, "{}", block[0]);
        assert!((block[63] - 0.501).abs() < 0.01, "{}", block[63]);
    }

    #[test]
    fn adaptive_gain_slews_instead_of_jumping() {
        let loud = TrackLoudness::new(
            LoudnessSettings {
                enabled: true,
                target_lufs: -18.0,
                known_gain_db: None,
                measure: true,
            },
            String::new(),
        );
        // 测到一首很响的曲子（已测 4 秒，过了起测门槛）：目标 -10 dB，
        // 但一块（~46ms）之内只能挪一点点。
        loud.record(-8.0, 0.9, 4.0, false);
        let mut block = vec![1.0f32; 2048];
        loud.apply(&mut block, 44100.0);
        let applied = block[2047];
        assert!(applied < 1.0, "应开始衰减: {applied}");
        // 起播窗口内向下走快速率 6 dB/s：一块 ~46ms ≈ 0.28 dB（慢速率只有 0.05 dB）。
        assert!(applied < 0.98, "衰减应走快速率: {applied}");
        assert!(applied > 0.9, "单块变化不应超过 ~0.3 dB: {applied}");
        // 持续施加最终会收敛到目标附近。
        for _ in 0..2000 {
            let mut b = vec![1.0f32; 2048];
            loud.apply(&mut b, 44100.0);
        }
        let mut b = vec![1.0f32; 2048];
        loud.apply(&mut b, 44100.0);
        assert!((b[2047] - db_to_linear(-10.0)).abs() < 0.01, "{}", b[2047]);
    }

    /// 抬升方向不吃快速率：早期测量撞上安静前奏会高估增益，猛抬再退回来
    /// 就是一次听得出的「忽大忽小」。
    #[test]
    fn boost_never_rushes_even_early() {
        let loud = TrackLoudness::new(
            LoudnessSettings {
                enabled: true,
                target_lufs: -18.0,
                known_gain_db: None,
                measure: true,
            },
            String::new(),
        );
        // 刚测 4 秒（仍在快速窗口内），拿到一段很安静的音频 → 目标 +12 dB。
        loud.record(-30.0, 0.2, 4.0, false);
        let mut block = vec![1.0f32; 44_100]; // 整整 1 秒
        loud.apply(&mut block, 44_100.0);
        // 1 秒最多抬 1 dB ≈ ×1.122，而非快速率的 6 dB ≈ ×2.0。
        let applied = block[44_099];
        assert!(applied > 1.10 && applied < 1.13, "{applied}");
    }

    #[test]
    fn meter_measures_full_scale_tone() {
        let rate = 48_000u32;
        let mut meter = LoudnessMeter::new(2, rate).unwrap();
        // 5 秒 1kHz 正弦（幅度 0.5），双声道。
        let mut samples = Vec::with_capacity(rate as usize * 10);
        for i in 0..rate * 5 {
            let t = i as f64 / rate as f64;
            let v = (0.5 * (2.0 * std::f64::consts::PI * 1000.0 * t).sin()) as f32;
            samples.push(v);
            samples.push(v);
        }
        meter.push(&samples);
        let (lufs, peak, secs) = meter.snapshot();
        assert!((secs - 5.0).abs() < 0.01, "{secs}");
        assert!((peak - 0.5).abs() < 0.01, "{peak}");
        // 双声道 0.5 幅度正弦：均方 0.125/声道 → -0.691 + 10log10(0.25) ≈ -6.7 LUFS，
        // 再加 1kHz 处 K 计权约 +0.7 dB ≈ -6.0（满幅立体声正弦正好 ≈ 0 LUFS）。
        assert!(lufs > -7.5 && lufs < -4.5, "{lufs}");
    }
}
