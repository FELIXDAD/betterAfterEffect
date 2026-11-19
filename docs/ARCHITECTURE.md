# 技术架构

本文是所有工作流共同遵守的技术约定。要修改这里的决定，请单独开 PR 说明理由，不要在功能 PR 里顺手改。

## 1. 技术选型

| 方面 | 选择 | 理由 | 没选的方案 |
|---|---|---|---|
| 语言 | Rust（stable，edition 2024） | 原生性能、无 GC、二进制小；能编译到 Win/Mac 以及将来的 iOS/Android；多线程安全 | C++（构建复杂、内存安全靠自觉）、Electron（体积大） |
| GPU | wgpu（Metal / D3D12 / Vulkan），着色器用 WGSL | 一套代码覆盖 Mac 和 Win，将来也能上移动端；支持 compute | 分别手写 Metal 和 D3D |
| 矢量渲染 | Vello（GPU compute 的 2D 渲染器）+ kurbo | 完全在 GPU 上，适合大量矢量图层；与 wgpu 同一技术栈 | Skia（C++，体积大） |
| 文字 | parley + fontique + skrifa（与 Vello 同一生态） | 纯 Rust，支持复杂排版和可变字体 | HarfBuzz + FreeType（C 依赖） |
| 界面 | egui + egui_dock，与渲染共用同一个 wgpu 设备 | 体积极小；视口纹理零拷贝；停靠面板适合做 AE 式布局 | Qt（50MB 以上、许可问题）、Tauri/WebView（视口要跨层合成） |
| 窗口与菜单 | winit；原生菜单栏用 muda | 标准做法，体积小 | — |
| 协同数据结构 | Loro（Rust 实现的 CRDT） | 支持树、Map、List、版本向量、历史回溯、增量同步、按用户撤销 | Automerge、Yjs (yrs) |
| 表达式 | QuickJS（rquickjs） | AE 表达式就是 JavaScript；QuickJS 只有约 1MB | V8（几十 MB）、Boa |
| 视频编解码 | 系统框架：macOS 用 VideoToolbox / AVFoundation，Windows 用 Media Foundation；FFmpeg 作为可选的按需下载插件 | 硬件加速，且不增加安装包体积 | 内置 FFmpeg（体积和许可） |
| 图像格式 | image / zune（PNG、JPEG、WebP）、exr、psd | 纯 Rust | OpenImageIO |
| 音频 | symphonia 解码 + cpal 播放 | 纯 Rust | — |
| 色彩 | 自研 bae-color：常用变换在 GPU 上实现，并能读取 OCIO 配置的常用子集 | OpenColorIO 体积大、构建成本高 | 链接 OCIO C++ 库 |
| 协作服务 | Rust：tokio + axum + WebSocket；单文件二进制 + Docker 镜像 | 能直接跑在 NAS 上 | — |
| Agent 接口 | MCP（stdio + Streamable HTTP），Rust SDK 用 rmcp | 主流 Agent（Claude、Codex 等）都支持 | 自定义协议 |

**风险最大的决定是界面用 egui。** 如果到 M1 发现做不出 AE/Ai 级别的观感，或者中文输入法体验不达标，由 `ui/shell` 工作流提出替换方案。为了让替换的代价可控，界面 crate 只负责显示和交互，不保存业务状态，也不直接改数据（见第 2 节原则 1）。

## 2. 五条总原则

1. **一切修改都是命令（Command）。** 界面、快捷键、Agent、脚本、导入器、协同同步，都只能通过 `bae-model` 的命令修改工程。命令可撤销、可序列化、带作者信息。这样撤销、协同、历史、Agent 能力、宏录制天然一致。
2. **界面能做的，Agent 都能做。** 命令只能定义在不依赖界面和 GPU 的 crate 里（例如剪辑命令放在 `bae-edit`，不能放在 `bae-ui-edit`）。界面 crate 只调用命令，不定义命令。
3. **帧留在 GPU 上。** 素材解码成 GPU 纹理后，合成、效果、色彩转换、显示都在 GPU 上进行。只有导出编码、缩略图和给 Agent 的截图才回读到 CPU。
4. **默认轻量。** 不打包大型运行时；可选能力（FFmpeg、额外 LUT、示例素材）按需下载。每个新依赖都要在 PR 里写明 release 体积增加了多少。
5. **沿用 AE/Ai 的习惯。** 术语、面板、快捷键、默认值（例如 AE 默认缓动影响力 33.33%）都和 AE/Ai 保持一致。中文术语使用 Adobe 官方中文版的译名，统一记录在 `docs/glossary.md`。

## 3. 模块关系

```
                           apps/bae（桌面应用：只负责组装）
     ┌──────────────┬──────────────┼──────────────┬───────────────┬──────────────────┐
 bae-ui-motion  bae-ui-design  bae-ui-edit   bae-agent (MCP)  bae-interop-ae / bae-interop-ai
     │              │              │              │               │
     └──────┬───────┴──────┬───────┘              │               │
         bae-ui（外壳、停靠、主题、快捷键、通用控件）  │               │
            │                                     │               │
            ▼                                     ▼               ▼
  ══════════════════════ 命令总线：Command / CommandRegistry ══════════════════════
     领域命令：bae-motion（动效）  bae-storyboard（分镜/设计）  bae-edit（剪辑）
            │
         bae-model（Stage / Sublayer / Prim / Attribute / 撤销 / 工程文件）
            │                         │
         bae-anim（关键帧求值、表达式）  bae-sync（CRDT 同步、在线状态、历史）──► services/bae-hub（NAS）
            │
         bae-render（渲染图、合成、缓存）
            ├── bae-vector / bae-text（Vello 矢量、排版）
            ├── bae-effects（WGSL 效果库）
            ├── bae-color（色彩管理）
            └── bae-media（解码、编码、图像格式）
         bae-gpu（wgpu 设备、表面、能力检测）    bae-platform（路径、日志、崩溃、单实例）
```

依赖规则：

- 依赖只能向下。界面依赖模型和渲染；渲染不依赖界面；模型不依赖渲染和 GPU。
- `bae-model`、`bae-anim`、`bae-sync` 以及各领域命令 crate，必须能在没有 GPU、没有窗口的环境（服务器、CI、将来的移动端）下编译运行。
- `apps/bae` 只做组装。每个功能 crate 对外提供一个 `register(&mut AppBuilder)`（命令、面板、工作区、导入器都在这里注册），这样多人开发时不会一直冲突同一个文件。

## 4. 工程数据模型（USD 式分层）

借用 USD 的概念，但自己实现（OpenUSD 太大，不引入）。

### 4.1 概念

| 概念 | 含义 | 在 AE/Ai 里大致对应 |
|---|---|---|
| Stage | 打开的工程，所有工程层合成后的结果 | 整个工程 |
| Sublayer（工程层） | 一组"意见"（opinion）的集合，是可以单独保存、单独同步的单元 | 无 |
| Layer Stack | 工程层的强弱顺序，强层的意见覆盖弱层 | 无 |
| Edit Target | 当前的编辑写进哪个工程层 | 无 |
| Prim | 工程里的节点，用路径定位，例如 `/comps/main/layers/title` | 合成、图层、画板、分镜格、序列、素材 |
| Attribute | Prim 上的属性：静态值、关键帧曲线或表达式 | 位置、不透明度、填充颜色…… |
| Reference | 一个 Prim 引用另一个 Prim，可以跨工程文件 | 预合成、链接的 Ai 文件 |

**命名约定：** AE 的"图层（Layer）"在我们的模型里是一种 Prim，类型名 `Layer`。USD 意义上的层统一叫 **Sublayer**，界面上叫"工程层"。代码和文档都按这个区分，不要混用。

### 4.2 Prim 类型（初版，由 `core/doc-model` 定稿）

- `Project`、`Folder`
- `Comp`：合成，包含尺寸、帧率、时长、背景色、工作区范围
- `Layer`：合成里的图层，种类有 `Shape | Text | Solid | Footage | Null | Camera | Light | Adjustment | PreComp | Audio`
- 形状内容：`ShapeGroup`、`Path`、`Fill`、`Stroke`、`GradientFill` 以及各种形状操作器，同时对齐 AE 形状图层和 Ai 的路径模型
- `Mask`、`Effect`：图层下的子 Prim，带参数属性
- `Artboard`：画板，Ai 风格的静态矢量画面，可以被合成引用
- `Board` / `Panel`：分镜表 / 分镜格（画面引用、时长、台词、动作、镜头说明）
- `Shot`：镜头，把分镜格、动效合成、剪辑片段串起来的 ID
- `Sequence` / `Track` / `Clip`：剪辑序列
- `Asset`：素材，包含文件引用、元数据、色彩空间、代理

### 4.3 一个工程的层叠示例

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

- 每个人默认编辑自己岗位的层。同一层也可以多人同时编辑，由 CRDT 合并。
- 界面上每个属性都能显示"这个值来自哪一层"，也可以把覆盖下推、合并到更弱的层（类似 USD 的 flatten）。
- 设计师在 `design/assets` 层改了角色，引用这个角色的所有动效合成实时更新。

### 4.4 时间

- 统一用整数时间单位 flicks（1 秒 = 705,600,000），所有常见帧率（23.976、24、25、29.97、30、50、59.94、60、120）的每一帧都能被整除表示，不会有浮点误差。
- 界面显示时按合成帧率换算成帧号或时码，与 AE 一致。

### 4.5 命令与事件

- `Op`：最小的修改单元，如 SetAttr、SetKeyframe、RemoveKeyframe、CreatePrim、DeletePrim、MovePrim、Reorder、SetReference。可以序列化成 JSON。
- `Command`：有名字、有参数 JSON Schema、会做参数校验，执行后产生一组 Op，作为一个撤销单元；带 `author`（用户或 Agent 的 ID）。
- `CommandRegistry`：所有命令都注册在这里。`bae-agent` 直接把它暴露成 MCP 工具；菜单和快捷键也绑定到命令 ID。
- 撤销按作者区分：只撤销自己的修改，不撤销协作者的修改。
- `StageEvent`：每次修改后发出，包含受影响的 Prim 路径和时间范围，渲染缓存据此精确失效。

### 4.6 工程文件格式（目录包）

```
MyFilm.bae/
  project.json                          工程清单：层叠顺序、工程设置、格式版本
  sublayers/
    root.loro                           每个工程层一个 Loro 快照（二进制）
    anim-shot010.loro
  updates/<sublayer>/<client-id>.log    每个客户端追加写的增量日志（共享目录协同用）
  assets/                               工程内素材，按内容哈希命名（可选；也可以只引用外部路径）
  cache/                                代理、缩略图、渲染缓存；可以删除，不进版本
```

- 用目录而不是单个文件，方便放在 NAS 上、做增量同步、只传输改动的部分。
- `bae export-json` 可以把任意工程层导出成可读的 JSON，用于调试、对比差异和给 Agent 阅读。
- 另外提供单文件打包格式 `.baez`（zip），用于发送和归档。

## 5. 渲染管线

```
Stage ──► bae-anim 在时间 t 求值 ──► EvaluatedComp（这一帧所有图层的最终变换和参数）
      ──► bae-render 构建渲染图（图层、预合成、效果、遮罩、轨道遮罩、混合模式、运动模糊）
      ──► GPU 执行（Vello 矢量 / 纹理采样 / WGSL 效果 / 色彩变换）
      ──► 显示到视口，或交给编码器导出
```

- 工作缓冲默认 RGBA16F（半精度浮点），可选 32 位浮点；8 位只用于显示和导出。
- 缓存键是 (Prim, 时间, 参数哈希)，由 StageEvent 精确失效。预览可以降到 1/2、1/4 分辨率，与 AE 一致。
- **与 AE 结果一致：** 同时支持 AE 默认的非线性混合和"线性化工作空间"两种模式，否则导入的 AE 工程颜色会对不上。
- 导出时 GPU→CPU 回读使用异步双缓冲，不阻塞渲染。
- 无窗口渲染：`bae render <工程> --comp main`，用于 CI、Agent 和渲染农场。

## 6. 色彩管理

- 工作空间：sRGB（非线性，兼容 AE 默认）、线性 sRGB、ACEScg、线性 Rec.2020、Display P3。
- 素材输入色彩空间：自动识别，也可以手动指定。常见 Log 曲线（S-Log3、LogC3/4、V-Log、Canon Log 等）分批支持。
- 显示：读取系统的显示器色彩空间；HDR 预览支持 macOS EDR 和 Windows Advanced Color。
- OCIO：读取 OCIO 配置的常用子集（矩阵、传递函数、1D/3D LUT、ACES 内置变换），在 GPU 上执行，不链接 OpenColorIO 库。
- 界面上显示的颜色（如拾色器）也要经过显示变换，保证看到的颜色一致。

## 7. 协同与版本管理

### 两种协同模式

1. **共享目录模式（不需要服务器）。** 工程目录放在 NAS 的 SMB/NFS 共享上。每个客户端只往自己的 `updates/<sublayer>/<client-id>.log` 追加增量，读取别人的日志后合并；CRDT 保证不会冲突。监听文件变化，同时用轮询兜底（SMB 的变更通知不可靠）。延迟在秒级。
2. **Hub 模式（实时）。** `services/bae-hub` 运行在 NAS 或服务器上（Docker）。客户端通过 WebSocket 同步增量和在线状态（谁在线、谁选中了什么、播放头在哪）。延迟在毫秒级。Hub 同时负责素材存储（内容寻址、去重、断点续传）、历史快照，以及可选的代理生成。

两种模式使用同一套同步协议。Hub 不可用时可以退回共享目录模式。

### 版本

- 自动历史：每个命令都是 CRDT 里的一次修改，可以回到任意时刻。
- 命名版本（Checkpoint）：记录所有工程层的版本向量，例如"给客户的第 1 版"。
- 分支：从某个版本分出一条线（例如客户修改的 A/B 方案），之后可以合并回来。
- 差异：按 Prim 列出改了什么、谁改的，并在时间轴上标出来。
- 锁（可选，仅作提示）：可以声明"我在改这个合成"，其他人会看到提示，但不会被强制阻止。

### 素材

- 素材按内容哈希存储，同一个素材整个团队只存一份。
- 远程素材在本地有缓存（LRU，大小可设）；大视频自动用代理预览。

## 8. 界面

- egui + egui_dock：所有面板都能停靠、拖动、组合成标签组，与 AE 一致。
- 工作区预设（用户可以自定义并保存）：
  - **分镜**（Ai 式）：左边工具栏，中间画板/分镜格画布，右边图层、属性、外观、对齐面板，下方分镜条。
  - **动效**（AE 式）：左边项目面板，中间合成查看器，下方时间轴（图层开关列、父级、属性展开、图表编辑器），右边效果和预设、效果控件、字符、段落面板。
  - **剪辑**：源监视器和节目监视器，下方多轨序列。
  - **审阅**：大播放器、批注、版本对比。
  - **Agent**：Agent 的对话和活动记录、提案审核。
- 快捷键按工作区切换：动效工作区用 AE 键位（U、P/S/R/T、J/K、Alt+[ / ] 等），分镜工作区用 Ai 键位（V、A、P、Shift+M 等）。
- 主题：默认深灰色，色值、字号、间距贴近 AE/Ai；图标自己绘制。
- 中英文界面。中文术语按 Adobe 官方中文版译名。
- 不内置中文字体（体积太大），界面使用系统字体（苹方、微软雅黑）。

## 9. AE / Ai 互转

### After Effects

- **导入主路径：** 在 AE 里运行的 ExtendScript 导出脚本（`bridges/ae/`）把工程导出成 BAE 交换格式（JSON + 收集的素材）。ExtendScript 在所有 AE 版本都能用。
- **没有 AE 时：** 直接读取 .aep（RIFX 二进制，靠逆向资料，只读，尽力而为）。
- **导出回 AE：** 生成 .jsx 脚本，在 AE 里运行即可重建工程（最可靠）。之后再评估直接写 .aepx（AE 的 XML 工程格式）。
- **Lottie**（bodymovin JSON）导入导出，作为公开、有文档的通道。
- 维护一张"AE 功能 → BAE 支持程度"的对照表：`docs/interop/ae-mapping.md`。

### Illustrator

- .ai 文件默认内嵌 PDF（"创建 PDF 兼容文件"默认开启）：解析 PDF 得到路径、填充、描边、渐变、文字和画板。
- 在 Ai 里运行的导出脚本（`bridges/ai/`）可以拿到更完整的信息：图层名、编组、画板名、外观。
- **剪贴板：** 在 Ai 里复制，在 BAE 里粘贴，得到可编辑的矢量（读取剪贴板里的 PDF/SVG 数据）。
- 导出：SVG、PDF（Ai 能直接以可编辑矢量打开），以及导回 Ai 的 .jsx 重建脚本。

## 10. Agent 接入

- `bae-agent` 内置 MCP 服务器：
  - stdio：本地 Agent 直接启动 `bae mcp`。
  - Streamable HTTP：连接正在运行的桌面应用或 Hub。
- MCP 工具由 CommandRegistry 自动生成（名字、描述、参数 Schema 都来自命令定义），另外提供查询和视觉类工具：
  - `stage.describe`：给 Agent 看的工程摘要
  - `prim.get`、`prim.find`、`selection.get`
  - `render.frame`：返回 PNG，让 Agent 看到画面
  - `render.contact_sheet`：多帧缩略图拼成一张图，用来检查动画节奏
- Agent 作为协作者加入工程：有名字和在线状态；默认写入自己的提案层 `agent/<name>`，用户在 Agent 工作区里逐条接受、拒绝，或者整体合并。
- 权限分三级：只读 / 只写自己的层 / 可写任意层。另有渲染预算和速率限制，所有操作都有审计记录。
- 无界面模式 `bae --headless`，用于批处理和 CI。
- 操作规范写在 `docs/agent-guide/`，同时打包成 Agent Skill（`skills/bae-animation/`）；MCP 服务器的 instructions 里放精简版。

## 11. 体积预算与性能基准

### 体积预算

| 项目 | 上限 |
|---|---|
| macOS arm64 应用包（解压后） | 40 MB |
| Windows x64 程序及依赖（解压后） | 40 MB |
| 安装包（压缩后） | 20 MB |
| bae-hub 二进制 | 15 MB |
| M0 阶段应用本体 | 15 MB |

- release 配置：`lto = "fat"`、`codegen-units = 1`、`strip = true`；性能热点 crate 用 `opt-level = 3`，界面等冷 crate 可以单独设成 `opt-level = "s"`。
- 不打包：Chromium/WebView、Qt、FFmpeg、OpenColorIO、OpenUSD、中文字体。
- CI 在每个 PR 上输出体积报告，超出预算直接失败。提高预算需要单独的 PR 并说明理由。

### 性能基准（由 `render/compositor` 建立，CI 记录趋势）

- **B1 动效图形：** 1080p30，10 秒，50 个形状图层 + 10 个文字图层 + 模糊和发光。
- **B2 素材合成：** 4K ProRes 素材 + 调色 + 遮罩。
- **B3 压力测试：** 500 个图层，多层预合成嵌套。
- 指标：预览帧率、最终渲染耗时（与 AE 同机对比）、显存占用、冷启动时间。参考硬件：Apple M1 和 RTX 3060 级别的 Windows 机器。

## 12. 平台

- macOS 12 及以上，Apple Silicon 优先，兼容 Intel。按架构分别打包，不做 universal 包（体积会翻倍）。
- Windows 10 21H2 及以上 / Windows 11，x64，D3D12。
- 将来的 iOS/Android：`bae-model`、`bae-anim`、`bae-sync`、`bae-render`、`bae-color` 不能依赖桌面专属 API。平台相关代码只能放在 `bae-platform`、`bae-gpu` 和 `bae-media` 的平台模块里。

## 13. 目录结构

```
apps/bae/                桌面应用入口（只做组装）
crates/
  bae-platform/          路径、日志、崩溃、单实例、命令行
  bae-gpu/               wgpu 设备、表面、能力检测
  bae-model/             Stage / Sublayer / Prim / 命令 / 撤销 / 工程文件
  bae-anim/              关键帧求值、表达式
  bae-render/            渲染图、合成、缓存、效果框架
  bae-vector/ bae-text/  矢量与文字
  bae-color/             色彩管理
  bae-effects/           内置效果
  bae-media/             媒体编解码
  bae-sync/              协同同步、历史
  bae-motion/            动效领域命令（预合成、关键帧助手……）
  bae-storyboard/        分镜/设计领域命令
  bae-edit/              剪辑领域命令
  bae-ui/                界面外壳、主题、通用控件、快捷键
  bae-ui-motion/ bae-ui-design/ bae-ui-edit/   三个工作区的界面
  bae-interop-ae/ bae-interop-ai/              AE / Ai 互转
  bae-agent/             MCP 服务器与 Agent 会话
services/bae-hub/        协作服务
bridges/ae/ bridges/ai/  在 AE / Ai 里运行的 ExtendScript 脚本
deploy/                  Docker 与 NAS 部署
docs/                    文档；docs/workstreams/ 是各工作流的任务书
skills/                  打包给 Agent 的操作规范
tools/ xtask/            构建、体积检查、基准等工具
```
