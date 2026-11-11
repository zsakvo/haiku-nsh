# 任务 M5-05：`.haikubak` 备份导入导出

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。M5 最后一个任务。

## 先读

- `specs/DATA.md` §5（备份，含热恢复顺序与 v1 插件规则）、§7 备份测试；`specs/SPEC.md` D9；`specs/UI.md` 设置“存储”或“其他”中的备份入口。
- 已有代码：`haiku-store::{settings,collections}`、`haiku-plugins`（插件包、实例、storage、凭据）、`native/app/src/{core,plugins,download_service,sync}.rs`（暂停/恢复入口）、M5-03/M5-04 的 Keychain 凭据。
- 参考版（只读，本机 `~/Project/GPUI/haiku`）：`src/services/{backup,backup-files}.ts` 及 native 端（ZIP 条目名、manifest 字段、schema 1/2 需兼容，D5）。
- 测试用临时目录与构造的 fixture，不访问网络。跑 nsh test 与截图前确认未锁屏。

## 1. 备份服务（Rust）

- 导出：先冲刷所有服务；写 schema 2，元信息标明 Haiku NSH 及版本；内容为设置、喜欢（含旧格式）、`playlists.loro`、来源列表、插件包与实例（含 storage 与凭据）、可选 GitHub PAT（默认不含）。不含音频、缓存、字体、历史。
- 检查：inspect 返回预览（创建时间、平台、版本、包含内容、需重装的 v1 插件列表）。拒绝版本过高、损坏、绝对路径、反斜杠、冒号、`..`、符号链接、白名单外条目。
- 导入：写入前自动安全备份（最多保留 3 份）；按 DATA.md §5 热恢复顺序执行，失败也在 finally 重新加载；v1 插件不恢复并列出，对应 v2 内置插件尽量迁移连接字段（如 Subsonic 地址、用户名）。
- 导出 `core.backup`：export(path, {includeGithubToken})、inspect(path)、import(path)、safetyBackups()。带文档注释。

## 2. 界面

- 设置中“备份与恢复”：导出（确认框明示“包含明文登录凭据”、可选含 GitHub PAT）、导入（选择文件 → 预览 → 确认覆盖 → 结果含 v1 插件提示）。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：参考版格式 schema 1/2 fixture 恢复；导出再导入往返一致；各类恶意路径与版本过高拒绝；安全备份轮转 3 份；v1 插件跳过并列出、Subsonic 连接字段迁移；导入失败后状态与磁盘一致。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`（全绿）。Headless：导出确认 → 导入预览 → 确认 → 结果提示。
3. 截图（浅色，fixture）到 `.artifacts/m5-05/`：导出确认、导入预览。
4. 收尾：更新 STATUS（F15 备份部分；追加 M5-05 小节 ≤ 40 行），保留改动等待审阅。

## 汇报

最终回复：`core.backup` API 概要、格式兼容情况、偏离、待人工确认项、需审阅者决定的事项、未完成项。
