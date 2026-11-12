# 任务 M6-07：按 QQMusicApi 重写 qq-music 插件

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**（不要 `/new`，不要切到 worktree）。完成后保留改动并停下汇报，不提交 git。

## 背景

现有 `dev-plugins/qq-music` 是在 2400 行的 `protocol.js` 外面套了一层 v2 外壳。`protocol.js` 从参考版 v1 机械移植而来，返回的是 v1 形状的数据（`trackId: "qq:song:…"`、`hasMore`），`plugin.ts` 再转换一遍。本任务以 `/Users/zsakvo/temp/QQMusicApi`（Python，提交 `108617f`，v0.7.2，只读）为上游协议依据，按 `specs/PLUGIN_SDK.md` 的 v2 范式从头重写。目标是读起来就是“QQ 音乐 API → haiku 实体”的直接映射。

## 先读

- `specs/PLUGIN_SDK.md` §2–§7、§11；`sdk/haiku.d.ts`；`sdk/GUIDE.md`；`plugins/subsonic/plugin.ts`（写法和可读性的样板）。
- 现有 `dev-plugins/qq-music/plugin.ts`、`fixtures/contract.json`：能力清单以它为准，要求不回退。
- QQMusicApi：`qqmusic_api/core/{api_context,request,versioning,client}.py`（请求组装、comm 公共参数、`zzc` 签名、凭据字段）、`algorithms/`（`sign.py`、`qrc_decrypt` 与 `tripledes.py`）、`modules/` 中与下面能力对应的文件、`models/` 中对应的响应模型（字段名以它为准）。只用它理解协议，不在本仓库运行它的测试。

## 1. 文件结构（`dev-plugins/qq-music/`）

删除 `protocol.js`、`protocol.d.ts`。改为以下文件，全部为 TypeScript：

| 文件 | 内容 | 行数上限 |
|---|---|---|
| `plugin.ts` | `definePlugin`：元数据、各能力处理函数、上游响应 → 实体的映射函数 | 400 |
| `api.ts` | 唯一的请求入口：`musicu.fcg` 批量请求、comm 公共参数、签名、凭据注入、上游 `code` → SDK 错误类的映射 | 200 |
| `auth.ts` | QQ/微信二维码、短信、Cookie 登录、刷新、注销 | 250 |
| `crypto.ts` | `zzc` 签名、QRC 解密（QQ 的非标准 3DES）及其他确实需要的算法 | 不限，S 盒等常量表单独放文件末尾 |

- 只移植这些能力用得到的部分。QQMusicApi 的评论、MV、私信、上传、乐谱、QIMEI/设备注册等，不需要就不移植；如果某个能力（例如取播放地址）确实依赖 QIMEI 或设备信息，用最小实现放进 `api.ts` 或 `crypto.ts`，并在汇报中说明原因。
- 平台档案（ct/cv/UA）沿用 QQMusicApi 对每个接口的默认选择；所有接口共用一个请求函数，不为每个接口单独拼装请求。
- 运行时只依赖 `haiku` SDK 和 haiku-jsrt 已有模块（`crypto`、`buffer`、`zlib`）。QQ 的 3DES 是非标准实现，必须自己写；AES/RSA 等标准算法使用 `crypto.createCipheriv`/`publicEncrypt`。
- `fixtures/` 按新的请求形状重写，不保存真实凭据或个人数据。`logo.png` 保留。

## 2. 能力（与现有版本一致，不回退）

id `qq.music`，名称、作者、logo 不变，版本改为 `3.0.0`。

- auth：qrcode（variants `qq`/`wx`）、sms、cookie、user、refresh、logout。凭据是 QQMusicApi `Credential` 中刷新与鉴权必需的字段，由宿主保存，插件不写 `ctx.storage`。
- search：track/album/artist/playlist。
- track：get、streams（按 `ctx.quality`/`ctx.format` 排列候选，从高到低回退；地址失效抛 `Expired`；会员曲目抛 `VipRequired`）、lyrics（有逐字歌词时返回 krc 风格，否则返回 lrc；附带翻译）。
- album/artist/playlist/toplist（list、get）：详情加曲目游标分页。
- home、categories（list、playlists）、me（playlists 用 `owned` 区分自建与收藏，likedTracks）。
- scrape：search、lyrics。
- QQMusicApi 提供私人电台或相似歌曲时，可以顺带实现 `radio.next`（不超过 40 行）；做不到就跳过并在汇报中说明。

## 3. 代码要求

- **干净、简洁**：每个处理函数尽量只做“调用一个 API + 映射”。重复出现的结构（游标分页、`{ info, tracks }` 详情、搜索）各提炼一个小函数，不写通用的分派器或配置表驱动框架。
- 只用原生 id，不出现 `qq:song:` 一类的前缀，也不保留 v1 的数据形状。
- 类型：为用到的上游响应字段写最小的 `interface`（只写实际读取的字段），不用 `Record<string, any>` 贯穿全文件。
- **注释**：每个文件开头用 2–4 行说明职责；上游的特殊约定（module/method 的用途、魔法数字、音质前缀表、`dirid 201` 表示“我喜欢”、加密方式等）就地写一行注释说明含义或出处；显而易见的代码不加注释。所有从 QQMusicApi 移植的文件在开头注明 `Ported from QQMusicApi 108617f (GPL-3.0)`。
- 不要为兼容旧版 protocol 留垫片或死代码。

## 验收（每项跑一次）

1. `native/tools/haiku-plugin` 的 `check` 与 `test` 对 `dev-plugins/qq-music` 通过（离线 fixture 契约覆盖第 2 节的全部能力，包括 QR 过期、SMS cooldown、`AuthExpired` → refresh 重放、`VipRequired`、`Expired`）。
2. `tsc --noEmit` 检查全部插件（`sdk/tsconfig.json`）。
3. `cargo test --manifest-path native/Cargo.toml -p haiku-plugins`（`tests.rs` 中引用 qq-music 的用例按新结构更新）。
4. 补充证据（允许失败，不阻塞验收）：`haiku-plugin test --live` 只跑不需要登录的能力（搜索、榜单、歌词、首页）一次，日志存档。
5. `wc -l dev-plugins/qq-music/*.ts` 写进日志，确认符合第 1 节的行数上限。
6. 收尾：STATUS 中 F03 行更新证据路径，追加 M6-07 小节（≤ 40 行）；日志放 `.artifacts/m6-07/`。

## 不做

不改宿主、SDK、`haiku.d.ts`（发现 SDK 缺能力时停下汇报，不要自行扩展）；不改 `plugins/qq-scraper`；不做 UI；不做真实账号登录。

## 汇报

最终回复：文件与行数、移植了 QQMusicApi 的哪些接口（module.method 列表）、放弃或简化的部分及原因、新旧能力差异、契约与 live 结果、待人工确认项（真实扫码/短信/VIP 播放）。
