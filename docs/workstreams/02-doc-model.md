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
- [ ] 工程文件读写、`.baez`、`export-json`、版本迁移、素材引用（PR C）
- [ ] Prim schema 初版、效果参数模型、插件数据（PR D）
- [x] spec 文档初稿：`docs/spec/data-model.md`（随 PR 逐步补全；`project-format.md` 在 PR C）

## 请求（其他工作流在这里留言）

### 15 interop/ae → 02（2026-11-20）

交换格式草案：`docs/interop/bae-interchange.md`（分支 `interop/ae`）。已按 `docs/spec/data-model.md` 对齐，用 dev 上的 `bae-model` 实测通过（`RestorePrim` → `to_trees` → 重新序列化逐字段一致）。对不上或需要 02 定的地方：

1. **Bug：带关键帧的 `PrimTree` 不能从 JSON 反序列化。** `PrimTree.spec` 用了 `#[serde(flatten)]`，serde 的 flatten 会先把内容缓冲，之后 `AttrSpec.keyframes`（`BTreeMap<Time, _>`）的字符串键 `"0"` 不再转成 `i64`，报 `invalid type: string "0", expected i64`。`AttrSpec`、`PrimSpec` 单独反序列化正常。复现：一个属性带一个关键帧的 `PrimTree`，`to_string` 后 `from_str::<PrimTree>` 失败。建议给 `keyframes` 加自定义的 `#[serde(with = ...)]`（键按字符串解析成 i64，也接受数字），或者 `PrimTree` 不用 flatten；加一个 `PrimTree` JSON 往返测试。影响 `RestorePrim`/粘贴/`export-json` 读回和 15 的导入器。
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

