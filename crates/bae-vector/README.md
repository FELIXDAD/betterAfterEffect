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

### 路径操作器与添加菜单

在目标组下用 `prim.create` 创建 `ShapeOperator`，传入 `index` 指定时间轴位置（0 为最上方）。用 `attr.set` 将 `match_name` 设为字符串，例如 `ADBE Vector Filter - Offset`。八种操作器的后缀依次是 `Merge`、`Offset`、`PB`、`Repeater`、`RC`、`Twist`、`Roughen`、`Zigzag`。添加菜单按选择位置创建填充和描边的逻辑归界面工作流。

参数沿用 AE 导出桥的属性名。把 AE 参数 matchName 传给 `bae_vector::operators::parameter_attr` 即可得到 `attr.set`、`key.set`、`attr.set_expression` 的 `attr`。例如 `ADBE Vector Offset Amount` 对应 `ae:ADBE_Vector_Offset_Amount`；中继器变换下的子属性带 `ae:ADBE_Vector_Repeater_Transform:` 前缀。二维值使用 `vec2`，菜单使用 AE 从 1 开始的数值，角度使用度数，不透明度使用百分数。已有工程的 `operator` 枚举和旧参数键仍可读取。

`tests/ae/probe.jsx` 读取 AE 26.2.1 的参数与初始值。直接脚本添加中继器时，AE 返回 1 个副本和位置 [0, 0]；添加菜单可以通过命令设置其交互预设。用 `tools/ae/aectl lock -- tools/ae/aectl run crates/bae-vector/tests/ae/render.jsx` 生成参考帧，输出目录见返回值。

### 还没有

描边锥度和波形；渐变的缩放和旋转（AE-SHP-063）；路径布尔运算和命中测试的公开 API（给 13/16，下一阶段）。

## 给 04（合成）的接法

1. 注册 provider：`providers.register(Arc::new(bae_vector::render::ShapeProvider::new()))`。
2. 适配 03 的 `EvaluatedComp` 时，形状图层生成
   `LayerContent::Custom(CustomContent { provider: SHAPE_PROVIDER.into(), prim: 图层路径, size: None, hash: <contents 的哈希>, data: Arc::new(evaluated_layer.clone()) })`。
   `data` 也可以直接放 `Arc<VectorScene>`（例如 Ai 画板）。
3. 形状画在图层原点周围（AE 新建形状图层时锚点 (0,0)、位置在合成中心），内容的一半在负坐标。`LayerSource::bounds()`（本 PR 在 bae-render 里加的默认方法）返回真实范围，渲染图按它协商 ROI。
4. Vello 输出 `Rgba8Unorm`、直通 alpha（`ImageAlpha::Straight`），合成时预乘。颜色值不做转换。

已知限制：图层源在图层空间按请求的分辨率栅格化，图层放大时会糊。AE 的形状图层总是按最终变换后的分辨率栅格化（连续栅格化），这需要 `SourceRequest` 带上图层到合成的变换，已在 04 的任务书里提请求。

## 兼容性：没有显卡的 Windows（软件适配器）

没有显卡的 Windows（虚拟机、远程桌面、GitHub 的 Windows runner）上，wgpu 拿到的是 WARP（微软的软件 D3D12 实现，`DeviceType::Cpu`）。在 WARP 上用 Vello 的 GPU 计算管线会让整个进程崩溃（`STATUS_ACCESS_VIOLATION`，0xc0000005），应用一画形状图层就退出。

现在 `VelloRenderer` 按适配器选后端（`VectorBackend`）：

- 硬件适配器：Vello GPU（默认，最快）。
- 软件适配器：`vello_cpu` 在 CPU 上栅格化，结果（预乘 `Rgba8Unorm`）上传成纹理交给合成器；**根本不创建 Vello 的 GPU 管线**。文字走 skrifa 字形轮廓 + `vello_cpu` 填充/描边，仿粗体、仿斜体、逐字符描边顺序与 GPU 路径一致。启动时打一条 `warn` 日志说明在用 CPU 栅格化。
- 环境变量 `BAE_VECTOR_RENDERER=cpu|gpu` 强制选择（`gpu` 用于在软件适配器上复现驱动问题）。

`vello_cpu` 0.1.0 本来就随 egui 的 epaint 链接进应用，不新增 crate。两个后端在同一画面上的差异只在抗锯齿边缘：`tests/cpu.rs` 在硬件适配器上逐像素对比（平均差 < 1%），在 WARP 上只跑 CPU 路径。

Windows 实机（RTX 5090 D，2026-11-21，报告见 #78）确认了两件事：

- **WARP 本身在 Vello 派发时崩溃。** `vello_gpu_stages` 在 WARP 上能创建管线（FXC 编译通过），但在第一次渲染（stage 2，派发）时访问违规（0xc0000005）。所以软件适配器上必须走 CPU 后端，和着色器编译器无关。强制 WARP 时 bae-vector、bae-text 的测试全部通过（Vello GPU 测试跳过，CPU 后端测试运行）。
- **真显卡上的 0xc000001d 是 DXC 的问题，不是 Vello。** wgpu 默认的 DX12 编译器是 `Auto`，会用 `PATH` 上找到的 `dxcompiler.dll`（那台机器上排第一的是某个应用自带的 dxcompiler 1.8.2505 配旧版 dxil），在创建管线时崩溃。设 `WGPU_DX12_COMPILER=fxc` 后真显卡上 bae-vector、bae-text 全部通过，CPU 与 Vello 的逐像素对比（平均差 < 1%）也通过。修法是在 bae-gpu 里固定 DX12 编译器（01 负责），不在本 crate。

复现诊断：

```bat
set BAE_GPU_ADAPTER=software
set BAE_GPU_VALIDATION=1
set WGPU_DX12_COMPILER=fxc
cargo test -p bae-vector --test cpu -- --ignored --nocapture vello_gpu_stages
```

它强制在 WARP 上走 Vello GPU，在"创建管线 / 渲染空场景 / 渲染测试场景 / 回读"每一步前打印 `[vello-diag]`，最后一条说明崩在哪一步。

## 测试

```sh
cargo test -p bae-vector          # 单元测试 + GPU 测试（没有 GPU 时跳过；BAE_REQUIRE_GPU=1 时失败）
```

`tests/gpu.rs` 在 GPU 上画矩形（含负坐标）、半分辨率、修剪描边、奇偶填充、隔离组不透明度、线性渐变并逐像素检查；`tests/evaluated.rs` 用模型命令搭一个带关键帧修剪路径的形状图层，经 03 求值后画出来；`tests/graph.rs` 走一遍 04 的渲染图，确认 ROI 覆盖负坐标。

### AE 26.2.1 路径操作器对比

测试用 Vello CPU 画出 256 × 160 的同一场景，逐像素累计 Alpha 绝对差，再除以 AE 的 Alpha 总和。这样缩小图形、增加透明边距都不会稀释误差。表内是对应参考帧中的最大值。

| 操作器 | 输入 | Alpha 差异 |
|---|---|---:|
| 合并路径 | 两个相交矩形，五种模式 | 0.50% |
| 位移路径 | 三种连接、限制斜接、内缩、多副本及小数偏移 | 0.21% |
| 收缩和膨胀、圆角 | 矩形，数量 40%、圆角 10 px | 1.23%、0.31% |
| 中继器 | 几何与已绘制内容，3.5 个副本，锚点、非等比缩放、旋转、透明度 | 0.44% |
| 锯齿（Z 字形） | 矩形和椭圆，边角和平滑 | 0.77% |
| 扭转 | 80 × 40 矩形，90° / 180° | 0.85% / 2.62% |
| 摆动路径 | 同一矩形，默认参数，0.5 秒 | 22.81% |

摆动路径使用可复现的 BAE 噪声。用同一植入值可以重现 BAE 的轨迹，和 AE 的顶点位置有差异。直线长度 20–180 px 的探针显示，详细信息决定每段新增点数，点间距随边长变化。扭转中心以输入轮廓的包围盒中心为基准。探针中长边的细分密度随角度分段增加，BAE 对变换后的曲线手柄使用近似拟合；1 px 开放直线、90° 扭转的六档边长（20、40、41、60、80、120 px）中，20 和 40 px 的二值轮廓重合，60 px 的差异最大，轮廓双向最近距离均值 2.02 px，最大 8.95 px（Alpha > 64/255）。可用 `tests/ae/sampling.jsx` 重跑角度、边长和详细信息网格。

中继器合成值 1 把新副本放在之前的副本下面。实际起止透明度的插值分母为向上取整后的副本数。3.5 个副本、起止 80% / 20% 时，四份实测 Alpha 峰值分别是 204、166、128、45（8 位）。绘画位于中继器下方时，只绘制一次复合路径。
