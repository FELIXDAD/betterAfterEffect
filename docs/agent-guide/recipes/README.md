# 配方（recipes）

配方是常见任务的标准做法：结构、时间表、调用步骤、自检要点。按配方做能稳定得到合格的结果；需求不同的地方按 [03 动画规范](../03-animation.md) 调整，并在提案说明里写出改了什么。

> 配方里的工具名、属性名、参数格式以 [tools.md](../tools.md) 为准；还没提供的工具见下面"调用示例里的约定"。

| 配方 | 典型时长 | 文件 |
|---|---|---|
| 标题出场 | 3 秒 | [title-in.md](title-in.md) |
| 下三分之一字幕 | 5–7 秒 | [lower-third.md](lower-third.md) |
| Logo 演绎 | 3–6 秒 | [logo-reveal.md](logo-reveal.md) |
| 数据图表动画 | 4–8 秒 | [data-chart.md](data-chart.md) |
| UI 动效 | 视交互步骤 | [ui-motion.md](ui-motion.md) |
| 分镜生成动态分镜 | 视分镜表 | [animatic.md](animatic.md) |
| 交接给 AE 用户 | — | [handoff-ae.md](handoff-ae.md) |

## 配方的统一格式

1. **输入**：开始前要确认的信息；哪些必须问用户。
2. **结构**：合成和图层树。
3. **时间表**：默认按 30 fps 写帧号。其他帧速率按"帧号 × 目标帧速率 ÷ 30"换算后取整（24 fps 乘 0.8，25 fps 乘 5/6，60 fps 乘 2）。
4. **步骤**：按事务分组的调用顺序。
5. **自检**：要渲染哪些帧、核对哪些数值。
6. **变体**和**常见坑**。

## 调用示例里的约定

示例里的 JSON 只写关键参数，`"...": "..."` 表示省略。工具名、参数格式以 [tools.md](../tools.md) 为准：

- 一个事务 = 一次 `command_batch`；
- 命令的时间参数是 **flicks**（`帧号 × 每帧 flicks`，30 fps 每帧 23,520,000），渲染工具用**帧号**；
- 值带类型标签：`{"type": "vec3", "value": [960, 510, 0]}`；图层的位置、缩放是 vec3；
- 缓动：`key_set` 放线性关键帧，再用 `key_ease`（`ease_out` 用在起点、`ease_in` 用在终点，`influence` 按预设）。

配方的时间表为了好读写成帧号和 2D 坐标，调用时按上面的规则换算。完整的调用示例见 [标题出场](title-in.md) 第 4 节。

**还没有的工具**（见 tools.md）：`marker_add`、`layer_precompose`、`text_add_animator`、`attribute_evaluate`、`proposal_describe`、`comment_add`，以及 13、15 的分镜和导出命令。配方里用到它们的步骤先跳过或用替代做法，并在汇报里写明。12 的 `comp_create`、`layer_create`、`key_ease` 等已实现，但接入 `bae mcp` 之前要用 `prim_create` / `attr_set` / `key_set` 代替。
