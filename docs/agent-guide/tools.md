# 工具名与参数约定

本页列出指南用到的全部 MCP 工具，以及它们现在的状态。来源：

- 17 `agent/api`（#20、#29 已合并）：`crates/bae-agent`，服务器 `bae mcp`；
- 02 `core/doc-model`：内置命令（`docs/spec/data-model.md` 第 6 节）；
- 12 `ui/motion`（#41 已合并）：`crates/bae-motion` 的动效领域命令。

## 0. 命名规则

- **MCP 工具名 = 命令 ID 把 `.` 换成 `_`**：命令 `attr.set` 的工具名是 `attr_set`，`stage.describe` 是 `stage_describe`。原因：Claude 和 OpenAI 的工具名只允许 `[a-zA-Z0-9_-]`。
- 指南正文、配方、Skill 一律写 **MCP 工具名**（下划线），这是 Agent 实际看到、实际调用的名字。
- `command_batch` 的 `command` 字段既可以写命令 ID（`attr.set`），也可以写工具名（`attr_set`）。
- MCP instructions（`mcp-instructions/en.md`、`zh.md`）按命令 ID 写（点号），服务器在后面附一段说明换算规则；17 的测试按点号名字检查。

状态列的含义：**可用** = `bae mcp` 里已经能调用；**已实现未接入** = 命令写好了，但 `bae mcp` 还没注册；**未提供** = 还没有，用"替代做法"。

## 1. 查询类（只读）

| 工具 | 用途 | 参数 | 状态 |
|---|---|---|---|
| `stage_describe` | 工程摘要：你的身份、权限、编辑目标，工程层层叠，节点树（合成的尺寸 / 帧速率 / 时长，图层种类，有意见的属性和关键帧数量、帧范围，`LOCKED` 锁标记） | `root`（默认 `/`）、`detail`（`brief` / `normal` / `full`）、`max_chars`（默认 8000） | 可用 |
| `prim_get` | 读一个节点（JSON）：类型、哪些工程层对它有意见、每个属性的值 / 关键帧（含 `key_frames` 帧号）/ 表达式、值来自哪一层以及它覆盖了哪些层、引用、子节点、所在合成的尺寸和帧速率 | `path`、`include_defaults` | 可用 |
| `prim_find` | 按类型、图层种类、名字通配查找 | `prim_type`、`kind`、`name`（`*` `?`，匹配节点名或显示名）、`under`、`limit` | 可用 |
| `schema_get` | 内置节点类型的属性名、值类型、默认值、能否做动画、枚举选项。`attr_set` / `key_set` 之前先查 | `prim_type`（省略则列出全部） | 可用 |
| `sublayer_list` | 工程层层叠（强 → 弱）、哪些是你的 / 可写 / 提案层、你的编辑目标和权限 | — | 可用 |
| `sublayer_diff` | 一个工程层自己的全部意见（不是合成结果）。默认是你的提案层：这就是用户要审核的内容 | `sublayer` | 可用 |
| `attribute_evaluate` | 某帧属性的最终值（含表达式、父级） | — | **未提供**（等 03 的求值）。替代：`prim_get` 读关键帧和静态值；表达式、父级的结果只能看渲染 |
| `selection_get` | 用户当前选中的内容 | — | **未提供**（等桌面应用接入）。替代：请用户说明是哪个合成 / 图层 |

## 2. 视觉类（消耗渲染预算）

| 工具 | 用途 | 参数 | 状态 |
|---|---|---|---|
| `render_frame` | 渲染一帧，返回 PNG | `comp`、`frame`（**帧号**，从 0 开始）、`scale`（`1` / `0.5` 默认 / `0.25`，长边最多 2048 px） | 可用 |
| `render_contact_sheet` | 多帧缩略图拼成一张图，每格左上角标帧号 | `comp`、`frames`（帧号列表）或 `start` / `end` / `step`、`columns`（默认 4）、`thumb_width`（默认 320） | 可用 |

**注意：** 04 的渲染器接入之前，渲染结果是占位图，返回结果里标 `placeholder: true`。看到占位图时，不能用它判断画面，要在汇报里说明"画面没有经过渲染检查"，并用 `prim_get` 核对数值。

## 3. 修改类：内置命令（02）

| 意图 | 工具名与主要参数 | Agent 能不能用 | 状态 |
|---|---|---|---|
| 创建节点 | `prim_create`：`path`、`prim_type`、`index` | 能 | 可用 |
| 删除节点及子树 | `prim_delete` | 只用于自己创建的节点（见下） | 可用 |
| 移动 / 重排 / 改名 | `prim_move` | 只用于自己创建的节点 | 可用 |
| 改名 | `prim_rename` | 只用于自己创建的节点 | 可用 |
| 设置静态值 | `attr_set`：`path`、`attr`、`value` | 能 | 可用 |
| 清除编辑目标层对某个属性的意见，露出弱层的值 | `attr_clear`：`path`、`attr` | 能。撤回自己的覆盖就用它 | 可用 |
| 增改关键帧 | `key_set`：`path`、`attr`、`time`（flicks）、`keyframe` | 能 | 可用 |
| 删除关键帧 | `key_remove` | 能 | 可用 |
| 设置、停用或删除表达式 | `attr_set_expression` | 能 | 可用 |
| 设置引用（预合成、引用设计资产） | `prim_set_references` | 能 | 可用 |
| 新建工程层 | `sublayer_create` | 不用 | 可用 |
| 切换编辑目标 | `sublayer_set_edit_target` | **不用**（会话开始时就是自己的提案层） | 可用 |
| 下推覆盖 / 合并工程层 | `opinion_move` / `sublayer_merge` | 不用，这是人审核时的操作 | 可用 |
| 接受 / 拒绝提案 | `proposal_accept` / `proposal_reject`（17） | **不用**，这是用户的决定 | 可用 |

**删除、移动、改名是命名空间操作。** `prim_delete`、`prim_move`、`prim_rename` 作用于层叠里**所有**在该路径上有意见的工程层——对别人定义的节点做这三个操作，就是在改别人的层。Agent 只对自己在提案层里新建（`def`）的节点使用它们；需要从画面里去掉别人的节点、或改别人节点的名字时，先问用户。（权限是"只写自己的层"时，服务器会在写入前拒绝这类调用。）

**在强层里改弱层定义的节点**时，命令会自动在编辑目标层补一个 `over`（只覆盖、不定义），再写属性。这就是 Agent 在提案层里"覆盖"别人的值的机制。

**节点名和显示名不同。** 节点名是路径的一段（字母含中文、数字、`_`、`-`）；界面上的图层名是属性 `name`，可以重复、带空格。`comp_create`、`layer_create` 的 `name` 参数同时决定两者。

## 4. 修改类：动效领域命令（12，bae-motion）

**状态：已实现未接入。** 12 已经实现并合并了这些命令，但 `bae mcp` 目前只注册了 02 的内置命令和 17 的提案命令，Agent 在工具列表里还看不到它们（已请 17 从模块注册表收集）。接入之前的替代做法见本节末尾。

| 意图 | 工具名 | 主要参数 | 状态 |
|---|---|---|---|
| 新建合成（同时建 `layers` 文件夹，默认放在 `/comps`） | `comp_create` | `name`、`width`、`height`、`fps`、`duration`（flicks，默认 10 秒）、`bg_color`（RGBA 0–1） | 已实现未接入 |
| 设置工作区域 / 修剪合成到工作区域 | `comp_set_work_area` / `comp_trim_to_work_area` | `comp`、`start`、`end`（flicks） | 已实现未接入 |
| 新建文件夹 | `folder_create` | `name`、`parent` | 已实现未接入 |
| 新建图层 | `layer_create` | `comp`、`kind`（`shape` / `text` / `solid` / `null` / `camera` / `light` / `adjustment` / `pre_comp` / `footage` / `audio`）、`name`、`source`、`index`（0 = 最上）、`color`、`size`、`text`、`font_size`、`shape`（`rect` / `ellipse` / `star`）、`position`、`start`（flicks） | 已实现未接入 |
| 调整图层顺序 / 复制 / 拆分 | `layer_reorder` / `layer_duplicate` / `layer_split` | `layers`、`index` / `time` | 已实现未接入 |
| 设置入点、出点 | `layer_trim` | `layers`、`in_point`、`out_point`（合成时间，flicks） | 已实现未接入 |
| 设置父级（默认保持屏幕位置不变） | `layer_set_parent` | `layers`、`parent`、`keep_transform`、`time` | 已实现未接入 |
| 秒表开 / 关 | `attr_set_animated` | `path`、`attr`、`animated`、`time` | 已实现未接入 |
| 在某个时间改值（有关键帧就加 / 改关键帧，没有就改静态值） | `attr_set_at` | `path`、`attr`、`time`、`value` | 已实现未接入 |
| 缓动 / 缓入 / 缓出（F9 / Shift+F9 / Ctrl+Shift+F9） | `key_ease` | `keys`（`[{path, attr, time}]`）、`mode`（`easy_ease` / `ease_in` / `ease_out`）、`influence`（默认 33.33） | 已实现未接入 |
| 关键帧插值、切换定格 | `key_set_interpolation` | `keys`、`interp_in`、`interp_out`（`linear` / `bezier` / `hold`） | 已实现未接入 |
| 移动 / 删除 / 时间反向 / 粘贴关键帧 | `key_move` / `key_delete` / `key_reverse` / `key_paste` | `keys`、`delta`（flicks）…… | 已实现未接入 |
| 预合成 | `layer_precompose` | — | **未提供**（12 下一阶段） |
| 添加合成标记 / 图层标记 | `marker_add` | — | **未提供**（12 下一阶段） |
| 给文本图层添加动画制作工具 | `text_add_animator` | — | **未提供**（12 下一阶段） |

`key_ease` 只设影响、速度固定为 0，正好覆盖指南的缓动预设（见 [03 动画规范](03-animation.md) 2.4）。`key_delete` 和 02 的 `key_remove` 的区别：删光关键帧时 `key_delete` 会关掉秒表并保留当前值。

**接入之前的替代做法：**

- 新建合成：`prim_create`（`prim_type: "Comp"`）+ 在其下 `prim_create` 一个 `layers` 文件夹（`Folder`）+ `attr_set` 设 `name`、`width`、`height`、`frame_rate`、`duration` 等（属性名和类型用 `schema_get` 查 `Comp`）。
- 新建图层：`prim_create`（`prim_type: "Layer"`，路径 `<合成>/layers/<名字>`）+ `attr_set` 设 `kind`、`name` 等（用 `schema_get` 查 `Layer`）。
- 缓动：直接在 `key_set` 的 `keyframe` 里写 `interp_in` / `interp_out: "bezier"` 和 `ease_in` / `ease_out`（见第 7 节）。
- 标记、预合成、动画制作工具：暂时不做，在汇报里说明。

## 5. 事务：`command_batch`

17 没有提供 `transaction.begin` / `commit` / `abort`，改为一次调用提交一批命令：

```json
{"tool": "command_batch", "args": {
  "label": "标题出场：入场动画与错帧",
  "commands": [
    {"command": "key_set", "params": {"...": "..."}},
    {"command": "key_ease", "params": {"...": "..."}}
  ]}}
```

- 一次 `command_batch` = 一个撤销单元 = 用户审核时的一条修改，`label` 进历史和审计。
- **全部成功或全部不生效**：任何一条命令失败或无权限，整批都不应用，不需要"回滚"。
- 指南里说的"一个事务"就是一次 `command_batch` 调用（单独调用一个修改类工具也算一个事务）。

## 6. 工程层、历史与提案

| 意图 | 工具名 | 状态 |
|---|---|---|
| 列出工程层、编辑目标、权限 | `sublayer_list` | 可用 |
| 查看自己提案层里的全部改动 | `sublayer_diff` | 可用 |
| 撤销 / 重做自己的上一步 | `history_undo` / `history_redo` | 可用 |
| 列出历史 | `history_list` | **未提供**。替代：`sublayer_diff` 看自己的全部改动 |
| 填写提案说明 | `proposal_describe` | **未提供**。替代：把提案说明（模板见 [05 协作礼仪](05-collaboration.md)）写在给用户的回复里 |
| 在节点上留批注 | `comment_add` | **未提供**。替代：在回复里写批注内容（节点路径 +【建议】【原因】【建议值】【影响】），请用户转达或决定 |

## 7. 参数格式

### 时间

- **命令的时间参数是 flicks**（1 秒 = 705,600,000）：`key_set` 的 `time`、`comp_create` 的 `duration`、`layer_create` 的 `start`、`layer_trim` 的入点出点、`key_move` 的 `delta` 等。换算：`flicks = 帧号 × 每帧 flicks`，每帧 flicks 见 [01 概念与术语](01-concepts.md) 第 5 节（30 fps 是 23,520,000，25 fps 是 28,224,000，24 fps 是 29,400,000）。
- **渲染工具的参数是帧号**（`render_frame` 的 `frame`、`render_contact_sheet` 的 `frames` / `start` / `end`）；`prim_get` 返回的 `key_frames` 也是帧号。
- 图层属性的关键帧时间是**图层时间**；图层的 `start_time` 为 0 时（默认）图层时间就等于合成时间。没有特别需要时不要改 `start_time`。
- 17 正在考虑在 MCP 层统一接受帧号；定下来后本节和所有配方会更新。

### 值

02 的值是带类型标签的 JSON：

```json
{"type": "float", "value": 100.0}
{"type": "vec3", "value": [960.0, 510.0, 0.0]}
{"type": "color", "value": [1.0, 1.0, 1.0, 1.0]}
```

图层的 `transform:position`、`transform:anchor_point`、`transform:scale` 是 **vec3**（2D 图层 z 写 0；缩放 z 写 100）；`transform:opacity`、`transform:rotation` 是 float。形状内容里的变换是 vec2。不确定时用 `schema_get`。

### 关键帧

```json
{"value": {"type": "vec3", "value": [960.0, 510.0, 0.0]},
 "interp_in": "bezier", "interp_out": "bezier",
 "ease_in": [{"speed": 0.0, "influence": 80.0}],
 "ease_out": [{"speed": 0.0, "influence": 33.333}]}
```

- **插值默认是 `linear`。** 只写 `ease_in` / `ease_out` 而不把对应一侧设成 `bezier`，缓动不会生效。
- 缓动每个维度一组：位置等空间属性一组；缩放等非空间多维属性按维度各一组。
- 推荐做法：`key_set` 只写值（线性），再用 `key_ease` 按预设设缓动——`key_ease` 自动设 Bezier、速度 0，并按维度展开。两步放在同一个 `command_batch` 里。

### 常用属性名

| 属性 | 说明 |
|---|---|
| `name` | 显示名 |
| `transform:position` / `transform:scale` / `transform:rotation` / `transform:opacity` / `transform:anchor_point` | 图层变换 |
| `start_time` / `in_point` / `out_point` | 图层时间（合成时间，flicks） |
| `parent` | 父级 |
| `motion_blur` | 图层的运动模糊开关 |
| `track_matte` / `track_matte_layer` | 轨道遮罩模式（`alpha` / `luma` / …）和遮罩图层 |
| `text:source` | 文本内容（TextDocument） |

完整列表用 `schema_get`。

## 8. 分镜领域命令（13，bae-storyboard）

**未提供**（13 现阶段只做框架）。名字按意图推测：`board_create`、`panel_create`、`panel_update`、`board_make_animatic`、`panel_promote_to_comp`。

## 9. 交接与打包

| 意图 | 工具名 | 状态 |
|---|---|---|
| 导出给 AE（`.jsx` 重建脚本 + 素材 + 导出报告） | `interop_export_ae` | **未提供**（15，推测名） |
| 扫描、打包、收集、解包素材，重新链接 | `package_scan` / `package_pack` / `package_collect` / `package_unpack` / `asset_relink` / `asset_set_files` | 已实现未接入（21，已请 17 注册） |

## 10. 还请 17 考虑

- 从模块注册表收集 12、21 等领域命令，让 `comp_create`、`layer_create`、`key_ease`、`package_*` 出现在 `bae mcp` 里。
- 时间参数在 MCP 层接受帧号（或时码），由服务器换算 flicks。
- `attribute_evaluate`、`proposal_describe`、`comment_add`、`history_list`、`selection_get`。
- 把本指南作为 MCP 资源暴露（如 `bae://agent-guide/03-animation`）。
