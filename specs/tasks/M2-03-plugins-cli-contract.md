# 任务 M2-03：宿主遗留补齐、feiniu/qq-scraper/kugou-scraper v2、`haiku-plugin` CLI、契约测试套件

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。本任务不做 UI。

M2 剩余拆分：**M2-03（本任务）** → M2-04 qq-music（dev-plugins，qrcode/sms/cookie 三种认证）、设置中“来源”“插件”“开发者”面板与 TSX 导出、web 登录 webview、`sdk/GUIDE.md`。

## 先读

- `specs/PLUGIN_SDK.md` §3–§7、§9、§10.1、§10.2、§11；HANDOFF 中 M2-02 小节的遗留清单。
- 已提交代码：`haiku-plugins`（`dd99016`）、`plugins/subsonic/`（范例风格：常规 TS 格式，行宽约 100）、`sdk/haiku.d.ts`。
- 参考版（只读）`~/Project/GPUI/haiku/assets/plugins/{feiniu.music,qq.scraper,kugou.scraper}`（v1 实现，作为上游 API 映射依据）与 `src/services/scraper.ts`（刮削打分/并发/超时语义，宿主侧）。
- 插件 id 沿用参考版：`feiniu.music`、`qq.scraper`、`kugou.scraper`。

## 0. M2-02 遗留

- SourceApi 结构校验补齐 `home`（Shelf 各类型、Entry、Link、`more`）、Playlist.creator/Program.host/episodeCount/Toplist.subtitle 等可选字段透传、无 id Artist 保留为不可点击。
- 宿主“完整列表”循环翻页（最多 100 页，超限仍有 next 时报错）。
- SMS：宿主管理 state 与 cooldown；QR：到期时 abort 进行中的 poll 并可重新 start。
- 描述符保存认证展示元数据（qrcode variants、password fields、web url/userAgent、sms 存在性）。
- 回归测试：cookie 罐持久化（重载实例后仍在）、SMS/cookie/password 登录、logout 清凭据、refresh 再失败→needs_login、启动时坏包隔离（一个包描述符失败不影响其他来源）、安装中途崩溃恢复（残留临时目录/备份在下次打开时回滚或清理）。

## 1. 内置插件 v2 重写

- `plugins/feiniu/plugin.ts`（库插件，connection + library 浏览 + 搜索 + 详情 + streams + 歌词如上游支持）；`plugins/qq-scraper/plugin.ts`、`plugins/kugou-scraper/plugin.ts`（仅 `scrape.search`/`scrape.lyrics`）。目录名可按现有 `plugins/subsonic` 习惯，插件 id 必须为参考版 id。
- 宿主刮削编排（§5.4）：打分（自动采用阈值 0.86 且领先 0.08，≥0.55 进入待审）、并发 4、单源超时 8 秒，放在 haiku-plugins（或 haiku-services，说明选择）；按参考版 scraper.ts 的打分口径实现并单测。
- 内置插件随应用打包并在 `Core::open` 首次安装（同 subsonic）。

## 2. `haiku-plugin` CLI（`native/tools/haiku-plugin`，§10.1）

- `new <name> --template provider|library|scraper`：生成 `plugin.ts`、`tsconfig.json`（指向 `sdk/haiku.d.ts`）、`fixtures/`、README。
- `check <dir>`：调用 tsc 类型检查 + 描述符生成并打印能力；tsc 不可用时给出明确提示（说明本机 tsc 的来源，不向应用引入 npm 运行时依赖）。
- `test <dir> [--live] [--record]`：运行契约测试套件。

## 3. 契约测试套件（§10.2，Rust，供 CLI 与 cargo test 共用）

- 在真实 haiku-jsrt runtime 中运行；描述符完整性与能力一致；每个已实现能力返回值符合 schema；id 往返（列表 id 能被对应 get 使用）；分页不重复、终止、游标可序列化；fixture 模拟 401/404/429/5xx 的错误映射；`AuthExpired` 触发 refresh 并重放；调用中 abort 迅速结束。
- 默认 record/replay HTTP fixture 离线运行；`--record` 访问真实上游录制（本任务不要求录制真实数据，可手写 fixture）；`--live` 仅作补充。
- 输出结构化报告（JSON + 人类可读摘要），供 M2-04 调试面板复用。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：第 0 步全部回归；三个新插件的契约套件（离线 fixture）通过；刮削打分阈值/领先/待审/并发/超时单测。
2. CLI：`haiku-plugin new` 三种模板生成后各自 `check` 通过并 `test` 通过；对 `plugins/` 下四个内置插件逐个 `check` + `test` 通过（输出保存到 `.artifacts/m2-03/`）。
3. `tsc --noEmit` 检查四个内置插件；`nsh check`、`nsh build`（仅已知例外）、`nsh test`。
4. 收尾：更新 STATUS（F02、F03、F10 刮削宿主部分；追加 M2-03 小节 ≤ 40 行，列出新增依赖），保留改动等待审阅。

## 不做

qq-music、任何 UI/TSX 导出、web 登录 webview、GUIDE（M2-04）；网络播放与在线页面（M4）；本地曲库刮削的 UI 接入（M4）。

## 汇报

最终回复：三个插件行数与能力、刮削编排位置与打分口径、CLI 用法与契约报告格式、与 PLUGIN_SDK.md 的偏离、需审阅者决定的事项、未完成项（额度不足时如实列出）。
