# 01 · 概念与术语

BAE 是一个 AE 类的动画制作软件。动效部分的概念和 After Effects 基本一一对应；不同的是，BAE 的工程由多个"工程层"叠加而成，多人（包括 Agent）同时编辑，各写各的层。

本章的权威来源是 `docs/ARCHITECTURE.md` 第 4 节和 02 `core/doc-model` 的 `docs/spec/data-model.md`（待 02 合并）。如有冲突，以它们为准。

## 1. AE / Ai 概念与 BAE 的对应

| AE / Ai 里叫 | BAE 里叫 | 节点类型（Prim type） | 说明 |
|---|---|---|---|
| 项目（Project） | 工程（Stage） | `Project` | 打开的工程是所有工程层叠加后的结果 |
| 文件夹（Folder） | 文件夹 | `Folder` | |
| 合成（Composition） | 合成 | `Comp` | 尺寸、帧速率、持续时间、背景颜色、工作区域 |
| 图层（Layer） | 图层 | `Layer` | 种类：`Shape` 形状、`Text` 文本、`Solid` 纯色、`Footage` 素材、`Null` 空对象、`Camera` 摄像机、`Light` 灯光、`Adjustment` 调整图层、`PreComp` 预合成、`Audio` 音频 |
| 形状图层内容（Contents） | 形状内容 | `ShapeGroup`、`Path`、`Fill`、`Stroke`、`GradientFill`、各种形状操作器 | 修剪路径、中继器等都是形状操作器 |
| 蒙版（Mask） | 蒙版 | `Mask` | 图层下的子节点 |
| 效果（Effect） | 效果 | `Effect` | 图层下的子节点，参数是属性 |
| 预合成（Pre-compose） | 引用（Reference） | `Layer`（`PreComp`）+ Reference | 预合成图层引用另一个合成 |
| Ai 画板（Artboard） | 画板 | `Artboard` | 静态矢量画面，可以被合成引用 |
| 分镜表 / 分镜格 | 分镜表 / 分镜格 | `Board` / `Panel` | 画面引用、时长、台词、动作、镜头说明，见 [04 分镜规范](04-storyboard.md) |
| 镜头 | 镜头（Shot） | `Shot` | 把分镜格、动效合成、剪辑片段串起来的 ID |
| Premiere 序列 / 轨道 / 片段 | 序列 / 轨道 / 片段 | `Sequence` / `Track` / `Clip` | 剪辑和动态分镜用 |
| 素材（Footage） | 素材 | `Asset` | 文件引用、色彩空间、代理 |
| 属性（位置、不透明度……） | 属性（Attribute） | — | 静态值、关键帧曲线或表达式 |
| — | **工程层（Sublayer）** | — | AE 里没有，见第 2 节 |

**最容易混淆的一点：** "图层"永远指 AE 意义上合成里的图层（节点类型 `Layer`）；USD 意义上的层叫"工程层"（Sublayer）。说"不要改别人的层"时，指的是工程层。

## 2. 工程层、编辑目标与提案层

### 工程层（Sublayer）

工程层是一组"意见"（opinion）的集合。每条意见的意思是"某个节点的某个属性应该是某个值"，或者"这里应该有一个节点"。一个工程由若干工程层按强弱顺序叠加（Layer Stack），**强层的意见覆盖弱层的意见**。

```
强 ┌ session            本机临时状态（预览开关、独奏），不保存
   │ agent/claude-01    Agent 的提案层，等人审核
   │ anim/shot010       动效 A 的层
   │ anim/shot020       动效 B 的层
   │ edit               剪辑的层
   │ storyboard         分镜的层
   │ design/assets      设计资产（角色、Logo，来自 Ai）
弱 └ root               工程设置
```

例子：`design/assets` 里 Logo 的填充颜色是蓝色。如果 `anim/shot010` 里没有对这个颜色发表意见，合成里看到的就是蓝色；如果 `agent/claude-01` 里写了"红色"，在合并结果里看到的就是红色——但这只是一个提案，人接受之前不会进入 `anim/shot010`。

`prim.get`（待确认）会告诉你每个属性的值来自哪个工程层。改一个值之前先看它来自哪里：如果它来自 `design/assets`，说明这是设计资产，通常不该由动效去覆盖（见 [05 协作礼仪](05-collaboration.md)）。

### 编辑目标（Edit Target）

编辑目标是"当前的修改写进哪个工程层"。人一般编辑自己岗位的层。

### 提案层（Proposal layer）

Agent 以协作者身份加入工程时，编辑目标默认就是它自己的提案层 `agent/<名字>`。Agent 的所有修改都只写进这里，作为提案等人审核。用户在 Agent 工作区里可以：

- 逐条接受或拒绝（按事务为单位，所以事务要分得清楚，见 [02 操作流程](02-workflow.md)）；
- 整体合并到某个工程层；
- 整体丢弃。

Agent **不要**主动切换编辑目标。即使你有"可写任意层"的权限，也只有在用户明确要求写进某个具体工程层时才这么做。

### 权限

| 级别 | 能做什么 |
|---|---|
| 只读 | 只能调用查询类和视觉类工具 |
| 只写自己的层 | 默认级别。只能写自己的提案层 |
| 可写任意层 | 由用户授予。仍然要遵守协作礼仪 |

另外有渲染预算和速率限制。所有操作都有审计记录，写明作者。

## 3. 节点路径

节点用路径定位，例如：

```
/comps/main                       合成 main
/comps/main/layers/title          合成 main 里名为 title 的图层
/comps/main/layers/title/masks/reveal
/boards/sb01/panels/sh010         分镜表 sb01 里的分镜格 sh010
/assets/logo_svg
```

路径的确切格式以 02 的 `data-model.md` 为准（待确认）。调用工具时总是传完整路径，不要依赖"当前选中"。

## 4. 属性的三种值

| 形式 | 说明 | 什么时候用 |
|---|---|---|
| 静态值 | 不随时间变化 | 不动的属性 |
| 关键帧 | 若干关键帧 + 插值方式 + 缓动 | 绝大多数动画。AE 用户最容易接手修改 |
| 表达式 | JavaScript，语义与 AE 表达式一致 | 循环（`loopOut`）、随机抖动（`wiggle`）、属性联动、数字滚动。交接给 AE 用户时要确认表达式在 AE 里同样可用 |

**优先用关键帧。** 关键帧在时间轴上看得见，动效设计师可以直接拖动修改；表达式只在确实更合适时使用，并在图层或属性的批注里写明用途。

## 5. 时间

BAE 内部统一用整数时间单位 flicks：**1 秒 = 705,600,000 flicks**。所有常见帧率的每一帧都能被整除，不会有浮点误差。界面上按合成帧速率显示成帧号或时码，与 AE 一致。

**Agent 只用帧思考。** 先把需求换算成当前合成帧速率下的整数帧，只在工具要求时再换算成 flicks：

```
flicks = 帧号 × 每帧 flicks
```

| 帧速率 | 每帧 flicks | 1 秒 = 多少帧 |
|---|---|---|
| 23.976（24000/1001） | 29,429,400 | 23.976 |
| 24 | 29,400,000 | 24 |
| 25 | 28,224,000 | 25 |
| 29.97（30000/1001） | 23,543,520 | 29.97 |
| 30 | 23,520,000 | 30 |
| 50 | 14,112,000 | 50 |
| 59.94（60000/1001） | 11,771,760 | 59.94 |
| 60 | 11,760,000 | 60 |
| 120 | 5,880,000 | 120 |

注意：

- 关键帧必须落在整数帧上。不要用秒乘帧率再得到 12.5 帧这种值。
- 合成的第 0 帧是合成开始。时码 `0:00:01:00` 在 25 fps 下是第 25 帧，在 30 fps 下是第 30 帧。
- 23.976 / 29.97 / 59.94 的时码显示以 AE 的规则为准（是否丢帧显示由合成设置决定）；计算时始终用帧号。

## 6. 命令、事务与撤销

- **命令（Command）**：所有修改都通过命令完成。命令有名字、有参数 Schema，执行后产生一组最小修改（Op），带作者 ID。界面、快捷键、Agent、脚本走的是同一套命令。
- **事务（Transaction）**：把若干命令合成一个撤销单元。对 Agent 来说，一个事务也是用户审核时"接受 / 拒绝"的最小单位。
- **撤销**：按作者区分，只撤销你自己的修改，不会撤销协作者的修改。

## 7. 动效术语速查

下表是本指南用到的 AE 术语。中文以 Adobe After Effects 官方简体中文版为准。

| 中文 | English | 说明 |
|---|---|---|
| 关键帧 | Keyframe | |
| 临时插值 | Temporal Interpolation | 时间上的插值：线性、贝塞尔、定格 |
| 空间插值 | Spatial Interpolation | 位置在画面上的路径形状 |
| 线性 | Linear | 匀速 |
| 定格 | Hold | 值保持不变直到下一个关键帧，用于跳变 |
| 自动贝塞尔曲线 / 连续贝塞尔曲线 | Auto Bezier / Continuous Bezier | |
| 缓动 | Easy Ease | 关键帧辅助，F9，两侧速度 0、影响 33.33% |
| 缓入 | Easy Ease In | Shift+F9，只影响**进入**该关键帧的一侧：运动减速停到这个关键帧 |
| 缓出 | Easy Ease Out | Ctrl/Cmd+Shift+F9，只影响**离开**该关键帧的一侧：运动从这个关键帧加速离开 |
| 速度 / 影响 | Speed / Influence | AE 的缓动模型。影响越大，减速或加速的过程越长 |
| 图表编辑器 | Graph Editor | 可切换值图表（Value Graph）和速度图表（Speed Graph） |
| 漂浮穿梭时间 | Rove Across Time | 让中间关键帧按速度连续自动分配时间 |
| 运动路径 | Motion Path | 位置动画在画面上的轨迹 |
| 单独尺寸 | Separate Dimensions | 把位置拆成 X、Y（、Z）分别做关键帧 |
| 父级和链接 | Parent & Link | 子图层跟随父图层的变换 |
| 空对象 | Null Object | 不渲染，用作父级或控制器 |
| 预合成 | Pre-compose | |
| 蒙版 | Mask | |
| 轨道遮罩 | Track Matte | 用另一个图层的 Alpha 或亮度做遮罩 |
| 形状图层 | Shape Layer | |
| 修剪路径 | Trim Paths | 描边"画出来"的效果 |
| 中继器 | Repeater | |
| 摆动路径 | Wiggle Paths | |
| 文本图层 | Text Layer | |
| 源文本 | Source Text | 文本图层的文字内容属性 |
| 动画制作工具 | Animator | 文本逐字 / 逐词动画 |
| 范围选择器 | Range Selector | 动画制作工具里控制作用范围 |
| 字符间距 | Tracking | |
| 逐字 3D 化 | Per-character 3D | |
| 合成标记 / 图层标记 | Composition Marker / Layer Marker | |
| 工作区域 | Work Area | |
| 运动模糊 | Motion Blur | 需要同时打开合成开关和图层开关 |
| 3D 图层 | 3D Layer | |
| 摄像机 / 灯光 | Camera / Light | |
| 时间重映射 | Time Remap | |
| 序列图层 | Sequence Layers | 关键帧辅助，把图层按顺序错开排列 |
| 关键帧助手 | Keyframe Assistant | |
| 指数比例 | Exponential Scale | 让大倍率缩放在视觉上匀速 |
| 将音频转换为关键帧 | Convert Audio to Keyframes | 用音量驱动动画 |
| 纯色 | Solid | |
| 调整图层 | Adjustment Layer | |
| 背景颜色 | Background Color | 合成设置 |

动画原理类的通用说法（预备动作、跟随与重叠、过冲、错帧等）见 [03 动画规范](03-animation.md)。
