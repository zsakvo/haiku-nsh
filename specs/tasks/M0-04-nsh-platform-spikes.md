# 任务 M0-04：数据目录修正 + 平台验证 S5、S6、S8

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。完成后停下汇报。

本任务只验证 NSH 自身的能力（窗口、列表、后台音频）。需要原生 macOS 代码的 S3、S4、S7 放在 M0-05。所有验证代码都是 spike，集中放在 `native/app/src/spike/` 和 `src/spike/`，M1 会整体删除；不要为它们建立设计系统。

## 0. 修正默认数据目录

`haiku-store::data_root` 目前沿用了参考版的默认目录 `Haiku GPUIX`，违反 D5（必须使用独立的数据根，不能读写参考版的数据）。按 DATA.md §1 改为 `~/Library/Application Support/Haiku NSH`，并保留 `HAIKU_DATA_DIR` 覆盖。如果有对应的单元测试，相应更新。

## S5：后台播放与窗口恢复

- 在 `native/app` 中导出最小的播放接口：`Spike.playLocal(path)`、`Spike.pause()`、`Spike.position()`，内部调用 haiku-audio 的引擎接口（与 M0-03 迁移的 playback 接口保持一致，不新增业务逻辑）。
- 测试音频：用 haiku-library 的 `test_support` 夹具，或者用一个本地生成的 30 秒以上的 FLAC/MP3（放在被忽略的 `.artifacts/m0-04/`），不要提交音频文件。
- 查阅 `nsh/desktop`（`.nsh/types` 和 nsh-core 源码）中关于窗口关闭和隐藏、应用重新激活的 API。目标行为：Cmd+W 或点击关闭按钮只隐藏窗口，进程和音频继续运行；应用被重新激活时恢复窗口。
- 验收：
  1. `nsh run` 播放后按 Cmd+W，等待 10 秒，确认 `position()` 仍在增长（从日志读取，不要求人工听）。
  2. 执行 `open -a <运行中的应用>` 或等效的 CLI 方式重新激活应用，窗口恢复（截图）。
  3. 如果 NSH 不提供“关闭时隐藏”或“重新激活”的钩子：记录查过的 API 和结论，说明需要哪些原生补充（进入 M0-05 或 MACOS.md §1），不要修改 NSH。
- 按 Cmd+W、点关闭按钮这类真实按键操作，如果无法用 CLI 触发，就由你实现对应的窗口操作入口（例如应用菜单的 Close Window），用菜单的快捷键测试；仍无法触发时，列为“待人工确认”。

## S6：1 万行虚拟列表

- 在 Rust 中写一个 `SpikeTrackList` 组件，基于 `virtual_list`（先读 rust-ui.md 和对应的 elements 文档），数据为 10000 条假曲目（标题、艺术家、时长）；行高固定，行内只用 token 颜色。
- 每行有一个“选中”状态，存放在以 id 为 key 的数据中，不能用行内 signal（Coding Guides 的列表规则）。
- 验收：
  1. Headless 测试：跳转到第 9000 行可以看到对应标题；选中第 5 行后滚动到底部再回来，选中状态仍在第 5 行，并且没有被回收到其他行。
  2. `nsh run` 截图一张（列表滚动到中间位置）。
  3. 如果 `nsh test --trace` 或 tooling.md 中的 trace 能用一条命令给出帧耗时，就记录一次滚动的数据；不能的话跳过，不要自行搭建测量工具。

## S8：第二个窗口共用 Core

- 用 `nsh/desktop` 的窗口 API，在主窗口中加一个按钮“打开设置窗口”；第二个窗口与主窗口都显示同一个 Rust 计数器（Rust 持有，TSX 通过监听镜像，参照 mixed.md 的单一所有者模式），两个窗口都有“+1”按钮。
- 验收：
  1. 在任一窗口点击 +1，两个窗口同时更新（如果 Headless 支持多窗口就写测试，否则以 `nsh run` 截图为准，每个窗口各一张）。
  2. 重复点击“打开设置窗口”时，激活已有窗口，不再新建。
  3. 关闭设置窗口后，主窗口和计数器不受影响。

## 汇报

- STATUS.md：更新 S5、S6、S8 三行，追加“M0-04”小节（不超过 40 行）。
- 提交一次：`M0-04: data root fix, NSH platform spikes (S5, S6, S8)`。
- 最终回复：每项验证的结论（通过、失败或待人工确认）；NSH 的窗口与列表 API 中有哪些限制会影响 UI.md 或 MACOS.md 的设计；建议审阅者修改哪些 specs。
