# bae-anim：动画求值

工作流 03。给定工程（`Stage`）和时间，算出合成里每个图层这一帧的最终状态，数值与 After Effects 一致。不依赖 GPU、窗口和平台 API。

## 对外接口

| 接口 | 用途 |
|---|---|
| `evaluate_comp(&Stage, comp: &PrimPath, time: Time) -> Result<EvaluatedComp, EvalError>` | 求值一个合成（`time` 是合成时间）。04 渲染只读它的结果。 |
| `evaluate_attribute(&Stage, prim: &PrimPath, attr: &str, time: Time) -> Result<Value, EvalError>` | 求值任意 Prim 的一个属性（界面、Agent 查询）。`time` 是所在合成的时间，图层及其下面的 Prim 会先换成图层时间。没有意见时用 schema 默认值。 |
| `Evaluator` | 同上两个函数，但缓存编译好的合成（关键帧解码、空间路径弧长表、父级和遮罩解析）。每个线程/渲染任务持有一个，收到 `StageEvent` 时调用 `invalidate(&event)`。 |
| `Curve` | 单独的关键帧插值（编译一次，多次取样）。 |
| `time::LayerTiming`、`time::MotionBlur` | 合成时间 ↔ 图层时间；运动模糊采样时间。 |

### `EvaluatedComp`

- 合成设置：尺寸、像素宽高比、帧率、时长、背景色、运动模糊设置。
- `layers`：按 AE 图层顺序，`layers[0]` 是最上面的图层（AE 序号 1）。**所有图层都列出**（包括不可见的、轨道遮罩、父级、摄像机），下标稳定，`parent`、`track_matte_layer`、`camera` 都是这个数组的下标。
- 每个图层（`EvaluatedLayer`）：
  - `transform`：变换各分量（AE 单位：像素、度、百分比）；`local_matrix`（图层空间 → 父级空间）、`world_matrix`（图层空间 → 合成空间，`glam::DMat4`，f64）。
  - `opacity`：0–1（不继承父级）；`transform.opacity` 保留 AE 的百分比。
  - `active`（在入点和出点之间）、`visible`（视频开关打开、active、没有被别的图层的独奏隐藏）、`switches`（独奏、参考图层、调整图层、3D、运动模糊、折叠变换……）。参考图层在这里算可见，最终输出时由渲染器跳过；轨道遮罩图层即使不可见也要用。
  - `source`：`None` / `Asset { asset, time }` / `Comp { comp, time }`，`time` 是已经算上开始时间、伸缩、时间重映射的源时间；预合成如果勾了"保留帧速率"，已经吸附到内部合成的帧上。素材按自身帧率取哪一帧由 08 决定。
  - `blend_mode`、`track_matte`、`track_matte_layer`（没设时是上面一层）。
  - `masks`（路径、模式、反转、羽化、不透明度 0–1、扩展）、`effects`（`param:NNNN` 全部求值，按 `"0001"` 这样的 ID 索引）、`contents`（形状内容的通用树，每个属性都已求值）。
  - 摄像机图层有 `camera`（缩放、景深参数、目标点、`world_matrix`；看向局部 +z，局部 +y 是屏幕向下），灯光图层有 `light`。
- `camera`：当前活动摄像机（最上面一个开着且在入出点内的摄像机图层）；`None` 表示 AE 的默认摄像机，缩放为 `default_camera_zoom(width)`（50mm，`width * 50 / 36`）。
- `errors`：单个属性的求值问题（表达式错误等），界面挂到对应属性上显示；不会让整个求值失败。

`EvaluatedComp` 可以序列化成 JSON（`serde`），方便调试和给 Agent 看。

## 约定

- **时间基准**：图层上的属性（以及它下面的遮罩、效果、形状内容）的关键帧用**图层时间**：`图层时间 = (合成时间 − start_time) × 100 / stretch`（`docs/spec/data-model.md` 第 10 节）。AE 脚本报告的关键帧时间是合成时间，导入时要换算。
- **属性名和默认值**以 `bae_model::schema` 为准；`bae_anim::schema` 里是同样的字符串常量，测试会检查它们都在 02 的 schema 里。
- **空间属性**：图层的锚点、位置、目标点，以及关键帧带空间切线的二维/三维属性（AE 的点控制等），按运动路径插值：一条时间缓动控制沿路径走过的距离。
- **截断**：不透明度（图层、遮罩）在插值之后截断到 0–100，和 AE 一样允许贝塞尔缓动中间过冲。

## 关键帧插值（与 AE 对照的结论）

用 AE 26.2.1 的实测数据（`tests/fixtures/ae-26.2-anim.json`）确认：

- 贝塞尔时间缓动：速度/影响力换成 (时间, 值) 平面上的三次贝塞尔，按时间求参数再取值；多维非空间属性每维一条曲线。
- 线性和贝塞尔混合的段：线性那一侧等于"速度 = 段平均速度、影响力 1/6"的贝塞尔手柄（AE 报告的线性关键帧缓动值也是这样）。误差 1e-9。
- 定格：出点定格保持前一个值；**只有入点定格**（后一个关键帧的入点是定格）时，一离开前一个关键帧就跳到后一个关键帧的值。
- 漂移关键帧：从前一个非漂移关键帧到后一个非漂移关键帧是一条缓动，按总弧长分配，不使用漂移关键帧自己的时间。
- 空间路径的速度手柄超过路径长度时，影响力按 `L / (speed × dt)` 截断（`spatial_speed_clamp` 用例验证）。
- 已知差距：空间路径的弧长参数化与 AE 相差最多约 0.02 像素（AE 的数值方法未公开，直线路径也有这个偏差），对照测试暂时对空间用例用 0.05 像素的容差。颜色属性的缓动速度单位（AE-ANI-034）还没有 AE 数据。

## 测试

```
cargo test -p bae-anim                                   # 单元测试、AE 对照、集成测试
cargo test -p bae-anim --test ae_reference -- --nocapture  # 打印每个对照用例的最大误差
cargo test --release -p bae-anim --test perf -- --nocapture  # 性能验收（1000 个带关键帧的图层，单帧 < 2 ms）
```

对照夹具由 `tests/fixtures/make_ae_fixture.py` 从 15 的 AE 采样结果生成（见 `docs/interop/ae-anim-samples.md`）。

## 还没做

- 表达式（rquickjs，下一个 PR）。
- 运动模糊的自适应采样数、折叠变换的嵌套求值、文字动画器。
