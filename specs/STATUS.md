# 实施与验收记录

状态只有 `未实现 / 部分实现 / 已验证`。没有证据不能标为“已验证”。每次更新在下方追加一个带日期的小节，说明改动、验证命令与结果、未覆盖的范围。

## 技术验证（M0）

| # | 验证项 | 结论 | 证据 |
|---|---|---|---|
| S1 | nsh CLI 与 mixed 模板 | 已验证（当前完整应用重新通过） | 2026-11-09 M0-02：移除 CJRT 后，当前应用 `nsh check/build/test` 通过，build 无 warn；开发窗口与打包版均显示 `Plugin echo(21) = 42`。证据 `.artifacts/m0-02/nsh-{check,build,test,run,package}.log`、`run-window.png`、`package-window.png`；开发 host 的独立链接器提示见下文 |
| S2 | 插件运行时与 NSH 共用一份 QuickJS | 已验证（M0 最小内核，macOS arm64） | `haiku-jsrt` 与 NSH 使用同一组 `quickjs-jit/stdlib 0.14.1`；仅一份 sys，无 CJRT/rquickjs-sys。运行时测试 11/11；16 worker 共 1000 次调用全部正确，同时有 173 次 UI JS 计算 tick，双向全局变量隔离通过；release 打包无 warn，`_JS_NewRuntime` 一个定义。证据 `.artifacts/m0-02/{jsrt-test,nsh-test,nsh-package}.log`、`cargo-tree-duplicates.log`、`nm-release.txt` 及两张窗口截图 |
| S3 | NSH webview 读取 HttpOnly Cookie | 未开始 | |
| S4 | NSStatusItem / NSPanel / MPRemoteCommandCenter | 部分实现（仅迁移，未接入 NSH） | M0-03：haiku-macos 编译、6 个纯逻辑测试通过；`.artifacts/m0-03/{cargo-test,cargo-clippy}.log`。真实原生交互未覆盖 |
| S5 | 后台播放与窗口恢复 | 未开始 | |
| S6 | virtual_list 1 万行 | 未开始 | |
| S7 | 原生玻璃视图嵌入 | 未开始 | |
| S8 | 第二个窗口共用 Core | 未开始 | |

## 功能要求

| 要求 | 状态 | 实施文件 | 验证 | 缺口 |
|---|---|---|---|---|
| F01 桌面外壳与导航 | 未实现 | | | |
| F02 插件包与来源生命周期 | 未实现 | | | |
| F03 来源认证与用户内容 | 未实现 | | | |
| F04 推荐、发现与分类 | 未实现 | | | |
| F05 搜索 | 未实现 | | | |
| F06 队列与播放控制 | 未实现 | | | |
| F07 音效与响度 | 未实现 | | | |
| F08 歌词与正在播放 | 未实现 | | | |
| F09 桌面集成 | 未实现 | | | |
| F10 本地曲库与刮削 | 未实现 | | | |
| F11 喜欢、歌单与远端收藏 | 未实现 | | | |
| F12 历史与听歌统计 | 未实现 | | | |
| F13 下载与媒体缓存 | 未实现 | | | |
| F14 Haiku 账号 | 未实现 | | | |
| F15 同步与备份 | 未实现 | | | |
| F16 设置、通知与日志 | 未实现 | | | |

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
