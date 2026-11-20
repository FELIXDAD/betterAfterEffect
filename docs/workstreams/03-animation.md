# 03 · 动画求值与表达式

| 项 | 内容 |
|---|---|
| 分支 | `core/animation` |
| Orca 工作区 | `bae-03-animation` |
| 阶段 | P1 |
| 依赖 | 02 |
| 负责目录 | `crates/bae-anim/` |

## 目标

给定工程和时间，算出这一帧每个图层的最终状态。行为和数值与 AE 一致，包括 AE 表达式。

## 要做

- **关键帧插值**：线性、贝塞尔（AE 的速度/影响力模型）、定格；空间贝塞尔（位置路径），包括漂移关键帧（roving）；位置分离维度。
- **变换**：锚点、位置、缩放、旋转、不透明度；3D 方向和 X/Y/Z 旋转；父子级；3D 图层和摄像机（AE 的 2.5D）。
- **时间**：时间重映射、预合成的时间偏移和伸缩、合成帧率与素材帧率不同时的取帧规则。
- **表达式**：嵌入 rquickjs，按 AE 的 JavaScript 表达式引擎语义实现常用 API：
  `time`、`value`、`thisComp`、`thisLayer`、`thisProperty`、`index`、`wiggle`、`loopIn`/`loopOut`、`valueAtTime`、`linear`/`ease`/`easeIn`/`easeOut`、`random`/`seedRandom`、`effect("名")("参数")`、`layer.toComp`/`fromComp`、`length`、`clamp`、向量运算等。
  - 求值结果缓存，记录依赖关系，依赖变了才重新算。
  - 表达式出错时把错误信息挂到对应属性上，供界面显示（和 AE 一样）。
- **运动模糊采样**：按快门角度和相位给出采样时间点，与 AE 一致。
- **输出 `EvaluatedComp`**：这一帧扁平化后的结果，包括每个图层的世界矩阵、不透明度、可见性、效果参数值、遮罩路径。04 只读它，不再自己求值。

## 对外接口

- `evaluate_comp(&Stage, comp: &PrimPath, time: Time) -> EvaluatedComp`
- `evaluate_attribute(&Stage, attr, time) -> Value`（给界面和 Agent 查询用）

## 验收

- 和 AE 对照测试：用 AE 生成一组关键帧和表达式样例的数值（可以请 15 的导出脚本帮忙导出），误差 < 1e-3。
- 1000 个带关键帧的图层，单帧求值 < 2 ms（不含表达式）。
- 不依赖 GPU 和窗口。

## 不做

- 文字动画器的排版部分（05）；渲染（04）。

## 进度

- [ ] 插值与变换
- [ ] 父子级与 3D
- [ ] 时间重映射与预合成时间
- [ ] 表达式引擎
- [ ] EvaluatedComp
- [ ] AE 对照测试

## 请求（其他工作流在这里留言）

### 15 interop/ae → 03（2026-11-20）

AE 对照样例的格式和生成工具已经在 `interop/ae` 分支：`docs/interop/ae-anim-samples.md`（格式与使用建议）、`bridges/ae/samples/anim-cases.json`（62 个用例：缓动、空间插值、漂浮、颜色、图层时间、帧率、确定性表达式）、`bridges/ae/bae-anim-samples.jsx`（在 AE 里采样）。**AE 参考数据还没生成**（本机没有 AE），生成后放在 `bridges/ae/samples/anim-samples.ae-<版本>.json`。需要更多用例（例如某个规格条目标了 UNVERIFIED）请在这里留言，或直接改 `gen_anim_cases.py`。

