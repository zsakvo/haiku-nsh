# 播放、音效与歌词（F06 F07 F08，F13 缓存部分）

业务语义以参考版 SPEC F06–F08 和 §6 为准，下文只列出本版中的归属与关键规则。

## 1. 组件归属

| 模块 | crate | 来源 |
|---|---|---|
| 解码、输出、流式拉取、前缀落盘、预加载、无缝衔接 | haiku-audio | 参考版 `player.rs`（原样迁移，保留测试） |
| EQ 与效果链 | haiku-audio | `effects.rs` |
| 响度测量与增益 | haiku-audio | `loudness.rs` |
| 媒体缓存（LRU、pin、完整性标记） | haiku-audio | `media_cache.rs` |
| 队列、模式、随机历史、编排、恢复、睡眠定时、电台续歌 | haiku-playback | 参考版 TS `core/playback.ts`、`queue.ts`、`radio.ts` 改写为 Rust |
| 歌词解析 | haiku-lyrics | `lyrics.rs` |
| 歌词来源链、当前行选择 | haiku-lyrics | 参考版 TS `services/lyrics*.ts`、`core/current-lyrics.ts` 改写 |
| 进度条、歌词视图、底栏 | native/app（Rust 组件） | 重新实现 |

## 2. 队列（haiku-playback::Queue）

- 所有播放入口都走同一个队列：`play(tracks, index, context?)`、`play_next`、`append`、`remove`、`clear`、`move`、`jump`。
- 模式：顺序、列表循环、单曲循环、随机。
  - 自然播完时，单曲循环重播当前曲；手动上一首/下一首按列表循环处理。
  - 顺序模式下，在队首按上一首会 seek 到 0。
  - 随机模式保留已播放前缀和真实历史，不重复也不遗漏；新一轮的第一首不能是上一轮的最后一首。电台不随机。
- 删除当前项、清空队列、到达末尾时的规则与原版语义一致（参考版测试迁移为 Rust 单元测试）。
- 会话最多 500 条，变更后 5 秒节流写盘，关键操作立即写盘；恢复时只恢复为暂停状态，不自动联网拉流；电台会话不跨启动恢复。
- 专辑和节目的续播位置单独记录（ListeningProgress）。

## 3. 播放编排（haiku-playback::Controller）

```
选曲 → 解析候选：完整下载文件 > 同音质完整媒体缓存 > SourceApi::streams(quality) 的网络候选
     → haiku-audio 打开（generation N） → 事件（Playing/Buffering/Finished/Error, generation）
     → 预加载下一首（与当前模式一致；seek 或队列变化时失效）
```

- 每次打开和每个预加载都从同一个单调递增的 session 分配器取得 generation；过期事件一律丢弃。
- 短命令（暂停、继续、音量）串行执行；长时间的打开操作可以取消；暂停意图先发布再等待底层完成。
- 网络错误：在 1/2/4 秒后重试，最多 3 次；暂停、停止、seek 或新队列会取消重试；连续 30 秒正常播放后清零计数。
- 在已知时长前 5 秒以上提前 EOF：判定缓存失效，走恢复流程。
- 错误自动跳曲有上限，不会绕回队首形成循环。
- 本地曲库中的文件打开失败直接报错，不回退到网络。
- 睡眠定时：15/30/45/60/90 分钟、每次延长 15 分钟、播完当前曲后停止、取消，界面显示剩余时间；到期后真正暂停。
- 统计：Controller 每秒向 `stats` 报告一次真实播放进度（排除暂停、seek 跳跃和异常间隔），见 LIBRARY.md §5。

## 4. 音效与响度（F07）

- EQ：十段（31/62/125/250/500/1k/2k/4k/8k/16k Hz），±12 dB，带预设和自定义；另有前级增益、声场宽度、左右平衡、单声道、软削波、渐入渐出。所有参数都必须真正传递到 DSP。
- 响度均衡：EBU R128，目标 -18 LUFS，峰值限制；按曲目缓存测量结果，只信任完整覆盖、60 秒或半首以上的测量，较差的测量不覆盖较好的结果。
- 设置持久化，切歌后不丢失；越界值 clamp，不允许出现 clipping 或 NaN。

## 5. 媒体缓存（F13）

- 与下载相互独立：开关、容量 512 MiB–20 GiB、占用统计与清理、LRU 淘汰、播放中的条目 pin、失败条目清理、完整性标记。
- 缓存键为 `(EntityRef, 音质)`；复用缓存时必须是同一音质。
- 清理缓存不得删除下载文件或扫描到的原始文件。
- 恢复暂停的会话时，验证完整缓存或本地文件以恢复缓冲比例，不打开音频，也不请求播放地址。

## 6. 歌词（F08）

### 6.1 来源链

依次尝试：手动覆盖 → 歌词缓存 → 本地文件（sidecar 优先于内嵌，单文件不超过 1 MiB，支持附加的译文 sidecar，编码支持 UTF-8/GBK/UTF-16）→ 当前来源插件的 `track.lyrics` → 歌词提供方（Kugou、LRCLIB、Better Lyrics、LyricsPlus、Paxsenix、自定义 HTTP）。

- 网络请求单源 8 秒、总计 25 秒；纯文本歌词作为兜底，但不阻止继续查找同步歌词。
- 由“当前曲目变化或元数据变化”触发，不依赖音频起播成功；恢复到暂停的曲目时也立即加载。
- 切歌或清空队列时先取消旧请求，保证不会把上一首的歌词显示在下一首上。
- 自动获取的歌词在媒体缓存开启时持久化；手动选择的歌词始终保存。
- 手动查找：输入关键词、选择来源、预览、采用。

> 已确认 Q4：Kugou、LRCLIB、Better Lyrics、LyricsPlus、Paxsenix 和自定义 HTTP 来源作为 haiku-lyrics 内置的 Rust 实现；第三方歌词通过插件的 `track.lyrics`/`scrape.lyrics` 接入。

### 6.2 解析与当前行

- 格式：LRC（含增强逐字）、KRC（含内嵌翻译）、YRC、TTML、纯文本；解析结果统一为 `Lines { start, end, text, words?[], translation? }`。
- `haiku-lyrics::Cursor` 根据播放时钟二分查找当前行；当前行变化时才更新 signal。

### 6.3 视图（Rust 组件）

- `LyricsView`：当前行居中跟随；非当前行逐渐淡出；用户滚动浏览后数秒自动回到当前行；点击某行 seek 到该行；只挂载可见窗口内的行。
- `KaraokeLine`：逐字填充宽度直接绑定时钟 signal；暂停、缓冲、窗口不可见时停止推进。
- 进度条拖动：水平移动 1 个逻辑像素后立即激活；按下时记录栏的左边界、宽度和时长；预览位置 = `(pointerX - left) / width * duration`；松开时只提交一次 seek。按钮、封面、非主键按下都不触发拖动。

## 7. 测试

- 迁移参考版 `native/` 中播放、缓存、响度、歌词的全部单元测试。
- 队列与编排：在 haiku-playback 中注入假的音频后端和假的 SourceApi，覆盖模式、随机、恢复、过期 generation、网络重试预算、短曲 Finished 不丢失。
- 真实音频：MP3/FLAC/M4A/OGG 的播放、seek、结束、候选回退；Opus 明确报“不支持”。
- Headless UI：进度拖动规则、歌词点行 seek、不同曲目之间歌词不串。
