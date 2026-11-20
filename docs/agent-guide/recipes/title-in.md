# 配方：标题出场

一个主标题加一个副标题和一条装饰线，3 秒内入场并停留。这也是 17 端到端示例的标准任务（"做一个 3 秒的标题出场动画并渲染预览"）。

## 1. 输入

| 信息 | 没有时怎么办 |
|---|---|
| 主标题、副标题文案 | **问用户**；用户让先做就用占位 `主标题文字` / `副标题文字`，并在提案里列出 |
| 合成尺寸、帧速率 | 沿用工程里已有合成；没有就 1920×1080、30 fps |
| 字体、颜色 | 沿用工程已有；没有就系统字体、深色背景浅色字 |
| 是否需要出场 | 默认不需要（只到停留）；需要时见"变体" |

## 2. 结构

```
SH010_title  1920×1080, 30 fps, 90 帧（0–89）
├─ line_accent   形状图层：240×4 px 矩形，锚点在中心，位置 (960, 640)
├─ title_sub     文本图层：40 px，居中，位置 (960, 590)
├─ title_main    文本图层：96 px，居中，位置 (960, 510)
└─ bg_solid      纯色图层（或用合成背景颜色）
```

没有镜头号时合成叫 `title`。

## 3. 时间表（30 fps）

| 元素 | 属性 | 起始帧 → 值 | 结束帧 → 值 | 缓动 |
|---|---|---|---|---|
| `title_main` | 位置 | 4 → (960, 550) | 22 → (960, 510) | 入场·强：起点 out 影响 15%，终点 in 影响 80%，速度都为 0 |
| `title_main` | 不透明度 | 4 → 0 | 12 → 100 | 线性 |
| `title_sub` | 位置 | 10 → (960, 620) | 28 → (960, 590) | 入场·强 |
| `title_sub` | 不透明度 | 10 → 0 | 18 → 100 | 线性 |
| `line_accent` | 缩放 | 16 → (0, 100) | 32 → (100, 100) | 入场·强 |
| 全部 | — | 32 | 89 | 停留 58 帧（约 1.9 秒） |

检查：主标题先动（第 4 帧），副标题晚 6 帧，装饰线最后；主标题和副标题的运动各 18 帧（"单行主标题入场"范围的下限，显得利落），装饰线 16 帧（"中等元素移动"范围）；不透明度在位置运动的前一半完成；开头留 4 帧空画面；所有动作在第 32 帧结束。

## 4. 步骤

> 本节用 12 的 `comp_create`、`layer_create`、`key_ease`。如果它们还不在工具列表里（见 [tools.md](../tools.md) 第 4 节），按那里的替代做法用 `prim_create` / `attr_set` / `key_set` 完成同样的结构。

30 fps 下每帧 23,520,000 flicks。本配方用到的时间：

| 帧 | 4 | 10 | 12 | 16 | 18 | 22 | 28 | 32 | 90（时长） |
|---|---|---|---|---|---|---|---|---|---|
| flicks | 94,080,000 | 235,200,000 | 282,240,000 | 376,320,000 | 423,360,000 | 517,440,000 | 658,560,000 | 752,640,000 | 2,116,800,000 |

### 事务 1：建立合成和版式

```json
{"tool": "stage_describe", "args": {"detail": "normal"}}
{"tool": "command_batch", "args": {"label": "标题出场：建立合成和版式", "commands": [
  {"command": "comp_create", "params": {"name": "SH010_title", "width": 1920, "height": 1080, "fps": 30,
    "duration": 2116800000, "bg_color": [0.06, 0.06, 0.07, 1]}},
  {"command": "layer_create", "params": {"comp": "/comps/SH010_title", "kind": "text", "name": "title_main",
    "text": "主标题文字", "font_size": 96, "position": [960, 510]}},
  {"command": "layer_create", "params": {"comp": "/comps/SH010_title", "kind": "text", "name": "title_sub",
    "text": "副标题文字", "font_size": 40, "position": [960, 590]}},
  {"command": "layer_create", "params": {"comp": "/comps/SH010_title", "kind": "shape", "name": "line_accent",
    "shape": "rect", "color": [1, 1, 1, 1], "position": [960, 640]}},
  {"command": "attr_set", "params": {"path": "/comps/SH010_title/layers/line_accent/contents/Rectangle_1/contents/Rectangle_Path_1",
    "attr": "size", "value": {"type": "vec2", "value": [240, 4]}}}
]}}
```

- `comp_create`、`layer_create` 的返回值里有新节点的路径；后续调用以返回的路径为准。
- `layer_create` 没有文字对齐参数。居中对齐要改 `text:source` 里的段落对齐（格式先用 `prim_get` 看一个文本图层），或者渲染后检查位置再调整。
- 形状预设的内部路径按 AE 结构生成（内容 > 矩形 1 > 内容 > 矩形路径 1），不确定时用 `prim_find` 查 `under` 这个图层。

自检：`render_frame` 第 60 帧，`scale: 0.5`。看文字是否居中、不重叠、在标题安全区内。

### 事务 2：入场动画与错帧

先用 `key_set` 放线性关键帧，再用 `key_ease` 按"入场·强"设缓动（起点缓出 15%，终点缓入 80%）。以 `title_main` 为例：

```json
{"tool": "command_batch", "args": {"label": "标题出场：入场动画与错帧", "commands": [
  {"command": "key_set", "params": {"path": "/comps/SH010_title/layers/title_main", "attr": "transform:position",
    "time": 94080000, "keyframe": {"value": {"type": "vec3", "value": [960, 550, 0]}}}},
  {"command": "key_set", "params": {"path": "/comps/SH010_title/layers/title_main", "attr": "transform:position",
    "time": 517440000, "keyframe": {"value": {"type": "vec3", "value": [960, 510, 0]}}}},
  {"command": "key_ease", "params": {"mode": "ease_out", "influence": 15, "keys": [
    {"path": "/comps/SH010_title/layers/title_main", "attr": "transform:position", "time": 94080000}]}},
  {"command": "key_ease", "params": {"mode": "ease_in", "influence": 80, "keys": [
    {"path": "/comps/SH010_title/layers/title_main", "attr": "transform:position", "time": 517440000}]}},
  {"command": "key_set", "params": {"path": "/comps/SH010_title/layers/title_main", "attr": "transform:opacity",
    "time": 94080000, "keyframe": {"value": {"type": "float", "value": 0}}}},
  {"command": "key_set", "params": {"path": "/comps/SH010_title/layers/title_main", "attr": "transform:opacity",
    "time": 282240000, "keyframe": {"value": {"type": "float", "value": 100}}}}
]}}
```

同一个 `command_batch` 里接着放 `title_sub` 和 `line_accent`：

| 图层 | 属性 | 起点（flicks → 值） | 终点（flicks → 值） | 缓动 |
|---|---|---|---|---|
| `title_sub` | `transform:position` | 235,200,000 → [960, 620, 0] | 658,560,000 → [960, 590, 0] | 起点 `ease_out` 15，终点 `ease_in` 80 |
| `title_sub` | `transform:opacity` | 235,200,000 → 0 | 423,360,000 → 100 | 线性（不调 `key_ease`） |
| `line_accent` | `transform:scale` | 376,320,000 → [0, 100, 100] | 752,640,000 → [100, 100, 100] | 起点 `ease_out` 15，终点 `ease_in` 80 |

两个关键帧的位置动画是直线，不需要另外设空间插值。

自检：`render_contact_sheet`，`start: 0`、`end: 36`、`step: 2`、`columns: 8`、`thumb_width: 240`。应该看到：前 2 格空；主标题出现时前几格移动大、后几格移动小；副标题晚一点出现；装饰线最后从中间展开。

### 事务 3：标记

`marker_add` 还没有（12 下一阶段）。有了以后加两个合成标记：`in`（第 4 帧）、`hold`（第 32 帧）；在那之前跳过，并在汇报里写明。

## 5. 最终自检

1. `render_frame`：第 0、22、32、89 帧，`scale: 0.5`。结果标 `placeholder: true` 时说明画面没有经过渲染检查。
2. `prim_get` 读 `/comps/SH010_title/layers/title_main`，核对：位置关键帧在第 4、22 帧（`key_frames`），值 [960, 550, 0] → [960, 510, 0]，第 4 帧 `interp_out` 和第 22 帧 `interp_in` 都是 `bezier`，影响分别是 15 和 80；不透明度关键帧在第 4、12 帧，0 → 100。`title_sub`、`line_accent` 同样核对。
3. `sublayer_diff` 确认所有改动都在自己的提案层，没有多余的东西。
4. 按 [质量检查清单](../06-quality.md) 过一遍，在回复里写提案说明（[模板](../05-collaboration.md)）。

## 6. 变体

- **遮罩升起（更有设计感）：** 加一个形状图层 `matte_title` 盖住主标题所在区域（静止不动），把 `title_main` 的轨道遮罩设为 `matte_title` 的 Alpha。位移改成字高的 60–100%（约 70 px），不透明度可以不动。文字会像从一条看不见的线后面升起。
- **需要出场：** 合成延长到 120 帧（4 秒）或压缩停留段。出场在第 104–115 帧：所有元素同时（或按入场顺序间隔 2 帧）向上 20 px 并淡出，出场·强，12 帧；最后几帧是空画面。
- **逐字入场：** 主标题用动画制作工具（位置 Y +40、不透明度 0），范围选择器偏移 0% → 100%，每字约 1.5 帧；字数超过 12 个时改为整体或逐行。
- **24 / 25 fps：** 帧号按比例换算（24 fps：主标题 3–18 帧，副标题 8–22 帧，装饰线 13–26 帧，合成 72 帧），再乘以该帧速率的每帧 flicks（24 fps 是 29,400,000）。

## 7. 常见坑

- 把"缓入"设在第一个关键帧上，结果入场是加速撞停（见 [03 动画规范](../03-animation.md) 2.3）。
- 主标题和副标题同时出现，没有层级。
- 不透明度和位置同时结束，元素在半透明状态下落位，显得发虚。
- 装饰线锚点在左端，展开方向和居中的版式不一致。
- 文本图层命名成文案内容（`2026 年度发布会`），改文案后名字就错了。
