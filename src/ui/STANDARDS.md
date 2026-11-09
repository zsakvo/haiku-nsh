# Haiku 设计标准

## 产品与平台
- macOS 14+ 桌面优先，指针与键盘并重。保留红绿灯、窗口拖动、原生菜单与系统快捷键。
- 安静、精确，以音乐内容为中心；不用装饰性卡片、渐变、徽章和虚构数据填充空白。
- 当前没有内容的页面说明交付阶段，不提供无效操作，不伪装加载。

## 密度与几何
- 默认 standard；顶栏、侧栏、菜单使用 compact。一个局部区域采用统一密度。
- 侧栏固定 200pt；顶栏期望 48pt，以 titleBar() 实际高度为下限；几何值统一由 theme 与 Chrome 组件提供。
- 侧栏顶部仅保留与顶栏等高的红绿灯拖动区，不重复显示应用名或“设置”；底部预留 96pt 给后续 PlayerDock。
- 页面与顶栏共享 20pt 水平边距；分组间距 16pt，控件间距 8pt，导航条目间距 2pt。
- 间距刻度：2、4、8、12、16、20、24、32pt；用 Tailwind 刻度或共享 token，不在页面发明偏移值。
- 侧栏条目最小高 28pt，图标 16pt、图文间距 8pt，左右距侧栏边缘 8pt；选中只用 medium 字重。
- 按钮 sm 最小高 28pt、md 32pt；图标按钮与顶栏搜索入口同为 28pt，图标 16/20pt。
- 控件最小高度与内边距共同约束，不固定文字容器高度。触摸平台另行扩至 44pt。
- 标准窗口 1200×800；内容区可收缩，列表/设置内容各自滚动。
- 首版舒适下限 900×700；更窄窗口优先保留内容和导航，允许设置区域滚动。

## 字体与图标
- 系统 UI 字体（中文使用系统 PingFang SC）；快捷键提示使用系统等宽字体。
- 字号：12pt 元信息/侧栏分组、14pt 侧栏正文/控件/正文、16pt 顶栏、18pt 分组、20pt 页面标题。
- 标题少量 semibold，正文 regular；中文不强制大写、不加装饰性字距。
- 仅用 iconoir，一次复制实际使用的 SVG 与许可证。图标通过 Image 的 tintColor 读取 token。
- 图标固定槽位保持列对齐；图标按钮必须有无障碍名称和 tooltip。
- 空态使用带许可证的 unDraw 插画；Image 只能整图 tint，故提供浅/深资源，深色将浅色块映射为主题 card/muted，保留插画主色。

## 主题与颜色
- 唯一颜色定义位于 src/ui/theme；其余 UI 只能消费语义 token。
- 标准角色覆盖背景、前景、卡片、popover、sidebar、muted、primary、secondary、accent、destructive、border、input、ring。
- 应用角色覆盖 success、warning、进度、底栏、歌词和 now-playing；封面数据颜色日后经 token 覆盖进入。
- 浅色为温暖中性色，深色为低亮度中性色；不以深浅色专用 class 代替 token。
- 17 种品牌色沿用参考版，默认 orange；深色品牌色向白色混合 48%。
- primary-foreground 依相对亮度选择对比度更高的黑/白；主色仅用于默认提交与持久选择。
- 外观模式与品牌色先在内存保存；Rust set_tokens 与 NSH setColorScheme 同步所有消费者。

## 圆角与表面
- radius 8pt：普通控件；radius-inset 6pt：快捷键/嵌套小表面；radius-panel 10pt：内容面板/设置选项卡片；radius-surface 14pt：弹出表面。
- 窗口底色为 sidebar，主内容和设置右侧共用 background 圆角面板，四周留 8pt；面板底部不为 PlayerDock 预留。
- 设置的模式卡片等分剩余宽度；品牌色使用等宽自适应网格，固定保留勾槽位，选中用 ring 与勾，不改变几何。
- 命令列表内缩 6pt，与 14pt 外圆角、8pt 选中项圆角形成同心层级。
- 嵌套半径考虑内边距，暴露的圆角不被子背景覆盖；聚焦轮廓必须留有空间。
- 所有 Tooltip/Menu/ContextMenu/Dialog 使用同一 popover、边框、圆角和 shadow-lg。
- 基础区域平面化；分隔线由边界一方绘制。设置覆盖层用实色背景，不应用玻璃。

## 状态与交互
- rest：清晰的标签/形状；hover：accent；pressed：muted 或主色透明度变化。
- focus-visible：2pt ring 轮廓；不依赖颜色 alone，保留选中标记/字重。
- selected/checked：selection 填充、明确前景和标记；侧栏不加单侧色条。
- disabled：降低到 35–40% 强度，由基础组件阻止输入；loading 保留原标签并禁用重复提交。
- 异步失败就近说明原因与恢复操作；即时可逆设置以控件状态反馈，不弹成功提示。
- Button 只保留 primary/secondary/ghost/destructive 与 sm/md；primary 用于默认确认。
- 所有交互包装 nsh/base，不用裸 View 重做按钮、焦点、菜单或 modal 生命周期。
- Esc 关闭最上层；Dialog 负责焦点约束和返回。设置与命令面板互斥，不叠加。
- 后退、前进、设置与命令面板共享命令函数；原生菜单与应用内提示使用同一快捷键定义。
- Cmd+[ / Cmd+] 导航；Cmd+K 命令面板；Cmd+, 设置；Cmd+W 隐藏；Cmd+Q 原生退出。
- 裸字母不绑定全局；输入控件保持编辑快捷键。窗口拖动区域不承担其他隐式命令。

## 状态所有权与文案
- TSX 持有路由、最多 100 条历史、页面内状态、覆盖层与外观选择；业务状态留给 Rust。
- 稳定路由和页面身份不使用翻译文本；EntityRef 视为不透明宿主编码，不拆分来源标点。
- 页面保持挂载，离开后保留输入和滚动；设置不改变历史，收起后继续原任务。
- 目的地用名词，操作用动词；打开需要进一步输入的表面用省略号“…”；短标签不加句号。
- 本任务不加入动画或声音；未来动效必须支持 reduced motion，不让动效承担唯一状态信息。

## 验证
- Headless 证明导航、快捷键、状态和关闭契约；真实窗口截图证明布局、裁切与对比度。
- 新组件必须覆盖默认、hover、pressed、focus、selected、disabled；有异步工作才提供 loading/error。
- 按任务列出的验收执行，不扩展为无边界的设计审计。
