# 数据、账号、同步、备份与日志（F14 F15 F16）

## 1. 数据根

- 默认 `~/Library/Application Support/Haiku NSH`（其他桌面按平台规范）；`HAIKU_DATA_DIR` 覆盖，用于测试；`HAIKU_NO_KEYCHAIN=1` 禁止访问系统钥匙串（隔离测试用）。
- 不与参考版共用目录（D5），首次启动不读取其他应用的数据。

```
<root>/
  settings.json            # 设置（schema 版本号 + 原子合并）
  playlists.loro           # 歌单与喜欢的权威数据（与参考版兼容）
  session.json             # 播放会话
  history.json  stats.json  listening-progress.json
  library/                 # index.json（可重建）  overrides.json  covers/
  lyrics/                  # 歌词缓存、手动选定的歌词
  plugins/
    packages/<id>/         # v2 包：plugin.ts、资源、descriptor.json
    sources/<sourceId>/    # 实例：storage.json、credential.json(0600)
    sources.json           # 实例列表、启停、排序、显示名
  downloads/               # index.json
  cache/media/  cache/images/  cache/palette/
  logs/                    # app.jsonl、plugin.jsonl，按会话轮转
  backups/                 # 恢复前的自动安全备份，最多 3 份
```

- 所有 JSON 写入都是原子的（临时文件 + rename）；文件损坏时，备份原文件为 `.corrupt-<时间>`，并在日志和通知中给出可诊断的错误，然后以默认值继续。
- 每个文件带 `schemaVersion`，迁移函数集中在 `haiku-store::migrate`。

## 2. 设置

- 一个强类型的 `Settings` 结构（Rust，serde），每个字段都有默认值和校验，越界时 clamp。
- 分组：外观 / 播放 / 歌词 / 下载 / 存储 / 来源 / 同步 / 快捷键 / 其他；参考版六组中的每一项都需要有归属，不能遗漏（参考版 §6）。
- 修改由 Rust 串行执行并落盘，然后发布变更事件；主窗口和设置窗口都只是镜像。在同一个进程内不再需要参考版的版本校验与 IPC 重试。
- 读取参考版设置（经备份导入时）：保留桌面端不使用的移动端字段（例如 `glassAutoInvert`、`downloadWifiOnly`），以便再次导出时不丢失。

## 3. Haiku 账号（haiku-services::account）

- 默认服务地址 `https://api.haiku.zhendong.ltd`，接口前缀 `/api/v1`，响应格式 `{code, message, data}`。
- 邮箱验证码登录（自动注册），重发冷却 60 秒，处理验证码错误；恢复会话；登出；读取用户信息。
- 401 或 code 10002 时单飞刷新一次并重放请求；刷新失败则清除会话。刷新响应没有返回新的 refresh token 时保留旧值；网络失败时保留离线会话；登出或切换服务地址后到达的刷新响应一律丢弃。
- 会员：读取 `me/subscription`（5 分钟缓存）和 `plans`，只用于展示；“购买”按钮标记为即将开放。**会员状态不控制任何功能**（D6）。
- 令牌保存在系统钥匙串中（macOS Keychain）。

## 4. 同步（haiku-services::sync，只做 Gist）

- GitHub PAT 保存在钥匙串中；支持创建 Gist、绑定已有 Gist（id 或 URL）、断开、手动同步；修改后 12 秒防抖自动同步；每 2 分钟拉取一次；失败时退避 15 秒到 5 分钟；显示版本与失败状态。
- 内容：Loro snapshot，使用与参考版兼容的 gzip/base64 编码。合并必须幂等且满足交换律；重复导入不产生重复歌曲；删除与重排冲突时不丢失数据。
- 切换账号或令牌时隔离旧状态。
- 自建后端同步暂缓（D8）。

## 5. 备份（haiku-services::backup）

- 格式：`.haikubak`（ZIP），schema 1 与 schema 2 的读取都与参考版兼容；写出时使用 schema 2，并在元信息中注明应用为 Haiku NSH 及其版本。
- 内容：设置、喜欢（含旧格式）、`playlists.loro`、来源列表、插件包与实例（含 storage 和凭据）、可选的 GitHub PAT（默认不包含）。不包含音频、缓存、字体和历史（保持参考版语义）。
- 导出前冲刷所有服务；确认对话框明确提示“包含明文登录凭据”。
- 导入：先 inspect 并展示预览（创建时间、平台、版本、包含的内容），确认后才覆盖。拒绝以下情况：版本过高、文件损坏、绝对路径、反斜杠、冒号、`..`、符号链接、白名单之外的条目。写入前自动做安全备份（最多保留 3 份）。
- **v1 插件**：备份中的 v1 插件包与实例不恢复，在预览和结果中列出“以下旧版插件需要安装新版本”（D9）；恢复对应的 v2 内置插件实例时，尽量迁移连接字段（例如 Subsonic 的地址和用户名）。
- 热恢复顺序：停止播放并清空队列 → 暂停同步与下载 → 写入 → 重新加载插件 → 重新读取设置与顺序 → 重新打开 Loro 并重建视图 → 重建同步 → 刷新来源和刮削器。写入失败时也要在 `finally` 中重新加载，使应用状态与磁盘一致。

## 6. 日志与通知

- 两个频道：app、plugin，各保留 1000 条；记录会话边界；detail 最长 8000 字符；持久化并按会话轮转。
- 支持分页，可按级别、来源、关键词过滤；“复制”复制当前筛选结果，“导出”导出全部频道；支持清空。
- 脱敏：URL 中的用户信息、query、fragment，以及 Cookie、Authorization 等敏感 header。
- 通知中心：后台任务的结果与错误，可以展开和清除。
- 关于页：版本、框架（NSH 版本）、许可证。

## 7. 测试

- 原子写入与损坏恢复、schema 迁移。
- 备份：使用参考版格式构造的 schema 1/2 fixture 恢复；导出再导入的往返；恶意路径拒绝；v1 插件跳过并列出。
- 账号：契约测试覆盖并发刷新、离线保留、登出后迟到的刷新、冷却。
- Gist：使用 HTTP fixture 的契约测试；两个 Loro 实例的合并。
