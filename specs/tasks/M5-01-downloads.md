# 任务 M5-01：下载服务、下载页与下载设置

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。M5 第一个任务。

## 先读

- `specs/LIBRARY.md` §6（下载）与 §7（下载测试）；`specs/PLAYBACK.md` 媒体缓存一节；`specs/PLUGIN_SDK.md` `streams`/音质；`specs/UI.md` 页面表“下载”行与设置分组；`specs/DATA.md` 设置分组。
- 已有代码：`haiku-services::{downloader,downloads}`（M0 迁移的底层下载器、嗅探扩展名、打标签与原子落盘）、`haiku-library::tags`、`haiku-audio::media_cache`、`haiku-playback::network`（候选回退与 headers）、`native/app/src/{core,plugins,track_list,collections}.rs`、`src/shell/{AppShell,Settings}.tsx`（侧栏底部已有下载入口 `nav-downloads`）。
- 参考版（只读，本机 `~/Project/GPUI/haiku`）：`src/services/{downloads,management,settings}.ts`、`src/ui/downloads.tsx`（任务记录字段、设置字段名与持久化格式需兼容，D5）。
- 测试用 `openIsolated` 与本地 loopback HTTP fixture，不访问真实上游。不新增依赖 Rust `animated_value` 的界面。跑 nsh test 与截图前确认未锁屏（`ioreg -n Root -d1 | grep IOConsoleLocked` 为 false）。

## 1. 下载服务（Rust，先做）

- 任务队列：key 为 `source + NUL + id`，每次尝试用唯一 task id；状态 queued / resolving / downloading / tagging / completed / failed / paused / canceled / missing，带进度与字节数。
- 并发 1–5（设置）；暂停、继续、重试、取消、移除（仅记录 / 连同文件删除）、清理已完成、全部失败重试；失败自动重试间隔 2 秒、8 秒。
- 按设置音质调用 `streams` 取候选（保留 headers），错误候选回退；同音质完整媒体缓存优先复用；`.part` 断点续传（Range）；嗅探真实容器修正扩展名；半成品不当完整文件。
- 目录与命名：默认 `~/Music/Haiku`；文件名模板（`{artist} - {title}` 等）；布局 flat / artist / artist-album / playlist / provider；非法字符过滤与重名处理。
- 附加内容：封面高清/标清；封面与歌词各自 none / embed / sidecar / both；可选包含译文（歌词走 M4-04 的 `haiku-lyrics` 来源链）；写标签后原子重命名。
- 持久化任务记录（位置与字段沿用参考版）；重启后进行中任务回到 queued。
- 导出 `core.downloads`：enqueue(tracks | collectionRef)、pause/resume/retry/cancel/remove(withFile)、clearCompleted、retryFailed、snapshot、reveal（显示文件）、verify（校验丢失 → missing）、onChange → Stop。带文档注释。

## 2. 界面

- TrackList 右键菜单与多选工具栏“下载”；歌单/专辑详情“下载全部”。
- 下载页（侧栏底部入口）：状态筛选、虚拟列表、行内进度与操作、批量操作、移除确认（是否删除文件）。
- 设置“下载”分组：目录、并发、音质、文件名模板、布局、封面/歌词选项、包含译文。

## 3. 后做（额度不足可留到 M5-02，如实汇报）

- 自动下载歌单（新增曲目自动下载；“跟随移除”需同时满足“非手动下载”且“不属于其他自动下载歌单”）。
- 更换目录：移动 / 保留 / 取消，移动期间调度器暂停，逐文件报告失败。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：loopback HTTP fixture 覆盖 headers 透传、Range 续传、MIME 错报修正扩展名、中途断开后重试成功、候选回退、缓存复用、标签与 sidecar 内容、重名处理、取消/暂停/继续、重启后回到 queued、参考版任务记录读写。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`（全绿）。Headless：从曲库菜单下载 → 下载页出现并完成；失败 → 重试；移除确认。
3. 真实运行截图（浅色，fixture）到 `.artifacts/m5-01/`：下载页（含进行中与已完成）、下载设置。
4. 收尾：更新 STATUS（F13；追加 M5-01 小节 ≤ 40 行），保留改动等待审阅。

## 不做

账号与统计上传（M5-02 之后）；Gist 同步与备份；移动端“仅 Wi-Fi”。

## 汇报

最终回复：`core.downloads` API 概要、持久化位置与参考版兼容情况、设置字段、偏离、待人工确认项、需审阅者决定的事项、未完成项（额度不足时如实列出）。
