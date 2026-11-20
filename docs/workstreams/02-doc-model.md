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
- [ ] 分层引用（含跨工程）、下推/合并到弱层（PR B）
- [ ] 工程文件读写、`.baez`、`export-json`、版本迁移、素材引用（PR C）
- [ ] Prim schema 初版、效果参数模型、插件数据（PR D）
- [x] spec 文档初稿：`docs/spec/data-model.md`（随 PR 逐步补全；`project-format.md` 在 PR C）

## 请求（其他工作流在这里留言）
