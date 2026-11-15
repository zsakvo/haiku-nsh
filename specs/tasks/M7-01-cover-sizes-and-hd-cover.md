# 任务 M7-01：封面尺寸声明与高清封面接口（含 QQ 底栏封面为空）

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**，在当前未提交的工作区上继续，不要回退、整理或提交已有改动。完成后停下汇报。

## 背景（已定位，不需要再排查）

- QQ 插件给出的封面是模板 `https://y.gtimg.cn/music/photo_new/T002R{size}x{size}M000<mid>.jpg`。
- 宿主在 `native/crates/haiku-plugins/src/api.rs` 映射实体时把 `{size}` 固定替换成 `cover_size = 512`（`with_cover_size` 没有调用方）。
- y.gtimg.cn 只支持 150/300/500/800，**512 和 1000 都返回 404**（审阅者已用 curl 实测）。`sources::artwork` 下载失败 → `presentation` 的 `artwork` 一直为空 → 底栏和正在播放都没有封面。
- 用户确定的方案：**尺寸声明 + 取封面处理函数**。

## 1. SDK 契约（`sdk/haiku.d.ts`、`specs/PLUGIN_SDK.md`、`sdk/GUIDE.md`）

1. `definePlugin` 新增可选元数据 `covers?: { sizes: number[] }`：`{size}` 模板只允许这些像素边长，按升序排列。不声明时表示任意尺寸都可以。
2. `track` 新增可选处理函数 `cover?(id: string, ctx: Context & { size: number }): Promise<Image | null>`：返回该曲目不小于 `size` 的最佳封面（可以带 `{size}` 模板）。只给那些列表中只有固定缩略图、模板不够用的来源使用。宿主不在列表中调用它。
3. 选档规则写进 PLUGIN_SDK.md（一处，宿主按这里实现）：取声明中不小于目标尺寸的最小值；没有就取最大值。
4. 删除 §1 表中“`getCoverArt` 需要一次额外调用”那行的过时说法，改为说明上面两条。能力推断（descriptor 的 `capabilities`）要包含 `track.cover`。

## 2. 宿主

1. **模板保留到使用处**：实体映射时不再替换 `{size}`。`Track.cover` 等字段保存原始模板，持久化（曲库、队列、收藏）照常保存字符串。在 `haiku-plugins` 中提供一个解析函数（输入模板、目标尺寸、来源声明的 sizes，输出 URL），所有消费方都调用它，不在别处手写 `replace("{size}")`（`download_service.rs` 那处也改用它）。
2. **按用途选档**：列表/网格缩略图的目标尺寸为显示尺寸 × 2（取各列表现有的显示尺寸，不改布局）；底栏、正在播放、媒体中心、当前曲与下一首的预热使用 UI.md §4 的 1600 档；下载按现有 hd 设置。TSX 页面拿到的封面 URL 在 Rust 投影时就解析好，不把模板或尺寸表传给 TSX。
3. **高清封面**：`presentation` 加载当前曲、预热下一首时，如果来源实现了 `track.cover`，先调用它（带 1600），失败或返回 null 时回退到 `Track.cover` 模板。结果与现有封面缓存共用一个键空间：键里包含最终 URL，不另建缓存。调用失败只记日志，不弹错误。
4. 旧数据：已持久化的 QQ 曲目可能保存了 `R512x512`。**不做迁移**；在汇报中说明影响范围（哪些列表仍显示不出缩略图）。正在播放会通过 `track.cover` 恢复。

## 3. QQ 插件（`dev-plugins/qq-music/`）

- 声明 `covers: { sizes: [150, 300, 500, 800] }`。
- 实现 `track.cover`：由曲目取专辑 mid，返回 `T002R800x800M000<mid>.jpg`；没有专辑时回退到歌手图 `T001`。复用 `track.get` 已有的请求，不新增上游接口。
- fixture 契约补充 `track.cover`。
- 版本号改为 `3.1.0`。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml -p haiku-plugins`：新增单元测试覆盖选档规则（命中、取上限、未声明）、模板在映射后保留、`track.cover` 进入 capabilities。
2. `native/tools/haiku-plugin` 的 `check` 与 `test` 对 `dev-plugins/qq-music` 通过；`tsc --noEmit -p sdk/tsconfig.json` 通过。
3. `cargo test --manifest-path native/Cargo.toml --workspace` 通过（已知偶发失败按 STATUS 历史记录说明即可）。
4. `nsh check`、`nsh test` 通过。新增一个 Rust Headless 断言：来源声明 sizes 且实现 `track.cover` 时，底栏 `artwork` 使用 `track.cover` 返回的地址（用本地 fixture 插件和本地图片，不访问网络）。
5. 补充证据（允许失败）：`haiku-plugin test --live` 只跑 `search.track` + `track.cover` 一次，日志中能看到 800 档 URL 返回 200。
6. 收尾：STATUS 中 F03、F08 行更新，追加 M7-01 小节（≤ 40 行）；日志放 `.artifacts/m7-01/`。

## 不做

不改正在播放的布局、动画、歌词与控制栏（后续任务）；不改其他插件的行为（`plugins/subsonic` 等可以不声明 sizes）；不做旧数据迁移；不做真实窗口截图。

## 汇报

改动的文件；解析函数的位置与所有调用点；各列表采用的目标尺寸；旧数据影响范围；验证结果；需要审阅者决定的事项。
