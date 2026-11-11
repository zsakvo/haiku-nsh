# 任务 M5-03：Haiku 账号与听歌统计上传

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。

## 先读

- `specs/DATA.md` §3（Haiku 账号）、§7 账号测试；`specs/LIBRARY.md` §5 统计“远程”一条；`specs/SPEC.md` D6（会员不控制功能）；`specs/UI.md` 设置“账户”分组。
- 已有代码：`haiku-store::{stats,settings}`、`native/app/src/{activity,settings,core}.rs`、设置“账户”分组占位、`haiku-services`（放 `account.rs`、`stats_upload.rs`）。
- 参考版（只读，本机 `~/Project/GPUI/haiku`）：`src/services/{account,stats,settings}.ts`（接口路径、字段、会话持久化、eventId 算法与 revision/watermark 需兼容，D5）。
- **M5-01 教训**：effect 内 spawn 的任务会在 effect 重跑时被取消；effect 只读真正决定请求的信号。
- 测试用 loopback HTTP fixture 模拟服务端，**不访问真实 `api.haiku.zhendong.ltd`**。跑 nsh test 与截图前确认未锁屏。

## 1. 账号服务（Rust，先做）

- 服务地址可配置（默认 `https://api.haiku.zhendong.ltd`，前缀 `/api/v1`，响应 `{code, message, data}`）。
- 邮箱验证码登录（自动注册）、重发冷却 60 秒、验证码错误提示；恢复会话；登出；用户信息。
- 401 或 code 10002：单飞刷新一次并重放；刷新失败清会话；刷新未返回新 refresh token 时保留旧值；网络失败保留离线会话；登出或切换服务地址后到达的刷新响应丢弃。
- 会员：`me/subscription`（5 分钟缓存）与 `plans` 仅展示；“购买”标记即将开放；不控制任何功能。
- 令牌存 macOS Keychain。若需新增依赖（如 `security-framework`），在汇报中列出理由；测试使用内存凭据后端。
- 导出 `core.account`：snapshot、sendCode(email)、login(email, code)、logout、refresh、subscription、plans、setServer、onChange → Stop。带文档注释。

## 2. 统计上传（Rust）

- 登录后上传播放事件：`eventId = sha1(device|plugin|track|played|tz)`；meta/events 使用 revision 与 watermark；远程数据不回推；防抖 12 秒、每 2 分钟拉取；失败退避（最长 30 分钟）；登出/切换账号时停止并隔离状态。

## 3. 界面

- 设置“账户”：未登录（邮箱、发送验证码含倒计时、验证码、登录、服务地址）；已登录（用户信息、会员展示、购买即将开放、登出确认）；统计上传状态（最近同步时间、失败提示）。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：登录/恢复/登出、并发 401 单飞刷新与重放、刷新失败清会话、离线保留、登出后迟到刷新丢弃、冷却；eventId 与参考版一致、watermark 增量、退避、登出停止。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`（全绿）。Headless：发送验证码倒计时 → 登录 → 显示用户与会员 → 登出确认。
3. 截图（浅色，fixture）到 `.artifacts/m5-03/`：账户未登录、已登录。
4. 收尾：更新 STATUS（F12 远程、F14；追加 M5-03 小节 ≤ 40 行），保留改动等待审阅。

## 不做

Gist 同步与 `.haikubak` 备份（M5-04）；购买流程。

## 汇报

最终回复：`core.account` 与上传 API 概要、持久化与参考版兼容、新增依赖、偏离、待人工确认项（真实服务端、Keychain 实机）、需审阅者决定的事项、未完成项（额度不足时如实列出）。
