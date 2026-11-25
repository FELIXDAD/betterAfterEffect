# bae-effects

- [效果库接入与测试](docs/usage.md)
- [AE 对照与参考图生成](tests/ae/README.md)
- [AE matchName 与导入计划](../../docs/interop/ae-effects.md)

## 新增效果

在 `src/categories/<分类>.rs` 登记效果和预热函数。新 Rust 内核在该文件用 `mod` 声明，代码放在同名分类目录；已有的分类内核也可直接扩展。需要读其他图层时，使用 `effects::sampled::Shader` 的磁盘 ID 参数和四个可选输入。单 pass 效果声明输入区域和输出边界：逐像素效果返回请求区域，邻域滤波按半径扩大，能取到任意图层坐标的变换才请求图层范围。整数像素中心直接读取，只有子像素采样需要插值。

按 AE 的 640 × 360 图层测得的 Rust 点参数用 `DefBuilder::reference_points()` 标记；声明式包的相对点坐标使用 0–1。效果测试写在相应分类的测试文件，图层应用和重置的用例放在 `tests/stack/<分类>.rs`。

## 效果分类

### [模糊和锐化](tests/ae/categories/blur-sharpen.md)

- [旧版模糊](tests/ae/categories/blur-sharpen.md#旧版)

### [通道](tests/ae/categories/channel.md)

### [颜色校正](tests/ae/categories/color.md)

### [表达式控制](tests/ae/categories/controls.md)

### [扭曲](tests/ae/categories/distort.md)

### [生成](tests/ae/categories/generate.md)

### [抠像](tests/ae/categories/keying.md)

### [遮罩](tests/ae/categories/matte.md)

### [杂色和颗粒](tests/ae/categories/noise-grain.md)

### [透视](tests/ae/categories/perspective.md)

### [风格化](tests/ae/categories/stylize.md)

### [过渡](tests/ae/categories/transition.md)
