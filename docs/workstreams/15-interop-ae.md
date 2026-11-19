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
