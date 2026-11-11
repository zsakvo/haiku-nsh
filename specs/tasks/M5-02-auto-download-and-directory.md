# 任务 M5-02：自动下载歌单与更换下载目录

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。

## 先读

- `specs/LIBRARY.md` §6 自动下载与更换目录两条；M5-01 已提交的 `native/app/src/{download_service,downloads}.rs`、`src/pages/DownloadsPage.tsx`、`src/shell/DownloadSettings.tsx`、`src/state/downloads.ts`。
- 参考版（只读，本机 `~/Project/GPUI/haiku`）：`src/services/{downloads,management,settings}.ts` 中自动下载歌单与目录迁移的字段名和持久化（D5）。
- **注意（M5-01 教训）**：effect 内 spawn 的任务会在 effect 重跑时被取消；effect 只读真正决定请求的信号，必要时用 `computed` 比较输入。
- 测试用 `openIsolated` 与 loopback fixture。跑 nsh test 与截图前确认未锁屏。

## 1. 自动下载歌单（Rust）

- 本地歌单与已收藏远端歌单可开启“自动下载”；开启时下载现有曲目，之后新增曲目（含远端收藏刷新带来的新曲目）自动入队，标记为非手动。
- “跟随移除”：曲目移出歌单时，仅当该下载“非手动”且“不再属于任何其他自动下载歌单”时才删除记录与文件。
- 设置与记录字段沿用参考版；重开 Core 后保持。

## 2. 更换下载目录（Rust）

- 选择新目录后：移动 / 保留 / 取消。移动期间调度器暂停，逐文件移动并报告失败（失败项保留原路径），完成后恢复调度并更新记录路径。

## 3. 界面

- 歌单详情（本地与收藏）“自动下载”开关与“跟随移除”选项。
- 下载设置更换目录时的三选确认与迁移进度/失败报告。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：自动下载新增入队、跟随移除两条件、重开保持；目录迁移移动/保留/取消、暂停调度、部分失败报告。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`（全绿）。Headless：歌单开启自动下载 → 下载页出现曲目；更换目录选择“移动”后记录路径更新。
3. 截图（浅色，fixture）到 `.artifacts/m5-02/`：歌单自动下载开关、目录迁移确认。
4. 收尾：更新 STATUS（F13；追加 M5-02 小节 ≤ 40 行），保留改动等待审阅。

## 不做

账号与统计上传（M5-03）；Gist 同步与备份（M5-04）。

## 汇报

最终回复：API 变更、持久化与参考版兼容、偏离、待人工确认项、需审阅者决定的事项、未完成项（额度不足时如实列出）。
