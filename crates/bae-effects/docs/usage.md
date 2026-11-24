# 效果库接入与测试

BAE 的内置效果（工作流 07）。效果在 GPU 上运行，参数的名称、顺序、默认值、取值范围与 After Effects 一致，AE 工程导入后直接对上。接口细节以代码文档注释为准（`cargo doc -p bae-effects --open`）。

## 接入

```rust
let mut defs = bae_model::EffectRegistry::new();      // 界面、Agent、AE 导入用的参数定义
let mut library = bae_render::EffectLibrary::new();   // 渲染图用的实现
bae_effects::register(&mut defs, &mut library);
// 打开工程时在工作线程上预编译管线，避免第一帧卡顿：
bae_effects::prewarm(&gpu, wgpu::TextureFormat::Rgba16Float)?;
```

- 文档里效果的身份（`Effect` Prim 的 `match_name`）是 AE 的 matchName，例如 `ADBE Gaussian Blur 2`；参数 id 等于 AE 的 disk id（参数组的开始和结束各占一个 id），即 `param:NNNN`。内置效果的 `EffectDef.plugin` 为 `None`。
- `bae_effects::catalog()`：给"效果和预设"面板（12）的中英文名称、分类（AE 英文分类名，`category_zh` 给中文）、搜索关键词、参数中文名。
- `bae_effects::get(match_name)`：按 matchName 查内置效果。
- `Standalone`：不经过渲染图直接运行一个效果（测试、基准、工具）。`bae_render::EncodeContext` 在 bae-render 外无法构造（R04-12）。

## 两种实现形态（插件规格 §19）

| 形态 | 位置 | 用于 |
|---|---|---|
| 声明式 WGSL 内嵌包 | `packages/<id>/`（`bae-plugin.toml` + 着色器 + `locales/zh-CN.toml`），由 `bae_plugin::DeclarativeEffect::load_builtin` 加载 | 逐像素效果（颜色校正等），与第三方声明式插件走同一条路径，可融合 |
| Rust 效果 | `src/effects/`，自己的 WGSL 核在 `src/shaders/` | 通道结构随参数变化的效果（模糊金字塔、迭代） |

Rust 效果的 uniform 走一个环形缓冲 + 动态偏移，每帧不创建缓冲；管线按（核、格式）缓存在每个 `GpuContext` 的运行时里。

## 按分类登记

每个分类的 `src/categories/<分类>.rs` 负责加载声明式包、登记 Rust 效果和预编译该分类的 GPU 管线。已有分类直接改本分类的 `register` 函数。扭曲和抠像已有独立入口，便于并行开发。基准用例放在 `benches/cases/`，与 AE 的误差和限制记录在 `tests/ae/categories/`。公开的 `builtins()`、`get()`、`catalog()` 和 `register()` 接口沿用原签名。

8 / 16 bpc 工程里每个效果的输出截断到 [0, 1]（AE 整数缓冲的行为），32 bpc 不截断（`EffectParams.float`）。

## 测试与基准

- `cargo test -p bae-effects`：GPU 结果与 CPU 参考实现（AE 规格公式）比较，覆盖 RGBA32F / RGBA16F、半分辨率、Repeat Edge Pixels、ROI 分块一致、identity；软件适配器（CI 上的 WARP）容差放宽 4 倍。
- `cargo bench -p bae-effects`：4K / 1080p 每帧耗时；`-- <过滤词> --steps` 打印每个 dispatch 的 GPU 时间。
- 与 AE 对照：`tests/ae/golden/` 是 AE 26.2.1 的输出，`tests/ae_golden.rs` 每次测试都比较（每个用例限定最大差和平均差）；重新生成和结论见[AE 对照说明](../tests/ae/README.md)。

命令行对照工具和 GPU 回归共用 `tests/ae/categories/<分类>.rs`。该模块登记本类的默认用例、仅作回归的补充用例、输入图生成函数，以及需要直通 Alpha 输入的效果。跨分类的既有 AE 批次会按原顺序运行一次。分类页记录 matchName、实测结果和工作流进度。
