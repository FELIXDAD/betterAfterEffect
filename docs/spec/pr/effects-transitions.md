# Pr 清单：效果与过渡（PR-FX）

## 范围

效果面板、效果控件面板、固有效果（运动、不透明度、时间重映射）、蒙版、效果顺序、主剪辑效果、效果预设、调整图层、过渡的应用与设置；文末附录是 Pr 26.2 的内置视频效果、视频过渡、音频效果、音频过渡清单。关键帧见 [keyframes.md](keyframes.md)，音频效果的使用见 [audio.md](audio.md)，Lumetri 见 [color.md](color.md)。

**BAE 的基本做法：** 剪辑里的效果与动效工作区的图层效果是同一种 `Effect` Prim、同一个 `EffectRegistry`、同一套 GPU 实现（07 `bae-effects`、19 插件）。Pr 里很多效果本来就是从 AE 移植的（matchName 是 `AE.` 加 AE 的 matchName），所以 07 按 AE 清单实现的效果在剪辑里直接可用，不需要第二套。剪辑独有的工作是：过渡（两个输入的效果）、固有效果的 Pr 语义、Pr 独有效果的对照。

## 效果

### PR-FX-001 · 效果面板（Effects Panel）

- **行为要点：** 顶层文件夹：预设、Lumetri 预设、音频效果、音频过渡、视频效果、视频过渡，下面按分类分组（分类见附录）。顶部搜索框按名称过滤所有文件夹；三个筛选按钮只显示"加速效果"（GPU）、"32 位颜色"、"YUV"效果。可以新建自定义素材箱收藏常用效果。Shift+7 聚焦。
- **优先级：** M4
- **BAE 模块：** bae-ui-edit (+ bae-effects, bae-plugin)
- **数据模型：** 无。列表来自 `EffectRegistry`（含插件效果），分类用 `EffectDef` 的分类字段。
- **备注：** 动效工作区的"效果和预设"面板是同一份数据，建议做成一个面板，两种工作区各自过滤（例如剪辑里显示过渡、动效里隐藏）。BAE 的效果全部在 GPU 上，"加速效果"筛选没有意义，可以换成"支持 HDR / 32 位"之类。

### PR-FX-002 · 应用效果（Apply Effect）

- **行为要点：** 把效果拖到时间轴上的剪辑（或节目监视器画面里的剪辑）；先选中多个剪辑再双击效果，一次应用到全部；拖到效果控件面板里放在指定位置；拖到调整图层上作用于下面所有轨道。音频效果只能拖到音频剪辑上，视频效果只能拖到视频剪辑上。
- **优先级：** M4（M1 预留 `effects/` 子文件夹）
- **BAE 模块：** bae-edit (+ bae-ui-edit, bae-model)
- **数据模型：** Clip 下的 `effects/<fx>` Effect Prim（edit-model.md 第 5.4 节）。
- **备注：** 命令 `clip.add_effect`（目标剪辑列表、match_name、插入位置），一次应用到多个剪辑是一个撤销步骤。

### PR-FX-003 · 效果控件面板（Effect Controls）

- **行为要点：** 显示选中剪辑的所有效果：上面是固有效果（视频：运动、不透明度、时间重映射；音频：音量、声道音量、声像器），下面是加上去的标准效果。每个效果有 fx 开关（临时旁路）、重置按钮、展开箭头；每个属性有秒表（切换动画）、数值、关键帧导航。面板右侧是小时间轴，显示剪辑范围内的关键帧，可以拖动。面板顶部的"源 / 序列"标签切换主剪辑效果和剪辑实例效果。Shift+5 聚焦；Cmd+L 循环播放音频（面板作用域）。
- **优先级：** M4
- **BAE 模块：** bae-ui-edit (+ bae-ui)
- **数据模型：** 无
- **备注：** 属性行、秒表、关键帧导航、热文本是 11 已有的通用控件，动效工作区的效果控件面板（12）也用它们。剪辑和动效可以共用一个效果控件面板实现，只是剪辑里多显示固有效果和过渡设置。

### PR-FX-004 · 运动（Motion）

- **行为要点：** 位置（默认序列中心）、缩放（%，取消"等比缩放"后分成缩放高度和缩放宽度）、旋转（度，超过 360° 显示圈数）、锚点（默认剪辑中心）、防闪烁滤镜（0–1）。在效果控件里点"运动"名称后，节目监视器画面上出现控制框，可以直接拖动位置、拖动角点缩放、在框外拖动旋转。Pr 26 的"属性"面板也提供这些控件（Pr 25 起的新面板）。
- **优先级：** M4（M1 预留属性名）
- **BAE 模块：** bae-edit (+ bae-ui-edit, bae-anim, bae-render)
- **数据模型：** Clip 的 `transform:position`、`transform:scale`、`transform:uniform_scale`、`transform:rotation`、`transform:anchor_point`、`transform:anti_flicker`（edit-model.md 第 5.2 节）。
- **备注：** 运动是二维的（vec2）；AE 图层的 `transform:*` 是 vec3。渲染时视频剪辑可以当作 2D 素材图层，复用 04 的图层变换。

### PR-FX-005 · 不透明度与混合模式（Opacity / Blend Mode）

- **行为要点：** 不透明度（%，默认 100）、混合模式（正常、溶解、变暗、相乘、颜色加深、线性加深、深色、变亮、滤色、颜色减淡、线性减淡（添加）、浅色、叠加、柔光、强光、亮光、线性光、点光、强混合、差值、排除、相减、相除、色相、饱和度、颜色、发光度，共 27 种，待核）。在时间轴上剪辑的不透明度橡皮筋可以直接上下拖动、Cmd 点击加关键帧。
- **优先级：** M4（M1 预留属性名）
- **BAE 模块：** bae-edit (+ bae-render)
- **数据模型：** Clip 的 `transform:opacity`、`blend_mode`（token 与 Layer 的混合模式一致，edit-model.md 第 5.2 节）。
- **备注：** Pr 的混合模式是 AE 混合模式的子集，04 已实现 AE 全部 38 种 token，直接复用。

### PR-FX-006 · 蒙版与蒙版跟踪（Masks / Mask Tracking）

- **行为要点：** 不透明度和大多数标准效果下都可以加蒙版：创建椭圆形蒙版、创建 4 点多边形蒙版、自由绘制贝塞尔曲线（钢笔）。属性：蒙版路径、蒙版羽化（默认 10）、蒙版不透明度、蒙版扩展、已反转。蒙版跟踪：向前/向后跟踪一帧或连续跟踪，方法有"位置""位置及旋转""位置、缩放及旋转""透视"（待核），结果是蒙版路径的逐帧关键帧。多个蒙版取并集。
- **优先级：** M4（蒙版）；以后（跟踪）
- **BAE 模块：** bae-edit (+ bae-vector, bae-render, bae-ui-edit)
- **数据模型：** Clip 下的 `masks/<m>`，复用 Mask schema；效果上的蒙版放在 Effect 下的 `masks/`（edit-model.md 第 5.2 节）。
- **备注：** 效果级蒙版（只在蒙版区域内应用这个效果）是 Pr 常用的局部调色手法，AE 没有完全对应的概念，04 需要支持"效果按蒙版混合回原图"（见 04 的请求）。

### PR-FX-007 · 效果顺序（Effect Order）

- **行为要点：** 效果控件里拖动标准效果改变顺序；Pr 按从上到下的顺序渲染标准效果，**固有效果（运动、不透明度、时间重映射）总是在标准效果之后渲染**，所以需要"先变换再加效果"时要用标准效果里的"变换"代替运动。音频效果同样按顺序处理，音量在最后。
- **优先级：** M4
- **BAE 模块：** bae-render (+ bae-edit)
- **数据模型：** `effects/` 的子节点顺序就是渲染顺序（edit-model.md 第 5.4 节）。
- **备注：** 这条顺序决定了导入 Pr 工程后画面能否一致，04 构建剪辑的渲染图时必须照做。

### PR-FX-008 · 主剪辑效果（Master Clip Effects）

- **行为要点：** 在源监视器打开来源后，效果控件里切到"源"标签（或把效果拖到源监视器画面上），效果加在主剪辑上，项目里所有使用它的剪辑都受影响；时间轴上的 fx 徽章用红色下划线（待核）标出有主剪辑效果。常用于给整段素材做基础调色（LUT）。
- **优先级：** 以后
- **BAE 模块：** bae-edit (+ bae-render)
- **数据模型：** 来源 Asset 下的 `effects/`，渲染时排在剪辑效果之前（edit-model.md 第 5.4 节）。
- **备注：** 和"解释素材 > 输入 LUT"（色彩管理）功能部分重叠，06 决定 LUT 放哪一层。

### PR-FX-009 · 效果开关与全局效果静音（fx Toggle / Global FX Mute）

- **行为要点：** 效果控件里每个效果的 fx 按钮临时关闭这个效果（渲染时跳过，参数保留）；时间轴剪辑上的 fx 徽章显示效果状态；节目监视器按钮"全局 FX 静音"一键关闭所有剪辑的效果，用于检查原始画面或提高播放速度（颜色模式下有快捷键，编辑模式需要在按钮编辑器里加）。
- **优先级：** M4
- **BAE 模块：** bae-edit (+ bae-render, bae-ui-edit)
- **数据模型：** Effect 的 `enabled`（schema 已有）。全局静音是视图状态，不进工程（渲染请求带一个"忽略效果"标志）。
- **备注：** 无

### PR-FX-010 · 复制、粘贴与删除效果（Copy / Paste / Delete Effects）

- **行为要点：** 在效果控件里选中一个或多个效果 Cmd+C，再选中别的剪辑 Cmd+V 粘贴（追加在已有效果之后）；Delete 删除选中的效果；"粘贴属性"可以选择性粘贴（PR-TL-033）；右键剪辑"删除属性"批量删除效果。
- **优先级：** M4
- **BAE 模块：** bae-edit (+ bae-ui-edit)
- **数据模型：** 无
- **备注：** 效果的 `state`（插件状态）随复制原样带走（data-model.md 第 11 节的约定）。

### PR-FX-011 · 效果预设（Effect Presets）

- **行为要点：** 在效果控件里选中效果，右键"保存预设"：命名、类型选"缩放"（关键帧按目标剪辑长度缩放）、"定位到入点"或"定位到出点"（关键帧保持与入点或出点的距离），可写描述。预设出现在效果面板的"预设"文件夹，拖到剪辑上应用。预设可以导入导出（.prfpset）。Pr 自带一些预设（斜角边、模糊入/出、画中画、扭曲入/出……）。
- **优先级：** 以后
- **BAE 模块：** bae-edit (+ bae-ui-edit)
- **数据模型：** 预设是一组 Effect 的 `PrimTree` 加关键帧时间处理方式，存在用户目录或工程里的预设文件夹（与 AE 的动画预设统一设计）。
- **备注：** 不复制 Adobe 自带的预设文件。导入用户自己的 .prfpset 属于交换格式，以后再看。

### PR-FX-012 · 调整图层（Adjustment Layer）

- **行为要点：** 项目面板新建"调整图层"（按序列尺寸），放到上层轨道；它上面的效果作用于它下面所有轨道在该时间范围内的合成结果。调整图层本身的不透明度、运动、蒙版控制效果的强度和范围。常用于整段调色、统一加效果。
- **优先级：** M4（M1 预留 Asset 种类）
- **BAE 模块：** bae-render (+ bae-edit)
- **数据模型：** Asset `kind = adjustment`，剪辑 `source` 指向它（edit-model.md 第 5.5 节）。
- **备注：** 与 AE 调整图层同语义，04 的渲染图已经规划了调整图层节点。

### PR-FX-013 · Pr 独有效果与 AE 效果的对照（Effect Mapping）

- **行为要点：** Pr 26.2 的视频效果里，大部分是 AE 效果的移植（matchName `AE.ADBE …`），少数是 Pr 独有的：Lumetri 颜色、极致键、变形稳定器（AE 也有）、自动重新构图、SDR 遵从情况、简单文本、视频限幅器、ASC CDL、色彩空间变换，以及 Pr 26 内置的 Film Impact 系列（matchName `AE.AE_Impact_*`，附录中标 †）。
- **优先级：** M4
- **BAE 模块：** bae-effects (+ bae-interop-edit)
- **数据模型：** 无新增。对照表：`AE.` 前缀去掉后就是 AE matchName，直接查 07 的实现；Pr 独有效果另行实现或保留为"缺失效果"占位（参数原样保留，data-model.md 第 11 节）。
- **备注：** 07 排期时可以按附录的使用频率挑选：高斯模糊、裁剪、变换、投影、亮度和对比度、Lumetri 颜色、极致键、Alpha 调整、轨道遮罩键、黑白、色调、边角定位、镜像、偏移、基本 3D 覆盖了剪辑里绝大多数场景。

### PR-FX-014 · 变形稳定器与果冻效应修复（Warp Stabilizer / Rolling Shutter Repair）

- **行为要点：** 应用变形稳定器后在后台分析（画面上显示"在后台分析"横幅），分析完再稳定；参数：结果（平滑运动 / 不运动）、平滑度、方法（位置；位置、缩放、旋转；透视；子空间变形）、取景（仅稳定、稳定并裁切、稳定裁切自动缩放、稳定合成边缘）。不能用在速度改过的剪辑上（要先嵌套）。
- **优先级：** 以后
- **BAE 模块：** bae-effects (+ bae-media)
- **数据模型：** 分析结果存进 Effect 的 `state`（bytes），协同时整体替换（data-model.md 第 11 节约定）。
- **备注：** 适合做成插件；分析在 GPU 上做光流。

### PR-FX-015 · 抠像（Ultra Key / Track Matte Key）

- **行为要点：** 极致键：用吸管选主要颜色，设置（默认、宽松、强效、自定义），遮罩生成（透明度、高光、阴影、容差、基值）、遮罩清除（抑制、柔化、对比度、中间点）、溢出抑制、颜色校正；输出可切换"合成 / Alpha 通道 / 颜色通道"查看。轨道遮罩键：选择遮罩所在轨道，合成方式 Alpha 遮罩或亮度遮罩，可反向。
- **优先级：** 以后（轨道遮罩键 M4）
- **BAE 模块：** bae-effects (+ bae-render)
- **数据模型：** 轨道遮罩键的"遮罩轨道"参数是对 Track 的引用（Effect 的 `layer` 类参数在剪辑里指向轨道，见 02 的请求）。
- **备注：** 轨道遮罩键与 AE 轨道遮罩是同一类运算，04 已实现。

## 过渡

### PR-FX-016 · 应用过渡（Apply Transition）

- **行为要点：** 把过渡从效果面板拖到两个剪辑之间的编辑点上（放下时可以选择落在出点侧、居中、入点侧）；或者选中编辑点按 Cmd+D 应用默认视频过渡、Shift+Cmd+D 应用默认音频过渡、Shift+D 应用默认音视频过渡；选中多个剪辑后按 Cmd+D 给它们之间的所有编辑点加过渡（剪辑两端接空白时加单侧过渡）。剪辑和空白之间、序列开头/结尾也可以加单侧过渡（常用于黑场淡入淡出）。
- **优先级：** M4（M1 预留数据结构）
- **BAE 模块：** bae-edit (+ bae-ui-edit)
- **数据模型：** 片段的 `transition_in` / `transition_out` 子 Prim，两个剪辑之间的过渡归后一个剪辑的 `transition_in`（edit-model.md 第 6 节）。#66 的 `clip.set_transition` 是它的最小版本。
- **备注：** 过渡应用到"所选编辑点"需要命令接收编辑点列表，一次执行。

### PR-FX-017 · 默认过渡与默认持续时间（Default Transition / Default Duration）

- **行为要点：** 默认视频过渡是交叉溶解，默认音频过渡是恒定功率；在效果面板里右键某个过渡"将所选过渡设置为默认过渡"改掉（图标上有蓝框）。偏好 > 时间轴：视频过渡默认持续时间、音频过渡默认持续时间（Pr 默认均为 1 秒，待核单位是秒还是帧）。
- **优先级：** M4（M1：#66 已用 1 秒作为默认长度）
- **BAE 模块：** bae-edit (+ bae-ui-edit)
- **数据模型：** 默认过渡和时长是用户偏好，不进工程；命令参数可以覆盖。
- **备注：** 无

### PR-FX-018 · 过渡对齐与调整（Alignment / Trimming Transitions）

- **行为要点：** 对齐方式：中心切入（默认）、起点切入（过渡从切点开始，全部在后一剪辑上）、终点切入（全部在前一剪辑上）、自定义起点（拖动过渡在切点两侧不对称）。在时间轴上拖动过渡的边缘改变时长；拖动过渡中间平移它；用滚动编辑工具拖动过渡中线是滚动修剪下面的编辑点；在效果控件的过渡时间轴里可以分别拖动 A、B 剪辑。
- **优先级：** M4（M1 预留 `alignment`、`offset`）
- **BAE 模块：** bae-edit (+ bae-ui-edit)
- **数据模型：** Transition 的 `duration`、`alignment`、`offset`（edit-model.md 第 6 节）。
- **备注：** 过渡缩放到很短（1 帧）也要可选中，时间轴绘制给过渡留最小点击宽度。

### PR-FX-019 · 单侧过渡与媒体不足（One-Sided Transition / Insufficient Media）

- **行为要点：** 编辑点一侧没有剪辑（空白或序列边界）时过渡只作用于有剪辑的一侧（从黑/透明渐入或渐出）。两侧都有剪辑但源媒体在切点之外没有余量（剪辑用到了素材的开头或结尾）时，Pr 提示"媒体不足。此过渡将包含重复的帧"，过渡区域显示斜纹，用定格帧补足。
- **优先级：** M4
- **BAE 模块：** bae-edit (+ bae-render, bae-ui-edit)
- **数据模型：** 无新增；序列求值结果里给出重复帧区间（edit-model.md 第 6、10 节）。
- **备注：** Pr 的单侧淡入淡出在合成上是"与透明混合"，对最下层视频轨来说就是黑场；音频单侧过渡是从静音渐入。

### PR-FX-020 · 过渡设置（Transition Settings in Effect Controls）

- **行为要点：** 选中过渡后效果控件显示：A/B 小预览（可勾"显示实际源"看真实画面）、持续时间、对齐、开始/结束百分比（过渡从中途开始或在中途结束）、边框宽度和颜色、反向、消除锯齿品质，以及每种过渡自己的参数（方向、柔和度、颜色……）；右侧小时间轴显示 A、B 两个剪辑和过渡区域。Film Impact 过渡还有预设下拉和更多参数（Pr 26 内置，待核细节）。
- **优先级：** M4
- **BAE 模块：** bae-ui-edit (+ bae-effects)
- **数据模型：** 通用参数 `start_percent`、`end_percent`、`reverse`、`border:width`、`border:color` 放在 Transition 上，种类自己的参数是 `param:NNNN`（edit-model.md 第 6 节）。
- **备注：** 无

### PR-FX-021 · 过渡渲染（Transition Rendering）

- **行为要点：** 过渡区间内，渲染器同时取 A 剪辑（含它自己的效果和运动）和 B 剪辑的画面，按进度交给过渡效果合成。A、B 在各自的轨道层级上合成（过渡只发生在同一条轨道的两个剪辑之间）；上面轨道的内容照常盖在过渡结果上面。
- **优先级：** M4（交叉溶解、黑场过渡）；其余随效果库扩充
- **BAE 模块：** bae-render (+ bae-effects, bae-edit)
- **数据模型：** 无新增；求值结果的 `TransitionState`（种类、进度 0–1、另一侧剪辑，edit-model.md 第 10 节）。
- **备注：** 需要 04 在渲染图里支持"两个输入的过渡节点"（见 04 的请求）。过渡也可以是插件（19 的声明式 WGSL 插件加一种"过渡"类型）。

### PR-FX-022 · 变形剪切（Morph Cut）

- **行为要点：** 用于访谈跳剪：把变形剪切拖到同一人物的两个镜头之间，后台分析人脸和画面，生成平滑过渡掩盖跳剪；分析中显示横幅；通常时长很短（几帧到 1 秒）。
- **优先级：** 以后
- **BAE 模块：** bae-effects (+ bae-media)
- **数据模型：** 分析结果存在 Transition 的 `state`。
- **备注：** 适合插件或 Agent 工具。

### PR-FX-023 · 效果和过渡插件（Plug-in Effects / Transitions）

- **行为要点：** Pr 支持 AE SDK 写的效果插件（很多第三方插件同时支持 AE 和 Pr）、Pr SDK 的过渡插件和 Pr 26 内置的 Film Impact 插件。插件效果出现在效果面板里，与内置效果用法相同。
- **优先级：** M3
- **BAE 模块：** bae-plugin (+ bae-effects)
- **数据模型：** 无新增；插件缺失时保留参数和状态（data-model.md 第 11 节）。
- **备注：** BAE 插件体系（ARCHITECTURE 第 14 节）需要在效果类型里加"过渡"（两个输入 + 进度），见 19 和 07 的请求。

## 附录 A：Pr 26.2 内置视频效果与视频过渡

来源：本机 Pr 26.2.0 的插件缓存 `Resources/txt/KnownPluginCache.txt`（每项的分类和 matchName），中文名取自 Pr 自带的简体中文词典（同一个字符串键）。† 表示 Pr 26 内置的 Film Impact 插件（matchName `AE.AE_Impact_*`），其余是 Adobe 自己的效果（多数 matchName 为 `AE.ADBE …`）。"（旧版）"是 26.0 起被 Film Impact 版本取代、但为兼容旧工程保留的效果。区分效果和过渡的依据是分类和插件模块名（Film Impact 的效果模块名以 `-FX` 结尾）。

**A.1 视频效果（107 个，不含"过时"）**

| 分类 | 效果 |
|---|---|
| 调整（Adjust） | 基本信号控制（ProcAmp）、灯光效果（Lighting Effects） |
| 模糊与锐化（Blur & Sharpen） | 减少交错闪烁（Reduce Interlace Flicker）、复合模糊（Compound Blur）†、定向模糊（旧版）（Directional Blur (Legacy)）、散景模糊（Bokeh Blur）†、方向模糊（Directional Blur）†、焦点模糊（Focus Blur）†、通道模糊（Channel Blur）†、钝化蒙版（Unsharp Mask）、锐化（Sharpen）、高斯模糊（Gaussian Blur）†、高斯模糊（旧版）（Gaussian Blur (Legacy)）、摄像机模糊（Camera Blur） |
| 颜色校正（Color Correction） | ASC CDL（ASC CDL）、Lumetri 颜色（Lumetri Color）、晕影（Vignette）†、色调（Tint）、视频限幅器（Video Limiter）、亮度和对比度（Brightness & Contrast）、色彩空间变换（Color Space Transform） |
| 扭曲（Distort） | 变形稳定器（Warp Stabilizer）、放大（旧版）（Magnify (Legacy)）、旋转扭曲（Twirl）、果冻效应修复（Rolling Shutter Repair）、波形变形（Wave Warp）、湍流置换（Turbulent Displace）、球面化（Spherize）、边角定位（Corner Pin）、镜像（Mirror） |
| 生成（Generate） | 四色渐变（4-Color Gradient）、渐变（Gradient）†、渐变（Ramp）、镜头光晕（Lens Flare）、闪光（Lightning）、高级闪电（Advanced Lightning） |
| 图像控制（Image Control） | 反转（Invert）、圆角裁切（Rounded Crop）†、通道混合（Channel Mix）†、黑白（Black & White） |
| 沉浸式视频（Immersive Video） | VR 分形杂色（VR Fractal Noise）、VR 发光（VR Glow）、VR 平面到球面（VR Plane to Sphere）、VR 投影（VR Projection）、VR 数字故障（VR Digital Glitch）、VR 旋转球面（VR Rotate Sphere）、VR 模糊（VR Blur）、VR 色差（VR Chromatic Aberrations）、VR 锐化（VR Sharpen）、VR 降噪（VR De-Noise）、VR 颜色渐变（VR Color Gradients） |
| 键控（Keying） | Alpha 调整（Alpha Adjust）、亮度键（Luma Key）、徽标剪切（Logo Cutout）†、极致键（Ultra Key）、轨道遮罩键（Track Matte Key）、颜色键（Color Key） |
| 光线与发光（Lights & Glows） | RGB 分离（RGB Split）†、仙境光晕（Wonder Glow）†、体积光（Volumetric Rays）†、微光（Glint）†、残影光（Echo Glow）†、漏光（Light Leaks）†、边缘光（Edge Glow）† |
| 杂色与颗粒（Noise & Grain） | 杂色（Noise）†、杂色（旧版）（Noise (Legacy)） |
| 透视（Perspective） | 基本 3D（Basic 3D）、投影（Drop Shadow）、长阴影（Long Shadow）† |
| 风格化（Stylize） | Alpha 发光（Alpha Glow）、复制（Replicate）、彩色浮雕（Color Emboss）、查找边缘（Find Edges）、毛边（Roughen Edges）、画笔描边（Brush Strokes）、色调分离（Posterize）、闪光灯（Strobe Light）、马赛克（Mosaic）†、马赛克（旧版）（Mosaic (Legacy)） |
| 时间（Time） | 像素运动模糊（Pixel Motion Blur）、抽帧（Posterize Time）、时间扭曲（Timewarp）、残影（Echo） |
| 变换（Transform） | 3D 旋转（3D Rotate）†、偏移（Offset）、变换（Transform）、扩张（Grow）†、摆动（Wiggle）†、旋转（Spin）†、相机抖动（Camera Shake）†、移动（Move）†、缩小（Shrink）†、羽化边缘（Edge Feather）、自动重新构图（Auto Reframe）、裁剪（Crop）、间隔片段（Spacer）† |
| 过渡（Transition） | 块溶解（Block Dissolve）、渐变擦除（Gradient Wipe）、线性擦除（旧版）（Linear Wipe (Legacy)） |
| 实用程序（Utility） | Cineon 转换器（Cineon Converter）、SDR 遵从情况（SDR Conform）、仿制（Clone）†、描边（Stroke）†、简单文本（Simple Text）、自动对齐（Auto Align）† |
| 视频（Video） | 形状（Shape）、文本（Text） |

**A.2 视频过渡（94 个）**

| 分类 | 效果 |
|---|---|
| 动画（Animation） | 块状运动（Block Motion）†、弹簧运动（Spring Motion）†、弹跳运动（Pop Motion）†、折叠运动（Fold Motion）†、旋转运动（Spin Motion）†、牵引运动（Pull Motion）†、翻转运动（Flip Motion）†、行进运动（Travel Motion）† |
| 溶解（Dissolve） | MorphCut（Morph Cut）、交叉溶解（Cross Dissolve）†、交叉溶解（旧版）（Cross Dissolve (Legacy)）、亮度淡化（Luma Fade）†、加深 Alpha（Burn Alpha）†、叠加溶解（Additive Dissolve）†、无叠加溶解（Non-Additive Dissolve）、模糊溶解（Blur Dissolve）†、渐隐为白色（Dip to White）†、渐隐为白色（旧版）（Dip to White (Legacy)）、渐隐为颜色（Dip to Color）†、渐隐为黑色（Dip to Black）†、渐隐为黑色（旧版）（Dip to Black (Legacy)）、胶片溶解（Film Dissolve）†、马赛克（Mosaic）† |
| 脏旧与扭曲（Grunge & Distort） | VHS 损坏（VHS Damage）†、万花筒（Kaleidoscope）†、地震（Earthquake）†、垃圾摇滚（Grunge）†、液态扭曲（Liquid Distortion）†、混乱（Chaos）†、玻璃（Glass）†、电视电源（TV Power）†、花屏（Glitch）†、闪烁（Flicker）† |
| 沉浸式视频（Immersive Video） | VR 光圈擦除（VR Iris Wipe）、VR 光线（VR Light Rays）、VR 渐变擦除（VR Gradient Wipe）、VR 漏光（VR Light Leaks）、VR 球形模糊（VR Spherical Blur）、VR 色度泄漏（VR Chroma Leaks）、VR 随机块（VR Random Blocks）、VR 默比乌斯缩放（VR Mobius Zoom） |
| 光线与模糊（Lights & Blurs） | Flash（Flash）†、交叉缩放（Cross Zoom）†、光晕（Flare）†、光线扫描（Light Sweep）†、加深色度（Burn Chroma）†、发光（Glow）†、射线（Ray）†、径向模糊（Radial Blur）†、方向模糊（Directional Blur）†、曝光过度（Solarize）†、条纹（Stripe）†、漏光（Light Leak）†、磷光体（Phosphor）†、缩放模糊（Zoom Blur）†、色度泄漏（Chroma Leak）†、镜头模糊（Lens Blur）† |
| 滑动（Slide） | 3D 卷（3D Roll）†、伸展（Stretch）†、急摇（Whip）†、急摇（旧版）（Whip (Legacy)）、拆分（Split）†、拆分（旧版）（Split (Legacy)）、推（Push）†、滑动（Slide）†、滚动（Roll）†、胶卷（Film Roll）†、中心拆分（Center Split） |
| 智能工具（Smart Tools） | 形状流动（Shape Flow）†、运动相机（Motion Camera）†、运动补间（Motion Tween）† |
| 文本（Text） | 打字机（Typewriter）†、文本动画制作器（Text Animator）† |
| 变换（Transformers） | 3D 回旋（3D Spinback）†、3D 旋转（3D Spin）†、切片（Slice）†、帧（Frame）†、波形（Wave）†、百叶窗（Louver）†、镜像（Mirror）†、页面剥落（Page Peel）† |
| 划出（Wipe） | 径向擦除（Radial Wipe）†、径向擦除（旧版）（Radial Wipe (Legacy)）、拉伸擦除（Stretch Wipe）†、时钟式擦除（Clock Wipe）†、时钟擦除（旧版）（Clock Wipe (Legacy)）、星形擦除（Star Wipe）†、柔和擦除（Soft Wipe）†、线性擦除（Linear Wipe）†、霓虹灯擦除（Neon Wipe）†、面板擦除（Panel Wipe）†、高原式擦除（Plateau Wipe）†、双侧平推门（Barn Doors）、插入（Inset） |

**A.3 归入"过时"的效果（默认隐藏，打开旧工程时仍然可用；已略去 RAW 源设置等内部项）**

| 分类 | 效果 |
|---|---|
| 过时（Obsolete） | RGB 差值键（RGB Difference Key）、中间值（旧版）（Median (Legacy)）、写入（Write-on）、剪辑名称（Clip Name）、单元格图案（Cell Pattern）、吸管填充（Eyedropper Fill）、固态层合成（Solid Composite）、图像遮罩键（Image Matte Key）、圆形（Circle）、复合模糊（Compound Blur）、复合运算（Compound Arithmetic）、差值遮罩（Difference Matte）、径向阴影（Radial Shadow）、快速模糊（Fast Blur）、斜面 Alpha（Bevel Alpha）、时间码（Timecode）、棋盘（Checkerboard）、椭圆（Ellipse）、油漆桶（Paint Bucket）、浮雕（Emboss）、混合（Blend）、百叶窗（Venetian Blinds）、移除遮罩（Remove Matte）、算术（Arithmetic）、纹理化（Texturize）、网格（Grid）、色度键（Chroma Key）、蒙尘与划痕（Dust & Scratches）、蓝屏键（Blue Screen Key）、计算（Calculations）、边缘斜面（Bevel Edges）、通道模糊（Channel Blur）、非红色键（Non Red Key）、页面剥落（过时）（Page Peel (Obsolete)）、杂色 Alpha（Noise Alpha）、杂色 HLS 自动（Noise HLS Auto）、杂色 HLS（Noise HLS）、设置遮罩（Set Matte）、阈值（Threshold） |

**A.4 不在插件缓存里的 Pr 原生视频过渡**

下面这些是 Pr 自己实现的过渡（不是 AE 式插件，所以不在上面的缓存里），名字和中文取自词典 `$$$/MediaCore/Transitions/*`。Pr 26.2 的效果面板里哪些仍然显示、哪些已移入"过时"，没能在运行中的 Pr 里确认，待核：

| 分类 | 过渡 |
|---|---|
| 3D 运动（3D Motion） | 立方体旋转（Cube Spin）、翻转（Flip Over） |
| 光圈（Iris） | 盒形划像（Iris Box）、交叉划像（Iris Cross）、菱形划像（Iris Diamond）、圆划像（Iris Round） |
| 页面剥落（Page Peel） | 翻页（Page Turn） |
| 滑动（Slide） | 带状滑动（Band Slide） |
| 擦除（Wipe） | 带状擦除（Band Wipe）、棋盘擦除（Checker Wipe）、棋盘（CheckerBoard）、油漆飞溅（Paint Splatter）、风车（Pinwheel）、随机块（Random Blocks）、随机擦除（Random Wipe）、螺旋框（Spiral Boxes）、百叶窗（Venetian Blinds）、楔形擦除（Wedge Wipe）、擦除（Wipe）、水波块（Zig-Zag Blocks） |
| 缩放（Zoom） | 交叉缩放（旧版）（Cross Zoom (Legacy)） |

**BAE 的实现顺序建议（M4）：** 交叉溶解、渐隐为黑色、渐隐为白色、渐隐为颜色、胶片溶解、叠加溶解、推、滑动、拆分、线性擦除、径向擦除、时钟式擦除、交叉缩放、急摇。这十几个覆盖了剪辑里绝大多数用法；其余（含 Film Impact 系列）作为声明式 WGSL 插件逐步补充，不阻塞剪辑工作区。

## 附录 B：音频效果

Pr 的音频效果按文件夹分类（Pr 2022 起的分类方式）。中文名取自 Pr 词典（`$$$/dvaaudiofilters/*`、`$$$/MediaCore/MediaLayer/Filter/*`）。分类归属是按公开文档整理的，个别项待核。音频效果的使用和优先级见 [audio.md](audio.md)。

| 分类 | 效果 |
|---|---|
| 振幅与压缩（Amplitude and Compression） | 增幅（Amplify）、通道混合器（Channel Mixer）、通道音量（Channel Volume）、消除齿音（DeEsser）、动态（Dynamics）、动态处理（Dynamics Processing）、强制限幅（Hard Limiter）、多频段压缩器（Multiband Compressor）、单频段压缩器（Single-band Compressor）、电子管建模压缩器（Tube-modeled Compressor） |
| 延迟与回声（Delay and Echo） | 模拟延迟（Analog Delay）、延迟（Delay）、多功能延迟（Multitap Delay） |
| 滤波器和 EQ（Filter and EQ） | 带通（Bandpass）、低音（Bass）、FFT 滤波器（FFT Filter）、图形均衡器（10 段 / 20 段 / 30 段）（Graphic Equalizer）、高通（Highpass）、低通（Lowpass）、陷波滤波器（Notch Filter）、参数均衡器（Parametric Equalizer）、科学滤波器（Scientific Filter）、简单的陷波滤波器（Simple Notch Filter）、简单的参数均衡（Simple Parametric EQ）、高音（Treble） |
| 调制（Modulation） | 和声/镶边（Chorus/Flanger）、镶边（Flanger）、移相器（Phaser） |
| 降噪/恢复（Noise Reduction/Restoration） | 自适应降噪（Adaptive Noise Reduction）、自动咔嗒声移除（Automatic Click Remover）、消除嗡嗡声（DeHummer）、降噪（DeNoise）、减少混响（DeReverb）、增强语音（Enhance Speech） |
| 混响（Reverb） | 卷积混响（Convolution Reverb）、室内混响（Studio Reverb）、环绕声混响（Surround Reverb） |
| 特殊（Special） | 扭曲（Distortion）、吉他套件（Guitar Suite，待核是否仍在 26.2）、响度计（Loudness Meter）、响度探测计（Loudness Radar）、母带处理（Mastering）、人声增强器（Vocal Enhancer） |
| 立体声图像（Stereo Imagery） | 立体声扩展器（Stereo Expander） |
| 时间与变调（Time and Pitch） | 音高换档器（Pitch Shifter） |
| 不分类（顶层） | 平衡（Balance）、用右侧填充左侧（Fill Left with Right）、用左侧填充右侧（Fill Right with Left）、反相（Invert）、静音（Mute）、互换通道（Swap Channels）、音量（Volume）、双耳均衡器 - 环境立体声（Binauralizer - Ambisonics）、环绕声控制器 - 环境立体声（Panner - Ambisonics） |

另外 Pr 可以加载系统里的 VST3 和 Audio Units 插件（BAE 对应 19 的插件体系，以后再定是否支持 VST3/AU）。

## 附录 C：音频过渡

| 分类 | 过渡 |
|---|---|
| 交叉淡化（Crossfade） | 恒定功率（Constant Power，默认音频过渡；效果面板里显示为"固定电源"）、恒定增益（Constant Gain，效果面板里显示为"固定增益"）、指数淡化（Exponential Fade） |

恒定功率在过渡中间响度不下陷（两侧按等功率曲线交叉），恒定增益是线性交叉（中间会有可感知的音量下陷），指数淡化用对数曲线淡出淡入，适合淡入/淡出到静音。
