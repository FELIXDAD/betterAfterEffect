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

- [x] 插值与变换：线性、贝塞尔（每维速度/影响力）、定格（含只有入点定格）、空间贝塞尔（弧长计时）、漂移关键帧、单独尺寸、不透明度截断；2D/3D 变换矩阵（AE-ANI-013/014）
- [x] 父子级与 3D：父级矩阵、2D 子图层压平 3D 父级（AE-ANI-016）、循环父级截断、摄像机/灯光（目标点朝向）、沿路径定向、朝向摄像机
- [x] 时间重映射与预合成时间：开始时间、伸缩（含倒放）、时间重映射、预合成"保留帧速率"吸附；运动模糊采样时间
- [x] 表达式引擎：rquickjs，每线程一个运行时；自写 JS 改写器（向量运算、完成值、AE 名字局部化）；常用 API（见 README）；错误挂到属性、循环截断、跨图层依赖 `depends_on`。AE 对照：不读位置的 18 个表达式用例 < 1e-6（`velocity` 7e-9）
- [x] EvaluatedComp：接口 PR（`[interface]`），`Evaluator` 缓存 + `invalidate(&StageEvent)`；说明见 `crates/bae-anim/README.md`
- [x] AE 对照测试：AE 26.2.1 的 59 个用例（夹具 `crates/bae-anim/tests/fixtures/ae-26.2-anim.json`，407 KB）。时间插值全部 < 1e-8；29.97/23.976 fps 和倒放用例约 3e-4（AE 自身的时间量化，见下）
- [ ] 空间插值与 AE 差最多约 0.02 像素，空间用例暂用 0.05 像素容差。已查明一部分：AE 的 `valueAtTime` 把时间量化到 1/(帧率×1024) 秒；匀速参数化的直线量化后误差 < 1e-7。剩下的是 AE 弧长参数化的近似（误差是段间平滑弧线、幅度约 1/64 像素以内、路径越长分段越多），继续反推
- [x] 颜色属性的缓动速度单位：用 aectl 自己补采了 6 个颜色用例（含 3 个非零速度），确认是 RGBA 差值 ×255 的欧氏距离（AE-ANI-034），误差 < 1e-12。采样脚本 `crates/bae-anim/tests/fixtures/ae-color-samples.jsx`

性能（Apple M4，release）：1000 个带关键帧的图层（位置空间贝塞尔、缩放、旋转、不透明度、父级）单帧约 0.4 ms；表达式 100 层 × 2 个（`wiggle` + 读上一层）单帧 1.07 ms。表达式递归最多 24 层，更长的依赖链用多轮预热解决（150 层正反两个方向的链有测试）。

## 请求（其他工作流在这里留言）

### 04 render/compositor → 03（2026-11-20）

04 的渲染输入是 `bae_render::RenderComp` / `RenderLayer`（`crates/bae-render/src/input.rs`，纯数据，不依赖 GPU）。合并前 04 用 `MockEvaluator`（只做静态值和线性关键帧）。希望 `EvaluatedComp` 至少给出下面这些，04 再写一个转换适配器（或者 03 直接产出 `RenderComp`，都可以）：

1. 合成：路径、求值时间、宽高、像素宽高比、帧率、背景色。
2. 图层按 `layers` 顺序（第一个 = 最上面），**包括眼睛关掉但被用作轨道遮罩的图层**。每层：
   - 图层时间（`(t − start_time) × 100 / stretch`；04 用它做缓存精确失效，图层关键帧的 `AttrChanged.range` 是图层时间）；
   - 最终的世界矩阵（图层像素 → 合成像素，已含锚点、位置、缩放、旋转、父级；3D 图层以后给 4×4）；
   - 不透明度（0–1）、视频开关、是否在入出点内（含独奏、参考图层规则）、调整图层；
   - 混合模式、保留基础透明度、轨道遮罩（遮罩图层的序号 + 模式，`track_matte_layer` 为空时取上一层）；
   - 内容：纯色（颜色、宽高）、素材（Asset 路径、解析后的文件、素材内时间）、预合成（**按映射后的时间递归求值好的嵌套合成**）、形状/文字（交给 05 的求值数据）；
   - 遮罩（路径已求值、模式、反转、不透明度、羽化、扩展）、效果（match name、开关、按 `param:NNNN` 序号的参数值）。
3. 运动模糊：每层的采样时间点（快门角度/相位），04 按这些时间多次合成。
4. 表达式依赖：如果某层的值依赖别的图层（`thisComp.layer(...)`），请把依赖关系也报出来，04 的帧缓存要据此失效。

### 15 interop/ae → 03（2026-11-20）

AE 对照样例的格式和生成工具已经在 `interop/ae` 分支：`docs/interop/ae-anim-samples.md`（格式与使用建议）、`bridges/ae/samples/anim-cases.json`（62 个用例：缓动、空间插值、漂浮、颜色、图层时间、帧率、确定性表达式）、`bridges/ae/bae-anim-samples.jsx`（在 AE 里采样）。**AE 参考数据还没生成**（本机没有 AE），生成后放在 `bridges/ae/samples/anim-samples.ae-<版本>.json`。需要更多用例（例如某个规格条目标了 UNVERIFIED）请在这里留言，或直接改 `gen_anim_cases.py`。

### 15 interop/ae → 03（2026-11-20，补充）

AE 26.2.1 的参考数据已提交：`bridges/ae/samples/anim-samples.ae-26.json`（62 个用例全部成功）。求值时要注意的几点写在 `docs/interop/ae-anim-samples.md` 第 4 节：AE 的 29.97 按 `n / 29.97` 计帧（不是 30000/1001）、自动贝塞尔和漂浮关键帧要用文件里读回的 `attr`、不透明度在插值后截断、二维图层的位置在表达式里只有两个分量。需要别的 AE 版本或更多用例，用 `bridges/ae/tests/ae/run-in-ae.jsx` 一键重跑（`tools/ae/aectl`，本机 AE 同一时间只能一个人用）。


### 04 render/compositor → 03（2026-11-20，接入 EvaluatedComp 之后）

04 已经用 `bae_render::AnimEvaluator` 接上 `Evaluator`（`crates/bae-render/src/anim.rs`），`bae render` 默认用它。缺的字段：

1. `EvaluatedLayer` 没有「保留基础透明度」（schema 的 `preserve_transparency`，#19 已加）。04 现在直接从 Stage 读静态值；建议放进 `LayerSwitches`。
2. 3D 图层目前按正交投影画（取 `world_matrix` 的 2D 部分）。摄像机投影由 04 做，需要的 `camera` / `world_matrix` 已经有了，不需要改。
