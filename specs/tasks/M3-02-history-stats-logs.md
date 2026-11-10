# 任务 M3-02：历史、听歌统计（本地）、日志与通知中心

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。这是 M3 最后一个任务。

## 先读

- `specs/LIBRARY.md` §4（历史）、§5（统计，只做本地部分）；`specs/DATA.md` §1（`history.json`、`stats.json`、`logs/`）、§6（日志与通知）；`specs/UI.md` §3 页面表“历史”“统计”、§6 设置“其他”（日志、关于）。
- 已有代码：`haiku-store::{collections,settings,json_file}`（服务写法、原子写、防抖与冲刷）、`haiku-playback::Controller`（播放事件来源）、`native/app/src/core.rs`（`openIn/openIsolated`）、`haiku-plugins` 的插件日志通道、`src/pages/LikesPage.tsx`（页面结构）。
- 参考版（只读）`~/Project/GPUI/haiku`：`src/services/{history,stats,logs,notifications}.ts`、`src/ui/{logs,notifications}.tsx` 及历史/统计页（字段名与 JSON 格式需与参考版兼容，D5）。
- 测试必须使用独立临时数据根（`openIsolated`）。不新增依赖 Rust `animated_value` 的界面。

## 0. M3-01 遗留

- PlayerDock 与正在播放覆盖层的喜欢按钮：已喜欢时显示实心爱心（与 TrackList 一致），未喜欢为描边。

## 1. 历史（`haiku-store::history`）

- 播放历史：跨来源身份去重，最多 500 条，按时间分组；搜索、来源筛选、删除单项、清空（确认）。
- 浏览歌单历史：最多 100 条，按 kind/source/id 去重；保存认证封面所需 headers；trackCount 未知留空（本任务本地来源可写入，在线详情 M4 接入）。
- 搜索历史：去重、删除单项、清空（数据层与导出，搜索页在 M4 使用）。
- 导出 `core.history`（列表、删除、清空、onChange → Stop）。

## 2. 听歌统计（`haiku-store::stats`，本地）

- 有效记录：单段真实播放 ≥ 5 秒，最多 5000 条；排除暂停、seek 跳跃、异常间隔；由 Controller 事件驱动，统计判定可注入模拟时钟。
- 聚合：累计时长、次数、歌曲数、歌手数；日/月/年/全部范围；排行（次数优先）；小时分布；日历热图；连续天数；里程碑（50/200/500/1000/2000/5000）。
- 本地清除（确认）；远程上传留 M5。导出 `core.stats`。

## 3. 日志与通知（F16）

- app、plugin 两个频道，各保留 1000 条，记录会话边界，detail ≤ 8000 字符，持久化并按会话轮转（`logs/app.jsonl`、`plugin.jsonl`）；脱敏规则同 DATA.md §6。
- 分页、按级别/来源/关键词过滤；复制当前筛选结果、导出全部频道（系统保存选择器）、清空（确认）。
- 通知中心：后台任务结果与错误，可展开、清除；M1-05 遗留的“设置文件损坏已备份”接入通知。入口放在侧栏底部（与下载/设置并列）或顶栏菜单，按 UI.md 说明选择并汇报。
- 设置“其他”：日志查看器入口、关于（版本、NSH 版本、许可证）。

## 4. 页面

- **历史页**：曲目历史（TrackList，按日期分组标题）与歌单历史两个 tab；搜索、筛选、删除、清空。
- **统计页**：范围切换；计数卡片；排行（曲目/歌手）；小时分布柱状图；日历热图；里程碑。图表用 token 颜色绘制（Rust 组件或 TSX 均可，说明选择）；空状态。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：历史去重/上限/分组；模拟时钟驱动的有效播放判定（暂停、seek、异常间隔、5 秒边界）与各聚合、连续天数、里程碑；日志轮转/截断/脱敏/过滤；参考版 `history.json`/`stats.json` fixture 读取。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`（全绿）。Headless：播放本地曲目后历史页出现该曲；清空需确认；统计页范围切换；日志过滤与清空确认；通知可清除。
3. 真实运行截图（浅色）到 `.artifacts/m3-02/`：历史页、统计页（用 fixture 数据填充，含热图与小时分布）、日志查看器、通知中心、Dock 已喜欢实心爱心。
4. 收尾：更新 STATUS（F12 改“部分实现（本地）”、F16 日志与通知；追加 M3-02 小节 ≤ 40 行），保留改动等待审阅。

## 不做

统计远程上传与 Haiku 账号（M5）；搜索页与在线歌单历史写入（M4）；备份与 Gist 同步（M5）。

## 汇报

最终回复：导出 API 概要、与参考版 JSON 兼容情况、通知入口位置、图表实现方式、偏离、待人工确认项、需审阅者决定的事项、未完成项（额度不足时如实列出）。
