# 任务 M4-03：推荐、分类与电台

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。

## 先读

- `specs/UI.md` §3 页面表“推荐”“分类”“电台”、§1.3（Shelf、Card、EmptyState、ErrorState、Skeleton）；`specs/PLUGIN_SDK.md` §5.2（home/Shelf/Link/categories/radio 及宿主缓存规则）。
- 已有代码：M4-01/02 的 `native/app/src/sources.rs`、`src/pages/{SourceDetailPage,SourceContent,SearchPage 等}.tsx`、`src/pages/PlaceholderPage.tsx`（推荐/分类/电台目前是占位）、Controller 队列 API。
- 参考版（只读）`~/Project/haiku-gpuix`：`src/ui/{discover,category-groups,discover-source-selector,cached-pages,page-refresh}.tsx/ts`、`src/services/{sources,catalog}.ts`（缓存、错误隔离、空货架补齐、续歌逻辑）。
- 测试用独立临时数据根和 loopback/fixture 插件（扩展现有 network fixture），不访问真实上游。

## 0. M4-02 遗留小项

- 命令面板曲目行显示“标题 · 歌手”。
- 库来源浏览分组间距收紧（与搜索页网格一致）。

## 1. 推荐

- 来源选择器：只列有 `home` 能力的已启用来源，记住上次选择；只显示所选来源（不聚合）。
- 按 Shelf 类型渲染：tracks（紧凑列表，可起播）、playlists/albums/toplists/programs（横向货架卡片，进 M4-01 详情）、entries（快捷入口，按 Link 跳转；radio Link 进电台并起播）。`more` 链接可跳转。
- 宿主缓存：首屏缓存 7 天、最多 40 个货架 × 60 条；缓存优先展示，后台刷新替换；刷新按钮强制刷新；刷新时保留电台条目；空货架不显示。实现位置（Rust 服务 + 数据根文件）自定并说明。
- 按来源记录错误：所选来源失败显示 ErrorState 带重试，有缓存时显示缓存并提示刷新失败。

## 2. 分类

- tag 墙（分组标题 + 标签）→ 分类歌单网格；排序（来源提供时）与分页加载更多；点歌单进详情。

## 3. 电台

- 频道列表（`radio.channels`）与私人 FM；选择后起播，队列快到末尾时调用 `radio.next(channelId, {history, count})` 持续续歌，不重复已播曲目；切换到其他队列后停止续歌。
- 续歌逻辑放在 Rust（Controller 或 sources 服务），可用模拟实现测试。

## 4. 导出

- `core.sources` 增补 home（含缓存/刷新）、categories、categoryPlaylists、radioChannels，`core.player` 增补电台起播；签名自定并在汇报中列出。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：home 缓存（7 天过期、40×60 上限、刷新保留电台、缓存优先）、电台续歌触发/去重/切队列停止。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`（全绿）。Headless：推荐页货架渲染并从 tracks 货架起播、货架卡片进详情、entry 跳转；来源失败重试；有缓存时离线展示；分类 tag → 歌单 → 加载更多 → 详情；电台起播并在队列末尾续歌。
3. 真实运行截图（浅色，loopback fixture）到 `.artifacts/m4-03/`：推荐页、分类 tag 墙、分类歌单网格、电台页、命令面板曲目行（第 0 步）。
4. 收尾：更新 STATUS（F04 改为相应状态，追加 M4-03 小节 ≤ 40 行），保留改动等待审阅。

## 不做

在线歌词（M4-04）；远端收藏刷新与刮削 UI（M4-05）；下载（M5）。

## 汇报

最终回复：导出 API 概要、缓存位置与格式、续歌策略、与参考版差异、偏离、待人工确认项（真实来源首页/电台）、需审阅者决定的事项。
