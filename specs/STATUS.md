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
| F02 插件包与来源生命周期 | 未实现 | | | |
| F03 来源认证与用户内容 | 未实现 | | | |
| F04 推荐、发现与分类 | 未实现 | | | |
| F05 搜索 | 未实现 | | | |
| F06 队列与播放控制 | 部分实现（本地文件与播放界面） | `haiku-playback`、`native/app/src/{core,player_dock,seek_bar}.rs` | M1-04：workspace 183 通过/1 忽略；Headless 10/10 验证 Dock 命令、空队列禁用、拖动预览/单次 seek、设置内控制与实例保留；浅深 Dock、浅色 Settings+Dock、静态正在播放截图通过 | 入场动画未实现，按用户批准的静态方案收尾；网络候选/缓存恢复、统计与 ListeningProgress、真实多曲无缝听感未覆盖 |
| F07 音效与响度 | 已验证（M1 范围，待审阅） | `haiku-store::settings`、`haiku-playback::{backend,loudness_cache}`、`src/shell/PlaybackSettings.tsx` | M1-05：DSP 映射/切歌与预加载保持、测量优劣规则通过；Headless EQ 设置与后端一致；浅色播放设置截图通过 | 真实听感待人工确认；软削波与 8ms 淡入淡出沿用固定保护机制 |
| F08 歌词与正在播放 | 部分实现（本地歌词） | `haiku-lyrics::Cursor`、`native/app/src/{presentation,lyrics_view,now_playing,player_theme}.rs` | M1-04：本地优先级/编码/上限、Cursor、配色版本测试通过；Headless 歌词点行/切歌及布局边界通过；真实日志证明行推进；静态覆盖层封面/歌词/Dock 截图通过 | 入场动画未实现：真实 host 的 Rust animated_value timing 不推进，Headless 正常，原因待后续复核；网络与手动歌词留 M4 |
| F09 桌面集成 | 部分实现（媒体中心接线） | `native/app/src/platform/media.rs`、`haiku-macos::{system,lifecycle}` | M1-05：真实标题/封面及 playback_state 1→2 快照；媒体回调通过 channel 进入 Controller；防息屏与退出清理已接线 | 媒体键/控制中心、关闭隐藏续播、Dock 恢复、Cmd+Q 与防息屏待人工确认；M6 功能未覆盖 |
| F10 本地曲库与刮削 | 部分实现 | `haiku-library/src/service.rs`、`native/app/src/{library,track_source,track_list}.rs`、`src/pages/{LibraryPage,TrackEditor}.tsx` | M1-03 本机实现与自动化验收通过，真实扫描/播放日志、浅深截图已保存，待审阅 | 刮削留 M4；原生双击/选择器交互与听感待人工确认；额外 cargo target-dir 警告见下节 |
| F11 喜欢、歌单与远端收藏 | 未实现 | | | |
| F12 历史与听歌统计 | 未实现 | | | |
| F13 下载与媒体缓存 | 部分实现（媒体缓存） | `haiku-audio::media_cache`、`haiku-playback::backend`、`src/shell/StorageSettings.tsx` | M1-05：容量/LRU/pin/完整性/失败删除/原文件保护测试通过；清理确认 Headless 与浅色截图通过 | 网络缓存真实写入/恢复留 M4，下载留 M5；缓存路径按任务使用 media-cache/ |
| F14 Haiku 账号 | 未实现 | | | |
| F15 同步与备份 | 未实现 | | | |
| F16 设置、通知与日志 | 部分实现（外观/播放/存储设置） | `haiku-store::settings`、`native/app/src/settings.rs`、`src/state/settings.ts`、`src/shell/Settings.tsx` | M1-05：原子合并、未知字段保留、clamp、跨 Core 重启恢复通过；core.settings 文档与 Stop 订阅齐全 | 其他设置分组、通知与持久日志留后续；损坏设置目前备份并输出诊断，尚无 UI 通知 |

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
