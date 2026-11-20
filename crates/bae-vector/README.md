# bae-vector · 矢量

工作流 05。AE 形状图层的求值与 GPU 绘制，同时提供给分镜/设计工作区（13）和 Ai 互转（16）用的路径几何。行为以 `docs/spec/ae/shapes.md`（AE-SHP-nnn）为准。

默认不带任何 GPU 依赖（只依赖 `bae-model`、`bae-anim`、`kurbo`），领域命令 crate 可以直接用；打开 `render` feature 才会引入 Vello、`bae-render`、`bae-gpu`。

## 数据流

```text
bae-anim EvaluatedLayer.contents ──► ShapeGroup（类型化的形状树）
    ──► VectorScene::from_group（AE 运算顺序、路径操作器）──► VectorScene（显示列表）
    ──► render::encode（Vello）──► render::ShapeSource（bae-render 的 LayerSource）
```

## 接口

| 类型 / 函数 | 作用 |
|---|---|
| `path::to_bez_path` / `from_bez_path` / `reversed` | AE 顶点+切线形式（`bae_model::BezierPath`）与 kurbo `BezPath` 互转；反转路径方向（AE-SHP-014） |
| `RectShape` / `EllipseShape` / `StarShape` + `Direction` | 矩形、椭圆、多边星形，顶点顺序、起点、方向与 AE 完全一致（AE-SHP-010..012），修剪路径和虚线依赖这一点 |
| `Paint`、`Brush`、`Gradient`、`StrokeStyle`、`FillRule`、`LineCap`、`LineJoin`、`BlendMode`、`Composite` | 填充、描边、渐变填充/描边的参数；颜色是直通 alpha 的工作空间值，按 AE 钳到 0–1 |
| `ShapeGroup` / `ShapeItem` / `Operator` / `GroupTransform` | 形状树：组、路径、绘画、路径操作器，按时间轴顺序（下标 0 在最上面）。`ShapeGroup::from_evaluated` 从 03 的求值结果构建，也可以在代码里直接搭 |
| `VectorScene` | **结果类型**：图层空间里的显示列表，`DrawItem::Fill` / `Stroke` / `Layer`（隔离组），从后往前画；`bounds(with_stroke)` 对应 `sourceRectAtTime`（AE-SHP-009），`content_hash()` 用作缓存键 |
| `ops::trim_shapes`、`ops::path_length` | 修剪路径（同时 / 单独，偏移，跨起点时经过顶点 0 连成一条） |
| `shape_layer_scene(&EvaluatedLayer, &mut warnings)` | 一步得到形状图层的显示列表 |
| `render::encode`（`render`） | 把 `VectorScene` 写进 `vello::Scene` |
| `render::VelloRenderer`（`render`） | 每个 `GpuContext` 一个共享的 Vello 渲染器（`VelloRenderer::shared`），文字和其他矢量绘制共用，管线只编译一次 |
| `render::ShapeSource` / `ShapeProvider`（`render`） | bae-render 的 `LayerSource` / `SourceProvider`，provider ID 是 `SHAPE_PROVIDER = "bae.shape"` |

### 已实现的 AE 行为

- 运算顺序（AE-SHP-003）：路径操作器作用于同组中它上面的所有路径（包括上方嵌套组里的路径），先于任何绘画；所以放在填充下面的修剪路径同样会修剪这个填充。外层组的操作器在内层之后执行。一个绘画把它上面的所有路径当作**一条**复合路径来画；合并路径之上的绘画不画。
- 组变换 `T(位置)·R(旋转)·K(倾斜, 倾斜轴)·S(缩放)·T(−锚点)`（AE-SHP-029）；组不透明度按隔离组处理，组里只有一个普通绘画时直接乘进去，不开离屏层（AE-SHP-030）。
- 绘画的"合成：在同组中前一个之上"（AE-SHP-004）、组和绘画的混合模式（Vello 支持的 16 种，AE-SHP-005），非零/奇偶填充规则，线段端点、连接、尖角限制、虚线和偏移。
- 线性渐变、径向渐变和高光长度/角度（AE-SHP-027/028），颜色和不透明度两条色带分别插值。
- 修剪路径（AE-SHP-036），按 lottie-web 的规则取整、交换和环绕。

### 还没有

合并路径的布尔模式（相加、相减、相交、排除；现在都按"合并"处理并给出警告）、中继器、位移路径、圆角、扭转、摆动、Z 字形、收缩和膨胀；描边锥度和波形；渐变的缩放和旋转（AE-SHP-063）；路径布尔运算和命中测试的公开 API（给 13/16，下一阶段）。

## 给 04（合成）的接法

1. 注册 provider：`providers.register(Arc::new(bae_vector::render::ShapeProvider::new()))`。
2. 适配 03 的 `EvaluatedComp` 时，形状图层生成
   `LayerContent::Custom(CustomContent { provider: SHAPE_PROVIDER.into(), prim: 图层路径, size: None, hash: <contents 的哈希>, data: Arc::new(evaluated_layer.clone()) })`。
   `data` 也可以直接放 `Arc<VectorScene>`（例如 Ai 画板）。
3. 形状画在图层原点周围（AE 新建形状图层时锚点 (0,0)、位置在合成中心），内容的一半在负坐标。`LayerSource::bounds()`（本 PR 在 bae-render 里加的默认方法）返回真实范围，渲染图按它协商 ROI。
4. Vello 输出 `Rgba8Unorm`、直通 alpha（`ImageAlpha::Straight`），合成时预乘。颜色值不做转换。

已知限制：图层源在图层空间按请求的分辨率栅格化，图层放大时会糊。AE 的形状图层总是按最终变换后的分辨率栅格化（连续栅格化），这需要 `SourceRequest` 带上图层到合成的变换，已在 04 的任务书里提请求。

## 测试

```sh
cargo test -p bae-vector          # 单元测试 + GPU 测试（没有 GPU 时跳过；BAE_REQUIRE_GPU=1 时失败）
```

`tests/gpu.rs` 在 GPU 上画矩形（含负坐标）、半分辨率、修剪描边、奇偶填充、隔离组不透明度、线性渐变并逐像素检查；`tests/evaluated.rs` 用模型命令搭一个带关键帧修剪路径的形状图层，经 03 求值后画出来；`tests/graph.rs` 走一遍 04 的渲染图，确认 ROI 覆盖负坐标。
