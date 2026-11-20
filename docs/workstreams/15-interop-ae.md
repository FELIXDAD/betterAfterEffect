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

- [x] 导出脚本 v0（`bridges/ae/bae-export.jsx`）：合成、图层、变换关键帧（时间缓动、空间切线、连续/自动贝塞尔/漂浮、标签）、表达式、遮罩、形状内容、文字（源文本、动画器、选择器）、效果（matchName + 参数，含图层/遮罩参数）、标记、素材清单与收集、导出报告。模拟 DOM 测试（`bridges/ae/tests/`，约 1.8 万项检查 + 金样）；已在 AE 26.2.1 上实测（见下）。
- [x] 交换格式草案 v0.1（`docs/interop/bae-interchange.md` + `bae-interchange.schema.json`）：容器直接用 02 的 `PrimTree`/`AttrSpec`/`Keyframe`/`Value` JSON；AE 属性名用可逆编码的 `ae:` 名字。用 dev 上的 `bae-model` 实测：示例文件经 `Op::RestorePrim` 写入 Sublayer、读回、重新序列化逐字段一致（`text_document` 除外，见请求）；素材 Prim 带 02 的 `file:*` 引用属性，`AssetFile::from_prim` 能直接读。向 02 报告的 `PrimTree` 反序列化 bug 已在 PR #9 修复并复核。
- [x] 给 03 的对照样例：格式（`docs/interop/ae-anim-samples.md` + schema）、62 个用例（`bridges/ae/samples/anim-cases.json`，由 `gen_anim_cases.py` 生成）、AE 采样脚本 `bridges/ae/bae-anim-samples.jsx`。AE 26.2.1 参考数据已生成并提交（`bridges/ae/samples/anim-samples.ae-26.json`）。
- [x] 真机一键实测脚本 `bridges/ae/tests/ae/run-in-ae.jsx`（T1 场景、T4 时间基准、T8 对照用例；全程不弹框，用 `tools/ae/aectl` 运行）。
- [ ] BAE 侧导入器（`crates/bae-interop-ae`，02 已合并，可以开始）
- [ ] 导回 AE 的 .jsx（`bae-anim-samples.jsx` 里的 `applyAttr` 是它的雏形）
- [ ] Lottie
- [ ] .aep 直接读取

### 真机实测（AE 26.2.1，macOS，2026-11-20）

- 用 `tools/ae/aectl` 跑 `run-in-ae.jsx`：21 步全部通过，62 个对照用例全部成功，约 2 秒。结果已提交：`bridges/ae/examples/ae26-fixture.bae-interchange.json`、`bridges/ae/samples/anim-samples.ae-26.json`。真机导出能被 dev 上的 `bae-model` 直接读回（1757 个属性）。
- T4 确认：`keyTime()` 返回合成时间（图层开始时间偏移、伸缩 200% 都一样），`key_time_base = "comp"` 不用改。
- 实测发现并已修复：颜色控制的 `isSpatial` 报 true 导致 3 个颜色用例失败（改成按值类型判断）；文字可变字体轴的值类型是手册里没有的 161（改成按值的形状导出，每种只报一次警告）；macOS 上文件默认 CR 换行；`/tmp` 是符号链接导致 `Folder.create()` 失败；ExtendScript 数组越大 `push` 越慢，把 400 KB 的 JSON 拼进一个数组要 50 秒（改成每层各自拼接后降到 1 秒以内）；用例 `expr_vector_position` 在二维图层上用了 `value[2]`。
- 新发现，已告知 02/03：AE 的 29.97 / 23.976 按 `n / 29.97` 计帧（内部 2997/100），不是 30000/1001，时间不是整数 flicks（交换格式 4.2 写了导入时的吸附规则）。
- 还没测：其他 AE 版本、Windows、README 的 T2/T3/T5/T6/T7 手动项。

### 与 02 Prim schema v1 对齐（交换格式 v0.2，2026-11-20）

- 设置项改用 02 schema 的名字、值类型和枚举 token（`time`、`frame_rate`、`text_document`、`bit_depth`、`track_matte: none`、`auto_orient: off`、素材 `kind: footage`、`solid_color`、`light:type`、`sampling`……），02 没有的 AE 专有设置写成 `ae:<脚本接口属性名>`；效果参数写成 `param:NNNN`；遮罩羽化点按 02 的 `FeatherPoint` 导出（不再进「不支持」）。
- 属性树（变换、遮罩路径、文字源文本、形状参数）仍是 `ae:` + matchName，导入器查 02 schema 映射；关键帧仍是合成时间，导入器按 02 的约定换成图层时间。
- 用 dev 上的 `bae-model` 实测：示例直接反序列化（不再替换 `text_document`），02 schema 认识的 453 个属性都通过 `schema::check_value`（只有夹具里故意构造的未知遮罩模式不通过），`RestorePrim` 往返一致。

### 待决定（先按默认做了）

1. 交换文件里动画属性用 AE 词汇（`ae:` + matchName），由 Rust 导入器映射到 BAE 属性名；而不是让 AE 脚本直接写 BAE 属性名。理由：ExtendScript 没法在 CI 里测，映射逻辑放在 Rust 里只有一份、能写单元测试；BAE 暂不支持的属性也能原样保留、导回 AE 不丢。
2. 金样 `bridges/ae/examples/mock-project.bae-interchange.json` 约 190 KB（缩进 JSON），作为测试基准提交进仓库。
3. AE 26.2.1（macOS）已经测过；其他版本（建议 CC 2018、2020、2023）和 Windows 由谁、什么时候测。
4. 真机导出示例（约 440 KB）和 AE 26 对照数据（约 450 KB）作为测试输入提交进仓库。

## 请求（其他工作流在这里留言）

### 03 core/animation → 15（2026-11-20）

AE 26.2.1 的对照数据已经用上了（`crates/bae-anim/tests/fixtures/ae-26.2-anim.json`），谢谢。几处问题：

1. **颜色用例在 AE 里搭建失败**（`color_linear`、`color_ease`、`color_hold`）：`bae-anim-samples.jsx` 第 167 行对颜色属性调用了 `setSpatialTangentsAtKey`，AE 报 "This property does not have a spatial PropertyValueType"。应该只在 `isSpatial` 的属性上设空间切线。修好后请重新采样这三个用例（AE-ANI-034 颜色速度单位要靠它定）；03 也可以自己用 aectl 补采。
2. **29.97 / 23.976 fps 的关键帧时间**：`comp_29_97` 里第 30 帧的关键帧，AE 报告的 `keyTime` 是 1.001001 s（= 30 / 29.97），不是 1.001 s（30 × 1001 / 30000）；出点是 120 / 29.97 s。导入器把这类时间换成 flicks 时，建议吸附到合成帧率（`FrameRate::snap`，30000/1001）上，否则关键帧会落在帧与帧之间约 1 µs 的位置。
3. **倒放图层**（`layer_stretch_reverse`）：AE 报告的关键帧时间是 −5512 flicks 和 2 s − 5512 flicks，入点 −235200 flicks（−1/3000 s），都是 AE 内部时间量化的结果。导入时同样建议吸附到帧。
4. `other` 里的属性（`expr_cross_property` 等用的位置、滑块）没有从 AE 读回，03 用的是用例里的输入值；如果方便，采样结果里也带上它们读回的 `attr`。

