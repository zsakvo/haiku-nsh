# 任务 M1-05：音效与响度、媒体缓存、设置持久化、媒体中心正式接入

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。完成后保留工作区改动并停下汇报，不提交 git（审阅通过后由审阅者提交）。这是 M1 最后一个任务。

## 先读

- `specs/PLAYBACK.md` §4（EQ/效果/响度）、§5（媒体缓存）；`specs/DATA.md` §1–§2（数据根、`settings.json`）；`specs/UI.md` §6（设置覆盖层）；`specs/MACOS.md` §1（媒体中心、媒体键、后台播放、防息屏）。
- 参考版（只读）`~/Project/GPUI/haiku`：设置页外观/播放/存储分组、音效面板、`settings` 服务及字段名（保持兼容，D5）。
- 已有代码：`haiku-audio`（`effects.rs` EffectsSettings/EffectsConfig、`loudness.rs`、`media_cache.rs` set_root/set_enabled/set_max_bytes/pin）、`haiku-store`（data_root、json_file 原子写）、`haiku-macos`（system.rs 媒体中心、menubar 不在本任务、lifecycle.rs 关闭隐藏已接入）、`native/app/src/{core,player_dock,now_playing}.rs`、`src/shell/Settings.tsx`、`src/ui/AppearanceOptions.tsx`。M0-05 曾验证 system.rs 的 snapshot/回调经 channel 回到引擎线程，可参考该路径（spike 代码已删，看 git 历史 `d70465f`/`b03c3d5`）。
- 构建前按 README 设置 `CARGO_PROFILE_{DEV,RELEASE}_BUILD_OVERRIDE_STRIP=false`。
- 已知限制：真实 host 中 Rust 组件的 `animated_value` 动画不推进（HANDOFF）。本任务不要新增依赖动画的 UI；不负责修复它。

## 1. 设置服务（Rust，haiku-store）

- `settings.json`：schema 版本号、原子写、按字段合并；未知字段原样保留（兼容参考版与移动端字段）；越界值 clamp。修改由 Rust 串行执行并落盘，再发布变更事件；TSX 只做镜像。
- 导出 `core.settings`：`get`、`update(patch)`、`onChange → Stop`。导出写文档注释。
- 外观（模式、品牌色）从当前 TSX 内存信号迁移为持久化设置，启动时恢复，保持已确认的视觉效果不变。

## 2. 音效与响度（F07）

- 十段 EQ（31Hz–16kHz，±12dB）、预设与自定义、前级增益、声场宽度、左右平衡、单声道、软削波、渐入渐出；响度均衡（EBU R128，目标 -18 LUFS，峰值限制，测量结果按曲目缓存，只信任完整覆盖或 ≥60 秒/≥半首的测量，较差测量不覆盖较好结果）。
- 所有参数真实传入 DSP；切歌后不丢失；越界 clamp，不出现 NaN/clipping（用已迁移的单元测试 + 新增接线测试证明）。
- 设置覆盖层“播放”分组：音效开关、EQ（Slider 组 + 预设 Select）、其他效果、响度均衡开关；“正在播放”覆盖层与 Dock 不新增入口（播放设置入口留后续）。

## 3. 媒体缓存（F13 缓存部分）

- 接线 `media_cache`：根目录为数据根下 `media-cache/`；开关、容量 512 MiB–20 GiB、占用统计、清理、LRU、当前与下一首 pin、失败条目清理、完整性标记。缓存键 `(EntityRef, 音质)`。
- 本地曲库文件不进入缓存，清理缓存不得删除扫描到的原始文件（测试证明）。M1 没有网络播放，缓存写入路径只需接通并由单元测试覆盖，真实写入留 M4。
- 设置“存储”分组：缓存开关、容量 Select/Slider、当前占用、清理（需确认）；本地曲库扫描根管理入口可链接到本地曲库页的“管理文件夹…”。

## 4. 媒体中心、媒体键、防息屏（MACOS.md §1）

- `MPNowPlayingInfoCenter`：标题、艺术家、专辑、时长、位置、播放状态、本地封面、能否上一首/下一首；位置在播放状态/seek 变化时更新，不每帧写。
- `MPRemoteCommandCenter`：play/pause/toggle/stop/next/previous/seek/repeat/shuffle 走与界面相同的 `Controller`；favorite 暂不注册（M3）。
- 防息屏：设置开启且正在播放覆盖层可见时持有 `PreventUserIdleDisplaySleep`，否则释放；退出时清理媒体信息并释放 assertion。
- 关闭隐藏/Dock 恢复已在 M1-01 接入，本任务只做回归确认，不重写。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：新增 settings 原子写/未知字段保留/clamp、音效参数传入 DSP 与切歌保持、响度测量优劣覆盖规则、缓存容量与 LRU/pin、清理不删除本地文件的测试。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`。Headless：外观设置修改后重启（重新 open Core）仍保留；播放分组改 EQ 后 `core.settings` 与播放器配置一致；存储分组清理需确认。
3. 真实运行：`nsh run` 播放本地测试音频，截图设置“播放”“存储”分组（浅色各一张）；日志记录媒体中心 snapshot（标题、状态 1→2）。媒体键/控制中心按钮、关闭隐藏后继续播放、Dock 恢复、Cmd+Q 列为待人工确认。
4. 收尾：保留改动，更新 STATUS（F07、F09（媒体中心部分）、F13（缓存部分）、F16（设置部分）；追加 M1-05 小节 ≤ 40 行），等待审阅。

## 不做

菜单栏组件、桌面歌词、全局快捷键、玻璃底栏（M6）；网络播放与缓存真实写入、下载（M4/M5）；喜欢（M3）；动画修复。

## 汇报

最终回复：设置 schema 与参考版字段的兼容情况、偏离 PLAYBACK/MACOS/DATA 的地方、待人工确认项、需审阅者决定的事项。
