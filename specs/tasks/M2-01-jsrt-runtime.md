# 任务 M2-01：haiku-jsrt 补齐 R1–R12

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。完成后保留工作区改动并停下汇报，不提交 git（审阅通过后由审阅者提交）。

M2 拆分：**M2-01 运行时（本任务）** → M2-02 haiku-plugins 宿主（包/实例/安装事务/描述符/SourceApi/认证）+ `sdk/haiku.d.ts` + subsonic → M2-03 其余内置插件、`haiku-plugin` CLI、契约测试套件、设置中来源/插件/开发者面板、`sdk/GUIDE.md`。本任务只做 Rust crate，不改 UI、不写 `haiku` SDK 语义（`haiku-jsrt` 不含任何音乐概念）。

## 先读

- `specs/PLUGIN_SDK.md` §8（8.1 复用与自建、8.2 R1–R12）、§7（`ctx.http` 需要的 fetch 能力）、§6（错误结构）；`specs/SPEC.md` D10–D12。
- 现有 `native/crates/haiku-jsrt/src/{lib.rs,tests.rs}`（M0-02 最小子集：专用线程、串行命令、超时退役、interrupt）及 STATUS 中 M0-02 的结论（同步原生 stdlib 工作不能被 interrupt 抢占）。
- 依赖只用 workspace 已有的 `rquickjs`（quickjs-jit）与 `quickjs-jit-stdlib`；新增依赖（reqwest+rustls、oxc、aes/rsa 等 RustCrypto）先确认与 NSH 已锁定版本不冲突，不引入第二份 QuickJS。
- 参考版（只读）`~/Project/GPUI/haiku`：插件运行时如何暴露 fetch/console/storage/加解密，`tests/qq-plugin.test.ts` 中用到的 crypto 调用形态（API 名称与参数需兼容，便于 M2-03 迁移 qq-music）。
- 构建前按 README 设置 `CARGO_PROFILE_{DEV,RELEASE}_BUILD_OVERRIDE_STRIP=false`。

## 范围（按 §8.2 逐项）

- **R1** 实例生命周期：create/load/unload/destroy；与 NSH UI VM 隔离（独立 Runtime/Context，专用线程）。保留现有退役与 join 语义。
- **R2** 模块加载：从包目录加载入口，支持相对 import；`.ts` 用 oxc 剥离类型并生成 source map，异常位置还原到 `.ts` 源行；拒绝包目录外路径（`..`、绝对路径、符号链接逃逸，测试覆盖）。
- **R3** 宿主内置模块：可按名称注册源码模块或 Rust 绑定模块（为 M2-02 的 `haiku` 模块预留），插件 `import ... from "haiku"` 可解析。
- **R4** 调用：按导出路径（如 `default.search`）调用，参数/返回 serde_json；单次调用超时；结构化错误（name/message/stack/文件/行/列）。
- **R5** 取消：宿主持有可 abort 的调用句柄；JS 侧为 `AbortSignal`；进行中的 fetch 随之中止；取消后实例仍可用。
- **R6** 上下文注入：每次调用传入宿主构造的 ctx 对象（含 signal、http、storage、log 的底层挂点），JS 只看到本次调用的 ctx。
- **R7** `fetch`/`Headers`/`Request`/`Response`（reqwest+rustls）：方法、头、body（文本/二进制/JSON）、重定向、超时、AbortSignal；请求钩子回调请求与响应元数据（不含 body）给宿主。测试用本地 HTTP 服务器，不访问外网。
- **R8** `console`：各级别输出到宿主日志通道（带实例 id），对 Authorization/Cookie/token 类字段脱敏。
- **R9** 实例存储：持久化 KV，原子写（复用 `haiku-store` 的 json 原子写或同等实现），路径由宿主传入；卸载重载后数据仍在。
- **R10** 加解密扩展：`createCipheriv`/`createDecipheriv`（aes-128/192/256 × ecb/cbc/ctr/gcm，含 setAutoPadding、getAuthTag/setAuthTag）、`publicEncrypt`（RSA PKCS#1 v1.5 / OAEP，PEM 公钥）；挂在 stdlib `crypto` 上，与 Node 调用形态一致。用已知向量测试（NIST/RFC 或 openssl 生成并写入测试）。
- **R11** 资源上限：内存上限（超限得到结构化错误且实例可退役重建）、CPU 时间上限（interrupt handler）；“描述模式”：可求值模块并读取导出结构，但调用处理函数被拒绝。
- **R12** 热重载：替换模块代码，存储保留，旧调用要么完成要么得到明确错误。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml -p haiku-jsrt`：R1–R12 每项至少一个单元测试（测试名带 `r1_`…`r12_` 前缀便于核对）；R1、R4、R5、R7 另有并发测试（多实例 × 多调用同时进行，含取消）。
2. `cargo test --manifest-path native/Cargo.toml --workspace` 全部通过；`cargo clippy -p haiku-jsrt --all-targets` 无新警告。
3. “与 NSH 同进程”验证：在 `native/app` 加一个 `#[cfg(test)]` 测试（或 Headless 测试），在 NSH engine 运行时并发跑 jsrt 实例的 R1/R4/R5/R7 场景，证明不影响 UI VM、不出现符号冲突；`nsh check`、`nsh build`（仅已知例外）、`nsh test` 通过。
4. 收尾：更新 STATUS（新增或修订 F02 的“运行时部分”说明，追加 M2-01 小节 ≤ 40 行，列出新增依赖及版本），保留改动等待审阅。

## 不做

`haiku` SDK 语义（definePlugin、实体、分页、认证）、haiku-plugins、包安装、CLI、契约套件、任何 UI（均在 M2-02/03）。不修改 NSH 或上游 quickjs 源码。

## 汇报

最终回复：公开 API 概要（类型与方法签名）、新增依赖、各 R 项对应测试名、与 §8 的偏离、无法用 interrupt 抢占的已知情形、需审阅者决定的事项。
