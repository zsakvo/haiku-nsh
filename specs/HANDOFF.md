# 交接记录（2026-11-10）

换电脑继续时先读本文件，再读 `specs/SPEC.md`（决策 D1–D18）、`specs/MILESTONES.md`、`specs/STATUS.md`、`specs/tasks/README.md`。

## 工作方式

- 用户要求：**代码全部由 Codex 编写**，Claude 只负责写任务说明（`specs/tasks/<任务号>.md`）、派发、审阅证据、更新 specs。
- 每个任务都遵守 `specs/tasks/README.md` 的通用约定：只做列出的验收、每项跑一次、不做 GUI 自动化、失败最多两次修复（审阅者可追加授权）、STATUS 小节有篇幅上限。Codex 不提交 git；**审阅通过后由 Claude 在 main 本地提交（不推送）**，提交信息末尾加 `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`（用户已授权）。
- 用户要求 Claude 派发后**自动监听 Codex 并自动推进**：后台每 30s `rex capture` 窗格末尾 25 行，连续 3 次无 “esc to interrupt” 视为空闲（只看末尾十几行会误报）。仅架构决策或人工验证才停下问用户。
- 派发时不要让 Codex 用 `/new`（会切到 `~/.codex/worktrees/` 工作树），消息里写明“在主仓库 /Users/zsakvo/Project/GPUI/haiku-nsh 工作”；Codex 状态栏显示 worktree 路径但实际改动落在主仓库。Codex 发送后有时需再补一次 `rex send-key enter`。
- 本机已确认 Codex 在当前 Rex 会话 `session:01a18a85-6562-735f-9af5-41e1a0ea8fc2` 的右侧窗格 `block:01a1c2f7-c4a0-7591-bd83-d40c4e0a846e`；派发方式是 `rex send` + `rex send-key enter`，随后 capture 确认已进入执行。换会话或机器时重新检查，不复用旧窗格 ID。
- 执行者不得修改 Rex 会话名称或布局。

## 环境

- macOS arm64，Xcode；`nsh 0.4.0`（`curl -fsSL https://nsh.dev/install.sh | sh`）；cargo 用 huacnlee/cargo（`curl -fsSL https://huacnlee.github.io/cargo/install.sh | sh`，会替换 `~/.cargo/bin/cargo`，原版留为 `cargo.rustup`）。
- 参考项目（只读，业务与行为依据）：路径随机器不同——MacBook Air（Rex 会话 Mono）为 `~/Project/GPUI/haiku`，另一台为 `~/Project/haiku-gpuix`；派发任务时按当前机器改写任务文件里的路径。原 Flutter 版 `~/Project/Flutter/haiku`。
- 仓库有 `origin` remote，分支 main；已验收任务逐个本地提交；推送由用户自行决定（截至 `625319a` 已推送）。`.artifacts/`（日志、截图、测试音频）被忽略，不随提交走；新机器验收需重新生成证据，不假定旧机器音频和日志存在。

## 当前进度（2026-11-10 交接时）

- M0–M3 完成（M3-02 见下方更新）。原记录：M3-01 已提交（`9a52e44`）；M3-02 当时进行中（任务 `specs/tasks/M3-02-history-stats-logs.md`）。
- 交接时 Codex 仍在执行 M3-02（约 34 分钟，处于自查阶段），周额度剩约 11%。已有改动：`haiku-store::{history,stats,logs,document}`、`tests/activity.rs` 与参考版 `history.json`/`stats.json` fixture、`native/app/src/activity.rs`、Core/plugins/controller/track_list 等接线、`src/pages/{HistoryPage,StatsPage}.tsx`、`src/shell/Diagnostics.tsx`、`assets/icons/heart-filled.svg`、根目录 `{History,Stats,Logs,Notifications}Preview.tsx`、`Activity*.tsx`。旧机器 `.artifacts/m3-02/` 已有 workspace/nsh 日志（含 repair1/2）及 history、statistics、liked-dock 截图；日志查看器、通知中心截图尚未见到。
- **新机器接手 M3-02**：未提交改动不会随 git 走。二选一：(a) 在旧机器等 Codex 完成、审阅、提交并推送后再拉取；(b) 直接在新机器让 Codex 按任务文件从头/续做（先 `git status` 看有无残留）。审阅要点：workspace 与 `nsh test` 全绿、5 张浅色截图（历史、统计含热图与小时分布、日志查看器、通知中心、Dock 实心爱心）、与参考版 JSON 兼容、通知入口位置、图表实现方式、STATUS F12/F16 与 ≤40 行小节；根目录 Preview 文件是否应归位到 `src/` 下需审阅。
- M3-02 通过后：提交 “M3-02: history, local stats, logs and notifications”，HANDOFF 记 M3 完成，再按 `specs/MILESTONES.md` 拆 M4（推荐/分类/搜索/详情页、网络播放、在线歌词、在线歌单历史写入、远端收藏刷新）。
- 累积待人工确认（给用户汇总）：M1 媒体键/控制中心/关闭隐藏后播放/Dock 恢复/Cmd+Q/防息屏/音效听感/拖动手感；M2 真实 Subsonic、飞牛（中继与安全码）、QQ/微信扫码、短信风控、系统文件选择器、编辑器保存热重载；M3-01 控制中心 favorite、真实 Cmd+Q 冲刷。
- 小项待后续 UI 任务：二维码过期置灰；开发者结果树空 `warnings` 不显示展开箭头；Rust `animated_value` 真实 host 不推进（未查明）。

## 已完成

| 阶段 | 提交 | 内容 |
|---|---|---|
| M0 | `77bb293` `f0c9c5c` `d70465f` `b03c3d5` | 骨架；自建 `haiku-jsrt`（基于 NSH 的 quickjs-jit，取代 CJRT，D11）；迁移参考版 Rust 核心到各领域 crate；平台验证 S1–S7（S8 多窗口不可行 → 设置改为覆盖层，D18）；S4/S5 经用户人工确认 |
| M1-01/01b/01c | `cf3fde5` `d7291cf` `e041101` `b364b75` | 设计系统、主题 token（17 品牌色，默认橙）、组件、外壳；用户已确认 UI。`b364b75` 由用户直接让 Codex 改：后退/前进/搜索在侧栏，下载/设置在侧栏底部 |
| M1-02 | `6631b82` | 播放核心：Queue、Controller（generation、预加载、错误跳曲）、睡眠定时、会话恢复；`Core`/`core.player` 导出与 CoreBridge（位置只走 Rust signal）。workspace 177 测试通过 |

## 2026-11-11 更新：M5-03 第一部分（账号服务、统计上传）已验收并提交

- `haiku-services::{account,stats_upload}` 与 `core.account`（snapshot/sendCode/login/logout/refresh/subscription/plans/setServer/onChange）；令牌存 Keychain（新增 macOS 依赖 `security-framework` 3.7.0），测试用内存凭据与 loopback 假服务端。workspace 282 通过/1 忽略，nsh test 61/61。
- 未完成：§3 设置“账户”界面、账户 Headless、两张截图。待人工确认：真实服务端、Keychain 授权、重启恢复。

## 2026-11-11 更新：M5-02 已验收并提交

- 自动下载歌单（本地与收藏，跟随移除双条件）、更换下载目录（移动/保留/取消，暂停调度，逐文件报告）。`core.downloads` 新增 changeDirectory / directorySnapshot / onDirectoryChange。审阅者复跑完整 nsh test 61/61。
- 遗留：`download_management::watch_collections` 每 100ms 轮询，宜改为等待已有变更通道。
- 下一步 M5-03（`specs/tasks/M5-03-account-and-stats-upload.md`）：Haiku 账号与统计上传；之后 M5-04 Gist 同步与备份。

## 2026-11-11 更新：M5-01 第二部分（下载界面）已验收并提交

- 下载页（状态筛选、虚拟列表、进度、批量与移除确认）、设置“下载”分组、TrackList 菜单/多选/详情页“下载全部”入口。审阅者修正测试选择器（菜单项是 `role:menuitem`，`role:button[下载]` 会命中侧栏导航），完整 nsh test 53/53。
- 下一步 M5-02（`specs/tasks/M5-02-auto-download-and-directory.md`）：自动下载歌单、更换下载目录。之后 M5-03 账号与统计上传、M5-04 Gist 同步与备份。

## 2026-11-11 更新：M5-01 第一部分已验收并提交

- 下载 Rust 服务与 `core.downloads`（`native/app/src/{download_service,downloads}.rs`）：任务记录 `<data>/haiku_state/downloads.json`（参考版格式），设置沿用参考版 `download*` 字段，默认目录 `~/Music/Haiku`。
- 审阅发现回归：Player 切歌后本地歌词不加载。根因是歌词 effect 依赖整份设置，下载设置写入使 effect 重跑并取消挂起的读取（NSH 重跑 effect 会取消其 spawn 的任务）；Codex 改为 `computed` 比较请求输入并加受控回归测试。审阅者复跑完整 nsh test 50/50。
- **注意**：effect 内 spawn 的任务会在 effect 重跑时被取消，effect 只能读真正决定请求的信号。
- 未完成：§2 界面（菜单“下载”、下载页、设置分组）与两张截图；§3 自动下载歌单、更换目录（M5-02）。高清封面只处理含 `{size}` 的 URL。
- 已知偶发：Search 命令面板 Escape、jsrt 并发用例在满载全量中偶发超时。
- Codex 周额度剩约 2%。

## 2026-11-11 更新：M4-05 已验收并提交，M4 完成

- 远端收藏刷新（`favorite_refresh.rs`：离线快照优先、完整分页原子换版、单飞/离页取消/迟到丢弃、取消收藏确认）；刮削服务 `scrape_service.rs` 与 `core.scrape`（start/cancel/jobs/review/accept/skip/undo/retry/onChange）；覆盖写 `library/overrides.json`，审阅存 `<data>/scrape_review.json`（参考版格式双向转换）；设置位于“存储 → 元数据刮削”；歌词逐源错误中文化。
- 证据：workspace 260 通过/1 忽略，完整 nsh test 43/43，check/build 通过；四张浅色截图（`.artifacts/m4-05/`）。
- 偏离：音乐来源候选默认关闭（参考版默认开）。遗留：参考版高级关键词清洗/文件名回退、专辑艺术家/碟号、刮削封面本地缓存；重开 Core 后审阅 UI 无 Headless。UI 小项：审阅列表项居中、“年份”差异行空白、来源显示内部 ID。
- 待人工确认：真实刮削上游、标签写回实机流程、真实来源收藏刷新。
- 下一步：M5（下载 → 账号与统计上传 → Gist 同步与 `.haikubak` 备份），按 MILESTONES.md 拆任务。Codex 周额度剩约 3%。

## 2026-11-11 更新：M4-04 已验收并提交

- 第一部分 `1dfae04`：来源链（手动 > 缓存 > 本地 > 插件 > 提供方）、六个内置提供方、`core.lyrics`、`<data>/lyrics/` 缓存。第二部分：查找歌词对话框（正在播放上的“查找歌词…”直达按钮）、设置“歌词”分组、M4-03 三项 UI 小项。
- 审阅定位：NowPlaying 条件挂载会破坏常驻（已撤回）；`initialNowPlaying=true` 初始即打开时组件事件不到达 AppShell（仅夹具路径，Dock 打开正常），列为已知问题。`service::set_track` 中冗余的 `cancel_preview()` 导致切歌后本地歌词不加载，由审阅者删除（一行，preview 已监听曲目取消）。
- 审阅者复跑：workspace 252 通过/1 忽略，check/build 通过，完整 nsh test 33/33；五张浅色截图。
- 遗留：Paxsenix 专用评分/令牌重取/content 兜底、Kugou 署名裁剪；initialNowPlaying 事件问题。待人工确认：真实歌词上游可用性。
- Codex 周额度剩约 3%，下一步 M4-05（远端收藏刷新、刮削 UI）需等额度恢复。

## 2026-11-11 更新：M4-03 已验收并提交

- Codex 修复：推荐页 tracks 货架 TrackList 补 `flex-1 min-h-0`（高度塌陷）；电台频道在来源描述符加载前请求导致永久为空。审阅者复跑完整 nsh test 29/29；workspace 247 通过/1 忽略；check/build 通过；五张浅色截图（推荐、分类 tag 墙、分类歌单网格、电台、命令面板“标题 · 歌手”）已审阅。
- Search 命令面板 Escape 用例在 Codex 一次全量中超时，单跑 2/2 两次、审阅者两次全量均通过，判为满载偶发；若再现单独排查。
- **锁屏会让 Player/真实音频类 Headless 停滞、截图失败**——跑 nsh test 与截图前确认 `ioreg -n Root -d1 | grep IOConsoleLocked` 为 false。
- 小项待后续 UI 任务：分类歌单页标题显示原始 ID（“cn”）；电台未选频道为纯文字，与已选按钮样式不一致；推荐页货架卡片间距偏大。
- 待人工确认：真实来源首页/分类/电台连续播放。
- Codex 周额度剩约 4%，下一步 M4-04（远端歌词与来源链、手动查找歌词）需等额度恢复。

## 2026-11-11（已解决）M4-03 验收曾被锁屏阻塞

- Codex 已完成推荐/分类/电台与 M4-02 小项；插件契约两项由审阅者决定并写入 PLUGIN_SDK.md §5.2：`categories.list` 可选 `sorts`，`radio.next(channelId?: string)` 中 undefined 为私人 FM。workspace 247 通过，check/build 通过。
- `nsh test` 卡在 Player.test.tsx 的 persistent dock 用例（点歌词跳到 0:10 后停住）。审阅者在干净工作树 `/tmp/haiku-nsh-baseline`（4b000a2）上同样卡住 15 分钟，说明不是 M4-03 回归。当时（02:30）屏幕已锁定（IOConsoleLocked=true），怀疑锁屏下真实音频/渲染不推进；截图也无法在锁屏下拍。
- 解锁后：先在基线工作树重跑 `nsh test Player.test.tsx` 确认能通过，再让 Codex 完成完整 nsh test、五张截图与 STATUS，然后审阅提交。
- 注：M4-03 代码已由用户以 `625319a` 提交并推送（非审阅提交），验收仍未完成。
- 2026-11-11 MacBook Air（未锁屏）复核：`nsh test Player.test.tsx` 2/2 通过（3.3s），停滞确认是锁屏所致。审阅者自跑：workspace 247 通过/1 忽略，nsh check/build 通过，`nsh test` 28/29——失败为 `Discovery.test.tsx` 首个用例：推荐页 tracks 货架标题在、曲目行（“潮汐之间”）不在屏幕上，疑原生 TrackList 在货架中高度为 0。已派 Codex 修复 + 五张截图 + STATUS（Codex 周额度剩 4%）。日志在 `.artifacts/m4-03/{player-unlocked,cargo-test-workspace-final,nsh-check-final2,nsh-build-final2,nsh-test-final}.log`。

## 2026-11-10 更新：M4-02 已验收并提交

- 搜索页（五种实体、来源筛选、即时查询 + 显式提交才写历史、加载更多）、命令面板多来源预览（280ms 防抖、起播用该来源完整第一页）；导出 `core.sources.search`、`core.sources.searchPreview`。旧响应按 (source, kind, query) + 递增版本丢弃。
- workspace 244 通过，nsh test 25/25（输入替换改用 `app.clear`，经审阅者授权超限修复）；五张浅色截图已审阅；M4-01 遗留排版已修。
- 小项待后续 UI 任务：命令面板曲目行只显示“播放 标题”，缺歌手；库来源浏览分组间距仍偏大。
- 待人工确认：真实来源搜索、小窗口长列表、实机键盘。
- 下一步 M4-03：推荐、分类、电台。

## 2026-11-10 更新：M4-01 已验收并提交

- 网络播放：候选 `Local(PathBuf)` / `Network{url,headers,format,quality,expires_at}`；Controller 通过注入的地址解析 trait 和时钟编排（换候选、Expired 重取、1/2/4 秒重试、30 秒清零、提前 EOF 恢复），app 层由 SourceApi 实现；缓存键 `(EntityRef, quality)`，沿用 `audioQuality`，默认 high。审阅者已批准该接口（与 PLAYBACK.md §3 一致）。
- 通用详情页（歌单/专辑/歌手/榜单）、库来源浏览（侧栏列出带 library 能力的来源）、远端歌单收藏、歌单浏览历史；M3-02 的 UI 小项已修。
- workspace 243 通过，nsh test 22/22（选择器歧义经审阅者授权第三次修复）；loopback 真实播放进度推进到 23 秒，五张浅色截图已审阅。
- 小项待后续 UI 任务：库来源浏览的卡片标题相对封面缩进、分组间留白过大；库来源侧栏图标与“电台”相同；详情“已加载 1 首/加载更多”离列表很远，贴在页面底部。
- 待人工确认：真实 Subsonic/飞牛/QQ 播放、听感、真实缓存二次命中与故障恢复。
- 下一步 M4-02：搜索页与命令面板搜索。

## 2026-11-10 更新：M3-02 已验收并提交，M3 完成

- 本机由 Codex 补齐并重新生成证据：workspace 238 通过/1 忽略，nsh test 21/21；五张浅色截图（历史、统计含 12 周热图与 24 小时分布、日志查看器含脱敏、通知中心、Dock 实心爱心）已审阅。
- 通知入口：顶栏“更多 → 通知中心…”及原生“视图”菜单；日志查看器在设置“其他”。图表为有界 TSX View，聚合在 Rust。参考版 history/stats JSON fixture 可读。
- NSH 0.4.0 嵌套 TSX 入口不加载根 `nsh.json`，所以 Preview 入口必须留在仓库根目录（三行薄入口），测试已移到 `src/pages/fixtures/`。NowPlaying 布局测试在 cfg(test) 中改读 on_layout 的 payload，断言不变。
- 小项待后续 UI 任务：通知/日志时间显示原始 ISO（UTC、微秒），应本地化；通知行按钮文字带序号（“清除通知 3”，疑为无障碍标签外露）；日志查看器在“其他”中同时有按钮和内联区块，重复；热图只占左侧一小块。
- 待用户决定：应用许可证（关于页目前写“未声明”；QQ 插件 QRC 部分为 GPL-3.0）。
- 待人工确认：真实鼠标/键盘、原生保存选择器、剪贴板、长期真实播放统计。
- 下一步：按 `specs/MILESTONES.md` 拆 M4。

## 2026-11-10 更新：M3-01 已验收并提交

- collections 服务（串行修改、500ms 防抖原子写盘、Core 生命周期与应用退出钩子冲刷）、`core.collections` 导出、侧栏“喜欢/歌单…”、喜欢页、用户歌单页（四格合成封面）、TrackList 右键喜欢/加入歌单、Dock 喜欢、媒体中心 favorite。workspace 234 通过，nsh test 20/20。
- 测试隔离：Core 新增 `openIn`/`openIsolated`，数据根按引擎保存；新测试与预览一律用独立临时目录。
- 遗留并入 M3-02 第 0 步：Dock 已喜欢时爱心应为实心（与列表一致），目前是强调色描边。右键子菜单展开截图因不做 GUI 自动化未拍，行为由 Headless 覆盖。
- 待人工确认：控制中心 favorite 点击、真实 Cmd+Q 冲刷。

## 2026-11-10 更新：M2-05 已验收并提交，M2 完成

- qq-music v2（`dev-plugins/qq-music`，327 行，契约 34/34，不默认安装）、开发者面板（500ms 内容哈希轮询热重载、描述符参数表单 + 原始 JSON、日志/HTTP 追踪）、插件 logo（沿用参考版图标）与系统文件选择器、Rust 生成二维码（qrcode 0.14.1）、`sdk/GUIDE.md`（7 节，以 subsonic 为例）。workspace 228 通过，nsh test 17/17。
- 小项待后续 UI 任务：二维码过期时应置灰/遮罩；开发者结果树空 `warnings` 不应显示展开箭头。
- 待人工确认：真实 QQ/微信扫码、短信风控、VIP/地区权限、系统选择器、编辑器保存触发热重载。

## 2026-11-10 更新：M2-04（部分）已验收并提交，剩余转 M2-05

- 完成：timers、feiniu FNID/中继/access-code、契约 detail 分页与裸 fetch 拦截；`core.sources`/`core.plugins` 导出；设置“来源/插件/开发者”面板（UI 修订后四张浅色截图已审阅）；设置左上橙色细线（隐藏外观面板 Radio ring 残留）已修。workspace 224 通过，nsh test 14/14。
- web 登录降级：NSH 0.4.0 webview 无独立会话、无法读 cookie，改为手动粘贴 Cookie/完成 URL 再调用 `auth.web.done`。M6 或 NSH 升级后复核。
- 转 M2-05：qq-music v2、`sdk/GUIDE.md`、开发者面板目录监听与自动参数表单、插件 logo 渲染、系统文件选择器导入、URL 转二维码、QR/SMS/web 面板与订阅 Stop 的 UI 回归、插件行无作者时不显示“未提供作者”。
- 待人工确认：真实 Subsonic/飞牛（含中继与安全码）连接、平台账号登录、键盘焦点与滚动。

## 2026-11-10 更新：M2-03 已验收并提交

- M2-02 遗留补齐；feiniu（237 行）、qq-scraper、kugou-scraper v2；刮削编排（`haiku-plugins::scraper`）；`native/tools/haiku-plugin` CLI（new/check/test）；契约套件 `haiku-plugins::contract`（JSON 报告到 stdout，摘要到 stderr）。workspace 221 通过；四个内置插件与三个模板契约全部通过；nsh test 13/13。
- 遗留并入 M2-04 第 0 步：feiniu 缺 FNID/中继/access-code 远程连接（参考版有，属功能缺口）；jsrt 未注册 timers/全局 setTimeout；契约套件未遍历 detail.tracks 分页、replay 未拦截裸 fetch、--record/--live 只有基础实现。

## 2026-11-10 更新：M2-02 已验收并提交

- haiku-plugins 宿主（描述符、安装事务、sources.json 兼容参考版、认证状态机、AuthExpired 单飞刷新、SourceApi）、`haiku` SDK、`sdk/haiku.d.ts`、`plugins/subsonic/plugin.ts`（297 行，id `subsonic.music`）。workspace 210 通过，tsc 通过，nsh test 13/13。
- 插件 id 必须沿用参考版（`subsonic.music`、`feiniu.music`、`qq.scraper`、`kugou.scraper`、`qq.music`），sourceId 与参考 `deriveSourceId` 一致（审阅者用 Node 复算过）。
- 遗留并入 M2-03 第 0 步：Shelf/Link 等结构校验、100 页聚合、SMS cooldown、QR 到期抢占、描述符认证展示元数据、cookie 罐/SMS/web/logout 回归、坏包隔离与安装崩溃恢复。

## 2026-11-10 更新：M2-01 已验收并提交

- M2 拆为 M2-01 运行时 → M2-02 宿主 + `sdk/haiku.d.ts` + subsonic → M2-03 其余插件、CLI、契约套件、设置面板、GUIDE。
- M2-01：haiku-jsrt R1–R12 完成；jsrt 24/24、workspace 203 通过/1 忽略、NSH 同进程并发测试、nsh check/build/test 12/12。
- 审阅决定：接受 crypto/fetch 子集（GCM 12 字节 IV、OAEP SHA-1/256、响应体 16 MiB、无 ReadableStream/FormData）；cipher 分段 update 每次重算累计输入的性能问题列入 M2-02 第 0 步。rsa 为 0.10.0-rc，M6 前复核。
- 仓库根 `.cargo/config.toml` 关闭 build-override strip，普通 `nsh run` 不再需要手动 export。
- Codex `/new` 会切到 `~/.codex/worktrees/` 下的独立工作树；派发时需明确要求在主仓库路径工作。

## 2026-11-10 更新：M1-05 已验收并提交，M1 完成

- M1-05：设置服务（`core.settings`，schemaVersion 1，保留参考版字段名与未知字段）、外观持久化、十段 EQ/效果/响度均衡、媒体缓存接线（`<data>/media-cache/`，DATA.md 已对齐）、设置“播放”“存储”分组、媒体中心/远程命令、防息屏。workspace 189 通过/1 忽略，nsh test 11/11，浅色截图已审阅。
- 审阅决定：软削波与 8ms 渐入渐出作为固定保护、不提供开关；huacnlee/cargo 忽略 target-dir 提示列为已知例外。
- 遗留：设置文件损坏时已备份，但 UI 通知未接（随 M5 通知/诊断一并处理）；设置页内容区左上有一处橙色细线，疑为 Sheet 进度/焦点残留，下次 UI 任务顺带查。
- 待人工确认：媒体键与控制中心按钮、关闭隐藏后继续播放、Dock 恢复、Cmd+Q、防息屏、音效听感。
- Codex 本周额度剩约 15%，下一步 M2（插件 SDK）或 M3（持久化）按额度安排。

## 2026-11-10 更新：M1-04 已验收并提交，下一步 M1-05

- M1-04：Dock、SeekBar、正在播放覆盖层、本地歌词、封面配色完成；nsh test 10/10、workspace 测试通过、浅深 Dock + 设置 + 正在播放截图已审阅。
- 已知缺口：真实 host 中 Rust 组件的 `animated_value` timing 不推进（Headless 正常；nsh-base Sheet 与自绘动画都卡在起点），正在播放覆盖层因此直接显示、无入场动画。Settings 覆盖层（TSX Sheet）正常。原因待查，疑与 CoreBridge 每帧 `next_frame` 有关；后续单独复核。
- 待人工确认：拖动手感、听感、队列/音量弹层。

## 2026-11-10 更新：M1-03 已验收并提交（62e85c2）

- M1-03 由 Codex 完成，审阅者已核对：workspace 180 通过/1 忽略，nsh test 7/7，真实扫描与播放日志、浅深截图（`.artifacts/m1-03/`）。unknown classes 在正式入口不复现。
- 遗留小项并入 M1-04 第 0 步：序号列颜色、`nsh.json` 测试 fs 授权。
- M1-04 任务：`specs/tasks/M1-04-player-dock-and-now-playing.md`。

## 已归档：M1-03 本地曲库 + TrackList（半成品已提交，尚未验收）

- 任务说明：`specs/tasks/M1-03-local-library.md`，含本机续做说明。
- 半成品保存于 `84c2a60`：已删除 `PlaybackSmoke.tsx` 与临时 Core 试播方法，新增 `haiku-library/src/service.rs`、可取消扫描和 `native/app/src/track_source.rs`；任务文件、UI 布局注记和 handoff 也已纳入。2026-11-10 本轮开始时工作区干净。
- 尚缺：`track_source` 模块注册、`core.library` 导出/事件桥、正式 TrackList、本地曲库 TSX 页面及验收证据。STATUS 的 F10 仍为未实现，不能因已有提交就进入 M1-04。
- 继续方式：先读 `84c2a60` 和现有半成品，再按任务文件补齐；本轮只保留改动和证据，不自动提交。审阅者本轮更新的任务/handoff 不覆盖。
- 待查：M1-02 试播出现 `unknown classes`（侧栏 token 类），仍按 M1-03 第 0 步处理。

## 后续顺序

M1-04 底栏 PlayerDock、SeekBar、正在播放覆盖层、本地歌词、封面配色 → M1-05 音效/响度/媒体缓存接线、设置的外观/播放/存储分组持久化、媒体中心与媒体键正式接入 → M2 插件 SDK（可与 M3 并行）→ M4 在线内容 → M5 下载/账号/同步/备份 → M6 macOS 深度集成。详见 `specs/MILESTONES.md`。

## 已知事项

- 允许的构建警告：`block 0.1.6` future-incompat、开发 host 的 `__eh_frame`、huacnlee/cargo 忽略 target-dir 提示。
- `haiku-macos` 三处 `clippy::mut_from_ref` 局部豁免，M6 复核。
- 玻璃底栏需用 `WithinWindow` 混合模式并覆盖滚动内容，M6 验收；M1 用 NSH 半透明回退。
- 截图前确认屏幕未锁定（锁屏时 `screencapture` 失败）。
- 本机（macOS 27，rustc 1.97.1）构建期依赖默认 `-C strip=debuginfo`，剥离后的 proc-macro dylib 被 dyld 拒绝（`mis-aligned LINKEDIT string pool`），表现为 `E0463 can't find crate for rquickjs_macro`。需关闭 build-override 的 strip（`CARGO_PROFILE_DEV_BUILD_OVERRIDE_STRIP=false` 或 profile 配置），清缓存无效。
