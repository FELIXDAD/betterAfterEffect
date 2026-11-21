# 术语表（英文 → 简体中文）

界面中文统一使用 Adobe After Effects 官方简体中文版的译名。下表的中文都已经和 After Effects 2026（26.0）简体中文界面核对过；新增术语时请同样核对，并写明出处。

代码里用英文原文作为翻译键（`bae_ui::tr("Project")`），中文写在对应 crate 的 i18n 表里。改动这里时请同步修改代码里的表。

## 一、BAE 自有概念

以 `docs/ARCHITECTURE.md` 为准，由 02 core/doc-model 定稿。

| 英文 | 中文 | 说明 |
|---|---|---|
| Sublayer | 工程层 | USD 意义上的层，不是 AE 的图层；见架构文档 4.1 节 |
| Prim | 节点 | 工程里用路径定位的对象 |
| Checkpoint | 命名版本 | |
| Proposal layer | 提案层 | Agent 写入自己的提案层，等人审核 |

## 二、面板

| 英文 | 中文 |
|---|---|
| Project | 项目 |
| Composition | 合成 |
| Timeline | 时间轴 |
| Effect Controls | 效果控件 |
| Effects & Presets | 效果和预设 |
| Preview | 预览 |
| Properties | 属性 |
| Align | 对齐 |
| Audio | 音频 |
| Info | 信息 |
| Character | 字符 |
| Paragraph | 段落 |
| Render Queue | 渲染队列 |
| Content-Aware Fill | 内容识别填充 |
| Tools | 工具 |

标签上的组合标题照 AE 的格式：`合成 Main Comp`、`效果控件 Shape Layer 1`、`属性: Shape Layer 1`。

## 三、停靠与面板菜单（☰）

| 英文 | 中文 |
|---|---|
| Close Panel | 关闭面板 |
| Undock Panel | 浮动面板 |
| Close Other Panels in Group | 关闭组中的其他面板 |
| Panel Group Settings | 面板组设置 |
| Close Panel Group | 关闭面板组 |
| Undock Panel Group | 浮动面板组 |
| Maximize Panel Group | 最大化面板组 |
| Restore Panel Group Size | 恢复面板组大小 |
| Stacked Panel Group | 堆叠的面板组 |
| Solo Panels in Stack | 堆栈中的单独面板 |
| Small Tabs | 小选项卡 |

## 四、工作区

| 英文 | 中文 |
|---|---|
| Workspace | 工作区 |
| Default | 默认 |
| Review | 审阅 |
| Learn | 学习 |
| Small Screen | 小屏幕 |
| Standard | 标准 |
| Reset to Saved Layout | 重置为已保存的布局 |
| Save Changes to this Workspace | 保存对此工作区所做的更改 |
| Edit Workspaces... | 编辑工作区... |
| Motion（BAE 工作区） | 动效 |
| Agent（BAE 工作区） | Agent |

## 四之二、菜单与外壳（AE 2026 简体中文词典 MenuID 0002 / 0003）

| 英文 | 中文 |
|---|---|
| Undo | 撤消（AE 用"消"） |
| Redo | 重做 |
| History | 历史记录 |
| Keyboard Shortcuts | 键盘快捷键 |
| New Project | 新建项目 |
| Open Project... | 打开项目... |
| Save | 保存 |
| Save As... | 另存为... |
| Quit | 退出 |
| Untitled Project | 未命名项目 |
| Agent Activity（BAE） | Agent 活动 |
| Proposals（BAE） | 提案 |
| Work Area | 工作区域（AE 26 的“将合成裁剪到工作区”这一项缩写成“工作区”；BAE 统一用“工作区域”，不和 Workspace“工作区”混用） |
| Trim | 修剪（如“将入点修剪到当前时间”“将合成修剪至工作区域”；“裁剪”是 Crop） |
| Bring Layer to Front / Bring Layer Forward | 置于顶层 / 前移一层（AE 26 图层 ▸ 排列，也是通用叫法） |
| Send Layer Backward / Send Layer to Back | 后移一层 / 置于底层 |
| Add to Render Queue | 添加到渲染队列 |
| Render Queue | 渲染队列 |

## 五、工具栏

| 英文 | 中文 |
|---|---|
| Home | 主页 |
| Selection Tool | 选取工具 |
| Hand Tool | 手形工具 |
| Zoom Tool | 缩放工具 |
| Rotation Tool | 旋转工具 |
| Pan Behind (Anchor Point) Tool | 向后平移（锚点）工具 |
| Rectangle Tool | 矩形工具 |
| Ellipse Tool | 椭圆工具 |
| Cube Tool | 立方体工具 |
| Pen Tool | 钢笔工具 |
| Horizontal Type Tool | 横排文字工具 |
| Brush Tool | 画笔工具 |
| Clone Stamp Tool | 仿制图章工具 |
| Eraser Tool | 橡皮擦工具 |
| Roto Brush Tool | Roto 笔刷工具 |
| Puppet Position Pin Tool | 人偶位置控点工具 |
| Local Axis Mode / World Axis Mode / View Axis Mode | 本地轴模式 / 世界轴模式 / 视图轴模式 |
| Snapping | 对齐 |
| Fill / Stroke | 填充 / 描边 |
| Add | 添加 |
| Auto-Open Panel | 自动打开面板 |

注意：工具栏的 Snapping 和 Align 面板在中文版里都叫「对齐」。

## 六、项目面板

| 英文 | 中文 |
|---|---|
| Name | 名称 |
| Type | 类型 |
| Size | 大小 |
| Frame Rate | 帧速率 |
| Comment | 注释 |
| Label | 标签 |
| Folder | 文件夹 |
| Footage | 素材 |
| Solids | 纯色 |

## 七、时间轴

| 英文 | 中文 |
|---|---|
| Layer | 图层 |
| Layer Name | 图层名称 |
| Source Name | 源名称 |
| Parent & Link | 父级和链接 |
| Mode | 模式 |
| Track Matte | 轨道遮罩 |
| Toggle Switches / Modes | 切换开关/模式 |
| Frame Render Time | 帧渲染时间 |
| Shy | 消隐 |
| Collapse Transformations | 折叠变换 |
| Quality and Sampling | 质量和采样 |
| Effect | 效果 |
| Frame Blending | 帧混合 |
| Motion Blur | 运动模糊 |
| Adjustment Layer | 调整图层 |
| 3D Layer | 3D 图层 |
| Graph Editor | 图表编辑器 |
| Shape Layer | 形状图层 |
| Null Object | 空对象 |
| None（父级下拉框） | 无 |

## 八、属性

| 英文 | 中文 |
|---|---|
| Transform | 变换 |
| Layer Transform | 图层变换 |
| Layer Contents | 图层内容 |
| Shape Properties | 形状属性 |
| Shape Transform | 形状变换 |
| Anchor Point | 锚点 |
| Position | 位置 |
| Scale | 缩放 |
| Rotation | 旋转 |
| Opacity | 不透明度 |
| Skew | 倾斜 |
| Skew Axis | 倾斜轴 |
| Roundness | 圆度 |
| Fill Color | 填充颜色 |
| Reset | 重置 |

## 九、合成查看器

| 英文 | 中文 |
|---|---|
| Full | 完整 |
| Half | 二分之一 |
| Third | 三分之一 |
| Quarter | 四分之一 |
| Custom | 自定义 |
| Auto | 自动 |
