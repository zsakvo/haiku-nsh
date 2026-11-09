# 任务 M1-02：播放核心（队列、编排、会话恢复、睡眠定时）+ Core 导出与事件桥

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。完成后提交一次并停下汇报。

## 先读

- `specs/PLAYBACK.md` §2、§3（业务规则的权威来源），`specs/ARCHITECTURE.md` §3、§4、§5。
- 参考版（只读，作为行为依据）：`~/Project/haiku-gpuix/src/core/{queue,playback,playback-clock,listening-progress}.ts` 及测试 `tests/{queue,playback,playback-clock}.test.ts`；参考版 `specs/SPEC.md` §6 中与队列、播放相关的条目。
- 已迁移的引擎：`native/crates/haiku-audio/src/playback.rs`（`Player`、`PlaybackEvent`、generation 语义）。
- `.agents/skills/nsh/references/mixed.md`（class、`Callback`、`Stop`、单一所有者镜像）。

## 范围

本任务只做**本地文件**播放；网络候选、插件 `streams`、预加载网络候选在 M4。不做任何界面（底栏、列表在 M1-03/M1-04），TSX 侧只写测试。

### 1. `haiku-model`

- `EntityRef { source, kind, id }`，以及无损的字符串编码/解码（来源 id 可能含 `#`，曲目 id 可能含 URL 字符）；`Track`（字段按 PLUGIN_SDK §3.2 的 Track，外加 `source`、可选 `local_path`）。
- 错误类型 `Error { code, message, retriable, … }`（ARCHITECTURE §5），先放本任务用到的错误码。

### 2. `haiku-playback`

- `Queue`：纯数据结构，不依赖音频。按 PLAYBACK §2 实现全部操作与四种模式（含随机历史、列表循环/单曲循环的区别、队首上一首 seek 0、删除当前项/清空/到末尾的规则）。把参考版 `queue.test.ts` 与 `playback.test.ts` 中覆盖队列语义的用例逐条移植为 Rust 测试，汇报中给出对照数量。
- `Controller`：依赖两个 trait，便于测试注入假实现：
  - `AudioBackend`（真实实现包装 `haiku_audio::Player`）：打开本地文件、预加载下一首本地文件、暂停、继续、停止、seek、音量、事件流。
  - `TrackResolver`：把 `Track` 解析为可播放的本地路径（本任务的实现只处理 `local_path`；接口要为 M4 的“下载 > 缓存 > 网络候选”留位置，但不要实现它们）。
- Controller 行为：单调 generation 分配，过期事件丢弃；短命令串行、打开可取消；暂停意图先发布；自然播完按模式前进；无缝预加载下一首（与模式一致，seek/队列变化时失效）；错误自动跳曲有上限且不绕回；短曲 Finished 不丢失。
- 睡眠定时：15/30/45/60/90 分钟、延长 15 分钟、播完当前曲后停止、取消、剩余时间；到期真正暂停。时间来源可注入，测试用假时钟。
- 会话持久化：通过 `haiku-store` 写 `session.json`（原子写入；最多 500 条；变更后 5 秒节流，关键操作立即写）；启动恢复为**暂停**状态，不自动打开音频。
- 对外暴露两类状态：低频的 `PlayerState`（当前曲、队列快照版本、播放/暂停/缓冲、模式、音量、睡眠定时剩余、错误），以及高频的位置/缓冲进度（原子量或 watch，**不**合进低频状态）。

### 3. `native/app`：`Core` 与事件桥

- 导出 `Core` class：`Core.open()` 初始化数据根（`HAIKU_DATA_DIR` 可覆盖）和播放服务；属性 `core.player` 暴露 `play(tracks, index)`、`toggle`、`pause`、`resume`、`next`、`previous`、`seek`、`setVolume`、`setMode`、队列操作、睡眠定时操作，以及 `onState(listener) → Stop`。导出都写文档注释。
- `CoreBridge`：在引擎线程用一个泵任务把核心事件写入 signal / `Callback.post`；播放位置按显示刷新节奏读取，只在播放中更新（暂停时停止）。位置不通过 `onState` 传给 TSX，而是给 M1-04 的 Rust 组件读取的 signal（本任务提供这个 signal 和一个读取它的最小测试即可）。
- 临时导出一个开发入口 `Core.devPlayFiles(paths)`（或同等方式），便于 M1-03 之前手动试播；在导出文档里标注“仅供开发，M1-03 删除”。

## 验收（每项跑一次）

1. `cargo test --workspace`：新增的队列、Controller（假后端 + 假时钟）、会话恢复、睡眠定时测试全部通过；原有测试不回退。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`。新增一个 `*.test.tsx`：用 `HAIKU_DATA_DIR` 指向临时目录，调用 `core.player` 的队列操作并通过 `onState` 断言状态变化（不需要真实出声，可以用 fixture 音频或不触发打开）。
3. 一次真实播放：`nsh run` 后通过开发入口播放 `.artifacts/m0-05/haiku-test-voice.m4a`，从日志确认位置增长、暂停后停止增长、播完后状态正确。不需要截图。
4. 提交：`M1-02: playback core, Core export and event bridge`，一并纳入本任务文件和 `specs/MILESTONES.md` 中尚未提交的修改。

## 汇报

STATUS.md：F06 改为“部分实现”（写明本地部分），追加 “M1-02” 小节（不超过 30 行）。最终回复：移植的参考测试对照、Controller 与 Bridge 的线程/时钟方案、偏离 PLAYBACK.md 的地方（如有）、需要审阅者决定的事项。
