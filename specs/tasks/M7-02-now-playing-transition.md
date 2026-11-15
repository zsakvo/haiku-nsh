# 任务 M7-02：正在播放的入场 / 离场动画

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。在主仓库当前工作区上继续，不提交 git。前置：M7-01 已完成。

## 背景

- UI.md §4 要求正在播放从底部升起、收起动画结束后才卸载。M1-04 时 Rust 组件的 `animated_value` timing 在真实 host 中不推进（Headless 正常；TSX Settings Sheet 正常），所以改成了静态切换（STATUS M1-04 小节、AUDIT G22）。当时的怀疑是 CoreBridge 播放时每帧 `next_frame` 读位置，与动画驱动冲突，但没有验证。
- 用户现在要求恢复动画。

## 1. 先定位原因（限时）

1. 在 `NowPlayingPreview.tsx` 旁新建一个最小夹具（不启动 Core、不播放），只放一个 Rust 组件用 `animated_value` 做 250ms 位移。`nsh run` 真实窗口看它是否推进。再在启动 Core 并播放的夹具里看一次。用这两次结果判断是 NSH 本身的问题，还是 CoreBridge 帧循环的问题。
2. 先看 NSH 文档（`nsh` skill 的 Rust 动画、`transition_*` 样式过渡章节）确认用法是否正确。
3. 最多尝试两种修复思路：
   - 如果是 CoreBridge 的问题：修正帧循环（例如不独占 `next_frame`，或改为与动画驱动共存的方式）。
   - 如果是 animated_value 的问题，或者原因定位不了：改用 NSH 的样式过渡（`transition_transform`/`transition_opacity` + `duration`）实现同样的效果。
4. 两种都不行就停下，汇报现象、最小夹具和结论。**不改 NSH 源码。** 最小夹具在确定方案后删除，或者改成正式的 Headless 回归测试。

## 2. 动画规格（`native/app/src/now_playing.rs`）

- 打开：整个覆盖层从窗口底部沿 y 方向升起（100% → 0），320ms，ease-out。背景与内容一起移动，不单独淡入。
- 收起：反向下沉，240ms，ease-in。动画结束后才卸载背景与内容；收起过程中覆盖层不再接收指针事件。
- 中断：动画进行中再次切换时，从当前采样值反向，不跳回端点。
- 降低动态效果（`reduce_motion()`）：只做 150ms 透明度过渡，不位移。
- 底栏在本任务中保持现状（M7-04 会处理底栏与正在播放的衔接）。
- 动画期间不能有位置信号驱动的整树重渲染。歌词光标逻辑不变。

## 验收（每项跑一次）

1. `nsh check`、`nsh test` 通过。新增或更新 Headless 测试：打开后进入完成状态、收起中途再打开会从当前值反向、收起完成后内容卸载、reduce motion 下不位移。
2. `cargo test --manifest-path native/Cargo.toml -p haiku-app`（或 app 所在 crate 名）通过。
3. 真实窗口：`nsh run` 播放中打开正在播放，截两张图（动画中途一张、完成一张），存档 `nsh run` 日志。截图时机由程序自身的日志或定时决定，不用 GUI 自动化点击：可以用夹具在启动后自动打开。
4. 收尾：STATUS 的 F08 行更新，AUDIT 的 G22 行改为已处理并写明原因结论；追加 M7-02 小节（≤ 40 行）；日志放 `.artifacts/m7-02/`。

## 不做

不改布局、控制栏、歌词渲染（M7-03/M7-04）；不改 NSH；不做封面共享元素动画。

## 汇报

原因结论（NSH / CoreBridge / 其他）及证据；采用的实现；改动的文件；验证结果；待人工确认的观感项。
