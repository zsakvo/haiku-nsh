# 任务 M7-03：原生逐字歌词层 KaraokeLayer

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。在主仓库当前工作区上继续，不提交 git。前置：M7-02 已完成。

## 背景

- MACOS.md §5 中的 `KaraokeLayer` 当时暂缓。用户现在要求正在播放的歌词区域改用原生方案，以提升性能。
- 现状：`native/app/src/lyrics_view.rs` 的 `karaoke_line` 为每个字建两个 text，并在每帧读 `position` 改裁剪宽度。播放时每帧都会触发 NSH 重新布局与重绘。
- 参考实现：`~/Project/haiku-gpuix/native/karaoke.m`（只读）。做法是用 CoreText 把整行排版一次，栅格化成暗、亮两张图；每个字片段是一个裁剪层，由 Core Animation 根据（位置、是否播放、epoch）自己推进宽度。只在播放、暂停、seek、切行时同步，不按帧同步。

## 1. 原生组件（`native/crates/haiku-macos/src/karaoke.rs`）

- 按 `glass.rs` 的方式注册原生组件 `haiku.KaraokeLayer`（NSView + CALayer，用 objc2/现有依赖，不新增 .m 文件）。把 karaoke.m 的 build/sync 逻辑移植过来：CoreText 排版、字片段按行拆分、按宽度比例分摊时间、`removedOnCompletion=NO` 的宽度动画、漂移超过 0.35s 才重排时间线。
- 属性：`words`（text/start/end）、`font`/`size`/`weight`、`color`/`dim`（来自主题 token 的解析值，组件内不写颜色字面量）、`clock`（`{position, playing, epoch}`）。
- 原生视图不接收点击（hitTest 返回 nil）；点击行跳转仍由 NSH 外层按钮处理。提供无障碍文本。
- **高度**：这一行的高度必须由 NSH 布局知道。先查 NSH 原生组件有没有测量 / 固有尺寸接口。没有的话，在 Rust 侧用同样字体的 NSH `text` 做一个不可见的占位来撑开高度，原生层叠在上面。两种都不可行就停下汇报（属于架构问题）。

## 2. 接入（`native/app/src/lyrics_view.rs`）

- 只有当前行使用 KaraokeLayer。其余行保持 NSH 文本（暗色/已唱色），不建每字视图。
- 时钟只在播放状态变化、seek、切曲、当前行变化时发送（`epoch` 在 seek/切曲时递增），**不在每帧发送 position**。
- 非 macOS、或原生组件注册不可用时，回退到现有 `karaoke_line`，并保留现有 Headless 测试。
- 没有逐字时间的歌词（lrc）不走原生层。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml -p haiku-macos` 与 app crate 测试通过：新增纯函数测试覆盖字片段时间分摊和漂移判定（把这两段逻辑写成可以脱离 AppKit 测试的函数）。
2. `nsh check`、`nsh test` 通过；回退路径的现有 Headless 测试保持通过。新增断言：播放 3 秒内当前行的 clock 属性最多更新一次（没有 seek 的情况下）。
3. 真实窗口：`nsh run` 播放一首有逐字歌词的本地夹具，截一张图，存档日志。
4. CPU：按 M6-04 的方法（`.artifacts/m6-04/cpu.log`）在 debug 构建中播放 60 秒、打开正在播放，记录平均和区间，和 M6-04 的 14.3% 对比。只测一次。
5. 收尾：MACOS.md §5 不改（审阅者负责）；STATUS F08/F09 行更新；追加 M7-03 小节（≤ 40 行）；日志放 `.artifacts/m7-03/`。

## 不做

不改桌面歌词与菜单栏歌词（可以在汇报中说明能否复用）；不改歌词布局、字号和配色；不改 NSH。

## 汇报

高度方案；时钟同步时机；改动的文件；CPU 对比；验证结果；待人工确认项（真实逐字填充的平滑度、换行、字体回退）。
