# 预期工具名与参数约定（待确认）

本页列出指南里用到的全部 MCP 工具。17 `agent/api` 还没有实现，这里的名字来自：

- 架构文档第 10 节：`stage.describe`、`prim.get`、`prim.find`、`selection.get`、`render.frame`、`render.contact_sheet`；
- 17 任务书：`attribute.evaluate`、`history.*`、`sublayer.*`；
- 02 `core/doc-model` 已经合并的内置命令（`docs/spec/data-model.md` 第 6 节）：`prim.create`、`attr.set`、`key.set` 等。17 会把 `CommandRegistry` 里的命令自动生成为 MCP 工具，第 3 节按**命令 ID** 写；MCP 工具名是否与命令 ID 完全相同，由 17 决定（见第 10 节）。
- 其余（动效领域命令、事务、提案、批注等）是按意图推测的。

**凡是本页标"待确认"的名字，指南正文里都按这里的写法使用。** 17 合并后，18 会按实际导出逐行核对，改正文和 Skill，并把本页的"状态"列改成"已确认"。

## 1. 查询类（只读）

| 工具 | 用途 | 主要参数（推测） | 状态 |
|---|---|---|---|
| `stage.describe` | 工程摘要：合成树、图层、关键帧概况、工程层与编辑目标、锁提示 | `root`（节点路径，可选）、`detail`（`brief` / `normal` / `full`）、`max_chars` | 架构文档已列出；参数待确认 |
| `prim.get` | 读一个节点：类型、属性、每个属性的值来自哪个工程层、子节点 | `path` | 架构文档已列出；参数待确认 |
| `prim.find` | 按类型、名称、通配路径查找节点 | `type`、`name`（支持通配）、`under` | 架构文档已列出；参数待确认 |
| `selection.get` | 用户当前选中的节点和时间指示器位置 | — | 架构文档已列出 |
| `attribute.evaluate` | 某一帧（或多帧）属性的最终值（已含关键帧、表达式、父级） | `path`、`attribute`、`frames` | 17 任务书已列出；参数待确认 |

## 2. 视觉类（消耗渲染预算）

| 工具 | 用途 | 主要参数（推测） | 状态 |
|---|---|---|---|
| `render.frame` | 渲染单帧，返回 PNG | `comp`、`frame`、`scale`（`1`、`0.5`、`0.25`） | 架构文档已列出；参数待确认 |
| `render.contact_sheet` | 多帧缩略图拼成一张图，检查节奏 | `comp`、`frames`（列表）或 `start` / `end` / `step`、`columns`、`scale` | 架构文档已列出；参数待确认 |

## 3. 修改类：内置命令（02 已定义，由 CommandRegistry 生成）

| 意图 | 命令 ID | Agent 能不能用 | 状态 |
|---|---|---|---|
| 创建节点（合成、图层、形状内容等） | `prim.create` | 能 | 02 已定义 |
| 删除节点及子树 | `prim.delete` | 只用于自己创建的节点（见下） | 02 已定义 |
| 移动 / 重排 / 改名 | `prim.move` | 只用于自己创建的节点 | 02 已定义 |
| 改名 | `prim.rename` | 只用于自己创建的节点 | 02 已定义 |
| 设置属性静态值（写进编辑目标层） | `attr.set` | 能 | 02 已定义 |
| 清除编辑目标层对某个属性的意见，露出弱层的值 | `attr.clear` | 能。撤回自己的覆盖就用它 | 02 已定义 |
| 增改关键帧 / 删除关键帧 | `key.set` / `key.remove` | 能 | 02 已定义 |
| 设置、停用或删除表达式 | `attr.set_expression` | 能 | 02 已定义 |
| 设置引用（预合成、引用设计资产） | `prim.set_references` | 能 | 02 已定义 |
| 新建工程层 | `sublayer.create` | 不用 | 02 已定义 |
| 切换编辑目标 | `sublayer.set_edit_target` | **不用**（会话开始时就是自己的提案层） | 02 已定义 |
| 把覆盖从一层移到另一层（下推） | `opinion.move` | 不用，这是人审核时的操作 | 02 已定义 |
| 把一层整体合并进另一层（接受提案） | `sublayer.merge` | 不用，这是人审核时的操作 | 02 已定义 |
| 对已有关键帧应用缓动 / 缓入 / 缓出（关键帧辅助） | `key.ease` | 能 | 推测，待确认（12） |

**删除、移动、改名是命名空间操作。** 按 02 的规则，`prim.delete`、`prim.move`、`prim.rename` 作用于层叠里**所有**在该路径上有意见的工程层——对别人定义的节点做这三个操作，就是在改别人的层。Agent 只对自己在提案层里新建（`def`）的节点使用它们；需要从画面里去掉别人的节点、或改别人节点的名字时，先问用户。

**在强层里改弱层定义的节点**时，命令会自动在编辑目标层补一个 `over`（只覆盖、不定义），再写属性。这就是 Agent 在提案层里"覆盖"别人的值的机制，不需要额外操作。

**节点名和显示名不同。** 节点名是路径的一段（标识符：字母含中文、数字、`_`、`-`）；界面上的图层名是一个属性，可以重复、带空格（具体属性名在 02 的 Prim schema 里定，待确认）。指南的命名规范同时适用于两者：新建时两者用同一个 snake_case 名字。

## 4. 修改类：动效领域命令（bae-motion，由 12 定义）

这些是为了少写底层调用而推测的便捷命令。如果最终没有，就用第 3 节的通用命令组合完成。

| 意图 | 预期工具名 | 状态 |
|---|---|---|
| 新建合成 | `comp.create` | 待确认 |
| 新建图层（`shape` / `text` / `solid` / `null` / `camera` / `light` / `adjustment` / `precomp` / `footage`） | `layer.create` | 待确认 |
| 设置父级 | `layer.set_parent` | 待确认 |
| 设置图层入点 / 出点 | `layer.trim` | 待确认 |
| 预合成 | `layer.precompose` | 待确认 |
| 添加合成标记 / 图层标记 | `marker.add` | 待确认 |
| 给文本图层添加动画制作工具 | `text.add_animator` | 待确认 |

## 5. 事务

02 的事务是 `Stage::transact(author, label, …)`：一次调用里的所有修改是一个撤销单元，提交信息 `{"author", "label"}` 进历史和审计。跨多个 MCP 工具调用的事务需要 17 另外提供，下面的名字是推测。

| 意图 | 预期工具名 | 状态 |
|---|---|---|
| 开始一个事务（之后的命令合成一个撤销单元、一条提案） | `transaction.begin`（参数 `label`） | 待确认 |
| 提交事务 | `transaction.commit` | 待确认 |
| 放弃事务（回滚其中所有修改） | `transaction.abort` | 待确认 |

如果 17 最终采用"一次调用提交一批命令"的形式（例如 `command.batch`），指南里"开始事务 → 若干命令 → 提交"的写法统一改成一次批量调用。

## 6. 工程层、历史与提案

| 意图 | 预期工具名 | 状态 |
|---|---|---|
| 列出工程层、强弱顺序、当前编辑目标、自己的权限 | `sublayer.list` | 待确认（17 写了 `sublayer.*`） |
| 查看自己提案层里的全部改动 | `sublayer.diff`（参数 `sublayer`） | 待确认 |
| 列出历史（可按作者过滤） | `history.list` | 待确认（17 写了 `history.*`） |
| 撤销 / 重做自己的上一步 | `history.undo` / `history.redo` | 待确认 |
| 填写提案说明（改了什么、为什么） | `proposal.describe` | 待确认 |
| 在节点上留批注 | `comment.add` | 待确认 |

切换编辑目标（02 的基础命令）对 Agent 来说默认不需要调用：会话开始时编辑目标就是自己的提案层。

## 7. 分镜领域命令（bae-storyboard，由 13 定义）

| 意图 | 预期工具名 | 状态 |
|---|---|---|
| 新建分镜表 / 分镜格 | `board.create` / `panel.create` | 待确认 |
| 修改分镜格字段（镜头号、时长、景别、台词、动作、镜头运动、备注） | `panel.update` | 待确认 |
| 由分镜表生成动态分镜（Sequence） | `board.make_animatic` | 待确认 |
| 把分镜格升级为动效合成并建立 Shot 关联 | `panel.promote_to_comp` | 待确认 |

## 8. 交接与打包（由 15、21 定义）

| 意图 | 预期工具名 | 状态 |
|---|---|---|
| 导出给 AE（`.jsx` 重建脚本 + 素材 + 导出报告） | `interop.export_ae` | 待确认（15） |
| 工程与素材打包成 `.baez` | `package.create` | 待确认（21） |

## 9. 参数约定（建议 17 采纳）

- **时间参数优先用帧号**：相对于合成开始、从 0 开始的整数帧，例如 `{"frame": 12}`。同时接受时码字符串（`"0:00:00:12"`）。内部再换算成 flicks（见 [01 概念与术语](01-concepts.md) 的换算表）。
- **节点用完整路径**，例如 `/comps/main/layers/title`；不要依赖"当前选中"。
- **颜色**用 0–1 的浮点 RGBA，并注明色彩空间（默认合成工作空间）。
- **属性名**用 02 的命名空间写法：`transform:position`、`transform:opacity`、`transform:scale`（具体属性名由 02 的 Prim schema 定，待确认）。
- **值**：02 的 `Value` JSON 带类型标签（`{"type": "vec2", "value": [960, 540]}`）。指南示例为了易读写成不带标签的值；建议 17 按参数 Schema 接受不带标签的写法。
- **关键帧**：02 的关键帧 JSON 是 `interp_in` / `interp_out`（`linear | bezier | hold`）、`ease_in` / `ease_out`（**每个维度一组** `speed` + `influence`，影响用百分比）、`spatial_in` / `spatial_out`、`flags`。指南示例用简写 `{"frame": 22, "value": [960, 510], "in": {"speed": 0, "influence": 80}}`：`in` / `out` 是一组缓动，表示所有维度相同；`"interpolation": "linear"` 表示两侧都是线性；`"spatial": "linear"` 表示空间插值线性。建议 17 支持这种简写，或者在工具说明里写清楚完整格式。
- 修改类工具返回受影响的节点路径和新建节点的路径，方便后续调用。

## 10. 请 17 一并考虑

- **工具名里的点号。** 部分客户端或模型接口对工具名有字符限制（例如只允许字母、数字、`_`、`-`），带点号的名字可能被改写成 `stage_describe` 之类。请 17 确认各客户端里的实际名字，并在 MCP instructions 里使用 Agent 实际看到的写法。

- 把本指南作为 MCP 资源（resources）暴露，例如 `bae://agent-guide/03-animation`，让没有安装 Skill 的 Agent 也能按需读取。
- `stage.describe` 的输出里直接带上：合成帧率、时长、工作区域，当前编辑目标，自己的权限级别，锁提示。这几项是指南要求 Agent 每次先看的内容。
- `render.contact_sheet` 在每格角上标出帧号，方便 Agent 对照时间表。
