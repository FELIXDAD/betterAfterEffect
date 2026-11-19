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

- [ ] 色彩空间库与测试向量
- [ ] GPU 变换生成
- [ ] 工作空间与素材解释
- [ ] 显示器与 HDR
- [ ] OCIO 子集与 LUT

## 请求（其他工作流在这里留言）
