# 预期工具名与参数约定（待确认）

本页列出指南里用到的全部 MCP 工具。17 `agent/api` 还没有实现，这里的名字来自：

- 架构文档第 10 节：`stage.describe`、`prim.get`、`prim.find`、`selection.get`、`render.frame`、`render.contact_sheet`；
- 17 任务书：`attribute.evaluate`、`history.*`、`sublayer.*`；
- 02 任务书的"基础命令"（创建/删除/移动/重命名节点、设置属性、增删改关键帧、设置引用、新建工程层、切换编辑目标）——这些会由 `CommandRegistry` 自动生成为 MCP 工具，名字取决于命令 ID，下面只是按意图推测。

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

## 3. 修改类：通用命令（由 CommandRegistry 生成）

| 意图 | 预期工具名 | 状态 |
|---|---|---|
| 创建节点（合成、图层、形状内容、标记等） | `prim.create` | 待确认 |
| 删除节点 | `prim.delete` | 待确认 |
| 移动节点到新的父节点 | `prim.move` | 待确认 |
| 调整同级顺序（例如图层上下顺序） | `prim.reorder` | 待确认 |
| 重命名 | `prim.rename` | 待确认 |
| 设置属性的静态值 | `attribute.set` | 待确认 |
| 设置或修改关键帧（值、插值、缓动） | `keyframe.set` | 待确认 |
| 删除关键帧 | `keyframe.remove` | 待确认 |
| 对已有关键帧应用缓动 / 缓入 / 缓出（关键帧辅助） | `keyframe.ease` | 待确认 |
| 设置或清除表达式 | `expression.set` | 待确认 |
| 设置引用（预合成、引用设计资产） | `reference.set` | 待确认 |

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

## 8. 参数约定（建议 17 采纳）

- **时间参数优先用帧号**：相对于合成开始、从 0 开始的整数帧，例如 `{"frame": 12}`。同时接受时码字符串（`"0:00:00:12"`）。内部再换算成 flicks（见 [01 概念与术语](01-concepts.md) 的换算表）。
- **节点用完整路径**，例如 `/comps/main/layers/title`；不要依赖"当前选中"。
- **颜色**用 0–1 的浮点 RGBA，并注明色彩空间（默认合成工作空间）。
- **缓动参数**沿用 AE 的速度 / 影响（speed / influence）模型：`{"in": {"speed": 0, "influence": 75}, "out": {"speed": 0, "influence": 33.33}}`，影响用百分比。
- 修改类工具返回受影响的节点路径和新建节点的路径，方便后续调用。

## 9. 请 17 一并考虑

- 把本指南作为 MCP 资源（resources）暴露，例如 `bae://agent-guide/03-animation`，让没有安装 Skill 的 Agent 也能按需读取。
- `stage.describe` 的输出里直接带上：合成帧率、时长、工作区域，当前编辑目标，自己的权限级别，锁提示。这几项是指南要求 Agent 每次先看的内容。
- `render.contact_sheet` 在每格角上标出帧号，方便 Agent 对照时间表。
