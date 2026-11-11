# 任务 M6-05：打包 `.app`

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。

## 先读

- `specs/MILESTONES.md` M6、`specs/SPEC.md` 收尾一节、`specs/PLUGIN_SDK.md`（内置插件随应用打包、首次启动安装、可停用不可删除）。
- nsh 技能中打包/发布相关说明（`nsh` CLI 的打包命令、图标与资源、capabilities）。
- 已有：`nsh.json`、`plugins/`（subsonic、feiniu、kugou-scraper、qq-scraper）、`assets/`、关于页（M6-01）。

## 1. 打包

- 用 nsh 自带的打包方式产出 release `.app`（不自写打包脚本，除非 nsh 不提供；若需要则停下汇报）。不签名、不公证。
- 应用图标：用 `assets/` 中已有图标资源；若没有合适的 `.icns`/源图，停下汇报，不自行设计图标。
- 内置插件与必要资源进入包内；首次启动从包内安装内置插件。
- `nsh.json` 的 `fs` capability 中指向 `.artifacts/m1-03` 与 `/tmp/haiku-m1-03-fixtures` 的测试路径不得进入发行配置：改为仅开发/测试生效，或说明为何必须保留。
- 第三方声明：生成包含 Rust 依赖许可证清单的第三方声明文件放进包内，关于页“第三方声明…”打开它。应用自身许可证仍为“尚未声明”，不改（由用户决定）。

## 验收（每项跑一次）

1. `nsh check`、`nsh build`（仅已知例外）、打包命令（release）成功，记录产物路径与大小。
2. 干净数据目录启动：用临时空数据目录（或等效环境变量）启动打包产物，日志确认内置插件已安装、无错误；截一张应用窗口截图（只截应用窗口）到 `.artifacts/m6-05/`。
3. 关于页“第三方声明…”在 Headless 或真实窗口中可打开（任选其一）。
4. 收尾：更新 STATUS（追加 M6-05 小节 ≤ 40 行），保留改动等待审阅。不重跑 workspace 与全量 nsh test，除非改动了 Rust 或 TSX 业务代码（此时跑受影响部分）。

## 不做

签名与公证、自动更新、DMG 安装包、F01–F16 审计（M6-06）。

## 汇报

最终回复：打包命令与产物、图标与资源来源、内置插件安装方式、capabilities 调整、第三方声明生成方式、偏离、待人工确认项（Finder 双击启动、Gatekeeper 提示）、需审阅者决定的事项、未完成项。
