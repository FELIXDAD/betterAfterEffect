# 原生插件：用 Rust 写

`bae-plugin-sdk` 是 C ABI（[`sdk/c/bae_plugin.h`](../../sdk/c/bae_plugin.h)，ABI 1.0）的 Rust 封装。用它写原生效果，只需要描述参数、处理像素；入口函数 `bae_plugin_entry`、效果 vtable、参数定义、版本协商都由宏生成。SDK 没有任何依赖，几秒就能编译完。

C ABI 的规则、宿主怎么运行原生插件（进程内 / 进程外、失败处理、目前提供的套件）见 [原生插件：C ABI 入门](native-c.md)。完整示例是 [`plugins/examples/duotone-rs`](../../plugins/examples/duotone-rs)。

## 新建插件

```toml
# Cargo.toml
[package]
name = "my-effect"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]          # 动态库：macOS .dylib，Windows .dll

[dependencies]
bae-plugin-sdk = { path = "<BAE 仓库>/crates/bae-plugin-sdk" }

[profile.release]
panic = "unwind"                 # 必须：SDK 靠它把 panic 转成错误码
```

```rust
// src/lib.rs
use core::ffi::CStr;

use bae_plugin_sdk::abi::{BAE_DISPLAY_PERCENT, BAE_EFFECT_FLAG_PIXEL_INDEPENDENT};
use bae_plugin_sdk::effect::{CpuEffect, Error, Param, RenderArgs};

#[derive(Default)]
struct Gain;

impl CpuEffect for Gain {
    const ID: &'static CStr = c"gain";                    // = 清单里 [[effect]] 的 id
    const PARAMS: &'static [Param] = &[
        Param::float(1, c"gain", c"Gain", 100.0, [0.0, 400.0])
            .slider(0.0, 200.0)
            .display(BAE_DISPLAY_PERCENT),
    ];
    const FLAGS: u64 = BAE_EFFECT_FLAG_PIXEL_INDEPENDENT;

    fn render(&self, args: &mut RenderArgs<'_>) -> Result<(), Error> {
        let g = args.params.float(1).unwrap_or(100.0) as f32 / 100.0;
        args.map_pixels(|_x, _y, [r, g0, b, a]| [r * g, g0 * g, b * g, a])
    }
}

bae_plugin_sdk::export_plugin!(id: "com.example.gain", version: "0.1.0", effects: [Gain]);
```

清单（`bae-plugin.toml`）：

```toml
schema_version = 1

[plugin]
id = "com.example.gain"          # = export_plugin! 的 id
version = "0.1.0"                # = export_plugin! 的 version
api = "1.0"                      # SDK 的 API 版本
name = "Gain"

[native]
isolation = "in-process"         # 可信插件；第三方插件在用户目录里仍会进程外运行

[native.libraries]
macos-aarch64 = "bin/macos-aarch64/libmy_effect.dylib"
windows-x86_64 = "bin/windows-x86_64/my_effect.dll"

[[effect]]
id = "gain"
name = "Gain"
category = "Color Correction"
class = "filter"
implementation = "native"
devices = ["cpu"]
```

清单可以不写 `[[effect.param]]`：宿主加载时直接用 `define_params` 的结果（SDK 从 `PARAMS` 生成）。如果写了（让宿主不加载代码也能列出参数），必须和 `PARAMS` 一致，否则加载失败并提示哪个参数对不上。

`cargo build --release` 后把动态库复制到清单写的位置，整个目录放进插件目录即可。`bae plugin check <目录>` 检查清单。

## 参数

| 构造函数 | 类型 | 渲染时读取 |
|---|---|---|
| `Param::float(id, key, name, default, [min, max])` | 浮点滑块；`.slider(lo, hi)`、`.precision(n)`、`.display(BAE_DISPLAY_*)` | `args.params.float(id)` |
| `Param::int(...)` | 整数滑块 | `args.params.int(id)` |
| `Param::angle(id, key, name, default)` | 角度（度） | `args.params.float(id)` |
| `Param::checkbox(id, key, name, default)` | 复选框 | `args.params.checkbox(id)` |
| `Param::color(id, key, name, [r, g, b, a])` | 颜色（非预乘） | `args.params.color(id)` |
| `Param::point(id, key, name, [fx, fy])` | 点，默认值是图层尺寸的比例 | `args.params.point(id)`（渲染像素） |
| `Param::popup(id, key, name, default_id, &[(id, c"名字"), …])` | 弹出菜单，值是稳定的选项 ID | `args.params.choice(id)` |

- **参数 ID** 是工程里保存值用的键（对应 AE 的参数索引）：发布后永不复用、永不改类型。
- **key** 是脚本和表达式用的名字，`[a-z][a-z0-9_]*`。
- 显示名用英文写在这里；中文写在 `locales/zh-CN.toml` 的 `[effect.<id>.params]` 里。
- `.flags(BAE_PARAM_FLAG_*)` 加标志，例如 `BAE_PARAM_FLAG_HIDDEN`。

## 渲染

`render` 拿到 `RenderArgs`：

- `args.input` / `args.output`：预乘 RGBA 图像，坐标是图层的渲染像素（半分辨率时坐标减半）。`output.rect()` 里的每个像素都要写。输入范围外的像素按透明处理（`input.pixel(x, y)` 自动返回透明）。
- `args.map_pixels(|x, y, 输入像素| 输出像素)`：逐像素处理的捷径，每行检查一次中止。
- 按行处理更快：`args.output.row_f32_mut(y)`、`args.input.row_f32(y)`（每像素 4 个 `f32`）。
- `args.time_seconds()`、`args.downsample()`、`args.info`（帧率、图层范围、渲染标志等）。
- 长时间计算要经常检查 `args.is_aborted()`，为真就返回 `Err(Error::aborted())`。

像素格式默认是 RGBA 32 位浮点（预乘）。想要 8 位，把 `const FORMATS` 设成 `&[BAE_PIXEL_FORMAT_RGBA_U8_PREMUL]`，用 `row_u8_mut` 处理。

**线程**：默认 `THREADING = BAE_THREADING_CONCURRENT`，表示 `render` 可以被多个线程同时调用（AE 的多帧渲染）；效果结构体本身要是 `Sync` 的。做不到就设成 `BAE_THREADING_SERIAL_GLOBAL`，宿主会串行调用。

## 错误和 panic

- 返回 `Err(Error::new(状态码, "原因"))`：宿主在状态栏显示原因，这一帧该效果直接输出输入。
- `panic!` 不会穿过 C ABI：SDK 在每个回调外面包了 `catch_unwind`，转成 `BAE_ERR_PANIC` 加 panic 信息。进程内运行的插件 panic 后会被停用（状态可能已经坏了）；进程外运行时同样处理。
- 所以 `[profile.release]` 不能设 `panic = "abort"`，否则 panic 会直接结束进程（进程外模式下只结束插件子进程，但宿主会把它当作崩溃）。

## 调试

- `BAE_PLUGIN_PATH=<插件目录> bae` 从开发目录加载插件。
- `BAE_PLUGIN_IN_PROCESS=<插件 ID>` 让插件在 BAE 进程内运行，调试器可以直接在插件代码里下断点；默认的进程外模式下，要附加到 `bae plugin runner` 子进程。
- `bae plugin list` 显示插件是否加载成功、实际运行方式和问题。

## 兼容性

`export_plugin!` 生成的入口声明 `abi_required = 1.0`，清单 `api` 也写 `"1.0"`。BAE 在主版本 1 内只做追加，按这个版本编译的插件在之后的 1.x 宿主上不用重新编译。SDK 的 Rust API 变化不会改变 ABI。
