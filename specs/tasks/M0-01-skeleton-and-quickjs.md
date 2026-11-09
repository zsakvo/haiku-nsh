# 任务 M0-01：项目骨架 + S1 + S2（QuickJS 双份链接）

执行者：Codex。审阅者：Claude。完成后**停下并汇报**，不要继续做后续任务。

## 先读

1. `specs/SPEC.md`（尤其 §2 决策）、`specs/ARCHITECTURE.md`（§2 仓库结构、§3 边界、§6 技术验证）、`specs/MILESTONES.md` M0。
2. NSH skill：`.agents/skills/nsh/SKILL.md`，以及 `references/mixed.md`、`references/rust.md`、`references/tooling.md`、`references/coding-guides.md` 中的 “Rules for coding agents”。
3. 规则：**不要凭印象写 NSH API**。所有名称都要先在 `.nsh/types` 或 `cargo metadata` 解析出的 nsh-* crate 源码中查到再用。

## 环境

- `nsh 0.4.0`（`~/.local/bin/nsh`），cargo 为 huacnlee/cargo（`cargo -V` 可见），Xcode 已安装，macOS arm64。
- 本目录目前不是 git 仓库。

## A. 骨架（S1）

1. `git init`，加入 `.gitignore`（`.nsh/`、`target/`、`native/target/`、`.DS_Store`）。**不要提交（commit）**，等审阅。
2. 在临时目录执行 `nsh init haiku --template mixed`，把生成的内容按 ARCHITECTURE.md §2 落到本目录：
   - `nsh.json`：`name` 为 `Haiku`，`id` 为 `dev.haiku.nsh`，`"rust": "native/app"`，不设 `root`（mixed 模式，入口 `App.tsx`）。native module 的 specifier 为 `haiku/native`。
   - `native/Cargo.toml` 为 workspace；`native/app` 是 NSH crate（保留模板的 `nsh-core`/`nsh-base` 版本与来源）。
   - 按 §2 建立 `native/crates/` 下各 crate 的**空壳**（`lib.rs` 加一行模块文档即可）：haiku-model、haiku-audio、haiku-lyrics、haiku-store、haiku-plugins、haiku-library、haiku-playback、haiku-services、haiku-palette、haiku-macos。暂不迁移参考版代码。
   - 建立空目录 `src/pages`、`src/ui`、`src/shell`、`src/state`、`sdk/`、`plugins/`、`dev-plugins/`（各放一个 `.gitkeep`）。
   - 注意：模板的 `Cargo.toml` 自带空 `[workspace]`。改成由 `native/Cargo.toml` 统一管理时，确认 nsh 仍能找到并构建 `native/app`；如果 nsh 要求 app crate 自成 workspace，把这一点记录下来，并选一个可行的布局。
3. `App.tsx` 保持最小：显示 “Haiku” 文字，并调用一个导出的 Rust 函数（例如 `Core.version()`），证明 TSX 能调到 Rust。
4. 依次通过：`nsh check`、`nsh build`（不能有 warn）、`nsh test`；`nsh run` 能打开 macOS 窗口（用 `screencapture` 截图，存到 `.artifacts/m0-01/`，此目录加入 `.gitignore`）。

## B. S2：QuickJS 双份链接

背景：NSH 的 mixed 模式内置一份 QuickJS；CJRT（`cjrt-plugin-host`）依赖 `rquickjs-sys 0.11`，也是一份 QuickJS。两者静态链接进同一个进程，可能出现符号冲突，或者两边的运行时状态互相干扰。

1. 在 `native/crates/haiku-plugins` 中依赖 CJRT，写法与参考版一致：
   `cjrt-plugin-host = { git = "https://github.com/zsakvo/cjrt-workspace.git", branch = "feature/base" }`
   （参考 `~/Project/haiku-gpuix/native/Cargo.toml` 和 `native/Cargo.lock`，以及 `.cargo/config.toml` 中的 `[net] git-fetch-with-cli`。用法看 `~/Project/haiku-gpuix/native/src/` 中调用 `PluginHost` 的代码。参考版只读，不得修改。）
2. 在 `haiku-plugins` 中提供一个最小函数：创建 PluginHost，安装并加载一个内联 JS 插件（`export async function echo(args){ return { ok: true, n: args.n * 2 } }`），调用它并返回结果。
3. 在 `native/app` 导出 `#[module] Spike`，其中 `async fn cjrt_echo(n: f64) -> Result<f64, String>`。CJRT 调用放在 `background`/`blocking` 中执行，不能阻塞引擎线程。
4. 验证（每项都记录命令和结果）：
   - a. debug 与 release 都能链接成功：`nsh build`、`nsh run`，以及 `nsh package --target macos`（或 tooling.md 中对应的 release 构建）。
   - b. 符号检查：对产物运行 `nm -gU`，筛出 `JS_` 开头的符号，看有没有重复定义，或被某一方静默覆盖（例如两边的 `JS_NewRuntime` 是否解析到同一个实现）。说明两份 QuickJS 的版本。
   - c. 并发：写一个 `*.test.tsx`（或 nsh_testing 测试），在 TSX 侧持续执行 JS 计算的同时，并发调用 `Spike.cjrtEcho` 至少 1000 次，结果全部正确，没有崩溃。
   - d. 在真实窗口中点击按钮调用 `cjrtEcho`，界面正确显示结果。
5. 如果出现冲突：**不要修改 cjrt-workspace，也不要修改 NSH**。记录具体的错误和符号，评估 ARCHITECTURE.md §6 S2 中的方案 A/B/C，给出推荐，然后停下。

## 汇报

- 更新 `specs/STATUS.md` 中 S1、S2 两行的结论和证据，并追加一个日期小节（命令、结果、截图路径、未覆盖的部分）。
- 在 Codex 的最终回复中列出：改动的文件、所有验证命令及其结果、遇到的问题，以及对布局或方案的选择和理由。
- 不要做的事：迁移参考版代码、开始 M1、提交 git、修改 `specs/` 中除 STATUS.md 以外的文件（如果认为需要修改，请在汇报中提出）。
