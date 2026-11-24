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

## 按分类登记

每个分类的 `src/categories/<分类>.rs` 负责加载声明式包、登记 Rust 效果和预编译该分类的 GPU 管线。已有分类直接改本分类的 `register` 函数。扭曲和抠像已有独立入口，便于并行开发。基准用例放在 `benches/cases/`，与 AE 的误差和限制记录在 `tests/ae/categories/`。公开的 `builtins()`、`get()`、`catalog()` 和 `register()` 接口沿用原签名。

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
| `ADBE Motion Blur` / `ADBE Radial Blur` | 定向模糊 / 径向模糊 | Rust |
| `ADBE Sharpen` / `ADBE Unsharp Mask2` | 锐化 / 钝化蒙版 | 声明式计算 / Rust |
| `ADBE Glo2` / `ADBE Drop Shadow` | 发光 / 投影 | Rust |
| `ADBE Bevel Alpha` / `ADBE Find Edges` | 斜面 Alpha / 查找边缘 | 声明式计算 |
| `ADBE Mosaic` / `ADBE Posterize` | 马赛克 / 色调分离 | Rust / 声明式像素 |

其余效果的对照和计划见 `docs/interop/ae-effects.md`。

## 高斯模糊的做法

与 AE 26 实测一致（`tests/ae/README.md`）：非线性工作空间里先按纯幂 2.4 转线性光再模糊（转换折进第一遍读和最后一遍写，不加通道），σ = 0.288 × 模糊度 × 降采样因子，核取 |x| ≤ 3σ 的采样。σ < 6 渲染像素时做两遍可分离卷积（源行缓存在 workgroup 内存，每线程算 4 个输出）；σ ≥ 6 时先块平均降采样到 1/2ⁿ（层上 σ 保持 3–6），在低分辨率上卷积，再双线性上采样，层上 σ 扣除降采样和上采样引入的方差。降采样网格对齐到 2ⁿ 的倍数，所以不同 ROI 分块渲染的结果一致。高斯模糊（旧版）不转线性光，σ 按实测表插值。

8 / 16 bpc 工程里每个效果的输出截断到 [0, 1]（AE 整数缓冲的行为），32 bpc 不截断（`EffectParams.float`）。

## 测试与基准

- `cargo test -p bae-effects`：GPU 结果与 CPU 参考实现（AE 规格公式）比较，覆盖 RGBA32F / RGBA16F、半分辨率、Repeat Edge Pixels、ROI 分块一致、identity；软件适配器（CI 上的 WARP）容差放宽 4 倍。
- `cargo bench -p bae-effects`：4K / 1080p 每帧耗时；`-- <过滤词> --steps` 打印每个 dispatch 的 GPU 时间。
- 与 AE 对照：`tests/ae/golden/` 是 AE 26.2.1 的输出，`tests/ae_golden.rs` 每次测试都比较（每个用例限定最大差和平均差）；重新生成和结论见 `tests/ae/README.md`。

<<<<<<< HEAD
## 生成、杂色和过渡

| 类别 | 效果 | AE matchName |
|---|---|---|
| 生成 | 填充、梯度渐变、四色渐变、网格 | `ADBE Fill`、`ADBE Ramp`、`ADBE 4ColorGradient`、`ADBE Grid` |
| 杂色和颗粒 | 分形杂色、杂色、添加颗粒 | `ADBE Fractal Noise`、`ADBE Noise2`、`VISINF Grain Implant` |
| 过渡 | 线性擦除、径向擦除、百叶窗、渐变擦除、块溶解 | `ADBE Linear Wipe`、`ADBE Radial Wipe`、`ADBE Venetian Blinds`、`ADBE Gradient Wipe`、`ADBE Block Dissolve` |

渐变擦除目前读取当前图层的亮度。外部渐变图层、添加颗粒的外部蒙版图层、填充的指定蒙版和所有蒙版暂不支持。相关选择器及依赖它们的设置在参数定义里保留并禁用，等待渲染图接入额外输入。无蒙版填充以素材原有的 alpha 为覆盖率，羽化对它无效。

分形杂色支持四种采样方式、分数复杂度、种子、演化与循环。AE 与 BAE 分别在相同的循环起止点复现自己的图像。同种子在两端会生成不同纹理。分形类型中的脏污到线程（菜单值 12–20）暂按基本型渲染。添加颗粒的胶片预设使用 AE 色阶图实测的 RGB 颗粒强度，噪点位置与 AE 有差异。各效果的实测范围见[对照记录](tests/ae/README.md)。
=======
### 颜色校正

曲线、颜色平衡 (HLS)、更改颜色、更改为颜色、通道混合器、阴影/高光均在 GPU 上处理像素。曲线的参数 1 使用 `ParamKind::CurvePoints`，值为 `Value::FloatList`。每三个数依次表示通道、输入、输出。通道 0 为 RGB 总曲线，1 至 4 依次为红、绿、蓝、Alpha，坐标范围为 0 到 1。例如 `[1, 0, 1, 1, 1, 0]` 反转红色，其余通道保持恒等。先应用分通道曲线，再应用总曲线。参数 2 选择编辑通道。曲线编辑器待接入，导入 AE 自定义曲线数据也待接入。

阴影/高光从本帧的亮度直方图估算自动数量，用邻域亮度控制阴影和高光的调整范围。瞬时平滑与场景检测保留 AE 参数和默认值。两项控件暂时禁用，等待渲染图提供同一效果之前的相邻帧。
>>>>>>> 06264ae (effects: add six AE color corrections and GPU reference tests)
