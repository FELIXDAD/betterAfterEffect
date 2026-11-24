# AE 文字动画器对照

`cases.json` 定义同一段 Arial 40 px 点文字的两帧。第一帧用于测量原始字形，第二帧用 12.5% 到 62.5% 的正方形范围选择器，把选中的字符下移 20 px 并把不透明度设为 20%。

`golden/` 中的 RGBA PNG 来自 macOS 上的 AE 26.2.1。逐字符扫描的结果如下。

| 字符（从左到右） | 覆盖率 | 相对原始帧的垂直位移 | 峰值 alpha（8 位） |
|---|---:|---:|---:|
| 第一个 H | 50% | 10 px | 153 |
| 第二个 H | 100% | 20 px | 51 |
| 第三个 H | 50% | 10 px | 153 |
| 第四个 H | 0% | 0 px | 255 |

`cargo test -p bae-text --test animators` 用 CPU 后端渲染这两个用例。测试比较每个字形的相对位移和峰值 alpha，允许 alpha 相差一个量化单位，避免字体平台的抗锯齿差异影响判定。

更新参考图时运行以下命令，再将返回目录中的两张 PNG 复制到 `golden/`。

```sh
tools/ae/aectl run crates/bae-text/tests/ae/render_cases.jsx --timeout 35
```
