# 任务 M6-04：玻璃底栏与累积 UI 小项

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。**在主仓库 `/Users/zsakvo/Project/GPUI/haiku-nsh` 工作**。完成后保留改动并停下汇报，不提交 git。

## 先读

- `specs/MACOS.md` §5（原生视图）、§7；`specs/UI.md` PlayerDock 一节；nsh 技能中 `#[native_components(macos)]` 的说明。
- 已有代码：`haiku-macos::glass`（M0-05 spike 验证过 NSVisualEffectView 嵌入，`.artifacts` 证据已不在本机）；PlayerDock 现有的 NSH 半透明表面（作为回退保留）。
- **教训**：effect 内 spawn 的任务会在 effect 重跑时被取消；不新增依赖 Rust `animated_value` 的界面。跑 nsh test 与截图前确认未锁屏。

## 1. `GlassSurface`

- 以 `#[native_components(macos)]` 提供 `GlassSurface`：NSVisualEffectView 作 PlayerDock 背景，无染色 clear glass、细圆角轮廓、上缘高光；其他平台与 Headless 回退到现有 NSH 半透明表面。不改 Dock 布局与尺寸。

## 2. UI 小项（每项改动尽量小）

1. 二维码过期后置灰并显示刷新入口。
2. 开发者结果树：空 `warnings` 不显示展开箭头。
3. 分类歌单标题显示原始 ID → 显示分类名称。
4. 电台未选频道的样式与已选区分清楚。
5. 推荐货架之间的间距统一。
6. 刮削审阅：列表项左对齐（不居中）、“年份”差异行为空时不显示、来源显示名称而非内部 ID。
7. 快捷键冲突提示：去掉英文 “Error:” 前缀，指明与哪一项冲突。
8. 桌面歌词描边：中文字形描边后可见内部重叠笔画轮廓，改为先描边后填充（或等效做法）使字内不出现轮廓线。
9. `download_management::watch_collections` 由 100ms 轮询改为等待已有的变更通道。

## 3. KaraokeLayer 判定（只测量，不实现）

- 真实窗口播放带逐字歌词的测试音频，Now Playing 页打开，用 `top -l` 或 `ps` 记录 30 秒 CPU 占用写入 `.artifacts/m6-04/cpu.log`。只记录数字并在汇报中给出；是否实现 KaraokeLayer 由审阅者决定。

## 验收（每项跑一次）

1. `cargo test --manifest-path native/Cargo.toml --workspace`（含第 9 项的回归测试）。
2. `nsh check`、`nsh build`（仅已知例外）、`nsh test`（全绿；已知偶发 Search Escape、Core 启动 5s 超时在满载时可能出现，单跑通过即可记为偶发）。
3. 截图到 `.artifacts/m6-04/`：真实窗口 PlayerDock 玻璃效果（只截应用窗口）；小项 1–7 各一张 fixture 截图或在 Headless 用例中断言；第 8 项桌面歌词面板截图（只截面板）。
4. CPU 记录（第 3 节）。
5. 收尾：更新 STATUS（追加 M6-04 小节 ≤ 40 行），保留改动等待审阅。

## 不做

参考版 albumProgress 接入、Rust `animated_value` 真实 host 不推进的排查（留给 M6-06 审计）、打包（M6-05）。

## 汇报

最终回复：GlassSurface 实现与回退、各小项结果、CPU 数字、偏离、待人工确认项、需审阅者决定的事项、未完成项。
