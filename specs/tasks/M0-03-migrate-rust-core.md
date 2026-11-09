# 任务 M0-03：git 基线提交 + 迁移参考版 Rust 核心

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md` 的全部约定。完成后停下汇报。

## 步骤 0：基线提交（先做）

1. 确认 `.gitignore` 覆盖 `.nsh/`、`target/`、`native/target/`、`.artifacts/`、`.DS_Store`；`git status` 中不能出现构建产物或日志。
2. 在 `main` 分支（如果当前默认分支名不是 main，改名为 main）提交当前全部内容，提交信息：`M0: project skeleton, haiku-jsrt minimal runtime (S1, S2)`。
3. 本任务完成后再做第二次提交（见“验收”）。除这两次之外，不要再提交。

## 迁移范围

来源：`~/Project/haiku-gpuix/native/`（只读）。**只迁移代码，不改行为**：保留原有逻辑和单元测试，只做让它在新 crate 中编译所必需的修改（模块路径、`pub` 可见性、依赖声明）。不重构，不做格式以外的整理。

| 源文件 | 目标 crate | 说明 |
|---|---|---|
| `src/player.rs` `src/effects.rs` `src/loudness.rs` `src/media_cache.rs` | `haiku-audio` | player 依赖另外三个模块，放在同一个 crate |
| `src/api/playback.rs` | `haiku-audio` | 作为对外的引擎接口；去掉其中 FRB/RPC 专用的类型转换 |
| `src/lyrics.rs` | `haiku-lyrics` | |
| `src/playlist_core.rs`、`src/api/playlist_sync.rs` | `haiku-store`（模块 `collections`） | |
| `src/local_scan.rs`、`src/api/local_library.rs` | `haiku-library` | |
| `src/downloader.rs`、`src/api/downloads.rs` | `haiku-services`（模块 `downloads`） | |
| `crates/cover-palette` | `haiku-palette` | 整个 crate 替换空壳，包名改为 haiku-palette |
| `src/menubar.rs` `src/desktop_lyrics.rs` `src/system.rs` `src/shortcuts.rs`，`src/api/menubar.rs` `src/api/desktop_lyrics.rs`，`native/assets/menubar/` | `haiku-macos` | 整个 crate 放在 `#[cfg(target_os = "macos")]` 下；暂时不接入应用，能编译、测试能通过即可 |

**不迁移**：`rpc.rs`、`rpc_dispatch.rs`、`bridge.rs`、`main.rs`、`menubar_ffi.rs`、`api/music_host.rs`（CJRT）、`api/mod.rs`、`tools/`、`tests/`（RPC 与 GPUIX 的冒烟脚本）、`examples/`、`*.m` 文件与 `build.rs`（M0-04 按需引入）。

依赖：
- 版本参照参考版的 `native/Cargo.toml` 和 `Cargo.lock`，在 `native/Cargo.toml` 的 `[workspace.dependencies]` 中集中声明，各 crate 使用 `workspace = true`。参考版里的注释（为什么开启某个 feature 等）一并保留。
- 去掉 `flutter_rust_bridge`、`cjrt-*`，以及 Android 专用依赖。
- 如果某个依赖与 NSH 依赖图冲突（例如 tokio、reqwest、RustCrypto 的版本），按“最小修改”处理，并在汇报中说明。
- 迁移进来的 crate 之间只能单向依赖（ARCHITECTURE.md §2）；如果原代码存在反向引用，在汇报中列出，不要自行重新设计。

`native/app` 本任务不接入这些 crate（只保留 M0-02 的内容），避免把迁移问题和应用集成问题混在一起。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：迁移进来的原有单元测试全部通过。汇报每个 crate 迁移前后的测试数量（参考值：player 38、local_scan 18、lyrics 15、playlist_core 15、effects 13、downloader 10、loudness 9、desktop_lyrics 4、media_cache 3、local_library 3、shortcuts 2、cover-palette 2）。如果有测试依赖音频设备、网络或 GUI 而无法在命令行运行，标记 `#[ignore]` 并附上原因，不要删除。
2. `cargo clippy --manifest-path native/Cargo.toml --workspace --all-targets`：不新增 error。原有代码的 warning 只统计数量，不修。
3. `nsh check`、`nsh test` 仍然通过（证明应用没有被影响）。
4. 第二次提交：`M0-03: migrate reference Rust core into domain crates`。

不需要：`nsh run`、截图、打包、为迁移的代码新增测试。

## 汇报

STATUS.md 追加“M0-03”小节（不超过 40 行）。最终回复列出：迁移映射是否有偏离、各 crate 的测试数量对照、被 ignore 的测试及原因、依赖冲突及处理方式、发现的反向依赖、两个提交的哈希。
