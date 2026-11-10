# 任务 M3-01：喜欢与用户歌单（collections 服务 + 页面 + 底栏/列表/媒体中心接入）

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。

M3 拆分：**M3-01 喜欢与歌单（本任务）** → M3-02 历史、听歌统计（本地）、日志与通知中心（F12 本地、F16 日志/通知）。

## 先读

- `specs/LIBRARY.md` §3；`specs/DATA.md` §1（`playlists.loro`）、§6；`specs/UI.md` §2（侧栏：喜欢、歌单…）、§3 页面表“喜欢 / 用户歌单”、TrackList 右键菜单、§5 PlayerDock（喜欢按钮）；`specs/MACOS.md` §1（媒体中心 favorite 命令，M1-05 推迟到本任务）；`specs/SPEC.md` D5。
- 已有代码：`haiku-store::{playlist_core,collections}`（参考版迁移，含 `__liked__`、plugin 收藏、export/import）、`native/app/src/{core,track_list,player_dock}.rs`、`native/app/src/platform/media.rs`、`native/app/src/settings.rs`（导出 + onChange → Stop 写法）、`src/pages/LibraryPage.tsx`（页面结构）。
- 参考版（只读）`~/Project/GPUI/haiku`：`src/services/collections.ts`、`src/core/likes`/`src/ui/likes.ts`、`collection-artwork.ts`、`local-collection-detail.tsx`（行为与字段名依据）。
- NSH Design Guides、Coding Guides（Lists、overlays、Menu）、`src/ui/STANDARDS.md`。不新增依赖 Rust `animated_value` 的界面（HANDOFF 已知限制）。

## 1. collections 服务（Rust）

- `haiku-store::collections` 收敛为服务：打开 `<data>/playlists.loro`（与参考版格式兼容，用参考版生成的 fixture 测试读取）；每次修改后防抖写盘（如 500ms），`Core` 关闭/退出时冲刷；调用方不再显式 `save_to_disk`。
- 喜欢：专用写入路径；跨来源 like/unlike/toggle/isLiked（按 EntityRef 身份）；按加入时间排序。
- 用户歌单：创建、重命名、编辑描述、删除；跨来源加歌（去重）、移除、重排；歌单之间排序。
- 收藏远端歌单：数据模型与 `favorite:<sourceId>:<remotePlaylistId>` 写入路径、`(refreshedAt, version)` 取较大方的镜像规则（单测）；打开详情时的远端刷新留 M4（本任务无在线详情页）。
- 导出 `core.collections`：列表、详情曲目、上述全部写操作、`onChange → Stop`（变更事件带受影响的歌单 id）、`isLiked`/喜欢集合订阅。带文档注释。TSX 只做镜像。
- 喜欢状态变更推送给 Rust 侧消费者（Dock、TrackList、媒体中心）不经 TSX。

## 2. 界面

- **侧栏**：“喜欢”入口与“歌单…”分组（用户歌单列表，可新建；超出时滚动；保持 M1-01c 字号与侧栏底部 96pt 预留）。
- **喜欢页**：TrackList（Rust 虚拟列表）；搜索、来源筛选、播放全部、批量管理（多选移除、加入歌单）；空状态。
- **用户歌单页**：头部（合成封面：前四首曲目封面拼合，无封面回退占位；标题、描述、曲目数）、编辑（重命名/描述，对话框）、删除（确认）、TrackList 支持拖动或菜单重排、移除；歌单总览支持列表/网格切换。
- **TrackList 右键菜单**：补齐“喜欢/取消喜欢”“加入歌单 ▸（子菜单含新建歌单…）”；下载、详情等未实现项不显示。
- **PlayerDock**：显示喜欢按钮（M1 隐藏的那个），状态绑定 Rust 信号；正在播放覆盖层同样提供。
- **媒体中心**：注册 favorite 命令（MPRemoteCommandCenter like/bookmark 中合适者），与界面同一路径；状态同步。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：防抖落盘与退出冲刷、重启恢复；参考版 `playlists.loro` fixture 读取；喜欢 toggle/跨来源身份；歌单 CRUD、去重加歌、重排；两个 Loro 实例合并与重复导入幂等；收藏镜像取较大方规则。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`。Headless：Dock 喜欢按钮切换后喜欢页出现该曲；右键“加入歌单 ▸ 新建歌单…”创建并加入；歌单重命名/删除确认；重新 open Core 后喜欢与歌单仍在；`onChange` Stop 后不再回调。
3. 真实运行截图（浅色，用本地测试音频）到 `.artifacts/m3-01/`：喜欢页（含若干曲目）、用户歌单详情页（合成封面）、TrackList 右键菜单展开“加入歌单”、Dock 已喜欢状态。日志确认媒体中心 favorite 状态。
4. 收尾：更新 STATUS（F11 改“部分实现（本地）”，追加 M3-01 小节 ≤ 40 行），保留改动等待审阅。

## 不做

远端歌单收藏的在线详情与刷新、来源“我的歌单”（M4）；Gist 同步与备份（M5）；历史/统计/日志（M3-02）；菜单栏（M6）。

## 汇报

最终回复：`core.collections` API 概要、与参考版 `playlists.loro` 兼容情况、偏离 LIBRARY/UI 的地方、待人工确认项、需审阅者决定的事项、未完成项（额度不足时如实列出）。
