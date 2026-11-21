# Illustrator 桥接格式（`bae-ai-bridge` 0.1）

负责：16 `interop/ai`。写入方：在 Illustrator 里运行的 [`bae-ai-export.jsx`](bae-ai-export.jsx)。读取方：`crates/bae-interop-ai` 的 `read_bridge` / `read_document`。真机样例见 `crates/bae-interop-ai/tests/data/illustrator/*.bae-ai.json`（Illustrator 30.4 导出）。

## 1. 原则

- **原样导出。** 脚本只把 Illustrator 脚本 DOM 能读到的内容换成统一的坐标和单位，不做语义映射。语义映射全部在 Rust 里做，包括画板分配、外观顺序、渐变中点、高光换算、文字降级，这样能写单元测试，ExtendScript 越薄越好（与 15 的 AE 桥接同一原则）。
- **一个坐标空间。** 文档空间，单位 pt，x 向右，**y 向下**（Illustrator 脚本坐标的 y 取反）。画板矩形、路径点、渐变端点、文字锚点、`bounds` 都在这个空间里，读取方减去画板左上角就得到画板空间。原点是 Illustrator 的文档坐标原点，没有固定含义，只有相对位置有意义。
- **颜色**统一成 0–1 的直通 RGBA（经 Illustrator 当前颜色设置 `app.convertSampleColor` 换算），原始色彩模型和数值写在 `source` 里。
- **顺序**与图层面板一致：下标 0 在最上面。
- 数字保留 4 位小数。读取方忽略不认识的字段；`format_version` 的主、次版本号不认识时拒绝读取（0.x 阶段次版本号变化可能不兼容）。
- 文件名建议 `<文档名>.bae-ai.json`，UTF-8，无 BOM。

## 2. 顶层

| 字段 | 类型 | 说明 |
|---|---|---|
| `format` | `"bae-ai-bridge"` | 固定值 |
| `format_version` | string | 本文是 `0.1.0` |
| `generator` | object | `{name: "bae-ai-export", version}` |
| `host` | object | `{app, version, os, locale}`，取自 `app.name`、`app.version`、`$.os`、`app.locale` |
| `exported_at` | string，可无 | ISO 8601 UTC；`stable` 选项下不写 |
| `source` | object | `{name, path?}`：文件名和绝对路径（未保存或 `stable` 时没有 `path`） |
| `document` | object | `{name, color_space: "rgb"\|"cmyk", units: "pt", y_axis: "down"}`，`name` 是去掉扩展名的文件名 |
| `artboards` | Artboard[] | Illustrator 的画板顺序 |
| `layers` | Layer[] | 顶层图层，最上面的在前 |
| `report` | object | `{warnings: string[]}`：导出时发现的问题，读取方原样转进导入报告 |

**Artboard**：`{name, rect: [left, top, right, bottom]}`（y 向下，所以 `top < bottom`）。

## 3. 图层与对象

每个对象都有 `type`。除图层外，对象都有这些公共字段：

| 字段 | 说明 |
|---|---|
| `name` | 对象名；Illustrator 里没起名时不写 |
| `hidden`、`locked` | 布尔 |
| `opacity` | 0–1 |
| `blend` | 混合模式 token：`normal`、`multiply`、`screen`、`overlay`、`soft_light`、`hard_light`、`color_dodge`、`color_burn`、`darken`、`lighten`、`difference`、`exclusion`、`hue`、`saturation`、`color`、`luminosity`（与 bae-model 的 token 一致） |
| `bounds` | Illustrator 的 `visibleBounds`（含描边），`[left, top, right, bottom]`。**剪切组的 bounds 包含被剪掉的内容**（Illustrator 就是这样报的），读取方用剪切路径定位 |

| `type` | 来源 | 专有字段 |
|---|---|---|
| `layer` | 图层、子图层 | `name`、`visible`、`locked`、`printable`、`preview`、`opacity`、`blend`、`color`（图层颜色 RGB）、`items`。没有公共字段 |
| `group` | 编组、剪切组、展开后的符号 | `clipped`、`clip`（剪切路径的子路径数组，已从 `items` 里拿掉）、`symbol`（符号实例时是符号名）、`items` |
| `path` | 路径、复合路径 | `compound`、`subpaths`、`fill_rule`（`non_zero` / `even_odd`）、`fill`（Paint 或 null）、`stroke`（Stroke 或 null） |
| `text` | 文字 | 见第 5 节 |
| `image` | 嵌入或链接的位图 | `image_kind`（`raster` / `placed`）、`embedded`、`file`、`matrix` |
| `other` | 网格、图表、实时效果插件对象等 | `item_type`：Illustrator 的 typename |

子图层和对象在同一个 `items` 里按图层面板的顺序交错排列（用 `absoluteZOrderPosition` 排序）。参考线（`guides`）不导出。

**Subpath**：`{closed, points: [[x, y, in_x, in_y, out_x, out_y], …]}`，控制柄是绝对坐标：`in` 是 Illustrator 的 `leftDirection`（进入这个锚点的那段曲线的控制点），`out` 是 `rightDirection`。复合路径的子路径按 Illustrator 的顺序（最上面的在前），绕向原样保留，非零规则下的洞靠绕向表达。

## 4. 绘画

**Paint**（填充或描边的颜色）：

- `{"type": "color", "color": [r, g, b, 1], "source": {...}}`。`source.model` 是 `rgb`（0–255）、`cmyk`（0–100）、`gray`（0–100，100 是黑）、`lab`、`spot`（另有 `spot` 名、`tint` 百分比和 `base` 原色）。专色按色调和白色混合。
- `{"type": "gradient", …}`：

| 字段 | 说明 |
|---|---|
| `gradient` | `linear` / `radial`（任意形状渐变等其他类型原样写出，读取方按线性处理并警告） |
| `name` | 色板名 |
| `stops` | `[{offset, midpoint, color: [r, g, b, a], source}]`。`offset`、`midpoint` 是 0–1；`a` 是色标不透明度。**`midpoint` 属于这个色标和下一个色标之间那一段**（实测：最后一个色标的中点不起作用） |
| `start`、`end` | 渐变向量的起点和终点（文档空间）。径向渐变时 `start` 是圆心，`end` 是半径方向上的圆周点 |
| `focal` | 只有径向渐变有：高光（焦点）位置 |
| `matrix` | 渐变空间 → 文档空间，`[a, b, c, d, tx, ty]`，y 向下。线性部分不是相似变换时，径向渐变是椭圆 |
| `raw` | Illustrator 原值（y 向上）：`origin`、`angle`、`length`、`hilite_angle`、`hilite_length`、`matrix` |

换算（用 Illustrator 自己导出的 SVG 核对过）：`start = M·origin`，`end = M·(origin + length·(cos angle, sin angle))`，`focal = M·(origin + hilite_length·length·(cos −hilite_angle, sin −hilite_angle))`，`M` 是 `raw.matrix`，结果再把 y 取反。

- `{"type": "pattern", "name"}`：图案，只记名字。

**Stroke**：`{paint, width, cap: "butt"|"round"|"projecting", join: "miter"|"round"|"bevel", miter_limit, dashes: [...], dash_offset}`。

## 5. 文字

| 字段 | 说明 |
|---|---|
| `text_kind` | `point`、`area`、`path` |
| `contents` | 全文，段落之间是 `\r`（Illustrator 和 AE 的习惯） |
| `anchor` | 点文字第一行基线的起点。区域文字和路径文字没有这个字段 |
| `frame` | 区域文字的文本框、路径文字所沿的路径（Subpath） |
| `matrix` | 文字的线性变换 `[a, b, c, d]`（y 向下）：旋转、缩放、倾斜 |
| `orientation` | `horizontal` / `vertical` |
| `paragraphs` | `[{justification}]`：`left`、`center`、`right`、`justify_left`、`justify_center`、`justify_right`、`justify_all` |
| `runs` | 样式相同的连续字符合并成一段：`{text, font（PostScript 名）, family, style, size, leading, auto_leading, tracking, baseline_shift, h_scale, v_scale, fill, stroke, stroke_width}` |
| `outline` | `group`：这段文字转成轮廓后的形状（导出时在副本上做 `createOutline()`），`textOutlines: false` 时不写 |

## 6. 读取规则（`bae-interop-ai`）

- 每个画板得到与它相交的顶层对象（子图层里的对象逐个判断），坐标减去画板左上角。一个对象跨两个画板时两边都有，和 PDF 的每页内容一致。不在任何画板上的对象不导入，并给出警告。
- 在某个画板上没有内容的图层，在这个画板上不建形状图层。
- 子图层 → 带名字的编组。图层不透明度或混合模式不是默认值时，把图层内容包进一个同名编组。不打印的图层（模板图层）导入后隐藏。
- 路径的绘画顺序是「描边在上、填充在下」（Illustrator 基本外观的顺序）。
- 渐变中点不在 50% 时，在中点位置插一个两色平均的色标（BAE 的渐变是分段线性的，只能近似 Illustrator 的中点曲线）。焦点换算成 AE 的高光长度（半径的百分比）和高光角度（相对起点→终点方向，顺时针）。椭圆形径向渐变按圆形导入并警告。
- 文字：点文字取第一段样式，位置用 `anchor`，旋转和缩放取自 `matrix` 和字符的水平/垂直缩放。区域文字和路径文字按点文字放在文本框左上角，并给出警告。`BridgeOptions { text_outlines: true }` 时改用 `outline`，得到精确字形，但不能再当文字编辑。
- `image`、`other`、图案只给警告，不导入。

## 7. 已知限制（Illustrator 脚本 DOM 本身的限制）

| 限制 | 影响 | 以后怎么补 |
|---|---|---|
| 外观面板的多重填充 / 描边、实时效果、不透明蒙版都读不到 | 只导出基本外观：一个填充、一个描边 | 读 .ai 内嵌的 PDF（16 的 PDF 读取器），或在 Ai 里先「扩展外观」 |
| `GradientColor.origin` 被截成整数 | 渐变位置最多偏约 1–1.5 pt（再乘以渐变矩阵的缩放）。实测 `paint.ai` 的旋转线性渐变偏 1.23 pt | PDF 读取没有这个问题；也可以以后从 Illustrator 的 SVG 导出里取精确值 |
| 用脚本新建的渐变不能直接设 origin / angle / length（会被忽略） | 只影响 BAE → Ai 方向：重建时要用 `transform()` 只变换渐变 | — |
| 渐变描边的「沿描边 / 跨描边」读不到 | 按「在描边内」处理 | PDF 读取 |
| 高光角度的方向只在渐变角度为 0 时验证过（SVG 导入的渐变都是 0） | 极少数用渐变工具手动拖出高光的渐变，焦点方向可能需要再核对 | — |
| 文字逐字符读取样式，超过 4000 个字符时只取第一个字符的样式 | 长文的样式变化丢失（导入时本来也只用第一段样式） | — |

## 8. 版本

- 0.1.0（2026-11-20）：第一版，Illustrator 30.4（macOS）实测。
