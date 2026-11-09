# 任务 M1-03：本地曲库 + TrackList

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。完成后提交一次并停下汇报。

## 先读

- `specs/LIBRARY.md` §1、`specs/UI.md` §1.4、§3（TrackList 与右键菜单）、`src/ui/STANDARDS.md`、设计规范中 “Designing data-heavy interfaces” 与 “Prefer command surfaces over hover toolbars”。
- 参考版行为（只读）：`~/Project/haiku-gpuix/src/services/local.ts`、`src/ui/local-library.tsx`、`src/ui/track-editor.tsx`，参考版 `specs/SPEC.md` F10 与 §6 中本地扫描相关条目。
- 已迁移的代码：`haiku-library` 的 `local_scan`、`local_library`、`tags`。
- M0-04 的 `virtual_list` 结论（STATUS.md M0-04 小节）；列表单元格不得持有行状态。

## 0. 清理与遗留

- 删除 M1-02 的开发入口 `Core.devPlayFiles`/`devVerifyPlayback` 与 `PlaybackSmoke.tsx`。
- M1-02 试播时出现 `unknown classes`（`mb-(--sidebar-top-gap)`、`size-(--sidebar-icon-size)`、`rounded-(--radius-inset)` 等）。先用正式入口 `App.tsx` 执行一次 `nsh run`，看是否同样出现：若只在单独入口出现（该入口未加载主题），在汇报中说明即可；若 `App.tsx` 也出现，修复（查 NSH 主题/tailwind 文档中自定义 token 的正确声明方式），不改动已确认的视觉效果。

## 1. Rust：`haiku-library` 服务

- `Library` 服务：扫描根的增删（持久化到数据根下 `library/roots.json`）、扫描/重新扫描（进度事件、可取消）、索引 `library/index.json`（可重建）、覆盖 `library/overrides.json`（稀疏字段，权威）。规则按 LIBRARY.md §1：深度 32、跨根去重、身份 `source=local, id=绝对路径`、重扫保留覆盖、文件暂时不在不删覆盖、只收录 haiku-audio 能解码的格式。
- 有效元数据 = 扫描标签 + 覆盖；编辑（单曲、批量只写改动字段）、重置；可选写回文件标签（用 `tags`，逐文件报告错误）。
- 分组：歌曲、专辑、艺术家（主要艺术家/专辑艺术家规则同参考版）；搜索、排序在 Rust 中做，结果以 id 列表或分页形式给 UI，不把整个库一次性序列化给 TSX。
- 导出 `core.library`：根管理、`scan`/`cancel`、`onProgress → Stop`、`onChanged → Stop`、查询（视图、搜索词、排序）、编辑/重置/写回。导出都写文档注释。

## 2. `TrackList` Rust 组件

- 基于 `virtual_list`，行高固定（compact 密度），列：序号/播放指示、标题+艺术家、专辑、时长；当前播放曲目有指示；只用 token。
- 数据源：Rust 侧的查询结果（id 列表），行内容按 id 读取，有效元数据实时反映编辑。选中集合存在以 id 为 key 的数据中，支持单击、Cmd 点击、Shift 范围选择、Cmd+A。
- 事件：双击/回车 → 用**当前完整筛选结果**作为队列从该行开始播放（调用 `core.player`）；右键 → `ContextMenu`。
- 右键菜单本任务只放已有能力：播放、下一首播放、加入队列、编辑信息、在 Finder 中显示。喜欢、加入歌单、下载等尚未实现的功能**不显示**（不是禁用）。
- 组件将被搜索、歌单、详情页复用，接口按“数据源 + 可选列 + 事件”设计，不要写死本地曲库。

## 3. 本地曲库页面（TSX）

- 无扫描根时：EmptyState + “添加文件夹…”（`pick_paths` 选择目录）。
- 有内容时：顶部为 歌曲/专辑/艺术家 ToggleGroup、搜索框、排序菜单、“管理文件夹…”与“重新扫描”；扫描中显示进度并可取消；扫描报告中的错误可展开查看。
- 专辑/艺术家视图：点进分组后显示该组的 TrackList（返回走导航历史，路由用 EntityRef 风格的参数）。
- 编辑信息：Dialog，单曲或多选批量；批量只提交改动字段；“重置”和“写回文件标签”都需要确认。
- 页面状态（视图、搜索词、排序、滚动位置）离开后保留。
- 加载、空、失败三种状态都要有。

## 验收（每项跑一次）

1. `cargo test --workspace`：新增扫描根持久化、重扫保留覆盖、跨根去重、批量编辑只写改动字段、搜索/排序的测试（用 `test_support` 和临时目录）。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`。Headless 测试覆盖：空状态 → 添加根（测试中注入临时目录）→ 扫描完成出现歌曲；切换专辑视图并进入一个专辑；多选 + 批量编辑后列表显示新值；双击行后 `core.player` 状态为该曲且队列为完整筛选结果；右键菜单项目正确。
3. 真实扫描：`nsh run` 后添加一个包含若干真实音频的目录（可用 `.artifacts/` 下自行准备的测试文件，不提交音频），截图本地曲库页（浅色一张、深色一张），双击一首确认开始播放（从日志看）。
4. 提交：`M1-03: local library service, TrackList, local library page`，一并纳入本任务文件和 `specs/UI.md` 的修改。

## 汇报

STATUS.md：F10 改为“部分实现”（扫描、编辑；刮削在 M4），追加 “M1-03” 小节（不超过 30 行）。最终回复：TrackList 的接口设计、unknown classes 的结论、偏离 LIBRARY.md 的地方、需要审阅者决定的事项。
