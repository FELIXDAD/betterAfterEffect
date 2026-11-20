# 清单参考（`bae-plugin.toml`）

清单是插件包根目录下的 TOML 文件。格式版本 `schema_version = 1`，对应规格 §5。宿主解析清单时：

- 语法错误、值类型错误、**未知的键**都会报错并给出行列号（拼错的键不会被悄悄忽略）。唯一例外是 `[x]` 表，留给厂商扩展，宿主不读。
- 所有路径都相对于包根目录，用 `/` 分隔，不能是绝对路径，不能含 `.` 或 `..`。

## 标识符规则

| 标识符 | 规则 | 例子 |
|---|---|---|
| 插件 ID | 反向域名，至少两段，每段 `[a-z0-9]([a-z0-9-]*[a-z0-9])?`，不超过 128 字节。`bae.*` 留给官方 | `com.example.glow` |
| 贡献 ID（效果、命令……） | `[a-z][a-z0-9_-]*`，不超过 64 字节，同一插件同一种类里唯一 | `box-blur` |
| 参数 key | `[a-z][a-z0-9_]*`，不超过 64 字节，不能以 `bae_` 开头，不能是 WGSL 关键字 | `amount` |
| 参数 id | 1 到 2147483647 的整数，同一效果里唯一，**永不复用** | `3` |
| 平台键 | `macos-aarch64`、`macos-x86_64`、`windows-x86_64`、`windows-aarch64`、`linux-x86_64`、`linux-aarch64` | |

插件 ID、贡献 ID、参数 id 会写进工程文件，发布后不能改。

## `[plugin]`

```toml
schema_version = 1

[plugin]
id = "com.example.vignette"         # 必需
version = "1.2.0"                   # 必需，语义化版本 2.0
api = "0.1"                         # 必需，需要的最低插件 API（主版本.次版本）
name = "Vignette"                   # 必需，默认（英文）名称
description = "Darkens the edges of a layer."
authors = ["Example Studio <dev@example.com>"]
license = "Apache-2.0"              # SPDX 表达式，仅作说明
homepage = "https://example.com/vignette"
host = ">=0.3, <2"                  # 可选：对 BAE 应用版本的要求
platforms = ["macos-aarch64", "windows-x86_64"]   # 可选：限制平台
replaces = ["com.example.old-vignette"]           # 可选：取代的旧插件 ID
icon = "icons/vignette.svg"         # 可选，只用自己绘制的 SVG
```

`api`：宿主接受"主版本相同、宿主次版本 ≥ 插件次版本"的插件。现在的宿主是 API 0.1。

## `[[effect]]`

```toml
[[effect]]
id = "vignette"                     # 必需
name = "Vignette"                   # 必需
category = "Stylize"                # "效果和预设"里的分类（英文键）
description = "..."                 # 提示文字，也给 Agent 看
class = "filter"                    # filter（默认）| generator | transition
implementation = "wgsl"             # 必需：wgsl（声明式）| native（原生）
keywords = ["edge", "darken"]       # 搜索关键词
ae_match_names = []                 # 导入 AE 工程时可以代替的 AE 效果 matchName
flags = ["pixel-independent"]       # 见下表

# ---- 只用于原生效果 ----
threading = "concurrent"            # serial-global | serial-instance | concurrent（仅列表用，以代码为准）
devices = ["cpu"]                   # cpu / gpu

# ---- 只用于声明式效果 ----
shader = "shaders/vignette.wgsl"    # 必需
includes = []                       # 在 shader 之前拼接的 WGSL 文件
features = []                       # WGSL 扩展：["f16"]
style = "pixel"                     # pixel | compute；不写时：有 [[effect.pass]] 就是 compute
entry = "vignette"                  # 像素式：像素函数名，默认 = id 把 - 换成 _
identity_when = "params.amount == 0"  # 成立时跳过效果
```

效果的完整 ID（工程里的 `match_name`）是 `<插件 ID>/<效果 ID>`。

`flags`：

| 值 | 含义 |
|---|---|
| `pixel-independent` | 输出像素只取决于同位置的输入像素（像素式效果自动具有） |
| `expands-buffer` | 结果可能超出输入范围（`grows = true` 时自动具有） |
| `shrinks-buffer` | 结果可能小于输入范围 |
| `non-param-vary` | 参数不变时结果也随时间变化（例如按时间变化的噪波） |
| `wide-time-input` | 读取其他时间的输入或参数 |
| `uses-shutter` | 读取快门角度/相位 |
| `uses-camera` / `uses-lights` | 依赖合成的摄像机/灯光 |
| `non-deterministic` | 永不缓存 |
| `in-place` | （原生 CPU）输出可以和输入共用缓冲区 |

### `[effect.roi]`（声明式）

告诉宿主效果需要多大的输入区域、输出会不会变大。表达式语法见 [WGSL 参考](wgsl.md#表达式)，单位是**全分辨率图层像素**，宿主会乘以降采样比例并向上取整。

```toml
[effect.roi]
extent = "input"                    # input（滤镜默认）| layer（生成器默认）| infinite
read = "ceil(params.radius)"        # 四边都读多少像素；也可以分别写 read_left / read_right / read_top / read_bottom
grows = true                        # 结果按读取范围向外扩展（模糊的典型情况）
```

像素式效果只读自己的像素，不能写 `read*`。

### `[[effect.texture]]` 与 `[[effect.pass]]`（声明式，计算式）

```toml
[[effect.texture]]
name = "tmp"                        # 着色器里用 bae_load_tmp / bae_sample_tmp 读
format = "working"                  # working（默认）| rgba16f | rgba32f | r32f | rg32f | rgba8unorm
rect = "input"                      # output（默认）| input：覆盖输出范围还是（扩展后的）输入范围
scale = 1.0                         # 保留；API 0.1 只支持 1.0

[[effect.pass]]
entry = "blur_horizontal"           # 着色器里的 @compute 入口
reads = ["input"]                   # 读哪些源：input、to（转场）、layer 参数的 key、中间纹理名
writes = "tmp"                      # output 或中间纹理名

[[effect.pass]]
entry = "blur_vertical"
reads = ["tmp"]
writes = "output"                   # 最后一个 pass 必须写 output
```

规则：pass 按顺序执行；中间纹理要先被写过才能被读；pass 不能读自己的目标；入口实际读到的源必须列在 `reads` 里（宿主在加载时检查）。

## `[[effect.param]]`

通用的键：`id`（必需）、`key`（必需）、`type`（必需）、`name`（必需）、`group`（所属参数组的 key）、`flags`、`hidden`、`disabled`。

| `type` | 专用的键（默认值） | 工程里存的值 | WGSL 里的字段 |
|---|---|---|---|
| `float` | `default`(0)、`range`、`slider`(=range)、`precision`(2)、`display` | 浮点数 | `f32` |
| `int` | `default`、`range`、`slider`、`display` | 整数 | `i32` |
| `angle` | `default`（度） | 浮点数（度） | `f32`（度） |
| `checkbox` | `default`(false)、`label` | 布尔 | `u32`（0/1） |
| `color` | `default` `[r, g, b]` 或 `[r, g, b, a]`（0–1）、`alpha` | 非预乘 RGBA | `vec4<f32>` |
| `point` | `default` `[x, y]`、`relative`(true：按图层尺寸的比例) | 图层像素 | `vec2<f32>`（**渲染像素**） |
| `point3d` | `default` `[x, y, z]`、`relative` | 图层像素 | `vec3<f32>` |
| `popup` | `choices = ["圆形", "适合图层"]`、`default`（从 1 开始的序号，默认 1） | 从 1 开始的序号（与 AE 相同） | `u32` |
| `layer` | `default`（`"none"` / `"self"`） | 图层引用 | 纹理 `bae_tex_<key>`（计算式；渲染图支持后可用） |
| `group` | `end_id`（必需）、`collapsed` | — | — |
| `button` | `label` | — | — |
| `arbitrary` | —（仅原生） | 字节 | — |
| `path` | —（仅原生） | 遮罩引用 | — |
| `string`、`asset` | 保留，API 0.1 还不能用 | | |

说明：

- `display`：`number`、`percent`、`pixels`、`degrees`、`seconds`、`frames`，只影响显示。
- `range` 是允许的值，`slider` 是滑块范围，`default` 必须在 `range` 里。超出范围的值（关键帧、表达式）在渲染时被限制到 `range`。
- **弹出菜单的选项只能在末尾追加**：工程里存的是序号，插入或删除会让旧工程的值错位。
- **参数组**：`type = "group"` 的参数开始一个组，属于它的参数写 `group = "<组的 key>"`，并且要紧跟在组参数后面。`end_id` 是组结束标记的 ID（AE 把组结束也算作一个参数），同样永不复用。
- 相对坐标的点参数（默认）：`default = [0.5, 0.5]` 表示图层中心。

参数的 `flags`：`hidden`、`disabled`、`supervise`（原生插件在值改变时得到通知）、`not-animatable`、`cannot-time-vary`、`start-collapsed`、`ui-only`（不影响像素，不进缓存键）、`no-expressions`、`transition-progress`（转场的进度参数，必须是 float，转场有且只有一个）。

## `[[effect.migration]]`

插件升级时迁移旧工程的数据：

```toml
[[effect.migration]]
from = "1"                          # 写入数据的插件版本要求
rename = { radius_px = "radius" }   # 旧 key → 新 key
drop = ["legacy_mode"]
defaults = { quality = 2 }          # 旧版本没有的参数取这个值
```

## 原生和脚本插件

```toml
[native]
isolation = "in-process"            # in-process（默认）| out-of-process | prefer-out-of-process
[native.libraries]                  # 每个平台一个库，路径相对包根目录
macos-aarch64 = "bin/macos-aarch64/libvignette.dylib"
windows-x86_64 = "bin/windows-x86_64/vignette.dll"

[script]
entry = "scripts/main.js"           # ES 模块
memory_limit_mb = 64                # 默认 64，最大 1024
```

有原生贡献就必须有 `[native]`，有脚本贡献就必须有 `[script]`。库文件是构建产物，检查工具在库缺失时只给警告。

## 其他贡献

```toml
[[command]]                         # 命令：菜单、快捷键、Agent 的 MCP 工具
id = "align-to-grid"
name = "Align Layers to Grid"
implementation = "script"           # script | native
entry = "alignToGrid"               # 脚本导出的函数
schema = "schemas/align-to-grid.json"   # 参数的 JSON Schema（必需）
menu = "Layer/Transform"
shortcut = { motion = "Ctrl+Alt+G" }    # 按工作区设置默认快捷键
description = "..."                 # 也是 MCP 工具的说明

[[automation]]                      # 自动化脚本（AE 的"文件 > 脚本"）
id = "report-off-grid"
name = "Report Off-Grid Layers"
entry = "reportOffGrid"
menu = "File/Scripts"
```

命令在命令注册表里的 ID 是 `plugin.<插件 ID>.<命令 ID>`，自动化脚本是 `script.<插件 ID>.<ID>`。

`[[shape_operator]]`、`[[importer]]`、`[[exporter]]`、`[[codec]]`、`[[panel]]`、`[[workspace]]`、`[[keymap]]`、`[[expression_library]]` 的格式见规格 §5.3.8；API 0.1 的宿主会解析它们，但还不加载（给出警告）。

## `[permissions]`、`[dependencies]`、`[exports]`

```toml
[permissions]                       # 脚本插件强制执行；原生插件是声明，安装时给用户看
project = "write"                   # none | read（脚本默认）| write
filesystem = ["read:project", "write:plugin-data"]
network = ["https://api.example.com"]
clipboard = "none"
process = false
gpu_native = false
render = false

[dependencies]
"com.example.shader-lib" = "^1.2"
"bae.std-shaders" = { version = "^0.1", optional = true }

[exports]                           # 给依赖本插件的插件用
wgsl = ["shaders/noise.wgsl"]
js = ["scripts/lib.js"]
```

（跨插件引用 WGSL 模块 `includes = ["plugin:<id>/<路径>"]` 在 API 0.1 里还不支持。）

## 本地化（`locales/<语言>.toml`）

```toml
[plugin]
name = "暗角"
description = "使图层边缘变暗。"

[effect.vignette]
name = "暗角"
keywords = ["边缘", "压暗"]

[effect.vignette.params]            # 参数 key → 显示名
amount = "数量"

[effect.vignette.choices.shape]     # 弹出菜单 key → 序号 → 显示名
1 = "圆形"
2 = "适合图层"

[command.align-to-grid]
name = "对齐到网格"
```

清单里的英文是默认值，缺少的翻译会退回英文。分类和菜单路径用英文键，界面按 Adobe 官方中文译名显示（`docs/glossary.md`）。
