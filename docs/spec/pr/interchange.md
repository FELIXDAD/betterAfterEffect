# Pr 清单：交换格式与动效衔接（PR-XCH）

## 范围

和其他剪辑软件交换时间线（FCP XML、EDL、AAF、OTIO、OMF）、读取 Pr 工程，以及 BAE 剪辑工作区和动效工作区、分镜工作区的衔接（Pr 里靠 Dynamic Link 和"替换为 After Effects 合成"完成的事情）、脚本与 Agent 接口。

**模块：** 交换格式的读写建议放在一个新 crate `bae-interop-edit`（不依赖 GPU，和 `bae-interop-ae` / `bae-interop-ai` 平级），只把外部格式翻译成 `bae-edit` 的命令或 Prim 树，不包含剪辑逻辑。这个 crate 现在没有工作流负责，建议作为 14 的后续阶段，或由协调人另行安排（见 14 任务书）。与动效、分镜的衔接是 `bae-edit` 的命令。

**交换格式的映射关系（导入导出共用）：**

| BAE | FCP XML（xmeml） | OTIO | EDL | AAF |
|---|---|---|---|---|
| Sequence | `<sequence>` | `Timeline` + `Stack` | 一个 EDL 文件（每次只一条视频轨） | `CompositionMob` |
| Track | `<video>/<track>`、`<audio>/<track>` | `Track`（kind Video / Audio） | 轨道字段（V、A、A2…） | `TimelineMobSlot` |
| Clip | `<clipitem>` | `Clip` + `ExternalReference` | 事件 | `SourceClip` |
| 空隙 | 由 `<start>` 推出 | `Gap` | 黑场事件或省略 | `Filler` |
| Transition | `<transitionitem>` | `Transition` | `D`（溶解）、`W`（划像）事件 | `OperationGroup`（过渡） |
| Marker | `<marker>` | `Marker` | 注释行 | `DescriptiveMarker` |
| 嵌套 Sequence | 嵌套 `<sequence>` | 嵌套 `Stack` | 不支持（需要拼合） | 嵌套 `CompositionMob` |
| 速度 | `Time Remap` 效果 | `LinearTimeWarp` / `FreezeFrame` | M2 速度字段 | 速度效果 |

### PR-XCH-001 · Final Cut Pro XML 交换（FCP XML）

- **行为要点：** Pr 的"文件 > 导出 > Final Cut Pro XML"导出 FCP 7 风格的 xmeml：序列、轨道、剪辑（含源文件路径、入出点、源时间码）、基础过渡（交叉溶解等按名称）、速度、运动和不透明度的关键帧（部分）、标记、嵌套序列；不支持的效果省略，并生成翻译报告。导入同样格式（来自 FCP 7、达芬奇、Avid 等），找不到对应项时写进报告。达芬奇和很多工具都读写这种 XML，是剪辑软件之间最通用的交换方式。
- **优先级：** M2（导出，交给只用 Pr / 达芬奇的人）；M4（导入）
- **BAE 模块：** bae-interop-edit (+ bae-edit)
- **数据模型：** 无新增。导出用 `Stage::flatten` 后的结果；BAE 独有的东西（动效合成来源、图形剪辑）导出时要么渲染成文件（结合 PR-EXP-006 的渲染），要么作为离线剪辑并写进报告。
- **备注：** 帧率和时间码按 data-model.md 第 10 节的 NTSC 约定换算（XML 里 29.97 写成 timebase 30 + `ntsc=TRUE`）。

### PR-XCH-002 · EDL 交换（CMX 3600 EDL）

- **行为要点：** "文件 > 导出 > EDL"：EDL 标题、起始时间码、包含视频电平、包含音频电平、音频处理（音频跟随视频 / 分开处理音频 / 结尾处理音频）、要导出的轨道（一条视频轨 + 若干音频声道）。EDL 只能描述一条视频轨的剪切、溶解、划像和卷号/源时间码，最多 999 个事件；嵌套、效果、多轨叠加都丢失。导入 EDL 生成一条用离线剪辑组成的序列，再用"链接媒体"按卷号和源时间码找回素材。
- **优先级：** 以后
- **BAE 模块：** bae-interop-edit (+ bae-edit)
- **数据模型：** 依赖素材的源时间码和卷号：Asset `timecode_start`（PR-PRJ-016）、`meta:reel`（以后）。
- **备注：** 调色和在线编辑流程里还会用到；格式简单，实现成本低。

### PR-XCH-003 · AAF 交换（AAF）

- **行为要点：** "文件 > 导出 > AAF"主要用于把音频交给 Pro Tools 混音：选项包括将视频混合为一个文件、嵌入音频（或分开的音频文件）、拆分为单声道、渲染音频效果、包含剪辑副本（不含效果）、包含音频手柄帧、采样率和位深。也能导入 Avid Media Composer 的 AAF。Pr 对 AAF 的支持有限制（例如不能引用超过 2 个单声道流的音频文件，不能把单声道剪辑导出为立体声）。
- **优先级：** 以后
- **BAE 模块：** bae-interop-edit (+ bae-media)
- **数据模型：** 无新增。
- **备注：** AAF 是二进制结构化存储格式（微软复合文档），纯 Rust 实现工作量较大，按用户需求再排。

### PR-XCH-004 · OpenTimelineIO 交换（OTIO）

- **行为要点：** Pr 26 的导入导出格式列表里有"OpenTimelineIO (*.otio)"（具体从哪个版本开始支持、支持哪些特性待核）。OTIO 是开放、有文档的时间线交换格式（JSON），达芬奇、Avid、Nuke Studio、很多流程工具都支持，表达能力介于 EDL 和 FCP XML 之间：多轨、剪辑、空隙、过渡、标记、嵌套、线性变速和定格，媒体引用可以带多个分辨率（代理）。
- **优先级：** M2 — 建议作为 BAE 首选的时间线交换格式。
- **BAE 模块：** bae-interop-edit (+ bae-edit)
- **数据模型：** 无新增；映射见本文开头的表。BAE 独有的数据（效果参数、动效合成引用）放进 OTIO 的 `metadata["bae"]`，导回 BAE 时无损。
- **备注：** 用 Rust 直接读写 OTIO 的 JSON（不引入 Python 运行时）；格式规范公开，能用官方测试样例检查互通。

### PR-XCH-005 · OMF 导出（OMF）

- **行为要点：** 旧的音频交换格式（交给 Pro Tools），Pr 仍然提供"导出 OMF"：嵌入音频或分开的文件、渲染方式（复制完整音频 / 修剪音频文件加手柄）、采样率和位深；单个文件 2 GB 上限。
- **优先级：** 以后
- **BAE 模块：** bae-interop-edit (+ bae-media)
- **数据模型：** 无
- **备注：** 优先做 AAF 或者直接导出分轨音频（每条轨一个 WAV），OMF 不必做。

### PR-XCH-006 · 读取 Pr 工程（Import .prproj）

- **行为要点：** Pr 可以导入另一个 Pr 项目（全部或选中的序列），项目文件是 gzip 压缩的 XML，内部结构不公开、随版本变化。
- **优先级：** 以后
- **BAE 模块：** bae-interop-edit (+ bae-edit)
- **数据模型：** 无新增；效果按 effects-transitions.md PR-FX-013 的 matchName 对照（`AE.` 前缀）。
- **备注：** 与 15 读取 `.aep` 一样属于"尽力而为"。更稳妥的路径是让用户从 Pr 导出 FCP XML 或 OTIO 再导入；也可以像 15 一样写一个在 Pr 里运行的导出脚本（ExtendScript / UXP），按 BAE 交换格式导出（参考 `docs/interop/bae-interchange.md`）。

### PR-XCH-007 · 替换为动效合成（Replace With Motion Comp）

- **行为要点：** （对应 Pr 的"使用 After Effects 合成替换"）在时间轴上选中若干剪辑，右键"替换为动效合成"：在同一个工程里新建一个合成（尺寸、帧率、时长按序列和选区），选中的剪辑按原来的时间和上下关系变成合成里的图层（运动 → 变换、不透明度 → 不透明度、能对应的效果原样带过去、过渡变成不透明度关键帧或预合成），时间轴上这些剪辑被替换成一个引用该合成的剪辑。Pr 做这一步要新建 AE 项目并靠动态链接回来；BAE 不需要中间文件，也不需要保存后才能同步。
- **优先级：** M2
- **BAE 模块：** bae-edit (+ bae-motion)
- **数据模型：** 无新增：新建 Comp + Layer（schema 已有），Clip `source` 改指向 Comp；剪辑的 `shot` 传给合成对应的 Shot（如果有）。
- **备注：** `bae-edit` 依赖 `bae-motion`（都是不依赖 GPU 的领域命令 crate，允许），复用 12 的建图层命令。反方向"把合成展开成剪辑"不做（合成里的动画无法表达成剪辑）。

### PR-XCH-008 · 动效合成作为片段（Dynamic Link Equivalent）

- **行为要点：** Pr 里导入 AE 合成（动态链接）后，合成像素材一样用在序列里，AE 那边保存后 Pr 自动更新，不用渲染中间文件；"编辑原始"（Cmd+E）在 AE 里打开它；动态链接的剪辑通常播放较慢，需要预渲染或"渲染和替换"。BAE：项目面板里的任何合成可以直接放进序列（`source` 指向 Comp），合成的修改（包括协作者在动效层上的修改）实时反映在序列里；双击剪辑（或 Cmd+E）切到动效工作区并打开该合成，定位到对应时间；如果剪辑关联了镜头（`shot`），从镜头找到它的合成。
- **优先级：** M2 — 14 任务书的验收要求"修改其中一个动效合成，序列里马上看到更新"。
- **BAE 模块：** bae-edit (+ bae-ui-edit, bae-render, bae-motion)
- **数据模型：** 无新增：Clip `source` → Comp（#66 已支持，`query` 里有"双击片段跳到的动效合成"）。合成的源时间 0 = 合成时间 0；合成变长后剪辑长度不自动变（与嵌套序列相同），合成变短时超出部分按"媒体不足"处理。
- **备注：** 渲染时合成作为图层源（04 的预合成路径），缓存按 StageEvent 精确失效，只有改动影响到的帧重渲染。这是 BAE 相对 Pr + AE 最核心的优势，M2 的演示重点。

### PR-XCH-009 · 剪辑与图层之间的复制粘贴（Copy Clips to Comp）

- **行为要点：** Pr 里在时间轴复制剪辑，到 AE 合成里粘贴，得到对应的图层（保持入出点、相对时间、运动和不透明度关键帧、部分效果）；反方向不行。BAE：在剪辑时间轴复制剪辑，在动效工作区的合成里粘贴得到图层（与 PR-XCH-007 同一套转换），在剪辑时间轴粘贴从动效复制的图层则得到以这些图层的来源为来源的剪辑（只带入出点和时间位置）。
- **优先级：** M4
- **BAE 模块：** bae-edit (+ bae-motion, bae-ui-edit)
- **数据模型：** 无新增；剪贴板携带一个"来源工作区"标记，粘贴时按目标工作区转换。
- **备注：** 转换逻辑与 PR-XCH-007 共享，放在 `bae-edit` 里一个函数（剪辑 → 图层）。

### PR-XCH-010 · 镜头关联与分镜衔接（Shots / Storyboard to Edit）

- **行为要点：** （BAE 特有）分镜格、动效合成、剪辑片段通过 `Shot` 关联：从分镜工作区的动态分镜生成一条序列（每个分镜格一个剪辑，时长取分镜格的 `duration`，关联同一个 Shot）；镜头升级为动效合成后，序列里对应的剪辑可以一键把来源从"分镜画面"换成"动效合成"；剪辑里修剪了镜头时长，可以把新时长回写到分镜格和合成（提示，不自动改）。
- **优先级：** M2（数据关联和"换成动效合成"）；以后（时长回写）
- **BAE 模块：** bae-edit (+ bae-model, bae-ui-edit)
- **数据模型：** 已有：Clip `shot`、Panel `shot`、Shot `comp`。可能需要的补充：Shot 增加 `duration`（镜头的计划时长），作为分镜、动效、剪辑三方的参照（见 02 的请求）。
- **备注：** 13 的 PR #68（`bae-storyboard`：动态分镜、升级为合成）正在做分镜一侧，14 和 13 要对齐"动态分镜 → 序列"由谁生成：建议由 `bae-storyboard` 调用 `bae-edit` 的命令生成序列，Shot 是共同的键。

### PR-XCH-011 · 导出到 AE（Sequences in AE Export）

- **行为要点：** （BAE 特有）15 把 BAE 工程导回 AE 时，AE 没有"序列"，序列可以导成一个合成（每个剪辑一个图层，按轨道上下排列，过渡变成不透明度关键帧），或者另外导出 FCP XML 交给 Pr。
- **优先级：** 以后
- **BAE 模块：** bae-interop-edit (+ bae-edit)
- **数据模型：** 无新增。
- **备注：** "序列 → 合成"的转换与 PR-XCH-007 共用；导出到 AE 的入口在 15。

### PR-XCH-012 · 脚本与 Agent 接口（Scripting / Agent API）

- **行为要点：** Pr 的脚本接口（ExtendScript 和 UXP）提供项目、素材箱、序列、轨道、剪辑的对象模型：`app.project.sequences`、`Sequence.videoTracks`、`Track.insertClip` / `overwriteClip`、`TrackItem.start` / `end` / `inPoint` / `outPoint`、标记、导出（`Sequence.exportAsMediaDirect`、编码器队列），以及非公开的 QE DOM（加效果、过渡、剃刀）。第三方面板和自动化都靠它们。BAE：所有剪辑命令自动成为 MCP 工具（17），另外提供面向剪辑的查询工具（序列摘要：轨道、剪辑、空隙、标记、总长；某时刻的画面；按镜头的缩略图条），让 Agent 能看懂一条序列并做粗剪。
- **优先级：** M2 — Agent 通过 MCP 操作是 M2 的内容；命令在 M1 就已经是 MCP 工具（17 的现有机制）。
- **BAE 模块：** bae-agent (+ bae-edit)
- **数据模型：** 无新增。
- **备注：** 查询工具的形状建议：`sequence.describe`（结构化摘要）、`render.frame` 支持序列（17 已有，需接上序列求值）、`render.contact_sheet` 支持"每个剪辑一张"（见 17 的请求）。18 的 Agent 操作指南需要补"剪辑"一章（剪辑节奏、J/L 剪切、音频电平的基本规范），以后再提给 18。
