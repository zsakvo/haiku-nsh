# 任务 M2-02：haiku-plugins 宿主、`haiku` SDK、`sdk/haiku.d.ts`、subsonic v2

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**（不要用 `~/.codex/worktrees`）。完成后保留改动并停下汇报，不提交 git。

M2 拆分：M2-01 运行时（已提交 `8913520`）→ **M2-02 宿主 + SDK + subsonic（本任务）** → M2-03 其余内置插件、`haiku-plugin` CLI、契约测试套件、设置中来源/插件/开发者面板、`sdk/GUIDE.md`。本任务不做任何 UI。

## 先读

- `specs/PLUGIN_SDK.md` §1–§7、§9（全部是本任务范围）、§11；`specs/DATA.md` §1（`plugins/` 目录布局）；`specs/SPEC.md` D5、D10–D12；`specs/LIBRARY.md` 中 EntityRef 编码约定（若有）。
- `native/crates/haiku-jsrt`（M2-01 的 RuntimeOptions、load_package/reload/describe、CallHandle/AbortHandle、宿主模块注册、KV、HTTP 钩子）；`haiku-model` 中已有的 EntityRef/Track 等类型；`haiku-store` 原子写。
- 参考版（只读）`~/Project/GPUI/haiku`：`src/services/{plugins,sources,secrets}.ts`（sources.json 字段、实例 id 规范化、启停/排序语义、凭据保存）、`assets/plugins/subsonic.music`（v1 Subsonic 实现，用作 API 映射依据）。
- 构建：仓库根 `.cargo/config.toml` 已关闭 build-override strip，无需再 export。

## 0. M2-01 遗留

- `haiku-jsrt` cipher 分段 `update` 每次重算累计输入：改为流式（ECB/CBC 保留未满块，CTR 保持计数器，GCM 用增量 API 或在 final 时一次计算但 update 不重复计算），补一个多分段等价性测试。

## 1. `haiku` SDK 运行时模块（注册为 jsrt 宿主模块）

- `definePlugin`、`field.{text,url,secret,...}`、错误类（§6 全部十种，带结构化字段如 `retryAfter`）、`ctx` 构造（§3.4 全部字段）。
- `ctx.http`（§7）：get/post（json/form/body/query/headers/timeout），`.json/.text/.bytes/.response`；合并 `http.headers`、接 `ctx.signal`、默认 15 秒；非 2xx 抛 `Upstream`、429 抛 `RateLimited`，`throwOnError:false` 例外；`http.cookies:true` 时持久化 cookie 罐存于实例 storage；调试追踪记录方法/URL/状态/耗时并脱敏。
- 未捕获普通异常映射 `PLUGIN_ERROR`，保留 stack 与源码位置。

## 2. `sdk/haiku.d.ts`

- 覆盖 §3–§7 全部类型（definePlugin 参数、实体、Page/PageArgs、Context、Auth 五种方式、Connection/Fields、各能力处理函数、错误类、Http）。插件作者只需 `import ... from "haiku"`。
- 用 `tsc --noEmit` 对 subsonic 插件和一个覆盖所有能力的类型夹具做类型检查（tsc 来源说明在汇报中；不引入 npm 运行时依赖到应用）。

## 3. haiku-plugins 宿主（Rust）

- **描述符**：在 jsrt 描述模式中求值，读取 `definePlugin` 返回值生成 `descriptor.json`（id、名称、版本、作者、logo、描述、类别推断 §3.1、能力列表、认证方式、连接字段）；列出与展示只读描述符。
- **Package / Source**：目录布局按 DATA.md（`plugins/packages/<id>/`、`plugins/sources/<sourceId>/{storage.json,credential.json(0600)}`、`plugins/sources.json`）；`sources.json` 字段与启停/排序语义沿用参考版（D5 兼容）；库插件实例 id 为 `pluginId#sha1(identity)[:8]`，identity 规范化规则按 §3.1（单元测试覆盖参考版的规范化样例）。
- **安装事务**（§9）：临时目录解压校验（zip ≤10MB、解压 ≤50MB、路径安全、描述符可生成）→ 替换旧包并保留备份 → 重载使用该包的实例 → 成功删除备份，任一步失败回滚；升级保留 storage/凭据；有实例使用的包不可删除；内置插件首次启动安装、可停用不可删除。
- **认证宿主逻辑**（无 UI）：qrcode（start + 每 2 秒 poll、过期、取消时 abort）、sms、password、cookie 的状态机与凭据保存；web 方式只定义接口（`done` 回调）留给 M2-03 的 webview；`AuthExpired` 单飞 `refresh` 并重放一次，仍失败标记“需要重新登录”；新建来源仅在登录/verify 成功后注册，失败回滚半建实例；`connection.verify` 与 secret 字段保存/脱敏。
- **SourceApi**：类型化 Rust 接口（search 各类、track.get/streams/lyrics、album/artist/playlist/toplist/program/radio/home/categories/me/library/scrape），内部调用 runtime、包装 `EntityRef { source, kind, id }`、结构校验（缺必填字段的条目丢弃并记警告，不使整体失败）、`{size}` 封面模板替换、`Expired` 时重新取 streams。上层只依赖这层接口。
- **App 接线**：`Core::open` 时打开插件宿主并安装内置插件（`plugins/` 下随应用打包的目录，本任务只有 subsonic）；暂不新增 TSX 导出（M2-03 做设置面板时再导出）。

## 4. subsonic v2 插件（`plugins/subsonic/plugin.ts`）

- 库插件：`connection`（baseUrl/username/password，identity baseUrl+username，verify 用 ping）、token+salt 认证、search（track/album/artist）、album.get、artist.get/albums、playlist.get、track.streams（含格式/码率）、track.lyrics（OpenSubsonic 歌词扩展可用时）、library.cards/browse、封面 `{size}` 模板。目标 ≤ 300 行（§1），超出须说明。
- 用本地 HTTP fixture 服务器模拟 Subsonic 响应（record/replay 风格，fixture 文件放 `plugins/subsonic/fixtures/`），不访问外网。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`：cipher 流式等价；描述符生成与类别推断；实例 id 规范化；安装事务的超限/路径逃逸/描述符失败回滚、升级保留 storage/凭据、使用中禁止删除；AuthExpired 单飞刷新+重放（并发调用只刷新一次）；qrcode poll 节奏/过期/取消；结构校验丢弃坏条目；Expired 重取 streams；subsonic 对 fixture 的 verify/search/album/streams/分页/错误映射（401→AuthRequired 或 AuthExpired、404→NotFound、429→RateLimited、5xx→Upstream）、取消后迅速结束。
2. `tsc --noEmit` 检查 `sdk/haiku.d.ts` + subsonic + 全能力类型夹具。
3. `nsh check`、`nsh build`（仅已知例外）、`nsh test`；Headless 或 app 测试证明 `Core::open` 后内置 subsonic 已安装且描述符可读（隔离 `HAIKU_DATA_DIR`）。
4. 收尾：更新 STATUS（F02、F03 宿主部分；追加 M2-02 小节 ≤ 40 行，列出新增依赖），保留改动等待审阅。

## 不做

任何 UI 与 TSX 导出、web 登录的 webview、其他内置插件、`haiku-plugin` CLI、契约测试套件框架（M2-03）；在线内容页面与播放网络流（M4）。

## 汇报

最终回复：SDK/宿主公开 API 概要、subsonic 行数、与 PLUGIN_SDK.md 的偏离、sources.json 与参考版兼容情况、需审阅者决定的事项、未完成项（额度不足时如实列出）。
