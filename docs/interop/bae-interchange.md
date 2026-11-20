# BAE 交换格式（草案 v0.2）

负责：15 `interop/ae`。机器可读的定义见同目录的 [`bae-interchange.schema.json`](bae-interchange.schema.json)（JSON Schema 2020-12）。

状态：**草案 v0.2**，已对齐 02 的 Prim schema v1（`crates/bae-model/src/schema.rs`、`docs/spec/data-model.md` 第 10、11 节）。导出结果能直接用 `bae-model` 的 serde 类型读入，02 schema 认识的属性都通过 `schema::check_value`（见第 11 节）。导出脚本已在 AE 26.2.1 上实测（`bridges/ae/README.md`）。

## 1. 用途

AE 工程进出 BAE 的唯一中间格式：

```
AE 工程 ──bridges/ae/bae-export.jsx──► 交换文件（.bae-interchange.json + 收集的素材）──bae-interop-ae──► 命令批次 ──► BAE Stage
BAE Stage ──bae-interop-ae──► 交换文件 ──bridges/ae/bae-import.jsx（以后做）──► AE 工程
```

- AE 一侧的脚本（ExtendScript）只做「原样导出」和「原样重建」，不做语义映射。
- 语义映射（AE matchName ↔ BAE 属性、效果对照、降级、导入报告）全部在 Rust 的 `bae-interop-ae` 里做。这样映射逻辑只有一份，可以写单元测试；ExtendScript 没法在 CI 里跑，越薄越好。

## 2. 设计原则

1. **容器就是 Prim JSON。** 文件里的 Prim 树直接用 02 的 `PrimTree` / `PrimSpec` / `AttrSpec` / `Keyframe` / `Value` 的 JSON 形式，`bae-interop-ae` 可以直接用 `bae-model` 的 serde 类型反序列化，然后用 `RestorePrim` 一类的 Op 落进 Stage。
2. **设置项用 BAE 的词，属性树用 AE 的词。** Prim 类型用 BAE 的内置类型（`Comp`、`Layer`、`Mask`、`Effect`、`Marker`、`TextAnimator`……）。合成、图层、素材、遮罩、标记的**设置项**（脚本接口上的属性，如入点、混合模式）凡是 02 schema 里有的，名字、值类型、枚举 token 都用 02 的；schema 里没有的 AE 专有设置写成 `ae:<脚本接口属性名>`（第 6 节）。**AE 属性树**里的属性（变换、遮罩路径、文字源文本、形状参数……）用 `ae:` 加 matchName 路径（4.8），由 Rust 导入器查表映射成 `transform:position` 一类的 BAE 属性。唯一的例外是效果参数：02 规定 `param:NNNN` 直接等于 AE 参数 matchName 末尾的序号，不用查表，所以导出时直接写成 `param:NNNN`。
3. **无损优先。** 导出脚本读得到的东西都写进去，不在 AE 端丢弃。BAE 暂时不支持的 `ae:` 属性，导入时原样保留在 Prim 上，导回 AE 时不丢。
4. **丢了什么要说出来。** 读不出来的东西（自定义值、渐变色标……）写进文件的 `report.unsupported`，导入报告直接用它。
5. **语言无关。** 只用 matchName、枚举 token 和数字，不依赖 AE 界面语言。显示名（`name`）只用来显示。

## 3. 文件

- 文件名建议：`<工程名>.bae-interchange.json`，UTF-8，无 BOM。
- 「收集素材」开启时，素材复制到同目录的 `<工程名>.assets/`，清单里的 `collected` 字段记录相对路径。

顶层对象：

| 字段 | 类型 | 必有 | 说明 |
|---|---|---|---|
| `format` | `"bae-interchange"` | 是 | 固定值 |
| `format_version` | string（semver） | 是 | 本文是 `0.2.0`。0.x 阶段次版本号不同就可能不兼容（0.2 相对 0.1 改了设置项的名字和类型），读取方应拒绝不认识的版本 |
| `profile` | `"ae"` | 是 | 属性词汇。目前只有 `ae`（AE matchName）；以后 Ai 互转可以有 `ai` |
| `generator` | object | 是 | `{name, version}`，例如 `{"name": "bae-ae-export", "version": "0.2.0"}` |
| `host` | object | 否 | 生成文件的软件：`{app, version, build, os, language}`；AE 端取 `app.version`、`app.buildName`、`$.os`、`app.isoLanguage` |
| `exported_at` | string | 否 | ISO 8601 UTC |
| `source` | object | 否 | `{project_path, project_name}`：原 .aep 的绝对路径（`/` 分隔）和文件名；未保存的工程两项都没有 |
| `time` | object | 是 | `{unit: "flicks", flicks_per_second: 705600000, key_time_base: "comp"}`，见 4.2 |
| `prims` | PrimTree[] | 是 | 根 `/` 下的 Prim 树，见第 5 节 |
| `assets` | AssetFile[] | 是 | 素材清单，见第 8 节 |
| `report` | object | 是 | 导出报告，见第 9 节 |

## 4. 编码约定

### 4.1 值（`Value`）

照搬 02 的 `Value`：`{"type": "<kind>", "value": ...}`。

| type | JSON value | AE 来源 |
|---|---|---|
| `float` | number | `PropertyValueType.OneD`；数值型设置项 |
| `int` | 整数 | 整数型设置项（宽高、标签、序号） |
| `bool` | boolean | 开关型设置项 |
| `string` | string | 名字、注释、路径 |
| `vec2` | `[x, y]` | `TwoD`、`TwoD_SPATIAL` |
| `vec3` | `[x, y, z]` | `ThreeD`、`ThreeD_SPATIAL`（AE 脚本接口里图层位置永远是三维，二维图层 z = 0） |
| `color` | `[r, g, b, a]` | `COLOR`（0–1 浮点，32 位工程可以超出 0–1）；AE 只给 RGB 的地方补 a = 1 |
| `bezier` | `{vertices, in_tangents, out_tangents, closed, feather?}` | `SHAPE`（遮罩路径、形状路径），切线相对顶点；遮罩羽化点是 `feather: [{segment, t, radius, tension, hold, inner, corner_angle}]`（AE `Shape.featherSegLocs` 等平行数组） |
| `time` | 整数 flicks | 入点、出点、开始时间、时长、工作区、标记时间（4.2） |
| `frame_rate` | `{num, den}` | 合成、素材的帧率（4.2） |
| `enum` | string | AE 枚举，转成 snake_case token，见 4.6 |
| `prim_ref` | Prim 路径 | 图层来源（素材、纯色、预合成）、父级、轨道遮罩、效果的图层参数（`LAYER_INDEX`）、遮罩参数（`MASK_INDEX`）、项目面板文件夹 |
| `text_document` | object，见 4.7 | `TEXT_DOCUMENT` |
| `float_list` | number[] | 未知值类型里长度不是 2、3 的数组（`unknown_value_type`） |

不导出的：`NO_VALUE`（属性组、按钮）；`CUSTOM_VALUE`（曲线、色阶直方图等，脚本读不到），记入 `report.unsupported`；`MARKER` 单独转成 `Marker` Prim（第 6.9 节）。脚本手册里没有的值类型（AE 26 的文字可变字体轴报 161）按值的形状导出，每种类型记一条 `unknown_value_type` 警告。

NaN / ±Infinity 不是合法 JSON：导出脚本遇到时写 `0` 并记一条 `report.warnings`（`code: "non_finite"`）。

### 4.2 时间

- 单位一律是 **flicks**（1 秒 = 705,600,000），和 02 的 `Time` 一致。所有常见帧率的帧边界都是整数。
- 导出脚本的换算：`flicks = Math.round(seconds * 705600000)`，即 AE 给的秒数四舍五入到最近的 flick。
- 设置项里的时间（入点、出点、开始时间、时长、工作区）用 `{"type": "int", "value": <flicks>}`。
- 关键帧的时间是 `AttrSpec.keyframes` 这个对象的键：flicks 整数写成十进制字符串（serde_json 对 `BTreeMap<Time, _>` 的写法），例如 `"23543520"`。
- `time.key_time_base = "comp"`：关键帧和图层标记的时间是 AE 脚本 `Property.keyTime()` 返回的值，即**所在合成的时间**（已经算上图层的开始时间和伸缩）。AE 内部按图层时间存储；导入器需要按图层的 `start_time` 和 `stretch` 换算成 BAE 约定的时间基准（等 02/03 定）。**已在 AE 26.2.1 实测**（`bridges/ae/README.md` 的 T4）：图层开始时间 1 秒、伸缩 100% 或 200% 时，合成时间 2 秒处的关键帧 `keyTime()` 都返回 2 秒。
- 帧率保留 AE 报告的浮点值（`frame_rate: float`），导入器负责换成有理数。
- **AE 的 NTSC 帧率不是 30000/1001（AE 26.2.1 实测）。** AE 报告的 `frameRate` 是单精度的 29.97（`29.9700317382812`），帧和关键帧落在 `n / 29.97` 秒上，即 AE 内部按 2997/100 计帧，而不是 `n × 1001/30000`。例如第 30 帧是 `706306306` flicks（`30000/1001` 时是 `706305600`），23.976 fps 同理（`n / 23.976`）。这样的时间不是整数 flicks，相对真 NTSC 每秒差约 1 µs（1 ppm）。导入器的做法：AE 的 29.97 / 23.976 / 59.94 合成换成 BAE 的 `30000/1001` / `24000/1001` / `60000/1001`，落在 AE 帧上的时间（与 `n / AE帧率` 相差不到 1 µs）吸附到 BAE 的第 n 帧，其他时间按两个帧率之比缩放。导回 AE 时反过来换算。

### 4.3 关键帧

照搬 02 的 `Keyframe`，字段与 AE 脚本接口一一对应：

| 字段 | AE 脚本接口 | 说明 |
|---|---|---|
| `value` | `keyValue(i)` | 类型同 4.1 |
| `interp_in` / `interp_out` | `keyInInterpolationType(i)` / `keyOutInterpolationType(i)` | `"linear"` / `"bezier"` / `"hold"` |
| `ease_in` / `ease_out` | `keyInTemporalEase(i)` / `keyOutTemporalEase(i)` | `[{speed, influence}]`。`TwoD`/`ThreeD` 非空间属性每维一项，其他（空间属性、颜色、一维、路径、文字）一项。**线性关键帧也照样导出 AE 报告的值**，不省略 |
| `spatial_in` / `spatial_out` | `keyInSpatialTangent(i)` / `keyOutSpatialTangent(i)` | 只在空间属性（值类型 `TwoD_SPATIAL` / `ThreeD_SPATIAL`）上有，不看 `isSpatial`（AE 26 对颜色控制也返回 true）；相对关键帧值；二维补 z = 0，总是三个数 |
| `flags.temporal_continuous` | `keyTemporalContinuous(i)` | 值为 false 时省略，下同 |
| `flags.temporal_auto_bezier` | `keyTemporalAutoBezier(i)` | |
| `flags.spatial_continuous` | `keySpatialContinuous(i)` | 只在空间属性上读 |
| `flags.spatial_auto_bezier` | `keySpatialAutoBezier(i)` | 只在空间属性上读 |
| `flags.roving` | `keyRoving(i)` | 只在空间属性上读 |
| `label` | `keyLabel(i)` | AE 22.6+，0 时省略 |

速度单位与 AE 的「关键帧速度」对话框一致（属性单位/秒；颜色按 0–255）。影响力单位是百分比（0.1–100）。

### 4.4 表达式

`AttrSpec.expression = {"source": <Property.expression>, "enabled": <Property.expressionEnabled>}`，只在 `expression` 非空时写。表达式出错（`expressionError` 非空）时额外记一条 `report.warnings`（`code: "expression_error"`），表达式原文照样导出。

有关键帧的属性只写 `keyframes`，不写 `value`；没有关键帧的写 `value`（`valueAtTime(0, true)`，即表达式之前的值）。

### 4.5 Prim 名字

02 的 Prim 名字只能包含 Unicode 字母数字、`_`、`-`，不能以 `-` 开头，不能为空。导出脚本这样生成：

1. 从 AE 显示名开始；ASCII 字母数字、`_`、`-`、常见 CJK 字符和拉丁扩展字母保留，其余字符（空格、标点、emoji）换成 `_`，连续的 `_` 合成一个，去掉首尾 `_`。
2. 结果为空或以 `-` 开头时，前面加类型前缀（`layer_`、`comp_`……）。
3. 同一父节点下重名时加 `_2`、`_3`……

原始显示名一律另存在 `name` 属性里。导入器不应信任文件里的名字，要再校验一遍。

### 4.6 枚举

AE 的枚举常量（`BlendingMode.MULTIPLY` 等）转成小写 snake_case token：`multiply`、`linear_dodge`、`no_track_matte`。导出脚本在运行时按常量名查表，不硬编码数值；查不到时写 `"unknown_<数值>"` 并记警告。

### 4.7 文字文档（`text_document`）

`TEXT_DOCUMENT` 值（「源文本」属性，可以有关键帧）按 02 的 `TextDocument`（`crates/bae-model/src/value.rs`）编码：

```json
{"type": "text_document", "value": {
  "text": "Hello\rWorld", "font": "SourceHanSansSC-Bold", "font_size": 72,
  "fill_color": [1, 1, 1, 1], "stroke_color": [0, 0, 0, 1], "stroke_width": 2, "stroke_over_fill": false,
  "justification": "center", "tracking": 25, "leading": 86.4, "baseline_shift": 0,
  "faux_bold": false, "faux_italic": false, "all_caps": false, "small_caps": false,
  "box_size": [400, 200], "box_position": [0, 0],
  "font_family": "Source Han Sans SC", "font_style": "Bold", "apply_fill": true, "apply_stroke": true,
  "auto_leading": false, "horizontal_scale": 1, "vertical_scale": 1, "superscript": false, "subscript": false,
  "tsume": 0, "point_text": false
}}
```

- `text`、`font`（PostScript 名）、`font_size` 总是有（02 要求必填；AE 读不到字体时写空字符串）。
- 02 的约定：有描边当且仅当有 `stroke_color`（AE `applyStroke` 为 false 时不写）；没有 `leading` 表示自动行距（AE `autoLeading`）；没有 `box_size` 表示点文字。
- `justification`：`left`、`right`、`center`、`justify_last_left`、`justify_last_right`、`justify_last_center`、`justify_all`、`multiple`（AE `ParagraphJustification`）。
- 后面一组（`font_family` 到 `point_text`）是 02 的结构里还没有的 AE 字段，照样写进去：`bae-model` 读的时候忽略它们，不报错；导回 AE 时需要它们（已请 02 补上或原样保留，见第 11 节）。
- 换行符保持 AE 的 `\r`。v0 只导出整体样式（第一个字符的样式），逐字符样式（AE 24.3+ 的 `CharacterRange`）以后再加。

### 4.8 `ae:` 属性名

02 规定属性名只能用字母、数字、`_`、`-`，`:` 作命名空间分隔，不超过 256 个字符（`validate_attr_name`）。AE 的 matchName 带空格、`/` 等字符，所以 AE 属性树里的属性名这样编码（可逆）：

```
ae:<段>:<段>:…        段 = 从所属 Prim 往下的每一级 matchName，编码后用 : 连接
```

每一段 matchName 逐字符编码：

| 原字符 | 编码 |
|---|---|
| ASCII 字母、数字 | 原样 |
| 空格 | `_` |
| `-` | `--` |
| 其他 ASCII 字符（`_`、`/`、`.`、`:`、`#` ……） | `-` + 两位大写十六进制，例如 `_` → `-5F`，`/` → `-2F` |
| 非 ASCII 字符 | `-u` + 四位大写十六进制（UTF-16 码元） |

同一命名组里 matchName 重复（极少见）时，第 k 个（k ≥ 2）在段尾加 `-n<k>`。解码时 `-` 后面只可能是 `-`、`u`、`n` 或两位大写十六进制，没有歧义。

| matchName 路径 | 属性名 |
|---|---|
| `ADBE Transform Group` / `ADBE Position` | `ae:ADBE_Transform_Group:ADBE_Position` |
| `ADBE Transform Group` / `ADBE Position_0` | `ae:ADBE_Transform_Group:ADBE_Position-5F0` |
| `ADBE Text Properties` / `ADBE Text Path Options` / `ADBE Text Path` | `ae:ADBE_Text_Properties:ADBE_Text_Path_Options:ADBE_Text_Path` |
| `ADBE Gaussian Blur 2-0001`（效果参数） | `ae:ADBE_Gaussian_Blur_2--0001` |
| `Pseudo/abc-0001`（伪效果参数） | `ae:Pseudo-2Fabc--0001` |

编码后超过 256 个字符的属性不导出，记一条 `report.unsupported`（`code: "attr_name_too_long"`）。导出脚本和 `bae-interop-ae` 各实现一份编码/解码，`bridges/ae/tests` 和 Rust 单元测试用同一组样例互相核对。

## 5. Prim 树布局

```
/project                                  Project
/folders/<folder>                         Folder          项目面板文件夹（扁平，层级用 folder 属性表示）
/assets/<asset>                           Asset           素材（文件、序列、纯色、占位）
/comps/<comp>                             Comp
/comps/<comp>/markers/<marker>            Marker          合成标记
/comps/<comp>/layers/<layer>              Layer           children 顺序 = AE 图层序号（第一个是 AE 的 1 号图层，最上面）
    …/layers/<layer>/markers/<marker>     Marker          图层标记
    …/layers/<layer>/masks/<mask>         Mask
    …/layers/<layer>/effects/<effect>     Effect
    …/layers/<layer>/contents/<item>      ShapeGroup / Path / Rect / Ellipse / Star / Fill / Stroke / GradientFill / GradientStroke / ShapeOperator
        …/contents/<group>/contents/<item>   形状组可以嵌套
    …/layers/<layer>/animators/<animator> TextAnimator
        …/animators/<animator>/selectors/<selector>   TextSelector
```

- `folders`、`assets`、`comps`、`layers`、`markers`、`masks`、`effects`、`contents`、`animators`、`selectors` 是不带类型的容器 Prim（不写 `prim_type`），只用来分组，空的不写。
- 项目面板的层级不体现在路径上（避免移动文件夹就改掉合成路径）：每个合成、素材、文件夹都有 `folder` 属性（`prim_ref`，指向 `/folders/...`；在根目录时省略）。
- 所有 Prim 的 `specifier` 都是 `"def"`。

### 5.1 AE 属性树怎么展开

AE 的属性树由三种节点组成（`PropertyBase.propertyType`）：`PROPERTY`（属性）、`NAMED_GROUP`（命名组，子项固定，例如「变换」）、`INDEXED_GROUP`（索引组，子项可增删、可重复，例如「蒙版」「效果」「内容」）。导出规则只有一条：

- **命名组展开成属性名路径**（第 4.8 节）：变换、文字选项、材质选项、摄像机选项等都变成所属 Prim 上的 `ae:` 属性。
- **索引组的子组变成子 Prim**，放在一个容器 Prim 下；子 Prim 的类型按 matchName 查表（第 6.10 节），每个都带 `match_name`、`name`、`enabled` 属性。
- 索引组里直接挂的属性（不是组）也展开成所属 Prim 上的 `ae:` 属性。

这样任何 AE 版本新增的属性组、第三方效果的参数都能导出，不需要导出脚本认识它们。

## 6. 各类 Prim 的属性

下表是**设置项**（AE 脚本接口上的属性）。「名字」一列没有 `ae:` 的都是 02 schema 里的属性（名字、值类型、枚举 token 与 `bae_model::schema` 一致）；`ae:` 开头的是 02 没有的 AE 专有设置，名字是 AE 脚本接口里的属性名。读不到就不写。AE 属性树里的属性按 4.8 / 5.1 展开，不在这些表里。

### 6.1 `Project`（`/project`）

| 名字 | 类型 | AE 来源 |
|---|---|---|
| `bit_depth` | enum `8` / `16` / `32` | `app.project.bitsPerChannel` |
| `working_space` | string | `workingSpace`（AE 16.0+） |
| `linearize` | bool | `linearizeWorkingSpace` |
| `blend_colors_using_gamma_1` | bool | `linearBlending` |
| `time_display` | enum `timecode` / `frames` | `timeDisplayType` |
| `expression_engine` | enum `javascript` / `extendscript` | `expressionEngine`（`javascript-1.0` 记作 `javascript`） |
| `ae:fileName` | string | 工程文件名（未保存时省略） |
| `ae:workingGamma`、`ae:displayStartFrame`、`ae:framesCountType`、`ae:feetFramesFilmType`、`ae:footageTimecodeDisplayStartType`、`ae:framesUseFeetFrames`、`ae:gpuAccelType` | | 同名脚本属性 |

### 6.2 `Folder`

`name`、`label`（int 0–16）；`ae:id`（`Item.id`）、`ae:parentFolder`（`prim_ref`，上级文件夹；在根目录时省略）、`ae:comment`。合成和素材也有这三个 `ae:` 属性。

### 6.3 `Asset`

| 名字 | 类型 | AE 来源 |
|---|---|---|
| `name`、`label` | | |
| `kind` | enum `footage` / `sequence` / `solid` / `placeholder` | 文件素材、图片序列（非静帧的图片文件）、`SolidSource`、`PlaceholderSource` |
| `width`、`height` | int | |
| `pixel_aspect` | float | |
| `frame_rate` | frame_rate | `frameRate`（静帧和纯色为 0，不写） |
| `duration` | time | |
| `has_video`、`has_audio` | bool | |
| `alpha_mode` | enum `straight` / `premultiplied` / `ignore` | `mainSource.alphaMode` |
| `premultiply_color` | color | `premulColor` |
| `solid_color` | color | 纯色素材的 `SolidSource.color` |
| `loop` | int | |
| `sequence:first_frame` | int | 图片序列第一帧文件名里的帧号 |
| `file:original`、`file:relative`、`file:size`、`file:mtime` | string / string / int / int | 文件引用，与 `docs/spec/project-format.md` 第 5 节一致，见第 8 节 |
| `ae:footageMissing`、`ae:isStill`、`ae:invertAlpha`、`ae:fieldSeparationType`、`ae:removePulldown`、`ae:conformFrameRate`、`ae:nativeFrameRate` | | 同名脚本属性 |

### 6.4 `Comp`

| 名字 | 类型 | AE 来源 |
|---|---|---|
| `name`、`label` | | |
| `width`、`height` | int | |
| `pixel_aspect` | float | |
| `frame_rate` | frame_rate | `frameRate`，换成 AE 实际计帧用的有理数（29.97 → `2997/100`，见 4.2） |
| `duration`、`display_start_time`、`work_area_start`、`work_area_duration` | time | |
| `bg_color` | color | `bgColor`（补 a = 1） |
| `motion_blur` | bool | |
| `shutter_angle`、`shutter_phase` | float | |
| `motion_blur_samples` | int | `motionBlurSamplesPerFrame` |
| `renderer` | enum `classic_3d` / `cinema_4d` / `advanced_3d` | `renderer`（`ADBE Standard 3D` / `ADBE Ernst` / `ADBE Advanced 3d`；不认识的写原文） |
| `preserve_frame_rate`、`preserve_resolution` | bool | `preserveNestedFrameRate`、`preserveNestedResolution` |
| `ae:motionBlurAdaptiveSampleLimit`、`ae:frameBlending`、`ae:draft3d`、`ae:hideShyLayers`、`ae:dropFrame`、`ae:resolutionFactor` | | 同名脚本属性 |

### 6.5 `Layer`

| 名字 | 类型 | AE 来源 |
|---|---|---|
| `kind` | enum | `shape` / `text` / `solid` / `footage` / `null` / `camera` / `light` / `adjustment` / `pre_comp` / `audio` |
| `name`、`label` | | |
| `source` | prim_ref | 素材、纯色指向 `/assets/...`，预合成指向 `/comps/...` |
| `parent` | prim_ref | 父级图层 |
| `start_time`、`in_point`、`out_point` | time | 合成时间 |
| `stretch` | float | 百分比，100 = 不伸缩，负数 = 倒放 |
| `enabled`、`audio_enabled`、`solo`、`shy`、`locked`、`guide` | bool | 视频开关、音频开关等；`guide` ← `guideLayer` |
| `three_d`、`adjustment_layer`、`collapse_transformation`、`motion_blur`、`time_remap_enabled` | bool | `threeDLayer`、`adjustmentLayer` 等 |
| `blend_mode` | enum | `BlendingMode`（AE 常量名的小写，`SILHOUETE_ALPHA` 记作 `silhouette_alpha`） |
| `track_matte` | enum `none` / `alpha` / `alpha_inverted` / `luma` / `luma_inverted` | `trackMatteType` |
| `track_matte_layer` | prim_ref | AE 23.0+ 用 `trackMatteLayer`；更早的版本是正上方一层（`index - 1`） |
| `auto_orient` | enum `off` / `along_path` / `towards_point_of_interest` | `autoOrient`；文字的「独立字符朝向摄像机」02 没有，写 `characters_toward_camera` |
| `quality` | enum `best` / `draft` / `wireframe` | |
| `sampling` | enum `bilinear` / `bicubic` | `samplingQuality` |
| `transform:separate_dimensions` | bool | 「位置」的 `dimensionsSeparated` |
| `light:type` | enum `parallel` / `spot` / `point` / `ambient` | 灯光图层的 `lightType` |
| `ae:matchName`、`ae:index`、`ae:id`、`ae:comment`、`ae:environmentLayer`、`ae:threeDPerChar`、`ae:preserveTransparency`、`ae:isTrackMatte`、`ae:effectsActive`、`ae:frameBlendingType` | | 同名脚本属性（`ae:id` 是 AE 22+ 的 `Layer.id`） |

属性树展开出来的 `ae:` 属性：`ae:ADBE_Transform_Group:…`、`ae:ADBE_Time_Remapping`、`ae:ADBE_Audio_Group:…`、`ae:ADBE_Material_Options_Group:…`、`ae:ADBE_Camera_Options_Group:…`、`ae:ADBE_Light_Options_Group:…`、`ae:ADBE_Text_Properties:…` 等，导入器映射到 `transform:*`、`time_remap`、`audio:levels`、`material:*`、`camera:*`、`light:*`、`text:source`、`text:path`。

### 6.6 `Mask`

设置项 `mode`（`none` / `add` / `subtract` / `intersect` / `lighten` / `darken` / `difference`）、`inverted`、`locked`、`color`；`ae:rotoBezier`、`ae:maskMotionBlur`、`ae:maskFeatherFalloff`。属性树：`ae:ADBE_Mask_Shape`（bezier，含羽化点）、`ae:ADBE_Mask_Feather`、`ae:ADBE_Mask_Opacity`、`ae:ADBE_Mask_Offset`，导入器映射到 02 的 `path`、`feather`、`opacity`、`expansion`。

### 6.7 `Effect`

`match_name`（效果的 matchName）、`name`、`enabled`。参数：效果组下直接挂的、matchName 形如 `<效果 matchName>-NNNN` 的属性写成 `param:NNNN`（02 第 11 节，例如 `ADBE Gaussian Blur 2-0001` → `param:0001`，伪效果 `Pseudo/abc-0002` → `param:0002`）；其他属性（命名子组里的、不符合这个模式的）仍按 4.8 写 `ae:` 名字。第三方效果同样导出，是否支持由导入器按 `docs/interop/ae-effects.md`（与 07 共用，以后建）和 02 的 `EffectRegistry` 判断。

### 6.8 形状内容

`contents` 下每一项：`match_name`、`name`、`enabled`，形状操作器另有 `operator`（token 与 02 schema 一致）。参数是 `ae:` 属性（例如 `ae:ADBE_Vector_Rect_Size`、`ae:ADBE_Vector_Transform_Group:ADBE_Vector_Position`），导入器映射到 02 的 `direction`、`fill_rule`、`merge:mode` 等。形状组（`ADBE Vector Group`）的子项在它自己的 `contents` 下。

### 6.9 `Marker`

AE 的标记是 `ADBE Marker` 属性上的「关键帧」，值是 `MarkerValue`。每个标记导出成一个 `Marker` Prim：

| 名字 | 类型 | AE 来源 |
|---|---|---|
| `time`、`duration` | time | 标记时间（与关键帧同一时间基准，4.2） |
| `comment`、`chapter`、`url`、`frame_target`、`cue_point_name` | string | |
| `label` | int | AE 16.0+ |
| `protected_region` | bool | AE 16.0+ |
| `ae:eventCuePoint` | bool | |
| `ae:param:<编码后的键>` | string | `getParameters()` 的每一项，键按 4.8 编码 |

### 6.10 matchName → Prim 类型

| matchName | Prim 类型 | 容器名 |
|---|---|---|
| `ADBE Mask Atom` | `Mask` | `masks` |
| `ADBE Effect Parade` 的子项（任意 matchName） | `Effect` | `effects` |
| `ADBE Vector Group` | `ShapeGroup` | `contents` |
| `ADBE Vector Shape - Group` | `Path` | `contents` |
| `ADBE Vector Shape - Rect` | `Rect` | `contents` |
| `ADBE Vector Shape - Ellipse` | `Ellipse` | `contents` |
| `ADBE Vector Shape - Star` | `Star` | `contents` |
| `ADBE Vector Graphic - Fill` | `Fill` | `contents` |
| `ADBE Vector Graphic - Stroke` | `Stroke` | `contents` |
| `ADBE Vector Graphic - G-Fill` | `GradientFill` | `contents` |
| `ADBE Vector Graphic - G-Stroke` | `GradientStroke` | `contents` |
| `ADBE Vector Filter - *` | `ShapeOperator`，`operator` 见下 | `contents` |
| `ADBE Text Animator` | `TextAnimator` | `animators` |
| `ADBE Text Selector`、`ADBE Text Wiggly Selector`、`ADBE Text Expressible Selector` | `TextSelector` | `selectors` |
| 其他索引组的子组 | `ae:Group` | 容器名 = 索引组 matchName 净化后的结果 |

`operator` token：`Merge` → `merge`、`Offset` → `offset`、`PB` → `pucker_bloat`、`Repeater` → `repeater`、`RC` → `round_corners`、`Trim` → `trim`、`Twist` → `twist`、`Roughen` → `roughen`、`Wiggler` → `wiggle_paths`、`Zigzag` → `zig_zag`、`Wiggle Transform` → `wiggle_transform`；不认识的写 matchName 原文。

## 7. 示例

一个 1920×1080、29.97 fps 的合成，里面一个形状图层，位置有两个缓动关键帧，不透明度有表达式（节选，完整例子见 `bridges/ae/examples/`）：

```json
{
  "format": "bae-interchange",
  "format_version": "0.2.0",
  "profile": "ae",
  "generator": {"name": "bae-ae-export", "version": "0.2.0"},
  "time": {"unit": "flicks", "flicks_per_second": 705600000, "key_time_base": "comp"},
  "prims": [
    {"name": "comps", "specifier": "def", "children": [
      {"name": "Main", "specifier": "def", "prim_type": "Comp",
       "attrs": {
         "name": {"value": {"type": "string", "value": "Main"}},
         "width": {"value": {"type": "int", "value": 1920}},
         "height": {"value": {"type": "int", "value": 1080}},
         "frame_rate": {"value": {"type": "frame_rate", "value": {"num": 2997, "den": 100}}},
         "duration": {"value": {"type": "time", "value": 7063063063}}
       },
       "children": [
        {"name": "layers", "specifier": "def", "children": [
          {"name": "Box", "specifier": "def", "prim_type": "Layer",
           "attrs": {
             "kind": {"value": {"type": "enum", "value": "shape"}},
             "name": {"value": {"type": "string", "value": "Box"}},
             "ae:ADBE_Transform_Group:ADBE_Position": {"keyframes": {
               "0": {"value": {"type": "vec3", "value": [200, 540, 0]},
                     "interp_in": "bezier", "interp_out": "bezier",
                     "ease_in": [{"speed": 0, "influence": 33.333333}],
                     "ease_out": [{"speed": 0, "influence": 33.333333}],
                     "spatial_in": [0, 0, 0], "spatial_out": [0, 0, 0]},
               "706306306": {"value": {"type": "vec3", "value": [1720, 540, 0]},
                     "interp_in": "bezier", "interp_out": "bezier",
                     "ease_in": [{"speed": 0, "influence": 33.333333}],
                     "ease_out": [{"speed": 0, "influence": 33.333333}],
                     "spatial_in": [0, 0, 0], "spatial_out": [0, 0, 0]}
             }},
             "ae:ADBE_Transform_Group:ADBE_Opacity": {
               "value": {"type": "float", "value": 100},
               "expression": {"source": "wiggle(2, 20)", "enabled": true}
             }
           }}
        ]}
       ]}
    ]}
  ],
  "assets": [],
  "report": {"warnings": [], "unsupported": []}
}
```

## 8. 素材清单（`assets`）

每个有文件的素材（`asset_kind` 为 `file` 或 `sequence`）一项：

| 字段 | 类型 | 说明 |
|---|---|---|
| `prim` | string | 对应的 `/assets/...` Prim 路径 |
| `path_absolute` | string | 原始绝对路径，`/` 分隔（Windows 盘符写成 `C:/...`） |
| `path_relative` | string | 相对交换文件所在目录的路径 |
| `aep_relative` | string | 相对原 .aep 所在目录的路径（工程未保存时省略） |
| `missing` | bool | AE 里是否缺失 |
| `size` | int | 字节数（缺失时省略） |
| `modified` | string | 修改时间，ISO 8601 |
| `sequence` | object | 图片序列：`{first, count}`，`first` 是第一帧文件名，`count` 是导出时找到的帧文件数 |
| `collected` | string 或 string[] | 收集素材时复制到的位置（相对交换文件）；序列是文件列表 |

这三种路径对应 ARCHITECTURE 第 16 节的查找顺序：相对路径 → 绝对路径 → 按文件名和指纹查找。`path_absolute`、`path_relative`、`size` 与 Asset Prim 上的 `file:original`、`file:relative`、`file:size` 相同（清单里的 `modified` 是 ISO 时间，Prim 上的 `file:mtime` 是 Unix 毫秒）；导入时以 Prim 上的属性为准，清单用于收集素材和展开序列。内容哈希 v0 不算（ExtendScript 太慢），以后由 BAE 导入时补。

## 9. 导出报告（`report`）

```json
"report": {
  "warnings":    [{"code": "expression_error", "path": "/comps/Main/layers/Box", "attr": "ae:ADBE_Transform_Group:ADBE_Opacity", "message": "..."}],
  "unsupported": [{"code": "gradient_colors", "path": "/comps/Main/layers/Shape/contents/Gradient_Fill_1", "attr": "ae:ADBE_Vector_Grad_Colors", "match_name": "ADBE Vector Grad Colors", "message": "..."}],
  "stats": {"folders": 1, "assets": 3, "comps": 2, "layers": 12, "properties": 640, "keyframes": 85, "expressions": 4, "effects": 3, "masks": 2, "seconds": 1.8}
}
```

`code` 目前有：

| code | 归类 | 含义 |
|---|---|---|
| `custom_value` | unsupported | `CUSTOM_VALUE` 类型的属性（曲线、色阶等），脚本读不到值 |
| `gradient_colors` | unsupported | 形状渐变的色标（`ADBE Vector Grad Colors`），脚本读不到值 |
| `mask_feather_points` | unsupported | 保留：v0.2 起羽化点按 02 的 `FeatherPoint` 导出，不再报这一项 |
| `text_per_character_style` | unsupported | 保留，v0 不检测 |
| `attr_name_too_long` | unsupported | 编码后的属性名超过 256 个字符，没有导出 |
| `read_error` | warning | 读属性时抛异常，附异常信息 |
| `expression_error` | warning | 表达式在 AE 里报错 |
| `non_finite` | warning | 值是 NaN / Infinity，写成了 0 |
| `unknown_enum` | warning | 枚举值查不到名字 |
| `missing_footage` | warning | 素材缺失 |
| `collect_failed` | warning | 收集素材时建目录或复制失败 |
| `unknown_value_type` | warning | 脚本手册里没有的 `PropertyValueType`（AE 26 的文字可变字体轴报 161），按值的形状导出；每种类型只报一次 |

## 10. v0 已知限制

- 形状渐变色标、`CUSTOM_VALUE` 参数（曲线、色阶）：脚本读不到，只进报告。渐变色标以后通过解析 .aepx 或 bodymovin 的做法补上。
- 文字只导出整体样式，不导出逐字符样式；段落样式只有 `justification`。02 的 `TextDocument` 还没有的 AE 字段（4.7）在 BAE 里读入时会被丢掉，导回 AE 前需要 02 补上。
- 只导出 `OneD` 的数值，不区分效果里的复选框、弹出菜单（都是 float）；由导入器按 matchName 判断。
- 不导出：渲染队列、输出模块、代理设置、合成的「基本图形」面板模板、3D 模型图层（AE 24+）。
- 「仅修改过的属性」选项（`onlyModified`）依赖 AE 的 `PropertyBase.isModified`，可能漏掉默认值不同于 BAE 的属性，默认关闭。

## 11. 与 02 Prim schema 的对齐状态

对照的是 02 合并进 `dev` 的 `crates/bae-model`（Prim schema v1，PR #11，`dev` 提交 `5214377` 及之后）、`docs/spec/data-model.md` 第 3、10、11 节和 `docs/spec/project-format.md` 第 5 节。核对方法（临时 Rust 程序，没有提交）：

- 用 `bae-model` 的 serde 类型直接反序列化 `bridges/ae/examples/` 下的两个示例（`Vec<PrimTree>`，`text_document` 不做任何替换），经 `Op::RestorePrim` 写入 Sublayer、`to_trees` 读回，重新序列化逐字段一致。
- 每个 02 schema 认识的属性都过一遍 `schema::check_value`（值类型、枚举 token）和 `schema::is_animatable`：模拟工程只有夹具里故意构造的未知遮罩模式 `unknown_9999` 不通过。
- 素材 Prim 用 `AssetFile::from_prim` 读出的路径、大小、修改时间正确；`bridges/ae/samples/anim-cases.json` 里的 67 个 `AttrSpec` 往返一致。

02 已经采纳（v0.2 按此导出）：`time`、`frame_rate`、`text_document` 值类型；`Marker`、`TextAnimator`、`TextSelector` Prim 类型；`FeatherPoint` 遮罩羽化点；图层顺序（第一个子 Prim 是最上面的图层）；图层来源用 `source`（`prim_ref`）；设置项的名字和枚举（第 6 节）；效果参数 `param:NNNN`；`PrimTree` 带关键帧的 JSON 反序列化 bug（PR #9 修复）。

02 已经定、由导入器处理的：

- **关键帧时间基准**：BAE 里图层（及其遮罩、效果、形状内容、标记）上的关键帧用图层时间，`合成时间 = start_time + 图层时间 × stretch / 100`。交换文件保持 AE 脚本接口给的合成时间（`key_time_base = "comp"`，4.2），导入器按 `图层时间 = (合成时间 − start_time) × 100 / stretch` 换算。
- **属性树映射**：`ae:ADBE_Transform_Group:ADBE_Position` → `transform:position` 等，按 02 schema 查表；查不到的原样保留为 `ae:` 属性。

还没对上、已写进 02 任务书「请求」的：

1. **NTSC 帧率**（AE 26.2.1 实测）：AE 按十进制的 29.97（2997/100）计帧，不是 30000/1001（4.2）。导入器会换成 `30000/1001` 并按帧号吸附；建议 02 在 `data-model.md` 写明这条约定。
2. **`TextDocument` 缺 AE 字段**：`font_family`、`font_style`、`apply_fill`、`horizontal_scale`、`vertical_scale`、`superscript`、`subscript`、`tsume` 等（4.7）。目前 serde 读入时丢掉，导回 AE 会丢。
3. **形状内容的 `name` / `enabled`**：02 schema 里 `ShapeGroup`、`Fill` 等没有这两个属性，导出时照样写（schema 外的属性原样保存）。

## 12. 版本

- `format_version` 遵循 semver。1.0 之前：只加可选字段时加补丁号；改名、改语义、删字段时加次版本号（0.1 → 0.2 就是这种）。1.0 之后按通常的 semver。
- 读取方遇到不认识的属性、Prim 类型和 `report.code` 时应保留或忽略，不能报错。
