# 任务 M0-05：macOS 原生验证 S5（原生部分）、S4、S7、S3

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。完成后停下汇报。

先读：`specs/SPEC.md` D18、`specs/MACOS.md` §1–§6、`specs/ARCHITECTURE.md` §6，以及 `.agents/skills/nsh/references/tooling.md` 中“Platform-specific code”一节和 `https://nsh.dev/docs/native-components.md`、`https://nsh.dev/docs/macos.md`（以项目的 nsh-core 0.4.0 源码为准）。

所有 AppKit 调用都必须在主线程执行；原生代码放在 `haiku-macos`，应用侧的 spike 接线继续放在 `native/app/src/spike/` 与 `src/spike/`。参考版的 `.m` 文件（`~/Project/haiku-gpuix/native/{web_auth,glass_player}.m`）只读，可以复制需要的部分；优先用 objc2 写 Rust，确实需要 ObjC 时用 `cc` 在 `haiku-macos/build.rs` 中编译，不要生成独立的 dylib。

**通用前置**：先查明 haiku-macos 如何拿到 NSH 创建的 NSApplication 与主 NSWindow，以及如何在主线程执行代码（例如 `NSApp`、`mainWindow`/`windows`、GCD 主队列）。把结论写进汇报，后面四项都依赖它。

## S5（原生部分）：关闭时隐藏、重新激活时恢复

- 接管主窗口的关闭：点击关闭按钮或按 Cmd+W 时只隐藏（`orderOut:`），不退出；Cmd+Q 正常退出。
- 处理 Dock 点击（`applicationShouldHandleReopen:hasVisibleWindows:`）恢复窗口。
- **必须保留 NSH 原有的 delegate**：用转发包装（子类化或消息转发），不能直接替换导致 NSH 的回调丢失。如果 NSH 的 delegate 无法安全包装，停下汇报。
- 验收：沿用 M0-04 的播放 spike。播放 → 关闭窗口 → 10 秒后从日志确认 `position()` 仍在增长 → 用 `open -a` 重新激活 → 窗口恢复（截图）→ Cmd+Q 退出后进程结束。无法用 CLI 触发关闭的，在应用菜单中加入 Close Window（Cmd+W），通过菜单测试；仍然不行就列为“待人工确认”。

## S4：媒体中心与菜单栏

- **媒体中心**：用 haiku-macos 已迁移的 `system.rs` 能力，把播放 spike 的标题、时长、位置、播放状态写入 `MPNowPlayingInfoCenter`；注册 `MPRemoteCommandCenter` 的 play/pause/toggle/next，事件经 channel 回到引擎线程，调用播放 spike。验收：用 `media.snapshot` 等价的诊断函数（参考版 `system.rs` 中已有，或用 MediaRemote 读取）读到正确的标题和播放状态；媒体键或控制中心的暂停按钮作为“待人工确认”。
- **菜单栏**：用已迁移的 `menubar.rs` 安装 NSStatusItem，显示当前曲名，带一个暂停按钮；点击事件回到引擎线程。验收：截图中能看到菜单栏组件；按钮点击列为“待人工确认”。
- 只验证“能安装、能回调”。不做菜单栏歌词、桌面歌词，不调样式。

## S7：原生视图嵌入（玻璃底栏）

- 用 `#[native_components(macos)]` 注册一个 `GlassSurface`（NSVisualEffectView），在窗口底部放一条高度约 64pt 的玻璃条，上面叠放 NSH 绘制的文字和一个 NSH 按钮。
- 验收：截图中玻璃效果正确，文字位于玻璃之上；Headless 测试中原生组件为空视图，不影响布局；点击叠放的 NSH 按钮能触发（Headless 测试按钮即可）。如果原生组件无法与 NSH 内容叠放，记录现象并停下。

## S3：网页登录读取 Cookie

- 在本地起一个 HTTP fixture（测试内用 Rust 启动，不依赖外部网络）：`/login` 返回一个页面并设置两个 Cookie，其中一个是 HttpOnly。
- 方案一：NSH 自带的 webview（先查 nsh-core 0.4.0 中是否有 webview 组件，以及是否能读取 Cookie）。方案二：参考版 `web_auth.m` 的做法，使用独立的 `WKWebsiteDataStore.nonPersistentDataStore` 和 `WKHTTPCookieStore`。
- 先查方案一，若 NSH 的 webview 无法读取 HttpOnly Cookie，直接实现方案二，不必两者都做完整。
- 验收：打开 fixture 页面，读到两个 Cookie（包括 HttpOnly 的那个），会话与其他 webview 隔离（第二次打开时看不到上一次的 Cookie）。以单个集成测试或 spike 按钮加日志作为证据。

## 汇报

- STATUS.md：更新 S3、S4、S5、S7 四行，追加“M0-05”小节（不超过 40 行）。
- 提交一次：`M0-05: macOS native spikes (S3, S4, S5, S7)`。本次提交一并纳入审阅者已修改的 specs 文件（SPEC.md、ARCHITECTURE.md、UI.md、MACOS.md）和 `specs/tasks/` 下尚未提交的任务文件。
- 最终回复：四项的结论；获取 NSApp/主窗口、切换到主线程的方式；哪些地方必须用 ObjC；建议审阅者修改哪些 specs。
