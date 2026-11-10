# 任务 M2-04：遗留补齐、qq-music v2、设置“来源/插件/开发者”面板、web 登录、`sdk/GUIDE.md`

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。这是 M2 最后一个任务。

## 先读

- `specs/PLUGIN_SDK.md` §4、§10.3、§10.4、§11；`specs/UI.md` §6（设置覆盖层：来源、插件、开发者）；HANDOFF 中 M2-03 小节遗留。
- NSH Design Guides 与 `src/ui/STANDARDS.md`；现有 `src/shell/{Settings,PlaybackSettings,StorageSettings}.tsx`、`src/ui/SettingsControls.tsx`、`src/state/settings.ts`（TSX 只镜像 Rust 状态的模式）、`native/app/src/settings.rs`（导出 + onChange → Stop 的写法）。
- 参考版（只读）`~/Project/GPUI/haiku`：`assets/plugins/feiniu.music/plugin.js`（FNID、中继、access-code）、`dev-plugins/qq.music/`、`src/ui/` 中来源/插件管理与登录对话框、`src/services/{plugins,sources,secrets}.ts`。插件 id：`qq.music`（放 `dev-plugins/`，不默认安装）。
- 已知限制：Rust 组件 `animated_value` 在真实 host 不推进；本任务 UI 用 TSX + nsh-base 组件，不新增依赖 Rust 动画的界面。

## 0. M2-03 遗留

- haiku-jsrt 注册 timers（全局 `setTimeout`/`clearTimeout`/`setInterval`/`clearInterval`，随调用 abort 与实例卸载清理）。
- feiniu 补齐参考版的 FNID / 中继 / access-code 远程连接（连接字段、identity 规则与参考版一致，保证 sourceId 兼容；fixture 覆盖三种连接路径）。
- 契约套件：自动遍历 detail.tracks 分页；replay 在 jsrt fetch 钩子层拦截（覆盖插件绕过 SDK 的裸 fetch）。`--record/--live` 保持基础实现即可，汇报中说明。

## 1. qq-music v2（`dev-plugins/qq-music/plugin.ts`）

- 平台来源 + 刮削：auth（qrcode 含 QQ/微信 variants、sms、cookie）、user、refresh、logout；search、track.streams/lyrics、album/artist/playlist、toplist、home、categories、me.playlists/likedTracks、scrape。按参考版 v1 的上游 API 映射，使用 haiku-jsrt 的 crypto 扩展。
- 离线 fixture 契约测试通过；不访问真实上游，不提交任何真实凭据或个人数据。
- 格式风格同 subsonic；行数不设硬性上限，但不得为压行数牺牲可读性。

## 2. 导出与设置面板（TSX + Rust 导出）

- Rust 导出 `core.sources` / `core.plugins`（列出、启停、排序、重命名、移除、登录流程驱动、包导入/删除、描述符、日志与 HTTP 追踪订阅、契约测试运行），带文档注释，订阅返回 Stop；TSX 只做镜像。
- **来源**：账号行（来源名、登录用户或“未登录”、登录/登出）；“添加新来源…”打开登录对话框，按描述符认证方式渲染：二维码（variants 切换、过期刷新、关闭即取消）、短信（cooldown）、表单/连接字段（secret 输入）、cookie、web。登录或 verify 成功才注册，失败回滚。
- **web 登录**：用 NSH webview（独立会话）打开描述符 url，每次导航完成回调 `done`，成功关闭；查 NSH 0.4.0 webview API 能否读取 cookie，不能则如实说明并降级为“cookie 粘贴”。
- **插件**：每包一行（logo、名称/版本/作者、启用开关）；详情 Popover（能力、实例、删除需确认；内置不可删除、有实例使用不可删除）；“导入”菜单：ZIP、目录、单个 .ts 文件、HTTPS URL。
- **开发者**：加载本地插件目录并监听变更热重载（保留 storage）；按描述符列能力并自动生成参数表单，单独调用，树形查看返回值与结构警告；实时插件日志与 HTTP 追踪；查看/清除实例 storage 与凭据；一键运行契约测试并显示报告。
- 全部用现有设计系统组件与 token；文案遵循 STANDARDS（打开需进一步输入的表面用“…”）。

## 3. `sdk/GUIDE.md`（§10.4）

- 概念（来源、实例、实体、游标）→ 最小插件 → 五种认证 → 各能力语义与约束 → 错误与恢复 → 调试与测试（CLI、契约套件、开发者面板）→ 发布。以 subsonic 插件逐段讲解，代码片段与仓库实际代码一致。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：timers、feiniu 三种连接、契约 detail 分页与裸 fetch 拦截、qq-music 契约（fixture）通过。
2. `haiku-plugin check` + `test` 对 `dev-plugins/qq-music` 及修改后的 feiniu 通过；`tsc --noEmit` 检查全部插件。
3. `nsh check`、`nsh build`（仅已知例外）、`nsh test`。Headless：添加 subsonic 来源（fixture 服务器）成功后出现在来源列表、失败不残留；插件启停与删除确认；开发者面板调用一个能力并显示结果；导出订阅 Stop 后不再回调。
4. 真实运行截图（浅色）：设置“来源”（含一个已添加的 subsonic fixture 来源）、登录对话框（二维码或表单）、“插件”列表、“开发者”面板调用结果各一张，保存到 `.artifacts/m2-04/`。
5. 收尾：更新 STATUS（F02、F03 改为 M2 范围完成状态；追加 M2-04 小节 ≤ 40 行），保留改动等待审阅。

## 不做

在线内容页面（推荐/搜索/详情）与网络播放（M4）；系统钥匙串存凭据（后续增强）；下载、账号同步（M5）。

## 汇报

最终回复：导出 API 概要、web 登录实现方式与限制、qq-music 能力与认证覆盖、与 UI.md/PLUGIN_SDK.md 的偏离、待人工确认项（真实账号登录等）、需审阅者决定的事项、未完成项（额度不足时如实列出）。
