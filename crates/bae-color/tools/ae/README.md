# 与 After Effects 对照

用 After Effects 2026（26.2.1）的色彩管理渲染同一张色卡，和 bae-color 的结果逐像素比较。

```sh
python3 crates/bae-color/tools/ae/make_charts.py /private/tmp/bae-color-ae      # 16 位 PNG 色卡，用 cICP 标记色彩空间
tools/ae/aectl run crates/bae-color/tools/ae/render_cases.jsx                     # AE 渲染各用例（16 位 PNG）
cargo run -q -p bae-color --example ae_compare > /private/tmp/bae-color-ae/bae.json
python3 crates/bae-color/tools/ae/compare.py /private/tmp/bae-color-ae
```

AE 的脚本接口不能设置"解释素材 > 覆盖媒体色彩空间"和输出模块的色彩设置，所以素材的色彩空间靠 PNG 的 cICP 块让 AE 自己识别；输出模块用隐藏模板 `_HIDDEN X-Factor 16`（16 位 PNG），输出色彩空间是默认的"工作色彩空间"。OCIO 模式下 AE 不向脚本暴露工作空间和输出空间，无法控制变量，所以 OCIO 改为直接用 AE 安装包里的三个配置文件和 OpenColorIO 库对比（`tests/ocio.rs`）。

## 结果（2026-11-20，Adobe 色彩引擎，32 bpc 工程）

误差是归一化码值（16 位 PNG 的 1 LSB = 1.5e-5）。

| 用例 | 工作空间 | 素材 | bae 模型 | 最大误差 | 平均误差 |
|---|---|---|---|---|---|
| a_srgb_identity | sRGB IEC61966-2.1 | sRGB | srgb → srgb | 1.1e-4 | 1.5e-5 |
| a_untagged_in_srgb | sRGB | 未标记 | 按工作空间，不转换 | 3.1e-5 | 6.5e-6 |
| a_p3_to_srgb | sRGB | Display P3 | display-p3 → srgb | 1.4e-4 | 1.5e-5 |
| a_rec709_to_srgb_comp | sRGB（补偿场景引用：开） | BT.709 标签 | rec709-gamma24 → srgb | 1.3e-4 | 1.7e-5 |
| a_rec709_to_srgb_nocomp | sRGB（补偿：关） | BT.709 标签 | rec709-gamma24 → srgb | 1.3e-4 | 1.7e-5 |
| a_srgb_linearized | sRGB + 线性化 | sRGB | 输入 → 线性缓冲 → 输出回 sRGB | 1.1e-4 | 1.5e-5 |
| a_srgb_to_g24 | Rec.709 Gamma 2.4 | sRGB | srgb → rec709-gamma24 | 3.6e-3 ※ | 2.6e-5 |
| a_srgb_to_acescg | ACES CG Linear (AP1) | sRGB | srgb → acescg | 7.7e-5 | 1.5e-5 |
| a_srgb_to_p3 | Display P3 | sRGB | srgb → display-p3 | 4.2e-5 | 1.2e-5 |
| a_srgb_to_pq_w100 | Rec.2100 PQ W100 | sRGB | srgb → rec2100-pq | 9.6e-5 | 6.1e-5 |
| a_srgb_to_pq_w203 | Rec.2100 PQ W203 | sRGB | srgb → rec2100-pq | 9.6e-5 | 6.1e-5 |
| a_pq_to_srgb | sRGB | PQ（Rec.2020） | rec2100-pq → srgb | 6.7e-4 | 4.6e-5 |
| a_rec2020_to_srgb | sRGB | BT.2020 标签 | rec2020-gamma24 → srgb | 3.2e-4 | 1.4e-5 |

※ 只出现在某个通道正好为 0 的像素上：AE 的 ICC 定点矩阵在两个 Rec.709 原色的空间之间留下 ~1e-6 的串扰，经过 1/2.4 次方被放大到 3.6e-3（例如 sRGB (0.74, 0, 0.68) → AE 输出 G = 0.0036）。bae 的结果是精确值。

## 从 AE 里测出来、已经写进 bae-color 的行为

- 视频的 BT.709 / BT.2020 传递函数标签（H.273 1、6、14、15），AE 按 BT.1886 Gamma 2.4（显示参考）解释，和"补偿场景引用的配置文件"开关无关。`SourceTags::from(&bae_media::ColorInfo)` 照此映射。
- 线性化工作空间时，输出模块默认把缓冲区转回工作空间本来的曲线（`ColorContext::default_output_space()`）。
- 未标记的素材按工作空间处理，不转换。
- PQ：线性 1.0 = 100 cd/m²，与 AE 的 "Rec.2100 PQ W100" 一致；AE 26.2 的 "W203" 对 SDR 素材的编码与 W100 相同。
- sRGB ↔ ACEScg 走 Bradford 适应，与 AE 的 ICC 路径（经 D50 PCS）结果相同。
