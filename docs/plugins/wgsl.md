# WGSL 参考：声明式效果的着色器约定

本文是插件 API 0.1 中声明式效果着色器的约定（规格 §11）。宿主把你的 WGSL 和它生成的**前导代码**（prelude）拼成一个模块，用 naga 校验，再在共享的 wgpu 设备上作为计算着色器运行。前导代码的权威实现是 `bae_plugin::wgsl::prelude`。

## 坐标与像素

- **渲染像素**：全分辨率图层像素 × 降采样比例。原点在图层左上角，y 向下。半分辨率预览时 `frame.downsample = (0.5, 0.5)`，一个渲染像素覆盖 2×2 个图层像素。
- 所有矩形 `BaeRect { min, max }` 都是半开区间 `[min, max)`，单位是渲染像素。
- 像素 `(x, y)` 的中心在 `(x + 0.5, y + 0.5)`。
- 颜色是工作色彩空间的值，**预乘 alpha**，可以超出 0–1（32 位浮点工程、HDR）。
- 读到源矩形之外时得到透明黑（与 AE 一致），不需要自己处理边缘。
- 距离类参数（半径、宽度）在清单里是全分辨率像素，着色器里用 `bae_scale(x)` 换算成渲染像素，这样不同预览分辨率下效果一致。**点参数**已经换算好了（`params.center` 就是渲染像素）。

## 模块的组成

```text
<前导代码>      宿主按清单生成：类型、绑定、辅助函数（虚拟文件，不计入你的行号）
<includes>      清单里 includes 列出的文件，按顺序
<shader>        清单里的 shader 文件
<内核>          只有像素式才有：宿主生成的 @compute 入口 bae_pixel_main
```

你的代码的规则（加载时在 naga IR 上检查）：

- 不能声明 `@group` / `@binding` 资源，所有资源都来自前导代码。
- 函数、类型、常量、全局变量不能以 `bae_` 或 `Bae` 开头。
- 用 `f16` 时在清单里写 `features = ["f16"]`，宿主会在模块开头加 `enable f16;`，你不要自己写。

## 前导代码提供的内容

```wgsl
struct BaeRect { min: vec2<i32>, max: vec2<i32> }

struct BaeFrame {
  target_rect: BaeRect,     // 当前 pass 目标覆盖的矩形（写 output 的 pass 等于 output_rect）
  output_rect: BaeRect,     // 效果的输出矩形
  layer_bounds: BaeRect,    // 图层范围
  downsample: vec2<f32>,    // (1,1) 全分辨率，(0.5,0.5) 半分辨率……
  time: f32,                // 图层时间（秒）
  time_step: f32,           // 每个合成帧的秒数（未知时为 0）
  frame_rate: f32,          // 合成帧率（未知时为 0）
  pixel_aspect: f32,
  shutter_angle: f32,       // 度
  shutter_phase: f32,
  flags: u32,               // BAE_FRAME_LINEAR | BAE_FRAME_FINAL | BAE_FRAME_HIGH_QUALITY | BAE_FRAME_FLOAT | BAE_FRAME_16_BPC | BAE_FRAME_INPUT_STRAIGHT | BAE_FRAME_INPUT_OPAQUE
  seed: u32,                // 每个效果实例固定的随机种子
  frame_index: i32,
  pass_index: u32,
}
var<uniform> frame: BaeFrame;

struct BaeParams { /* 每个数值参数一个字段，按清单顺序 */ }
var<uniform> params: BaeParams;   // 没有数值参数时不存在
```

`BaeParams` 的字段类型：

| 参数类型 | 字段类型 | 说明 |
|---|---|---|
| `float`、`angle` | `f32` | 角度单位是度 |
| `int` | `i32` | |
| `checkbox` | `u32` | 0 或 1 |
| `popup` | `u32` | 从 1 开始的序号 |
| `color` | `vec4<f32>` | 非预乘 RGBA |
| `point` | `vec2<f32>` | 渲染像素 |
| `point3d` | `vec3<f32>` | 渲染像素 |

`BAE_FRAME_16_BPC` 表示 16 bpc 整数工程，满白为 32768。

`BAE_FRAME_FLOAT`：工程是 32 bpc（浮点）。没有这个标志时（8 / 16 bpc，AE 的默认），像素式效果的返回值由宿主截断到 `[0, 1]`（非预乘值），和 AE 整数缓冲的行为一致；AE 里"32 bpc 颜色时关闭"这类选项也按它判断。

`group`、`button`、`layer`、`arbitrary`、`path` 没有字段；`layer` 参数在计算式里是一个源纹理。

> 注意：在 API 0.1 里，`frame.layer_bounds` 暂时用效果的输入范围代替，`time_step`、`frame_rate`、`frame_index`、快门参数暂时为 0，等渲染器补齐这些信息（已向 04 提出）。

### 辅助函数

| 函数 | 作用 |
|---|---|
| `bae_load_<src>(p: vec2<i32>) -> vec4<f32>` | 读源 `<src>` 在渲染像素 `p` 的值；范围外为透明 |
| `bae_sample_<src>(p: vec2<f32>) -> vec4<f32>` | 在渲染像素位置 `p` 双线性采样（像素中心在 .5）；范围外为透明 |
| `bae_rect_<src>() -> BaeRect` | 源覆盖的矩形 |
| `bae_store(p: vec2<i32>, c: vec4<f32>)` | 写当前 pass 的目标 |
| `bae_target_pixel(id: vec3<u32>) -> vec2<i32>` | 调用 ID → 目标中的渲染像素 |
| `bae_in_target(p: vec2<i32>) -> bool` | `p` 是否在目标矩形里 |
| `bae_scale(x: f32) -> f32` | 全分辨率距离 → 渲染像素 |
| `bae_to_layer(p)` / `bae_from_layer(p)` | 渲染像素 ↔ 全分辨率图层像素 |
| `bae_layer_uv(p) -> vec2<f32>` | 在图层范围内的 0–1 坐标 |
| `bae_premultiply(c)` / `bae_unpremultiply(c)` | 预乘 / 去预乘（alpha 为 0 时去预乘得 0） |
| `bae_to_linear(c)` / `bae_from_linear(c)` | 工作值 ↔ 线性光（工作空间已线性化时不变，否则用 sRGB 曲线；对负值保号） |
| `bae_luma(c) -> f32` | Rec.709 亮度 |
| `bae_hash(seed: u32, p: vec2<i32>) -> f32` | 确定性的 0–1 噪波 |

`<src>` 是 `input`（效果的输入）、`to`（转场的目标图层）、`layer` 参数的 key 或中间纹理名。

前导代码里没有采样器：`bae_sample_*` 用四次 `bae_load_*` 做双线性插值，所以任何工作格式（包括不一定能硬件过滤的 RGBA32F）结果都一样。

## 像素式

```wgsl
struct BaePixel {           // 宿主生成
    pos: vec2<i32>,         // 渲染像素
    layer_pos: vec2<f32>,   // 像素中心，全分辨率图层像素
    uv: vec2<f32>,          // 像素中心，在图层范围内的 0–1 坐标
}

fn my_effect(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    return ...;             // c 和返回值都是预乘的
}
```

- 宿主生成内核：读输入 → 调用你的函数 → 写输出，工作组 16×16。
- 像素函数**不能使用任何纹理**（包括通过它调用的函数），只能读 `frame`、`params` 和调用辅助函数。这样相邻的像素式效果以后可以融合成一个内核，省掉中间纹理的读写。
- 适合：调色、色调分离、暗角、反转、阈值、渐变叠加这类逐像素运算。

## 计算式

```wgsl
@compute @workgroup_size(8, 8)
fn blur_horizontal(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = bae_target_pixel(id);
    if (!bae_in_target(p)) {
        return;
    }
    // ……读 bae_load_input(...)、bae_sample_input(...)……
    bae_store(p, color);
}
```

- 每个 `[[effect.pass]]` 一次调度，工作组大小来自入口的 `@workgroup_size`（每组最多 256 个调用），宿主调度 `ceil(目标尺寸 / 工作组尺寸)` 个工作组。
- 每个 pass 里，`bae_store` 写的是这个 pass 的目标（`output` 或某个中间纹理），`frame.target_rect` 是目标的矩形。
- 目标纹理来自宿主的纹理池，**初始内容未定义**：每个 pass 必须给目标矩形里的每个像素都写一个值（透明也要写 `vec4<f32>(0.0)`），不能只写一部分。
- 需要读周围像素时，在 `[effect.roi]` 里声明读取范围，宿主会请求足够大的输入；输出变大（例如模糊）时写 `grows = true`。
- 依赖数据的 pass 结构（层数随参数变化的金字塔、迭代求解）不适合声明式，请用原生插件。

## 表达式

`identity_when` 和 `[effect.roi]` 的 `read*` 用一种很小的表达式语言，加载时解析，每帧求值不分配内存：

```text
数字、params.<key>、frame.downsample_x、frame.downsample_y、true、false
+ - * /（除以 0 得 0）   == != < <= > >=   && || !   ( )
min(a, b)  max(a, b)  abs(x)  floor(x)  ceil(x)  sqrt(x)  clamp(x, lo, hi)
```

`params.<key>` 可以读 float、int、angle、checkbox（0/1）和 popup（序号）参数。例：

```toml
identity_when = "params.amount == 0 || params.radius < 0.5"
read = "ceil(params.radius * 1.5)"
```

## 诊断

着色器错误在加载时报告，位置指向你的文件：

```text
shaders/box_blur.wgsl:27:21: error: effect "box-blur": no definition in scope for identifier: `nope`: unknown identifier
```

常见的规则错误：

| 信息 | 原因 |
|---|---|
| `declares @group/@binding` | 你的代码声明了绑定；用前导代码提供的 `params`、`bae_load_*` |
| `the name ... is reserved` | 名字以 `bae_` / `Bae` 开头 |
| `pixel function ... not found` / `must have the signature` | 像素函数名或签名不对 |
| `pixel functions ... read no textures` | 像素函数（或它调用的函数）读了纹理；改用计算式 |
| `reads ... but the manifest does not list it in reads` | 计算式入口读了没在 `reads` 里声明的源 |
| `reads its own target` | pass 读了自己正在写的纹理 |
| `invocations per workgroup` | 工作组超过 256 个调用 |

## 性能

- 宿主每帧只做：求参数值、算 ROI 表达式、打包一个很小的 uniform、录制调度命令。管线按工作格式编译一次并缓存。
- 声明式效果和用同样着色器写的内置效果走同一条路径；验收目标是两者耗时差 < 2%。
- 尽量用像素式；计算式里先 `bae_in_target` 再读写；大半径的循环要设上限（参考 `box-blur` 的 `MAX_TAPS`），防止拖住 GPU。
