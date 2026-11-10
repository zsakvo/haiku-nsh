# 任务 M2-05：qq-music v2、`sdk/GUIDE.md`、开发者面板与插件管理收尾

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。完成后 M2 关闭。

## 先读

- `specs/tasks/M2-04-qq-music-settings-guide.md` §1、§2（开发者/插件部分）、§3——本任务是其未完成部分，要求不变。
- HANDOFF 中 M2-04 小节；`specs/PLUGIN_SDK.md` §4、§10.3、§10.4、§11。
- 已提交代码：`4218472`（`src/shell/PluginSettings.tsx` 等设置面板、`core.sources/plugins` 导出）、`plugins/*`、`haiku-plugins::contract`、`native/tools/haiku-plugin`。
- 参考版（只读）`~/Project/GPUI/haiku/dev-plugins/qq.music/`（72KB v1，上游 API 与加密映射依据）、`tests/qq-plugin.test.ts`。

## 1. qq-music v2（`dev-plugins/qq-music/plugin.ts`，id `qq.music`）

- 与 M2-04 §1 相同：auth（qrcode QQ/微信 variants、sms、cookie）、user、refresh、logout；search、track.streams/lyrics、album/artist/playlist、toplist、home、categories、me.playlists/likedTracks、scrape。
- 离线 fixture 契约通过；不访问真实上游，不提交真实凭据或个人数据。可读性同 subsonic。
- 不默认安装；开发者面板“加载本地插件目录”可加载它。

## 2. 开发者面板与插件管理收尾

- 开发者：本地插件目录监听文件变化自动热重载（保留 storage）；按描述符生成参数表单（替代 JSON 数组输入，保留“原始 JSON”切换）；实时插件日志与 HTTP 追踪列表（虚拟列表）。
- 插件：渲染包 logo（包内相对路径或 https）；“导入”菜单中 ZIP/目录/.ts 使用系统文件选择器（NSH picker 能力）；无作者时次行不显示“未提供作者”。
- 登录：二维码 `url` 由宿主生成二维码图像（Rust 侧生成，选用小依赖并说明）。
- Headless 回归：QR 登录（过期刷新、关闭即取消）、SMS cooldown、cookie/web 降级表单、导出订阅 Stop 后不再回调。

## 3. `sdk/GUIDE.md`

- 按 M2-04 §3 要求；代码片段取自仓库实际 `plugins/subsonic/plugin.ts`，并说明 CLI（new/check/test）、契约报告、开发者面板用法与 web 登录降级。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`（含 qq-music 契约、二维码生成）。
2. `haiku-plugin check` + `test` 对 `dev-plugins/qq-music`；`tsc --noEmit` 检查全部插件。
3. `nsh check`、`nsh build`（仅已知例外）、`nsh test`（含上述 Headless 回归）。
4. 浅色截图到 `.artifacts/m2-05/`：插件列表（带 logo）、开发者面板（自动参数表单 + 日志/追踪）、二维码登录对话框（fixture 生成的二维码）。
5. 收尾：更新 STATUS（F02、F03 标记 M2 范围完成情况；追加 M2-05 小节 ≤ 40 行），保留改动等待审阅。

## 不做

在线内容页面与网络播放（M4）；钥匙串凭据；真实 webview cookie 读取（NSH 不支持）。

## 汇报

最终回复：qq-music 能力与认证覆盖、GUIDE 目录、新增依赖、偏离、待人工确认项（真实 QQ 登录等）、需审阅者决定的事项、未完成项（额度不足时如实列出）。
