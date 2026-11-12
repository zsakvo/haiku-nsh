# F01–F16 审计（M6-06 第一步）

审计日期：2026-11-12。代码基线：`ba58330`（M6-07 QQ 重写已提交）。在 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作；参考版 `/Users/zsakvo/Project/GPUI/haiku` 只读。

依据：参考版 `specs/SPEC.md` §3、§6 及 `ACCEPTANCE_{UI,NATIVE,SOURCES,SERVICES}.md`；本仓库 `specs/SPEC.md` D1–D18、`HANDOFF.md`、代码、测试断言和已有 `.artifacts`。本轮未运行测试、live、构建或 GUI，不修改实现。

## 判定与证据边界

- 子项使用任务规定的五种结论：**已实现并有证据 / 已实现缺证据 / 部分实现 / 未实现 / 按 SPEC 决定不做**。代码存在、测试文件存在、历史验收记录、当前机器原始日志分别说明。
- 表中“有证据”只表示列出的断言或场景曾执行通过，不表示真实账号、硬件操作或全部异常组合已验证。旧日志不能证明 `ba58330` 的完整应用基线仍全绿。
- STATUS 功能表沿用三态：没有功能缺口且有对应证据可列“已验证”；存在未接线或缺少关键原始证据则列“部分实现”。每个 F 的汇总结论不能替代其子项表。
- 所有实现路径相对本仓库。`haiku-xxx/src/` 简写指 `native/crates/haiku-xxx/src/`；`app/` 指 `native/app/src/`。缺口编号 G01–G23 见文末，跨 F 共用编号。

| 证据索引 | 本机可复核内容及限制 |
|---|---|
| E1 | `.artifacts/m5-05/cargo-workspace.log`：291 通过、1 忽略；含队列/网络播放、DSP、扫描、歌词、插件、收藏、历史/统计、账号、同步/备份、下载、刮削。`.artifacts/m5-05/nsh-test.log`：64/64，含当时的 Rust Headless 和 TSX。均早于 M6。 |
| E2 | `.artifacts/m1-03/` 扫描/曲库；`.artifacts/m1-04/nsh-test-static-overlay.log`、`now-playing-static-overlay.png`、`library-dock-{light,dark}.png`；`.artifacts/m1-05/nsh-test-complete.log`、`settings-{playback,storage}-light.png`。 |
| E3 | `.artifacts/m2-03/cli-test-{subsonic,feiniu,qq-scraper,kugou-scraper}.log`；`.artifacts/m2-04/` 认证/设置；`.artifacts/m2-05/nsh-test.log`、`{plugins,developer,qrcode}.png`。旧 QQ 的 34/34 不作为现版 QQ 证据。 |
| E4 | `.artifacts/m3-01/nsh-test-revision-repair.log` 20/20、`.artifacts/m3-02/nsh-test-repair1.log` 21/21 及 m3-02 的 `{history,statistics,logs,liked-dock}.png`；最终回归另有 E1。 |
| E5 | `.artifacts/m4-03/nsh-test-review.log` 29/29、`{home,categories-tags,categories-playlists,radio,command-tracks}.png`；`.artifacts/m4-04/review-nsh-test.log` 33/33、`review-workspace.log` 及歌词截图；`.artifacts/m4-05/nsh-test-full.log` 43/43、`{favorite-refresh,scrape-settings,scrape-progress,scrape-review}.png`。 |
| E6 | `.artifacts/m5-02/nsh-test-reviewer.log`、`.artifacts/m5-03/nsh-test-ui-full.log` 62/62、`{account-signed-in,account-signed-out}.png`；`.artifacts/m5-04/nsh-test.log`、`.artifacts/m5-05/nsh-test.log` 及各自同步/备份截图。 |
| E7 | `.artifacts/m6-07/cli-check-complete-types.log`、`cli-test-complete-types.log` 59/59、`tsc-all-plugins-final-types.log`（成功空输出，结合 STATUS 原运行记录）、`cargo-test-haiku-plugins-final-tidy.log` 22/22。`cli-live.log` 8/9 为公开能力旧记录；榜单 limit=1 触及契约器 100 页上限，不能据此判插件故障，也不能称 live 全过。 |
| H | HANDOFF/STATUS 记载 M0–M6-05 已审阅提交，M6-05 审阅复跑 workspace 304、nsh test 76/76。本机缺 `.artifacts/m0-*`、`m1-01*`、`m1-02`、`m4-01`、`m4-02`、`m6-01`～`m6-05`；尤其 M6 菜单栏/桌面歌词/玻璃/release 原始记录及产物无法本机复核。不能把不存在的路径当 E1–E7 的执行证据（G01）。 |

## F01 桌面外壳与导航

汇总：**部分实现**。核心外壳与已接入页面有证据；“我的”用户内容入口缺失，最终窗口/系统交互证据不齐。

| 参考版 §3 / §6 要求点 | 实现、测试、证据 | 判定 / 边界 |
|---|---|---|
| 推荐/分类/电台/搜索/本地/喜欢/歌单/历史/统计/下载/设置、来源/插件、通知/日志入口；游客 | `src/shell/AppShell.tsx`、`Settings.tsx`、各 `src/pages/`；设置内来源/插件/账户，顶栏通知、设置日志；E1/E3–E6 | 已实现并有证据；入口按 D7 重组，来源不是固定占位。 |
| 第三方“我的”及用户歌单 | `SourcesSettings` 只展示登录信息；`SourceApi::me_playlists/liked_tracks` 无 app 调用；无对应 route/page | 未实现；不以 Haiku 账户设置替代第三方用户内容（G14）。 |
| 根部单一 Dock、整窗正在播放、队列 | `app/{player_dock,now_playing}.rs`；`Player.test.tsx`、NowPlaying 几何测试；E1/E2 | 已实现并有证据；队列操作缺项见 F06。 |
| 前进/后退、100 条历史、新导航截断 forward、页面保留 | `src/shell/navigation.ts`、AppShell 常驻页面与按历史保留详情；`App.test.tsx`、Search/Discovery 回归；E1/E5 | 已实现并有证据；未把保留组件状态推导为所有页面滚动位置均已专项验证。 |
| 实体详情可达、包含 # / URL 的 ID 无损 | `EntityRef::{encode,decode}`、`PluginService::reference/detail`、`SourceDetailPage`；`haiku-model` 测试、`Network/Search/Discovery.test.tsx`；E1 | 已实现并有证据；节目使用 v2 `program`，不要求保留 v1 `programAlbum` ABI（D9）。 |
| Cmd+K、280ms、每源 3+3、完整第一页起播、上下/Enter/Escape | `CommandPalette.tsx`、`app/command_keyboard.rs`、`sources::search_preview`；`Search.test.tsx`；E1/E5 | 已实现并有证据；Escape 满载偶发在 HANDOFF 留档，不据单次通过声称已消除。 |
| 窗口缩放/最小化/关闭隐藏/恢复、Cmd+W、Cmd+Q；右键菜单 | AppShell 原生菜单、`haiku-macos/{lifecycle,shortcuts}`、`app/exit.rs`、TrackList；E1/E2，H | 部分实现；右键已有证据，最终系统生命周期待人工；M6 原始证据缺失（G01）。 |

## F02 插件包与来源生命周期

汇总：**已实现并有证据**（v2 范围；真实选择器与编辑器热重载体验待人工）。

| 要求点 | 实现、测试、证据 | 判定 / 边界 |
|---|---|---|
| 原 JS+manifest / ABI1 包格式 | `specs/SPEC.md` §3、D9/D10；`host::import_location` 旧格式拒绝，备份跳过并报告 | 按 SPEC 决定不做；仅 v2 `.ts`，不能按参考版旧 ABI 判缺口。 |
| 本地文件/目录/ZIP/HTTPS、校验、10MiB/30s | `haiku-plugins/{host,descriptor}.rs`；`import_location/install_directory/install_zip`；宿主坏包/旧格式/路径测试、E1/E3 | 已实现并有证据；未覆盖全部恶意 ZIP 和断电组合。 |
| 包/实例分离、升级/同版本代码/降级、storage 与配置保留、失败回滚/启动恢复 | `host.rs` 安装事务及 journal；`tests.rs` 安装/回滚/恢复断言；E1/E3 | 已实现并有证据；进程崩溃恢复不等于断电持久性保证。 |
| 四内置身份、开发 provider 显式导入、provider 单例/library 多实例 | `Host::open/activate/connect`、`source_id`；SHA1 keyFields 顺序及 URL 规范化 golden 测试；E1/E3 | 已实现并有证据；QQ 仍在 `dev-plugins`，未默认安装。 |
| 实例独立凭据、添加/编辑/切换/改名/启停/排序/删除、包在用保护 | `host.rs`、`SourcesSettings/PluginsSettings`；PluginSettings Headless、宿主生命周期测试；E1/E3 | 已实现并有证据。 |
| 描述符能力/角色决定入口；日志、表单、调试、脚手架、指南 | `app/plugin_tools.rs`、`DeveloperSettings`、`native/tools/haiku-plugin`、`sdk/GUIDE.md`；CLI 模板/契约、Stop、参数表单测试；E1/E3 | 已实现并有证据；500ms 内容哈希轮询是当前实现，record/live 为基础能力，不声称完整录制产品体验。 |

## F03 来源认证与用户内容

汇总：**部分实现**。认证状态机与 QQ 新版契约有证据；D14 回退和用户内容桌面链路未完成。

| 要求点 | 实现、测试、证据 | 判定 / 边界 |
|---|---|---|
| password/API key/动态表单、Cookie、SMS 冷却/challenge | `haiku-plugins/source.rs`、`SourceLogin`；`tests.rs` 状态机、PluginSettings Headless；E1/E3；QQ E7 | 已实现并有证据；API key 用声明字段表达；QQ SMS 安全校验返回改用扫码的错误提示，真实风控未验。 |
| QR 变体/刷新/轮询/到期/取消，失败取消不留半来源 | `LoginDraft/QrSession`、`SourceLogin`、`plugin_tools::qr_image`；过期优先/取消/草稿清理与 UI 测试；E1/E3/E7 | 已实现并有证据；真实扫码见人工清单。 |
| web 独立会话、HttpOnly、自动完成/取消 | `SourceLogin` 手工粘贴 Cookie/URL 后调用 `web_done`；`haiku-macos/web_auth.rs` 有隔离 Cookie 底层 fixture，但未接桌面登录 | 部分实现；D14 要求 NSH 不足时接 WKWebView 回退，当前手工方案不是 D14 的等价完成（G14）。 |
| auth/me 成功刷新、退出、凭据日志脱敏 | `source.rs`、`plugins::login_step/logout/changed`、`logs::redact`；auth/me、刷新单飞、logout/日志断言；E1/E3/E7 | 已实现并有证据；刷新来源快照不等于刷新尚不存在的“我的”页面。 |
| 第三方资料/自建与收藏分组/用户喜欢、用户歌单分页 | QQ `plugin.ts::me` 输出 `owned`，`types.ts` 保留别名来源；SDK `me_playlists/liked_tracks`；CLI 59 项包含该能力，E7 | 部分实现；插件和宿主 API 有，app 无桥接/页面/登录后列表刷新（G14）。 |
| 专辑/歌手/播客节目详情与音频 headers | `PluginService::detail`、`SourceDetailPage`、`SourceApi::streams`；Network/Search fixture，E1/E5；QQ E7 | 已实现并有证据；真实平台内容与 VIP 权限未验。 |
| 受保护封面 headers、收藏/历史重启及跨端恢复 | SDK/模型封面仅 URL；`app/sources::artwork` 只合并静态 descriptor headers；网格直接 `Image src`；`StoredTrack` 可读旧 coverHeaders 但从 Track 新建时无此信息 | 部分实现；不能据音频 headers 测试认定封面鉴权已完整覆盖。逐图动态凭据及私有缓存恢复链路缺失（G18）。 |
| 新结构 QQ 功能/特殊约定 | `dev-plugins/qq-music/{plugin,api,auth,types,crypto}.ts`；`auth.ts` 两步 manual fetch 传 signal，读取 302 Cookie/Location；E7 | 已实现并有证据；无旧 protocol.js。manual fetch 为审阅者明确批准的 SDK 不扩展例外；59 项 replay 不是实账号通过。 |

## F04 推荐、发现和分类

汇总：**部分实现**。单来源推荐、分类和持续电台有证据；聚合与首页分页未接通。

| 要求点 | 实现、测试、证据 | 判定 / 边界 |
|---|---|---|
| 启用来源聚合、顺序、选择、分源错误隔离 | `HomePage/useSource` 保留每源 feeds/errors、按选择加载一源；来源排序由 Host 保存；Discovery 测试 E1/E5 | 部分实现；无全来源聚合 feed。`specs/UI.md` §3 写单来源，与参考 SPEC F04 聚合要求存在范围冲突；D1–D18 未明确取消聚合，需审阅者确认（G15）。 |
| 缓存先显/后台刷新、7 天、40×60、不存 cursor、普通刷新保留 radio | `app/discovery.rs::HomeCache/bounded`；`home_cache_expiry_caps_radio_retention_and_cache_first`、Discovery offline；E1/E5 | 已实现并有证据。 |
| 七类首页内容、hero/快捷动作/查看更多及正确路由 | v2 `Shelf/Entry/Link`、`ui/Discovery.tsx`、HomePage；实体卡/曲目/分类和电台 entry/更多路由测试，E1/E5 | 已实现并有证据（v2 表达）；D9/D7 允许 ABI 与视觉重建，不要求旧 hero 组件逐像素复刻。 |
| 首屏 2 页、续页 1 页、空货架补齐/分页 | QQ home 输出 next；`HomeCache::home` 只 `api.home(None,…)`，`bounded` 清掉 next，HomePage 无加载下一页 | 部分实现；只过滤空货架，没有请求补齐或 home 分页，不能以分类分页测试代替（G15）。 |
| 分类 tag 墙、名称、排序、分页 | `CategoriesPage`、SourceApi；Discovery.test 显示名、sort/next；E1/E5/E7 | 已实现并有证据。 |
| library 动态 cards、专辑/艺人/流派/随机/最近/收藏/目录按能力 | `PluginService::browse` 遍历描述符 cards，`SourceLibraryPage` 通用分组/分页；Network fixture、插件契约 E1/E3 | 已实现并有证据（已声明 cards）；不虚构所有内置库都声明了流派/目录能力。 |
| FM 连续批次/去重、最多 10 历史 seed、来源、单飞、不随机、不跨启动恢复 | `SourceRadio`、Controller RadioSession；`radio_*`、会话/取消测试及 Discovery.test；E1/E5；QQ 私人电台契约 E7 | 已实现并有证据（单来源）；混合来源 FM 未实现，归 G15；真实连续播放待人工。 |

## F05 搜索

汇总：**部分实现**。单来源五实体和多来源命令预览有证据；搜索页聚合及在线元数据入口不完整。

| 要求点 | 实现、测试、证据 | 判定 / 边界 |
|---|---|---|
| 多来源五实体、来源筛选、错误不牵连 | `SearchPage` 五种 kind 由能力过滤，资源键含一个 source；`sources::search_preview` 并行各来源并隔离失败；E1/E5 | 部分实现；命令面板实现多源歌曲/歌单预览，搜索页仅单源，无“全部来源”五实体聚合（G16）。 |
| 提交/清空、历史去重/删除/清空、分页、更多、页面状态 | `SearchPage`、History、SourceGrid/Tracks；Search.test、history 边界测试；E1/E5 | 已实现并有证据；输入即时请求，只有显式提交写历史。 |
| 旧查询/旧类型/旧来源/旧页迟到不得覆盖新请求 | SearchPage 全身份 + epoch，CommandPalette 防抖 + epoch；受控慢响应断言；E1/E5 | 已实现并有证据；丢弃交付不等于已取消上游每条请求。 |
| 歌曲播放/下一首/入队/喜欢/加歌单/下载；其他实体详情 | SourceTracks 共用 TrackList 原生菜单；Search/Network/Downloads fixture；E1 | 已实现并有证据；不是每一入口组合都做过端到端点击。 |
| 搜索曲目详情/编辑有效元数据 | TrackList 只对全为 local 的选择提供编辑；SourceTracks 不处理编辑 | 未实现（在线曲目部分）；共享缺口 G19。 |

## F06 队列与播放控制

汇总：**部分实现**。队列与音频核心证据充分；下载优先、独立续播及若干设置/桌面入口缺失。

| 要求点 | 实现、测试、证据 | 判定 / 边界 |
|---|---|---|
| 唯一队列；播放/暂停/停止、上下首、seek、音量、静音；队列定位/删/清/重排/next/append/all | Controller/Queue、PlayerService、PlayerDock、TrackList；`tests/core.rs`；E1/E2 | 部分实现；服务支持 Stop/Clear/Move，但 Dock 仅切换/上下首/音量滑杆/跳转/移除，无停止、独立静音、清队列/重排/定位当前入口（G08）。 |
| 顺序/列表循环/单曲自然结束与手动区分；随机历史/跨轮不相邻重复；删当前/空列表/末尾 | `queue.rs`、Controller；`ref_queue_*`、`natural_repeat_one_manual_next_and_sequential_end`、删除/重排测试；E1 | 已实现并有证据。 |
| 本地直读，完整下载 > 同音质完整缓存 > 网络，坏文件逐级回退 | `network::Addresses::open` 只有 local_path→cache→resolver；app SourceResolver 只取 streams，无下载记录查询 | 部分实现；缓存/网络与损坏缓存回退有 E1，完整下载优先及其失效回退无接线（G17）。 |
| 格式 MP3/FLAC/M4A AAC/OGG、不支持格式拒绝；流式/前缀写盘 | `haiku-audio/{player,media_cache}`、`Candidate::supported`；codec/坏流/缓存单测 E1；历史 macOS 播放日志 E2 | 已实现并有证据；不是四格式全平台听感证明，真实多曲无缝仍待人工。 |
| 候选 headers/取消代次/过期重取、1/2/4 秒最多三次、稳定 30 秒重置、提前 EOF | `network.rs`、Controller；`tests/network.rs` 五项受控后端/时钟测试；E1 | 已实现并有证据；真实公网抖动未验。 |
| 错误跳过可配置；gapless 开关、下一首匹配模式、seek/换队列失效 | Controller 有界向前跳不绕回、实际 next_index/预载代次；core/network 测试 E1 | 部分实现；fail 总会有界跳曲，prepare_next 没消费 gaplessPlayback 开关；无 error-skip/gapless 桌面设置，播放音质只保存 audioQuality 无选择入口（G09）。 |
| 会话 500 条/5s/关键操作、只恢复 paused/不联网；ready 前恢复可信缓存缓冲 | Controller::start/persist、Core 先设置缓存再 start；session/paused/缓存测试 E1 | 已实现并有证据；缓冲从本地有效文件/同音质 complete 缓存校验，不信任保存值；下载路径缺口仍是 G17。 |
| 独立专辑/节目续播 | sync 保存/合并 `haiku_state/album-progress.json`；播放器/详情页无读取、记录或续播上下文 | 未实现（播放链路）；数据保留不能算续播，G12。 |
| 睡眠 15/30/45/60/90、+15、当前曲/取消/剩余/到期暂停 | Controller/PlayerService 与 sleep 测试 E1；正式 src/无调用 | 部分实现；核心已验证，设置/正在播放无 UI，G08。 |

## F07 音效与响度

汇总：**部分实现**。DSP/缓存有执行证据；完整 EQ 管理入口欠缺。

| 要求点 | 实现、测试、证据 | 判定 / 边界 |
|---|---|---|
| 十段频率、±12dB、预设/自定义、启用 | `PlaybackSettings.tsx`、Settings::AudioEffects、`haiku-audio/effects.rs`；DSP/clamp 与 Effects Headless，E1/E2 | 已实现并有证据；自定义可另存和删除。 |
| 重置、覆盖自定义预设、删除确认 | PlaybackSettings 无重置/覆盖按钮，删除立即修改设置 | 部分实现；G02，不把选择 flat 当全部参数重置。 |
| 自动/手动前级、宽度、平衡、mono，切歌保持、持久化、NaN/clipping 防护 | `haiku-playback/backend.rs::effects/configure`、effects/loudness 单测；E1/E2 | 已实现并有证据；主观听感待人工。 |
| 软削波、8ms 渐入渐出 | 固定 DSP 保护；STATUS M1-05 审阅决定 | 已实现并有证据；不提供独立开关是已批准决定，不列缺口。 |
| EBU R128/-18 LUFS、峰值限制、测量可信度/来源隔离/不降级缓存 | `haiku-audio/loudness.rs`、`haiku-playback/loudness_cache.rs` 及 backend；E1 对应单测 | 已实现并有证据。 |

## F08 歌词与正在播放

汇总：**部分实现**。歌词来源链/高频 Rust 投影有证据，在线元数据、展示设置与部分上游策略仍欠缺。

| 要求点 | 实现、测试、证据 | 判定 / 边界 |
|---|---|---|
| 封面/信息/双栏唱片、Material You Celebi+Score/HCT、过期丢弃、有界预热 | `haiku-palette`、`app/{presentation,player_theme,now_playing}.rs`；palette golden、Player/布局与 E1/E2 | 部分实现；当前/实际 next 预热存在，但通用 1600 封面与磁盘缓存上界不完整，G18。 |
| 可编辑标题/歌手/专辑/封面/歌词覆盖、重置，全部列表读有效层 | Library Patch、TrackEditor、歌词 adopt/reset；本地和手工歌词 E1/E4/E5 | 部分实现；无远端元数据 authority；历史/收藏快照不是统一有效层，且本地 Patch 没 genre/歌词编辑字段，G19/G20。 |
| LRC/KRC/YRC/TTML、原文/译文、逐行/逐字、点行 seek、居中跟随、浏览后回归 | `haiku-lyrics/{lib,formats,cursor}.rs`、`app/lyrics_view.rs` 虚拟列表/4s 浏览；解析/Player/RemoteLyrics 测试 E1/E2/E5 | 已实现并有证据；恢复本地歌词回归见 `presentation_tests.rs`。 |
| 纯音乐/无歌词、翻译显示设置 | 无歌词文字存在；LRCLIB instrumental 映射为“纯音乐，请欣赏”；NowPlaying 常规歌词直接渲染 translation | 部分实现；纯音乐路径有实现但未找到专门UI执行断言；普通歌词翻译开关缺失，桌面歌词 showTranslation 不代替主界面（G10）。 |
| 不经 JS 高频快照、仅活动行变更发布、有界挂载；暂停/缓冲/不可见停止 | CoreBridge 独立 position/buffered、Cursor 行变更 signal、Rust virtual_list；`paused_progress_is_frozen_position_not_in_low_frequency_state` 与组件 clock 测试 E1 | 已实现并有证据（架构/暂停场景）；遮挡/恢复和 release CPU 无本机完整证据，G01/G21；不能用此测试声称原生合成时间轴已实现。 |
| macOS 正在播放逐字 CoreText/CA | 当前 Rust 时钟驱动宽度；`specs/MACOS.md` §5：用户 2026-11-11 决定 KaraokeLayer 暂不实现 | 按 SPEC 决定不做（当前阶段）；H 中 debug CPU 均值 14.3% 的原日志本机缺失，不声称 release 性能已验。 |
| 手动→缓存→文件→插件→提供方、sidecar 优先/附加译文、UTF-8/GBK/UTF-16/1MiB、纯文兜底、8/25s | `local_scan::read_lyrics`、Lyrics Service、Origin；`tests/remote.rs`、本地扫描/编码/超时测试，E1/E5 | 已实现并有证据；依 D6 无会员门槛。 |
| 当前选择即加载/暂停恢复也加载；进度不重取、切歌取消、手工永存/自动依媒体缓存开关 | `presentation::bind_lyrics` computed 输入、Service generation/fingerprint；`pending_local_lyrics_survive_pause_and_download_settings_then_switch_tracks`、priority/cancel 测试 E1 | 已实现并有证据；initialNowPlaying 夹具初始开事件问题保留 G22，Dock 正常入口有通过证据。 |
| 六类内置/HTTP、排序开关、手动关键词/来源/预览/采用 | `providers.rs/service.rs`、LyricsSearch/Settings；remote 协议/Headless E1/E5 | 部分实现；Paxsenix 专用评分、令牌重取/content 兜底、Kugou 署名裁剪仍欠参考策略（G10）。 |

## F09 桌面集成

汇总：**部分实现**。M6 接线已存在且历史已审阅，但本机缺关键证据；媒体附歌词及底栏完整交互/性能语义未齐。

| 要求点 | 实现、测试、证据 | 判定 / 边界 |
|---|---|---|
| 菜单栏歌曲/逐字/滚动/高亮色、上下首/播放/喜欢/队列、鼠标真实屏幕坐标命中 | `haiku-macos/menubar.rs`、`app/menubar.rs`、`platform/menubar.rs` 单 worker/事件；`app/menubar/tests.rs`、Menubar.test | 已实现缺证据（本机 M6 原始记录缺）；历史已验收 H，真实点击/暂停/seek 仍待人工（G01）。 |
| 原生组件同 NSApplication，不装第二份；事件回主应用 | `platform/media.rs` 接 macOS event channel，D2/D3 进程内集成 | 已实现并有证据（基础接线 E1/E2）；按 D3 不再要求参考 sidecar/dylib 架构。 |
| 桌面歌词显隐/锁定/拖动/位置/屏幕限制、字体字号字重/颜色/描边/阴影/翻译/工具栏 | `haiku-macos/src/desktop_lyrics.rs`、`desktop_lyrics/geometry.rs`、`app/{desktop_lyrics,platform/desktop_lyrics}.rs`、DesktopLyricsSettings/Test | 已实现缺证据（本机）；H 已审阅，物理拖动/穿透/悬停解锁/字体面板仍待人工。 |
| 媒体中心元信息/封面/播放控制、媒体键 | `haiku-macos/system.rs`、`app/platform/media.rs`；基础 E1/E2、历史 H | 已实现并有证据（投影/注册）；最终 OS 按钮/媒体键待人工，动态封面鉴权受 G18 限制。 |
| 可选当前歌词写媒体信息 | media DTO 与设置无该字段/消费者 | 未实现；G04。 |
| 全局快捷键校验/裸字母拒绝/冲突回滚/恢复默认 | `haiku-store/shortcuts.rs`、`haiku-macos/shortcuts.rs`、`app/shortcuts.rs`、ShortcutSettings/test | 已实现缺证据（本机 M6）；历史 H；备份热恢复注册不刷新增 G07。 |
| 关闭隐藏后台播放/重新激活/Cmd+Q、防息屏 | lifecycle/exit、`platform/media.rs` IOPM 与 open/lifecycle 联动；E1/E2、H | 已实现并有证据（基础）；最终硬件/系统操作仍待人工。M0 人工确认不替代 M6 最终应用复核。 |
| 单一根 Dock、24px/96px 预留、双栏正在播放、封面深色主题及回退 | PlayerDock/NowPlaying、`src/ui/Chrome.tsx`；Player/几何 E1/E2；玻璃 `haiku-macos/glass.rs` H | 部分实现；原生玻璃背景已接但最终截图本机缺，完整底栏规范另见下一行。 |
| 全栏 1px 激活拖动/排除按钮；clear glass 轮廓；独立进度层/隐藏停钟 | 当前只有 SeekBar 区域 touch 拖动；缓冲/播放是 NSH 子 View，GlassSurface 仅背景；CoreBridge 控制时钟可见性 | 部分实现；全栏拖动未覆盖（G21）。D2/D7及本仓库 PLAYBACK §6.3、MACOS §5 采用 Rust signal/NSH进度与原生玻璃背景，不能仅因缺参考CA轨迹/350ms阈值判为必须移植旧实现；应验证等价交互与可见性/性能，当前缺最终实测。 |
| 正在播放升起/收起动画 | STATUS M1-04：用户明确批准静态显示；真实 host animated_value 不推进，Headless 正常 | 按 SPEC 决定不做（当前批准的静态方案）；底层时序原因尚未解决，G22 保留为待决优化，不作为未获批准的强制补项。 |
| Windows/Linux、macOS 14+ | D1：首版只验 macOS；其他平台系统集成 no-op | 按 SPEC 决定不做（首版其他平台）；不声称已存在跨平台浮窗或全局按键。 |

## F10 本地曲库与刮削

汇总：**部分实现**。本地库、稀疏覆盖、任务审阅与撤销有证据；刮削与编辑尚未达到全部参考能力。

| 要求点 | 实现、测试、证据 | 判定 / 边界 |
|---|---|---|
| 多根/增删/重扫、进度取消/错误、格式标签封面/sidecar、深度32/交叉根去重、local绝对路径身份 | `haiku-library/{local_scan,service}.rs`、LibraryPage/Service；真实临时媒体/扫描取消/重扫测试 E1/E2 | 已实现并有证据；不删除音频，移盘覆盖保留。 |
| 歌/专辑/艺人分组、未知专辑按目录/专辑艺人、搜索排序、多选/批量 | Library Query/Group、TrackList、LibraryPage；分组/选择回收、Library.test，E1/E2 | 已实现并有证据；全部组合未穷举。 |
| 单曲/批量编辑、稀疏字段、重置、封面、草稿保护、写标签错误 | Library Patch、TrackEditor、`write_tags/import_cover`；Library/collections 编辑与写标签测试 E1/E4 | 部分实现；标题/艺人/专辑/专辑艺人/年份/曲碟号/封面已有；genre/歌词直接编辑未接，跨历史收藏有效层也有 G19/G20。真实容器写回仍待人工。 |
| 刮削插件启停/顺序、音乐来源候选、歌词刮削可配置 | ScrapeSettings、`scrape_service::sources`；E1/E5 | 部分实现；音乐候选默认关为 M4-05 已记录偏离；`scrape.lyrics` 只有 SDK 方法/契约，没有 app 编排/设置调用（G20）。 |
| 单/批量/整组/全库、4并发/8s、0.86+领先0.08自动/0.55审阅 | `haiku-plugins/scraper.rs`、scrape_service/LibraryPage/ScrapeReview；阈值、并发、任务测试 E1/E5 | 已实现并有证据；真实上游待人工。 |
| 只填缺失/覆盖、持久审阅、跳过/取消保留成果、撤销冲突保护、可选显式写回 | scrape_service Patch/previous/applied、Library 稀疏写回；`scrape_*` Rust/Headless E1/E5 | 已实现并有证据；未审阅不写实体标签；撤销覆盖不保证撤销已写实体文件。 |
| 查询清洗/文件名双解释、专辑艺人/碟号、封面本地缓存 | scraper 只基础 normalize/标题艺人查询；Candidate/Patch 缺刮削相关字段，封面 URL 无专用下载缓存 | 部分实现；HANDOFF 的已知遗留成立，G20。重启审阅 UI 尚缺专项执行证据。 |

## F11 喜欢、歌单与远端收藏

汇总：**部分实现**。CRDT 与远端镜像核心有证据；受保护封面/有效元数据和部分浏览设置欠缺。

| 要求点 | 实现、测试、证据 | 判定 / 边界 |
|---|---|---|
| `__liked__` 专用 CRDT、跨源喜欢/取消、加入时间排序 | Collections/PlaylistCore、like_button/TrackList；真实 Loro/不同来源同 ID、Dock/媒体 favorite 接线 E1/E4 | 已实现并有证据；真实控制中心 favorite 待人工。 |
| 搜索/来源筛选/播放/批量管理；用户创建/改名/描述/删除、去重/移除/重排 | CollectionPage/PlaylistEditor、CollectionsService；Collections.test、CRDT 双 peer E1/E4 | 已实现并有证据。 |
| 歌单顺序、网格/列表、四格封面、浏览设置持久化/曲目排序 | CollectionsBrowser 支持顺序/合成封面，但 mode 为初始 `"list"` 局部 signal；Collection 查询无可选排序 | 部分实现；网格/列表与当前顺序可用，未消费 playlistsViewMode、缺参考曲目排序入口（G03）。 |
| favorite 规范 ID、kind=plugin/source/remote/label/cover/time、只读 | `Collections::favorite`、SourceDetailPage/CollectionPage；只读保护测试 E1/E5 | 已实现并有证据。 |
| 完整分页后原子镜像、达到上限报错、不做曲目并集、最大(time,version)、离线/刷新/取消 | `favorite_refresh.rs`、`PlaylistCore` 权威快照、`favorite_playlist`；Rust 单飞/取消/分页和 FavoriteScrape.test，E1/E5 | 已实现并有证据；离线先显再刷新，取消收藏仅删除本机镜像。 |
| 收藏/喜欢同步前后可用，保护封面/有效编辑覆盖 | Loro merge 有 E1；存储为 Track 快照，缺跨视图 metadata authority/鉴权图片私有缓存 | 部分实现；G18/G19，普通封面与合成封面通过不代表鉴权跨端已通过。 |

## F12 历史与听歌统计

汇总：**部分实现**。记录口径、同步和主要统计有证据；部分参考筛选/图表未呈现。

| 要求点 | 实现、测试、证据 | 判定 / 边界 |
|---|---|---|
| 曲目500/歌单100、跨源身份/去重、时间分组、搜索/来源筛选、删/清、未知数量为空 | History/HistoryPage、SourceDetailPage；`history_reference_json_identity_limits_searches_and_groups`、Activity.test，E1/E4/E5 | 已实现并有证据；认证 coverHeaders 可读旧数据，新记录受 G18 限制。 |
| 时间过滤、独立浏览歌单历史管理 | 独立 tab/删除/清空已接；History 查询仅 search/source | 部分实现；缺时间范围过滤（G11）。 |
| 实听单段≥5秒、排除暂停/seek/异常间隔、最多5000、demo不传 | Stats::sample、Core observe、stats_upload；注入时钟/5s/seek/5000/demo 排除断言，E1/E6 | 已实现并有证据；按参考 §6 的 5 秒纠正 §3 的 30 秒。 |
| 时长/次数/曲/艺人、日/月/年/全部、排行、小时/热图/连续天/六级里程碑 | Stats::aggregate、StatsPage；aggregate 和 Activity.test，E1/E4 | 已实现并有证据（上述子集）；测试总数不等于每个里程碑临界点均独立覆盖。 |
| 专辑排行、月分布展示 | aggregate 只有 tracks/artists，无 albums；有 months 数据但 StatsPage 未画月分布 | 部分实现；G11。 |
| 账号上传/去重/重试/聚合、SHA1身份、revision/watermark、不回传远程、账号隔离 | `haiku-services/stats_upload{,_tests}.rs`、account/Activity；五项 loopback 测试，E1/E6 | 已实现并有证据；真实服务和长期累计待人工。 |

## F13 下载与媒体缓存

汇总：**部分实现**。下载管理与独立缓存已有证据；完成文件供播放器优先使用尚未接通，高清封面效果受限。

| 要求点 | 实现、测试、证据 | 判定 / 边界 |
|---|---|---|
| 单/歌单/批量、九态/进度、1–5并发、暂停续传/重试取消/移除/清完成/失败重试/重启 | `app/download_service.rs`、DownloadsPage；`download_service_tests.rs` + Downloads.test，E1/E6 | 已实现并有证据；running→queued、part/Range、2/8s 重试有专项断言。 |
| 音质候选/headers、容器识别、错误回退、半成品不转正 | 下载 service + `haiku-services/downloader.rs`；真实 loopback 认证/错误/标签/原子转正测试 E1 | 已实现并有证据；真实公网断网待人工。 |
| 目录/模板/五布局、非法名/碰撞、迁移移动/保留/取消/逐文件失败 | `download_service::{relative_path,reserve}`、download_management、DownloadSettings；目录/自动下载 Rust与Headless E1/E6 | 已实现并有证据；跨卷、满盘、系统选择器待人工。 |
| 高清/标清封面、封面/歌词四模式、翻译/标签 | `cover/lrc`、finalize、DownloadSettings；headers/tags/sidecars 测试 E1 | 部分实现；四模式有消费者，但 SourceApi 默认已把 `{size}` 展开为512，下载 cover 只替换残留模板，无法通用重新取得高清（G18）。未穷举全部格式×模式。 |
| 同音质完整缓存复用、自动新增/跟随删除三条件 | Service 同 root cache key、download_management 事件监听；手动保护/多归属/重新加载测试 E1/E6 | 已实现并有证据；旧 100ms watch_collections 轮询已在 M6-04 改为 channel select，不能再列为现存缺口。 |
| 显示/打开目录、验证 missing、删音频确认 | DownloadService/DownloadsPage；确认取消/仅记录/删除文件与 verify 测试 E1/E6 | 已实现并有证据；Finder 与真实目录权限待人工。 |
| 独立缓存开关/512MiB–20GiB/占用清理/LRU/pin/失败清理/complete、离线再播 | `haiku-audio/media_cache.rs`、StorageSettings/Controller；`capacity_lru_pins_integrity_and_local_safety`、网络 cache 测试 E1/E2 | 已实现并有证据；不误删扫描原文件/下载文件；不表示 20GiB 长时压力已验。 |
| 下载完成文件优先播放和坏文件回退 | SourceResolver/Addresses 不读取下载记录 | 未实现；G17，与 F06 共用，不以“下载成功”推导“离线播放下载文件成功”。 |
| Wi-Fi 限制 | 保留 downloadWifiOnly，桌面不消费 | 按 SPEC 决定不做；参考 §6 明确桌面忽略该移动限制。 |

## F14 Haiku 账号与权限

汇总：**已实现并有证据**（D6 无会员门槛；真实服务/Keychain 仍待人工）。

| 要求点 | 实现、测试、证据 | 判定 / 边界 |
|---|---|---|
| 邮箱验证码登录/自动注册、60s重发、错误、资料/会员/有效期/套餐 | `haiku-services/account.rs`、AccountSettings/Account.test；loopback 与 UI E1/E6 | 已实现并有证据；展示套餐价格/期限，不伪造购买流程。 |
| `/api/v1`、envelope、code/login/refresh/logout/me/subscription/plans、默认服务 | account raw/request、`DEFAULT_SERVER`；四项 account_tests，E1/E6 | 已实现并有证据。 |
| session恢复、安全存储、单飞401/10002刷新重放一次、失败清理、离线缓存/订阅5min | SystemCredentials/MemoryCredentials、account.rs；过期并发/离线/退出/服务切换竞态测试 E1/E6 | 已实现并有证据（协议/内存后端）；真实 Keychain 授权与重启恢复待人工。 |
| 游客、会员功能门槛 | D6；插件/歌词/刮削/Gist 均不以 subscription.active 阻断 | 按 SPEC 决定不做（参考版会员限制）；不是缺失权限校验。 |
| 支付购买 | 参考版明确无实现；AccountSettings 只展示套餐 | 按 SPEC 决定不做；无需新增订单/假购买。 |

## F15 同步和备份

汇总：**部分实现**。Gist/CRDT/备份安全核心有证据；部分热恢复消费者和实际使用的扫描根映射未齐。

| 要求点 | 实现、测试、证据 | 判定 / 边界 |
|---|---|---|
| Gist PAT安全保存/创建/绑定/断开/手动、状态、12s debounce/120s pull/15–300s退避 | `haiku-services/sync.rs`、SyncSettings/Sync.test；sync_tests loopback、E1/E6 | 已实现并有证据；真实 GitHub/PAT/跨设备待人工。 |
| 自建后端快照同步/账号切换隔离 | SPEC D8、§3 F15 | 按 SPEC 决定不做（首版仅 Gist）；不能当已完成服务端同步。 |
| Loro幂等/可交换、去重/删除重排冲突、gzip/base64兼容 | PlaylistCore/Collections、sync codec、reference-sync-v2 fixture；真实双 peer 和 codec 测试 E1 | 已实现并有证据；albumProgress 仅合并保留，播放器缺 G12。 |
| `.haikubak` schema1/2、settings/likes/Loro/sources/运行storage/包/PAT、导出flush/明文提示 | backup.rs、BackupSettings；backup_tests五项、Backup.test，E1/E6 | 已实现并有证据（v2 调整）；D9 旧包/实例跳过报告、Subsonic连接字段可迁移。不是兼容运行 v1 插件。 |
| inspect固定字节/预览/确认、未来schema/坏ZIP/路径/symlink/白名单拒绝、安全副本≤3 | backup.rs 及恶意路径/替换预览/失败后重读测试；E1/E6 | 已实现并有证据；未做 ZIP bomb/满盘全部组合。 |
| 热恢复停播清队列、停稳同步/下载/插件、reload设置/Loro/来源/同步 | `app/backup.rs::Runtime` before/after_restore、SettingsService/PluginService refresh；Backup.test/E1/E6 | 部分实现；现有消费者有恢复，未等待刮削任务停稳，也未调用 ShortcutsService 注册刷新（G07/G13）。 |
| 恢复设置消费者与扫描根、跨平台目录保留 | backup apply 操作 `settings.localScanRoots`；Library 实际用 `library/roots.json`，无导出/导入/重读映射 | 部分实现；恢复设置中的键不能更新实际扫描根，重启也不会自动桥接；G13。 |
| 不含音频/媒体缓存/字体/历史、显式导入独立根 | backup 白名单、DataRoot/Core；E1/E6 | 已实现并有证据；与 D5 一致，不擅自读取参考用户数据。 |

## F16 设置、字体、通知和日志

汇总：**部分实现**。设置/诊断有证据；D8 保留的独立系统字体选择仍未实现，许可证待用户决定。

| 要求点 | 实现、测试、证据 | 判定 / 边界 |
|---|---|---|
| 独立设置窗口/helper/IPC | SPEC D18、§3 F16 | 按 SPEC 决定不做；使用主窗口内设置覆盖层。 |
| 重复打开、分类搜索/滚动、账户首栏、六原分类内容、开关/输入/选择 | Settings/AppShell、各 Settings panel；E1/E2/E3/E6 | 已实现并有证据（现有项）；缺失的业务选项分别归 G02/G06/G08/G09/G10/G20，不以保留未知设置字段算实现。 |
| 串行原子落盘、嵌套merge、值校验/默认、实时消费者、系统深浅/品牌/玻璃 | SettingsStore/Service、AppearanceOptions/theme、GlassSurface；settings/DSP/主题测试 E1/E2，M6 H | 部分实现；主要消费者已接，快捷键热恢复/字体/缺失选项例外；最终玻璃证据缺失 G01。 |
| UI与普通歌词分别选择系统字体/即时生效 | AppearanceOptions 无字体字段，LyricsSettings 只有桌面歌词字体面板；lyrics_view 无可选 family | 未实现；D8 只暂缓下载/导入/镜像，没有取消系统字体选择（G06）。 |
| 内置/字体下载/导入/镜像/安装卸载保护 | D8、§3 F16 | 按 SPEC 决定不做；不把这些延期项列为强制缺口。 |
| 通知结果/错误、详情展开/清理 | Logs::notify、NotificationCenter、各服务错误发布；Activity.test/logs测试，E1/E4 | 已实现并有证据（现有发布链）；不保证所有网络/OS故障都已有专项截图。 |
| app/plugin各1000、session、detail8000、分页/来源级别关键词、复制筛选/导出全部/清理、URL及敏感headers脱敏 | `haiku-store/logs.rs`、ActivityService、Diagnostics/LogsView；`logs_rotate_bound_redact_filter_and_notifications_dismiss`、Activity.test，E1/E4 | 已实现并有证据；真实剪贴板/保存选择器与长时日志待人工。 |
| 快捷键录制/冲突/重置、关于版本/框架/许可证/第三方声明 | ShortcutSettings/About、`native/app/assets/THIRD_PARTY_NOTICES.txt`、About.test；M6 H | 部分实现；代码/声明存在，本机 M6执行证据缺；应用许可证仍“尚未声明”（G23）。 |
| release .app与资源、干净目录启动、最低系统和真实启动门禁 | `packaging/{App.tsx,nsh.json}`、builtin资源/asset URI；H 记载83.75MiB adhoc包/空根安装 | 已实现缺证据（本机无产物/原日志）；strip缺libLLVM警告、Finder/Gatekeeper等仍有限制（G01/G05），不声称已签名公证或验 macOS14。 |

## 缺口清单（本轮不补，按规模排序）

规模为实施估计，非承诺：小 <1 小时，中约1–4小时，大 >4小时；跨平台/实账号/性能调查可能更长。表中的“补齐”是建议交审阅者派发，当前未获第二步授权。标为“需用户决定”的项不能擅自扩大产品范围。

| 编号 | 规模 | 涉及 F | 缺口、建议与完成判据 |
|---|---|---|---|
| G01 | 小（搬回已有证据）；若遗失则中/大 | F01/F08/F09/F16 | 本机缺 M0、M1-01/02、M4-01/02、M6-01～05 原始证据/最终 app。**如实标注**历史已审阅；优先取回旧证据。以后单独派当前基线/最终包验收，不在审计中重跑。 |
| G02 | 小 | F07/F16 | EQ 全参数重置、自定义预设覆盖及删除确认。**补齐**管理入口，保留 enable/自定义预设语义并验证 DSP 设置一致。 |
| G03 | 小（视图持久化）；曲目排序中 | F11/F16 | playlistsViewMode 未持久化/恢复，歌单内缺可选曲目排序。**补齐**设置消费者与入口；不改 CRDT 手工顺序语义。 |
| G04 | 小 | F09/F16 | 可选“媒体中心当前歌词”无设置/投影。**补齐**且关闭时不写该字段。 |
| G05 | 小（工具链修复是否成立待核） | F09/F16 | 最终打包 strip 缺 libLLVM.dylib 警告（H）。**如实标注**，后续定位既有工具链配置；不能据成功产包称无警告。 |
| G06 | 中 | F08/F16 | 独立 UI/普通歌词系统字体选择与运行消费者。**补齐** D8 保留范围；桌面歌词 NSFontPanel 不替代这两项。 |
| G07 | 中 | F09/F15/F16 | 备份热恢复只刷新设置，不重注册全局/应用快捷键或通知快捷键订阅。**补齐**恢复接线/冲突回滚，验证恢复后立即生效；当前需重启。 |
| G08 | 中 | F01/F06/F16 | 睡眠完整 UI、Stop/静音、队列清空/重排/当前项定位。**补齐**桌面入口，复用现有 Controller 命令，不另造队列。 |
| G09 | 中 | F06/F16 | 自动错误跳曲、gapless开关未消费；audioQuality 无播放选择入口。**补齐**设置到实际播放/预载行为，验证关闭后行为不同。 |
| G10 | 中 | F08/F16 | 普通歌词翻译开关；Paxsenix 评分/令牌重取/content兜底、Kugou署名裁剪。**补齐**参考语义及失败 fixture；纯音乐已有文本映射，需补显示证据，不先判未实现。 |
| G11 | 中 | F12 | 历史时间过滤、专辑排行、月分布显示。**补齐**聚合/入口，已有 months 数据可复用；补对应边界断言。 |
| G12 | 中 | F06/F15 | albumProgress 未接播放器/详情页，无专辑/节目独立写入/续播。**补齐**上下文/暂停退出flush/切换隔离，沿用既有同步格式。 |
| G13 | 大 | F10/F15 | 扫描根实际文件与备份 settings.localScanRoots 脱节；热恢复不等待刮削停稳。**补齐**同平台根映射/重读、跨平台本机保留与刮削恢复屏障，避免恢复后旧任务继续写。 |
| G14 | 大 | F01/F03 | 第三方“我的”页及 owned/收藏/喜欢分页、登录后刷新无 app 链路；web 未接 D14 原生回退。**补齐**来源用户内容入口与独立 WKWebView 会话；QQ新接口已可复用。 |
| G15 | 大 | F04 | home 多源聚合、首屏2页/续页1页/空货架补齐；混合来源 FM。**补齐**分页等明确要求；聚合存在 UI.md 单来源与参考 SPEC 的范围冲突，**需审阅者决定**保留参考范围还是取得用户批准调整，不能静默视作取消。 |
| G16 | 大 | F05 | 搜索页没有全部来源的五实体聚合/分源错误状态，只有单源页和两实体多源命令预览。**补齐**查询身份、分页和分源失败不丢成功结果。 |
| G17 | 大 | F06/F13 | 播放/预载/会话缓冲恢复不查询已完成下载。**补齐**下载→同音质缓存→网络的统一解析与坏下载标失效/回退，验证零联网离线播放和预载同序。 |
| G18 | 大 | F03/F08/F09/F11/F12/F13 | 逐图动态认证/私有缓存恢复、1600档当前/实际next预热、磁盘缓存限额、下载高清重取欠缺。**补齐**统一封面解析；默认512已展开的URL不能靠再次替换模板解决；凭据/本机路径不得同步泄漏。 |
| G19 | 大 | F05/F08/F11/F12 | 在线曲目元数据编辑/重置缺失，历史/收藏/队列无统一有效覆盖层。**补齐**按(source,id)生效并订阅全部消费者，避免只改某列表快照。 |
| G20 | 大 | F08/F10/F16 | 刮削高级清洗/文件名回退、歌词刮削选项与编排、专辑艺人/碟号/genre/歌词编辑及封面本地缓存。**补齐**参考语义；保留稀疏覆盖、未审阅不写标签和撤销保护；重启审阅UI还需补证据。 |
| G21 | 大（含实机性能验收） | F06/F08/F09 | 全栏seek尚未实现，Rust/NSH进度有基础时钟测试但缺最终遮挡恢复/release性能证据。**补齐**交互并验证等价NSH方案；参考CA时间轴/350ms校正属于不同渲染实现，不要求机械照搬。若要降低原交互/性能验收标准则**需用户决定**。 |
| G22 | 大（原因调查）；可延期 | F01/F08/F09 | Rust animated_value真实host不推进；当前静态开关已获用户批准。另有initialNowPlaying夹具初始开事件问题。**如实标注/需用户决定**是否恢复动画及修夹具；先单独调查，不强制改NSH。 |
| G23 | 需用户决定（落实文字/图资源为小） | F09/F16 | 应用自身许可证；Dock图标是否用带macOS圆角/留白版本。GPL来源已在QQ代码和第三方声明标明，不能代用户选择应用许可证。**需用户决定**，现满版图标为用户选定资源，不擅自重绘。 |

不列为强制缺口：D1其他桌面平台首版验收、D8自建后端同步与字体下载/导入/镜像、D9旧ABI运行、D18独立设置窗口、用户批准暂缓KaraokeLayer和静态NowPlaying动画、参考§6桌面Wi-Fi限制、原版没有的支付购买。M4-05音乐来源刮削默认关闭、M6全局快捷键默认未绑定均已有审阅记录；不静默改默认值。

## 累积待人工确认（M1–M6 去重）

此清单合并 HANDOFF 的累积项与本次代码证据边界，不是已通过清单。测试均未读取真实用户凭据或向真实账号写入。

1. **窗口与系统播放**：真实媒体键/耳机及控制中心播放、上下首/停止/favorite；关闭隐藏后持续播放、Dock/重新激活恢复、最小化、Cmd+W、Cmd+Q及收藏/会话flush；防息屏实际效果。M0已有人工确认保留，最终产品仍需复核。
2. **音频与大列表**：音量/静音/seek拖动与队列弹层手感，四类支持格式听感、多曲无缝、EQ/响度保护；真实缓存二次命中、离线再播与公网故障恢复；小窗口/长列表/真实鼠标键盘焦点。
3. **来源与认证**：真实Subsonic/飞牛（FNID、中继、安全码）、QQ/微信扫码、短信风控、Cookie/令牌刷新、VIP/地域权限、私人电台；系统插件选择器和编辑器保存热重载；未实现的web回退不能先验收为通过。
4. **在线内容**：真实搜索、首页/分类、电台持续批次、详情分页、来源收藏刷新/取消；受保护CDN封面与音频。fixture或QQ公开live不替代这些项。
5. **歌词与刮削**：六类真实歌词上游、真实刮削候选、实体标签写回及只读/失败提示；长时间真实播放统计与实际服务端聚合。
6. **下载与文件操作**：真实下载上游/默认目录权限、系统目录选择器、Finder显示文件/打开目录；跨卷移动/部分失败/真实碰撞、文件删除确认；原生保存选择器、剪贴板、日志复制导出。
7. **账号与同步/备份**：真实Haiku邮箱登录/资料/会员与服务地址、Keychain授权/重启恢复；真实GitHub PAT权限/大文件raw URL、跨设备Gist收敛；真实用户schema1/2备份往返/跨平台目录保留、PAT导入授权、原生备份选择器。
8. **全局快捷键/菜单**：应用失焦后的真实全局按键、系统占用冲突/回滚；菜单“关于”、版本与许可证展示。
9. **菜单栏与桌面歌词**：真实按钮/队列菜单命中、多屏、暂停/seek与歌词同步；锁定悬停解锁/穿透、拖动保存重启恢复、八个工具按钮及NSFontPanel。
10. **玻璃与发行包**：系统深浅/应用主题/辅助功能减少透明度或提高对比度、窗口缩放；最终.app Finder双击/Gatekeeper提示、干净根内置资源、Dock图标观感。当前adhoc、未公证及最低macOS14实机覆盖均需如实说明。

完成第一步后停止。后续补缺口、补基线或实机验收均需审阅者另行派发；本轮仅新增本文件并更新 STATUS 功能表及 M6-06 记录。
