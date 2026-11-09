# 插件范式 v2 与 `haiku` SDK

## 1. 设计目标

参考版的 v1 插件（QQ 72KB、Subsonic 39KB、飞牛 39KB）里，大部分代码在重复实现宿主本应提供的能力：

| v1 的负担 | v2 的处理 |
|---|---|
| 每个插件都手写 `musicError()`，把 JSON 塞进 `Error.message` | SDK 提供错误类：`throw new NotFound()` |
| 插件自己管理登录会话 Map、sessionId、轮询状态，还要手写 `localStorage` 凭据 | 插件只写 `start` / `poll` / `submit`；会话、轮询节奏、过期、取消、凭据保存都由宿主负责 |
| 凭据过期后在每个函数里各自“刷新再重试” | 插件提供一个 `auth.refresh`；任何调用抛出 `AuthExpired` 时，宿主单飞刷新并重放一次 |
| 每个函数各算 `page/pageSize/hasMore/total` | 统一游标分页 `{ items, next? }` |
| 手动拼接和解析 `qq:playlist:xxx`，一个 `getPlaylist` 分派多种实体 | 插件只处理原生 id；宿主负责命名空间；每种实体单独一个处理函数 |
| manifest 中的 `capabilities`/`searchTypes`/`idSchemes` 与导出函数重复 | 能力由插件定义推断，不需要声明 |
| `getCoverArt` 需要一次额外调用 | 封面 URL 支持 `{size}` 模板，宿主自行替换 |
| 运行时缺 AES，插件手写分组密码 | haiku-jsrt 提供 `crypto.createCipheriv` 等（§8） |
| 调用无法取消 | 每次调用都带 `ctx.signal`，SDK 的 `http` 自动接入 |

目标：**一个 Subsonic 插件应在 300 行 TypeScript 内完成**，并且读起来就是 Subsonic API 的映射。

## 2. 插件的形态

### 2.1 文件

插件就是**一个 `.ts` 文件**。需要本地 logo 或拆分模块时，可以是一个目录（或该目录的 zip）：

```
subsonic/
  plugin.ts        # 入口，固定文件名
  logo.png         # 可选
  api.ts           # 可选，入口可以相对路径 import
```

不再需要 `manifest.json`：元数据写在 `definePlugin` 里。安装时宿主在一个禁止调用处理函数的临时 runtime 中求值模块，读取 `definePlugin` 的返回值，生成**描述符**（id、名称、版本、能力列表、认证方式、连接字段等），缓存为包目录下的 `descriptor.json`。列出插件、展示能力时只读描述符，不执行插件代码。

> 已确认 Q1：单文件插件，元数据写在 `definePlugin` 中，描述符由宿主生成。

### 2.2 最小示例

```ts
import { definePlugin, NotFound } from "haiku";

export default definePlugin({
  id: "example.radio",
  name: "示例电台",
  version: "1.0.0",

  search: {
    async track({ keyword, cursor }, ctx) {
      const page = Number(cursor ?? 1);
      const res = await ctx.http.get("https://api.example.com/search", {
        query: { q: keyword, page },
      }).json<ApiSearch>();
      return {
        items: res.songs.map(toTrack),
        next: res.hasNext ? String(page + 1) : undefined,
      };
    },
  },

  track: {
    async streams(id, ctx) {
      const s = await ctx.http.get(`https://api.example.com/song/${id}`).json<ApiSong>();
      if (!s.url) throw new NotFound();
      return [{ url: s.url, format: "mp3", quality: "standard" }];
    },
  },
});

const toTrack = (s: ApiSong) => ({
  id: s.id,
  title: s.name,
  artists: s.artists.map((a) => ({ id: a.id, name: a.name })),
  album: s.album && { id: s.album.id, title: s.album.name },
  cover: s.cover,
  duration: s.durationMs / 1000,
});
```

这个插件自动具备“歌曲搜索”和“播放”能力，界面只展示这两项。

## 3. `definePlugin` 结构

```ts
definePlugin({
  // ---- 元数据 ----
  id: string;                 // 反向域名风格，[a-z0-9.-]
  name: string;
  version: string;            // semver
  author?: string;
  logo?: string;              // 包内相对路径或 https URL
  description?: string;

  // ---- 共享 HTTP 默认值 ----
  http?: { headers?: Record<string, string>; timeout?: number; cookies?: boolean };

  // ---- 连接（有此项即为“库”插件：可多实例，如 Subsonic、飞牛） ----
  connection?: Connection;

  // ---- 账号（provider 平台的登录） ----
  auth?: Auth;

  // ---- 内容能力（按需实现，未实现即不支持） ----
  search?: Partial<Record<SearchKind, SearchHandler>>;   // track album artist playlist program
  track?:    { get?, streams, lyrics? };
  album?:    { get };          // 详情 + 曲目分页
  artist?:   { get, tracks?, albums? };
  playlist?: { get };
  toplist?:  { list?, get };
  program?:  { get };          // 播客/节目专辑
  radio?:    { channels?, next };
  home?:     HomeHandler;      // 首页货架
  categories?: { list, playlists };
  me?:       { playlists?, likedTracks? };   // 需要登录的用户内容
  library?:  { cards, browse };              // 库插件的浏览入口

  // ---- 刮削（为本地曲库提供元数据与歌词候选） ----
  scrape?:   { search, lyrics? };
})
```

### 3.1 插件类别由定义推断

| 定义了 | 宿主行为 |
|---|---|
| `connection` | 库来源：可以添加多个实例，实例 id 为 `pluginId#sha1(identity)[:8]`（identity 规范化规则同参考版：URL 去空白、补协议、去末尾斜杠和 `rest/app`，scheme/host 小写，用户名保留大小写） |
| 无 `connection` 且有内容能力 | 平台来源：单实例 |
| `scrape` | 出现在刮削器列表中。同一个插件可以同时是平台来源和刮削器（例如 QQ 音乐插件不必再拆成 `qq.music` 和 `qq.scraper` 两个包） |

### 3.2 实体模型

插件返回的所有实体只使用**原生 id**。宿主将其包装为 `EntityRef { source, kind, id }`，`source` 是来源实例 id，路由和缓存的编码由宿主统一负责（往返无损，同参考版 F01 要求）。

```ts
type Track = {
  id: string; title: string;
  artists: Artist[] | string[];            // 有 id 的歌手可以点击进入
  album?: { id?: string; title: string };
  cover?: Image; duration?: number;        // 秒
  quality?: Quality[];                     // 可用音质（下载/播放选择）
  explicit?: boolean; extra?: Json;        // extra 原样回传给插件
};
type Album    = { id; title; artists?; cover?; year?; trackCount?; description?; extra? };
type Artist   = { id?; name; cover?; description? };
type Playlist = { id; title; cover?; creator?; trackCount?; description?; owned?: boolean };
type Program  = { id; title; cover?; host?; episodeCount?; description? };
type Toplist  = { id; title; subtitle?; cover? };
type Image    = string;   // URL；可以包含 {size} 占位符，宿主按需要的边长替换
```

- `artists` 允许直接返回字符串数组，作为最简写法。
- `Playlist.owned`：`true` 为用户自建，`false` 为收藏；用于 F03“自建/收藏歌单”分组（v1 无法表达）。
- 结构校验由宿主完成：缺少必填字段的条目会被丢弃，并在调试面板中给出警告，不会导致整个结果失败。

### 3.3 分页

```ts
type Cursor = string;
type Page<T> = { items: T[]; next?: Cursor; total?: number };
type PageArgs = { cursor?: Cursor; limit: number };
```

- 处理函数收到 `{ cursor, limit }`，返回 `next` 表示还有下一页；游标内容由插件决定，宿主不解析。
- 需要完整列表的场景（例如收藏远端歌单需要完整快照）由宿主循环翻页，最多 100 页；超过上限仍有下一页时报错（同参考版 §6）。
- 详情类返回值为 `{ info, tracks: Page<Track> }`，第一次调用不带 cursor 同时返回 info；后续翻页只需要 tracks。

### 3.4 处理函数签名

所有处理函数的最后一个参数都是 `ctx`：

```ts
interface Context {
  signal: AbortSignal;          // 用户切走、超时、来源卸载时触发
  http: Http;                   // 已合并 http.headers、自动接入 signal、可选 cookie 罐
  storage: KV;                  // 实例私有的持久化键值（JSON），替代 localStorage
  credential: Json | null;      // 当前登录凭据（只读）；由宿主在登录成功后保存
  connection: Record<string, string> | null;  // 库插件的连接字段
  locale: string;               // "zh-CN"
  log: Logger;                  // 写入插件日志频道，自动脱敏
  crypto: typeof import("crypto");
}
```

## 4. 认证

### 4.1 平台账号 `auth`

```ts
auth: {
  qrcode?: {
    variants?: Record<string, string>;      // { qq: "QQ", wx: "微信" }
    start(variant, ctx): Promise<{ image?: string; url?: string; state: Json; expiresIn?: number }>;
    poll(state, ctx): Promise<
      | { status: "waiting" | "scanned" }
      | { status: "expired" }
      | { status: "done"; credential: Json; user?: User }>;
  };
  sms?: {
    send(phone, ctx): Promise<{ state: Json; cooldown?: number }>;
    verify(state, code, ctx): Promise<{ credential: Json; user?: User }>;
  };
  password?: { fields?: Fields; submit(values, ctx): Promise<Login> };
  cookie?:   { submit(cookie: string, ctx): Promise<Login> };
  web?:      { url: string; userAgent?: string; done(page: { url: string; cookies: Cookie[] }, ctx): Login | null };
  user(ctx): Promise<User | null>;          // 用当前凭据读取用户信息，null 表示未登录
  refresh?(ctx): Promise<Json>;             // 返回新凭据
  logout?(ctx): Promise<void>;              // 可选的远端注销；本地凭据由宿主清除
}
type Login = { credential: Json; user?: User };
type User  = { id?: string; name: string; avatar?: string; vip?: string };
```

宿主负责：

- **二维码**：调用 `start` 得到二维码（`image` 为 data URL，或由宿主把 `url` 生成为二维码），每 2 秒调用一次 `poll`；处理过期刷新、用户取消、对话框关闭，并 abort 进行中的调用。
- **web 登录**：用独立会话的 webview 打开 `url`，每次导航完成都调用 `done`，返回非 null 即为成功并关闭页面。
- **凭据**：成功后保存凭据（实例目录下 `credential.json`，权限 0600，见 Q2），之后注入 `ctx.credential`。
- **自动刷新**：任何处理函数抛出 `AuthExpired` 时，宿主单飞调用 `refresh` 保存新凭据，再重放原调用一次；仍失败则把来源标记为“需要重新登录”。
- 新建来源时，只有登录成功才注册实例；取消或失败时回滚半建的实例（同参考版 F03）。

> 已确认 Q2：凭据保存在实例目录 `credential.json`（0600），备份导出时提示含明文凭据；系统钥匙串作为后续增强。

### 4.2 库连接 `connection`

```ts
connection: {
  fields: {
    baseUrl:  field.url("服务器地址", { placeholder: "http://192.168.1.10:4533" }),
    username: field.text("用户名"),
    password: field.secret("密码"),
  },
  identity: ["baseUrl", "username"],      // 决定实例 id
  async verify(conn, ctx): Promise<{ user?: User; name?: string }>,   // 能连上即成功
}
```

连接字段即凭据：`secret` 字段与凭据同样保存，日志中不出现。编辑连接时重新执行 `verify`。

## 5. 内容能力

### 5.1 播放

```ts
track: {
  streams(id, ctx & { quality: Quality; format?: Format }): Promise<Stream[]>;
  lyrics?(id, ctx): Promise<Lyrics | null>;
  get?(id, ctx): Promise<Track>;
}
type Stream = {
  url: string; headers?: Record<string, string>;     // 自动合并 http.headers
  format?: "mp3" | "flac" | "m4a" | "ogg" | "opus";
  quality?: Quality; bitrate?: number; size?: number;
  expiresAt?: number;                                // unix 秒；过期前宿主不重复调用
};
type Quality = "standard" | "high" | "lossless" | "hires";
type Lyrics = { format: "lrc" | "krc" | "yrc" | "ttml" | "plain"; text: string; translation?: string };
```

- 返回的候选按优先级排序，播放器依次尝试（参考版 F06 的候选回退）。
- 播放地址失效时抛出 `Expired`，宿主重新调用 `streams`；不再需要单独的 `refreshPlayback`。
- 不支持的容器（例如 Opus，symphonia 0.5 无法解码）由宿主判定为不可用候选并跳过。

### 5.2 发现

```ts
home(ctx & { cursor?: Cursor; refresh: boolean }): Promise<{ shelves: Shelf[]; next?: Cursor }>;
type Shelf =
  | { id; title; type: "tracks";    items: Track[];    more?: Link }
  | { id; title; type: "playlists"; items: Playlist[]; more?: Link }
  | { id; title; type: "albums";    items: Album[];    more?: Link }
  | { id; title; type: "toplists";  items: Toplist[] }
  | { id; title; type: "programs";  items: Program[] }
  | { id; title; type: "entries";   items: Entry[] };   // 快捷入口，包括持续电台
type Entry = { title; subtitle?; cover?; to: Link };
type Link  = { playlist: string } | { album: string } | { toplist: string } | { radio: string } | { category: string };

categories: {
  list(ctx): Promise<{ groups: { title: string; items: { id; title; cover? }[] }[] }>;
  playlists(id, ctx & PageArgs & { sort?: string }): Promise<Page<Playlist>>;
};
radio: {
  channels?(ctx): Promise<{ id; title; cover? }[]>;
  next(channelId, ctx & { history: Track[]; count: number }): Promise<Track[]>;   // 持续续歌
};
```

缓存（首屏缓存 7 天、最多 40×60 条、刷新时保留电台）、来源错误隔离、空货架补齐都是宿主行为，同参考版 F04。

### 5.3 用户内容与库浏览

```ts
me: {
  playlists?(ctx & PageArgs): Promise<Page<Playlist>>;      // 用 owned 区分自建与收藏
  likedTracks?(ctx & PageArgs): Promise<Page<Track>>;
};
library: {
  cards: { id: string; label: string; icon?: Icon; entity: "track" | "album" | "artist" | "playlist" }[];
  browse(cardId, ctx & PageArgs & { sort?: string }): Promise<Page<Track | Album | Artist | Playlist>>;
};
```

### 5.4 刮削

```ts
scrape: {
  search(query: { title: string; artists: string[]; album?: string; duration?: number }, ctx)
    : Promise<ScrapeCandidate[]>;
  lyrics?(candidate: ScrapeCandidate, ctx): Promise<Lyrics | null>;
};
type ScrapeCandidate = { id; title; artists: string[]; album?; year?; cover?: Image; duration?; track?: number; extra? };
```

打分（自动采用阈值 0.86 且领先 0.08，0.55 以上进入待审）、并发（4）、单源超时（8 秒）由宿主统一实现，插件只负责返回候选。

## 6. 错误

```ts
import { AuthRequired, AuthExpired, VipRequired, GeoRestricted, NotFound,
         RateLimited, Upstream, Expired, ParseError, Unsupported } from "haiku";

throw new VipRequired("该歌曲需要会员");
throw new RateLimited({ retryAfter: 30 });
```

- 错误类型与 v1 错误码一一对应；`AuthExpired` 与 `Expired` 会触发宿主的自动恢复。
- 未捕获的普通异常映射为 `PLUGIN_ERROR`，保留 stack，并在调试面板中显示源码位置。
- `ctx.http` 对非 2xx 响应默认抛出 `Upstream`（429 抛 `RateLimited`）；需要自行处理时使用 `{ throwOnError: false }`。

## 7. `ctx.http`

```ts
ctx.http.get(url, { query?, headers?, timeout? })
ctx.http.post(url, { json? | form? | body?, query?, headers? })
  .json<T>() | .text() | .bytes() | .response()   // response() 可读取 status、headers、set-cookie
```

- 基于 haiku-jsrt 的 fetch 实现；自动合并 `http.headers`，自动接入 `ctx.signal`，默认超时 15 秒。
- `http.cookies: true` 时为每个实例维护一个持久化 cookie 罐（保存在实例 storage 中）。
- 每个请求的方法、URL、状态码、耗时记录到调试追踪（URL 中的凭据、query 中的敏感参数脱敏）。

## 8. 插件运行时底层：`haiku-jsrt`

D11（修订）：不依赖 cjrt-workspace，在本仓库实现一个参照 CJRT 能力的运行时 crate。它只依赖 NSH 已经链接的 QuickJS：`quickjs-jit`（longbridge 维护的 rquickjs 分支，与 NSH 同版本）和 `quickjs-jit-stdlib`（vendored 的 LLRT 标准模块），进程内只有一份引擎。`haiku-jsrt` 不包含任何音乐语义。

### 8.1 复用与自建

| 能力 | 来源 |
|---|---|
| `buffer`、`crypto`（含 `crypto.subtle`、hash、hmac、随机数）、`zlib`、`path`、`url`、`timers`、`abort`（AbortController）、`events`、编码 | 复用 `quickjs-jit-stdlib` |
| 每实例一个 runtime 的调度与生命周期、调用队列 | 自建，对标 CJRT 的 `PluginHost` 和 `cjrt_core` 调度器 |
| `fetch` / `Headers` / `Request` / `Response` | 自建（stdlib 中没有）；基于 reqwest + rustls，支持 AbortSignal 和请求钩子 |
| `console` | 自建，输出到插件日志频道并脱敏 |
| `localStorage` 风格的实例存储 | 自建；SDK 中以 `ctx.storage` 暴露 |
| `crypto.createCipheriv`/`createDecipheriv`、`publicEncrypt` | 自建，作为 stdlib `crypto` 的扩展（stdlib 中没有） |
| TS 加载 | 自建，使用 oxc 剥离类型 |

### 8.2 能力清单

| # | 能力 | 用途 |
|---|---|---|
| R1 | 运行时实例：每个实例运行在专用线程上，独立的 Runtime/Context；创建、加载、卸载、销毁；与 NSH 的 UI VM 完全隔离 | D12 |
| R2 | 模块加载：入口与相对路径 import；剥离 `.ts` 类型（oxc），用 source map 还原错误行号；只允许包目录内的路径 | D10 |
| R3 | 宿主内置模块注册：以源码或 Rust 绑定注入 `haiku` 等模块 | SDK 运行时 |
| R4 | 调用：按导出路径调用，参数与返回值经 serde 转换；单次调用超时；返回结构化错误（JS 异常的 name/message/stack/位置） | SourceApi |
| R5 | 取消：宿主可以 abort 进行中的调用，JS 侧表现为 `AbortSignal`，fetch 随之中止 | `ctx.signal` |
| R6 | 每次调用注入上下文对象 | `ctx` |
| R7 | `fetch` 与请求钩子（观察请求和响应的元数据） | `ctx.http`、调试追踪 |
| R8 | `console` 与日志通道 | 插件日志 |
| R9 | 实例存储（持久化 KV，原子写入） | `ctx.storage`，备份时保留 |
| R10 | 加解密扩展：`createCipheriv`/`createDecipheriv`（aes-128/192/256 的 ecb/cbc/ctr/gcm）、`publicEncrypt`（RSA PKCS#1 v1.5 / OAEP） | QQ、网易等平台 |
| R11 | 资源上限：内存上限、通过 interrupt handler 实现的 CPU 时间上限；“描述模式”（求值模块但禁止调用处理函数） | 安装时生成描述符、防止插件卡死 |
| R12 | 热重载：替换模块代码但保留存储 | 调试面板 |

每项都有单元测试；R1、R4、R5、R7 另外还要在完整应用中（与 NSH 同进程）做并发测试。

## 9. 宿主实现（`haiku-plugins` crate）

- `Package`：已安装的插件包（源码 + 资源 + 描述符），位于 `plugins/packages/<id>/`。
- `Source`：运行中的来源实例（一个 runtime），位于 `plugins/sources/<sourceId>/`，包含 `storage.json`、`credential.json`。
- 安装事务：在临时目录解压并校验（大小 10MB、解压后 50MB、路径安全、描述符可生成）→ 替换旧包，保留备份 → 重新加载使用该包的实例 → 全部成功后删除备份，任一步骤失败则回滚。
- 升级保留 storage 与凭据；仍有实例在使用的包不允许删除。
- 内置插件随应用打包（`plugins/`），首次启动安装；内置插件可以停用，不能删除。
- 启停、排序、日志沿用参考版 F02 语义。
- `SourceApi`：类型化的 Rust 接口（`search`、`album`、`streams`……），内部调用 runtime 并校验结构；上层（playback、services）只使用这层接口，不接触 JSON。

## 10. 开发支持

### 10.1 脚手架与 CLI（`native/tools/haiku-plugin`）

```sh
haiku-plugin new my-source --template provider   # provider | library | scraper
haiku-plugin check ./my-source                   # 类型检查（调用 tsc 读取 sdk/haiku.d.ts）+ 描述符生成
haiku-plugin test  ./my-source [--live]          # 契约测试
```

模板包含 `plugin.ts`、`tsconfig.json`（指向 `haiku.d.ts`）、`fixtures/` 目录和 README。

### 10.2 契约测试套件

在真实 haiku-jsrt runtime 中运行，覆盖：

- 描述符：元数据完整，推断出的能力与实现一致。
- 结构：每个已实现能力的返回值符合 schema；id 往返（列表中的 id 能被对应的 `get` 使用）。
- 分页：连续翻页不重复、最终结束、游标可以序列化。
- 错误：使用 fixture 模拟 401、404、429、5xx，检查映射到的错误类型；`AuthExpired` 能触发刷新并重放。
- 取消：调用中 abort 后迅速结束。
- 默认使用录制的 HTTP fixture（record/replay），可以离线运行；`--live` 访问真实上游，结果仅作为补充证据。

### 10.3 应用内调试面板（设置 → 开发者）

- 加载本地插件目录，监听文件变化并热重载（C8）。
- 按描述符列出每个能力，自动生成参数表单，单独调用并以树形查看返回值和结构警告。
- 实时查看插件日志与 HTTP 追踪；查看和清除实例 storage 与凭据。
- 一键运行契约测试并查看报告。

### 10.4 编写指南（`sdk/GUIDE.md`）

内容：概念（来源、实例、实体、游标）→ 最小插件 → 认证的五种方式 → 每个能力的语义与约束 → 错误与恢复 → 调试与测试 → 发布。以 Subsonic 插件作为完整示例逐段讲解。

## 11. 内置插件重写

| 插件 | 类别 | 备注 |
|---|---|---|
| `subsonic` | 库 | 范式的完整示例与指南样板 |
| `feiniu` | 库 | |
| `qq-music` | 平台 + 刮削 | 放在 dev-plugins，不默认安装；同时验证 qrcode/sms/cookie 三种认证 |
| `qq-scraper`、`kugou-scraper` | 刮削 | 独立保留，见 Q3 |

> 已确认 Q3：保留独立的轻量 `qq-scraper` 内置插件；qq-music 同时也实现 `scrape`。
