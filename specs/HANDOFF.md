# 交接记录（2026-11-10）

换电脑继续时先读本文件，再读 `specs/SPEC.md`（决策 D1–D18）、`specs/MILESTONES.md`、`specs/STATUS.md`、`specs/tasks/README.md`。

## 工作方式

- 用户要求：**代码全部由 Codex 编写**，Claude 只负责写任务说明（`specs/tasks/<任务号>.md`）、派发、审阅证据、更新 specs。
- 每个任务都遵守 `specs/tasks/README.md` 的通用约定：只做列出的验收、每项跑一次、不做 GUI 自动化、失败最多两次修复、STATUS 小节有篇幅上限；当前 M1-03 续做不提交 git，完成后交审阅者。
- 本机已确认 Codex 在当前 Rex 会话 `session:01a18a85-6562-735f-9af5-41e1a0ea8fc2` 的右侧窗格 `block:01a1c2f7-c4a0-7591-bd83-d40c4e0a846e`；派发方式是 `rex send` + `rex send-key enter`，随后 capture 确认已进入执行。换会话或机器时重新检查，不复用旧窗格 ID。
- 执行者不得修改 Rex 会话名称或布局。

## 环境

- macOS arm64，Xcode；`nsh 0.4.0`（`curl -fsSL https://nsh.dev/install.sh | sh`）；cargo 用 huacnlee/cargo（`curl -fsSL https://huacnlee.github.io/cargo/install.sh | sh`，会替换 `~/.cargo/bin/cargo`，原版留为 `cargo.rustup`）。
- 参考项目（只读）：本机为 `~/Project/GPUI/haiku`（业务与行为依据，旧机器路径 `~/Project/haiku-gpuix` 已失效）；原 Flutter 版 `~/Project/Flutter/haiku`。
- 本机仓库有 `origin` remote，当前分支 main；本轮不提交或推送。`.artifacts/`（日志、截图、测试音频）被忽略，不随提交走；本机尚无 `.artifacts/m1-03/`，验收需保存新证据，不假定旧机器音频和日志存在。

## 已完成

| 阶段 | 提交 | 内容 |
|---|---|---|
| M0 | `77bb293` `f0c9c5c` `d70465f` `b03c3d5` | 骨架；自建 `haiku-jsrt`（基于 NSH 的 quickjs-jit，取代 CJRT，D11）；迁移参考版 Rust 核心到各领域 crate；平台验证 S1–S7（S8 多窗口不可行 → 设置改为覆盖层，D18）；S4/S5 经用户人工确认 |
| M1-01/01b/01c | `cf3fde5` `d7291cf` `e041101` `b364b75` | 设计系统、主题 token（17 品牌色，默认橙）、组件、外壳；用户已确认 UI。`b364b75` 由用户直接让 Codex 改：后退/前进/搜索在侧栏，下载/设置在侧栏底部 |
| M1-02 | `6631b82` | 播放核心：Queue、Controller（generation、预加载、错误跳曲）、睡眠定时、会话恢复；`Core`/`core.player` 导出与 CoreBridge（位置只走 Rust signal）。workspace 177 测试通过 |

## 2026-11-10 更新：M1-04 已验收并提交，下一步 M1-05

- M1-04：Dock、SeekBar、正在播放覆盖层、本地歌词、封面配色完成；nsh test 10/10、workspace 测试通过、浅深 Dock + 设置 + 正在播放截图已审阅。
- 已知缺口：真实 host 中 Rust 组件的 `animated_value` timing 不推进（Headless 正常；nsh-base Sheet 与自绘动画都卡在起点），正在播放覆盖层因此直接显示、无入场动画。Settings 覆盖层（TSX Sheet）正常。原因待查，疑与 CoreBridge 每帧 `next_frame` 有关；后续单独复核。
- 待人工确认：拖动手感、听感、队列/音量弹层。

## 2026-11-10 更新：M1-03 已验收并提交（62e85c2）

- M1-03 由 Codex 完成，审阅者已核对：workspace 180 通过/1 忽略，nsh test 7/7，真实扫描与播放日志、浅深截图（`.artifacts/m1-03/`）。unknown classes 在正式入口不复现。
- 遗留小项并入 M1-04 第 0 步：序号列颜色、`nsh.json` 测试 fs 授权。
- M1-04 任务：`specs/tasks/M1-04-player-dock-and-now-playing.md`。

## 已归档：M1-03 本地曲库 + TrackList（半成品已提交，尚未验收）

- 任务说明：`specs/tasks/M1-03-local-library.md`，含本机续做说明。
- 半成品保存于 `84c2a60`：已删除 `PlaybackSmoke.tsx` 与临时 Core 试播方法，新增 `haiku-library/src/service.rs`、可取消扫描和 `native/app/src/track_source.rs`；任务文件、UI 布局注记和 handoff 也已纳入。2026-11-10 本轮开始时工作区干净。
- 尚缺：`track_source` 模块注册、`core.library` 导出/事件桥、正式 TrackList、本地曲库 TSX 页面及验收证据。STATUS 的 F10 仍为未实现，不能因已有提交就进入 M1-04。
- 继续方式：先读 `84c2a60` 和现有半成品，再按任务文件补齐；本轮只保留改动和证据，不自动提交。审阅者本轮更新的任务/handoff 不覆盖。
- 待查：M1-02 试播出现 `unknown classes`（侧栏 token 类），仍按 M1-03 第 0 步处理。

## 后续顺序

M1-04 底栏 PlayerDock、SeekBar、正在播放覆盖层、本地歌词、封面配色 → M1-05 音效/响度/媒体缓存接线、设置的外观/播放/存储分组持久化、媒体中心与媒体键正式接入 → M2 插件 SDK（可与 M3 并行）→ M4 在线内容 → M5 下载/账号/同步/备份 → M6 macOS 深度集成。详见 `specs/MILESTONES.md`。

## 已知事项

- 允许的构建警告：`block 0.1.6` future-incompat、开发 host 的 `__eh_frame`。
- `haiku-macos` 三处 `clippy::mut_from_ref` 局部豁免，M6 复核。
- 玻璃底栏需用 `WithinWindow` 混合模式并覆盖滚动内容，M6 验收；M1 用 NSH 半透明回退。
- 截图前确认屏幕未锁定（锁屏时 `screencapture` 失败）。
- 本机（macOS 27，rustc 1.97.1）构建期依赖默认 `-C strip=debuginfo`，剥离后的 proc-macro dylib 被 dyld 拒绝（`mis-aligned LINKEDIT string pool`），表现为 `E0463 can't find crate for rquickjs_macro`。需关闭 build-override 的 strip（`CARGO_PROFILE_DEV_BUILD_OVERRIDE_STRIP=false` 或 profile 配置），清缓存无效。
