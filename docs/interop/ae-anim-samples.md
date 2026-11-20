# AE 关键帧与表达式对照样例（草案 v0.1）

负责：15 `interop/ae`。使用方：03 `core/animation`（`bae-anim` 的 AE 对照测试，验收标准「误差 < 1e-3」）。

机器可读定义：[`ae-anim-samples.schema.json`](ae-anim-samples.schema.json)（同时描述输入和输出两种文件）。

> **状态：AE 参考数据还没有生成。** 本机没有 AE，生成脚本只在模拟的 AE 对象模型上测过（`bridges/ae/tests/anim-harness.js`）。需要有 AE 的同事按 `bridges/ae/README.md` 的测试 T8 跑一次，把结果提交到 `bridges/ae/samples/`。

## 1. 流程

```
gen_anim_cases.py ──► anim-cases.json ──(在 AE 里运行 bae-anim-samples.jsx)──► anim-samples.ae-<AE 版本>.json ──► bae-anim 测试
```

| 文件 | 位置 | 谁生成 |
|---|---|---|
| 用例生成器 | `bridges/ae/samples/gen_anim_cases.py` | 手写（只用 Python 标准库） |
| 用例 | `bridges/ae/samples/anim-cases.json` | 生成器（提交进仓库；`--check` 检查是否最新） |
| AE 采样脚本 | `bridges/ae/bae-anim-samples.jsx` | 手写（ExtendScript，需要同目录的 `bae-export.jsx`） |
| AE 参考数据 | `bridges/ae/samples/anim-samples.ae-<AE 版本>.json` | 在 AE 里运行采样脚本 |

03 的测试直接读 `bridges/ae/samples/` 下的参考数据，不需要复制。参考数据可以有多份（不同 AE 版本），文件名带 `app.version` 的主版本号，例如 `anim-samples.ae-24.json`。

## 2. 用例文件（`format: "bae-anim-cases"`）

```json
{
  "format": "bae-anim-cases",
  "format_version": "0.1.0",
  "defaults": {
    "comp":   {"width": 1920, "height": 1080, "frame_rate": {"num": 30, "den": 1}, "duration": 2822400000},
    "layer":  {"type": "null"},
    "sample": {"from": 0, "to": 2822400000, "per_frame": 2}
  },
  "cases": [
    {
      "id": "easy_ease",
      "group": "temporal_1d",
      "description": "Easy Ease on both keys (speed 0, influence 33.33%).",
      "property": ["ADBE Transform Group", "ADBE Rotate Z"],
      "attr": {"keyframes": {
        "0":          {"value": {"type": "float", "value": 0.0},   "interp_in": "bezier", "interp_out": "bezier",
                       "ease_in": [{"speed": 0.0, "influence": 33.333}], "ease_out": [{"speed": 0.0, "influence": 33.333}]},
        "1411200000": {"value": {"type": "float", "value": 360.0}, "interp_in": "bezier", "interp_out": "bezier",
                       "ease_in": [{"speed": 0.0, "influence": 33.333}], "ease_out": [{"speed": 0.0, "influence": 33.333}]}
      }}
    }
  ]
}
```

- `attr` 就是 02 的 `AttrSpec`（与交换格式 `docs/interop/bae-interchange.md` 第 4.3 节相同），关键帧时间是合成时间的 flicks。
- `property` 是从图层往下的 matchName 路径。
- `layer`：`type`（`null` / `solid` / `shape`）、`three_d`、`start_time`（flicks）、`stretch`（百分比）、`separate_position`（位置「单独尺寸」）、`effects`（要先加上的效果 matchName，例如 `ADBE Color Control`）。
- `comp`、`sample` 覆盖 `defaults` 里的同名字段。`frame_rate` 可以是有理数 `{num, den}`。
- `sample`：从 `from` 到 `to`（含两端，flicks），步长 = 一帧 / `per_frame`。默认每帧采两次（帧上和半帧），4 秒 30 fps 共 241 个时刻。
- `other`：同一图层上要先设置的其他属性（表达式读取的位置、滑块控制等）。

### 用例分组（共 62 个）

| group | 内容 |
|---|---|
| `temporal_1d` | 线性、定格、只有入点定格、缓动（对称/不对称/非零速度/影响 100% 与 0.1%）、等值鼓包、线性与贝塞尔混合、范围外取值、时间连续、时间自动贝塞尔、逐帧关键帧、帧间关键帧、不透明度过冲截断 |
| `temporal_nd` | 缩放：线性、每维不同缓动、定格 |
| `color` | 颜色控制：线性、缓动（一条共享曲线）、定格 |
| `spatial` | 位置：直线、贝塞尔切线、切线 + 缓动（弧长）、空间自动贝塞尔、连续贝塞尔、漂浮关键帧、定格、3D、零长度路径、速度过大被截断、单独尺寸、点控制 |
| `time` | 图层开始时间偏移、伸缩 200%、伸缩 -100%、29.97 / 23.976 / 25 fps |
| `expression` | `time`、`value`、`linear/ease/easeIn/easeOut`、`loopOut` 四种、`loopIn`、`valueAtTime`、`velocity`、`Math`/`clamp`、`timeToFrames/framesToTime`、`posterizeTime`、数组运算、读同层其他属性、`speed`、`length`、按 matchName 读效果参数 |

只放结果确定的表达式；`wiggle`、`random` 依赖 AE 未公开的随机数实现，不放进对照。

## 3. 参考数据文件（`format: "bae-anim-samples"`）

```json
{
  "format": "bae-anim-samples",
  "format_version": "0.1.0",
  "generator": {"name": "bae-anim-samples", "version": "0.1.0"},
  "host": {"app": "After Effects", "version": "24.6x56", "build": "...", "os": "...", "expression_engine": "javascript-1.0"},
  "cases_format_version": "0.1.0",
  "time": {"unit": "flicks", "flicks_per_second": 705600000, "key_time_base": "comp"},
  "results": [
    {
      "id": "easy_ease",
      "comp": {"width": 1920, "height": 1080, "frame_rate": 30, "duration": 2822400000},
      "layer": {"start_time": 0, "stretch": 100, "in_point": 0, "out_point": 2822400000, "three_d": false},
      "property": {"path": ["ADBE Transform Group", "ADBE Rotate Z"], "value_type": "float", "spatial": false, "dimensions": 1},
      "attr": {"keyframes": {"0": {"...": "AE 实际存下的关键帧"}}},
      "times": [0, 11760000, 23520000],
      "pre_expression": [[0], [0.0123], [0.0491]],
      "post_expression": [[0], [0.0123], [0.0491]]
    }
  ]
}
```

- **`attr` 是从 AE 读回来的**，不是用例里写的。AE 会改写一部分输入：自动贝塞尔的缓动和切线、漂浮关键帧的时间、影响力的截断（0.1–100）。03 应该用这里的 `attr` 求值，而不是用例里的。
- `times`：采样时刻（合成时间，flicks）。
- `pre_expression`：`valueAtTime(t, true)`，只有关键帧插值。`post_expression`：`valueAtTime(t, false)`，只在有表达式的用例里有。
- 每个值都是数组（一维值是一个元素的数组）；颜色是 0–1 的 RGBA。AE 给出 NaN/Infinity 时写 `null`。
- `error`：这个用例在 AE 里搭建失败（例如某个版本没有该属性），其他字段可能缺失。`expression_error`：表达式在 AE 里报错。`notes`：读回关键帧时的警告。

## 4. 给 03 的使用建议

1. 用 `bae-model` 反序列化 `attr`（`AttrSpec` 单独反序列化没问题；注意 `PrimTree` 目前反序列化带关键帧的 JSON 会失败，已报给 02）。
2. 按 `layer.start_time`、`layer.stretch` 把合成时间换成 BAE 约定的时间基准（`time.key_time_base` 目前是 `comp`，见交换格式 4.2）。
3. 在每个 `times[i]` 求值，与 `pre_expression[i]`（以及 `post_expression[i]`）逐分量比较，容差 1e-3。
4. 已知需要单独看的用例：`spatial_roving`（时间由 AE 重算）、`spatial_speed_clamp`（影响力截断，规格 AE-ANI-035 标了 UNVERIFIED）、`color_ease`（颜色速度的单位，AE-ANI-034 标了 UNVERIFIED）、`opacity_overshoot_clamp`（截断发生在插值之后）、`layer_stretch_reverse`。这些正好是 AE 规格（`docs/spec/ae/transform-keyframes-expressions.md`）里没有核实的地方，用 AE 的数据定下来。

## 5. 改用例

改 `gen_anim_cases.py`，运行它重新生成 `anim-cases.json`，然后在 AE 里重新采样。`bridges/ae/tests/run.sh` 会检查生成脚本能在模拟环境里跑通全部用例；`python3 bridges/ae/samples/gen_anim_cases.py --check` 检查提交的用例文件是否最新。
