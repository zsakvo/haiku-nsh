# 任务 M4-05：远端收藏刷新、刮削任务与审阅界面

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。这是 M4 最后一个任务。

## 先读

- `specs/LIBRARY.md` §1（编辑、覆盖记录、写回确认）、§2（刮削）、§3（收藏远端歌单与镜像规则）；`specs/PLUGIN_SDK.md` §3.3（完整翻页）与 `scrape.search/lyrics`；`specs/UI.md` §3 “详情”“本地曲库（刮削审阅）”。
- 已有代码：`haiku-plugins::scraper`（打分、verdict 0.86/0.08/0.55、并发 4、8 秒超时、跨源合并）、`haiku-store::collections`（远端收藏镜像 `(refreshedAt, version)`）、`native/app/src/{collections,sources,library}.rs`、`src/pages/{SourceDetailPage,LibraryPage}.tsx`、M2-03 qq/kugou scraper 插件与契约 fixture。
- 参考版（只读，本机 `~/Project/GPUI/haiku`）：`src/services/{scraper,collections}.ts` 及曲库刮削/审阅相关 UI（字段名与持久化格式兼容，D5）。
- 测试用 `openIsolated` 与 loopback/fixture 插件，不访问真实上游。不新增依赖 Rust `animated_value` 的界面。跑 nsh test 与截图前确认未锁屏。

## 0. M4-04 遗留小项

- 查找歌词对话框的逐源错误显示为可读中文（如“返回格式无法解析”），原始错误进日志。

## 1. 远端收藏刷新（先做）

- 打开收藏的远端歌单详情时：先显示离线快照，后台按 PLUGIN_SDK §3.3 完整翻页拉取，成功后原子更新镜像（`refreshedAt` 递增），失败保留快照并提示“刷新失败，显示离线内容”。
- 同一歌单并发刷新单飞；离开页面取消；迟到结果按版本丢弃。
- 详情页“取消收藏”（确认），不影响远端。

## 2. 刮削任务（Rust 服务）

- 本地曲库单首/批量刮削：并发 4、单源 8 秒；按 verdict 自动采用 / 待审 / 跳过；可取消（已完成的结果保留）；进度事件。
- 候选来源配置：启用 `scrape` 的插件 + 可选音乐来源（`search.track`），可启停排序，存设置（字段名兼容参考版）。
- 应用模式：“只填补缺失字段”与“覆盖”；结果写入 `library/overrides.json` 稀疏覆盖；自动采用可撤销；人工确认结果持久化。未审阅结果绝不写回文件标签；写回文件需确认（沿用 M1 编辑写回路径）。
- 导出 `core.scrape`：start(ids, options)、cancel、jobs/队列快照、review 列表、accept(candidate)/skip/undo、onChange → Stop。带文档注释。

## 3. 界面

- 本地曲库 TrackList 菜单与多选工具栏加“刮削…”；刮削进度条（顶部或页内，可取消）。
- 审阅面板：待审曲目列表 → 候选（分数、来源、字段差异对比当前值）、手动匹配（自定义关键词重搜）、采用/跳过；自动采用区可撤销。
- 设置“来源”或“存储”中加刮削候选来源与模式（位置按 UI.md 选择并汇报）。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：收藏刷新完整翻页/失败保留快照/单飞/迟到丢弃；刮削 verdict 三档、取消保留已完成、填补 vs 覆盖、撤销、未审阅不写回、重开 Core 后人工结果仍在。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`（全绿）。Headless：收藏歌单打开后刷新显示新曲目、离线失败提示；批量刮削 → 自动采用 + 待审 → 审阅采用/跳过 → 撤销；取消收藏确认。
3. 真实运行截图（浅色，fixture）到 `.artifacts/m4-05/`：收藏歌单刷新后详情页、刮削进度、审阅面板（含字段差异）、刮削设置。
4. 收尾：更新 STATUS（F10 刮削、F11 远端收藏；追加 M4-05 小节 ≤ 40 行），保留改动等待审阅。

## 不做

下载（M5）；Gist 同步与备份（M5）；歌词刮削写回文件（随下载附加内容做）。

## 汇报

最终回复：`core.scrape` 与刷新 API 概要、持久化位置与参考版兼容情况、设置位置、偏离、待人工确认项、需审阅者决定的事项、未完成项（额度不足时如实列出）。
