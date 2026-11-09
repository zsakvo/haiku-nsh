# macOS 系统集成（F09）

所有实现都放在 `native/crates/haiku-macos`，从参考版 `native/src/menubar*.rs`、`desktop_lyrics.rs`、`system.rs`、`shortcuts.rs`，以及 `*.m`（window_bridge、karaoke、glass_player、web_auth）迁移而来。参考版通过 RPC 调用 sidecar；本版改为进程内调用，全部 AppKit 调用都派发到主线程。事件通过 channel 回到 `CoreBridge`，再交给 `haiku-playback` 的正常控制路径处理。

前提是 ARCHITECTURE.md §6 的 S4、S5、S7 技术验证通过。

## 1. M1：媒体中心与后台播放

- `MPNowPlayingInfoCenter`：标题、艺术家、专辑、时长、位置、播放状态、封面（先把需要认证的封面 URL 下载为本地图片：单张不超过 8MB，缓存最多 32 个文件、32MB）、能否上一首/下一首、喜欢状态。可选显示当前歌词行。
- `MPRemoteCommandCenter`：play/pause/toggle/stop/next/previous/seek/favorite/repeat/shuffle，全部走与界面相同的控制器。
- 媒体键：由 MPRemoteCommandCenter 处理。
- 后台播放（D18，NSH 不提供这些钩子）：接管主 NSWindow 的关闭（`windowShouldClose:` 返回 NO 并 `orderOut:`），Cmd+W 或点击关闭按钮只隐藏窗口，音频继续播放；处理 NSApplication 的 reopen（Dock 点击）和菜单中的“显示主窗口”，恢复窗口；Cmd+Q 正常退出。实现时必须保留 NSH 原有的 delegate 行为（转发而不是替换），不修改 NSH 源码。
- 防息屏：IOPMAssertion `PreventUserIdleDisplaySleep`，仅在正在播放页可见且设置开启时持有。
- 退出时清理媒体信息并释放 assertion。

## 2. M6：菜单栏组件

- 自定义 NSView 加独立的 NSButton，宽 290pt（歌词区 152pt、5 个 24pt 按钮、6pt 间隔、两侧各 6pt 边距）。
- 内容：歌曲信息或逐字歌词（可以滚动）、上一首/播放/下一首/喜欢/队列菜单；可选高亮色。图标是 template image，跟随菜单栏外观，不跟随应用主题。
- 鼠标命中使用真实的屏幕指针坐标（参考版修复过的问题：系统复制的视图会把点击位置固定在中心）。
- 菜单：当前队列（菜单项带期望的曲目身份，拒绝过期的选择）、可写入的歌单、显示主窗口、歌词显示与锁定、退出。
- 只安装一个状态栏组件（参考版要求音频核心不能再安装第二个；本版是单进程，天然满足）。

## 3. M6：桌面歌词

- 非激活的 NSPanel：显示、隐藏、锁定（`ignoresMouseEvents`，悬停时显示 32pt 的解锁子面板）、拖动；位置记忆（保存中心点、Y 和宽度，默认宽度 600pt，启动时限制在已连接的显示器范围内）。
- 样式：字体（NSFontPanel，字号 14–60pt，字重 light 到 bold）、颜色、描边、阴影、显示译文；背景可选透明（默认）、纯色或系统模糊。
- 布局：经典/左对齐/舞台模式最多显示两行（当前行加译文，或当前行加下一行），焦点模式只显示当前行；过长的行先缩小到可读的最小字号，再平移显示。
- 逐字：Core Animation 时间轴，暂停、seek、恢复都与播放器一致。
- 工具栏：8 个 SF Symbol 按钮（上一首、播放、下一首、喜欢、锁定、显示主窗口、歌词设置、隐藏）。
- 没有歌词时显示歌曲名和艺术家。
- 可见性与锁定状态保存在设置中；位置保存在 `haiku_state/desktop-lyrics-position.json`。

## 4. M6：全局快捷键

- 原子替换整组绑定；普通按键必须带 Cmd、Ctrl 或 Option（不允许裸字母或只带 Shift）；F1–F20 可以不带修饰键；先检查重复和冲突再修改；与系统冲突时报错，并恢复之前的注册。
- 设置页提供录制、冲突提示、单项恢复默认、全部恢复默认。

## 5. M6：原生视图（`#[native_components(macos)]`）

| 名称 | 实现 | 回退 |
|---|---|---|
| `GlassSurface` | NSVisualEffectView 作为 PlayerDock 的背景；无染色的 clear glass，细圆角轮廓，上缘高光 | NSH 绘制的半透明表面（M1 起就有） |
| `KaraokeLayer`（可选） | CoreText + Core Animation 逐字填充 | Rust 组件 `KaraokeLine`。只有在实测 CPU 占用不可接受时才启用此项 |

## 6. 网页登录（D14）

- 优先使用 NSH webview 的独立会话；如果 S3 验证不通过，在 macOS 上使用参考版的 `web_auth.m`（独立的 WKWebsiteDataStore，通过 WKHTTPCookieStore 读取包括 HttpOnly 在内的 Cookie，自定义 UA，可以取消）。

## 7. 其他平台

`haiku-macos` 对外暴露的 trait（`MediaCenter`、`StatusBar`、`DesktopLyrics`、`GlobalShortcuts`、`PowerAssertion`）在其他平台上有 no-op 实现，设置界面中隐藏不支持的项；Windows/Linux 的实现留待后续版本，不对外宣称已支持。

## 8. 测试

- 用诊断快照读取 AppKit 的实际状态（参考版的 `media.snapshot`、`desktopLyrics.snapshot` 仅在 debug 构建中提供）。
- 桌面歌词的布局策略写成纯函数并做单元测试。
- 真实窗口验证：菜单栏点击命中、锁定后悬停解锁、暂停/seek 时的同步、窗口隐藏后从 Dock 恢复。
