# 实施与验收记录

状态只有 `未实现 / 部分实现 / 已验证`。没有证据不能标为“已验证”。每次更新在下方追加一个带日期的小节，说明改动、验证命令与结果、未覆盖的范围。

## 技术验证（M0）

| # | 验证项 | 结论 | 证据 |
|---|---|---|---|
| S1 | nsh CLI 与 mixed 模板 | 已验证（当前完整应用重新通过） | 2026-11-09 M0-02：移除 CJRT 后，当前应用 `nsh check/build/test` 通过，build 无 warn；开发窗口与打包版均显示 `Plugin echo(21) = 42`。证据 `.artifacts/m0-02/nsh-{check,build,test,run,package}.log`、`run-window.png`、`package-window.png`；开发 host 的独立链接器提示见下文 |
| S2 | 插件运行时与 NSH 共用一份 QuickJS | 已验证（M0 最小内核，macOS arm64） | `haiku-jsrt` 与 NSH 使用同一组 `quickjs-jit/stdlib 0.14.1`；仅一份 sys，无 CJRT/rquickjs-sys。运行时测试 11/11；16 worker 共 1000 次调用全部正确，同时有 173 次 UI JS 计算 tick，双向全局变量隔离通过；release 打包无 warn，`_JS_NewRuntime` 一个定义。证据 `.artifacts/m0-02/{jsrt-test,nsh-test,nsh-package}.log`、`cargo-tree-duplicates.log`、`nm-release.txt` 及两张窗口截图 |
| S3 | NSH webview 读取 HttpOnly Cookie | 已验证（macOS 原生回退） | M0-05：NSH 公开 WebView 接口无 Cookie 读取；独立 WKWebsiteDataStore 读取本地 fixture 的两个 Cookie（含 HttpOnly），第一会话存活时第二会话为空。`.artifacts/m0-05/web-cookies-repair2.log` |
| S4 | NSStatusItem / NSPanel / MPRemoteCommandCenter | 已验证（含用户人工确认） | M0-05：正确标题、playback_state 1→2；菜单栏截图可见曲名和控制按钮，channel 接回引擎线程。`.artifacts/m0-05/nsh-run.log`、`menubar.png`；实际按钮/媒体键回调待人工确认，NSPanel 未覆盖 |
| S5 | 后台播放与窗口恢复 | 已验证（含用户人工确认） | M0-05：原 delegate 实例加无 ivar 子类，只覆盖关闭；reopen/quit 继承 NSH。可见窗口 position 0.603→34.995。`.artifacts/m0-05/nsh-run.log`；关闭后 10 秒、open -a 恢复、Cmd+Q 退出未覆盖 |
| S6 | virtual_list 1 万行 | 已验证（跳转、选中回收与中部截图） | M0-04：Headless 1/1，跳转第 9000 行、选中第 5 行往返底部无串行；`.artifacts/m0-04/nsh-test-trace.log`、`list-middle.png`。trace 是提交耗时，未证明实际帧率 |
| S7 | 原生玻璃视图嵌入 | 部分实现（嵌入、叠放与按钮已验证；模糊未证实） | M0-05：Headless 1/1，空原生视图保留 64pt 布局，NSH 按钮可触发；真实截图文字/按钮层级正确，但玻璃区仍均匀灰色，两种修复后不再扩展排查。`.artifacts/m0-05/glass-headless-repair1.log`、`native-window.png` |
| S8 | 第二个窗口共用 Core | 不可行，已关闭（D18：设置改为窗口内覆盖层） | M0-04：Rust 持有值、TSX listener 镜像已实现；当前 nsh/desktop 无窗口创建/标识 API，三项双窗口验收均未覆盖。`.artifacts/m0-04/platform-api.log` |

## 功能要求

| 要求 | 状态 | 实施文件 | 验证 | 缺口 |
|---|---|---|---|---|
| F01 桌面外壳与导航 | 部分实现 | `src/ui/`、`src/shell/`、`src/pages/PlaceholderPage.tsx`、`native/app/src/platform/` | M1-01/M1-01b 已审阅；M1-01c 与侧栏布局反馈修订 check/build/test 通过、原有 Headless 4/4，主窗口与设置浅色截图已查看 | 页面内容、来源选择、PlayerDock 待后续任务；仅有已知警告例外 |
| F02 插件包与来源生命周期 | M2 范围已实现，M2-05 待审阅 | `haiku-plugins/src/{host,contract}.rs`、`native/app/src/{plugins,plugin_tools}.rs`、`src/shell/PluginSettings.tsx`、`sdk/GUIDE.md` | workspace 228 通过/1 忽略；QQ CLI 契约 34/34；tsc、nsh check/build、nsh test 17/17；`.artifacts/m2-05/` | 目录监听为 500ms 内容哈希轮询；record/live 仍为基础实现；系统文件选择器与真实热重载体验待人工确认 |
| F03 来源认证与用户内容 | M2 范围已实现，M2-05 待审阅 | `haiku-plugins/src/source.rs`、`dev-plugins/qq-music/`、`plugins/feiniu/plugin.ts`、`native/app/src/plugins.rs` | QQ/微信 QR、SMS、Cookie、refresh/logout 离线契约；QR PNG、过期刷新/取消、SMS cooldown、cookie/web 表单与事件 Stop 回归；`.artifacts/m2-05/` | web 按已接受方案手动粘贴 Cookie + 完成 URL；真实 QQ 登录/风控/VIP/地区、Subsonic/飞牛远程连接仍待人工；QQ 短信额外安全验证提示改用扫码 |
| F04 推荐、发现与分类 | 部分实现（推荐/分类/电台，M4-03 已验收） | `native/app/src/discovery.rs`、`native/crates/haiku-playback/src/controller.rs`、`src/pages/DiscoveryPages.tsx` | M4-03 审阅复跑29/29；M4-04 Discovery回归通过，分类显示名/电台secondary/推荐货架间距已改；分类歌单与电台浅色截图齐全；恢复NowPlaying常驻后Search含Escape定向及完整均通过 | 真实来源首页/电台待人工确认 |
| F05 搜索 | 部分实现（搜索页与命令面板；M4-02 fixture 验收完成，待审阅） | `native/app/src/{sources,plugins,command_keyboard}.rs`、`src/pages/SearchPage.tsx`、`src/shell/CommandPalette.tsx` | M4-02：workspace 244 通过/1 忽略；最终 nsh check/build 通过，nsh test 25/25；五张浅色实机截图齐全 | 五种实体、来源筛选、分页、历史、防旧结果覆盖、280ms 面板预览与完整第一页起播已实现；真实来源搜索与实机键盘体验待人工确认 |
| F06 队列与播放控制 | 部分实现（本地与来源播放，M4-01 待审阅） | `haiku-playback::{controller,backend,network}`、`native/app/src/{core,sources,player_dock,seek_bar}.rs` | M4-01：workspace 243 通过/1 忽略，假后端/时钟覆盖候选、过期、重试、提前 EOF 与缓存；nsh test 22/22；真实 macOS loopback 日志进度 0→23 秒、来源封面媒体快照和浅色 Dock 截图通过 | 真实上游、实际听感与多曲无缝未覆盖；入场动画沿用静态方案 |
| F07 音效与响度 | 已验证（M1 范围，待审阅） | `haiku-store::settings`、`haiku-playback::{backend,loudness_cache}`、`src/shell/PlaybackSettings.tsx` | M1-05：DSP 映射/切歌与预加载保持、测量优劣规则通过；Headless EQ 设置与后端一致；浅色播放设置截图通过 | 真实听感待人工确认；软削波与 8ms 淡入淡出沿用固定保护机制 |
| F08 歌词与正在播放 | 部分实现（来源链、提供方、导出、查找与设置界面，M4-04 已验收） | `haiku-lyrics::{formats,providers,service,Cursor}`、`native/app/src/{lyrics,presentation,lyrics_view,now_playing}.rs`、`src/shell/Lyrics{Search,Settings}.tsx` | 审阅者复跑：workspace 252 通过/1 忽略，nsh check/build 通过，完整 nsh test 33/33；五张浅色截图 | initialNowPlaying 初始打开时组件事件不到达（夹具专有）；Paxsenix 评分/令牌重取、Kugou 署名裁剪遗留；真实上游待人工确认 |
| F09 桌面集成 | 部分实现（媒体中心接线） | `native/app/src/platform/media.rs`、`haiku-macos::{system,lifecycle}` | M1-05：真实标题/封面及 playback_state 1→2 快照；媒体回调通过 channel 进入 Controller；防息屏与退出清理已接线 | 媒体键/控制中心、关闭隐藏续播、Dock 恢复、Cmd+Q 与防息屏待人工确认；M6 功能未覆盖 |
| F10 本地曲库与刮削 | 曲库、刮削任务与审阅界面已实现（M4-05 已验收） | `haiku-library/src/service.rs`、`native/app/src/scrape{,_service}.rs`、`haiku-plugins/src/scraper.rs`、`src/pages/{LibraryPage,ScrapeReview}.tsx` | 4并发/8秒、三档verdict、取消、稀疏填补/覆盖、撤销冲突保护、持久化审阅与参考格式转换；新增Headless通过；workspace260通过/1忽略、完整nsh test43/43；收尾补充Rust定向各4/4，见M4-05 | 真实刮削上游、写回文件标签实机流程待人工；参考版高级查询清洗、专辑艺术家/碟号与刮削封面本地缓存仍未补齐 |
| F11 喜欢、歌单与远端收藏 | 本地收藏、远端镜像刷新已实现（M4-05 已验收） | `haiku-store::collections`、`native/app/src/{collections,plugins,favorite_refresh}.rs`、`src/pages/{CollectionPage,SourceDetailPage}.tsx` | 离线快照优先、完整分页原子换版、单飞/取消/迟到丢弃、取消收藏确认；Rust4/4、Headless刷新/离线/取消确认通过；浅色截图齐全 | 真实来源刷新与取消收藏、媒体命令实机交互仍待人工确认 |
| F12 历史与听歌统计 | 部分实现（本地） | `haiku-store::{history,stats}`、`native/app/src/activity.rs`、`src/pages/{HistoryPage,StatsPage}.tsx` | M3-02：workspace 238 通过/1 忽略；参考版 JSON、有效播放边界/聚合测试通过；Headless 历史与范围切换、浅色图表截图通过 | 远程上传留 M5；在线歌单历史与搜索页接线留 M4；真实长时累计待人工确认 |
| F13 下载与媒体缓存 | 部分实现（M5-01 Rust 下载服务与导出，界面未做） | `native/app/src/{download_service,downloads,presentation}.rs`、`haiku-services::downloader`、`haiku-audio::media_cache`、`haiku-store::settings` | 下载 Rust 5/5；初版 workspace 266 通过/1 忽略、check/build 通过；Player 回归已修复，新增受控歌词回归通过，Player 2/2 | 审阅者复跑完整 nsh test 50/50（Search Escape 为既有偶发）；下载界面、截图、自动歌单/迁移目录未做；详见 M5-01 小节 |
| F14 Haiku 账号 | 未实现 | | | |
| F15 同步与备份 | 未实现 | | | |
| F16 设置、通知与日志 | 部分实现（外观/播放/存储、日志与通知） | `haiku-store::{settings,logs}`、`native/app/src/{settings,activity}.rs`、`src/shell/{Settings,Diagnostics}.tsx` | M1-05 设置测试沿用；M3-02 日志轮转/限额/脱敏/过滤、Headless 清空确认与通知清除通过；设置损坏备份接入日志和通知；日志/通知浅色截图通过 | 其他设置分组待后续；系统保存选择器与剪贴板结果待人工确认；应用许可证待确定 |

## 2026-11-09 实施文档

完成 specs 初稿：SPEC、ARCHITECTURE、PLUGIN_SDK、PLAYBACK、LIBRARY、DATA、UI、MACOS、MILESTONES。尚未开始编码。待确认问题 Q1–Q4 已全部按推荐方案确认。

## 2026-11-09 M0-01：骨架完成，QuickJS 共存验证未通过

执行范围仅为 `tasks/M0-01-skeleton-and-quickjs.md` 的 A、B。S2 已发现冲突，按任务要求保留证据并停止；没有开始 M1、迁移参考版业务代码、提交 git 或修改上游 CJRT/NSH。本次 `specs/` 内只更新本文件。

### 实施文件与布局选择

- 初始化 `.git/`，没有 commit；`.gitignore` 包含 `.nsh/`、`target/`、`native/target/`、`.DS_Store`、`.artifacts/`。
- 在临时目录 `/var/folders/yj/8637xr4n1wx5ndd98xzb5mrm0000gn/T/haiku-m0-01-47w2yrhd/` 执行 `nsh init haiku --template mixed`，再按架构落入当前目录。`nsh.json` 为 `name: Haiku`、`id: dev.haiku.nsh`、`rust: native/app`，无 `root`；入口 `App.tsx`，specifier 为 `haiku/native`。
- `native/Cargo.toml` 是统一 workspace，成员为 `app` 和 `crates/*`；移除了模板 app manifest 的空 `[workspace]`。`native/app/Cargo.toml` 保留模板 crates.io `nsh-core = "0.4.0"`、`nsh-base = "0.4.0"`。S1 的 describe、headless runner 和 macOS host 都成功构建，证明 app 无须独立 workspace。NSH 另行生成 `.nsh/rust` host workspace，这是 CLI 行为，不是领域 workspace 的替代。
- `native/crates/haiku-{model,audio,lyrics,store,plugins,library,playback,services,palette,macos}` 均有 manifest；除 `haiku-plugins` 的本次探针外，`src/lib.rs` 只有模块文档。`src/{pages,ui,shell,state}`、`sdk/`、`plugins/`、`dev-plugins/` 均建立 `.gitkeep`。
- `native/app/src/lib.rs` 导出 `Core.version()` 和 `Spike.cjrtEcho(n)`；后者通过经解析源码确认的 `nsh_core::background` 执行 CJRT future，返回值才回到引擎线程。`native/crates/haiku-plugins/src/lib.rs` 创建 `PluginHost`、安装和加载内联 JS、调用 echo、检查 `{ok,n}`、销毁 host。每次调用使用独立 `haiku-m0-echo-*` 临时目录，不使用真实应用数据根。
- `.cargo/config.toml` 仅开启 `[net] git-fetch-with-cli = true`。`native/Cargo.lock` 最终锁定参考版的 CJRT `400cb02481e2379453dfcc823c2c2ada505f13f7` 与其 RC 加密依赖；依赖声明仍使用任务要求的 git URL 和 `feature/base` 分支。
- `App.tsx` 保持 M0 验证屏幕：Haiku、Rust 版本、回声按钮及等待/执行/结果/错误状态。`src/ui/Button.tsx` 包装 `nsh/base` 的语义按钮，使用默认 semantic theme token、标准间距/字号及 hover/pressed/focus/disabled 状态。本阶段不建立 M1 完整产品设计系统。
- `App.test.tsx` 包含版本显示与按钮测试；`Spike.test.tsx` 配合 `tests/fixtures/QuickJs.tsx`，设计为 16 个并发 worker 合计调用 1000 次，NSH VM 同时持续分配数组、排序、计算，并要求原生调用尚未结束时确实发生 JS tick。S2 的这些测试**尚未执行到测试主体**。
- `README.md`、`tsconfig.json` 记录模式、命令、布局和当前阻塞。诊断探针源文件保存在被忽略的 `.artifacts/m0-01/link-probe/`，不属于产品 crate。

### 命令与结果

下表日志均相对于 `.artifacts/m0-01/`。S1 命令是在接入 CJRT **之前**执行；S2 命令是在当前集成状态执行，不能混为一次全绿验收。

| 阶段 | 命令 | 结果与证据 |
|---|---|---|
| 环境 | `nsh --version`；`cargo -V` | `nsh 0.4.0 (e626a8d)`；`cargo 1.101.0`，huacnlee/cargo `0.102.0-9` |
| A | `git init` | 成功；仓库 HEAD 尚无 commit |
| A | 在上述临时目录执行 `nsh init haiku --template mixed` | 成功；以生成的 manifest、注册及入口为起点 |
| A | `cargo metadata --manifest-path native/Cargo.toml --format-version 1` | 成功；`metadata-s1.json`。NSH API 从其解析出的 crate 源码和生成的 `.nsh/types` 查证后使用 |
| A 模板基线 | `nsh check`；`nsh test` | 均退出 0；模板原始 press 测试 1/1 通过（`template-test.log`）。之后再收敛为 Haiku 最小入口 |
| A 最小入口 | `nsh check` | 退出 0（`s1-check.log`） |
| A 最小入口 | `nsh build` | 退出 0，无 warn（`s1-build.log`） |
| A 最小入口 | `nsh test` | 退出 0；版本显示测试 1/1 通过（`s1-test.log`） |
| A 最小入口 | `nsh run` | host 构建成功、macOS 连接 build 1，窗口显示 Haiku / Core 0.1.0（`s1-run.log`）。截图后用 SIGINT 正常停止 |
| A 截图 | `screencapture -x -l 21237 .artifacts/m0-01/s1-window.png` | 成功；已实际查看截图。21237 是本次运行通过 CoreGraphics 查得的窗口 ID，重跑须重新查询 |
| B 初始依赖解析 | `cargo metadata --manifest-path native/Cargo.toml --format-version 1`；`nsh check` | 首次解析到 CJRT `eda746863dd2918103e4ecc245a6bffe86ceea74`；check 退出 1，`llrt_crypto` 对新版 RustCrypto 的 API 不兼容（`s2-check-fresh-resolution-failed.log`）。原始解析锁另存 `native-lock-fresh-resolution.toml` |
| B 参考锁验证 | `cp /Users/zsakvo/Project/haiku-gpuix/native/Cargo.lock native/Cargo.lock`，再运行上述 native `cargo metadata` | Cargo 裁剪并补充本项目依赖，保留 CJRT `400cb024` 和兼容 RC；`metadata-s2.json`、`metadata-s2-reference-lock.log`。参考版仅只读 |
| B host 锁验证 | `cp native/Cargo.lock .nsh/rust/Cargo.lock`；`nsh check` | 退出 1（`s2-check.log`）；host workspace 加入 NSH stdlib 后仍被迫升级加密依赖。证明复制参考锁不能解决完整应用依赖组合；没有把生成锁文件当成修复方案 |
| B 格式 | `cargo fmt --manifest-path native/Cargo.toml --all`；`cargo fmt --manifest-path native/Cargo.toml --all --check` | 均退出 0（`fmt.log`） |
| B Rust workspace | `cargo check --manifest-path native/Cargo.toml --workspace --all-targets` | 退出 0、无 warning（`s2-cargo-check.log`）；只证明领域和导出 Rust 代码能单独检查 |
| B Rust lint | `cargo clippy --manifest-path native/Cargo.toml --workspace --all-targets -- -D warnings` | 退出 0（`clippy.log`）；不能替代完整 host 构建 |
| B 最终完整检查 | `nsh check -v` | 退出 1（`s2-check-final.log`），host describe 编译其依赖时的 `llrt_crypto` API 错误，未完成 TSX 类型检查 |
| B debug 构建 | `nsh build -v` | 退出 1（`s2-build.log`），同上；未产出有效的 S2 应用构建 |
| B 真实运行 | `nsh run -v` | 退出 1（`s2-run.log`），同上；未打开 S2 窗口 |
| B 测试 | `nsh test -v` | 退出 1（`s2-test.log`），同上；1000 次并发与按钮测试没有运行，不能标通过 |
| B release 打包 | `nsh package --target macos -v` | 退出 1（`s2-package.log`），同上；在 describe 阶段即失败，**没有走到完整 release 链接，也没有产出可验收 .app** |
| B host 元数据 | `cargo metadata --manifest-path .nsh/rust/Cargo.toml --format-version 1` | 退出 0（`metadata-host.json`）；确认实际组合包含两份 sys crate，二者均开启 `bindgen`，NSH stdlib 开启 `crypto` |
| B 符号 | `nm -gU <两份 sys crate 构建出的 libquickjs.a>`，筛选 `_JS_` 的全局定义 | CJRT 236 个，NSH 250 个，共同定义 234 个。完整路径和 nm 输出在 `nm-rquickjs-sys-*.txt`、`nm-quickjs-jit-sys-*.txt`；交集在 `quickjs-common-symbols.txt` |
| B 最小 debug 链接诊断 | `cargo build --manifest-path .artifacts/m0-01/link-probe/Cargo.toml` | 退出 0，**有重复符号 warning，因此不通过无 warn 要求**（`link-probe-debug.log`）。探针 feature 最终与实际 host 对齐为两侧均 `bindgen` |
| B 最小 release 链接诊断 | `cargo build --release --manifest-path .artifacts/m0-01/link-probe/Cargo.toml` | 退出 0，同样有重复符号 warning（`link-probe-release.log`）。这是 sys 诊断程序，不是完整 Haiku release |
| B 符号解析诊断 | 分别运行 `.artifacts/m0-01/link-probe/target/{debug,release}/haiku-m0-quickjs-link-probe` | 两者退出 0；两侧 `JS_NewRuntime` 地址相同（`same=true`），两侧 `JS_GetVersion()` 均返回 `0.16.2`（`link-probe-{debug,release}-run.log`）。探针不创建 runtime，不是并发正确性测试 |
| B 产物 nm | `nm -gU .artifacts/m0-01/link-probe/target/{debug,release}/haiku-m0-quickjs-link-probe` | 每个诊断产物只剩一个 `_JS_NewRuntime` 定义（`nm-link-probe-{debug,release}.txt`），结合地址与版本证明发生符号覆盖，不能据“没有多个同名定义”认定隔离成功 |

另有机器可读退出码记录 `s2-command-results.json`、重复符号汇总 `symbol-summary.json` 及 `link-probe-{debug,release}-duplicates.txt`。普通 Rust workspace 检查曾报模板保留的 `nsh-base` 未使用；app 显式保留该依赖导入后，最终 check/Clippy 无 warning。

### 两个独立阻塞及其原因

1. **RustCrypto API 不兼容先阻断完整应用编译。** CJRT 的 `llrt_crypto` 使用旧 RC 的 `p256/p384/p521::EncodedPoint`、`rsa::pkcs1::RsaPublicKey<'_>`、旧 rand trait 方法；NSH `quickjs-jit-stdlib 0.14.1` 要求 `p256 0.14`、`elliptic-curve 0.14`、`rand 0.10`、`rsa 0.10.0-rc.19` 等较新版本，host workspace 的统一解析不再满足旧代码 API。最终日志有 E0433、E0107、E0282、E0599。native workspace 用参考 RC 锁能通过，而 host 加入 NSH 后失败，这解释了为何“核心 cargo check 成功”不能证明集成成功。直接精确锁回旧 RC 会与 NSH 所要求的版本冲突，不能作为完整修复。
2. **C ABI 符号确实冲突，且诊断产物发生覆盖。** NSH 使用 crates.io `quickjs-jit-sys 0.14.1`，其 vendored header 标注 QuickJS-ng **0.16.2**，另有该 crate 的补丁；CJRT 使用 `rquickjs-sys 0.11.0`，vendored header 标注 QuickJS-ng **0.11.0**。两侧导出相同的 `_JS_NewRuntime`、`_JS_NewRuntime2`、`_JS_FreeRuntime`、`_JS_NewContext`、`_JS_Eval` 等，还包括 `_js_malloc`、`_js_free` 等非 `JS_` 前缀符号。最小探针同时引用旧版独有 `JS_NewTwoByteString` 与新版独有 `JS_NewStringUTF16`，保证两份 C 对象进入链接；链接器在 debug 报出 114 个不同重复符号、release 报出 52 个（分别含 100/43 个 `JS_`，优化与错误报告范围导致数量不同）。两种 profile 最终都把两侧 `JS_NewRuntime` 解析到同一地址、版本查询都落在 0.16.2。这不是安全地“共享一个兼容引擎”，而是未经适配就把旧 wrapper 接到新版 C 实现；不能运行插件后再碰运气判断是否安全。

以上最小链接诊断补充了被加密依赖阻断的符号证据，不能冒充完整 app 的 debug/release 验证。本次没有添加忽略重复符号的 linker flag，也没有修改 NSH/CJRT 源码。

### ARCHITECTURE §6 S2 的方案评估

| 方案 | 判断与前置工作 |
|---|---|
| A：CJRT 的 QuickJS 符号前缀 | **推荐下一项独立任务优先验证。** 保留进程内、两个独立引擎及现有 CJRT wrapper 边界。须在上游为全部外部 C 符号建立一致前缀映射，并同步 C 源、头文件、inline 调用及 Rust FFI/bindgen；不能只处理 `JS_NewRuntime` 或只改 `JS_`，否则仍可能混用分配器。还必须独立修复/升级 CJRT 的 RustCrypto 适配，使它能与 NSH 的依赖版本共存。前缀本身无法解决第一个阻塞 |
| B：共用一份 QuickJS 构建 | 可保留进程内并减少一份引擎，但须把 CJRT、LLRT 模块和 serde wrapper 对齐到 NSH 的 quickjs-jit/rquickjs API、QuickJS-ng ABI 与依赖组合；还需证明 runtime/context 所有权、worker 线程、取消与限制语义。不能通过链接器覆盖冒充此方案。若升级 CJRT 的总代价接近符号隔离加 crypto 适配，值得重新比较，但本次没有证据证明可直接替换 |
| C：插件宿主独立子进程 | 最强隔离；独立构建 workspace/依赖图能同时避开 C 符号和 RustCrypto 合并问题。代价是回到 IPC、进程管理、取消/错误传递、打包与崩溃恢复，偏离本项目 D3 的进程内决定；保留为 A/B 成本或可靠性不可接受时的最后手段 |

推荐顺序：后续独立任务先实施上游 A + RustCrypto 适配验证，再重跑本任务的完整 debug/release、符号、1000 次并发及真实按钮验收；如果维护两份引擎的成本过高，再比较 B；最后才采用 C。这里只记录建议，没有实施任何一种替代方案，也没有改写架构决策。

### 未覆盖与停止点

- S2 的完整应用 debug/release 链接、1000 次并发结果、真实窗口按钮结果、S2 截图全部未通过验收；原因与上述日志一致。当前目录整体 `nsh check/build/test/run/package` 不能报绿。
- S1 只有基线窗口截图；没有把 S1 旧 host 重新运行后称为 S2 成功。S2 产物不存在，无法对它执行 nm；已明确区分静态库、独立探针与完整应用证据。
- 主题、键盘、窗口尺寸等 S2 交互验收也未运行；完整设计系统、S3–S8、业务迁移、M1 和任何真实账号/数据操作均未开展。
- 到此停止，交由审阅者评估阻塞及下一项任务；不提交 git。

## 2026-11-09 M0-02：haiku-jsrt 最小内核与 S2 通过

按修订后的 D11 执行 A/B/C；M0-01 的冲突记录保留为历史，本次已移除 CJRT 依赖及旧 echo。

- 新增 `native/crates/haiku-jsrt/{Cargo.toml,src/lib.rs,src/tests.rs}`；在 `native/Cargo.toml` 集中精确声明 `quickjs-jit`、`quickjs-jit-stdlib = 0.14.1`，更新 `native/Cargo.lock`，删除只供 CJRT 使用的 `.cargo/config.toml`。
- 更新 `haiku-plugins` 的 manifest/实现、`native/app/src/lib.rs`、`App.tsx`、`App.test.tsx`、`Spike.test.tsx`、`tests/fixtures/QuickJs.tsx`；导出改为 `Spike.pluginEcho`，内联插件校验 MD5、Buffer 和 UI 全局变量不可见。
- 每实例一个专用 OS 线程、独立 AsyncRuntime/AsyncContext、线程内 current-thread Tokio；channel 串行接收命令，JS 值留在线程内。每批最多轮询 32 个 job，以 stdlib future 的 waker 唤醒，空闲不忙轮询。
- API 为 `spawn/load_module/call/shutdown`，支持 JS ES module、top-level await、同步/异步导出、单个 JSON 参数及 JSON 返回值、`JsError { name, message, stack }`。模块加载期限 5 秒；调用期限包含排队时间。
- interrupt handler 中止死循环；超时返回 `TimeoutError` 并销毁 VM，后续请求明确返回 `RebuildRequired`。shutdown/Drop 中止工作并 join；测试覆盖 pending Promise、pending 原生 future 及重复销毁。

以下证据均在 `.artifacts/m0-02/`，均为审阅者要求收尾前已取得的结果：

| 验证命令 | 结果与证据 |
|---|---|
| `cargo tree --manifest-path .nsh/rust/Cargo.toml -d`；`cargo metadata` | 仅一份 `quickjs-jit-sys 0.14.1`，无 `rquickjs-sys`/CJRT；`cargo-tree-duplicates.log`、`metadata-final.json` |
| `cargo test --manifest-path native/Cargo.toml -p haiku-jsrt` | 11/11，通过 sync/async、stdlib、异常、超时、死循环、实例隔离、销毁及 Tokio 唤醒测试；`jsrt-test.log` |
| `cargo fmt --all --check`；workspace `cargo clippy --all-targets -- -D warnings`（均指定 native manifest） | 均退出 0；`fmt-check.log`、`clippy.log.stderr` |
| `nsh check -v` → `nsh build -v` → `nsh test -v` | 均退出 0，最终三份日志无 warn；TSX 测试 3/3，1000 次回声正确，ticks=173、overlap=173、checksum=464564928，双向隔离通过；`nsh-{check,build,test}.log` |
| `nsh run --target macos -v` | 真实开发窗口按钮触发后显示 42；`run-window.png`、`run-click.log`；截图后 SIGINT 停止，退出 130 |
| `nsh package --target macos -v`；启动 `.nsh/out/Haiku.app` | 最终打包退出 0、无 warn；打包版按钮再次显示 42；`nsh-package.log`、`package-window.png`、`package-click.log` |
| `nm -gU .nsh/out/Haiku.app/Contents/MacOS/Haiku` | `_JS_NewRuntime` 恰好一个全局定义；`nm-release.txt`。开发/release 日志均无 duplicate symbol 警告 |

RustCrypto 相关重复版本仍有：block-buffer 0.10.4/0.12.1、const-oid 0.9.6/0.10.2、crypto-common 0.1.7/0.2.2、der 0.7.10/0.8.3、digest 0.10.7/0.11.3、ed25519 2.2.3/3.0.0、ed25519-dalek 2.2.0/3.0.0、pkcs8 0.10.2/0.11.0、rand 0.8.6/0.9.4/0.10.3、rand_core 0.6.4/0.9.5/0.10.1、sha1 0.10.7/0.11.0、sha2 0.10.9/0.11.1、signature 2.2.0/3.0.0、spki 0.7.3/0.8.1；本次没有出现旧 CJRT 的 API 冲突。

工具与限制：最终验收局部使用当前 toolchain 自带 Cargo 1.98.1，避免 huacnlee/cargo 忽略 target-dir 的警告；release 局部设置 `DYLD_LIBRARY_PATH=<toolchain>/lib` 后重新链接，解决 rust-objcopy 找不到 LLVM 的警告，原日志保留为 `nsh-package-llvm-path-warning.log`。开发 host 仍有 `__eh_frame` 超过 16 MB 的 compact-unwind 性能提示（`nsh-run.log`），未消除；不能声称所有运行阶段完全无警告。

窗口截图已实际查看。终端无辅助功能权限，已有两次按钮验证通过 LLDB 向各自 AppKit 窗口队列投递 mouse moved/down/up，触发真实控件；未直接调用 JS 或写入结果。一次早期 debugger attach 导致 CLI 丢失 child 的诊断保留在 `nsh-run-lldb-parent-failed.log`，随后开发窗口验证完成。

未覆盖：其他平台/架构、长期稳定性及资源上限；R2/R3/R5–R12、完整 R1/R4 和音乐 SDK 均留后续任务。同步原生 stdlib 工作不能被 JS interrupt 抢占，期限在返回后检查。本次不提交 git，不修改上游源码、Rex 会话名或其他 specs 文件；按审阅者要求停止新增验证，仅用已有证据收尾。

## 2026-11-09 M0-03：Rust 核心迁移完成

- 基线提交 `77bb293`：`M0: project skeleton, haiku-jsrt minimal runtime (S1, S2)`；提交前确认 main 和忽略规则，证据 `.artifacts/m0-03/git-baseline.log`。
- 将 player/effects/loudness/media_cache/playback 迁入 haiku-audio；lyrics → haiku-lyrics；playlist_core/playlist_sync → haiku-store（collections 接口）；local_scan/local_library → haiku-library；downloader/downloads → haiku-services；cover-palette → haiku-palette；4 个 macOS 模块、2 个 API 和菜单栏图片 → haiku-macos。
- 按后续授权调整映射：AudioTags/write_tag 下沉 haiku-library::tags；共享 FLAC/JPEG 夹具下沉 test_support（仅 test 或显式 feature），下载端到端测试仍留 services。原 library → downloader 的反向引用改为 services → library，无反向领域依赖残留。
- 排除原 stdout bridge/RPC：共享 EventSink 下沉 haiku-model，Rust channel 保留 generation、关闭错误及原事件转发逻辑；macOS 原生回调改用可订阅 channel。仅将 data_root 纯函数提取到 haiku-store，保留 HAIKU_DATA_DIR 和原 GPUIX 默认路径，本任务未调用真实数据目录。
- playback 删除歌词 RPC 镜像转换；歌词解析仍归 haiku-lyrics。迁入 crate 保持参考版 edition 2021，避免混入 edition 升级；haiku-macos 整体 cfg(macos)。native/app 未改动、未接入领域 crate。
- 依赖在 workspace 集中声明，各 crate workspace=true，保留参考 feature 注释；新增直接依赖按参考锁定版本，Tokio 合并播放/下载所需 features。未引入 FRB/CJRT/Android 专用依赖。
- 首次 workspace test 在 loro 1.13.6 + 自动解析的 loro-internal 1.17.1 处失败；一次修复思路：Cargo.lock 对齐参考版 internal 1.13.6、common/kv-store 1.13.1，重试通过；未改依赖源码。
- 首次 Clippy 被原 macOS 三个 Objective-C ivar 访问函数的 mut_from_ref 默认 deny 阻断；一次修复思路：只在这三个函数局部 allow，保留原实现；重试通过。这不证明旧指针别名实现的安全性。

| crate | 参考代码测试数 → 迁入后 | 最终结果 |
|---|---|---|
| haiku-audio | 63 → 63（player 38 / effects 13 / loudness 9 / media_cache 3） | 62 通过，1 ignore |
| haiku-library | 21 → 21（local_scan 18 / local_library 3） | 全通过 |
| haiku-lyrics / haiku-store | 15 / 15 → 15 / 15 | 全通过 |
| haiku-services / haiku-macos | 10 / 6 → 10 / 6 | 全通过；macos 为 desktop_lyrics 4 + shortcuts 2 |
| haiku-palette | 2 单元 + 11 既有集成 → 2 + 11 | 全通过；任务参考值 2 只计单元测试 |

- 迁入的 143 个既有测试：142 通过、1 ignore；加原有 haiku-jsrt 11 个，workspace 合计 153 通过、1 ignore，0 失败。其他空壳 crate/app 仍为 0 个 Rust 测试，未新增测试。
- 唯一 ignore 沿用原 player::tests::output_pipeline_consumes_samples，原因：需要真实音频输出设备；未新增 ignore。
- `cargo test --manifest-path native/Cargo.toml --workspace`：最终退出 0；首次失败、依赖锁修复及重试均保留在 `.artifacts/m0-03/cargo-test.log`。
- `cargo clippy --manifest-path native/Cargo.toml --workspace --all-targets`：最终退出 0；旧源码 warning 11 处（audio 5 / library 2 / lyrics 1 / store 3），不修；另有 block 0.1.6 future-incompat 提示。首次失败及重试见 `cargo-clippy.log`。
- `nsh check`：退出 0；`nsh test`：退出 0，3/3，通过 1000 次回声、191 次重叠 UI tick 和双向 VM 隔离。证据 `.artifacts/m0-03/nsh-{check,test}.log`。
- 全程使用同一 toolchain Cargo（1.98.1 路径），未更换工具链重跑。仅失败项目各作上述一次修复并重试；未增加其他验收、探针、GUI 自动化或汇总 JSON。
- 第二次提交：本节随 `M0-03: migrate reference Rust core into domain crates` 纳入，哈希见最终汇报；本任务仅上述两次提交。
- 未覆盖：真实音频设备、NSH 中的 macOS 原生交互、channel/generation 适配的专项接线测试、其他平台；未 run、截图或打包。参考项目、README、上游依赖、Rex 会话保持不变；本次停止，不开展后续任务。

## 2026-11-09 M0-04：独立数据根、平台 spikes 与窗口 API 限制

- 第 0 步先修正 haiku-store::data_root：macOS 默认 `~/Library/Application Support/Haiku NSH`，其他桌面 `haiku-nsh`，保留 HAIKU_DATA_DIR；未创建/读取默认目录。原函数没有对应单元测试，本任务未增加额外测试。
- spike 代码集中在 `native/app/src/spike/{mod,audio,tracks}.rs` 和 `src/spike/{PlatformSpikes,List,List.test}.tsx`；只在 app manifest、注册入口、App.tsx 做接线，workspace 声明 audio 依赖并更新锁文件。未建立设计系统。
- Spike.playLocal/pause 直接调用迁入的 playback::Player；position 读取该引擎 Position 事件的原子快照，不用墙钟模拟播放。仅设置 HAIKU_SPIKE_AUDIO 时启动本地 fixture。
- S5 结论：完整验收未通过。可见窗口播放接口运行，日志 position() 从 0.628 增至 34.995 秒；不能用它冒充 Cmd+W 隐藏后继续播放。
- 读过当前 `.nsh/types/nsh/desktop.d.ts`、nsh-core 0.4.0 的 desktop.rs、element/desktop.rs、runtime/desktop.rs；Window 只有当前窗口操作，close 发 DesktopCommand::Close，声明明确最后窗口关闭会结束应用；无关闭拦截、隐藏窗口、reopen/激活回调。
- S5 因此未执行真实 Cmd+W/关闭按钮、隐藏后等待 10 秒或 open -a 恢复；真实按键待人工确认，但人工操作也不能补齐缺失的生命周期钩子。M0-05 需评估原生 close→hide、保持进程存活及 Dock/reopen 恢复。
- S6：Rust SpikeTrackList 使用 virtual_list、10000 条标题/艺术家/时长数据、44pt 固定行高、token 颜色；HashSet 按曲目 id 持有选择，事件和绑定在回收时读取当前 Position，行内无独立选择 signal。
- `nsh test src/spike/List.test.tsx --trace`：最终退出 0、1/1；第 9000 行可见，第 5 行选择经底部往返仍保留，底部无选中泄漏，返回后仅第 5 行选中。
- 上述验收首次因 Track 缺 PartialEq 在编译阶段失败；修复后因嵌套测试 open 路径解析失败；第二处修复为同目录 `./List.tsx`，最后重试通过。两种修复后停止扩展；全过程原日志保存在 `.artifacts/m0-04/nsh-test-trace.log`。
- 同条 trace 的滚动提交耗时为 2798/3549/3279/3259µs，选择更新 902µs；369 个节点未随 10000 条数据全量创建。该数据是 Headless 提交成本，不是 GPU 完整帧耗时；真实滚动帧率未覆盖，未自建测量工具。
- `nsh run --target macos`：构建并连接真实窗口；起始索引 4999，截图实际显示曲目 05000 起的中部内容，已查看。证据 `.artifacts/m0-04/nsh-run.log`、`list-middle.png`；截图后 Ctrl+C 停止，终端会话返回 1，不计作构建失败。
- 音频为本地生成的 35 秒、44.1kHz、mono、16-bit 常量采样 FLAC，文件及生成记录仅在 `.artifacts/m0-04/{silence-35s.flac,audio-fixture.log}`，不提交音频。
- 截图用只读 CGWindowListCopyWindowInfo 查询目标 id，再 screencapture -l；记录 `window-capture.log`。未使用 lldb、AppleScript、辅助功能、CGEvent 或自动 GUI 输入。
- S8 结论：第二窗口验收未通过。实现一个 Rust SpikeCounter，TSX 只经 onChange 镜像、经 increment 请求写入；设置窗口按钮禁用并说明缺少 API，没有用同窗面板或第二进程冒充第二窗口。
- 当前 nsh/desktop 和 Rust DesktopCommand 都无 create/open、窗口 id 或多窗口引用；Headless 也无窗口创建/切换驱动。skill tooling 提到的 Window.open/currentWindow 不存在于当前 0.4.0 源码/声明，未据新文档虚构调用。
- S8 的双窗同步、重复打开激活同窗、关闭设置不影响主窗均未覆盖。M0-05 需增加宿主多窗口/多 surface 接线及共享 Rust Core 的生命周期验证；本任务未修改 NSH 或引入原生补丁。
- 建议审阅者修订 UI.md §1 的 Cmd+W 与 §6 设置窗口、MACOS.md §1 及 ARCHITECTURE S5/S8：明确当前版本缺口，将原生宿主生命周期/多窗口列为前置项；不宣称 NSH 现成支持。
- 本任务只跑上述 Headless/trace 和一次真实窗口运行，未跑全 workspace、check、Clippy、打包或额外探针。沿用同一 toolchain Cargo 路径；`cargo update --workspace --offline` 仅维护 app 的依赖锁，0 个包升级，证据 cargo-lock.log。
- 未覆盖：真实音频听感、暂停按钮人工交互、S5 关闭/恢复、S8 所有真实双窗交互、真实滚动 FPS、其他平台。应用仍为 spike；不开展 S3/S4/S7 或 M1。
- 随一次提交 `M0-04: data root fix, NSH platform spikes (S5, S6, S8)` 收尾；用户已有 ARCHITECTURE 修改和未跟踪 M0-04 任务文件保持原样，不纳入本次提交。其余 specs、README、参考项目、NSH/依赖源码、Rex 均未修改。

## 2026-11-09 M0-05：macOS 原生验证（S3、S4、S5、S7）

- 新增 `haiku-macos/{lifecycle,glass,web_auth}.rs`；应用接线位于 `native/app/src/spike/macos.rs` 与 `src/spike/`，通过 app 根部 `#[native_components(macos)]` 注册玻璃背景。
- 主线程入口：从引擎用 GCD `Queue::main().exec_sync` 派发；原生组件工厂及 WebKit 集成测试本身在主线程，以 MainThreadMarker 约束。
- NSApp：`NSApplication::sharedApplication`；主窗口：`mainWindow`，回退 `keyWindow`、`windows().firstObject()`。
- S5：确认 NSApplication/NSWindow 共用现有 NshAppDelegate 实例；运行时添加无 ivar、等实例大小的子类，只覆盖 windowShouldClose→orderOut/NO，保留实例身份与原有回调。
- reopen 与 Cmd+Q 继续继承 NSH 原 delegate 的实现；默认应用菜单已有 Close（Cmd+W）。未知 delegate 类型或布局不匹配时拒绝安装。
- S4：复用 system.rs、menubar.rs，发布固定 fixture 标题、35 秒时长、实际位置和播放状态；媒体与状态栏事件经 channel，由 NSH 引擎线程 tick 收取，再调用同一播放器的 play/pause/toggle/next。
- S3：查过 NSH 0.4.0 的 WebView/ElementRef 接口，无 Cookie 读取入口；采用独立 nonPersistentDataStore + WKHTTPCookieStore，未完整实现 NSH WebView 方案。
- `cargo test --manifest-path native/Cargo.toml -p haiku-macos --test web_cookies`：最终退出 0，本地 Rust HTTP fixture 的两个 Cookie（含 HttpOnly）正确；第一会话仍存活时新会话为空。证据 `web-cookies-repair2.log`。
- Cookie 测试使用 harness=false，保持 WebKit 和 run loop 在进程主线程；此前两次编译修复为 objc2 类型/feature 与方法 ABI 签名，原始日志保留。
- `nsh test src/spike/Glass.test.tsx --filter S7`：最终退出 0、1/1；原生空视图保留 64pt 底栏，叠放按钮触发一次。证据 `glass-headless-repair1.log`。
- 该命令首次被生成 host 锁中的 loro-internal 1.17.1 与 loro 1.13.6 不兼容阻断；对齐为 1.13.6，并在 haiku-macos manifest 精确约束，供 NSH 独立 workspace 解析。证据 `host-lock-alignment.log`；native 锁已同步。
- `HAIKU_SPIKE_AUDIO=<M0-04 的 silence-35s.flac> nsh run --target macos`：host 运行、delegate 安装成功，可见窗口 position 0.603→34.995；media.snapshot 标题为 M0-05 local fixture，播放态 1、结束后 2。证据 `nsh-run.log`。
- 截图：`native-window.png` 显示玻璃背景区域及上层 NSH 文字/按钮；`menubar.png` 显示曲名和播放控件。截图均在 `.artifacts/m0-05/`，已实际查看。
- S7 视觉缺口：WithinWindow 呈均匀灰色；先改 BehindWindow/Sidebar，再尝试透明 NSWindow，仍未从截图证实背景模糊。已到两种修复上限，停止进一步排查，不标玻璃视觉验收通过。
- S5 未覆盖/待人工确认：关闭按钮、Cmd+W、隐藏后 10 秒播放增长、open -a 恢复、恢复截图、Cmd+Q 正常退出。终端以 Ctrl+C 收尾（退出 1），不能代替 Cmd+Q 验收。
- S4 待人工确认：媒体键/控制中心暂停、菜单栏按钮点击，以及事件实际抵达引擎；NSPanel 不在本次安装范围。
- 全部新增原生实现使用 Rust/objc2，无新增 .m、build.rs 或独立 dylib；已迁移的媒体/状态栏继续使用原 objc/cocoa 实现。
- 构建有 CLI 汇总的 2/3 个 warnings；未额外重跑以展开。Cookie 测试提示原有 block 0.1.6 的 future incompatibility；未做全量测试、Clippy、release 或额外审计。
- 建议审阅者把 D18/API 描述限定为“NSH 公开接口不可配置”；其 macOS 内部 delegate 已实现 reopen/quit，本次直接保留。S3 记录 macOS 原生回退可行；S7 暂保留 NSH 半透明回退，待视觉结论后再决定。
- 本次一并提交审阅者已修改的 SPEC、ARCHITECTURE、UI、MACOS 及未提交任务文件；未自行修改这些正文，未修改 Rex 会话名、上游或参考项目。

## 2026-11-09 M0 人工确认（用户）

用 `HAIKU_SPIKE_AUDIO=.artifacts/m0-05/haiku-test-voice.m4a nsh run` 实测，全部符合预期：播放中 Cmd+W 隐藏窗口后声音继续；点击 Dock 图标恢复窗口；键盘媒体键与菜单栏组件的暂停按钮均可暂停/继续；Cmd+Q 退出并停止声音。S4（媒体中心、菜单栏回调）与 S5（关闭隐藏、重新激活）的人工确认项据此关闭。M0 完成。

## 2026-11-09 M1-01：设计系统与应用外壳

- 已完整阅读设计规范、指定 Coding Guides、UI §1–2/6、D7/D18；API 依据本机 0.4.0 源码与生成声明。
- `src/ui/STANDARDS.md` 定义 macOS standard 密度、compact 导航/工具栏、系统字体、12–20pt 字号、6/4/12pt 圆角及完整交互状态。
- 外壳侧栏 220pt、顶栏 56pt（不低于实际 titleBar 高度）、内容边距 24pt、侧栏底部预留 96pt；保留红绿灯与拖动区。
- `src/ui/theme/` 提供浅/深语义 token 与应用角色；17 个参考品牌色，默认 orange；深色向白混合 48%，前景按对比度选黑/白。
- 外观与品牌色由 TSX 内存信号持有；薄 Rust 异步导出调用 `set_tokens`，使 TSX 与 Rust 共用更新后的主题。
- `src/ui/` 包装 Button/IconButton/Tooltip/SidebarItem/SectionLabel/SearchField/Kbd/EmptyState 与 Dialog/Overlay/Menu/ContextMenu 表面；资源只复制所用 iconoir SVG 和一张插画，附许可证。
- `src/shell/` 提供类型路由、不透明 EntityRef、最多 100 条历史与前进截断；九页保持挂载，输入/页面状态保留；均为真实空态，无假数据。
- 设置为全窗口覆盖层，含全部分类与实际外观选项；命令面板用 base Command；快捷键和原生菜单调用相同命令。
- 删除 M0 spike、测试及 QuickJs fixture；关闭隐藏安装移入正式 platform 启动；媒体/菜单栏接线移除，haiku-macos 能力保留。
- `nsh check -v`：退出 0；`nsh build -v`：退出 0，应用/TSX 无新增警告，但“无 warn”严格条件未达到：依赖 `block 0.1.6` 仍有未来兼容警告。
- `nsh test -v`：退出 0，Headless 4/4；覆盖九个入口、后退/前进与截断、输入保留、Cmd+K 跳转、Cmd+, 切深色及 Esc 后页面/历史保留。
- 上述最终日志：`.artifacts/m1-01/nsh-{check,build,test}-layout-final.log`。
- `cargo test --manifest-path native/Cargo.toml --workspace`：退出 0，153 通过、1 忽略，另含 WebKit Cookie harness 通过；日志 `cargo-test-workspace-repair1.log`。
- workspace 保留 haiku-audio 原有 unused import/dead_code 警告；未为外壳任务改动领域实现或依赖源码。
- `nsh run` 四种状态已实际截图并查看：`main-light.png`、`main-dark-repair2.png`、`settings-light-repair1.png`、`command-dark.png`；均位于 `.artifacts/m1-01/`。
- 对应日志为 `run-light-repair1.log`、`run-dark-repair2.log`、`run-settings-light-repair1.log`、`run-command-dark.log`；开发 host 另有 `__eh_frame` 链接器警告。
- 截图入口固定为根目录 `ShellPreview.tsx`、`ShellDarkPreview.tsx`、`SettingsPreview.tsx`、`CommandPreview.tsx`；复用正式 AppShell，无 GUI 自动输入。
- 修复记录：初次编译被磁盘不足中断；类型修正为 Image style.objectFit/fixture 窄类型；主题同步导出缺 runtime 改为异步任务；输入断言改读 value，设置 Esc 仅在打开期间注册。
- 截图入口起初不在 nsh.json 同级而缺少原生模块，移至根目录；GUI host 未继承预览环境变量，改用固定入口；设置动态 display 导致横排裁切，显式 flex-col 后已重截图确认。
- 最终截图正文、控件无裁切/错位；深色插画保留资源原色，部分装饰细线对比偏低，不承载交互或信息。
- 未覆盖：窗口缩放/大字号、真实鼠标与菜单交互、此次启动后的关闭隐藏/Dock 恢复人工复验、其他平台、外观持久化；M0 人工确认保留。
- 需审阅者判断是否接受现有依赖未来兼容警告作为“无 warn”例外；未屏蔽警告或修改上游。
- 本次仅自行修改 STATUS；按任务授权一并纳入原有未提交 specs 修改，以 `M1-01: design system foundations and app shell` 提交一次后停止；Rex 会话名保持不变。

## 2026-11-10 M1-01b：外壳视觉修订（截图验收受阻）

- 先更新 STANDARDS 与主题，再调整组件/外壳；侧栏 200pt、条目最小 28pt、正文 14pt/选中 medium、图标 16pt/图文距 8pt、行间距 2pt、组间距 16pt。
- 顶栏 48pt（以实际 titleBar 高度为下限）、图标按钮/搜索入口目标 28pt、页面/顶栏水平内距 20pt；红绿灯拖动区与顶栏共用高度。
- 圆角：radius 8pt、inset 6pt、panel 10pt、surface 14pt；主内容/设置右侧共用四周 8pt 留白的圆角面板，侧栏底色延伸，面板底部无 96pt 预留。
- 删除侧栏 Haiku/设置标题及下载上方分隔线；下载锚定侧栏底部，并保留侧栏未来 PlayerDock 的 96pt 空间。
- 新增 `src/ui/{Chrome,AppearanceOptions}.tsx`；模式选项采用 panel 卡片，品牌色等宽自适应网格，固定勾槽位，选中用 ring+勾；命令列表内距 6pt/选中圆角 8pt。
- 设置 12 类分别配置语义图标；新增 8 个实际使用的 Iconoir SVG，保留 MIT LICENSE，其中 Command/代码括号取自相同 Iconify 分发源的 Iconoir 7.12.2。
- Image 无单独路径 tint 接口，新增深色插画资源：浅色块映射主题 card/muted，细线映射 muted-foreground，保留紫色主色；浅深资源随 colorScheme 切换。
- `nsh check -v`、`nsh build -v`、`nsh test -v` 均退出 0，原有四个 Headless 测试全部通过，断言与选择器均未改；日志 `.artifacts/m1-01b/nsh-{check,build,test}.log`。
- UI 颜色检查通过：主题目录外无颜色字面量或调色板 class；SVG 资源为任务允许的浅深插画数据。证据 `theme-colors.log`。
- 构建/运行仅有约定允许的 block 0.1.6 future-incompat 与开发 host __eh_frame 提示；未跑全 workspace、Clippy、release 或额外探针。
- `nsh run ShellPreview.tsx --target macos -v` 成功连接；但 screencapture 返回 `could not create image from window`，窗口信息复核/重试与显式 `--display 1` 后仍失败，按修复上限停止。
- 运行日志 `run-light.log`、`run-light-display1.log`；捕获失败日志 `capture-light{,-repair1,-display1}.log` 与只读 `window-light*.log`，均在 `.artifacts/m1-01b/`。
- 四张截图均未完成，未确认实际裁切、网格、圆角及深色插画视觉；未用旧截图充当证据，失败原因尚未确认，已 Ctrl+C 停止 host。
- 按任务提交一次，纳入已有任务文件及通用约定修改；仅自行更新 specs/STATUS.md，Rex/上游/参考项目未修改。视觉验收需后续补齐，无新增架构决策。

## 2026-11-10 M1-01c：侧栏与顶栏密度微调

- STANDARDS/theme/共享组件先行：两侧栏宽 200pt、条目最小 32pt、行距 4pt、组距 24pt、标题至首条目 8pt；红绿灯区后 8pt，设置搜索至首分类 12pt。
- 主内容/设置顶栏 40pt，保留 `max(token, titleBar().height)` 下限；本次截图呈 40pt，图标按钮/搜索入口最小 28pt、垂直居中。
- “前往…”12pt、入口最小宽 176pt（增加约 40pt）；设置 placeholder 12pt，输入后仍为 14pt（NSH 无独立 placeholder 字号，按空值切换共享组件字号）。
- `nsh check -v`、`nsh build -v`、`nsh test -v` 均退出 0，原有 Headless 4/4、断言未改；日志 `.artifacts/m1-01c/nsh-{check,build,test}.log`，仅约定允许的 block 0.1.6 / 开发 host __eh_frame 提示。
- 两次截图前锁屏检查均为 false；主窗口与设置预览各运行/捕获一次，已实际查看，无可见裁切；截图 `.artifacts/m1-01c/{main-light,settings-light}.png`，对应 run/capture/window/screen-lock 日志同目录，截图后 Ctrl+C 收尾。
- 审阅者已确认 M1-01b 捕获失败原因是锁屏，用户已查看真实窗口；保留上节历史记录，本次两张新截图完成当前浅色视觉验收。
- 未覆盖：深色截图、窗口缩放/大字号、真实鼠标/菜单交互；无新增待决策事项，纳入任务文件并按指定标题提交一次后停止。

## 2026-11-10 用户反馈：侧栏布局与字号

- 主界面与设置侧栏统一图标 15pt、正文 13pt、分组/搜索提示/快捷键提示 11pt；主题和共享组件提供尺寸，导航条目最小高仍为 32pt。
- 搜索入口移至主侧栏顶部；新增共享 SidebarChrome/SidebarSearch，按 `titleBar().left` 留出原生红绿灯区域，与右侧后退/前进按钮组 justify-between、垂直居中。
- 主侧栏底部为下载/设置两个图标按钮，下载在左、设置在右；保留下载 testID、选中态、tooltip 和无障碍名称，底部改为 16pt 内距。
- 主内容与设置顶栏删除分割线和标题；主内容保留 40pt 拖动区，设置只保留关闭按钮；导航历史、菜单和快捷键行为保留。
- `nsh check -v`、`nsh build -v`、`nsh test -v` 均退出 0，原有 Headless 4/4、测试文件未改；仅 block 0.1.6 / 开发 host __eh_frame 已知提示，日志 `.artifacts/m1-sidebar-layout/nsh-{check,build,test}.log`。
- 两次截图前锁屏检查均为 false；主窗口与设置各运行/截图一次并实际查看，无可见重叠或裁切；`.artifacts/m1-sidebar-layout/{main-light,settings-light}.png`，运行和截图日志同目录。
- 未覆盖：深色截图、窗口缩放/大字号、真实鼠标与菜单交互；遵守通用约定未提交 git，未修改 Rex/上游/参考项目。

## 2026-11-10 M1-02：本地播放核心与 Core 事件桥

- 新增 model 的 EntityRef/Track/Error；EntityRef 使用 JSON 三元组无损编码，覆盖来源含 #、URL/引号/Unicode；SDK extra 原样保留。
- Queue 为纯 Rust 数据结构，支持四种模式、实际随机历史、跨轮避免重复、下一首插入、追加/删除/移动/跳转/替换后缀/清空，重复曲目按位置区分。
- 参考测试对照：queue.test.ts 全部 4 项逐项迁移；playback.test.ts 中队列相关的睡眠截断/进度保持快照/无缝交接 3 项迁移，另迁移本地化恢复续播/暂停恢复 2 项，共 9 个 ref_* 测试；其余新增 13 项播放测试。
- Controller 由 Tokio 单一 actor 持有业务状态，短命令串行；解析/打开/预加载是可 abort 任务，播放与预加载共用单调 generation，过期完成/事件丢弃，加载期间终态先缓存。
- AudioBackend/TrackResolver 可注入；真实适配器包装现有 Player，只解析 localPath，不引入网络；失效预加载边界会修复实际流，错误跳曲最多 20 次且仅向列表尾部。
- 注入单调 Clock；睡眠支持 15/30/45/60/90 分钟、延长 15 分钟、当前曲结束、取消/剩余时间，到期实际调用暂停。
- session.json 经 haiku-store 临时文件 + sync_all + 原子替换；普通变更按 5 秒节流，关键操作立即写；最多 500 项，截取包含当前项的窗口，恢复为暂停且不打开音频，带 radio sessionTag 的会话不恢复。
- PlayerState 仅低频状态和队列版本；高频位置/缓冲直接读音频线程计数器。Player 增加只读 progress；两处原有测试专用代码加 cfg(test)，消除它们接入应用后产生的警告。
- Core 改为 class，保留原有 initialize/applyTheme 静态调用；Core.open 每引擎唯一，HAIKU_DATA_DIR 可覆盖，core.player 提供播放/队列/音量/模式/睡眠/flush/onState→Stop，错误 Promise 带 code/retriable 等字段。
- CoreBridge 在引擎线程仅运行一个泵，写 signal 并 Callback.post；可见且播放时等待 next_frame 读取位置，暂停/隐藏不请求动画帧；位置信号最小 Rust 测试通过。
- 开发入口 Core.devPlayFiles/devVerifyPlayback 与 PlaybackSmoke.tsx 仅用于本次试播，文档明确 M1-03 删除；正式 TSX 界面未改，新增 CorePlayback.test.tsx 仅测试服务。
- `cargo test --manifest-path native/Cargo.toml --workspace` 最终退出 0：177 通过、1 忽略，另 WebKit Cookie harness 通过；日志 `.artifacts/m1-02/cargo-test-workspace-repair2.log`。
- `nsh check -v`、`nsh build -v`、`nsh test -v` 均退出 0；TSX 5/5（原有 4 + Core 1），另 Rust 时钟 1/1，CLI 汇总 6/6；日志 `.artifacts/m1-02/nsh-{check,build,test}.log`。
- nsh test 使用 `/tmp/haiku-m1-02-test.5ZYrGP` 数据根；上述构建/测试仅出现 block 0.1.6 与 __eh_frame 已知例外，无导出文档警告。
- 一次 `nsh run PlaybackSmoke.tsx --target macos -v` 播放既有 haiku-test-voice.m4a：0.778→1.855→2.773s；暂停两次均 2.773s；恢复至 42.932s，自然结束 Idle/0 且保留当前项；日志 `.artifacts/m1-02/nsh-run.log`，通过后 Ctrl+C 收尾。
- 该试播启动另出现 theme token 相关 unknown classes 运行时警告（含 sidebar-top-gap/sidebar-icon-size）；原因未确认，未修改已确认 UI 或扩展重跑；check/build/test 中无此警告，需后续处理。
- 修复记录：初次 workspace 编译补显式 JsonValue/Trace/Tracer 导入；第二次发现会话窗口有符号减法错误，改 usize 饱和减法后通过，原始日志保留。
- PLAYBACK 完整范围尚未覆盖：网络候选/1-2-4 秒恢复与提前 EOF 缓存处理留 M4；统计上报、专辑/节目 ListeningProgress 尚未接入；真实多曲无缝听感、系统媒体控制及界面留后续任务。
- 按指定标题提交一次并纳入任务文件；开始时 MILESTONES 无待提交修改；未修改 Rex/上游/参考项目。

## 2026-11-10 M1-03：本地曲库与 Rust TrackList（待审阅）

- 基线 `84c2a60` 为半成品；续接 service/可取消扫描/track_source，未重复 M1-02；旧试播入口与临时 Core 方法已在基线删除。
- Core.library 现已导出：持久根、扫描/取消、进度/变更→Stop、后台文件操作、Rust 查询句柄、分组分页、编辑/重置/逐文件写回、原生目录/封面选择与 Finder 请求；所有导出有文档。
- Rust TrackList 使用 virtual_list、固定 token 行高、按当前 Position 读取元数据，选择/锚点/焦点/滚动按 ID 存在数据源；支持 Cmd/Shift/Cmd+A、键盘移动/Return、双击与五项上下文菜单。
- 接口为 source 句柄 + albumColumn/durationColumn/currentId/active + onPlay/onAction/onSelectionChange；播放在 Rust 中构造当前完整筛选队列，TSX 不接收整库曲目数组。
- LibraryPage/TrackEditor 接入已确认外壳：歌曲/专辑/艺术家、EntityRef 历史、保留页面状态、搜索排序、根管理/进度/取消/错误、批量稀疏草稿、封面、重置和写回确认；分组按名称正/倒序。
- LibraryControls 复用应用组件；原有几何/颜色保留，仅新增 track-row-height token。修正显式 display:flex 导致的横向排列；浅深截图均已查看。
- 构建阻断由审阅者定位为 dyld 拒绝剥离后的宏 dylib（mis-aligned LINKEDIT string pool），不是缓存；设置 CARGO_PROFILE_DEV_BUILD_OVERRIDE_STRIP=false 及 CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP=false 后通过，无须改 profile 或生成 host；README 已记录。
- `cargo test --manifest-path native/Cargo.toml --workspace`：退出 0，180 通过、0 失败、1 忽略；服务测试覆盖根持久化/跨根去重、重扫与缺失文件保留覆盖、批量只改提交字段、有效元数据搜索排序；`cargo-test-workspace.log`。
- `nsh check -v`、`nsh build -v`：最终退出 0，类型/导出文档/应用类无新增警告；`nsh-check-complete.log`、`nsh-build-complete.log`。
- `nsh test -v`：最终 7/7（6 个 TSX + 1 个 Rust），`nsh-test-repair2.log`；新增可复用 Library.test/LibraryFixture/生成 FLAC helper，123 首临时音频验证空态→加根→扫描、专辑进入/返回、单元格回收保留选择、Cmd 多选批量专辑编辑保留标题/艺术家、双击 Beta→playing 且完整筛选队列 [Alpha,Beta]、恰好五项菜单。
- Headless 失败修复限两种：首次 /tmp 无 fs 授权；首次修复还受 asset:/ 模块 URL 影响；第二次使用显式专用 /tmp/haiku-m1-03-fixtures，README 说明先创建父目录。nsh.json 仅补 fixture/preview fs 根，原未声明时的其他权限保持等价；应用数据每次用独立 HAIKU_DATA_DIR、禁用 keychain，测试结束移除音频/根。
- 同时修复 JS finalizer 在 engine 销毁后访问查询 registry 的 panic；查询逐页释放，剩余 registry 随 engine 销毁；早期 Headless/浅色热重载日志保留旧错误，最终测试及深色新 host 无 panic/error。
- 正式 `App.tsx` 真实运行通过，未出现 unknown classes（`run-app-strip.log`）；浅深预览也无该警告。旧独立试播未加载主题是可能原因，未作为已证明因果；无需修改已确认 token 声明。
- 固定 LibraryLightPreview/LibraryDarkPreview 真实运行，各扫描 3 首 35 秒 FLAC，report.errors=[]；日志均记录 Beta、完整筛选队列 [Alpha,Beta]、playing/error:null，随后自然 idle；`run-light.log`、`run-dark.log`。未做 GUI 自动操作。
- 浅/深截图：`library-light-final.png`、`library-dark.png`；首次窗口 ID 截图失败，重新读取当前窗口 ID 后成功，日志 capture-light*.log/capture-dark.log；真实窗口均 Ctrl+C 收尾。
- 全部证据位于 `.artifacts/m1-03/`，音频和预览数据不提交。已知例外仍有 block 0.1.6、开发 host __eh_frame；额外 huacnlee/cargo 的“忽略 target-dir，使用 ./target”警告已汇报，严格“仅已知例外”条件未达成，未换工具链或改依赖。
- LIBRARY 全范围未覆盖：刮削/加入歌单按任务留后续；原生目录/封面选择器、Finder、真实鼠标双击和听感待人工确认；未扩展格式矩阵、手工重置/写回/取消视觉验收或 M1-04。
- 保留全部工作区供审阅，不提交/推送；specs 仅修改本 STATUS，审阅者 HANDOFF/任务改动保留；未修改 Rex 会话/布局、参考版、NSH 或依赖源码。

## 2026-11-10 M1-04：播放界面与本地歌词（按静态覆盖层方案收尾，待审阅）

- 基线为审阅通过的 `62e85c2`；本轮未提交/推送，未修改 Rex、参考版、NSH 或依赖源码；specs 仅改本文件，README 未改。
- 第 0 步：TrackList 普通序号改为 muted-foreground，当前播放指示保留 primary。
- NSH 0.4.0 的 capabilities 为应用级配置，未找到按入口区分的配置；保留既有 `/tmp/haiku-m1-03-fixtures` 与 `.artifacts/m1-03` 授权，未扩大，**M6 打包前移除**。
- 新增 Rust player_dock/seek_bar/now_playing/lyrics_view/player_controls/presentation/player_theme；Core 初始化唯一展示状态，position/buffered 留在 Rust，buffering 时停止帧时钟推进。
- PlayerDock 根部唯一实例，左右/底部 24pt，80pt 高；普通进度使用中性 token，只有中心播放按钮用 primary；侧栏底部留 112pt（96pt 预留加原 16pt 内距）。
- Dock 接口 onOpenChange/onLyricChange（仅当前行变化）；SeekBar 接口 position/duration/buffered/nowPlaying/onSeek({seconds})；NowPlaying 接口 open/onOpenChange。
- Dock 提供封面、上一首/播放暂停/下一首、模式、音量 Popover+Slider、虚拟队列跳转/删除、歌词入口；空队列保留布局并禁用播放操作。
- SeekBar 主指针按下捕获 left/width/duration，横移 ≥1pt 激活，预览独立于时钟，松开仅提交一次；缓冲/已播放独立图层。
- 本地歌词按有效曲目元数据加载（含暂停恢复）；sidecar→内嵌，UTF-8/GBK/UTF-16，译文规则复用；补 1MiB 边界与内嵌文本上限。
- Cursor 二分查找且仅行变化发布；LyricsView 虚拟行、点行 seek、当前居中、邻行渐隐、用户浏览 4 秒后回跟；KaraokeLine 逐字裁切直接绑定 Rust 时钟。
- 当前及 Controller 实际 next_index（含随机/单曲循环）的封面在后台预热，按路径/mtime/长度缓存 12 项；请求版本丢弃旧完成；运行时覆盖深色 now-playing/lyric token，新增准确 HCT tone 15/38。
- 所有 Cargo/NSH 命令设置 CARGO_PROFILE_DEV_BUILD_OVERRIDE_STRIP=false 与 CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP=false，未改 profile/工具链。
- `cargo test --manifest-path native/Cargo.toml --workspace`：183 通过、0 失败、1 忽略；`cargo-test-workspace.log`，新增 Cursor、UTF-16 双端序/1MiB 边界、配色过期版本测试。
- `nsh check`：首轮 API 用法错误；修复一统一 0.4.0 类型/样式 API；修复二补 Tooltip 元素、Clone 与 f32 类型后通过；`nsh-check{,-repair1,-repair2}.log`。
- `nsh build`：通过，无 unknown classes/导出文档警告；`nsh-build.log`。
- `nsh test`：9/9（8 个 TSX +1 个 Rust）通过；`nsh-test.log`。新增 Player.test、PlayerFixture、SeekBarFixture，覆盖阈值/预览/单次提交/非主键、空队列/命令、设置内控制、导航状态/Dock 实例保留、暂停歌词/点行/切歌清理。
- 上述初次自动化之后，真实预览暴露封面权限与覆盖层几何问题；审阅者授权本次续修，最后代码变更后的 check/build/test 结果见下，workspace 沿用既有结果。
- `scripts/generate-player-fixture.py` 生成两首 120 秒低音量 FLAC，含内嵌 PNG、40 行增强 LRC/译文；媒体/各预览数据根均隔离在 `.artifacts/m1-04/`，禁用 keychain；`generate-media.log`。
- 真实首次失败：file:// 封面被 NSH 拒绝。修复一在 Rust 有界读取封面，转内存 data URI，并与配色共同缓存；未增加权限；`nsh-run-light.log`、`nsh-run-light-repair1.log`。
- 浅/深本地曲库+Dock 截图已查看且通过：`library-dock-light.png`、`library-dock-dark.png`；对应 `nsh-run-light-repair1.log`、`nsh-run-dark.log`、`capture-{light,dark}.log`。
- 真实扫描两首、errors=[]，播放 loading→playing/error:null；Rust 与 JS 低频日志记录 Cursor 0→1→2…（约每 3 秒）；三份最终预览日志均有证据。
- 历史失败证据保留：`now-playing.png`、`now-playing-repair2.png` 及对应 run 日志；此前显式高度方案仍裁切。
- 审阅续修：确认 nsh-core 0.4.0 ambient.rs 的 window() / TS windowSize() 均为逻辑 pt；NowPlaying 与 Settings 删除显式 height，统一 top=0、bottom=112、left/right=0，布局留出 Dock。
- Settings 保持已确认内容/主题，新增 PlayerSettingsPreview 复用真实扫描播放入口；浅色 `settings-light-review.png` 已查看，通过，Dock 完整可见；`nsh-run-settings-review.log`、`capture-settings-review.log`。
- 新增 Rust Headless 几何回归：实际 NowPlaying 内容根 ref.measure() 的 top≈0、bottom≈688（1200×800 窗口）；只在 cfg(test) 测量，不增加产品调试接口。**API 不含动画 transform，不能证明原生屏幕位置正确。**
- 上轮失败历史：约束修正后 `now-playing-review.png` 仍下移；追加数据根移至仓库外、Rust popup 显式 flex-col 两种修复，`now-playing-review-repair{1,2}.png` 均仍越出底部；审阅者随后判定 SheetMotion fade 未完成，并授权替换 Sheet，不再排查其内部。
- 上述截图均有 `nsh-run-now-playing-review{,-repair1,-repair2}.log` 与 capture 日志；仓库外数据隔离在 `/tmp/haiku-m1-03-fixtures/m1-04-review-{settings,now-playing}-data`，禁用 keychain；未扩大授权。
- 上轮 `nsh check -v`、`nsh build -v`、`nsh test -v` 均退出 0，测试 10/10（8 TSX+2 Rust）；`nsh-{check,build,test}-review-final.log`，首轮续修日志也保留。
- 动画尝试历史：替换 Sheet 为自绘覆盖层、独立 animated_value 250ms timing 后 Headless 10/10，但 `now-playing-owned-overlay.png` 在约 27 秒仍仅背景/Dock，真实日志无入场完成事件；`nsh-{check,build,test}-owned-overlay.log` 与 `nsh-run-now-playing-owned-overlay.log` 保留。
- 最终按用户批准的静态方案：NowPlaying 无 translate/animated_value/timing，打开直接 y=0、收起立即卸载背景与内容；绝对定位 top/left/right=0、bottom=112、z=20（Dock z=30），保留原双栏内容与几何测试。
- Esc 仅打开期间注册 Rust keymap（Always）；内容挂载 effect 请求焦点，不等待 next_frame。Dock 不卸载，接口 open/onOpenChange 保留；Settings 未改动，沿用已审阅通过截图。
- 最终代码后 `nsh check -v`、`nsh build -v`、`nsh test -v` 各一次，均退出 0，Headless 10/10（8 TSX+2 Rust），含几何与打开/收起/导航/Dock 保留；`nsh-{check,build,test}-static-overlay.log`，两个 STRIP=false 环境变量均设置。
- **最终静态覆盖层真实截图通过**：`now-playing-static-overlay.png` 已查看，封面/唱片、标题、歌词/译文位于窗口内，Dock 完整可见；`nsh-run-now-playing-static-overlay.log` 证明扫描 2 首 errors=[]、40 行歌词、playing/error:null 与行推进；截图日志 `capture-now-playing-static-overlay.log`。数据根隔离于 `/tmp/haiku-m1-03-fixtures/m1-04-static-overlay-preview-data`，禁用 keychain。
- **已知缺口：入场动画未实现**。真实 host 中 Rust 组件的 animated_value timing 不推进，Headless 正常，原因待查；可能与 CoreBridge 每帧 next_frame 有关（未验证因果），留后续复核。当前打开/收起均直接切换，偏离原动画要求，按用户明确批准收尾。
- unknown classes 结论：最终 check/build/Headless/真实运行均未出现；编译警告仅有允许的 block 0.1.6、__eh_frame、huacnlee/cargo target-dir；停止 host 后有 dev WebSocket 断连 debug 日志。
- 待人工确认/后续：动画问题复核与恢复后的观感、真实拖动手感、听感、队列/音量弹层逐项人工检查；其他平台/窗口尺寸/大字号未覆盖。未做 GUI 自动操作，所有预览 host 已 Ctrl+C 收尾。
- 全部证据位于 `.artifacts/m1-04/`；保留历史失败证据与工作区供审阅，M1-04 按入场动画缺口收尾，不提交/推送，不进入后续任务。


## 2026-11-10 M1-05：音效/响度、缓存、持久设置与媒体中心（待审阅）

- Rust Settings schemaVersion=1，保持参考版 themeMode/brandColor/audioEffects/volumeNormalization/preventScreenSleepOnPlayer/mediaCacheEnabled/mediaCacheMaxBytes/audioQuality 字段名；未知顶层/嵌套字段由 serde flatten 保留，移动端字段不丢失。
- SettingsStore 互斥合并、临时文件+sync+原子替换、数值 clamp（含负数容量）；失败不发布。损坏文件改名 settings.json.corrupt-<时间> 后默认恢复并输出诊断；尚无 UI 通知。
- core.settings.get/update/onChange→Stop 有导出文档；更新由 Rust 串行落盘，再应用播放器/缓存并通知镜像；外观启动恢复，TSX 不再持有权威偏好。
- RealAudio 延迟创建设备时应用常驻效果，当前/预加载使用带 EntityRef 的逐曲响度；切歌保持 EQ/前级/声场/平衡/mono；软削波与 8ms fade 使用既有 DSP 固定保护，不新增设置字段。
- 响度记录按 EntityRef 持久化 loudness.json，固定 -18 LUFS/峰值余量；只有完整或 ≥60秒/≥半首记录作为已知增益，完整优先、同级覆盖秒数更多优先。
- 缓存根按任务为 <data>/media-cache/（区别 DATA 的 cache/media/）；键为 (EntityRef, quality) 的 JSON 编码，本地路径不生成缓存键；Controller 发布当前/实际下一首 pin，M1 仍只解析本地文件。
- 修正访问时未更新 mtime 的 LRU 与 finalize 未校验完整性的缺口；容量、统计、清理、残片/失败删除沿用媒体缓存模块；清理只作用 objects/，测试证明原始本地文件保留。
- Settings 增加播放与存储分组，共享 SettingsControls 包装 Switch/Slider/Select；十段 EQ、8 个内置与自定义预设、其他效果/响度/防息屏、缓存容量/用量/清理确认；文件夹入口跳转本地曲库页。
- 媒体服务归 CoreBridge detached owner；原生事件 channel→引擎→Controller，favorite 不注册；媒体写入串行，仅曲目/状态/seek/模式/前后可用性变化触发，seek 使用已提交位置，不逐帧写系统信息。
- 本地封面输入上限 8 MiB；防息屏仅设置开启且正在播放页可见、应用非 Background 时持有；AppKit 终止通知与 Core 销毁清理媒体信息和 assertion；关闭隐藏/Dock reopen 原委托逻辑保留。
- 全部 Cargo/NSH 构建均设置两个 CARGO_PROFILE_{DEV,RELEASE}_BUILD_OVERRIDE_STRIP=false；所有启动/测试 HAIKU_DATA_DIR 位于 /tmp/haiku-m1-03-fixtures/m1-05-*，HAIKU_NO_KEYCHAIN=1。
- cargo test --manifest-path native/Cargo.toml --workspace：最终 189 通过、0 失败、1 忽略（另原有 WebKit harness 通过）；cargo-test-workspace-final2.log。初轮 188 通过；媒体生命周期/clamp 收尾与补强 Controller 切歌/预加载测试后保留续测日志。
- nsh check -v / nsh build -v：最终退出 0，无类型/导出文档/unknown classes 警告；nsh-check-complete.log、nsh-build-complete.log。
- nsh test：初轮 10/11，新增测试拖动 Slider 外层未命中实际 thumb；修复一改用无障碍 thumb 标签并订阅已提交配置后 11/11；nsh-test{,-repair1}.log。最终代码续测仍 11/11（9 TSX+2 Rust），nsh-test-complete.log。
- 新回归覆盖设置原子合并/未知字段/clamp、真实 DSP 映射与有限/限幅输出、Controller 切歌/预加载保持、响度信任与覆盖、缓存容量/LRU/pin/完整性/原文件保护；Headless 覆盖重开 Core 后外观持久化、EQ 与播放器配置一致、清理取消/确认。
- 复用 M1-04 生成的无版权 120秒 FLAC/封面/歌词，复制到本任务 media/；两个真实预览分别扫描 2 首 errors=[]，低音量播放后由正常 Controller 命令暂停，无 GUI 自动操作。
- 浅色截图 settings-playback-light.png / settings-storage-light.png 均已实际查看，内容在窗口内、Dock 完整；对应 nsh-run-{playback,storage}.log、capture-*.log、window-*.log。两份运行日志均含媒体中心“暮色归途”、has_artwork=true、playback_state 1→2。
- 两个预览均 Ctrl+C 收尾；全部证据 .artifacts/m1-05/，原始失败与续测证据保留，不生成额外汇总 JSON/hash。
- 警告：block 0.1.6 与 __eh_frame 为允许例外；仍有 huacnlee/cargo 的 ignoring configured target directory 提示，故严格“仅已知例外”未满足；未换工具链或修改依赖。
- 未覆盖/待人工：媒体键与控制中心按钮、关闭隐藏后续播、Dock 恢复、Cmd+Q、真实防息屏/听感；其他平台与深色新分组视觉未验收；网络缓存真实写入/恢复留 M4，下载及其他设置/通知/日志留后续。
- 未引入新的架构决策；目录差异按本任务具体要求执行，后续由审阅者统一 DATA 路径说明。未修改 README、Rex、参考版、NSH 或依赖源码；specs 仅改本 STATUS，保留审阅者未跟踪任务文件；不提交/不推送。

## 2026-11-10 构建修复：普通 nsh run

- 用户直接执行 nsh run 时缺少两个 STRIP=false 环境变量，复现交接中记录的 proc-macro dylib 被 dyld 拒绝问题，表现为 E0463 rquickjs_macro。
- 新增仓库根 .cargo/config.toml，配置 dev/release 的 build-override.strip=false，同时覆盖 native 与 .nsh/rust 生成 workspace；不再依赖手动 export。
- 显式移除 CARGO_PROFILE_DEV_BUILD_OVERRIDE_STRIP 与 CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP 后运行 nsh run：Rust crate、App.tsx、macOS 开发 host 均构建成功，真实应用 connected。
- 验证使用 /tmp/haiku-m1-03-fixtures/build-strip.* 隔离 HAIKU_DATA_DIR，HAIKU_NO_KEYCHAIN=1；启动后 Ctrl+C 收尾，无 GUI 自动操作。
- 日志：.artifacts/m1-05/nsh-run-repo-strip-config.log；host 汇报 1 条未展开警告，未额外重跑；release 配置未另做打包验证。
- 未修改 README、生成 host、依赖、工具链、参考版或 Rex；保留工作区，不提交/推送。


## 2026-11-10 M2-01：运行时 R1–R12（待审阅）

- 所有操作均在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh`；只改 Rust/依赖及本 STATUS；未提交/推送，未改 UI、Rex、参考版或上游。
- 保留审阅者既有改动/未跟踪任务说明；根 `.cargo/config.toml` 的 dev/release build-override.strip=false 按审阅者明确授权保留，本轮未另改。
- RuntimeOptions 配置实例 id/内存/描述模式/宿主模块/日志/HTTP 钩子/KV 路径；独立 Runtime/Context 与专用线程，串行队列、超时退役、shutdown/Drop join。
- 新增 load_package/unload/reload/describe、CallHandle/AbortHandle、导出路径/this、JSON args+逐次 ctx（signal/http/storage/log）；取消同步 JS 与异步调用后可继续使用实例。
- 包 loader canonicalize 根/目标，拒绝绝对路径/父路径/符号链接逃逸；oxc 剥离 TS 与内存 source map；注册源码模块及 Rust ModuleDef 工厂。
- fetch/Headers/Request/Response 使用 reqwest+rustls；文本/JSON/原始字节、重定向/超时/AbortSignal、脱敏的请求响应元数据钩子；响应体限 16MiB。
- console 分级、实例 id、对象及文本敏感字段脱敏；KV 临时文件+sync+rename+父目录 sync，卸载/重建/热重载后保留。
- crypto 包装并保留 stdlib 导出/default/WebCrypto；补 createCipheriv/createDecipheriv/publicEncrypt，AES 三种 key size × ECB/CBC/CTR/GCM，padding/tag/AAD、PEM RSA PKCS#1 v1.5/OAEP。
- 续修一：恢复 stdlib DOMException 初始化前 QuickJS Error.prototype.stack descriptor，保持 M0 catch/read stack 路径，TS source map 还原后 R2/R4 通过；R10 原失败为 Buffer.equals 不存在，扩展导出已断言验证。
- R7 二进制原失败在 HTTP fixture 的 from_utf8_lossy；改为完整读取 Content-Length 后原字节回显，bytes/arrayBuffer 都验证 0x00/0x01/0xff 保真。
- 续修二：保留 reqwest is_timeout 语义；HTTP TimeoutError 不再误退役 VM；补 RSA 两种 PEM/三种 padding-hash 组合、GCM 认证失败和分段 update/final。
- 续修三只清理 clippy 的 collapsible_if 与测试 MutexGuard 词法作用域；不重复 workspace 构建，最终 nsh test 同进程用例再次通过。
- 新增直接依赖（沿用已锁定版本）：reqwest 0.12.28、serde 1.0.229、tempfile 3.24.0、regex 1.12.2；aes 0.9.4、aes-gcm 0.11.1、ctr 0.10.1、rsa 0.10.0-rc.19、rand 0.10.3、sha1 0.11.0、sha2 0.11.1。
- TS 新增 oxc_allocator/parser/span/semantic/transformer/codegen 0.155.0；oxc_sourcemap 9.0.0 沿用锁定版本；仍使用同一套 quickjs-jit 0.14.1。
- 所有构建设置两个 CARGO_PROFILE_{DEV,RELEASE}_BUILD_OVERRIDE_STRIP=false；NSH/workspace 使用隔离 HAIKU_DATA_DIR=/tmp/haiku-m1-03-fixtures/m2-01-data、HAIKU_NO_KEYCHAIN=1；未换工具链。
- `cargo test --manifest-path native/Cargo.toml -p haiku-jsrt`：最终 24 通过、0 失败；`cargo-test-jsrt-review2.log`。早期依赖/API 编译失败、18/23 和 22/23 失败日志全部保留。
- `cargo test --manifest-path native/Cargo.toml --workspace`：203 通过、0 失败、1 忽略；`cargo-test-workspace.log`；含 jsrt_r1_r4_r5_r7_concurrent_with_nsh_ui_vm，通过真实 NSH JS engine 与两实例多调用/HTTP/取消，并验证 UI 期间和销毁后的事件响应。
- `cargo clippy --manifest-path native/Cargo.toml -p haiku-jsrt --all-targets`：修复两条局部告警后无警告；`cargo-clippy-jsrt-review3.log`，初轮日志也保留。
- `nsh check` / `nsh build`：退出 0；`nsh-check.log` / `nsh-build.log`，未输出警告。
- `nsh test`：退出 0，12/12（9 TSX+3 Rust）；`nsh-test.log`；runner 构建摘要提示 1 条未展开警告，未为展开警告重跑构建，不能认定其已属已知例外。
- workspace 仅 block 0.1.6 future-incompat 已知例外；最终 clippy/check/build 无新增告警；严格“仅已知例外”仍待审阅确认 runner 的未展开告警。
- R1–R12 每项的 r1_…r12_ 测试均通过；R1/R4 同实例排队/多实例，R5 同步/异步取消后复用，R7 多实例 HTTP/宿主取消/JS abort/请求超时；R2 包边界拒绝断言均已执行。
- R10 AES 对照 NIST SP800-38A/38D 向量；RSA 为固定公开测试 key 的 PEM 导入/加密/解密回归，未另跑 OpenSSL 交叉验证。
- §8 限制：GCM 仅 12 字节 IV/16 字节 tag；OAEP 仅 SHA-1/SHA-256；cipher update 已返回分段输出，但每段重算累计输入（大块/多段优化留后续）；fetch 为有界缓冲 body，未实现 Web ReadableStream/FormData。
- interrupt 无法抢占同步原生 stdlib/crypto/KV/模块读取与编译/宿主回调；返回后检查 deadline。内存限额覆盖 QuickJS heap；Rust HTTP（另限16MiB）/crypto/编译分配不计入。
- 待审阅决定：是否接受上述 crypto/fetch 子集与 update 重算实现进入 M2-02；确认未展开 runner 告警。未进入插件宿主/SDK/CLI/UI，无额外 GUI 验收。
- 全部证据 `.artifacts/m2-01/`，无一次性诊断脚本、汇总 JSON/hash；保留工作区等待审阅。

## 2026-11-10 M2-02：插件宿主核心、SDK 与 subsonic（待审阅，部分语义未完成）

- 全部读写/构建在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh`；不提交/推送；未改 UI、TSX 导出、参考版、NSH、Rex；保留审阅者未跟踪任务文件。
- cipher：ECB/CBC 保留尾块与 CBC 链状态，CTR 保留计数器/偏移，GCM 缓冲至 final 一次计算；新增三种 key size × 四种模式多分段加解密等价测试。
- `haiku` 模块提供 definePlugin、field、十种结构化错误、Context、HTTP builder；合并默认 headers、15s 超时、signal、cookie 持久化与脱敏调试追踪；JsError 增加 details 并保留原 stack/source map 路径。
- Host/Package/Descriptor/Source：描述模式生成 descriptor.json，类别/能力/认证方式/连接字段缓存；列出包只读描述符；Core::open 首次安装内嵌 subsonic，同数据根共享宿主。
- 安装：ZIP 10MiB/解压 50MiB 上限、路径/符号链接检查、临时目录校验、备份替换、实例重载；失败恢复旧包/VM/storage，升级保留凭据/KV；内置及被引用包禁止删除。
- sources.json 沿用参考版 version/sources/sourceId/pluginId/kind/displayName/addedAt/enabled 与顺序；保留顶层/来源未知字段；实例 id 使用规范化 identity 的 SHA-1 前 8 位，用户名大小写保留。
- 来源启停/排序/重命名/移除写入串行化；新连接 verify 成功才注册；已有连接 verify 失败恢复 storage/凭据及旧实例；secret 脱敏、credential.json 原子写且 0600。
- LoginDraft 提供 password/cookie/SMS/web done/QR 接口；QR 两秒 poll、等待期过期/取消、临时登录目录清理；AuthExpired 按 credential generation 单飞 refresh、合并凭据、重放一次，失败标记 needs_login。
- SourceApi 覆盖 search/track/detail/artist/library/toplist/categories/me/radio/home/scrape 调用；EntityRef 元组无损编码、丢弃坏条目并记警告、封面 {size}、Expired 重取 streams。
- `sdk/haiku.d.ts`、全能力类型夹具、tsconfig；`plugins/subsonic/plugin.ts` 105 行，含连接、token/salt、搜索、详情、流格式/码率、OpenSubsonic 歌词、库浏览、分页、错误映射。
- Subsonic fixture 在 plugins/subsonic/fixtures/replies.json；仅本地 HTTP，无外网；覆盖 401/404/429/503、搜索三类、album 分页、artist/playlist/browse、歌词、取消与 secret 不进入请求。
- 新增 haiku-plugins 直接依赖：serde/tokio/haiku-model/haiku-store/tempfile/log（workspace），sha1 0.11.0、url 2.5.8、semver 1.0.27、zip 8.6.0（仅 deflate）；dev reqwest（workspace）。App 增加 haiku-plugins。
- Cargo.lock 新增 zip 8.6.0、typed-path 0.12.4、zlib-rs 0.5.5、zopfli 0.8.3；未增加第二套 QuickJS。
- `cargo test --manifest-path native/Cargo.toml --workspace`：最终 210 通过/0 失败/1 忽略，jsrt 25/25、plugins 5/5、app 4/4；cargo-test-workspace-repair2.log。
- 首轮编译失败（sha1 digest Array 无 LowerHex）及 repair1 通过日志保留；repair2 补 storage/verify 回滚及并发只刷新一次、登录后 401 的明确断言；未再重复 workspace 构建。
- `node /Users/zsakvo/Project/GPUI/haiku/node_modules/typescript/lib/tsc.js --noEmit -p sdk/tsconfig.json`：通过；TypeScript 7.0.2，只读使用参考版编译器，无应用 npm 运行时依赖；tsc.log、tsc-version.log。
- `nsh check -v` / `nsh build -v`：通过；nsh-check.log / nsh-build.log；`nsh test -v`：13/13（9 TSX + 4 Rust），nsh-test.log，包含 Core 内置描述符读取与 NSH/插件同进程测试。
- Rust 验证均设置 HAIKU_DATA_DIR=/tmp/haiku-m1-03-fixtures/m2-02-data、HAIKU_NO_KEYCHAIN=1；沿用根 .cargo/config.toml，未换工具链。
- 警告仅 block future-incompat、huacnlee target-dir 与 __eh_frame 已知例外；全 workspace rustfmt 遇既有 lyrics_view.rs trailing whitespace，恢复其无关格式化改动后仅格式化本任务 Rust 文件通过。
- 未完成：SourceApi home.entries/Link/Shelf.more、creator/host/episodeCount/subtitle 等字段、无 id Artist 的保留、完整列表最多 100 页聚合；当前结构校验主要覆盖实体页/曲目。
- 未完成：SMS state/cooldown 尚由调用者管理；QR 到期未抢占正在进行的 poll，也未自动重启；描述符仅保存认证方式名，未保存 web URL/QR variants/password fields 等认证展示元数据。
- 未覆盖：cookie 罐持久化回归、SMS/cookie/web/logout 与刷新再失败回归、启动坏包隔离/安装中崩溃恢复；HTTP cookie 路径匹配为前缀简化实现。
- 审阅决定：是否接受上述未完成语义随 M2-03 补齐；M2-01 已接受的 crypto/fetch 子集保持，GCM 仍 final 缓冲，无新增架构决策。
- 全部证据 `.artifacts/m2-02/`；无一次性诊断脚本、汇总 JSON/hash；保留改动等待审阅。

## 2026-11-10 M2-02 小轮修订：D5 来源 ID 与 TS 可读性

- 插件 id 改为参考版的 subsonic.music，同步宿主内置包路径、插件测试与 app 内置描述符断言；未改 API 行为。
- 新增固定期望值回归：baseUrl=" EXAMPLE.COM:80/rest/ "、username=" User " 按参考 deriveSourceId 规范化为 "http://example.com|User"，得到 subsonic.music#eeb7a0de。
- plugin.ts 按常规 TS 换行整理为 297 行，最长行 104 字符；haiku.d.ts 展开声明、每个导出添加一行 JSDoc；格式与文档变更不改变语义。
- 仅执行指定三项各一次：cargo test --manifest-path native/Cargo.toml -p haiku-plugins（6/6）；tsc --noEmit -p sdk/tsconfig.json（通过）；nsh test -v（13/13）。
- tsc 沿用参考版已有 TypeScript 7.0.2；Rust 测试隔离 HAIKU_DATA_DIR=/tmp/haiku-m1-03-fixtures/m2-02-review-data、HAIKU_NO_KEYCHAIN=1。
- 证据：.artifacts/m2-02/{cargo-test-plugins-review,tsc-review,nsh-test-review}.log；警告仅既有 block、target-dir、__eh_frame。
- 上节列出的模型/聚合/认证状态/元数据/回归/坏包隔离与崩溃恢复遗留已获审阅者接受，纳入 M2-03 第 0 步，本轮未扩展；未提交 git。

## 2026-11-10 M2-03：遗留、刮削宿主、三插件、离线契约与 CLI（待审阅）

- 全部操作在主仓库 /Users/zsakvo/Project/GPUI/haiku-nsh；未提交/推送，未改 UI/TSX 导出、参考版、NSH、Rex；保留审阅者未跟踪任务文件。
- SourceApi：home 六类 Shelf、Entry/Link/more 校验并丢弃坏条目；creator/host/episodeCount/subtitle/quality/explicit 透传；无 id Artist 保留，reference=None 表示不可点击。
- 新增 SourceApi::collect_all，适用于任一类型化分页调用，最多 100 页；重复游标和超限明确报错。
- SMS state/cooldown 由 LoginDraft 保存，sms_verify 改为仅接受 code；QR 到期 abort 进行中 HTTP，原草稿可重新 start；logout 即使远端失败也清本地凭据。
- 描述符 auth 保存 QR variants、password fields、web url/userAgent、SMS 存在性；旧描述符兼容缺少 auth 字段。
- 回归覆盖 cookie 罐实例重载、password/cookie/SMS/web 登录、logout 清凭据、refresh 再失败→needs_login；来源启动失败存入 source_errors，不阻断其他来源，包列表跳过坏描述符。
- 安装备份新增原子 journal；未提交事务在下次启动恢复旧包/storage，已提交事务清理备份；清理残留 stage/login；测试模拟安装中断残留，未实际杀进程/断电。
- 新插件及 id：plugins/feiniu/plugin.ts（237 行，feiniu.music）；plugins/qq-scraper/plugin.ts（81 行，qq.scraper）；plugins/kugou-scraper/plugin.ts（88 行，kugou.scraper）。
- 飞牛：直连 connection、password-login SHA-256/deviceId、token 刷新、track/album/artist 搜索、album/artist/playlist 详情、原始/转码 streams、歌词与四类 library 浏览；暂未移植 FNID/中继/access-code。
- QQ：匿名搜索、年份/轨号、专辑封面、base64 LRC/翻译；酷狗：匿名搜索、封面模板、KRC XOR+zlib、LRC 回退；两者仅 scrape.search/lyrics。
- Core::open 首次安装四内置包，app 同进程测试逐一验证 id/类别/builtin；sourceId 仍沿用 M2-02 参考版规范。
- 刮削编排放 haiku-plugins/src/scraper.rs：参考 normalizeMatch、计数 bigram Dice、durationFit；有艺术家权重 0.5/0.3/0.2，无艺术家 0.8/0.2；自动 ≥0.86 且领先 ≥0.08，≥0.55 待审。
- Scraper 共享 4 permit、单源 8 秒 timeout 后 abort；按规范化标题/首位艺术家/时长跨源合并，记录 sourceCount；局部错误隔离，全失败报错。
- contract::run_contract / validate_schema：真实 runtime、隔离数据目录；能力覆盖、内容 schema、native id/get 往返、列表分页不重/终止/游标序列化、401/404/429/503、取消；另有 refresh+重放契约。
- 离线 SDK HTTP 经本地 fixture replay，未匹配路径失败；四插件 fixtures/contract.json 手写上游响应，无真实上游访问；酷狗覆盖 KRC 与 LRC 回退。
- 报告 JSON schemaVersion=1、pluginId、passed、cases[{name,passed,durationMs,error}]，ContractReport::summary 提供人类摘要；CLI JSON 写 stdout、摘要写 stderr。
- CLI native/tools/haiku-plugin：new <dir> --template provider|library|scraper；check <dir> 调 tsc 并打印描述符；test <dir> [--live|--record]；模板含 plugin.ts/tsconfig/fixtures/README，已有目标拒绝覆盖。
- HAIKU_TSC 可指定可执行文件或 tsc.js，缺失时明确提示；HAIKU_SDK_PATH 可覆盖声明文件。验收只读使用参考项目 TypeScript 7.0.2，无应用 npm 运行时依赖。
- 新增工作区工具 haiku-plugin，依赖现有 haiku-plugins/serde_json/tokio/tempfile；haiku-plugins 将 reqwest 0.12.28 从 dev 改为普通依赖；无新增第三方包/版本。
- cargo test --manifest-path native/Cargo.toml --workspace：最终 221 通过/0 失败/1 忽略，plugins 16/16；cargo-test-workspace-repair2.log。
- 首轮为 QR future 借用错误；repair1 两夹具误用全局 setTimeout；repair2 改为 pending Promise/本地慢 HTTP 后通过；失败日志保留，未超过两轮修复，未重复 workspace 构建。
- 三模板各 new/check/test 一次均通过：provider 5/5、library 5/5、scraper 4/4；cli-{new,check,test}-{provider,library,scraper}.log，生成物在 templates/。
- 四内置包各 check/test 一次均通过：subsonic 20/20、feiniu 21/21、qq-scraper 9/9、kugou-scraper 10/10；cli-{check,test}-<name>.log。
- tsc --noEmit -p sdk/tsconfig.json 四插件检查通过（tsc.log）；nsh check -v / build -v 通过；nsh test -v 13/13（含四内置包 app 断言）；nsh-{check,build,test}.log。
- Rust 验收隔离 HAIKU_DATA_DIR=/tmp/haiku-m1-03-fixtures/m2-03-data、HAIKU_NO_KEYCHAIN=1；CLI 使用 m2-03-cli-data；警告仅 block、target-dir、__eh_frame 既有例外。
- 未完成/偏离：契约自动翻页目前覆盖 page:* 列表，detail.tracks 尚未自动翻页；认证由宿主回归/独立 refresh 契约覆盖，未把所有认证方式逐包自动枚举。
- 未完成/未覆盖：replay 钩子覆盖 ctx.http，直接裸 fetch 尚未拦截；--live/--record 为基础实现，尚需区分故障注入场景、补响应序列/query/cookie/header 录制；未联网验证、未执行真实录制。
- 飞牛 FNID/中继/access-code、真实服务器与需鉴权封面展示未验证；记录器当前写 fixtures/recorded.json，复杂真实认证录制不能视为完整可用。
- 发现 stdlib 默认 loader 未注册 timers/global setTimeout；本轮不改 jsrt，测试不依赖它；是否补运行时注册与上述契约/联网增强，交审阅者决定后续任务。
- 全部证据 .artifacts/m2-03/；未留一次性诊断脚本或额外汇总 JSON/hash；本节范围外 UI/webview/qq.music/GUIDE 留 M2-04。

## 2026-11-10 M2-04：遗留与设置核心（部分交付，待审阅）

- 在主仓库工作，无 git 提交；按额度优先交付第 0 步及设置核心，F02/F03 保留未完成状态。
- jsrt 新增四个 timers 全局，调用 abort 清理其 timers、卸载释放 VM；新增通用 fetch URL 钩子，裸 fetch 也进入离线 replay。
- replay 钩子启用时返回重定向、不跟随到外网；record/live 仍为基础版。
- feiniu 实现参考 FNID 签名解析、中继 Cookie、安全码 base64 头；identity 仍按参考 URL 字段规则，登录保留原始输入。
- 契约遍历 detail.tracks、检测第二页重复；Subsonic fixture 补齐按曲目 ID 的 getSong 应答。
- core.sources/core.plugins 共用 PluginService：快照/状态 Stop、来源启停排序重命名移除、取消式连接、五种认证驱动。
- 导出目录/ZIP/单 TS/HTTPS 导入、包启停/删除、能力调用/结构警告、KV 查看清理、登出清凭据、离线契约与日志/HTTP 事件 Stop。
- Rust 持有临时登录、用户展示信息与状态，TSX 镜像；登出清连接字段及用户信息，包禁用阻止新连接/登录。
- 三个设置面板复用已有组件/token；来源表单 secret 输入、包详情 Popover/删除确认、开发者 JSON 参数/结果树/日志。
- NSH 0.4.0 WebView 声明无 cookie 读取/独立会话；macOS 使用默认 WKWebViewConfiguration，无隔离数据存储设置。
- 因此不启用共享会话内嵌登录，web 表面要求手动粘贴 Cookie 和完成 URL，再调用 auth.web.done。
- workspace 首轮新增 detail 检查发现旧 fixture 缺 ID 应答；修复后 224 passed / 0 failed / 1 ignored（含 jsrt 26、plugins 18）。
- feiniu CLI check 通过、test 21/21；全插件 tsc 初轮发现设置测试插件缺 streams，修复后通过。
- nsh check 经两轮修复通过（菜单项使用 children）；nsh build 通过，行为修复后的 final build 亦通过。
- nsh test 初轮订阅任务上下文错误；首修又发现 signal.set 失去 this，二修后 14/14（含 Rust 4）。
- Headless 覆盖本地 HTTP Subsonic 添加成功、失败无残留、状态 Stop、包导入/启停/删除确认、开发者真实调用结果。
- 证据：`.artifacts/m2-04/` 的 cargo-test-workspace[-repair1]、cli-*-feiniu、tsc[-repair1]、nsh-* 日志，保留首次失败。
- 浅色真实窗口：sources.png、login.png、plugins.png、developer.png；预览使用本地 fixture，无真实账号/上游请求。
- 未完成：整个 dev-plugins/qq-music 与其 CLI/契约、sdk/GUIDE.md；未声称 QQ 认证或内容能力已覆盖。
- 未完成：开发者目录自动监听/自动参数表单（目前手动重载/JSON 数组）、凭据明文查看、logo 与系统文件选择器。
- 未完成：仅提供 image 的二维码渲染，url→二维码生成未做；QR/SMS/web 面板、事件 Stop、连接中取消缺专门 UI 回归。
- 待人工：真实 Subsonic/飞牛（含安全码）及认证交互、键盘焦点/滚动；既有设置左上橙色细线仍存在。
- 审阅建议：本轮可审第 0 步及设置核心；安排剩余项后再关闭 M2，不将 F02/F03 标为全部完成。


## 2026-11-10 M2-04 UI 修订

- 主仓库内修订，无 git 提交；qq-music、GUIDE、目录监听/自动表单、logo 文件选择器按审阅要求留 M2-05。
- 来源：库副标题为服务器主机（含端口）· 用户名；平台显示登录用户/未登录状态。库操作改为编辑连接，预填非敏感字段、重新输入密码并 verify；右侧开关与按钮统一对齐。
- 插件：统一 logo/占位图标、名称、版本 · 作者 · id；开关固定右列、仅保留无障碍标签；主体行打开详情 Popover。
- 开发者：结果树分支与叶子共用图标槽，每层缩进 16pt、从左对齐；结果区限制 256pt 高度并内部滚动。
- 橙色细线定位为 display:none 外观面板中 checked Radio ring 的残留绘制；外观改为条件挂载，真实窗口确认移除。
- nsh check / build / test 各一次通过；14 tests passed，新增编辑连接与库来源无登录/登出回归；未新增构建警告（host 仍有既有例外）。
- 日志：.artifacts/m2-04/nsh-{check,build,test}-ui-revision.log；浅色真实窗口覆盖 sources.png、login.png、plugins.png、developer.png。
- 截图使用隔离数据目录与 loopback fixture，来源截图含同名、不同用户的两条来源；真实平台账号登录仍待人工确认。

## 2026-11-10 M2-05：QQ、开发工具与指南收尾（待审阅）

- 全部在主仓库工作，无 git 提交；qq.music 仅在 dev-plugins，不默认安装。
- QQ v2：QQ/微信 QR、SMS、Cookie、user/refresh/logout；四类搜索、track get/streams/lyrics、album/artist/playlist、toplist、home、categories、me 与 scrape。
- 协议层复用参考版请求/签名/QIMEI/QRC；AES/RSA 使用 haiku-jsrt crypto，凭据只经宿主注入与保存，不复制到 KV；QRC 保留原 GPL-3.0 归属注释。
- 契约新增 requestJson 请求体部分匹配、认证返回 schema、expectedJson 值断言；QQ fixture 全离线，含独立加密 QRC 向量与首页 song→tracks/入口转换。
- 修复包加载器声明模块时持锁造成二级相对导入死锁；增加三级模块导入回归。
- 开发者目录按内容哈希每 500ms 轮询，安装事务热重载保留 KV；错误保留旧包并报告状态，Stop 停止后续重载。
- 描述符导出标准 SDK 参数元数据，生成表单并保留原始 JSON；结果与虚拟化日志/HTTP 追踪分列、分别滚动。
- 包内相对 logo 校验边界/512KiB 后转 data URL，支持 HTTPS；内置包附参考 logo，无作者时省略作者项；ZIP/目录/TS 使用现有 Rust 系统文件对话框。
- Rust 将 QR url 编成带静区的 PNG；新增 qrcode 0.14.1（仅 image feature），复用已有 workspace image，无新平台依赖。
- GUIDE 已完成：概念→Subsonic 实际片段→五种认证→能力/分页约束→错误恢复→CLI/契约/面板→发布，说明 web 降级与 record/live 限制。
- workspace 首轮 228 passed / 0 failed / 1 ignored；新增首页值断言后再次回归同样通过，日志 cargo-test-workspace[-home-regression].log。
- QQ check 首次发现加载器死锁，修复后通过；test 首轮缺 auth schema，修复后 34/34，首次/修复日志均保留；tsc 全插件通过。
- nsh check 初轮静态文件选择器调用错误，改为 PluginService.pickImport 后通过；build 通过；test 17/17。
- Headless 包含自动表单真实调用、来源成功/失败、插件启停/删除、QR 过期/刷新/进行中取消、SMS 冷却、Cookie/web 降级及状态/事件 Stop。
- 证据 .artifacts/m2-05/：cli-{check,test}-qq-music*.log、tsc.log、nsh-*.log；浅色 plugins.png、developer.png、qrcode.png（真实 host，本地 fixture）。
- 待人工：真实 QQ/微信手机确认、短信与安全风控/订阅权限、原生选择器、编辑器保存时热重载体验；无真实账号/上游访问或录制。
- 偏离：目录轮询而非新增 OS watcher 依赖；参数元数据来自固定 SDK 签名，复杂认证使用 JSON；web 维持已接受降级；record/live 基础版。
- 既有已安装且无 logo 元数据的内置旧包继续显示占位图标；本轮随新安装包提供 logo，不自动覆盖旧包。
- 本轮无已知验收阻塞；M2 范围实现提交审阅，实际平台可用性不以 fixture 代替人工确认。


## 2026-11-10 M3-01：喜欢与用户歌单（修订后测试通过，截图未齐）

- 全部改动在主仓库，未提交 git；保留审阅者的未跟踪任务文件。
- collections 服务：串行修改、500ms 防抖原子写盘、句柄释放/Core.close/独立应用退出钩子冲刷、重开恢复、错误状态查询。
- 喜欢按 EntityRef 身份去重，专用 setLiked(bool/null) 写入路径；用户歌单 CRUD/描述、批量加歌/移除、曲目与歌单重排。
- 远端收藏沿用 favorite:<sourceId>:<remoteId> 和只读完整镜像；测试 (refreshedAt, version) 冲突取大方及重复导入。
- core.collections 导出列表/曲目/写操作、onChange(受影响 id)→Stop、onLiked→Stop、快照导入导出、Rust 虚拟查询与播放。
- Dock/正在播放/TrackList/媒体中心直接消费 Rust 喜欢状态；TSX 仅订阅镜像。
- 参考 fixture 由只读参考版 dist/haiku-native RPC 生成，写入 native/crates/haiku-store/tests/fixtures/reference-playlists.loro。
- 沿用旧根容器与 pluginId/trackId 字段；可选 sdk_track 扩展保留 SDK 元信息；本轮验证旧快照读取，未另跑参考版反读新扩展。
- UI：侧栏歌单滚动列表与新建、喜欢页搜索/来源/批量管理、详情合成封面/编辑/删除确认、总览列表/网格、原生曲目子菜单与菜单重排。
- 首轮 workspace 232 passed；nsh test 最终 12/18，复用数据根导致导航、来源与 EQ 断言污染；旧失败日志保留。
- 经审阅者授权修订：Core.openIn/openIsolated 按引擎选择根，临时目录随 Core/引擎存活；所有测试/预览先隔离再建业务镜像。
- 媒体缓存换根时重置进程单例；Settings 重开测试只显式共享本用例的目录，不清理用户目录或依赖执行顺序。
- NowPlaying 隔离后单项通过；整套同样通过，底边仍为 688（800−112），未改 Dock 高度或断言。
- 退出冲刷注册在 CoreBridge，macOS 生命周期独立订阅终止通知；测试覆盖无媒体会话时冲刷及注销、两个引擎独立根。
- collections 订阅 Library revision，刷新原生查询并通知受影响页面；tracks 读取本地有效元数据。
- 封面文件读取/base64 移到 blocking 池，按请求序号拒绝旧结果；同步 artwork 只读缓存，完成后推送 artwork 事件。
- Collections Headless 新增元数据标题/封面变化回归；原有 Dock→喜欢、菜单建歌单、编辑/删除、Stop、close/open 全通过。
- 修订 workspace 首次编译因新增测试 API 用法失败，修正后 234 passed / 0 failed / 1 ignored。
- nsh check/build 各一次通过；nsh test 首次 15/20（App 未等就绪、认证镜像过早 open），修正夹具时序后 20/20 全绿。
- 仅既有工具链/依赖警告；日志：cargo-test-workspace-revision{1,2}.log、nsh-{check,build}-revision.log、nsh-test-revision[-repair].log。
- 证据均在 .artifacts/m3-01/；repair-now-playing-isolated.log 保留单项结果，reference-fixture.log 保留参考快照生成记录。
- 已重拍真实浅色 likes.png、playlist.png（四格合成封面）、liked-dock.png；窗口使用独立临时 Core 与本地静音音频。
- run-playlist-media-revision.log 再次记录 favorite:true/favorite_enabled:true、Song Alpha、paused、has_artwork:true。
- 未完成：展开“加入歌单”子菜单的真实窗口截图；行为已由 Headless 覆盖，未使用 GUI 自动化补图。
- 待人工：控制中心 favorite、真实 Cmd+Q、展开菜单外观；远端在线刷新仍按计划留 M4，无新增业务架构决策。

## 2026-11-10 M3-02：历史、本地统计、日志与通知收尾

- 基于已有提交 8b045ac 补齐接线与本机证据；本次不提交，未修改 Rex、参考项目、NSH 或依赖源码。
- 复核 history/stats/logs 存储与 Controller 驱动：历史 500/歌单 100/搜索 30、有效连续播放 ≥5 秒、事件上限 5000、app/plugin 各 1000 条、detail 8000 字符、脱敏与会话轮转。
- API：core.history 的 list/query/setQuery/playSource/recordPlaylist/search/remove/clear；core.stats 的 aggregate/clearStats；core.logs 的 page/copyText/exportLogs/clearLogs；core.notifications 的 notices/clearNotice；onChange 均返回 Stop。
- 补齐历史来源筛选的歌单来源；参考 JSON fixture 读取通过，沿用 sourceId/playedAt/listenedSec/tzOffsetMin/eventId、coverHeaders 与未知 trackCount 省略语义，新增 schemaVersion。
- 通知入口改为顶栏“更多 → 通知中心…”，原生“视图”菜单也可打开；与设置/命令面板互斥。设置“其他”已有日志查看器和关于；损坏设置告警接入日志/通知。
- 图表使用有界 TSX View，聚合留 Rust；热图改为周一到周日的 12 周网格，补星期、强度图例与数值 tooltip；24 小时柱底统一对齐。fixture 为 56 次模拟聆听，分布于 28 天和多个小时。
- Dock/正在播放共用的 like_button 已由原提交支持实心/描边切换，本次截图确认；通知及三处清空确认采用现有 self-center/mt-24 弹窗位置。
- Activity.test.tsx 移至 src/pages/fixtures/；共享 fixture 保持在 src/pages/fixtures/ActivityFixture.tsx。根目录四个 Preview 和 ActivityFixture 保留三行薄入口，新增 DockLikedPreview.tsx。
- 归位限制实测：NSH 0.4.0 对嵌套 TSX 入口未加载根 nsh.json，导致真实 host 缺 haiku/native、Headless 缺 fs 权限；恢复根薄入口、测试引用 ../../../ActivityFixture.tsx 后解决，未复制第二份配置。
- 验收 1：cargo test --manifest-path native/Cargo.toml --workspace，238 通过、1 忽略；证据 .artifacts/m3-02/cargo-test-workspace.log。
- 验收 2：nsh check、nsh build -v 通过；nsh test 最终 21/21（8 Rust、13 Headless）；日志 nsh-check.log、nsh-build.log、nsh-test-repair2.log 均在 .artifacts/m3-02/。
- 首轮 nsh test 的 Activity 因嵌套入口权限失败；修复后 Activity 通过，但既存 NowPlaying 布局测试异步 measure 丢失首帧超时。第二种修复仅 cfg(test) 改读 on_layout payload，保持 top=0/bottom=688 断言；最后全绿。前两轮日志保留为 nsh-test.log、nsh-test-repair1.log。
- 验收 3：五个根预览分别 nsh run --target macos -v，均为独立临时 Core、浅色；日志 nsh-run-{history-repair1,stats,logs,notifications,dock-liked}.log；真实截图已查看。
- 五张截图：history-light.png、stats-light.png（小时分布/热图）、logs-light.png（可见脱敏）、notifications-light.png、dock-liked-light.png，均在 .artifacts/m3-02/。
- 捕获前检查屏幕未锁定，仅 CGWindow/CGSession 只读查询及 screencapture；证据 screen-lock.log、capture-*.log。各预览截图后 Ctrl+C 停止，其退出 1 为人为结束，不作构建失败。
- 正常入口构建/最终测试与预览只出现已允许的 block 0.1.6 / __eh_frame 提示；最初嵌套入口的 unknown classes 与模块错误随根入口恢复消失，原失败日志 nsh-run-history.log 保留。
- 最后小时标签高度、弹窗定位是纯样式调整，经真实 host 热更新/截图检查；未重复 check/build/test。未新增探针、GUI 自动化、release/clippy、汇总 JSON 或 hash 清单。
- 偏离/未覆盖：参考目录 ~/Project/GPUI/haiku 在本机不存在，改只读 ~/Project/haiku-gpuix 核对对应行为；热图当前显示最近 12 周，不提供全年选择；远程/M4/M5 内容未做。
- 待人工确认：真实鼠标/键盘交互、原生保存选择器与实际剪贴板、长期真实播放统计；关于页如实标明应用许可证未声明，需审阅者确定许可证。

## 2026-11-10 M4-01：网络播放与来源详情（fixture 验收完成，待审阅）

- 主仓库保留改动，未提交 git；未改 Rex、参考项目、NSH 或依赖源码；既有 MILESTONES/任务文件改动保留。
- 第 0 步：通知/日志采用本地可读时间、整秒；通知按钮可见“清除”，序号保留于无障碍标签；设置只留按钮打开日志 Dialog；12 周热图铺满内容宽度。
- 播放采用已批准 Candidate：Local(PathBuf) / Network{url,headers,format,quality,expires_at}；Controller 注入 TrackResolver/Clock，app 经 SourceApi 获取地址。
- 本地曲库只走 Local，失败不回退；同音质完整缓存先于 streams；候选过期前复用，Expired/到期有界刷新，不支持格式打开前跳过。
- 网络失败 1/2/4 秒最多三次；暂停/停止/seek/新队列取消；连续正常进度 30 秒清零；提前 EOF ≥5 秒失效缓存并从位置恢复，预加载共用解析路径。
- 媒体缓存键为 (完整 EntityRef, quality)，沿用设置 audioQuality（默认 high）；M5 下载文件提供者未实现。
- 来源封面经带请求头的有界下载进入媒体中心；来源曲目沿用历史/统计接线。
- 来源详情覆盖 playlist/album/artist/toplist，含分页、热门曲目/专辑、播放/随机、原生 TrackList 菜单、收藏镜像和歌单历史；异步结果校验页面身份。
- 侧栏显示已启用 library 来源；按 cards 分组为虚拟封面网格/TrackList，错误提供重试与去设置；实机发现单项标题横向拉伸，补 items-start 与封面左对齐。
- core.sources 新增 detail/artistAlbums/browse/favoritePlaylist/trackSource/releaseTracks，静态 reference/isRemote；core.player.play 接受 SDK 来源曲目；监听返回 Stop。
- 回归夹具位于 src/pages/fixtures 与 native/app/tests/fixtures/network-plugin；根 Network*Preview 为薄入口，以加载根 nsh.json/native/theme；全部独立临时根、loopback，无真实上游。
- 验收 1：workspace 初轮编译失败（测试 LocalResolver 路径），修正后 243 通过、1 忽略；日志 .artifacts/m4-01/cargo-test[-repair1].log。
- 五个新增 Rust 用例覆盖候选回退/容器跳过/地址复用、Expired/到期、同音质缓存/本地失败、重试计时/取消、30 秒清零/提前 EOF。
- 验收 2：nsh test -v 最终 22/22（8 Rust、14 Headless），nsh-test-review-repair.log；已覆盖来源浏览→错误重试→详情分页/播放→歌手专辑→收藏侧栏→歌单历史回访及第 0 步。
- 前三轮 21/22：错误注入/析构、错误暂停标签、侧栏与历史同名选择器歧义；原始 nsh-test[-repair1/-repair2].log 全部保留。
- 审阅者明确授权超出修复上限继续；历史 List 新增 history-playlist-list，最后点击限定其后代按钮，未用 nth；没有改业务导航。
- 授权续跑后 nsh check/build 各一次通过（nsh-{check,build}-final.log）；随后仅改卡片对齐 class，并再各跑一次通过（nsh-{check,build}-visual-final.log）；样式调整后未重复整套测试。
- 验收 3：五个根预览分别真实 nsh run --target macos -v，独立临时根、浅色；nsh-run-{library,album,artist,dock,stats}.log，截图均已查看。
- 五张截图均在 .artifacts/m4-01/：library-light.png、album-light.png、artist-light.png、dock-light.png、stats-light.png（小时分布与全宽 12 周热图）。
- nsh-run-dock.log：loopback 220Hz 非静音 FLAC、音量 0.15，真实 macOS 播放进度由 0 推进至 23 秒、buffered=1.000；媒体快照 has_artwork=true，截图可见播放中暂停按钮与实心喜欢。
- 截图仅使用 CGSession/CGWindow 只读查询与 screencapture，capture-*.log 记录屏幕未锁定；临时捕获脚本已删除，无 GUI 自动化。
- 各预览截图后 Ctrl+C 停止；退出 1/随后开发连接关闭不作构建失败；仅出现约定允许的 block 0.1.6 / __eh_frame 提示。
- 未覆盖/待人工：真实缓存落盘后二次命中与实机故障恢复、Subsonic/飞牛/QQ 播放、实际扬声器听感、真实远端收藏刷新及原生媒体操作；未访问真实上游。
- 偏离：Rust Controller 直接注入 SourceApi 适配器，未复用参考版 TS/RPC 播放控制层；没有新增待定架构决策；F04/F05 保持原状。

## 2026-11-10 M4-02：搜索与命令面板（fixture 验收完成，待审阅）

- 主仓库保留改动，未提交 git，未改 Rex、参考项目、NSH 或依赖源码；保留审阅者的未跟踪任务文件。
- 第 0 步：来源卡片标题去掉横向按钮内边距并与封面左对齐；网格按行数收紧高度、分组间距减小；侧栏库来源改用 folder 图标。
- 来源详情改为可滚动内容，短列表按曲目数收拢，分页操作跟随列表；长列表保留最多 8 行的原生虚拟列表视口。
- 搜索页保留挂载，提供 track/album/artist/playlist/program 五种类型，只显示所选来源支持项；已启用来源筛选，searchSourceId 沿用设置的未知字段持久化机制保存上次选择。
- 查询结果使用原生 TrackList/封面网格；支持游标分页、加载/空/错误重试、历史提交/单项删除/确认清空。
- 节目来源有 program.get 时进入通用详情，否则显示“此来源暂不提供节目详情”提示，不尝试猜测播放地址。
- 首屏与分页结果用完整 (source,kind,query) 身份、递增 epoch、页面 active/销毁状态共同检查；面板用查询 epoch 和 open 检查丢弃旧结果。
- 命令面板保留导航，使用现有原生 Command 上下选择/回车；280ms 防抖，多来源并发、每源预览至多 3 个歌单和 3 首曲目，提供搜索页查看全部。
- 新增 app 原生 CommandKeyboard，焦点处理完成后注册 Escape 关闭，失焦/关闭时解绑；覆盖 Command 默认“先清空再关闭”，单次 Escape 关闭非空面板。
- 面板曲目起播传入对应来源完整第一页 tracks 与所选 index，未截断为 3 首；监听仍返回 Stop，搜索页历史订阅随组件释放。
- API：core.sources.search(source,kind,query,cursor?)→Page；searchPreview(query)→来源分组数组，含 source/title/tracks/playlists/errors。
- SourceApi 新增 search_tracks，保持 SDK Track 的 source/extra/album；预览按来源及 track/playlist 分别隔离错误，不新增依赖。
- 夹具：扩展 network-plugin，并增加 track-only 的 search-lite-plugin；SearchFixture 使用 openIsolated、loopback、音量 0，无真实上游。
- 根 Search*Preview 为加载根 nsh.json/native/theme 的薄入口，共享夹具与两个新 Headless 用例放在 src/pages/fixtures/。
- 验收 1：cargo test --manifest-path native/Cargo.toml --workspace 首轮通过，244 passed/1 ignored；新增屏障用例验证来源并发与错误隔离，沿用既有搜索历史去重测试。
- 验收 2：nsh test -v 最终 25/25（9 Rust、16 Headless）；证据 .artifacts/m4-02/nsh-test-keyboard-final.log。
- 两条搜索流程已通过：搜索起播第 2 首/队列 5 首、加载至 7 项、切 tab/来源、慢请求后新查询、历史单项删除/确认清空。
- 面板覆盖 280ms 防抖（短词未调用来源）、预览仅 3 首但起播队列 5 首、旧结果丢弃、查看全部和非空查询单次 Escape 关闭。
- 审阅者授权超出修复上限后，替换输入统一改为 app.clear(selector) 后 app.type(selector,text)，去掉 press 聚焦/全选退格；搜索页流程随即通过。
- 随后面板测试 24/25，原生 Command 的 Escape 优先于 JS 绑定且先清空查询；仅加 Dialog.onCancel 仍未关闭，最终用 app 原生按键适配解决，未弱化测试。
- 早期失败日志 nsh-test{,-repair1,-repair2,-review-repair,-review-final}.log 均保留；nsh check 初轮 app.clock 可空错误及修复日志亦保留。
- 全绿后 nsh check、nsh build -v 各一次通过，日志 nsh-check-final.log、nsh-build-final.log；之后仅更新 STATUS，没有再改代码。
- 验收 3：五个根预览分别真实 nsh run --target macos -v，独立临时根、浅色、loopback；日志 nsh-run-{tracks,albums,history,command,library}.log。
- 五张截图均已查看：search-tracks-light.png、search-albums-light.png、search-history-light.png、command-preview-light.png、library-light.png。
- 所有证据位于 .artifacts/m4-02/；capture-{tracks,albums,history,command,library}.log 记录屏幕未锁定及窗口；仅 CGSession/CGWindow 只读查询与 screencapture。
- 各预览截图后 Ctrl+C 停止，其退出 1 为人为结束；仅有允许的 block 0.1.6 / __eh_frame 提示；临时捕获脚本已删除，无 GUI 自动化或汇总 JSON。
- 已接受的做法：输入变化即时查询，显式搜索/回车/历史选择才写历史；旧 native 请求可完成但结果被丢弃；节目无详情选提示方案；根薄入口沿用既有约定。
- 未覆盖/待人工：真实来源搜索、最小窗口长列表布局与实机键盘体验；未访问真实上游，无新增待定架构决策；保留改动并停止，未提交 git。


## 2026-11-11 M4-03：推荐、分类与电台（发现页通过，完整回归剩一项）

- 在主仓库完成 625319a 的验收收尾；本轮未提交 git，未修改审阅者 HANDOFF、参考项目、NSH、依赖或 Rex。
- 第 0 步：命令面板曲目“标题 · 歌手”、来源曲库分组收紧；推荐/分类/电台沿用既有共享组件与 token。
- 推荐按 home 能力筛选已启用来源，保存 homeSourceId；tracks 原生 TrackList、实体横向货架、Entry/more 跳转、来源错误隔离。
- 修复 SourceContent.tsx：TrackList TSX 宿主补 flex-1/min-h-0，使用货架既有定高，原生行实际显示并可双击起播。
- 修复 DiscoveryPages.tsx：频道 resource 等描述符及 radio.channels 能力就绪再请求，避免初始空快照返回空数组后不重载。
- Discovery 回归保留原曲目选择器/双击/起播断言；增加异步行就绪等待与频道按钮可见断言。
- 分类 tag→四列歌单网格，支持来源 sorts、分页和详情；QQ 仅声明 hot，不臆造 new；私人 FM 使用真正 undefined。
- 导出 sources.home(source,refresh)→{shelves,cached}；categories(source)→{groups,sorts?}；categoryPlaylists(source,id,sort|null,cursor|null)→Page；radioChannels(source)→频道数组。
- 导出 player.startRadio(source,channel|null)；null 在宿主转为 SDK undefined，普通频道拒绝空字符串。
- 缓存由 Rust HomeCache 持有：<data>/cache/home/<来源哈希>.json，{version:1,saved_at:Unix秒,home}，原子写、7 天、最多40×60、仅首屏。
- 缓存命中先返回，页面随后 home(true) 刷新；刷新保留旧 radio Entry，隐藏空货架，离线缓存展示与刷新错误并存。
- 续歌在 Rust Controller：顺序队列剩余≤3首时单次请求 count=10，以 EntityRef 排除已听/已排队曲目；切换/清空/停止取消，session token 拒绝迟到结果。
- 无新曲或来源失败停止自动重试，由重新选频道恢复；仅所选来源，不混合来源 FM。相对参考版以 Rust 文件缓存/Controller 替代 TS 服务。
- 审阅者已验收 workspace 247 passed/1 ignored、nsh check/build 通过；沿用 cargo-test-workspace-final.log、nsh-check-final2.log、nsh-build-final2.log，本轮未重跑。
- 锁屏导致 Player 停滞已由审阅者解锁复核确认；本轮 Player 单文件2/2、Discovery单文件3/3通过，日志 player-layout-repair.log、nsh-test-discovery-repair.log。
- 首次完整收尾测试28/29：货架标题早于异步原生行就绪，立即press失败；增加原选择器waitFor，保留双击/队列断言；失败日志 nsh-test-acceptance-final.log。
- 最终完整回归28/29：Discovery三项及Player均通过；Search命令面板用例最后Escape后仍显示默认命令，等待关闭5秒超时；根因未定位，日志 nsh-test-acceptance-repair.log。
- 五张真实浅色截图齐：home.png、categories-tags.png、categories-playlists.png、radio.png、command-tracks.png；已逐张查看，运行日志 run-*-final*.log 等保留。
- 所有证据 .artifacts/m4-03/；预览与测试使用 openIsolated/loopback，无真实上游；截图无 GUI 自动化。
- 待人工：真实来源首页/分类分页/电台连续播放与听感。小项：直接路由分类标题显示原始ID（如cn），tag点击显示中文，未做名称反查。
- 审阅者定位 Player 失败：service::set_track 中新增的 cancel_preview() 调用导致切歌后本地歌词不加载；preview 已监听曲目 cancel，该调用冗余，审阅者删除后完整 nsh test 33/33、workspace 252 通过（review-*.log）。遗留：Paxsenix 专用评分/401令牌重取/content结构兜底、Kugou署名裁剪。

## 2026-11-11 M4-04：远端歌词与界面续作（已验收）

- 主仓库内工作，未提交git；未修改参考版、NSH、依赖源码或其他specs。
- 已验收核心：haiku-lyrics::{formats,providers,service}；app lyrics.rs适配本地/插件，presentation推送既有Rust歌词视图及Cursor。
- 来源链：手动覆盖→指纹缓存→本地文件→track.lyrics→排序/禁用后的提供方；单源8秒/总计25秒，纯文本兜底，逐字优先，generation防串与取消。
- 缓存 <data>/lyrics/<SHA256(JSON.stringify([sourceId,id]))>.json，原子写；{result,source,manual,fingerprint,savedAt}兼容参考版，TTML/YRC规范化增强LRC+译文。
- fingerprint为JSON.stringify([title,artists,album,duration])；自动遵守媒体缓存开关，手动始终保存。
- core.lyrics保留snapshot/onChange→Stop/search/resolve/adopt/resetToAuto；新增searchReport→{candidates,sourceErrors}、cancelPreview，带导出注释。
- 搜索保留成功候选并逐源记录失败；关闭/换候选/切歌取消独立预览，不清空播放歌词，Rust回归证明drop旧future及失效ticket不发请求。
- LyricsSearch.tsx：关键词/歌手、来源筛选、评分/推荐/逐字/翻译、虚拟候选列表、预览、采用及恢复自动，带陈旧结果保护。
- 按审阅决定保留ghost“查找歌词…”直达按钮，nsh_base::button发送FindLyrics，“恢复自动”位于对话框。
- 按审阅结论撤回条件挂载，AppShell恢复常驻NowPlaying，open={nowPlaying() && !lyricsSearch()}，保留FindLyrics/OpenChange事件。
- RemoteLyrics夹具移除initialNowPlaying；Headless从Dock“显示歌词”打开，覆盖Escape关闭→Dock重开→关闭按钮，以及查找→预览→采用→恢复自动。
- 已知夹具问题：initialNowPlaying=true初始即打开时组件事件不到达AppShell；审阅review-diag2.log确认，按决定本轮不修；正常Dock打开路径通过。
- LyricsSettings.tsx：自动查找、提供方启停/排序、自定义HTTP名称/URL/响应类型/JSON路径/headers增删改与校验。
- 设置Headless验证禁用后0候选/0请求及自定义来源增删改；所有夹具openIsolated，网络只用loopback。
- 第0步：分类显示“华语”而非cn、电台未选频道secondary、推荐横向货架gap收紧；Discovery回归通过，分类/电台已截图检查。
- providers.rs原8条超过160字符的行已拆分并rustfmt；未改变提供方请求语义。
- 前轮workspace252 passed/1 ignored：cargo-test-workspace-ui.log；此前check修复后通过、build通过，本轮未重复。
- 本轮三个定向各一次：RemoteLyrics3/3、Player1/2、Search2/2；日志nsh-test-resident-{remote-lyrics,player,search}.log。
- 本轮完整nsh test一次32/33：nsh-test-resident-full.log；RemoteLyrics与Search含Escape均通过，未修改Player/Search断言。
- 唯一失败：Player暂停Beta后覆盖层已打开，但显示“暂无本地歌词”，等待label:Beta middle五秒超时；恢复常驻后定向与完整仍复现，根因未定位。
- 旧失败与诊断保留；nsh-test-final.log的31/33属于已撤回条件挂载版本，不代表当前结果。
- 本轮定向及完整测试前IOConsoleLocked=No：screen-lock-before-resident-{tests,full}.log；仅已知host/工具链警告。
- 真实浅色主题截图5张均逐张检查：remote-word-lyrics.png、lyrics-search.png、lyrics-settings.png、category-playlists.png、radio.png。
- NowPlaying图为暂停恢复的远端逐字歌词，区域沿用封面派生暗色；查找图含候选/预览/逐源错误，设置图含启停/排序/HTTP来源。
- 截图与日志均在.artifacts/m4-04/；本轮仅调整挂载/打开路径，无视觉改动，复查正在播放及查找原图后保留五张截图；预览进程均已关闭。
- 提供方覆盖沿用首部分：Kugou KRC/LRC、LRCLIB、Better Lyrics TTML、LyricsPlus、Paxsenix、HTTP模板/headers/JSON点路径。
- 未完成：完整nsh test全绿（上述Player一项失败）；Paxsenix专用评分/401令牌重取/content结构兜底、Kugou署名裁剪仍遗留。
- 真实上游可用性与返回格式变化需人工确认；无新增架构待决事项。


## 2026-11-11 M4-05：远端收藏刷新、刮削任务与审阅（已验收）

- 主仓库内工作，未提交git；未修改参考版、NSH、依赖源码或其他specs，无新增依赖。
- favorite_refresh.rs共享按EntityRef索引的完整分页future；beginFavoriteRefresh预留lease，refreshFavorite等待结果，cancelFavoriteRefresh释放；最后一个消费者离开才取消上游。
- collections::refresh_version/refresh_favorite在同一锁内比较版本并替换；修改/import/删除使旧请求失效，取消与提交串行，删除后迟到结果不复活收藏。
- 收藏详情先显示离线快照，后台最多100页、游标循环与超限报错；完整成功后原子写入，refreshedAt严格递增，失败提示“刷新失败，显示离线内容”。
- CollectionPage与SourceDetailPage均接入刷新/离页取消；取消收藏需确认，只删本地镜像。
- 权威歌单仍为<data>/playlists.loro，镜像与(refreshedAt,version)合并沿用参考版，无格式变更。
- scrape_service.rs复用插件Scraper的4并发/8秒、跨源合并与verdict；批量进度、取消保留已完成、自动采用/待审/跳过、手动关键词重搜。
- 音乐来源search.track可参与候选；同分合并和排序遵守配置来源优先级，不依赖请求完成顺序。
- core.scrape：start(ids,{mode:missing|replace})、cancel、jobs、review、accept(id,candidateIndex)、skip(id)、undo(id)、retry(id,keyword)、onChange→Stop，均带导出注释。
- 应用到library/overrides.json的稀疏覆盖，保存previousOverride/appliedOverride；撤销拒绝覆盖后续人工修改；Core.close取消并等待任务退出。
- 审阅持久化为<data>/scrape_review.json数组，字段沿用参考版；磁盘track转sourceId/字符串album/字符串artists，previousOverride元数据转extra；新增冲突保护字段由旧版忽略。
- 恢复searching为pending；参考版review读写转换与采用后重开/撤销测试通过；本服务不调用write_tags，文件写回仍走既有TrackEditor明确确认路径。
- LibraryPage单曲菜单/多选工具栏“刮削…”→模式确认→进度与可滚动审阅；候选展示评分/来源/字段差异，自动采用项标注并可撤销。
- 设置位置：存储→元数据刮削；使用scrapersDisabled/scraperOrder/scrapeUseMusicPlugins，新增scrapeMode保存默认应用模式。
- 偏离：音乐来源候选默认关闭（参考版默认开启，可在设置启用）；审阅采用与自动结果在同一列表以状态区分。
- 第0步：歌词逐源错误改为中文解析/超时/不可用提示，原始错误写入app日志lyrics scope。
- §1相关Rust4/4：cargo-test-favorite-refresh-repair2.log；初次编译/API与夹具激活修复日志保留。
- §2相关Rust3/3：cargo-test-scrape-service-repair1.log；初次编译错误修复日志保留。
- nsh check通过：nsh-check.log；nsh build通过：nsh-build.log；仅已知工具链/host警告。
- workspace一次260 passed/1 ignored：cargo-test-workspace.log；完整nsh test一次43/43：nsh-test-full.log。
- 新增Headless三项3/3：nsh-test-favorite-scrape.log，覆盖收藏完整刷新/失败快照/取消确认及批量采用、跳过、自动撤销。
- 上述全量后收尾补充参考审阅格式适配与同分来源优先级；仅跑相关Rust：scrape_service4/4、haiku-plugins scrape4/4（cargo-test-scrape-{review-compat,provider-order}.log），未重复全量。
- 设置截图发现外层默认横排，补flex-col并重拍；此布局类修正通过真实窗口复查，未重复构建/全量测试。
- 测试/截图前IOConsoleLocked=No；所有夹具openIsolated，来源使用显式fixture与loopback，无真实上游请求。
- 四张浅色截图已逐张检查：favorite-refresh.png、scrape-progress.png（2/3运行中）、scrape-review.png（候选/差异）、scrape-settings.png。
- 证据均在.artifacts/m4-05/；run-*.log保留，所有本轮预览已关闭；git diff --check通过。
- 未补齐参考版高级关键词清洗/文件名回退、专辑艺术家/碟号候选字段、刮削封面下载到本地缓存；当前封面保存候选URL。
- 未覆盖真实上游与原生写回实机交互；重开Core的审阅UI未单独做Headless（Rust服务/曲库重开已测）。无新增架构待决事项。
- 审阅者核对：workspace 日志 260 通过/1 忽略，完整 nsh test 43/43，check/build 无新警告；四张截图已审阅。UI 小项留后续：审阅列表项居中未左对齐、字段差异“年份”行空白、来源显示内部 ID（network.fixture#…）而非名称。

## 2026-11-11 M5-01：下载 Rust 服务与 Player 回归修订（第一部分已验收）

- 仅在主仓库修改，未提交 git。§1 主体已实现；本轮优先修复审阅确认的 Player 回归，未进入 §2/截图；§3 留 M5-02。
- 下载文件 `native/app/src/{download_service,downloads,download_service_tests}.rs`；Core getter/关闭/退出接线；Cargo 仅增加已有 workspace 依赖。
- key=`source + NUL + id`；每次尝试 UUID；并发 1–5；暂停后 worker 释放 .part 才可继续；旧事件隔离、2s/8s 自动重试、移除/清理/校验缺失。
- streams 保留 headers；从当前 Core 根复用同音质完整缓存；Range 与响应起点校验；不同候选切换清理残片；嗅探容器、打标签并原子发布音频，再写侧车。
- 歌词走 M4-04 来源链，下载任务使用独立 authority，共享手选/缓存；支持译文、嵌入/侧车，不改变播放器 generation。
- `core.downloads`：enqueue（SDK 单曲/数组、远端 playlist/album/toplist EntityRef）、pause/resume/retry/cancel/remove(withFile)、clearCompleted/retryFailed/snapshot/reveal/verify/onChange→Stop。
- 持久化 `<data>/haiku_state/downloads.json` 数组；参考字段与 sourceId/provider、字符串 album/artists；保留未知字段，haikuTrack 扩展保留 SDK 实体身份；进行中重开回 queued。
- 设置沿用 downloadDir/Quality/Naming/Layout/Concurrency、downloadCoverMode/Hd、downloadLyricsMode/Translation、downloadReuseMediaCache、downloadWifiOnly（桌面忽略）；空目录按 ~/Music/Haiku 解析。
- 初版 workspace 修复后 266 通过/1 忽略，下载新增 5/5；nsh check/build 通过。初版全量 Player 失败已由审阅者与基线对照确认为回归，撤回偶发判断。
- 根因：歌词 effect 订阅整个播放状态，重跑先取消待完成的本地读取；随后旧 key 相同而直接返回，未重新加载。新增下载初始化/调度改变时序后暴露此生命周期错误。
- `presentation.rs` 改为 computed 比较曲目/generation/歌词设置；只有实际请求变化才重跑 effect。播放/暂停、下载设置变化保留待完成读取，切歌仍取消旧请求。
- 下载记录读取合并进 Core 原有 background 初始化；启动 pump 在下载 runtime 调度。移除全设置 effect；SettingsService 在现有更新工作中仅当下载并发改变时 pump。
- 新增 `presentation_tests.rs`：Core.openIsolated + 受控本地读取，覆盖读取挂起时暂停/修改下载设置、读取完成、再切歌；无负载等待或超时放宽，回归通过。
- 本轮 `nsh test Player.test.tsx` 2/2；完整测试首次及 repair1 因新增测试 Headless API/引用类型编译错误失败，已修正；最终 repair2 为 49/50，Player、下载 5 项及新增歌词回归均通过。
- 最终唯一失败：Search.test.tsx 的命令面板用例在 Escape 后 5 秒仍未关闭；未修改断言，未扩大到 Search 修复，完整套件仍未全绿。
- 每次 nsh test 前 `IOConsoleLocked = No`。verbose 日志确认 runner warning 为已知 __eh_frame；另有已接受 cargo target-dir 与 block future-incompat 提示。
- 本轮证据 `.artifacts/m5-01/nsh-test-player-regression.log`、`nsh-test-regression{,-repair1,-repair2}.log`、`screen-lock-{player,full}-regression*.log`；初版 workspace/check/build 日志保留。
- 未完成：§2 下载菜单/多选/详情入口、下载页、下载设置与对应 Headless、两张浅色截图；§3 自动歌单/跟随移除及目录迁移。
- §1 边界沿用：固定尺寸封面尚不能重取高清；超长 Unicode 路径、损坏下载记录恢复未专测；真实上游/目录权限/Finder reveal 待人工确认。
- 调度权威在 app Rust，复用 haiku-services 搬运器；本地歌单可传 tracks 数组。无新增架构决定；保留 Search 全量失败供审阅，不以此前通过代替最终结论。
- 审阅者：回归根因为歌词 effect 依赖整份设置，下载设置写入导致 effect 重跑并取消挂起的本地歌词读取；改为 computed 比较请求输入。复跑完整 nsh test 50/50（nsh-test-reviewer2.log）；基线对照见 nsh-test-baseline-bcf4fa5.log。
