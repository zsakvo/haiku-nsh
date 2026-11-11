# 任务 M6-02：菜单栏组件

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。

## 先读

- `specs/MACOS.md` §2（菜单栏组件）、§7、§8；`specs/UI.md` 设置中与菜单栏/歌词相关的项。
- 已有代码：`haiku-macos::{menubar, menubar_api}`（M0 迁入，M0-05 曾在 spike 中验证过安装与按钮回调，之后接线已移除）；M6-01 的 `native/app/src/shortcuts.rs` 是事件经 channel 回到 Core、走同一控制器的现成范例；歌词当前行与逐字时间来自已有的歌词状态。
- 参考版（只读，本机 `~/Project/haiku-gpuix`）：`native/src/menubar.rs`、`native/src/api/menubar.rs`、`src/core/menubar.ts`、`src/services/settings.ts` 中 `menubarLyricsVisible`、`menubarLyricHighlight` 字段，`specs/ACCEPTANCE_NATIVE.md` 菜单栏一节。
- **教训**：effect 内 spawn 的任务会在 effect 重跑时被取消，effect 只读真正决定请求的信号；不新增依赖 Rust `animated_value` 的界面。跑 nsh test 与截图前确认未锁屏。

## 1. 接线（Rust）

- 应用启动时安装唯一的状态栏组件（fixture/Headless 不安装）；其他平台 no-op。
- 推送：歌曲信息或逐字歌词行（随播放/暂停/seek 同步，暂停时停止推进）、播放状态、喜欢状态、高亮色（设置开关）。只在值变化时推送，不按帧轮询。
- 事件经 channel 回到 Core，走与界面相同的控制器：上一首/播放暂停/下一首/喜欢、显示主窗口、歌词显示切换、退出。
- 菜单：当前队列（菜单项携带曲目身份，选择时若队列已变化则拒绝并忽略）、可写入的歌单（加入当前曲目）、显示主窗口、菜单栏歌词显示、退出。
- 点击命中使用真实屏幕指针坐标（MACOS.md §2），保留参考版的修复。
- 设置字段 `menubarLyricsVisible`、`menubarLyricHighlight` 与参考版兼容，放在设置“歌词”分组（或参考版所在分组）。
- debug 构建提供诊断快照（当前文本、播放/喜欢状态、菜单项数），供测试与审阅读取。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：推送去重、过期队列项拒绝、事件到控制器的映射（用假后端，不碰 AppKit）、设置字段兼容。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`（全绿）。Headless：设置开关切换后持久化。
3. `nsh run` 真实窗口：播放测试音频后截一张含菜单栏组件的屏幕截图到 `.artifacts/m6-02/`，并保存诊断快照日志。按钮点击、菜单选择列为待人工确认。
4. 收尾：更新 STATUS（F09 菜单栏相关；追加 M6-02 小节 ≤ 40 行），保留改动等待审阅。

## 不做

桌面歌词窗口（M6-03，菜单中的“桌面歌词”项可先留空操作）、玻璃底栏（M6-04）、打包（M6-05）。

## 汇报

最终回复：接线方式与推送时机、菜单项与过期拒绝、设置字段兼容、偏离、待人工确认项、需审阅者决定的事项、未完成项。
