# 任务 M6-01：全局快捷键、快捷键设置页与关于页

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。M6 第一个任务。

## 先读

- `specs/MACOS.md` §4（全局快捷键）、§7（其他平台 no-op）、§8；`specs/UI.md` 键盘一条与设置“快捷键”“其他”分组；`specs/DATA.md` §6 关于页。
- 已有代码：`haiku-macos::{system,lifecycle}`、应用内快捷键与 keymap（`src/shell` 中的 `shortcuts`/`keymap`）、`haiku-store::settings`、设置“快捷键”分组占位、设置“其他”中已有的关于区块（M5-05 截图可见）。
- 参考版（只读，本机 `~/Project/haiku-gpuix`）：`native/src/shortcuts.rs` 及设置中快捷键字段名与默认绑定（D5）。
- **M5-01 教训**：effect 内 spawn 的任务会在 effect 重跑时被取消。不新增依赖 Rust `animated_value` 的界面。跑 nsh test 与截图前确认未锁屏。

## 1. 全局快捷键（Rust，`haiku-macos`）

- `GlobalShortcuts` trait，macOS 实现（Carbon `RegisterEventHotKey` 或参考版的方式），其他平台 no-op。
- 原子替换整组绑定：先校验（普通键必须带 Cmd/Ctrl/Option，F1–F20 可无修饰，组内重复、与应用内快捷键冲突）再注册；系统注册失败时报错并恢复之前的整组注册。
- 触发事件经 channel 回到 Core，走与界面相同的控制器（播放/暂停、上一首、下一首、音量、喜欢、显示主窗口、显示/隐藏桌面歌词——后者在 M6-03 前可先留空操作）。
- 导出 `core.shortcuts`：snapshot、apply(bindings)、resetOne(id)、resetAll、onChange → Stop。带文档注释。

## 2. 界面

- 设置“快捷键”：应用内与全局两组列表；录制（按下组合键）、冲突提示、单项恢复默认、全部恢复默认；不支持的平台隐藏全局组。
- 关于页（设置“其他”）：版本、NSH 版本、许可证与第三方声明入口；与应用菜单“关于”一致。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：校验规则（裸字母/仅 Shift 拒绝、F 键允许、重复、冲突）、原子替换与失败回滚（用假注册后端模拟系统拒绝）、设置持久化与参考版字段兼容。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`（全绿）。Headless：录制新组合 → 冲突提示 → 单项恢复默认 → 全部恢复默认。
3. 截图（浅色，fixture）到 `.artifacts/m6-01/`：快捷键设置（含冲突提示）、关于页。
4. 收尾：更新 STATUS（F09 全局快捷键、F16 相关；追加 M6-01 小节 ≤ 40 行），保留改动等待审阅。

## 不做

菜单栏（M6-02）、桌面歌词（M6-03）、玻璃底栏（M6-04）、打包（M6-05）。

## 汇报

最终回复：`core.shortcuts` API、默认绑定与参考版兼容、注册实现方式、偏离、待人工确认项（真实全局按键、与系统快捷键冲突）、需审阅者决定的事项、未完成项。
