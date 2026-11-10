# 任务 M1-04：PlayerDock、SeekBar、正在播放覆盖层、本地歌词、封面配色

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。完成后保留工作区改动并停下汇报，不提交 git（提交由审阅者在用户确认后执行）。

## 先读

- `specs/UI.md` §1.2（`now-playing-*`、`lyric-*`、`dock-surface`、`progress-*` token）、§4 正在播放、§5 PlayerDock；`specs/PLAYBACK.md` §3、§6（歌词来源链、Cursor、LyricsView/KaraokeLine、进度条拖动规则）。
- NSH Design Guides、Coding Guides（重点：Rust 组件、overlays、motion、Lists）、`src/ui/STANDARDS.md`。API 以本机 NSH 0.4.0 源码和 `.nsh/types` 为准。
- 参考版行为（只读）：`~/Project/GPUI/haiku` 中底栏、正在播放、歌词视图、`core/current-lyrics.ts`、`services/lyrics*.ts` 的本地部分。
- 已有代码：`native/app/src/core.rs`（`CoreBridge` 的 `position/buffered/state` signal 只给 Rust 组件用）、`native/app/src/track_list.rs`、`haiku-lyrics`（parse/parse_krc/merge_translation）、`haiku-palette`（extract_file/fallback_palette）、`haiku-library` 的 sidecar 扫描。
- 构建环境：先 `export CARGO_PROFILE_DEV_BUILD_OVERRIDE_STRIP=false CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP=false`（见 README）。

## 0. M1-03 审阅遗留（小改）

- TrackList 序号列：非当前曲目用 `muted-foreground`，只有播放指示用 `primary`（UI.md §5：只有主要播放控件使用强调色）。
- `nsh.json` 中 `/tmp/haiku-m1-03-fixtures`、`.artifacts/m1-03` 的 fs 授权属于测试/预览，在汇报中说明能否改为仅测试/预览生效；如 NSH 0.4.0 不支持按入口区分，保留并在 STATUS 记为“M6 打包前移除”，不扩大授权。

## 1. PlayerDock（Rust 组件，应用根部唯一实例）

- 挂在 AppShell 根部，主内容和设置覆盖层打开时都可见可操作；左右与底部留 24pt（或按当前外壳 token 调整并说明），侧栏底部已预留 96pt。
- 内容：封面（无封面时占位）、标题/艺术家、上一首/播放暂停/下一首、SeekBar、喜欢（**M1 不显示**，喜欢在 M3）、播放模式、音量（Popover + Slider）、队列（Popover：当前队列虚拟列表，点击跳转、删除）、歌词按钮（打开正在播放覆盖层）。
- 视觉：`dock-surface` 半透明表面，中性灰阶，仅中间播放按钮用强调色；位置/缓冲直接绑定 `CoreBridge` signal，不经 TSX。
- 空队列：显示禁用状态而不是隐藏（布局不跳动）。

## 2. SeekBar（Rust 组件）

- 已播放、缓冲为两个独立图层（`progress-played`、`progress-buffered`）。
- 拖动规则严格按 PLAYBACK.md §6.3：水平移动 1 逻辑像素后激活；按下时记录 left/width/duration；预览位置 = `(pointerX - left) / width * duration`；松开只提交一次 seek；非主键、按钮、封面上按下不触发。拖动预览时不被播放位置覆盖。
- 显示当前/总时长；时长未知时禁用拖动。

## 3. 正在播放覆盖层

- 点击底栏封面或歌词按钮，从底部升起覆盖整个窗口（含侧栏、顶栏）；收起动画结束后恢复原页面；不增加导航历史；Esc 收起；底栏不卸载、不重建。
- 双栏：左侧大封面（带唱片装饰），右侧标题/艺术家/专辑与 LyricsView。
- 背景：封面配色 palette index 3 向黑压暗 50%，叠加顶部 50%、底部 70% 黑色渐变；始终深色角色；无封面回退主题强调色。配色在后台计算，切歌时丢弃过期结果；通过运行时覆盖 `now-playing-*`、`lyric-*` token 使用，组件只引用 token。
- 预热：当前曲和下一首（按当前模式）的本地封面预先计算配色并缓存（M1 只有本地封面；网络封面下载留 M4），切歌首帧即为正确背景。
- 正在播放页内用封面色绘制进度：已播放 tone 15、缓冲 tone 38（若 palette 输出不含这些 tone，说明实际取值）。

## 4. 本地歌词

- 来源链 M1 只实现本地部分：sidecar（同名 `.lrc`，优先）→ 内嵌歌词；单文件 ≤ 1 MiB；UTF-8/GBK/UTF-16；附加译文 sidecar（按参考版命名规则）。手动覆盖、缓存、插件、网络提供方留 M4，接口预留但不显示。
- 触发：当前曲目或其有效元数据变化即加载，不依赖起播成功；恢复到暂停的曲目也加载；切歌/清空先取消旧请求，保证不串歌词。
- `haiku-lyrics::Cursor`：按时钟二分查找当前行，当前行变化才更新 signal（新增 Rust 单元测试）。
- `LyricsView`（Rust）：当前行居中跟随、非当前行渐隐、用户滚动后数秒自动回到当前行、点击行 seek、只挂载可见窗口内的行；纯文本歌词静态显示；无歌词显示空状态。
- `KaraokeLine`：有逐字时间时按字填充，宽度绑定时钟 signal；暂停、缓冲、不可见时停止推进。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：新增 Cursor 二分/边界、歌词 sidecar 优先级与编码（UTF-8/GBK/UTF-16）、1 MiB 上限、配色过期结果丢弃的测试。
2. `nsh check`、`nsh build`（仅已知例外：block 0.1.6、开发 host `__eh_frame`、huacnlee/cargo 忽略 target-dir 提示）、`nsh test`。Headless 覆盖：
   - SeekBar 拖动规则：< 1px 不激活、激活后预览值正确、松开只发一次 seek、非主键不触发。
   - 底栏播放/暂停/下一首驱动 `core.player` 状态；空队列禁用。
   - 打开/收起正在播放：导航历史不变、原页面状态保留、底栏实例未重建（例如同一 testID 节点或计数器验证）。
   - 歌词：点击某行 seek 到该行开始时间；切到另一首后不残留上一首歌词。
3. 真实运行：用带 sidecar LRC 和内嵌封面的测试音频（自行生成在 `.artifacts/m1-04/`，不提交），`nsh run` 截图：本地曲库页 + 底栏播放中（浅色、深色各一张）、正在播放覆盖层一张（有封面配色背景和滚动歌词）。日志确认播放与歌词行推进。
4. 收尾：保留改动，更新 STATUS（F06 补充界面部分、F08 改“部分实现（本地歌词）”，追加 M1-04 小节 ≤ 40 行），等待审阅。

## 不做

网络歌词与歌词提供方、手动查找歌词、桌面歌词、媒体中心正式接入、音效/响度、防息屏、玻璃底栏（M6）、喜欢（M3）。

## 汇报

最终回复：组件接口、偏离 UI.md/PLAYBACK.md 的地方、待人工确认项（真实拖动手感、动画观感、听感）、需审阅者决定的事项。
