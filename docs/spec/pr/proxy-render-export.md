# Pr 清单：代理、预览渲染与导出（PR-EXP）

## 范围

收录设置与代理、预览渲染与渲染条、渲染和替换、导出模式与导出设置、导出预设、快速导出、渲染队列、导出帧、项目管理器（收集与修剪）。预设名称取自本机 Pr 26.2 的 `MediaIO/systempresets/` 和 `Settings/IngestPresets/`（只列名称，不复制预设文件）。

**BAE 的分工：** 编码、代理转码、渲染队列在 08 `bae-media`（已有 `EncodeSettings`、`Encoder`、`RenderQueue`、H.264/HEVC/ProRes/PNG/EXR 编码）；序列的逐帧画面来自 `bae-edit` 求值 + `bae-render`；音频混入由 08 的混音模块提供（尚未有，见 audio.md 开头）；打包和裁片在 21 `bae-package`。

## 代理

### PR-EXP-001 · 收录设置与代理（Ingest Settings / Proxies）

- **行为要点：** 项目设置 > 收录设置（或媒体浏览器里的"收录"勾选框）：导入时自动"复制"（复制到项目位置，可选 MD5 校验）、"转码"（转成指定格式）、"创建代理"、"复制并创建代理"，由 Adobe Media Encoder 在后台执行。Pr 26.2 自带的代理预设：Apple ProRes 422 Proxy、H.264 MOV、H.264 MP4、GoPro CineForm、VR 单像/立体 DNxHR；转码预设：与源匹配 - AVC-Intra / Apple ProRes 422 / 422 LT / DNxHD / GoPro CineForm / H.264 高比特率 / IMX / XAVC / XDCAM EX / XDCAM HD。代理分辨率可以比原片小，但长宽比必须相同。
- **优先级：** M4（M1 预留数据结构）
- **BAE 模块：** bae-media (+ bae-model, bae-ui-edit)
- **数据模型：** Asset 增加代理文件引用 `proxy:file:relative`、`proxy:file:original`、`proxy:file:size`、`proxy:file:mtime`、`proxy:file:hash`（与原文件引用同一套规则，project-format.md 第 5 节），以及 `proxy:width`、`proxy:height`、`proxy:status`（`none | creating | attached | offline`）（edit-model.md 第 13 节第 9 条）。
- **备注：** BAE 默认把代理放在工程 `cache/proxies/`（可删除、可重建）；NAS 协同时 Hub 可以代为生成代理（ARCHITECTURE 第 7 节）。代理是 4K 多轨实时播放、J/K/L 快速穿梭、多机位的兜底，M4 必须有。

### PR-EXP-002 · 创建代理与连接代理（Create Proxies / Attach Proxies）

- **行为要点：** 在项目面板选中素材，右键"代理 > 创建代理"：格式（H.264 / QuickTime）、预设、目标位置（原始媒体旁的 Proxies 文件夹或指定位置），交给 Media Encoder 排队转码，完成后自动连接。"连接代理"手动指定已有的代理文件；"重新连接完整分辨率媒体"在代理和原片之间重新建立关系。列表视图可以加"代理"列查看状态。
- **优先级：** M4
- **BAE 模块：** bae-media (+ bae-ui-edit, bae-model)
- **数据模型：** 同 PR-EXP-001；连接代理是命令（`asset.set_proxy`），与 `asset.set_file` 对称。
- **备注：** 后台转码要可取消、可显示进度，复用 08 的 `RenderQueue`。

### PR-EXP-003 · 切换代理（Toggle Proxies）

- **行为要点：** 监视器按钮"切换代理"（默认不在按钮栏，需要按钮编辑器加入）或偏好 > 媒体 > "启用代理"：全局在代理和原片之间切换，播放和缩略图跟着切换；导出时默认用原片，除非在导出设置里勾选"使用代理"。代理分辨率较小时，Pr 自动按原片尺寸放大显示，运动参数不受影响。
- **优先级：** M4
- **BAE 模块：** bae-media (+ bae-render, bae-ui-edit)
- **数据模型：** 无，开关是视图/用户状态。
- **备注：** 渲染缓存键要包含"用的是代理还是原片"（08 的 `content_hash` 已按打开选项区分）。

## 预览渲染

### PR-EXP-004 · 渲染条与预览渲染（Render Bar / Render Previews）

- **行为要点：** 时间标尺下方的渲染条：无色（可以用 Mercury 实时播放）、黄色（可能需要渲染才能实时）、红色（需要渲染才能实时）、绿色（已有预览文件）。Enter（Return）"渲染入点到出点的效果"只渲染红色部分；"渲染入点到出点"渲染所有黄色和红色；"渲染选择项""渲染音频"；"删除渲染文件"/"删除入点到出点的渲染文件"。渲染结果存为预览文件（格式在序列设置里），改动受影响的区间后对应部分变回红色。
- **优先级：** M4
- **BAE 模块：** bae-render (+ bae-ui-edit, bae-media)
- **数据模型：** 无，预览缓存在工程 `cache/`（不进版本）。
- **备注：** BAE 的目标是常见工程 GPU 实时，不需要 Pr 式的手动"预渲染成文件"。渲染条显示 04 帧缓存的状态（AE 的绿条），Enter 触发后台把入出点范围渲染进缓存（与 AE 的 RAM 预览同一机制），缓存失效由 StageEvent 精确驱动（04 已规划）。

### PR-EXP-005 · 智能渲染与使用预览（Smart Rendering / Use Previews）

- **行为要点：** 导出时勾选"使用预览"，已经渲染过的区间直接复用预览文件（不重新渲染），预览格式与导出格式相同（例如都是 ProRes）时可以"智能渲染"：没有改动的原片片段直接拷贝压缩数据、不重新编码，导出更快、无损。
- **优先级：** 以后
- **BAE 模块：** bae-media (+ bae-render)
- **数据模型：** 无
- **备注：** 与 21 的"无损直拷裁片"是同一类能力（08 已有按 GOP 直拷的 `AvTrimmer`），可以复用。

### PR-EXP-006 · 渲染和替换（Render and Replace）

- **行为要点：** 右键剪辑"渲染和替换"：把剪辑（常见是动态链接的 AE 合成、动态图形模板、高负担的素材）渲染成媒体文件，序列里的剪辑换成这个文件；格式可选（与序列匹配 / 与素材匹配 / 指定预设），可含手柄帧；"恢复未渲染的内容"换回原来的剪辑。
- **优先级：** 以后
- **BAE 模块：** bae-edit (+ bae-media, bae-render)
- **数据模型：** Clip 增加 `rendered:source`（prim_ref，指向渲染出的 Asset）和"原来源"的保留方式（以后定；可能用 `source` + `rendered:active` 开关）。
- **备注：** BAE 的动效合成是实时引用，不需要渲染和替换才能播放；但对特别重的合成，"冻结成文件"仍然有用（审阅、交付、弱机器）。

## 导出

### PR-EXP-007 · 导出模式（Export Mode）

- **行为要点：** 顶部"导出"标签（Opt+4）或 Cmd+M 进入导出模式：左栏是目标（媒体文件，以及 Pr 内置的发布目标如 YouTube、Vimeo、TikTok、X、Behance 等，可多选同时导出），中栏是设置（文件名、位置、预设、格式，下面是视频、音频、多路复用、字幕、效果、元数据、常规几个可展开区块），右栏是预览（范围选择、缩放方式：缩放以适合 / 缩放以填充 / 拉伸以填充 / 适合等）和估算文件大小。底部"导出"直接导出，"发送至 Media Encoder"排进队列。
- **优先级：** M4（M1 只要能把序列导出成文件）
- **BAE 模块：** bae-ui-edit (+ bae-media, bae-render)
- **数据模型：** 导出设置是 08 的 `EncodeSettings`；渲染任务是否存进工程由 02/08 定（AE 的渲染队列存在工程里，见 AE 清单），剪辑序列的导出任务复用同一种 Prim。
- **备注：** 不做第三方平台发布（需要账号授权），只做媒体文件。

### PR-EXP-008 · 导出设置（Export Settings）

- **行为要点：** 视频：编解码器、帧大小（与源匹配或指定）、帧速率、场序、长宽比、电视标准、配置文件和级别、比特率编码（CBR / VBR 1 次 / VBR 2 次）、目标和最大比特率、关键帧距离、硬件编码 / 软件编码、HDR 元数据；音频：格式、编解码器、采样率、声道（单声道 / 立体声 / 5.1）、比特率；多路复用（MP4 / 无）；常规：导入到项目中、使用预览、使用代理、以最高渲染质量渲染、以最大深度渲染、时间插值（帧采样 / 帧混合 / 光流）。
- **优先级：** M4
- **BAE 模块：** bae-media (+ bae-ui-edit)
- **数据模型：** 无新增（`EncodeSettings`）。
- **备注：** 08 已支持 H.264/HEVC 硬件编码、ProRes 全系列、PNG/EXR 序列；"音频混入导出"尚未完成，是 M1 能导出带声音的成片的前提（见 08 的请求）。

### PR-EXP-009 · 导出预设（Export Presets）

- **行为要点：** 预设下拉按格式分组。Pr 26.2 自带（节选）：H.264 —"匹配源 - 适应高/中/低比特率"、高品质 2160 4K / 1080 HD / 720 HD / 480 SD、YouTube / Vimeo 2160p 4K、1080p HD、720p HD、480p SD、Facebook、Twitter、移动设备、Adobe Stock 交付、VR；HEVC —与源匹配 - 高比特率、HDR（Rec.2020 / HLG / PQ）、8K UHD、4K UHD、HD 1080p / 720p、SD；QuickTime —与源匹配、Apple ProRes 422 Proxy / LT / 422 / HQ、ProRes 4444 / 4444 XQ（含带 Alpha、HLG、PQ 版本）、GoPro CineForm、DV；MXF OP1a —DNxHD / DNxHR、AVC-Intra、XAVC 等广播格式；图像序列 —PNG（含带 Alpha）、TIFF、DPX、OpenEXR（含带 Alpha、32 位浮点、Piz、无压缩）、JPEG、Targa、BMP、GIF；动画 GIF；音频 —WAV、AIFF、MP3、AAC。用户可以保存、导入、导出自己的预设（.epr）。
- **优先级：** M4（M1 只需要"与源匹配 H.264"和"ProRes 422"两个）
- **BAE 模块：** bae-media (+ bae-ui-edit)
- **数据模型：** 预设是 `EncodeSettings` 的命名集合，内置数据，不进工程。
- **备注：** BAE 不打包 FFmpeg（VISION），MXF、DNxHR、AVC-Intra、CineForm 这类广播和中间格式交给可选 FFmpeg 插件（08 规划中），内置预设只覆盖系统编码器能做的格式。不复制 Adobe 的 .epr 文件。

### PR-EXP-010 · 快速导出（Quick Export）

- **行为要点：** 标题栏右侧的"快速导出"按钮：只有文件名和位置、预设（匹配源 - 适应高比特率、高品质 1080p HD、高品质 720p HD、高品质 4K 等少数几个）三项，点"导出"直接用 H.264 导出当前序列（入出点或整条）。
- **优先级：** M4
- **BAE 模块：** bae-ui-edit (+ bae-media)
- **数据模型：** 无
- **备注：** 对剪辑岗位和 Agent 都是最常用的导出路径，命令 `sequence.export`（序列、范围、预设、路径）要在 M2"Agent 通过 MCP 操作"之前提供（实现依赖 08 的编码和音频混入），按钮界面在 M4。

### PR-EXP-011 · 导出队列与后台导出（Media Encoder Queue）

- **行为要点：** Opt+Shift+M（或导出模式里"发送至 Media Encoder"）把导出任务交给 Adobe Media Encoder：可以排多个任务、批量对多个序列套同一预设、监视文件夹；排队后可以继续在 Pr 里剪辑（AME 渲染时动态链接到项目的快照）。
- **优先级：** M4
- **BAE 模块：** bae-media (+ bae-ui-edit)
- **数据模型：** 渲染任务（见 PR-EXP-007 的数据模型说明）；后台导出时用的是提交那一刻的工程版本（09 的版本向量），之后的编辑不影响正在导出的任务。
- **备注：** BAE 不需要独立的编码程序：08 的 `RenderQueue` 在应用内后台运行。"按版本快照导出"利用了 BAE 的 CRDT 历史，比 Pr 的做法更可靠。

### PR-EXP-012 · 导出帧（Export Frame）

- **行为要点：** Shift+E 或监视器上的相机按钮：名称、格式（JPEG、PNG、TIFF、DPX、OpenEXR、Targa、BMP、GIF，待核完整列表）、路径、"导入到项目中"；导出的是监视器当前帧，按序列分辨率（不是监视器的回放分辨率）。
- **优先级：** M4（Agent 需要的截图能力由 17 的 `render.frame` 提供，M1 已有类似能力）
- **BAE 模块：** bae-ui-edit (+ bae-media, bae-render)
- **数据模型：** 无
- **备注：** 无

### PR-EXP-013 · 导出范围（Export Range）

- **行为要点：** 导出模式预览区下方的范围：整个源（整条序列）、序列入点到出点、工作区（旧版）、自定义（在预览时间轴上拖出范围，不改序列入出点）。默认是序列入点到出点（有入出点时）。
- **优先级：** M4
- **BAE 模块：** bae-ui-edit (+ bae-media)
- **数据模型：** 无，范围是导出任务的参数。
- **备注：** 无

### PR-EXP-014 · 导出效果（Export Effects）

- **行为要点：** 导出设置"效果"区块：Lumetri Look/LUT（导出时套一个 LUT）、SDR 遵从情况（HDR 转 SDR）、图像叠加（水印）、名称叠加、时间码叠加、时间调谐器（微调节目总时长到目标长度）、视频限制器、响度标准化（PR-AUD-018）。这些只作用于导出文件，不改序列。
- **优先级：** 以后（时间码叠加、水印可以 M4）
- **BAE 模块：** bae-media (+ bae-render, bae-effects)
- **数据模型：** 无，导出设置的一部分。
- **备注：** 渲染管线在序列画面之后、编码之前插入一组"导出后处理"效果节点，04/08 协作实现。

### PR-EXP-015 · 项目管理器（Project Manager: Collect / Consolidate）

- **行为要点：** "文件 > 项目管理"：选择序列；方式"收集文件并复制到新位置"或"整合并转码"；选项：排除未使用剪辑、包含手柄（默认若干帧）、包含音频符合文件、包含预览文件、将图像序列转换为剪辑、重命名媒体文件以匹配剪辑名、包含 After Effects 合成、保留 Alpha；显示原有和生成后的项目大小估算。
- **优先级：** M2 — 对应 21 的打包与视频裁片（VISION 第 7 条、M2"打包时视频裁片"）。
- **BAE 模块：** bae-package (+ bae-edit, bae-media)
- **数据模型：** 无新增。21 需要 `bae-edit` 提供每条序列实际用到的源区间（含嵌套、变速、过渡手柄），即 edit-model.md 第 10 节的 `used_source_ranges`。
- **备注：** "包含手柄"就是 21 的"前后余量"；"排除未使用剪辑"就是"只打包用到的素材"。21 任务书里已经留好这些参数，14 负责提供区间计算（见 21 的请求）。
