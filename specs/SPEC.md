# Haiku NSH 实施总纲

状态：设计定稿前的实施文档，尚未开始编码。编写日期：2026-11-09。

## 1. 目标

在 Native Shell（NSH）上重新实现 Haiku 音乐播放器，功能对标 `~/Project/haiku-gpuix`（以下简称“参考版”）的 F01–F16。参考版的 `specs/SPEC.md` 第 3、6 节是业务语义的事实来源；本仓库文档只记录**与参考版不同的决定**和**新架构下的实现方式**，业务细节未另行说明时以参考版为准。

参考版只读，不修改；不读取、不迁移用户真实账号凭据；测试使用隔离数据目录。

## 2. 已确认的决策

| # | 主题 | 决定 |
|---|---|---|
| D1 | 平台 | macOS 优先，首版只验收 macOS（14+，NSH 要求）。平台集成隔离在独立 crate，Windows/Linux 后续补齐或明确降级 |
| D2 | NSH 模式 | Rust + TSX 混合：Rust 持有全部业务与高频视图，TSX 只做屏幕 |
| D3 | Rust 核心 | 从参考版 `native/` 拷贝进本仓库，按领域拆 crate，**进程内调用**，取消 JSONL RPC 与 sidecar；保留并迁移原单元测试 |
| D4 | 范围 | F01–F16 全量，按 [MILESTONES.md](MILESTONES.md) 分阶段交付 |
| D5 | 数据兼容 | 独立数据根；兼容 `.haikubak` schema 1/2 及参考版目录格式中的设置、歌单（`playlists.loro`）、喜欢、历史等；用户显式导入，首次启动不擅自读取其他应用数据 |
| D6 | 账号 | 保留 Haiku 账号（登录、会话、资料、会员状态展示、统计上传），**取消全部会员门槛**：插件导入、自动刮削、联网歌词、歌单同步对所有人开放 |
| D7 | 视觉 | 按 NSH Design Guides 重建主题 token 与组件库；保留标志性体验：底部播放控制栏（macOS 玻璃）、封面 Material You 配色、双栏正在播放、逐字歌词 |
| D8 | 暂缓 | 自建 Haiku 后端歌单同步、自定义字体管理（下载/导入/镜像）。首版仅 Gist 同步、系统字体 |
| D9 | 插件 | **全新插件范式 v2**，只支持 v2，内置插件全部重写。见 [PLUGIN_SDK.md](PLUGIN_SDK.md) |
| D10 | 插件语言 | 插件用 TypeScript 编写，宿主直接加载 `.ts`（引擎侧剥离类型），无打包步骤 |
| D11 | 插件运行时 | 不再依赖 cjrt-workspace。在本仓库新建 `haiku-jsrt` crate，参照 CJRT 的能力，基于 NSH 自带的 QuickJS（`quickjs-jit` + `quickjs-jit-stdlib`）实现插件运行时底层；音乐 SDK（`haiku-plugins`）建立在它之上。进程内只有一份 QuickJS（2026-11-09 因 M0-01 的 S2 失败而修订，原决定为“继续使用 CJRT 并在 cjrt-workspace 补全”） |
| D12 | 插件隔离 | 每个来源实例一个独立 JS runtime |
| D13 | 插件网络 | 不限制访问域名 |
| D14 | 网页登录 | SDK 声明式 web 登录，宿主用 NSH webview 承载；若无法读取 HttpOnly Cookie，macOS 回退到参考版 WKWebView 实现 |
| D15 | 插件开发支持 | 应用内插件调试面板、契约测试套件、脚手架模板、插件编写指南 |
| D16 | 系统集成节奏 | 媒体中心与媒体键、后台播放在 M1 完成；菜单栏歌词、桌面歌词、全局快捷键、原生玻璃控制栏放在最后阶段 |
| D17 | 验收 | 沿用参考版的 STATUS.md 证据制 |

## 3. 相对参考版的范围调整

| 要求 | 调整 |
|---|---|
| F02 插件 | 包格式、manifest、ABI 全部换成 v2；不再导入 v1 插件（导入时明确提示“旧版插件格式不受支持”）。备份中的 v1 插件包与实例跳过并在恢复摘要中列出 |
| F03 认证 | 认证方式不变（QR、web/cookie、账号密码/表单、SMS、API key），由 SDK 统一驱动；新增“自建/收藏歌单”分组（v1 ABI 无法表达） |
| F14 账号 | 去掉 `subscription.active` 判据对功能的限制，仅展示会员状态与套餐 |
| F15 同步 | 只做 Gist；自建后端快照同步暂缓 |
| F16 字体 | 只支持系统字体选择（UI 字体、歌词字体分别设置）；字体下载、导入、镜像暂缓 |
| F16 设置窗口 | 独立设置窗口改为**同进程第二个 NSH 窗口**，不再需要 helper 进程与 IPC |
| F09 桌面集成 | 不变，但分阶段：媒体中心先做，其余放 M6 |

其余 F01、F04–F13 业务语义不变。

## 4. 文档索引

| 文档 | 内容 | 覆盖要求 |
|---|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | 仓库结构、crate 划分、Rust/TSX 边界、线程与事件模型、错误模型、技术验证 | 全局 |
| [PLUGIN_SDK.md](PLUGIN_SDK.md) | 插件范式 v2、`haiku` SDK、宿主职责、haiku-jsrt 运行时能力、开发工具 | F02 F03 F04 F05 F10(刮削) |
| [PLAYBACK.md](PLAYBACK.md) | 队列、播放编排、音频引擎、音效响度、媒体缓存、歌词链路 | F06 F07 F08 F13(缓存) |
| [LIBRARY.md](LIBRARY.md) | 本地曲库、刮削、喜欢与歌单、历史、统计、下载 | F10 F11 F12 F13 |
| [DATA.md](DATA.md) | 数据根布局、兼容性、设置、账号、同步、备份、日志、凭据 | F14 F15 F16 |
| [UI.md](UI.md) | 设计系统、外壳与导航、页面清单、正在播放、设置窗口 | F01 F04 F05 F08 F16 |
| [MACOS.md](MACOS.md) | 媒体中心、菜单栏、桌面歌词、快捷键、原生视图 | F09 |
| [MILESTONES.md](MILESTONES.md) | 阶段划分、退出条件 | 全局 |
| [STATUS.md](STATUS.md) | 逐项实施与验收证据 | 全局 |

## 5. 完成门禁

- 每个 F01–F16 在 STATUS.md 记录实施文件、验证命令、证据和缺口。状态只有 `未实现 / 部分实现 / 已验证`；没有证据不能标“已验证”。
- 循环：`nsh check` → `nsh build`（warn 视为问题）→ `nsh test` → `nsh run`。Rust 逻辑用 `cargo test`；UI 行为用 `nsh_testing::Headless` 与 `*.test.tsx`；外观靠真实窗口截图。
- 播放用真实音频文件验证 seek、结束和候选回退；下载用本地 HTTP fixture 核对文件和标签；CRDT 用两实例验证冲突合并；备份做往返和恶意路径拒绝；验证重启后的数据恢复。
- 插件：每个内置插件通过契约测试套件；真实上游可用性只作为补充证据，不能替代契约测试。
- 构建独立 `.app`，打包内置插件与必要资源；用干净数据目录启动。
