# 交接记录（2026-11-10）

换电脑继续时先读本文件，再读 `specs/SPEC.md`（决策 D1–D18）、`specs/MILESTONES.md`、`specs/STATUS.md`、`specs/tasks/README.md`。

## 工作方式

- 用户要求：**代码全部由 Codex 编写**，Claude 只负责写任务说明（`specs/tasks/<任务号>.md`）、派发、审阅证据、更新 specs。
- 每个任务都遵守 `specs/tasks/README.md` 的通用约定：只做列出的验收、每项跑一次、不做 GUI 自动化、失败最多两次修复、STATUS 小节有篇幅上限、只提交一次。
- 原机器上 Codex 运行在 Rex 会话 `haiku-nsh-player` 右侧窗格（block `…076c11`）；派发方式是 `rex send` + `rex send-key enter`（常需按两次 Enter），用 Monitor 轮询窗格中是否还有 `esc to interrupt`。新机器需重新确认 Codex 所在窗格。
- Codex 会改 Rex 会话名，发现后改回。

## 环境

- macOS arm64，Xcode；`nsh 0.4.0`（`curl -fsSL https://nsh.dev/install.sh | sh`）；cargo 用 huacnlee/cargo（`curl -fsSL https://huacnlee.github.io/cargo/install.sh | sh`，会替换 `~/.cargo/bin/cargo`，原版留为 `cargo.rustup`）。
- 参考项目（只读）：`~/Project/haiku-gpuix`（业务与行为依据），原 Flutter 版 `~/Project/Flutter/haiku`。新机器上需有这两个目录，任务说明里引用了它们的路径。
- 仓库**没有 git remote**。`.artifacts/`（日志、截图、测试音频 `.artifacts/m0-05/haiku-test-voice.m4a`）被忽略，不随提交走；需要时整目录复制或重新生成。

## 已完成

| 阶段 | 提交 | 内容 |
|---|---|---|
| M0 | `77bb293` `f0c9c5c` `d70465f` `b03c3d5` | 骨架；自建 `haiku-jsrt`（基于 NSH 的 quickjs-jit，取代 CJRT，D11）；迁移参考版 Rust 核心到各领域 crate；平台验证 S1–S7（S8 多窗口不可行 → 设置改为覆盖层，D18）；S4/S5 经用户人工确认 |
| M1-01/01b/01c | `cf3fde5` `d7291cf` `e041101` `b364b75` | 设计系统、主题 token（17 品牌色，默认橙）、组件、外壳；用户已确认 UI。`b364b75` 由用户直接让 Codex 改：后退/前进/搜索在侧栏，下载/设置在侧栏底部 |
| M1-02 | `6631b82` | 播放核心：Queue、Controller（generation、预加载、错误跳曲）、睡眠定时、会话恢复；`Core`/`core.player` 导出与 CoreBridge（位置只走 Rust signal）。workspace 177 测试通过 |

## 进行中：M1-03 本地曲库 + TrackList（未提交）

- 任务说明：`specs/tasks/M1-03-local-library.md`。
- 交接时 Codex 会话被中断，工作区有未提交的半成品：删除了 `PlaybackSmoke.tsx`，改动 `native/app/src/core.rs`、`haiku-library`（新增 `service.rs`），新增 `native/app/src/track_source.rs`。`specs/UI.md` 的修改（记录 `b364b75` 的布局）和任务文件也未提交。
- 继续方式：在新机器上让 Codex 重新执行 M1-03 任务说明，说明“工作区已有部分实现，先阅读 `git diff` 再继续”。
- 待查：M1-02 试播出现 `unknown classes`（侧栏 token 类），已作为 M1-03 第 0 步。

## 后续顺序

M1-04 底栏 PlayerDock、SeekBar、正在播放覆盖层、本地歌词、封面配色 → M1-05 音效/响度/媒体缓存接线、设置的外观/播放/存储分组持久化、媒体中心与媒体键正式接入 → M2 插件 SDK（可与 M3 并行）→ M4 在线内容 → M5 下载/账号/同步/备份 → M6 macOS 深度集成。详见 `specs/MILESTONES.md`。

## 已知事项

- 允许的构建警告：`block 0.1.6` future-incompat、开发 host 的 `__eh_frame`。
- `haiku-macos` 三处 `clippy::mut_from_ref` 局部豁免，M6 复核。
- 玻璃底栏需用 `WithinWindow` 混合模式并覆盖滚动内容，M6 验收；M1 用 NSH 半透明回退。
- 截图前确认屏幕未锁定（锁屏时 `screencapture` 失败）。
