# Premiere Pro 基础功能清单（PR）

本目录是 BAE 剪辑工作区（工作流 14 `ui/edit`）的路线图依据：逐项列出 Premiere Pro（下称 Pr）日常剪辑用到的基础功能，写清楚"做到和 Pr 一样好用"要注意的行为细节、在 BAE 里什么时候做、由哪个模块负责、对数据模型有什么影响。

- 格式参照 `docs/spec/ae/`（AE 功能清单）：一项一个功能、字段固定、用 `speclint.py` 检查并生成下面的统计表。
- 数据模型的整体设计（序列、轨道、片段、过渡、标记怎样落到 `bae-model` 的 Prim 上）单独写在 [edit-model.md](edit-model.md)，清单里各项的"数据模型"字段引用它的章节号。
- 14 的分阶段计划和"框架必须预留的接口"在 `docs/workstreams/14-ui-edit.md`。

## 1. 范围

**包括：** 项目与素材箱、序列设置；源监视器和节目监视器；时间轴编辑（插入/覆盖、波纹、滚动、外滑、内滑、剃刀、轨道定位、同步锁定、链接、嵌套、多机位、速度）；过渡、视频效果、音频效果（附内置清单）；关键帧；音频（剪辑混合器、音轨混合器、基本声音）；调色（Lumetri）；基本图形、字幕；标记；代理、预览渲染、导出与预设；交换格式（XML、EDL、AAF、OTIO）和与 BAE 动效工作区的衔接；默认快捷键。

**不包括：** Frame.io 审阅、团队项目（Team Projects）、AI 生成功能（生成式扩展、语音增强以外的 Firefly 功能）、VR 沉浸式视频专用功能、磁带采集。BAE 有自己的协同（工作流 09）和 Agent（工作流 17），对应能力不照搬 Pr。

**底层统一，前端可换：** 清单按 Pr 的行为写，因为 VISION 要求"最大限度保留原有使用习惯"，并且剪辑工作区的第一个前端预设就是 Premiere 风格。但 `bae-edit` 的命令是通用的剪辑操作（插入、覆盖、修剪……），达芬奇、Final Cut Pro 风格的前端以后只换布局、键位和工具行为，复用同一套命令和数据（见 edit-model.md 第 9 节）。

## 2. 依据与核对方式

| 来源 | 用来核对什么 |
|---|---|
| 本机 Premiere Pro 2026（26.2.0）安装包里的数据文件 | 默认快捷键：`Keyboard Shortcuts/en/Adobe Premiere Pro Defaults V6.kys`（26.x 的默认键位，分 Edit / Color 两种模式）。内置视频效果和过渡的分类与 matchName：`Resources/txt/KnownPluginCache.txt`。序列预设：`Settings/SequencePresets/`。导出预设：`MediaIO/systempresets/`。收录与代理预设：`Settings/IngestPresets/`。中文译名：`Dictionaries/zh_CN/*.dat` 与 `en_US/*.dat` 按同一个键对照。 |
| Adobe 公开的 Premiere Pro 用户指南、脚本 API 文档 | 行为描述（已转述，不照抄）。 |
| AE 功能清单 `docs/spec/ae/` | 与 AE 共用的部分（效果、关键帧插值、色彩），直接引用 AE 条目，不重复写。 |

**没能做的：** 本机 Pr 启动时被一个非 Adobe 的模态框挡住（来自注入进程序的第三方动态库），在没有界面操作权限的情况下无法关闭，所以没有用 ExtendScript / QE DOM 在运行中的 Pr 里读取。上面列的数据文件就是 Pr 运行时加载的内容，清单里的快捷键、效果分类、预设名称都以这些文件为准；只靠公开文档、没有在文件里找到对应的细节，条目里标 `待核`。

**只记录事实，不复制资源：** 快捷键、效果名称、预设名称是功能事实，可以列出；不复制 Adobe 的图标、预设文件、示例工程、文档原文。

## 3. 条目格式

```
### PR-TL-001 · 插入编辑（Insert）

- **行为要点：** Pr 怎么做，以及做到"和 Pr 一样好用"必须注意的细节
- **优先级：** M1 | M2 | M3 | M4 | 以后（可附说明，例如"M4；M1 预留 link 属性"）
- **BAE 模块：** bae-edit (+ bae-ui-edit, bae-model)
- **数据模型：** 对 edit-model.md / data-model.md 的影响；没有影响写"无"
- **备注：** 可选，放最后
```

- 标题：`PR-<区域>-<三位序号> · 中文名（英文名）`。中文名用 Pr 简体中文版的界面译名（从 Pr 自带词典核对过）；没核对上的写 `待核`。
- "BAE 模块"第一个是主负责，括号里是配合的模块。名称必须是下表里的一个。
- 正文里的 `Cmd` 指平台命令键（macOS ⌘、Windows Ctrl），`Opt` 是 macOS 的 Option（Windows 的 Alt），与 `docs/spec/ui-shell.md` 第 4 节的写法一致。快捷键以 macOS 为准，Windows 不同的地方单独写。

## 4. 优先级（对照 `docs/VISION.md` 里程碑）

VISION 的总原则是"当前版本优先做好更快的 AE，剪辑现在只留接口、写框架"，剪辑工作区的完整实现在 M4。所以这里的优先级回答的是"这项功能最晚什么时候必须具备"：

| 优先级 | 含义 |
|---|---|
| **M1** | 0.0.1 / M1 的剪辑框架就要做，或者数据结构、命令接口必须现在定下来（以后加功能不用迁移数据）。主要是 `bae-edit` 的数据模型和核心编辑命令、能播放的骨架界面。 |
| **M2** | 随 M2"交接与协同"一起具备：动效合成作为片段实时引用、多人同时剪同一条序列、Agent 用 MCP 剪辑、打包时按序列裁片、导出给只用 Pr 的人（XML / OTIO）。 |
| **M3** | 随 M3"生态"：插件形式的效果、过渡、导入导出器。 |
| **M4** | M4"大一统"剪辑工作区：一个 Pr 用户打开就能顺手剪片的基础功能。清单里大多数条目在这里。 |
| **以后** | M4 之后的增强（多机位细节、字幕的广播格式、Lumetri 的高级部分等）。 |

一项功能可以是"M4 实现、M1 预留"：例如过渡渲染在 M4，但过渡的数据结构 M1 就要定好。这种情况写在"优先级"字段的说明里，并在 edit-model.md 里列出预留项。

## 5. 模块分工

| 模块 | 负责 | 约束 |
|---|---|---|
| `bae-edit` | 剪辑领域命令（插入、覆盖、修剪、链接、嵌套、过渡、标记……）、序列查询与求值（某时刻哪些片段可见/可听、源时间换算）、编辑计划（拖动修剪时的预览）。 | 不依赖界面、GPU、平台（ARCHITECTURE 第 2 节原则 2）。所有修改都是 `bae-model` 的命令。 |
| `bae-ui-edit` | 剪辑工作区界面：源/节目监视器、序列时间轴、工具、素材箱视图、音频混合器、剪辑工作区预设和 Pr 键位表。 | 只显示和交互，只调用命令。 |
| `bae-model` | Prim 类型与 schema（`Sequence`、`Track`、`Clip`、`Transition`、`Marker`、`Asset`……）。 | 由 02 维护；剪辑需要的 schema 改动见 edit-model.md 和 02 任务书的请求。 |
| `bae-anim` | 片段上属性的关键帧求值（运动、不透明度、音量、效果参数）、时间重映射。 | 由 03 维护。 |
| `bae-render` | 把序列在时刻 t 的求值结果渲染成画面：多轨叠加、过渡、嵌套序列、调整图层；预览缓存与渲染条状态。 | 由 04 维护。 |
| `bae-media` | 解码（含倒放、快速穿梭）、代理、音频混合与播放时钟、波形、导出编码（含音频）。 | 由 08 维护。 |
| `bae-color` / `bae-effects` | Lumetri 对应的调色、示波器；视频效果与过渡的 GPU 实现。 | 06 / 07。 |
| `bae-text` / `bae-vector` | 基本图形里的文字和形状、字幕渲染。 | 05。 |
| `bae-interop-edit`（拟新建） | FCP XML、EDL、AAF、OTIO 的导入导出，读取 .prproj。 | 现在没有工作流负责，建议归 14 的后续阶段或另开工作流，由协调人决定（见 14 任务书）。 |
| 其他 | `bae-ui`（外壳、键位系统）、`bae-package`（打包裁片）、`bae-agent`（MCP 工具）、`bae-plugin`（效果/过渡插件）、`bae-sync`（协同）、`bae-motion`（动效衔接）、`apps/bae`。 | |

## 6. 文件与区域

| 文件 | 区域 | 内容 |
|---|---|---|
| [project-sequence.md](project-sequence.md) | PRJ | 项目、素材箱、导入、解释素材、元数据、子剪辑、新建项、序列设置与预设 |
| [monitors.md](monitors.md) | MON | 源监视器、节目监视器、入出点、三点和四点编辑、JKL、回放分辨率、修剪监视器 |
| [timeline-editing.md](timeline-editing.md) | TL | 轨道、工具、插入/覆盖、提升/提取、波纹、滚动、外滑、内滑、剃刀、轨道定位、源修补、同步锁定、链接、编组、嵌套、多机位、速度 |
| [effects-transitions.md](effects-transitions.md) | FX | 固有效果、效果控件、效果预设、过渡；附录：Pr 26.2 内置视频效果、视频过渡、音频效果、音频过渡清单 |
| [keyframes.md](keyframes.md) | KF | 效果控件和时间轴里的关键帧、插值、时间基准 |
| [audio.md](audio.md) | AUD | 音频轨道与声道、增益、剪辑混合器、音轨混合器、自动化、基本声音、响度 |
| [color.md](color.md) | CLR | Lumetri 颜色、Lumetri 范围、色彩管理 |
| [graphics-captions.md](graphics-captions.md) | GFX | 文字和形状图形、基本图形、动态图形模板、字幕与转录 |
| [markers.md](markers.md) | MRK | 序列标记、剪辑标记、标记面板 |
| [proxy-render-export.md](proxy-render-export.md) | EXP | 代理、预览渲染、导出模式与预设、导出帧 |
| [interchange.md](interchange.md) | XCH | FCP XML、EDL、AAF、OTIO、读取 Pr 工程；与 BAE 动效工作区的衔接（对应 AE 动态链接） |
| [shortcuts.md](shortcuts.md) | KEY | Pr 26.2 默认快捷键（按用途分组）及对应的 BAE 命令 |

## 7. 统计（`python3 speclint.py --readme` 生成）

<!-- speclint:begin -->
（清单写完后生成）
<!-- speclint:end -->

## 8. 常用译名（与 AE 不同或剪辑特有）

以 Pr 2026 简体中文界面为准，供 11 合并进 `docs/glossary.md`。

| English | Pr 中文 | 说明 |
|---|---|---|
| Bin | 素材箱 | |
| Sequence | 序列 | |
| Clip / Master Clip / Subclip | 剪辑 / 主剪辑 / 子剪辑 | Pr 里时间轴上的片段叫"剪辑"。BAE 代码里叫 `Clip`，界面在剪辑工作区显示"剪辑"。 |
| Source Monitor / Program Monitor | 源监视器 / 节目监视器 | |
| Insert / Overwrite | 插入 / 覆盖 | |
| Lift / Extract | 提升 / 提取 | |
| Ripple Delete | 波纹删除 | |
| Ripple Edit Tool / Rolling Edit Tool | 波纹编辑工具 / 滚动编辑工具 | |
| Slip Tool / Slide Tool | 外滑工具 / 内滑工具 | |
| Razor Tool / Add Edit | 剃刀工具 / 添加编辑 | |
| Rate Stretch Tool | 比率拉伸工具 | |
| Track Select Forward Tool | 向前选择轨道工具 | |
| Link / Unlink / Linked Selection | 链接 / 取消链接 / 链接选择项 | |
| Nest | 嵌套 | |
| Multi-Camera | 多机位 | |
| Toggle Track Output | 切换轨道输出 | |
| Mark Clip / Mark Selection | 标记剪辑 / 标记选择项 | |
| Speed/Duration | 速度/持续时间 | |
| Time Remapping | 时间重映射 | |
| Frame Hold | 帧定格 | |
| Fit Clip / Change Clip Speed (Fit to Fill) | 适合剪辑 / 更改剪辑速度（适合填充） | 四点编辑对话框 |
| Audio Track Mixer / Audio Clip Mixer | 音轨混合器 / 音频剪辑混合器 | |
| Essential Sound / Essential Graphics | 基本声音 / 基本图形 | |
| Lumetri Color / Lumetri Scopes | Lumetri 颜色 / Lumetri 范围 | |
| Captions | 字幕 | |
| Adjustment Layer | 调整图层 | |
| Proxy / Create Proxies / Attach Proxies | 代理 / 创建代理 / 连接代理 | |
| Render In to Out / Render Effects In to Out | 渲染入点到出点 / 渲染入点到出点的效果 | |
| Interpret Footage / Link Media / Make Offline | 解释素材 / 链接媒体 / 设为脱机 | |
| Scene Edit Detection / Auto Reframe Sequence | 场景编辑检测 / 自动重构序列 | |
| Ripple Sequence Markers | 波纹序列标记 | |
| Insufficient media | 媒体不足 | 过渡手柄不够时的提示 |
