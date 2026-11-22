# bae-effects

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

## 已有效果

| AE matchName | 名称 | 形态 |
|---|---|---|
| `ADBE Tint` | 色调 | 声明式 |
| `ADBE Tritone` | 三色调 | 声明式 |
| `ADBE Easy Levels2` | 色阶 | 声明式 |
| `ADBE Pro Levels2` | 色阶（单独控件） | 声明式 |
| `ADBE HUE SATURATION` | 色相/饱和度 | 声明式 |
| `ADBE Brightness & Contrast 2` | 亮度和对比度 | 声明式 |
| `ADBE Exposure2` | 曝光度 | 声明式 |
| `ADBE Gaussian Blur 2` / `ADBE Gaussian Blur` | 高斯模糊 / 高斯模糊（旧版） | Rust |
| `ADBE Box Blur2` / `ADBE Box Blur` | 快速方框模糊 / 方框模糊 | Rust |

完整对照（含尚未实现的第一批效果）见 `docs/interop/ae-effects.md`。

## 高斯模糊的做法

σ = 0.3 × 模糊度 × 降采样因子（AE 的系数待 AE 实测确认），核在 3σ 处截断。σ < 6 渲染像素时做两遍可分离卷积（源行缓存在 workgroup 内存，每线程算 4 个输出）；σ ≥ 6 时先块平均降采样到 1/2ⁿ（层上 σ 保持 3–6），在低分辨率上卷积，再双线性上采样，层上 σ 扣除降采样和上采样引入的方差。降采样网格对齐到 2ⁿ 的倍数，所以不同 ROI 分块渲染的结果一致。

## 测试与基准

- `cargo test -p bae-effects`：GPU 结果与 CPU 参考实现（AE 规格公式）比较，覆盖 RGBA32F / RGBA16F、半分辨率、Repeat Edge Pixels、ROI 分块一致、identity；软件适配器（CI 上的 WARP）容差放宽 4 倍。
- `cargo bench -p bae-effects`：4K / 1080p 每帧耗时；`-- <过滤词> --steps` 打印每个 dispatch 的 GPU 时间。
- 与 AE 对照（需要装了 AE 的机器）：见 `tests/ae/`，`cargo run -p bae-effects --example ae_compare -- prepare|compare|golden`，`tests/ae_golden.rs` 在有 golden 图时比较。
