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

- [x] 导出脚本 v0（`bridges/ae/bae-export.jsx`）：合成、图层、变换关键帧（时间缓动、空间切线、连续/自动贝塞尔/漂浮、标签）、表达式、遮罩、形状内容、文字（源文本、动画器、选择器）、效果（matchName + 参数，含图层/遮罩参数）、标记、素材清单与收集、导出报告。**未在 AE 中验证**：只在模拟 DOM 上测过（`bridges/ae/tests/`，约 1.3 万项检查 + 金样），手动测试步骤见 `bridges/ae/README.md` 第 4 节。
- [x] 交换格式草案 v0.1（`docs/interop/bae-interchange.md` + `bae-interchange.schema.json`）：容器直接用 02 的 `PrimTree`/`AttrSpec`/`Keyframe`/`Value` JSON；AE 属性名用可逆编码的 `ae:` 名字。用 dev 上的 `bae-model` 实测：示例文件经 `Op::RestorePrim` 写入 Sublayer、读回、重新序列化逐字段一致（`text_document` 除外，见请求）；素材 Prim 带 02 的 `file:*` 引用属性，`AssetFile::from_prim` 能直接读。向 02 报告的 `PrimTree` 反序列化 bug 已在 PR #9 修复并复核。
- [x] 给 03 的对照样例：格式（`docs/interop/ae-anim-samples.md` + schema）、62 个用例（`bridges/ae/samples/anim-cases.json`，由 `gen_anim_cases.py` 生成）、AE 采样脚本 `bridges/ae/bae-anim-samples.jsx`。**AE 参考数据还没生成**，等有 AE 的人跑 README 的 T8。
- [ ] BAE 侧导入器（`crates/bae-interop-ae`，02 已合并，可以开始）
- [ ] 导回 AE 的 .jsx（`bae-anim-samples.jsx` 里的 `applyAttr` 是它的雏形）
- [ ] Lottie
- [ ] .aep 直接读取

### 受阻

- 两个脚本还没在 AE 里运行过；交换格式里「关键帧时间是合成时间」（`key_time_base = "comp"`）也是按脚本文档推断的，要用 README 的 T4 核实。2026-11-20 本机已装 AE 2026（26.2.1），等 `osascript` 调 `DoScriptFile` 的权限处理好后实测。

### 待决定（先按默认做了）

1. 交换文件里动画属性用 AE 词汇（`ae:` + matchName），由 Rust 导入器映射到 BAE 属性名；而不是让 AE 脚本直接写 BAE 属性名。理由：ExtendScript 没法在 CI 里测，映射逻辑放在 Rust 里只有一份、能写单元测试；BAE 暂不支持的属性也能原样保留、导回 AE 不丢。
2. 金样 `bridges/ae/examples/mock-project.bae-interchange.json` 约 190 KB（缩进 JSON），作为测试基准提交进仓库。
3. 谁、在哪些 AE 版本上做第一次真机测试（建议 CC 2018、2020、2023 和当前版本各一次）。

## 请求（其他工作流在这里留言）
