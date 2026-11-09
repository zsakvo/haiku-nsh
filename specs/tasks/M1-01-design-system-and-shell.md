# 任务 M1-01：设计系统 + 应用外壳

执行者：Codex。审阅者：Claude。遵守 `specs/tasks/README.md`。完成后提交一次并停下汇报。

## 先读

- `.agents/skills/nsh-design-guides`（**全文**，这是本任务的主要依据）。
- `.agents/skills/nsh/references/coding-guides.md` 中的 “Build the app's UI foundations before pages”、“Theme and styling”、“Events, commands, and focus”、“Stable identity”、“Rules for coding agents”。
- `specs/UI.md` §1–§2、§6；`specs/SPEC.md` D7、D18。
- 所有 NSH API 以 `.nsh/types` 和 nsh-core/nsh-base 0.4.0 源码为准；`nsh/desktop` 只有当前窗口的 API（D18）。

## 0. 清理 M0 spike

- 删除 `src/spike/`、`native/app/src/spike/` 及其测试和 fixture；删除 App.tsx 中的 spike 界面。
- **保留并正式接入**“关闭时隐藏、Dock 恢复”：从 spike 移到 `native/app/src/platform/` 中（或 haiku-macos 提供的安装函数），应用启动时安装。媒体中心与菜单栏的接线本任务不保留（M1-05 重新接入，haiku-macos 中的能力代码保留）。
- `haiku-jsrt`、`haiku-plugins` 中的 echo 示例可以留在 crate 的单元测试里，应用不再导出 `Spike`。

## 1. 设计标准 `src/ui/STANDARDS.md`

写出本应用的设计标准（目标平台 macOS 桌面、默认密度、字号与间距刻度、圆角层级、图标、各种交互状态的表现、浅色与深色）。内容遵循设计规范，篇幅控制在 120 行以内，后续任务都要遵守它。

## 2. 主题 token

- 按 NSH 的主题机制定义（具体写法查 coding guides 的 Theme 一节与 tailwind 参考），浅色与深色两套，用语义角色：标准角色（background、foreground、card、popover、sidebar、muted、primary、secondary、accent、destructive、border、input、ring 及其 foreground），加上 UI.md §1.2 列出的应用角色（本任务只需要定义，`now-playing-*` 等先给出合理的默认值）。
- **品牌色**：沿用参考版的 17 种品牌色（`~/Project/haiku-gpuix/src/ui/appearance.ts` 的 `brandColors`），默认 `orange`（#ba5a24）；深色模式下品牌色向白色混合 48%（同参考版 `themedPalette`）。`primary` 取品牌色，`primary-foreground` 按对比度选黑或白。品牌色切换在运行时生效，本任务先做成内存中的状态（持久化在 M1-05）。
- 外观模式：跟随系统 / 浅色 / 深色，同样先保存在内存中。
- 除主题定义文件外，代码中不能出现任何原始颜色值。

## 3. 第一批组件 `src/ui/`

只做外壳需要的组件，每个都具备完整的交互状态，并包装 `nsh/base`：Button（primary / secondary / ghost / destructive；sm / md）、IconButton（必须有无障碍名称和 tooltip）、Tooltip、SidebarItem（图标 + 文字，选中、hover、focus 状态）、SectionLabel、SearchField、Kbd（快捷键提示）、EmptyState（插画 + 标题 + 说明 + 可选操作）、Dialog / Overlay 的表面、Menu / ContextMenu 的表面（所有弹出层共用一种 popover 处理方式）。

图标：使用参考版 `~/Project/haiku-gpuix/assets/icons/iconoir/` 中的 SVG，**只复制用到的那些**，连同其 LICENSE，放在 `assets/icons/`。插画用参考版 `assets/illustrations/`（同样带 LICENSE）。图标的显示方式（asset、tint）查 NSH 的文档和类型。

## 4. 外壳

- **标题栏**：`setTitleBar({ style: "hidden" })`，红绿灯按钮保留；侧栏延伸到窗口顶部，顶部留出红绿灯区域；内容区顶栏放后退/前进 IconButton（Cmd+[ / Cmd+]）、页面标题、右侧的搜索入口（点击打开命令面板）。拖动区域按设计规范保留。
- **侧栏**（宽 220pt，固定）：分组和条目按 UI.md §2（推荐、分类、电台、搜索 / 本地曲库、喜欢、历史、统计 / 歌单（本任务为空分组）/ 下载），底部预留 96pt（将来被底栏覆盖）。
- **导航**：类型化的路由（每个页面一个变体，详情类路由预留 `EntityRef` 参数位）；历史最多 100 条，新导航截断前进历史；每个页面的状态（例如滚动位置、页面内的输入）在离开再回来后保留。导航状态归 TSX（屏幕状态）。
- **页面**：每个侧栏条目对应一个页面，暂时只显示 EmptyState，文案说明该功能将在哪个阶段提供（例如“本地曲库将在下一版本提供”），不显示任何假数据。
- **设置覆盖层**：Cmd+, 和顶栏/命令面板都能打开；全窗口覆盖，左侧是分类列表（UI.md §6 的分类），右侧是分组内容区（本任务只实现“外观”分组：外观模式、品牌色选择，均实时生效）；Esc 和关闭按钮收起；收起后原页面状态不变，不进入导航历史。
- **命令面板**：Cmd+K，使用 `nsh/base` 的 Command 组件；命令包括：前往各页面、打开设置、切换外观模式。搜索功能留到 M4。
- **应用菜单**：`set_application_menu`，包含标准角色（关于、隐藏、退出、编辑菜单等）、视图（后退、前进、命令面板）、窗口（Close Window = Cmd+W，走关闭隐藏）；菜单中的快捷键与应用内快捷键保持一致，裸字母键不绑定全局快捷键。

## 验收（每项跑一次）

1. `nsh check`、`nsh build`（无 warn）、`nsh test` 通过。
2. Headless 测试（写在项目中）：
   - 点击侧栏每个条目都能到达对应页面；
   - 后退/前进正确，新导航截断前进历史；
   - 页面状态在离开再回来后保留（选一个页面放一个输入框作为测试）；
   - Cmd+K 打开命令面板并能跳转页面；
   - Cmd+, 打开设置覆盖层，切换深色模式生效，Esc 关闭后原页面和导航历史不变。
3. `nsh run` 截图 4 张：主窗口浅色、主窗口深色、设置覆盖层浅色、命令面板深色。截图要实际查看，确认没有裁切、错位、对比度问题。
4. `cargo test --workspace` 仍然通过（确认清理 spike 没有影响领域 crate）。
5. 提交：`M1-01: design system foundations and app shell`。本次提交一并纳入 `specs/` 中尚未提交的修改。

## 汇报

STATUS.md：F01 一行改为“部分实现”并写明本任务覆盖的内容，追加“M1-01”小节（不超过 40 行）。最终回复：做出的设计选择（STANDARDS.md 的要点、侧栏和顶栏的尺寸）、NSH 中缺少或不顺手的地方、需要审阅者决定的事项。
