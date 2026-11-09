# 架构

## 1. 总览

```
┌──────────────────────────── 一个进程 ────────────────────────────┐
│ TSX 屏幕（QuickJS, NSH）                                         │
│   App.tsx · src/pages/* · src/ui/*（应用组件库）· 导航 · 页面状态   │
│        │ import { Core, TrackList, … } from "haiku/native"       │
│ ───────┼──────────────────────────────────────────────────────── │
│ native/app（NSH crate, #[native_module("haiku/native")]）        │
│   导出 class/module · Rust 组件（列表/进度/歌词/底栏）              │
│   CoreBridge：核心事件 → 引擎线程 → signal / Callback.post        │
│        │ 普通 Rust 调用                                          │
│ ───────┼──────────────────────────────────────────────────────── │
│ 领域 crate（不依赖 NSH）                                          │
│   haiku-playback · haiku-plugins · haiku-library · haiku-store   │
│   haiku-audio · haiku-lyrics · haiku-services · haiku-model      │
│        │                                                         │
│   haiku-jsrt（quickjs-jit，每来源实例一个 runtime）  symphonia  Loro │
│ ───────────────────────────────────────────────────────────────  │
│ haiku-macos：MPNowPlaying · NSStatusItem · 桌面歌词 · 热键 · NSView │
└──────────────────────────────────────────────────────────────────┘
```

参考版有三个进程：GPUIX 主进程、`haiku-native` sidecar、设置 helper。本版合并为**一个进程**，层与层之间只用 Rust 函数调用和 NSH 导出。

## 2. 仓库结构

```
haiku-nsh/
  nsh.json                 # "rust": "native/app"，无 "root"，入口 App.tsx
  App.tsx                  # 挂载、主窗口、Core 注入
  src/
    pages/                 # 每个功能一个目录：discover/ search/ library/ …
    ui/                    # 应用自有样式组件库（包装 nsh/base），主题 token
    shell/                 # 侧栏、顶栏、导航栈、命令面板、窗口
    state/                 # 只放页面状态与 Rust 状态的镜像信号
  native/
    Cargo.toml             # workspace
    app/                   # NSH crate：导出面、Rust 组件、原生组件注册
    crates/
      haiku-model/         # 共享领域类型：EntityRef、Track、错误码、ID 编码
      haiku-audio/         # 解码/输出/预加载/EQ/响度/媒体缓存（来自 player.rs effects.rs loudness.rs media_cache.rs）
      haiku-lyrics/        # LRC/KRC/YRC/TTML 解析与歌词来源链（来自 lyrics.rs）
      haiku-store/         # 数据根、原子落盘、设置、Loro 歌单（playlist_core.rs）、历史、统计、日志、凭据
      haiku-jsrt/          # 插件 JS 运行时底层：基于 NSH 的 quickjs-jit + quickjs-jit-stdlib，对标 CJRT 的宿主能力
      haiku-plugins/       # 音乐 SDK：`haiku` 模块、包仓库、来源实例、SourceApi（依赖 haiku-jsrt）
      haiku-library/       # 本地扫描、标签读写（local_scan.rs）、刮削编排
      haiku-playback/      # 队列、播放编排、会话恢复、睡眠定时、电台续歌
      haiku-services/      # 目录聚合（首页/搜索/分类）、下载（downloader.rs）、账号、Gist 同步、备份
      haiku-palette/       # 封面 Material You 取色（cover-palette）
      haiku-macos/         # AppKit 集成（menubar.rs desktop_lyrics.rs *.m）
    tools/
      haiku-plugin/        # 插件 CLI：new / check / test（契约测试套件）
  sdk/
    haiku.d.ts             # 插件 SDK 类型（随宿主版本发布）
    templates/             # provider / library / scraper 模板
    GUIDE.md               # 插件编写指南
  plugins/                 # 内置插件（v2 源码，.ts）：subsonic feiniu qq-scraper kugou-scraper
  dev-plugins/             # 不默认安装的插件：qq-music
  specs/
```

依赖规则：

- 领域 crate 之间单向依赖：`model ← audio/lyrics/store ← plugins/library ← playback/services ← app`。
- 领域 crate 不依赖 `nsh-*`，`cargo test` 可以脱离引擎运行（Coding Guides 中“业务逻辑放在不认识 NSH 的普通 Rust 类型里”）。
- `haiku-macos` 只被 `app` 依赖，并整体放在 `#[cfg(target_os = "macos")]` 下；其他平台使用同名 no-op 实现。

## 3. Rust / TSX 边界

原则（mixed.md）：每份状态只有一个所有者。**业务状态全部归 Rust**，TSX 持有镜像信号并通过方法请求修改；**屏幕状态归 TSX**。

| Rust 持有 | TSX 持有 |
|---|---|
| 队列、当前曲、播放状态、进度、音量、模式、睡眠定时 | 当前页面、导航栈、侧栏折叠 |
| 来源实例、登录状态、插件包 | 打开的弹窗、表单草稿、筛选与排序条件 |
| 曲库、歌单、喜欢、历史、统计、下载、缓存 | 选中的 tab、多选集合（提交时传给 Rust） |
| 设置（读写、校验、落盘） | 设置页的搜索框 |
| 歌词解析与当前行计算 | 布局、文案、样式 |

### 3.1 导出面（`haiku/native`）

| 导出 | 类型 | 说明 |
|---|---|---|
| `Core` | class | 应用唯一核心句柄，`Core.open()` 异步初始化数据根与各服务；以属性方式提供下列子服务 |
| `core.player` | class | `play(tracks, index)`、`toggle()`、`seek(s)`、`next()`、队列操作；`onState(listener) → Stop`（低频状态）。进度不经过 TSX |
| `core.sources` | class | 来源列表、登录流程、`call` 风格的类型化目录方法（home/search/detail…） |
| `core.library` `core.collections` `core.history` `core.stats` `core.downloads` `core.settings` `core.account` `core.sync` `core.backup` `core.logs` | class | 各领域服务。读方法返回快照；`on*` 监听返回 `Stop` |
| `TrackList` | 组件 | 虚拟化歌曲列表，行内容由 Rust 绘制；通过 `Renderer` 让 TSX 定制少数插槽；事件：play、contextMenu、selectionChange |
| `SeekBar` `PlayerDock` | 组件 | 直接订阅 Rust 播放时钟的 signal，TSX 不接触进度 |
| `LyricsView` `KaraokeLine` | 组件 | 歌词滚动与逐字高亮，绑定 Rust 当前行与时钟 |
| `Artwork` | 组件 | 封面：经核心的图片缓存（携带 headers）落盘后显示本地文件，封面切换时淡入 |
| `NowPlayingBackdrop` | 组件 | 封面配色背景与渐变遮罩 |

规则：

- 高频数据（进度、缓冲、逐字时间轴）**只在 Rust 组件内部绑定**，不越过边界，也就不会触发 TSX 重新执行。
- TSX 侧对 Rust 状态的镜像集中在 `src/state/`，每份一个信号，只由监听器写入（避免 mixed.md 所说的“两侧写入”）。
- 导出即公共 API：每个导出写文档注释，`nsh build` 不允许出现 undocumented 警告。
- 页面切换、hot update 不会丢失播放状态，因为状态在 Rust。

### 3.2 为什么高频视图在 Rust

参考版为了绕开 React 的逐帧开销，引入了原生 CoreText 图层、标量 FFI 时钟和 mutation 过滤。NSH 的绑定模型本身就能做到“只更新读取了变化信号的绑定”，所以 Rust 组件里的 `text(move || …)` 和 `width(move || …)` 已经满足要求。macOS 原生 CoreText 逐字层降级为可选的性能后备（见 MACOS.md §5），先用 NSH 实现并测量。

## 4. 线程与事件模型

| 线程 | 运行内容 |
|---|---|
| NSH 引擎线程 | signal、组件、TSX、导出方法的同步部分 |
| tokio 运行时（`background`/`blocking`） | 插件调用、网络、扫描、下载、Loro 落盘 |
| 音频线程（cpal 回调、解码线程） | haiku-audio，与参考版一致 |
| 每个插件实例的 JS 线程 | haiku-jsrt runtime（D12） |
| macOS 主线程 | AppKit 调用（菜单栏、浮窗、媒体中心），通过主队列派发 |

核心服务是 `Send + Sync` 的普通 Rust 对象，通过 `tokio::sync::broadcast`/`watch` 发布领域事件。`native/app` 的 `CoreBridge` 在引擎线程 `spawn` 一个泵任务，把事件写入 signal（Rust 组件读取）或用 `Callback::post` 通知 TSX。规则：

- signal 不是 `Send`，禁止被核心线程捕获（编译器会检查）。
- 播放时钟：音频线程把位置写入原子量；`CoreBridge` 以显示帧率读取它并写入 `position` signal，只在播放中且窗口可见时运行，暂停或隐藏时停止。
- 所有异步结果带 generation，过期结果在写入 signal 前丢弃（参考版“过期异步结果不得覆盖当前曲目”）。

## 5. 错误模型

- `haiku-model::Error { code: ErrorCode, message, source_id?, retriable, detail? }`；`ErrorCode` 沿用 v1 业务码（AUTH_REQUIRED、VIP_REQUIRED、GEO_RESTRICTED、NOT_FOUND、RATE_LIMITED、UPSTREAM_ERROR、PARSE_ERROR、PLAYBACK_EXPIRED、TIMEOUT、CANCELLED、PLUGIN_UNAVAILABLE、CAPABILITY_UNSUPPORTED、PLUGIN_ERROR），并补充本地错误码（IO、CORRUPT_DATA、UNSUPPORTED_FORMAT…）。
- 导出方法返回 `Result`，TSX 中表现为 Promise reject，并带 `code` 字段；页面按 code 决定展示重试、登录或详情。
- 日志统一写入 `haiku-store::logs`（app/plugin 两个频道），敏感信息脱敏规则同参考版 F16。

## 6. 技术验证（M0，先于功能开发）

| # | 验证项 | 通过标准 | 不通过时的方案 |
|---|---|---|---|
| S1 | 安装 nsh CLI，`nsh init --template mixed` 跑通 check/test/run | 生成项目在 macOS 窗口运行 | — |
| S2 | **插件运行时与 NSH 共用一份 QuickJS**：haiku-jsrt 基于 `quickjs-jit`，在独立线程上创建与 NSH UI VM 隔离的 runtime | 完整应用 debug/release 构建无重复符号；TSX 持续执行 JS 时，并发 1000 次插件调用结果全部正确；release 打包成功 | 已排除：CJRT 直接链接（M0-01 证实 234 个 QuickJS 符号冲突、被静默覆盖，且 RustCrypto 版本冲突）。最后手段：插件宿主独立成子进程 |
| S3 | NSH webview 能否在独立会话中读取目标域 Cookie（含 HttpOnly） | 本地 HTTP fixture 写入的 HttpOnly Cookie 可读 | macOS 复用参考版 `web_auth.m`（WKWebView + WKHTTPCookieStore） |
| S4 | `haiku-macos` 能否在 NSH 的 NSApplication 中安装 NSStatusItem、NSPanel，并注册 MPRemoteCommandCenter | 菜单栏组件可点击；媒体键事件回到引擎线程 | 调查 NSH 的 macOS 后端扩展点，必要时在 app 内持有主线程派发器 |
| S5 | cpal 输出在 NSH 进程中稳定；Cmd+W 隐藏窗口后继续播放，Dock 点击后恢复窗口 | 关闭窗口后音频不中断，窗口可恢复 | 查 NSH 窗口生命周期 API |
| S6 | `virtual_list` 1 万首歌的滚动、行回收与跳转 | 滚动无掉帧；回收后行状态正确 | — |
| S7 | `#[native_components(macos)]` 嵌入 NSVisualEffectView 作为玻璃底栏 | 与 NSH 内容正确叠放、可点击 | 底栏改用 NSH 绘制的半透明回退 |
| S8 | 第二个 NSH 窗口（设置窗口）与主窗口共用 Core | 两窗口状态同步，关闭设置窗口不影响主窗口 | — |

验证结论写入 STATUS.md 的 M0 小节。S2 不通过会影响插件宿主的形态，必须最先完成。
