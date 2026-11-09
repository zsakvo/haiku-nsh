# 任务 M0-02：haiku-jsrt 最小内核 + 重做 S2

执行者：Codex。审阅者：Claude。完成后**停下并汇报**。

## 背景

M0-01 证实：CJRT（rquickjs-sys 0.11 / QuickJS-ng 0.11）与 NSH（quickjs-jit-sys 0.14.1 / QuickJS-ng 0.16.2）不能共存于一个进程（符号被静默覆盖，且 RustCrypto 版本冲突）。决策 D11 已修订：**不再依赖 cjrt-workspace**，在本仓库新建 `native/crates/haiku-jsrt`，参照 CJRT 的能力，基于 NSH 已经链接的 `quickjs-jit` + `quickjs-jit-stdlib` 实现插件运行时底层。

## 先读

1. `specs/SPEC.md` D11、`specs/ARCHITECTURE.md` §2/§4/§6（S2 已改写）、`specs/PLUGIN_SDK.md` §8（R1–R12）。
2. `specs/STATUS.md` 中 M0-01 小节（了解已有的产物、日志与结论）。
3. 依赖源码（以 `cargo metadata` 解析出的路径为准，不要凭印象写 API）：
   - `quickjs-jit 0.14.1`（rquickjs 分支，其 API 与 rquickjs 0.11 未必相同）
   - `quickjs-jit-stdlib 0.14.1`（README 中有 `init` / `resolver` / `loader` 的用法，并说明“宿主负责 Tokio 执行和 QuickJS job 轮询”）
   - `nsh-js 0.4.0`：看 NSH 自己是怎样创建 runtime、轮询 job、接入 stdlib 的，照着它的做法来
4. CJRT 的宿主层只作为**设计参考**（只读，不得作为依赖）：`~/Project/Rust/cjrt-workspace/cjrt_plugin_host/src/api.rs`、`cjrt_core/src/{core,scheduler,task,promise}.rs`。

## 范围（只做 S2 所需的最小子集）

### A. 去掉 CJRT

- 从 `haiku-plugins` 和 `native/Cargo.lock` 中移除 `cjrt-plugin-host` 及其依赖；删除 M0-01 的 CJRT echo 代码。`.cargo/config.toml` 中只为 CJRT 加的配置如果已经不需要，一并删除。

### B. `haiku-jsrt` 最小内核（R1、R4，加最小的 stdlib 接入）

- 依赖 `quickjs-jit` 与 `quickjs-jit-stdlib`，**版本要求必须与 nsh-core 解析出的一致**，保证依赖图中只有一份 `quickjs-jit-sys`。在 `native/Cargo.toml` 的 `[workspace.dependencies]` 中集中声明。
- `Runtime` 实例（R1）：在一个专用 OS 线程上持有 QuickJS Runtime/Context，通过 channel 接收命令；提供 `spawn`、`load_module(name, source)`（M0 只支持 JS 源码，不做 TS）、`call(export, args_json, timeout) -> Result<json, JsError>`、`shutdown`。异步导出必须能正常 resolve：线程内驱动 job 轮询，并为 stdlib 的异步 API 提供 Tokio 执行环境（参照 nsh-js 的做法）。
- 接入 stdlib：`init`、`resolver`/`loader`，保证插件可以 `import { createHash } from 'crypto'`、`import { Buffer } from 'buffer'`。
- 超时（R4 的一部分）：调用超时返回明确的错误；用 interrupt handler 中止死循环（`while(true){}`），并且中止后该实例仍然可用，或者被明确标记为需要重建。
- `JsError { name, message, stack }`。
- 单元测试（`cargo test -p haiku-jsrt`）：sync/async 导出、stdlib 的 crypto 与 buffer、抛出异常、超时、死循环中止、多个实例并行、销毁后不泄漏线程。
- 公共 API 写好文档注释。不做 fetch、console、storage、TS、加解密扩展（留给 M2）。

### C. 重做 S2 验收

`haiku-plugins` 用 haiku-jsrt 实现与 M0-01 相同的 echo（内联 JS 插件中同时调用 `crypto.createHash('md5')`，证明 stdlib 可用）；`native/app` 的 `Spike.cjrtEcho` 改名为 `Spike.pluginEcho`。App.tsx、按钮、`Spike.test.tsx`（16 个 worker 并发 1000 次，同时 TSX 持续做 JS 计算）沿用 M0-01 的设计并相应更新。

依次执行并记录：

1. `cargo tree --manifest-path .nsh/rust/Cargo.toml -d`（或等效命令）：依赖图中不能有两份 `quickjs-jit-sys`/`rquickjs-sys`；记录 RustCrypto 相关的重复版本（如果有）。
2. `nsh check`、`nsh build`（无 warn）、`nsh test`（包括 1000 次并发，全部正确）。
3. `nsh run`：在真实窗口中点击按钮，显示结果；截图保存到 `.artifacts/m0-02/`。
4. `nsh package --target macos`：生成 `.app`，启动后再点一次按钮（截图）；对 release 可执行文件运行 `nm -gU`，确认 `_JS_NewRuntime` 只有一个定义；链接日志中不能出现 duplicate symbol 警告。
5. 补充验证：插件 runtime 与 NSH UI VM 隔离。在插件中修改 `globalThis`（例如 `globalThis.__leak = 1`），确认 TSX 侧读不到；同样在 TSX 侧修改，插件侧也读不到。

如果失败（例如 quickjs-jit 的全局状态或 JIT 不支持多线程多 runtime）：记录证据和原因，评估可行的替代方案（例如所有插件实例共用一个专用线程、每个实例一个 Context），给出推荐后停下。不要修改 NSH 或 quickjs-jit 的源码。

## 汇报

- 更新 `specs/STATUS.md`：S2 一行写明结论与证据；S1 一行注明当前完整应用是否重新通过；追加日期小节“M0-02”，包括命令、结果、截图路径、未覆盖的部分。
- 最终回复列出：改动的文件、验证命令与结果、问题，以及对 haiku-jsrt 线程模型和 job 轮询方式的说明（M2 会在此基础上继续扩展）。
- 不要做的事：提交 git、修改 `specs/` 中除 STATUS.md 以外的文件、实现 R2/R3/R5–R12、修改 Rex 会话名。
