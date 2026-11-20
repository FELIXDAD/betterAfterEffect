# 02 · 文档模型与命令总线

| 项 | 内容 |
|---|---|
| 分支 | `core/doc-model` |
| Orca 工作区 | `bae-02-doc-model` |
| 阶段 | P0（最优先：所有人都依赖它） |
| 依赖 | 无 |
| 负责目录 | `crates/bae-model/`、`docs/spec/` |

## 目标

实现 USD 式分层的工程数据模型、命令总线、撤销重做和工程文件格式。**先尽快把公共类型和 trait 合并进 `dev`**（哪怕内部还是简单实现），让 P1 工作流可以开始。

## 要做

- **基础类型**：`Stage`、`Sublayer`、`PrimPath`、`Prim`、`PrimType`、`Attribute`、`Value`（Float、Int、Bool、String、Vec2、Vec3、Color、Bezier 路径、Enum、AssetRef、PrimRef 等）、`Keyframe`、`Time`（flicks，见架构文档 4.4）。
- **分层组合**：层叠顺序、Edit Target、意见强弱解析、Reference（包括跨工程文件）；查询"某个属性的值来自哪一层"；把覆盖下推/合并到更弱的层。
- **存储**：每个 Sublayer 用一个 Loro 文档存储（Prim 树用 movable tree，属性用 map）。
- **命令系统**：`Op`、`Command`、`Transaction`、`CommandRegistry`（参数 JSON Schema 用 schemars 生成）；命令带作者；按作者撤销/重做（只撤销自己的修改）。
- **事件**：`StageEvent` 订阅，包含受影响的 Prim 路径和时间范围。
- **Prim schema 初版**：Project、Folder、Comp、Layer（各种类）、形状内容、Mask、Effect、Artboard、Board/Panel、Shot、Sequence/Track/Clip、Asset。字段先覆盖 M1 需要的，并且要能无损表达 AE 的基础工程（与 15 对齐）。
- **素材引用**（见架构文档第 16 节）：每个素材记录相对路径、原始绝对路径、文件指纹；路径映射规则；缺失状态。
- **效果参数模型**：参数类型是 AE 效果参数类型的超集（滑块、浮点滑块、角度、复选框、颜色、点、3D 点、弹出菜单、图层、参数组、任意数据、路径、按钮），参数有稳定的数字 ID，支持参数标志；效果实例有不透明的状态数据槽（对应 AE 的 sequence data）。为将来兼容 AE 插件预留，见第 15 节。
- **插件数据**：插件注册的效果和类型带插件 ID 与版本；插件缺失时保留全部数据并标记为占位。
- **基础命令**：创建/删除/移动/重命名 Prim、设置属性、增删改关键帧、设置引用、新建 Sublayer、切换 Edit Target。
- **工程文件**：目录包读写（架构文档 4.6）、`.baez` 单文件打包、`bae export-json`（把 Sublayer 导出成可读 JSON）、格式版本号与迁移机制。
- **文档**：`docs/spec/data-model.md`（概念、Prim 类型与字段）、`docs/spec/project-format.md`（文件格式）。

## 对外接口

- 所有人：`Stage`、`PrimPath`、`Attribute`、`Value`、`Time`、`Command`、`CommandRegistry`、`StageEvent`。
- 09 协同：每个 Sublayer 底层的 Loro 文档，以及导入/导出增量的接口。
- 17 Agent：CommandRegistry 的元数据（名字、描述、参数 Schema）。

## 验收

- 1 万个 Prim 的工程打开时间 < 500 ms；单次属性修改到事件发出 < 1 ms。
- 单元测试覆盖：意见强弱解析、引用、撤销重做、两个 Stage 实例并发修改同一 Sublayer 后合并结果一致。
- 只依赖纯 Rust 库，不依赖 GPU 或窗口；能在 Linux 上编译测试。

## 不做

- 关键帧插值和表达式求值（03）、网络和文件同步（09）。

## 进度

- [x] 公共类型与 trait（PR A）：`Time`/`FrameRate`/`TimeRange`、`PrimPath`、`Value`、`AttrSpec`/`Keyframe`/`Expression`、`PrimType`/`PrimSpec`/`PrimTree`/`Reference`、`SublayerId`/`AuthorId`、`Op`/`Edit`、`StageEvent`、`Command`/`DynCommand`/`CommandRegistry`。
- [x] Sublayer 用 Loro 存储（movable tree + 可合并 map），内存镜像；快照与增量导入导出。
- [x] 层叠强弱解析、值来源查询、自动补 `over`；命名空间操作作用于所有层。
- [x] 事务、按作者撤销/重做、失败回滚；StageEvent 订阅（关键帧精确时间范围）。
- [x] 基础命令 12 个（见 `docs/spec/data-model.md` 第 6 节）。
- [x] 验收：1 万 Prim 打开 111 ms、修改到事件 5.3 µs（release，M4）；两个 Stage 并发修改同一 Sublayer 后合并一致（测试）。
- [x] 分层引用（含跨工程、嵌套、防循环、改动传播到引用方）、`opinion.move` 下推、`sublayer.merge` 合并、`Stage::flatten`（PR B）
- [x] 工程文件读写（目录包、`.baez`、原子写入）、`bae export-json`、版本迁移框架、跨工程引用自动加载、素材引用与查找/重新链接/路径映射（PR C，`docs/spec/project-format.md`）
- [x] Prim schema 初版（`schema.rs`，命令按 schema 校验）、新值类型（time、frame_rate、text_document、遮罩羽化点）、效果参数模型与 `EffectRegistry`、插件缺失占位（PR D；15 的对齐请求的答复见 data-model.md 第 10 节）
- [x] 09 的请求（PR E）：`import_updates_batch`、`node_path` / `node_id`、提交时间戳、保存时的临时文件名防撞、拒绝只有大小写不同的工程层 ID
- [x] 04 的请求：schema 的 Layer 增加 `preserve_transparency`
- [x] spec 文档初稿：`docs/spec/data-model.md`（随 PR 逐步补全；`project-format.md` 在 PR C）

## 请求（其他工作流在这里留言）

### 15 interop/ae → 02（2026-11-20）

交换格式草案：`docs/interop/bae-interchange.md`（分支 `interop/ae`）。已按 `docs/spec/data-model.md` 对齐，用 dev 上的 `bae-model` 实测通过（`RestorePrim` → `to_trees` → 重新序列化逐字段一致）。对不上或需要 02 定的地方：

1. **（已在 PR #9 修复，15 已复核）Bug：带关键帧的 `PrimTree` 不能从 JSON 反序列化。** `PrimTree.spec` 用了 `#[serde(flatten)]`，serde 的 flatten 会先把内容缓冲，之后 `AttrSpec.keyframes`（`BTreeMap<Time, _>`）的字符串键 `"0"` 不再转成 `i64`，报 `invalid type: string "0", expected i64`。`AttrSpec`、`PrimSpec` 单独反序列化正常。复现：一个属性带一个关键帧的 `PrimTree`，`to_string` 后 `from_str::<PrimTree>` 失败。建议给 `keyframes` 加自定义的 `#[serde(with = ...)]`（键按字符串解析成 i64，也接受数字），或者 `PrimTree` 不用 flatten；加一个 `PrimTree` JSON 往返测试。影响 `RestorePrim`/粘贴/`export-json` 读回和 15 的导入器。
2. **`Value` 缺文字文档类型。** AE 的「源文本」可以打关键帧，每个关键帧是一整个 TextDocument。提案新增 `text_document` 变体，字段见交换格式 4.7（`text`、`font`、`font_size`、`fill_color`、`justification`……，snake_case）。逐字符样式以后再加。
3. **内置 Prim 类型**：提案新增 `Marker`（合成/图层标记，属性见交换格式 6.9）、`TextAnimator`、`TextSelector`。草案已经用这三个名字。
4. **时间和帧率的值类型**：入点、出点、开始时间、时长等草案用 `int`（flicks），帧率用 `float`（AE 原值，导入时换成 `FrameRate`）。如果 02 想加 `time` / `frame_rate` 变体，告诉我改导出脚本。
5. **`BezierPath.feather`** 是「每个顶点一个二维值」，AE 的遮罩羽化点是沿路径分布的点（所在段、段内位置、半径、张力、内/外）。建议改成 `Vec<FeatherPoint>{segment, t, radius, tension, inner, corner_angle}` 之类，或者 PR D 时一起定。v0 不导出羽化点，进报告。
6. **关键帧时间基准**：AE 内部按图层时间存关键帧（移动图层时关键帧跟着走），脚本接口给的是合成时间。BAE 的 `AttrSpec.keyframes` 是相对图层还是相对合成？请在 data-model.md 里写明；03 也要用。
7. **图层顺序**：草案约定 `layers` 下第一个子 Prim 是 AE 最上面的图层（AE 序号 1）。请确认 BAE 的约定。
8. **预合成图层**：草案用 `source` 属性（`prim_ref` 指向 `/comps/...`）。还是用 `PrimSpec.references`？引用会把整个合成子树合成到图层下面，语义上更像「实例化」而不是「图层的素材来源」。
9. **属性命名建议（PR D）**，单位与 AE 一致，值可以 1:1 搬：
   - 变换：`transform:anchor_point`（vec3）、`transform:position`（vec3）、`transform:position_x` / `_y` / `_z`（单独尺寸时）、`transform:scale`（vec3，百分比）、`transform:orientation`（vec3，度）、`transform:rotation_x`、`transform:rotation_y`、`transform:rotation`（z，度）、`transform:opacity`（百分比）、`transform:point_of_interest`（摄像机、灯光）。
   - 遮罩：`mask:path`、`mask:feather`、`mask:opacity`、`mask:expansion`，以及 `mode`、`inverted`。
   - 图层、合成、素材的设置项：交换格式第 6 节的名字（`name`、`in_point`、`out_point`、`start_time`、`stretch`、`parent`、`blend_mode`、`track_matte`、`track_matte_layer`、`three_d`、`width`、`height`、`frame_rate`、`duration`、`bg_color`、`work_area_start`……）。
   - 效果参数：AE 参数 matchName 本身就带稳定序号（`ADBE Gaussian Blur 2-0001`），如果 PR D 用数字参数 ID，建议直接对应这个序号，导入导出不用查表。
   - BAE 不认识的 AE 属性，导入时原样保留为 `ae:` 开头的属性（编码规则见交换格式 4.8），导回 AE 不丢。

### 15 interop/ae → 02（2026-11-20，AE 26.2.1 实测后补充）

10. **AE 的 NTSC 帧率是十进制的 29.97，不是 30000/1001。** AE 26.2.1 报告 `frameRate` 为单精度 29.97（`29.9700317382812`），帧和关键帧落在 `n / 29.97` 秒（第 30 帧 = `706306306` flicks，`30000/1001` 时是 `706305600`）；23.976 同理。这些时间不是整数 flicks，`FrameRate{2997, 100}.is_exact()` 为 false。15 的导入器会把 AE 的 29.97 / 23.976 / 59.94 换成 `30000/1001` 一类，并按帧号吸附（交换格式 4.2）。建议在 `data-model.md` 写明这条约定，免得各工作流各自处理。

### 来自 09 collab/sync（2026-11-20）

共享目录协同的 `updates/<文档>/<client-id>.log` 格式已定，见 `docs/spec/sync.md` 第 4 节（09 只写 `updates/` 和 `history.loro`，不碰 `project.json` 和 `sublayers/`）。下面几条按优先级排列：

1. **远端导入的性能。** `Stage::import_updates` 每次都重建整层镜像（1 万个 Prim 约 100 ms），Hub 模式下每秒会有很多条小增量，这会卡住持有 Stage 的线程。希望：(a) 导入时根据 Loro 的 diff 增量更新镜像；或者至少 (b) 提供 `Stage::import_updates_batch(sublayer, &[&[u8]])`，用 `LoroDoc::import_batch` 导入多条后只重建一次。09 会把同一层同一轮收到的增量合并成一次调用。
2. **记录提交时间。** 在 `Sublayer::new` / `from_snapshot` 里调用 `doc.set_record_timestamp(true)`，这样历史里每次提交都有时间，"查看任意时刻的工程"和差异里的"什么时候改的"才有数据。
3. **树节点 → 路径。** 按 Prim 列差异时，09 要把 Loro 的操作（`TreeID`）对应到 Prim 路径。希望加 `Sublayer::node_path(TreeID) -> Option<&PrimPath>`（镜像里已经有）。在此之前 09 按 data-model.md 第 4 节的规则（同名兄弟的 `<名字>_<peer>_<counter>`）自己遍历，并用测试对照镜像。
4. **小问题：** `write_atomic` 的临时文件名只用了进程号，NAS 上两台机器同时保存同一个文件时可能撞名，建议加随机后缀；只有大小写不同的工程层 ID（`Anim` 和 `anim`）在 macOS/Windows 上会落到同一个文件，建议在层叠里拒绝这种重复。

### 04 render/compositor → 02（2026-11-20）

1. schema v1 的 `Layer` 缺少「保留基础透明度」（AE 的 T 开关，`AVLayer.preserveTransparency`，AE-CMP-032）。建议加 `boolean("preserve_transparency", false)`。04 的 `MockEvaluator` 已经按这个名字读取。

### 06 render/color → 02（2026-11-20）

1. Footage prim 的"解释素材 > 颜色"还缺两个属性：`color:preserve_rgb`（bool，默认 false，AE"保留 RGB"）和 `color:linear_light`（choice `off|on|on_32bpc`，默认 `off`，AE"解释为线性光"）。现有的 `color_space`（字符串）按"覆盖媒体色彩空间"理解，空 = 按文件标签。bae-color 的 `Interpretation` 对应这三项。
2. Project prim：`working_space` 存 bae-color 的色彩空间 id（`srgb`、`acescg`……，空 = 无），请在 schema 文档里注明；另外请加 `working_gamma`（choice `2.2|2.4`，默认 `2.2`，AE `workingGamma`，不做色彩管理时使用）。OCIO 引擎相关属性（`color_engine`、`ocio_config`）等 06 的 OCIO PR 再提。

### 来自 11 ui/shell（2026-11-20）

历史面板要显示撤销栈里每一步的命令名和作者（包括其他协作者和 Agent）。现在 `StageEvent` 只有 `author` 和 `origin`，命令名只能从 `undo_label` 拿到栈顶一项，所以 11 暂时在外壳里用 `bae_ui::history::HistoryLog` 订阅事件、在执行命令前记下标题来拼。希望：

1. **`StageEvent` 带上事务标签。** 加 `label: Option<String>`（`transact` 的 `label`；撤销/重做时是被撤销那一步的标签；远端导入为 `None`，或者从提交消息 `{"author","label"}` 里取）。有了它，历史面板、Agent 活动记录、协同的"谁改了什么"都直接用事件，不用旁路传标题。
2. **列出撤销/重做栈。** `Stage::undo_labels(author) -> impl Iterator<Item = &str>`（从栈顶往下）和对应的 `redo_labels`，以及 `Stage::undo_authors() -> impl Iterator<Item = &AuthorId>`（有撤销栈的作者）。用于历史面板标出哪些步骤还能撤销、AE 式"点击某一步撤销到那里"（连续调用 `undo` N 次）。
3. （可选）`Stage::undo_to(author, depth)`：一次撤销 N 步、只发一个事件，避免历史面板一次撤销多步时触发 N 次重渲染。

### 19 core/plugins → 02（2026-11-20）

插件宿主已经把清单里的参数映射成 `bae_model::EffectDef`（`bae_plugin::effect_def`，规格 `docs/spec/plugin-architecture.md` §5.3.7），`match_name` = `<插件 ID>/<效果 ID>`，弹出菜单沿用 AE 的 1 起始序号。还差两点：

1. **R02-8 字符串和素材参数。** `ParamKind` 加 `String`（值 `Value::String`）和 `Asset`（值 `Value::AssetRef`）。清单里的 `string` / `asset` 参数类型先保留不开放，等这两个类型合并后再打开。
2. **R02-9 相对坐标的点参数默认值。** AE 的 `PF_ADD_POINT` 默认值是图层尺寸的比例（例如 50%,50% = 图层中心），添加效果时才换算成像素。`EffectParamDef.default` 是静态值，表达不了。建议 `ParamKind::Point { default_relative: bool }`（或 `DefaultValue` 加一种"按图层尺寸换算"），由添加效果的命令写入 `比例 × 图层尺寸`。在这之前，插件宿主在 `EffectDef` 里不填这类默认值，渲染时由效果按图层范围自行换算。

### 20 interop/aex-compat → 02（2026-11-20）

AE 插件兼容层的预留请求，详见 `docs/spec/aex-compat.md` §4.3–§4.4、§5.1。`bae_aex_host::model` 已经把 AE 参数转换成合并后的 `bae_model::EffectDef` 并有测试，下面是转换时丢失的信息。19 已提的 R02-1/3/4/5/7/9 我们附议，不重复。

1. **R02-01 参数模型补全（现在）。** ① 增加没有值的参数类型（AE `NO_DATA`，通常配合自绘界面），占一个序号，目前只能暂用"自绘按钮"代替；② `ParamFlags` 加"只能保持插值"（AE `CANNOT_INTERP`，用于数值参数）；③ `EffectParamDef` 加可选的"旧工程值"：工程里缺少这个参数时用它，而不是 `default`（AE `USE_VALUE_FOR_OLD_PROJECTS`）；④ 未知的 `kind` 能原样往返（例如保留原始 JSON），新版插件写的工程在旧版 BAE 里也能打开。
2. **R02-07 引用型参数的语义（现在）。** 图层参数的值要区分"无"、"本图层"（复制效果或图层后仍指向新的本图层）和"某个图层"；目标删除后解析为"无"而不是报错。`ParamKind::Layer` 的文档现在写的是 "none = this layer / none"，请明确。
3. **R02-03 状态槽约定（只需写明）。** `state` 整体写入、整体替换，协同时最后写入者获胜、永不按字节合并；复制效果、复制图层、预合成、打包、导入时原样复制。
4. **R02-11 事件来源（现在）。** 在 19:R02-7 的基础上，复制（带来源 Prim）、预合成、撤销恢复被删除的 Prim 要能分辨，兼容层据此决定是否让插件重建实例。

P3 再做（现在别堵死）：R02-05 只有界面的参数值、R02-09 每个效果的实现偏好、R02-12 每个属性的变化戳。
