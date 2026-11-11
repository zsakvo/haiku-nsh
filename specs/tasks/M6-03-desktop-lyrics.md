# 任务 M6-03：桌面歌词

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。

## 先读

- `specs/MACOS.md` §3（桌面歌词）、§7、§8；`specs/UI.md` 播放设置入口（桌面歌词）。
- 已有代码：`haiku-macos::{desktop_lyrics, desktop_lyrics_api}`（M0 迁入的 NSPanel 实现，约 2200 行，尚未接线）；M6-02 的 `native/app/src/menubar.rs` 与 `platform/menubar.rs` 是投影 + 单一 worker + 变化/行边界唤醒的现成范例，按同一模式接线；M6-01 快捷键中的“显示/隐藏桌面歌词”与 M6-02 菜单中的桌面歌词项目前为空操作，本任务接通。
- 参考版（只读，本机 `~/Project/haiku-gpuix`）：`native/src/desktop_lyrics.rs`、`src/core/desktop.ts`、`src/services/settings.ts` 的 `desktopLyrics` 字段，`specs/ACCEPTANCE_NATIVE.md` 桌面歌词一节。
- **教训**：effect 内 spawn 的任务会在 effect 重跑时被取消，effect 只读真正决定请求的信号；不新增依赖 Rust `animated_value` 的界面。跑 nsh test 与截图前确认未锁屏。

## 1. 接线（Rust）

- 复用已有 NSPanel 实现，尽量不重写；只在接线需要时修改 `haiku-macos`。fixture/Headless 不创建面板；其他平台 no-op。
- 推送：当前行/译文或下一行（按布局模式）、逐字时间、播放/暂停/seek 进度、喜欢状态、样式；只在变化或行边界推送，不按帧轮询。没有歌词时显示歌曲名与艺术家。
- 事件经 channel 回到 Core 的同一控制器：工具栏 8 个按钮（上一首、播放、下一首、喜欢、锁定、显示主窗口、歌词设置、隐藏）、拖动结束保存位置。
- 布局策略（模式、行数、缩小到最小可读字号后再平移）写成纯函数。
- 状态：可见性、锁定、样式存在设置 `desktopLyrics`（与参考版字段兼容）；位置存 `haiku_state/desktop-lyrics-position.json`（中心点、Y、宽度，默认宽 600pt，启动时限制在已连接显示器内）。
- debug 构建提供诊断快照（可见、锁定、当前文本、帧）。

## 2. 界面

- 设置“歌词”分组中的桌面歌词区块：显示开关、锁定、布局模式、字体（NSFontPanel）、字号 14–60、字重、颜色、描边、阴影、显示译文、背景（透明/纯色/模糊）。分组内已有菜单栏歌词项，保持一起。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：布局纯函数（各模式行数、过长行缩小与平移）、位置限制到显示器、推送去重、事件到控制器映射（假后端）、设置字段兼容。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`（全绿）。Headless：桌面歌词设置项修改后持久化。
3. `nsh run` 真实窗口：播放测试音频、打开桌面歌词，截一张屏幕截图到 `.artifacts/m6-03/`（只截面板所在区域，避免截入无关窗口），并保存诊断快照日志。锁定后悬停解锁、拖动、工具栏点击、字体面板列为待人工确认。
4. 收尾：更新 STATUS（F09 桌面歌词相关；追加 M6-03 小节 ≤ 40 行），保留改动等待审阅。

## 不做

玻璃底栏与 KaraokeLayer（M6-04）、打包（M6-05）。

## 汇报

最终回复：接线方式与推送时机、布局纯函数、位置与设置兼容、偏离、待人工确认项、需审阅者决定的事项、未完成项。
