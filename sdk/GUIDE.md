# Haiku 插件开发指南

## 1. 来源、实例、实体与游标

一个包声明一个插件 `id`，例如 `subsonic.music`。安装包只提供能力，不会自动登录。
平台插件（`auth`）的来源 ID 为插件 ID；库插件（`connection`）可以连接多台服务器、
多个账号，宿主根据 `identity` 字段派生稳定的来源 ID。修改插件 ID 会改变已有来源身份。

插件返回上游原始实体 ID。宿主把来源 ID 和原始 ID 组合成 `EntityRef`；不要在插件中解析
或自行构造宿主的 EntityRef。曲目必须有 `id`、`title`、`artists`；只有名字的艺术家可省略 ID。
时长单位为秒。封面是 URL，可以带 `{size}` 占位符；播放鉴权头放在 Stream.headers 中。

分页返回 `{ items, next?, total? }`。`next` 是不透明字符串；结束时省略它，不要返回对象、
重复游标或不断返回同一页。详情中的 `tracks` 也必须分页。宿主的全量聚合最多读取 100 页。

## 2. 从 Subsonic 开始

完整、可运行的库插件在 [`plugins/subsonic/plugin.ts`](../plugins/subsonic/plugin.ts)。
它使用用户名、密码和服务器地址连接，认证盐和 token 缓存在实例 KV 中。

包的入口必须是 `plugin.ts`，默认导出 `definePlugin(...)`。下面是该插件的实际定义片段：

```ts
export default definePlugin({
  id: 'subsonic.music',
  logo: 'logo.png',
  name: 'Subsonic',
  version: '2.0.0',
  author: 'Haiku',
  description: 'Subsonic / OpenSubsonic music library',
```

实际连接字段与身份定义：

```ts
  connection: {
    fields: {
      baseUrl: field.url('服务器地址'),
      username: field.text('用户名'),
      password: field.secret('密码'),
    },
    identity: ['baseUrl', 'username'],
    async verify(conn, ctx) {
      const result = await call(ctx, 'ping', {}, conn);
      ctx.storage.set('subsonic.open', !!result.openSubsonic);
      return {
        name: conn.username,
      };
    },
  },
```

`verify` 成功之后才会注册来源。失败、关闭对话框或取消时，宿主丢弃临时实例。
编辑连接会重新执行 verify；secret 字段只在输入时显示掩码，导出的连接字段会被脱敏。

最小插件也可以由 CLI 生成，详见第 6 节。先实现一个可测试的能力，再逐步添加可选能力。
定义缺少某个能力时应省略它；不要用“成功但永远为空”的实现代替不支持。

## 3. 连接与五种平台认证

库连接使用 `connection`，不同时声明 `auth`。平台认证的五种入口都在 `auth` 中，
返回 `Login = { credential, user? }`；`auth.user(ctx)` 返回当前用户或 null。
当前用户应有可展示的 name。完整类型见 [`haiku.d.ts`](haiku.d.ts)。

| 方式 | 插件实现 | 宿主行为 |
| --- | --- | --- |
| 二维码 | `qrcode.start(variant, ctx)` 返回 image 或 url、state、expiresIn；poll 返回 waiting/scanned/expired/done | url 在 Rust 生成 PNG；每 2 秒轮询；过期可刷新，关闭即取消 |
| 短信 | `sms.send(phone, ctx)` 返回 state、cooldown；`verify(state, code, ctx)` 返回 Login | 保存 state，限制冷却期内重复发送；输入验证码后验证 |
| 密码 | `password.fields` 声明字段，`submit(values, ctx)` 返回 Login | 按 fields 生成表单，secret 输入不回显明文 |
| Cookie | `cookie.submit(cookie, ctx)` 校验并返回 Login | 用户手动粘贴 Cookie；不要把输入记录到日志 |
| web | `web.url`、`web.done({url,cookies}, ctx)` 返回 Login 或 null | 当前 NSH 不支持独立会话和读取 Cookie，使用手动粘贴 Cookie/完成 URL 的降级表单 |

`state` 必须可 JSON 序列化，不要保存函数、Response 或 AbortSignal。二维码 URL 本身可能
带临时登录令牌，只用于渲染，不应写入日志或作为发布 fixture 中的真实凭据。
宿主负责 `credential.json` 的 0600 权限、原子保存和清理。不要把平台凭据复制进 KV。

遇到凭据过期时抛出 `AuthExpired`；宿主单飞调用 `auth.refresh(ctx)`，保存返回的新凭据并
重放原调用一次。刷新失败会保留“需要重新登录”状态。`auth.logout` 可做远端注销；
即使远端出错，宿主也清除本地凭据。请求始终尊重 `ctx.signal`。

[`dev-plugins/qq-music`](../dev-plugins/qq-music/plugin.ts) 展示 QQ/微信二维码、短信和 Cookie。
它复用参考版 QQ 协议，v2 入口负责实体、游标和认证适配，且不会默认安装。
短信遇到上游额外图形安全验证时会提示使用扫码；当前没有内嵌验证码浏览器。

## 4. 能力与数据约束

| 能力 | 输入与返回约束 |
| --- | --- |
| `search.track/album/artist/playlist/program` | 第一个参数为 keyword、limit、cursor；返回对应实体 Page |
| `track.get` | 原始曲目 ID → Track；搜索结果的 ID 必须能 round-trip |
| `track.streams` | ID，ctx.quality/format → 有序 Stream[]；带可用 URL、必要 headers，可带 expiresAt（Unix 秒） |
| `track.lyrics` | ID → Lyrics 或 null；format 为 lrc/krc/yrc/ttml/plain，翻译可选 |
| `album.get/artist.get/playlist.get/toplist.get/program.get` | ID、ctx.limit/cursor → `{ info?, tracks: Page<Track> }` |
| `artist.tracks/albums` | ID、ctx.limit/cursor → 对应实体 Page |
| `toplist.list` | 返回 Toplist[]，其 ID 能被 toplist.get 使用 |
| `home` | ctx.cursor/refresh → shelves、next；type 仅 tracks/albums/playlists/toplists/programs/entries |
| `categories.list/playlists` | 返回 groups 与可选 sorts: {id,title}[]；未提供 sorts 时宿主隐藏排序；分类 ID → Playlist Page，ctx.sort 使用声明的 id |
| `me.playlists/likedTracks` | 当前用户的 Playlist/Track Page，未登录抛 AuthRequired |
| `library.cards/browse` | 声明库入口、实体类型；按卡片 ID 返回实体 Page |
| `radio.channels/next` | 电台列表；next(channelId: string | undefined, ctx) → Track[]，undefined 为私人 FM，普通频道 ID 必须非空；ctx.history/count 用于续歌 |
| `scrape.search/lyrics` | title、artists、album/duration → 元数据候选；候选 → Lyrics 或 null |

Shelf 的 `more` 与 Entry 的 `to` 使用 SDK Link（playlist/album/toplist/radio/category）。
不要把完整 URL 当作内部导航 ID。无 ID 的艺术家只用来显示名字；不应伪造可点击详情。
刮削候选使用原始 ID，保持时长、年份、曲序等字段单位一致；宿主负责评分、去重与选择。

`ctx.http` 提供 get/post 的 `.json()`、`.text()`、`.bytes()`、`.response()`，支持 query、
headers、json、form、timeout。默认 headers/cookie 罐由插件 http 定义控制；HTTP 失败要转换
为 SDK 错误。二进制响应使用 bytes，不经过 UTF-8 文本。不要在模块顶层发起网络请求。

Subsonic 中的分页实现直接取自实际代码：

```ts
function remotePage<T>(items: T[], args: PageArgs): Page<T> {
  return {
    items,
    next: items.length === args.limit ? String(Number(args.cursor || 0) + args.limit) : undefined,
  };
}
```

运行时提供受限的 crypto/buffer/zlib，以及 timers 和 fetch。AES ECB/CBC/CTR 支持增量更新；
GCM 为当前已验收子集（12 字节 IV），RSA OAEP 为 SHA-1/256；HTTP 响应体限制 16 MiB，
没有 ReadableStream/FormData。不要假设 Node 的全部 API 都存在。
只允许包内 `./` 相对导入，禁止绝对路径与 `../`。包可以同时包含 TS、JS 与静态资源。

## 5. 错误与恢复

直接抛出从 `haiku` 导入的错误，字符串或结构化参数均可。不要用字符串 JSON 模拟 SDK 错误。

| 错误 | 含义与恢复 |
| --- | --- |
| AuthRequired / AuthExpired | 需要登录 / 尝试刷新后重放一次 |
| VipRequired / GeoRestricted | 订阅或地区限制，明确告知用户 |
| NotFound | 原始实体不存在，不做空成功处理 |
| RateLimited | 可携带 retryAfter（秒），等待后重试 |
| Upstream | 上游 HTTP/业务失败，可携带 status |
| Expired | 播放 URL 过期，宿主重新请求一次 streams |
| ParseError | 响应格式或数据不符合预期 |
| Unsupported | 当前服务/实现不支持此能力或流程 |

宿主保留 JS 错误类型、stack、源文件位置和 details。开发者面板显示结构校验 warnings，
严格契约则拒绝缺字段、非法游标、重复 ID 等问题。取消不是上游故障；不要吞掉 AbortError。

## 6. 调试与离线测试

先构建 CLI：`cargo build --manifest-path native/Cargo.toml -p haiku-plugin`。
命令位于 `native/target/debug/haiku-plugin`；可将该目录加入 PATH。

```sh
haiku-plugin new example --template library
haiku-plugin check example
haiku-plugin test example
```

模板可选 provider/library/scraper。`check` 调用 TypeScript 编译器，再读取运行时描述符；
设置 `HAIKU_TSC=/path/to/tsc.js` 可选已有编译器，`HAIKU_SDK_PATH` 可指定声明文件。
仓库全插件检查为 `tsc --noEmit -p sdk/tsconfig.json`。

契约文件位于 `fixtures/contract.json`。`routes` 描述 path/method/query、status/headers/body，
可用 `requestJson` 部分匹配请求 JSON，以区分 musicu 这样的共享 CGI 地址。`cases` 列出
path、args、context，可指定 schema、expectedCode、status、abortAfterMs；expectedJson 可部分匹配返回值，数组匹配前缀。
`login` 可提供 fixture Cookie 或密码；库插件使用 `connection`。所有值必须是假数据。
参照 Subsonic、QQ 的实际 fixture，不要为真实账号录制凭据。

默认 replay 在 jsrt fetch 层拦截，包括裸 fetch；未匹配请求返回 599 并导致契约失败，
不会回退到真实上游。契约覆盖所有声明的内容能力，自动遍历列表与 detail.tracks，
检查 ID 回查、游标终止和结构。报告 JSON 写 stdout，摘要写 stderr；失败返回非零退出码。

`--live` / `--record` 仍为基础模式，需要主动选择且会访问真实网络。录制不完整覆盖
响应序列/复杂认证，响应内容也可能包含敏感信息；发布前必须人工清理，不能视为自动脱敏工具。
本任务的验收只使用 replay。

设置 → 开发者可以加载本地目录。加载后每 500ms 检查内容变化，变更通过安装事务热重载，
保留实例 storage；语法/描述符错误时保留之前的可用包。“停止监听”或关闭页面会清理订阅。
自动表单来自描述符的标准 SDK 参数签名；复杂认证/自定义输入可切换原始 JSON。
选择实例、能力并填写参数，返回值和 warnings 在树形结果中显示；日志与 HTTP 追踪使用虚拟列表。
面板也可以查看/清除 KV、清除凭据、运行包内离线契约。它不会显示凭据明文。

## 7. 发布与兼容性

使用稳定的小写插件 ID 和有效语义化版本；入口保持 plugin.ts，附必要模块与 logo。
logo 支持包内相对路径或 HTTPS；本地文件受包边界和 512 KiB 大小限制。
提交前检查所有能力、错误、分页与认证的离线 fixture，删除账号、Cookie、密钥和个人数据。

分发目录或 ZIP；应用插件面板支持系统选择 ZIP、目录、单个 .ts，也可输入 HTTPS 包地址。
单文件导入只适用于无额外相对模块/资源的插件；QQ 应按目录或 ZIP 导入。
压缩包路径不能逃逸包根目录，压缩/展开大小限制分别为 10/50 MiB；不要携带 node_modules。
内置包不可删除，有来源引用的第三方包也不可删除，先移除来源。

发布接口兼容的更新，尽量保持原始 ID、identity 字段和 KV 格式。复杂迁移要自行兼容旧值。
真实账号、二维码手机确认、平台风控、VIP/地区限制、飞牛中继和系统选择器仍需人工验收。
