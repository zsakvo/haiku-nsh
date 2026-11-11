# 任务 M4-04：远端歌词与来源链、手动查找歌词

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。

## 先读

- `specs/PLAYBACK.md` §6（来源链、超时、取消、缓存、手动查找，Q4 内置提供方）；`specs/DATA.md` §1（`lyrics/`）与设置字段；`specs/UI.md` §4 正在播放、§6 设置“歌词”分组；`specs/PLUGIN_SDK.md` 中 `track.lyrics` / `scrape.lyrics`。
- 已有代码：`haiku-lyrics::{parse,parse_krc,merge_translation,split_bilingual,Cursor}`、M1-04 本地歌词接线（`native/app/src/{presentation,lyrics_view,now_playing}.rs`）、`haiku-store::settings`、`native/app/src/sources.rs`（SourceApi 调用与取消）、M4-03 `discovery.rs`（Rust 服务 + 数据根缓存写法）。
- 参考版（只读，本机路径 `~/Project/GPUI/haiku`）：`src/services/{lyrics,lyrics-providers,lyric-file}.ts`（提供方请求参数、Kugou KRC 解密、TTML 转换、评分 `scoreLyrics/rankLyrics`、质量分级、设置字段 `lyricsProviderOrder`/`lyricsProvidersDisabled`/`lyricsCustomProviders`）。字段名与缓存格式保持兼容（D5）。
- 测试只用 loopback/fixture，不访问真实上游。不新增依赖 Rust `animated_value` 的界面。

## 0. M4-03 遗留 UI 小项

- 分类歌单页标题显示分类名称而不是原始 ID（如“cn”）。
- 电台未选中频道使用与已选一致的按钮样式（次要变体），不是纯文字。
- 推荐页横向货架卡片间距收紧，与搜索/分类网格一致。

## 1. 来源链（Rust，`haiku-lyrics` 或新服务模块，位置自定并说明）

- 顺序：手动覆盖 → 歌词缓存 → 本地文件（M1-04 已有）→ 当前来源插件 `track.lyrics` → 内置提供方（按设置排序、可禁用）。
- 内置提供方用 Rust 实现：Kugou（含 KRC 解密）、LRCLIB、Better Lyrics（TTML）、LyricsPlus、Paxsenix、自定义 HTTP（URL 模板 + 可选 JSON 路径）。上游 URL 可注入，便于 loopback 测试。
- 单源 8 秒、总计 25 秒；纯文本作为兜底但继续查找同步歌词；按参考版质量分级（逐字 > 逐行 > 纯文本）取最佳。
- 由当前曲目或元数据变化触发，不依赖起播成功；恢复暂停会话时立即加载。切歌/清空队列先取消旧请求，迟到结果按曲目身份 + generation 丢弃。
- 缓存：媒体缓存开启时持久化自动结果；手动选择始终保存；位置 `<data>/lyrics/`，格式说明。
- 解析补齐：TTML（含逐字与 x-translation）、YRC；统一到现有 `Lyrics` 结构。

## 2. 手动查找

- 正在播放歌词区菜单“查找歌词…”打开对话框：关键词（默认标题 + 歌手）、来源筛选、候选列表（评分/推荐/逐字/翻译标记）、预览、采用；“恢复自动”清除手动覆盖。
- 评分与排序沿用参考版规则（时长、标题、歌手、逐字、翻译加分）。

## 3. 设置“歌词”分组

- 提供方启用开关与排序；自定义 HTTP 来源增删改（名称、URL 模板、响应类型、JSON 路径、headers）。字段名兼容参考版。

## 4. 导出

- `core.lyrics`：当前状态（loading/lyrics/错误/各源错误）、`search(query, providers?)`、`resolve(candidate)`、`adopt(candidate)`、`resetToAuto()`、`onChange → Stop`。带文档注释，TSX 只做镜像；歌词行与当前行仍走 Rust 组件。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：来源链优先级（手动 > 缓存 > 本地 > 插件 > 提供方顺序/禁用）；单源与总超时；纯文本兜底后仍取到同步歌词；切歌取消与迟到结果丢弃；缓存开关；KRC 解密、TTML/YRC 解析 fixture；评分排序；自定义 HTTP JSON 路径。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`（全绿）。Headless：切歌后歌词不串；手动查找 → 预览 → 采用后重开 Core 仍生效；恢复自动；设置中禁用提供方后不再请求。
3. 真实运行截图（浅色，loopback fixture）到 `.artifacts/m4-04/`：正在播放显示远端逐字歌词、手动查找对话框（含候选与预览）、设置“歌词”分组、M4-03 遗留三项修正后的分类歌单页与电台页。
4. 收尾：更新 STATUS（F08 改为相应状态，追加 M4-04 小节 ≤ 40 行），保留改动等待审阅。

**跑 nsh test 与截图前确认屏幕未锁定**（`ioreg -n Root -d1 | grep IOConsoleLocked` 为 false），锁屏会让播放类用例停滞。

## 不做

桌面歌词与菜单栏歌词（M6）；刮削写回歌词到文件（M4-05）；歌词字体设置以外的外观项。

## 汇报

最终回复：`core.lyrics` API 概要、来源链实现位置、缓存位置与格式、各提供方覆盖情况与差异、偏离、待人工确认项（真实上游可用性）、需审阅者决定的事项、未完成项（额度不足时如实列出）。
