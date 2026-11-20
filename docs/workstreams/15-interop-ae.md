# 15 · AE 工程互转

| 项 | 内容 |
|---|---|
| 分支 | `interop/ae` |
| Orca 工作区 | `bae-15-interop-ae` |
| 阶段 | P0 起步（脚本和交换格式先做），Rust 部分在 02 合并后开始 |
| 依赖 | 02（Prim schema）；效果映射依赖 07 |
| 负责目录 | `crates/bae-interop-ae/`、`bridges/ae/`、`docs/interop/ae-*` |

## 目标

AE 工程能进来，也能回去。让团队里只用 AE 的人照常工作，工程交接不丢东西；丢了什么也要明确告诉用户。

## 要做

- **交换格式**：`docs/interop/bae-interchange.md` + JSON Schema。尽量直接采用 BAE 的 Prim JSON（与 02 对齐），另外附带素材清单。
- **AE 导出脚本（`bridges/ae/`，ExtendScript / ES3，支持 AE CC 2018 及以上）**：遍历工程，导出合成、图层、变换、关键帧（含时间缓动和空间切线）、表达式文本、遮罩、形状图层内容、文字（含动画器）、效果（matchName + 参数）、标记、素材引用，并可选收集素材文件。
- **AE 导入脚本**：读取 BAE 交换文件，在 AE 里重建工程。
- **BAE 侧导入导出器（`bae-interop-ae`）**：交换 JSON ↔ Stage（通过命令）；效果 matchName 映射（与 07 共用 `docs/interop/ae-effects.md`）；不支持的特性自动降级，并生成导入报告，列出丢了什么。
- **直接读取 .aep（第二阶段）**：基于公开的 RIFX 逆向资料，只读，先支持结构（合成、图层、关键帧），没装 AE 的人也能打开。
- **Lottie**：导入和导出 bodymovin JSON。
- **对照表**：`docs/interop/ae-mapping.md`，列出 AE 每个功能在 BAE 里的支持程度（完全 / 近似 / 不支持）。
- **测试工程集**：自己制作一组 AE 测试工程（不使用来源不明的商业模板），做往返测试：AE → BAE → AE 结果一致。

## P0 阶段先做（不依赖 Rust 代码）

1. AE 导出脚本 v0：合成、图层、变换关键帧、形状、文字。
2. 交换格式草案，交给 02 对齐 Prim schema。
3. 帮 03 导出一组关键帧和表达式的数值样例，用于动画求值的对照测试。

## 对外接口

- `import_ae_interchange(path) -> 命令批次`、`export_ae_jsx(stage) -> .jsx`
- 导入报告的数据结构（给界面显示）。

## 验收

- M1：一个包含形状、文字、关键帧缓动、预合成、常用效果的 AE 工程，导入后画面与 AE 渲染结果肉眼一致。
- 往返测试：AE → BAE → AE，关键帧数值误差 < 1e-3。
- 导入报告准确列出所有未支持的项目。

## 不做

- 兼容 AE 第三方插件效果（只记录 matchName 并在报告中标出）。

## 进度

- [ ] 导出脚本 v0
- [ ] 交换格式草案
- [ ] 给 03 的对照样例
- [ ] BAE 侧导入器
- [ ] 导回 AE 的 .jsx
- [ ] Lottie
- [ ] .aep 直接读取

## 请求（其他工作流在这里留言）

### 03 core/animation → 15（2026-11-20）

AE 26.2.1 的对照数据已经用上了（`crates/bae-anim/tests/fixtures/ae-26.2-anim.json`），谢谢。几处问题：

1. **颜色用例在 AE 里搭建失败**（`color_linear`、`color_ease`、`color_hold`）：`bae-anim-samples.jsx` 第 167 行对颜色属性调用了 `setSpatialTangentsAtKey`，AE 报 "This property does not have a spatial PropertyValueType"。应该只在 `isSpatial` 的属性上设空间切线。修好后请重新采样这三个用例（AE-ANI-034 颜色速度单位要靠它定）；03 也可以自己用 aectl 补采。
2. **29.97 / 23.976 fps 的关键帧时间**：`comp_29_97` 里第 30 帧的关键帧，AE 报告的 `keyTime` 是 1.001001 s（= 30 / 29.97），不是 1.001 s（30 × 1001 / 30000）；出点是 120 / 29.97 s。导入器把这类时间换成 flicks 时，建议吸附到合成帧率（`FrameRate::snap`，30000/1001）上，否则关键帧会落在帧与帧之间约 1 µs 的位置。
3. **倒放图层**（`layer_stretch_reverse`）：AE 报告的关键帧时间是 −5512 flicks 和 2 s − 5512 flicks，入点 −235200 flicks（−1/3000 s），都是 AE 内部时间量化的结果。导入时同样建议吸附到帧。
4. `other` 里的属性（`expr_cross_property` 等用的位置、滑块）没有从 AE 读回，03 用的是用例里的输入值；如果方便，采样结果里也带上它们读回的 `attr`。

