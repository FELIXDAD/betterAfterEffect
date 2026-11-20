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

### 事务 1：建立合成和版式

```json
{"tool": "stage.describe", "args": {"detail": "normal"}}
{"tool": "transaction.begin", "args": {"label": "标题出场：建立合成和版式"}}
{"tool": "comp.create", "args": {"name": "SH010_title", "width": 1920, "height": 1080, "fps": 30, "duration_frames": 90, "background": [0.06, 0.06, 0.07, 1]}}
{"tool": "layer.create", "args": {"comp": "/comps/SH010_title", "kind": "text", "name": "title_main", "text": "主标题文字", "font_size": 96, "align": "center", "position": [960, 510]}}
{"tool": "layer.create", "args": {"comp": "/comps/SH010_title", "kind": "text", "name": "title_sub", "text": "副标题文字", "font_size": 40, "align": "center", "position": [960, 590]}}
{"tool": "layer.create", "args": {"comp": "/comps/SH010_title", "kind": "shape", "name": "line_accent", "shape": {"rect": [240, 4]}, "fill": [1, 1, 1, 1], "position": [960, 640]}}
{"tool": "transaction.commit"}
```

自检：`render.frame` 第 60 帧，`scale: 0.5`。看文字是否居中、不重叠、在标题安全区内。

### 事务 2：入场动画与错帧

```json
{"tool": "transaction.begin", "args": {"label": "标题出场：入场动画与错帧"}}
{"tool": "keyframe.set", "args": {"path": "/comps/SH010_title/layers/title_main", "attribute": "transform.position", "spatial": "linear", "keyframes": [
  {"frame": 4, "value": [960, 550], "out": {"speed": 0, "influence": 15}},
  {"frame": 22, "value": [960, 510], "in": {"speed": 0, "influence": 80}}]}}
{"tool": "keyframe.set", "args": {"path": "/comps/SH010_title/layers/title_main", "attribute": "transform.opacity", "interpolation": "linear", "keyframes": [
  {"frame": 4, "value": 0}, {"frame": 12, "value": 100}]}}
{"tool": "keyframe.set", "args": {"path": "/comps/SH010_title/layers/title_sub", "attribute": "transform.position", "spatial": "linear", "keyframes": [
  {"frame": 10, "value": [960, 620], "out": {"speed": 0, "influence": 15}},
  {"frame": 28, "value": [960, 590], "in": {"speed": 0, "influence": 80}}]}}
{"tool": "keyframe.set", "args": {"path": "/comps/SH010_title/layers/title_sub", "attribute": "transform.opacity", "interpolation": "linear", "keyframes": [
  {"frame": 10, "value": 0}, {"frame": 18, "value": 100}]}}
{"tool": "keyframe.set", "args": {"path": "/comps/SH010_title/layers/line_accent", "attribute": "transform.scale", "keyframes": [
  {"frame": 16, "value": [0, 100], "out": {"speed": 0, "influence": 15}},
  {"frame": 32, "value": [100, 100], "in": {"speed": 0, "influence": 80}}]}}
{"tool": "transaction.commit"}
```

自检：`render.contact_sheet`，第 0–36 帧，`step: 2`，`columns: 8`，`scale: 0.25`。应该看到：前 2 格空；主标题出现时前几格移动大、后几格移动小；副标题晚一点出现；装饰线最后从中间展开。

### 事务 3：标记

```json
{"tool": "transaction.begin", "args": {"label": "标题出场：段落标记"}}
{"tool": "marker.add", "args": {"comp": "/comps/SH010_title", "frame": 4, "name": "in"}}
{"tool": "marker.add", "args": {"comp": "/comps/SH010_title", "frame": 32, "name": "hold"}}
{"tool": "transaction.commit"}
```

## 5. 最终自检

1. `render.frame`：第 0、22、32、89 帧，`scale: 0.5`。
2. `attribute.evaluate`：

```json
{"tool": "attribute.evaluate", "args": {"path": "/comps/SH010_title/layers/title_main", "attribute": "transform.opacity", "frames": [3, 4, 12, 89]}}
```

期望：第 3 帧 0（不可见）、第 4 帧 0、第 12 帧 100、第 89 帧 100；位置在第 22 帧以后保持 (960, 510)。

3. 按 [质量检查清单](../06-quality.md) 过一遍，写提案说明（[模板](../05-collaboration.md)）。

## 6. 变体

- **遮罩升起（更有设计感）：** 加一个形状图层 `matte_title` 盖住主标题所在区域（静止不动），把 `title_main` 的轨道遮罩设为 `matte_title` 的 Alpha。位移改成字高的 60–100%（约 70 px），不透明度可以不动。文字会像从一条看不见的线后面升起。
- **需要出场：** 合成延长到 120 帧（4 秒）或压缩停留段。出场在第 104–115 帧：所有元素同时（或按入场顺序间隔 2 帧）向上 20 px 并淡出，出场·强，12 帧；最后几帧是空画面。
- **逐字入场：** 主标题用动画制作工具（位置 Y +40、不透明度 0），范围选择器偏移 0% → 100%，每字约 1.5 帧；字数超过 12 个时改为整体或逐行。
- **24 / 25 fps：** 帧号按比例换算（24 fps：主标题 3–18 帧，副标题 8–22 帧，装饰线 13–26 帧，合成 72 帧）。

## 7. 常见坑

- 把"缓入"设在第一个关键帧上，结果入场是加速撞停（见 [03 动画规范](../03-animation.md) 2.3）。
- 主标题和副标题同时出现，没有层级。
- 不透明度和位置同时结束，元素在半透明状态下落位，显得发虚。
- 装饰线锚点在左端，展开方向和居中的版式不一致。
- 文本图层命名成文案内容（`2026 年度发布会`），改文案后名字就错了。
