# 任务 M4-01：网络播放、通用详情页与库来源浏览

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。这是 M4 第一个任务：先打通“来源里的歌能播放、能点进详情”，搜索、推荐、在线歌词等放到后续任务。

## 先读

- `specs/PLAYBACK.md` §3（候选、重试、提前 EOF 恢复）、§5（媒体缓存）；`specs/PLUGIN_SDK.md` §3.2–3.3（实体与分页）、§5.1（streams/Expired/不支持的容器）、§5.3（library cards/browse）；`specs/UI.md` §3 页面表“详情”“库来源浏览”、TrackList 右键约定。
- 已有代码：`haiku-playback::{Controller,backend}`、`haiku-audio::media_cache`、`haiku-plugins` 的 `SourceApi`、`native/app/src/{core,track_source,collections,activity,track_list}.rs`、`src/pages/{CollectionPage,LikesPage}.tsx`。
- 参考版（只读）`~/Project/haiku-gpuix`：`src/services/{sources,catalog}.ts`、`src/ui/{detail-song-table,native-song-list,cached-pages}.tsx` 及播放相关 Rust 代码（行为依据）。
- 测试一律用独立临时数据根（`openIsolated`）和本地 loopback/fixture 插件，不访问真实上游。不新增依赖 Rust `animated_value` 的界面。

## 0. M3-02 遗留小项

- 通知中心与日志查看器的时间改为本地时区、可读格式（不显示微秒）。
- 通知行按钮可见文字为“清除”，序号只放在无障碍标签里。
- 设置“其他”中日志查看器只保留一种入口（按钮或内联区块，二选一并说明）。
- 统计页热图宽度与页面内容区协调（不要只占左侧一小块）。

## 1. 网络播放（`haiku-playback` + app 接线）

- 队列可以包含来源曲目（`EntityRef` 指向插件来源）。解析顺序按 PLAYBACK.md §3：完整下载文件（M5 前为空）> 同音质完整媒体缓存 > `streams(quality)` 候选，候选按顺序回退；不支持的容器跳过。
- `Expired` 或 `expiresAt` 过期：重新调用 `streams`；未过期不重复调用。
- 网络错误 1/2/4 秒重试最多 3 次；暂停/停止/seek/新队列取消重试；连续 30 秒正常播放清零。提前 EOF（已知时长前 ≥5 秒）走恢复流程。
- 网络播放写入媒体缓存（键 `(EntityRef, 音质)`），完整后标记；下次播放命中缓存不再请求地址。预加载下一首与当前模式一致。
- 音质取设置中的默认音质（没有就新增设置字段 `playback.quality`，默认 `high`，保留参考版字段名如有）。
- 来源曲目进入历史与统计（M3-02 已有接口），媒体中心显示来源封面。

## 2. 通用详情页

- 一个页面承载歌单、专辑、歌手、榜单：头部（封面、标题、副标题/作者、曲目数、描述折叠）+ `TrackList`；分页按 SDK 规则加载更多。歌手页至少有热门曲目与专辑列表。
- 播放全部、随机播放；TrackList 右键沿用现有菜单（喜欢、加入歌单等对来源曲目同样有效）。
- 远端歌单收藏/取消收藏：来源支持时显示按钮，写入 collections 的远端收藏（沿用 M3-01 `favorite:<sourceId>:<remoteId>` 约定）。
- 打开歌单详情时写入歌单浏览历史（M3-02 接口，trackCount 未知留空）；历史页“歌单历史”点击可回到详情。
- 加载中骨架、错误状态带重试；旧请求结果不得覆盖新导航。

## 3. 库来源浏览

- 侧栏音乐库下列出已启用、带 `library` 能力的来源（如 Subsonic、飞牛），点进后按描述符 `cards` 渲染分组；track 卡片用 TrackList，album/artist/playlist 用封面网格，点击进详情页。
- 来源未登录/连接失败时显示错误状态与“去设置”入口。

## 4. 导出

- `core.sources` 增补：详情获取（分页）、library browse、收藏远端歌单；`core.player` 支持以来源曲目列表起播。监听均返回 Stop。具体签名自定并在汇报中列出。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：候选回退、Expired 重取、重试计时与取消、30 秒清零、提前 EOF 恢复、缓存命中不请求地址、不支持容器跳过（假后端与模拟时钟）。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`（全绿）。Headless：从库来源浏览进入专辑详情并起播来源曲目；详情分页加载更多；收藏远端歌单后出现在侧栏/喜欢相关列表；错误状态重试；第 0 步的通知按钮文字。
3. 真实运行截图（浅色，loopback fixture 来源）到 `.artifacts/m4-01/`：库来源浏览、专辑详情、歌手详情、正在播放来源曲目时的 Dock、统计页热图（第 0 步）。另保存一份真实 `nsh run` 日志，证明 loopback 来源曲目实际出声/进度推进。
4. 收尾：更新 STATUS（F06、F11、F13 相关行，F04/F05 不动；追加 M4-01 小节 ≤ 40 行），保留改动等待审阅。

## 不做

搜索页与命令面板搜索、推荐/分类/电台（M4-02/03）；远端歌词与手动查找歌词（M4-04）；刮削 UI（M4-05）；下载（M5）。

## 汇报

最终回复：导出 API 概要、网络播放状态机与参考版差异、缓存键与音质设置、详情页覆盖的实体类型、偏离、待人工确认项（真实 Subsonic/飞牛/QQ 播放）、需审阅者决定的事项。
