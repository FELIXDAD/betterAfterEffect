# 06 · 色彩管理

| 项 | 内容 |
|---|---|
| 分支 | `render/color` |
| Orca 工作区 | `bae-06-color` |
| 阶段 | P1 |
| 依赖 | 04（渲染图里的色彩变换节点） |
| 负责目录 | `crates/bae-color/` |

## 目标

在不引入 OpenColorIO 的前提下，提供专业、正确、在 GPU 上运行的色彩管理。

## 要做

- **色彩空间库**：原色矩阵、白点、传递函数（sRGB、Rec.709、Gamma 2.2/2.4、PQ、HLG、线性），常见 Log 曲线分批（S-Log3、LogC3/4、V-Log、Canon Log）。
- **GPU 变换**：根据源/目标空间生成 WGSL 变换片段，能拼进 04 的渲染图；相邻变换自动合并。
- **工作空间设置**：架构文档第 6 节列出的工作空间；兼容 AE 的"非线性混合"和"线性化工作空间"。
- **素材解释**：根据元数据自动识别素材的色彩空间，也可以手动覆盖。
- **视图与显示**：视图变换（sRGB、Rec.709、ACES 输出变换）；读取系统显示器色彩空间（macOS ColorSync，Windows Advanced Color）；HDR 预览（macOS EDR、Windows HDR）。
- **OCIO 配置子集**：解析 config.ocio 的 YAML，支持矩阵、传递函数、1D/3D LUT（.cube、.spi1d、.spi3d）以及 ACES 内置变换。
- **界面颜色**：给 11 提供"界面颜色也经过显示变换"的工具函数，保证拾色器里的颜色和画面一致。

## 对外接口

- `ColorSpace`、`ColorContext { working, view, display }`
- 给 04 的变换节点生成器；给 08 的素材色彩空间识别。

## 验收

- 与 colour-science 生成的测试向量对比，误差 < 1e-4。
- 色卡往返测试（转到工作空间再转回来）误差 < 1e-5。
- 在 P3 显示器和 sRGB 显示器上看到的颜色一致。

## 不做

- 调色效果（曲线、色阶等属于 07）。

## 进度

**2026-11-20**

- PR 1 [interface]：`crates/bae-color` 公共接口与第一批实现，用法见 `crates/bae-color/README.md`。
  - 色彩空间库：sRGB、Rec.709、Gamma 2.2/2.4/1.8、线性、Display P3、DCI-P3、Rec.2020、Rec.2100 PQ/HLG、ACEScg、ACES2065-1、ACEScct/ACEScc、Adobe RGB、S-Log3（S-Gamut3 / .Cine）、LogC3（EI800）、LogC4、V-Log、Canon Log 3、CIE XYZ D65。参考空间是 ACES2065-1（与 OCIO ACES 配置相同）；相机空间按厂商 IDT 用 CAT02（Sony/ARRI/Canon）或 Bradford（Panasonic）。
  - `ColorTransform`：相邻步骤自动合并（矩阵相乘、互逆曲线抵消、gamma 合并、钳制求交），CPU f64 参考实现。
  - `to_wgsl(prefix)`：自包含的 WGSL 片段 + vec4 数组 uniform + Rgba32Float LUT 纹理；结构哈希做管线缓存键。
  - `WorkingSpace` / `ColorContext`：AE 的非线性混合、线性化工作空间、使用 1.0 灰度系数混合颜色；显示目标（直接写、硬件 sRGB、EDR/scRGB 扩展线性、HDR10）；渲染队列输出变换。
  - 素材解释：文件标签自动识别（`media` feature 接 bae-media 的 `ColorInfo`）、覆盖、保留 RGB、解释为线性光，优先级与 AE 一致。
  - 验证：colour-science 向量 ≤ 2.6e-6；OCIO 2.6 内置 ACES studio 配置 ≤ 2.4e-5；GPU 与 CPU ≤ 9.7e-5（PQ）/ 1.6e-6（其他曲线）；内置空间两两往返 < 1e-9。
- PR 1 已合并（#28）。
- PR 2：OCIO 子集与 AE 对照。
  - `ocio::Config`：config.ocio（v1、v2）的色彩空间、显示色彩空间、角色、别名、视图变换、显示/视图（含共享视图）、文件规则；变换 Group / Matrix / Exponent / ExponentWithLinear / Log / LogAffine / LogCamera / Range / CDL / File / ColorSpace / Builtin；YAML 用自写的子集解析器（无新依赖）。
  - 新 op（CPU + WGSL）：逐通道幂函数、ExponentWithLinear、Log/LogAffine/LogCamera、CDL、1D LUT 精确反求（二分查找）。
  - LUT 文件：.cube（1D / 3D / shaper+3D）、.spi1d、.spi3d。
  - 与 OpenColorIO 2.6 对比：自制配置、ACES studio 2.1、ACES 1.3 CG、AE 自带的三个配置，共 700 多组转换，最大误差 5.7e-6。
  - 与 AE 26.2 逐像素对比（`crates/bae-color/tools/ae/`）：13 个 Adobe 引擎用例全部吻合（≤ 1.4e-4，16 位 PNG 精度；一处 3.6e-3 是 AE 自己的 ICC 定点串扰）。实测得出并已实现：BT.709 / BT.2020 标签的视频按 Gamma 2.4 解释；线性化工作空间时输出模块转回工作空间曲线；PQ 线性 1.0 = 100 nit（= AE 的 W100）。
- 下一步：ACES 输出变换（1.x RRT+ODT 与 2.0），作为视图变换；界面颜色工具函数；OCIO 的 Look / NamedTransform。

- [x] 色彩空间库与测试向量
- [x] GPU 变换生成
- [x] 工作空间与素材解释（Adobe 引擎与 OCIO 引擎）
- [ ] 显示器与 HDR（bae-color 侧的 `DisplayTarget` 已有；系统显示器信息等 01 的接口，见 01 任务书的请求）
- [x] OCIO 子集与 LUT（ACES 输出变换除外）

## 请求（其他工作流在这里留言）

### 20 interop/aex-compat → 06（2026-11-20）

AE 插件看到的是工作色彩空间里的编码值，宿主不替它转换。详见 `docs/spec/aex-compat.md` §3.6、§5.4。

1. **R06-01 可查询的工作色彩空间描述（现在）。** 原色、白点、传递函数、是否线性化、"用 1.0 gamma 混合"、是否启用色彩管理、ICC 配置文件字节、OCIO 来源（配置、工作空间、显示 / 视图），渲染节点和兼容层都能读到；颜色参数值能无损转换到这个编码。
2. **R06-02 缓冲区保持工作空间自己的编码（现在，与 04 一起）。** 包括 AE 默认的非线性 sRGB；"用 1.0 gamma 混合"是混合操作的选项，不在节点之间重新编码缓冲区。请提供 GPU 片段（带零 alpha 保护的去预乘 → 传递函数 / 矩阵 → 重新预乘），只在 CPU 节点需要不同编码时由 04 插入。
