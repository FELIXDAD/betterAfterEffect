# bae-color · 色彩管理

工作流 06。不依赖 OpenColorIO，在 GPU 上做专业、正确的色彩管理。本 crate 不依赖 GPU、窗口和平台 API（架构文档第 12 节）；显示器色彩空间和 HDR 状态由 `bae-gpu` / `bae-platform` 提供。

## 概念

| 类型 | 作用 |
|---|---|
| `Chromaticities` | 三原色和白点的 xy 坐标；`rgb_to_xyz()` 由坐标推导（不用 IEC 取整矩阵，与 OCIO ACES 配置一致到 2e-10） |
| `Adaptation` | 白点适应方法：Bradford（默认）、CAT02（Sony/ARRI/Canon 厂商 IDT 用的）、Von Kries、XYZ 缩放 |
| `TransferFunction` | 传递函数：`decode` = 编码值 → 线性光，`encode` 反之。见下表 |
| `ColorSpace` | 色彩空间：原色 + 传递函数（`Rgb`），或显式变换（`Transform`，给 OCIO 和 XYZ 用），或数据（`Data`，不转换）。工程文件里只存 `id` |
| `builtin` | 内置色彩空间库，`builtin::by_id("acescg")` 按 id 查找 |
| `ColorTransform` | 变换 = 一串 `ColorOp`（矩阵、传递函数、1D/3D LUT、钳制）。拼接时相邻的步骤自动合并 |
| `GpuSnippet` | 由 `ColorTransform::to_wgsl(prefix)` 生成：WGSL 代码 + uniform 数据 + LUT 纹理，拼进渲染图的着色器 |
| `WorkingSpace` | 工程色彩设置：工作空间（`None` = 不做色彩管理，AE 默认）、线性化工作空间、使用 1.0 灰度系数混合颜色 |
| `ColorContext` | 一次渲染的色彩上下文：`working`、`view`、`display` |
| `Interpretation` / `SourceTags` | 素材解释：文件里的色彩标签、手动覆盖、保留 RGB、解释为线性光 |

### 参考空间

所有色彩空间都能转到参考空间 **ACES2065-1**（AP0 原色、ACES 白点、场景线性），与 OCIO 的 ACES 配置相同，所以内置空间和从 `config.ocio` 读出来的空间可以混用。`a → b` 的变换是 `a.to_reference()` 接 `b.from_reference()`，合并后中间的白点往返会精确抵消：两个 D65 空间之间只剩一个比色矩阵。

### 内置色彩空间

| id | 名称 | 原色 | 传递函数 |
|---|---|---|---|
| `srgb` | sRGB | Rec.709 | sRGB |
| `srgb-linear` | 线性 sRGB | Rec.709 | 线性 |
| `rec709` | Rec.709（HDTV） | Rec.709 | BT.709 摄像机曲线 |
| `rec709-gamma24` | Rec.709 Gamma 2.4（BT.1886） | Rec.709 | 2.4 |
| `gamma22-rec709` / `gamma18-rec709` | Gamma 2.2 / 1.8 | Rec.709 | 2.2 / 1.8 |
| `display-p3` / `p3-d65-linear` | Display P3 / 线性 P3-D65 | P3-D65 | sRGB / 线性 |
| `dci-p3` | DCI-P3 | P3（DCI 白点） | 2.6 |
| `rec2020` / `rec2020-linear` | Rec.2020 / 线性 Rec.2020 | Rec.2020 | BT.2020 / 线性 |
| `rec2100-pq` / `rec2100-hlg` | Rec.2100 PQ / HLG | Rec.2020 | PQ / HLG |
| `acescg` / `aces2065-1` | ACEScg / ACES2065-1 | AP1 / AP0 | 线性 |
| `acescct` / `acescc` | ACEScct / ACEScc | AP1 | ACEScct / ACEScc |
| `adobe-rgb` | Adobe RGB (1998) | Adobe RGB | 563/256 |
| `slog3-sgamut3` / `slog3-sgamut3cine` | Sony S-Log3 | S-Gamut3 / S-Gamut3.Cine | S-Log3 |
| `logc3-awg3` | ARRI LogC3（EI 800） | ARRI Wide Gamut 3 | LogC3 |
| `logc4-awg4` | ARRI LogC4 | ARRI Wide Gamut 4 | LogC4 |
| `vlog-vgamut` | Panasonic V-Log | V-Gamut | V-Log |
| `clog3-cinemagamut` | Canon Log 3 | Cinema Gamut | Canon Log 3 |
| `cie-xyz-d65` | CIE XYZ D65 | — | 线性 |
| `raw` | 不转换（数据） | — | — |

`<id>~linear`（例如 `srgb~linear`）表示同一原色的线性版本，用于"线性化工作空间"和"解释为线性光"。

传递函数的约定：编码值是归一化码值（`码值 / (2^n − 1)`）；显示曲线对负数做奇对称延伸，超过 1 的部分沿幂函数继续（32 bpc 超范围值不丢）；PQ 的线性 1.0 = 100 cd/m²（与 OCIO 相同）；HLG 是 BT.2100 OETF，不含 OOTF。

## 给 04 渲染图的接法

```rust
use bae_color::{ColorContext, WorkingSpace, builtin, interpret};

// 工程设置（来自 /project prim 的 working_space、linearize、blend_colors_using_gamma_1）
let working = WorkingSpace::from_project_attrs("srgb", false, false, builtin::by_id)?;
let ctx = ColorContext::new(working);

// 1. 素材进图：素材节点上挂输入变换
let tags = bae_color::SourceTags::from(&media_info.color);          // 需要 `media` feature
let interp = bae_color::Interpretation::from_footage_attr(color_space_attr);
let input = interpret::input_transform(&tags, &interp, &ctx.working, bpc, builtin::by_id)?;

// 2. 相邻的色彩节点直接合并
let merged = input.then(&other_color_node);
if merged.is_identity() { /* 整个节点可以去掉 */ }

// 3. 生成 WGSL，拼进采样素材的那个 pass
let snip = merged.to_wgsl("cin");
// 着色器里：snip.wgsl + 自己声明 uniform/纹理 + 调用
//   @group(1) @binding(0) var<uniform> u_cin: cin_Params;
//   let c = cin_apply_premul(textureSample(src, smp, uv), u_cin);
// uniform 数据：snip.params_bytes()；LUT：snip.textures（Rgba32Float、只用 textureLoad，
//   绑定类型 Float { filterable: false }）
// 管线缓存键：snip.key（结构相同 → WGSL 相同，只有 uniform 不同）

// 4. 混合
match ctx.blend_space() {
    bae_color::BlendSpace::Stored | bae_color::BlendSpace::LinearBuffers => { /* 直接混合缓冲区的值 */ }
    bae_color::BlendSpace::LinearBlend => {
        let lin = ctx.blend_linearization().unwrap(); // 混合前 decode、混合后 encode（逐通道曲线）
    }
}

// 5. 视口最后一个 pass：缓冲区 → 交换链（不进帧缓存）
let display = ctx.display_transform()?.to_wgsl("cdisp");

// 6. 渲染队列输出
let out = ctx.output_transform(&builtin::REC709)?;
```

- 一个着色器里可以放多个片段，只要前缀不同。
- `*_apply_premul` 输入输出都是预乘 RGBA；纯矩阵变换不做反预乘。
- 帧缓存键用 `ColorContext::render_fingerprint()`（只含工作空间，显示和视图变换不影响缓存的帧）。

## 精度（本机 Apple M4）

- 曲线与 colour-science 0.4.7 对比：最大相对误差 2.6e-6（Rec.709 是 colour 常数取整的差别，其余 ≤ 3e-15）；空间转换 ≤ 2.6e-6。
- 与 OCIO 2.6 内置 ACES studio 配置（AE 的 OCIO 引擎用的同类配置）对比：≤ 2.4e-5（Gamma 2.2/2.4，OCIO 在 f32 下计算），相机 Log 空间 ≤ 1.1e-5。
- GPU（WGSL，f32）与 CPU 参考（f64）：PQ ≤ 9.7e-5，其他曲线 ≤ 1.6e-6，空间转换 ≤ 1.6e-5（含 PQ 的 4e-5），LUT ≤ 2.4e-7。
- 往返（转到工作空间再转回）：所有内置空间两两之间 < 1e-9。

测试向量由 `tools/gen_vectors.py` 生成（需要 colour-science 和 opencolorio），GPU 测试在没有 GPU 的机器上自动跳过（`BAE_REQUIRE_GPU=1` 时改为失败）。
