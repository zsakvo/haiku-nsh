# 任务 M5-04：Gist 同步

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。

## 先读

- `specs/DATA.md` §4（同步，只做 Gist）、§7 Gist 测试；`specs/LIBRARY.md` §3（Loro 合并规则）；`specs/SPEC.md` D8；`specs/UI.md` 设置“同步”分组。
- 已有代码：`haiku-store::{collections,playlist_core}`（Loro 文档）、`haiku-services::account`（M5-03 的 Keychain 凭据后端与单飞/退避写法可复用）、`native/app/src/{core,settings,account}.rs`、设置“同步”分组占位。
- 参考版（只读，本机 `~/Project/GPUI/haiku`）：`src/services/sync.ts` 及其调用的 native 端（Gist 文件名、gzip/base64 编码、设置字段、状态字段需兼容，D5）。
- **M5-01 教训**：effect 内 spawn 的任务会在 effect 重跑时被取消；effect 只读真正决定请求的信号。M5-02 遗留：不要用固定间隔轮询代替变更通道。
- 测试用 loopback HTTP fixture 模拟 GitHub API，**不访问真实 api.github.com**。跑 nsh test 与截图前确认未锁屏。

## 1. 同步服务（Rust）

- GitHub PAT 存 Keychain（测试用内存后端）；创建 Gist、绑定已有 Gist（id 或 URL）、断开、手动同步。
- 内容：Loro snapshot，gzip/base64 与参考版兼容；拉取后合并到本地文档并原子落盘、刷新视图；推送本地。
- 调度：本地修改后 12 秒防抖自动同步；每 2 分钟拉取；失败退避 15 秒 → 5 分钟；单飞；显示版本、最近成功时间与失败状态。
- 切换令牌或 Gist 时隔离旧状态，迟到响应丢弃；Core.close 时取消并等待退出。
- 导出 `core.sync`：snapshot、setToken、clearToken、createGist、bindGist(idOrUrl)、disconnect、syncNow、onChange → Stop。带文档注释。

## 2. 界面

- 设置“同步”：PAT 输入（掩码）与保存/清除、创建或绑定 Gist、断开确认、立即同步、状态（版本、最近同步、错误与下次重试）。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：两个 Loro 实例经假 Gist 往返合并（幂等、交换律、重复导入无重复曲目、删除与重排冲突不丢数据）；参考版编码互读；防抖、拉取、退避、单飞；切换令牌后迟到响应丢弃。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`（全绿）。Headless：保存 PAT → 创建 Gist → 立即同步显示版本 → 断开确认。
3. 截图（浅色，fixture）到 `.artifacts/m5-04/`：同步未连接、已连接。
4. 收尾：更新 STATUS（F15 同步部分；追加 M5-04 小节 ≤ 40 行），保留改动等待审阅。

## 不做

`.haikubak` 备份（M5-05）；自建后端同步（D8 暂缓）。

## 汇报

最终回复：`core.sync` API 概要、编码与参考版兼容、持久化位置、偏离、待人工确认项（真实 GitHub、Keychain）、需审阅者决定的事项、未完成项。
