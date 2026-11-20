# 快速开始：第一个声明式效果

本文从零写一个"色调分离"（Posterize）效果：把每个颜色通道减少到 N 个色阶。完整代码就是仓库里的 [`plugins/examples/posterize`](../../plugins/examples/posterize)。

声明式效果只需要两个文件：清单 `bae-plugin.toml` 和一个 WGSL 着色器。不写任何本地代码，不需要编译器，也不需要 Rust 工具链（检查工具除外）。

## 1. 目录结构

```text
com.example.posterize/
  bae-plugin.toml          清单（必需）
  shaders/posterize.wgsl   着色器
  locales/zh-CN.toml       中文名称（可选）
```

目录名只是给人看的，宿主以清单里的插件 ID 为准。

## 2. 清单

```toml
schema_version = 1

[plugin]
id = "com.example.posterize"     # 反向域名，发布后不能改
version = "1.0.0"                # 语义化版本
api = "0.1"                      # 需要的最低插件 API 版本
name = "Posterize (example)"     # 默认（英文）名称；中文放在 locales/zh-CN.toml
description = "Reduces each color channel to a number of levels."

[[effect]]
id = "posterize"                 # 效果 ID；完整 ID 是 com.example.posterize/posterize
name = "Posterize (example)"
category = "Stylize"             # "效果和预设"里的分类（用英文键，界面按 Adobe 中文译名显示）
implementation = "wgsl"          # 声明式效果
shader = "shaders/posterize.wgsl"
identity_when = "params.levels >= 256"   # 条件成立时跳过这个效果，不占 GPU

[[effect.param]]
id = 1                           # 稳定的数字 ID：工程按它保存参数值，永不复用
key = "levels"                   # 着色器里用 params.levels 读取
type = "int"
name = "Levels"
default = 6
range = [2, 256]                 # 允许的值
slider = [2, 32]                 # 滑块显示的范围

[[effect.param]]
id = 2
key = "linear"
type = "checkbox"
name = "Use Linear Light"
default = false
```

要点：

- `id`（插件、效果、参数）发布后都不能改，工程文件里存的就是它们。要改名字只改 `name`。
- 没有写 `style` 时，没有 `[[effect.pass]]` 的效果就是**像素式**（pixel style）：你只写"一个像素进、一个像素出"的函数，宿主生成计算着色器的其余部分，还能把相邻的像素式效果融合成一个内核。
- `identity_when` 是一个小表达式（[语法](wgsl.md#表达式)），成立时宿主直接把输入当作输出。

## 3. 着色器

```wgsl
fn posterize(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    // 先去掉预乘，让半透明边缘保持原来的覆盖度。
    var s = bae_unpremultiply(c);
    if (params.linear != 0u) {
        s = bae_to_linear(s);
    }
    let n = f32(max(params.levels, 2) - 1);
    let q = vec4<f32>(floor(clamp(s.rgb, vec3<f32>(0.0), vec3<f32>(1.0)) * n + 0.5) / n, s.a);
    var out = q;
    if (params.linear != 0u) {
        out = bae_from_linear(q);
    }
    return bae_premultiply(out);
}
```

- 像素函数的名字默认等于效果 ID（`-` 换成 `_`），签名固定为 `fn 名字(c: vec4<f32>, px: BaePixel) -> vec4<f32>`。
- `c` 是这个输出像素位置上的输入颜色（**预乘** alpha，工作色彩空间的值）。返回值也是预乘的。
- `params` 是宿主按清单生成的 uniform：`struct BaeParams { levels: i32, linear: u32 }`。整数参数是 `i32`，复选框是 `u32`（0/1），浮点和角度是 `f32`，颜色是 `vec4<f32>`，点是 `vec2<f32>`（渲染像素）。
- `bae_unpremultiply`、`bae_to_linear` 等辅助函数来自宿主生成的前导代码，完整列表见 [WGSL 参考](wgsl.md)。
- 不要自己声明 `@group` / `@binding`，也不要用 `bae_` / `Bae` 开头的名字；像素函数不能读纹理（要读周围像素请用计算式，见 `box-blur` 示例）。

## 4. 中文名称（可选）

`locales/zh-CN.toml`：

```toml
[plugin]
name = "色调分离（示例）"
description = "把每个颜色通道减少到指定的色阶数。"

[effect.posterize]
name = "色调分离（示例）"
keywords = ["色调分离", "色阶"]

[effect.posterize.params]
levels = "色阶"
linear = "使用线性光"
```

## 5. 检查

在 BAE 仓库里运行检查工具（应用的 `bae plugin check` 子命令接入前，先用它）：

```sh
cargo run -p bae-plugin --example check -- path/to/com.example.posterize
```

```text
path/to/com.example.posterize: ok — com.example.posterize 1.0.0 (API 0.1), effects: com.example.posterize/posterize
```

它和宿主加载插件时做的检查完全一样：清单语法和取值、文件是否存在、把着色器和宿主生成的前导代码拼在一起用 naga 编译校验。出错时给出编译器风格的位置，编辑器可以直接跳转：

```text
path/to/com.example.posterize/shaders/posterize.wgsl:13:28: error: effect "posterize": invalid field accessor `levles`: invalid accessor
path/to/com.example.posterize/bae-plugin.toml:30:1: error: unknown field `slidr`, expected one of `id`, `key`, `type`, `name`, ...
```

行列号指向你自己的文件（宿主生成的前导代码不计入行号）。

## 6. 测试

仓库里的示例用 `crates/bae-plugin/tests/declarative_gpu.rs` 测试：在真实 GPU 上渲染，再和 CPU 参考实现逐像素对比，同时测半分辨率（降采样）和 `identity_when`。自己的插件可以照抄这个模式。

## 7. 安装

把插件目录放进用户插件目录（版本并存时用 `<插件 ID>/<版本>/` 的布局）：

| 平台 | 用户插件目录 |
|---|---|
| macOS | `~/Library/Application Support/BAE/Plug-ins/` |
| Windows | `%LOCALAPPDATA%\BAE\Plug-ins\` |
| Linux（无界面渲染） | `$XDG_DATA_HOME/bae/plugins/` |

设置 `BAE_HOME=<目录>` 时所有目录都在 `<目录>` 下。插件宿主（`bae_plugin::PluginHost`）扫描这些目录、为每个插件 ID 选出最高版本、注册效果；替换或删除插件后可以重新加载或卸载，不需要重启。应用启动时自动加载插件目录要等应用外壳接入插件宿主（后续版本）。详见 [打包、安装与版本](publishing.md)。

## 下一步

- 读周围像素、多个 pass、中间纹理：看 [`box-blur`](../../plugins/examples/box-blur) 和 [WGSL 参考](wgsl.md#计算式)。
- 点、颜色、弹出菜单参数和线性光混合：看 [`vignette`](../../plugins/examples/vignette)。
- 所有清单键：[清单参考](manifest.md)。
