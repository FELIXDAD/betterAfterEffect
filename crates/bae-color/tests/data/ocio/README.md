# OCIO 对照数据

由 `crates/bae-color/tools/gen_ocio_vectors.py`（PyOpenColorIO 2.6）生成，不要手改。

- `studio-config-v5.0.0_aces-v2.1_ocio-v2.6.ocio`、`cg-config-v2.2.0_aces-v1.3_ocio-v2.4.ocio`：OpenColorIO 自带的 ACES 配置（OpenColorIO-Config-ACES，BSD-3-Clause，Copyright Contributors to the OpenColorIO Project），用 `Config.serialize()` 原样导出。
- `handmade.ocio` 和 `*.cube` / `*.spi1d` / `*.spi3d`：脚本生成的测试配置和 LUT，覆盖 bae-color 支持的每种变换。
- `vectors.json`：OCIO（`OPTIMIZATION_LOSSLESS`，不用快速 pow 近似）对各色彩空间转换和显示/视图转换的计算结果。
