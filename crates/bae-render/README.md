# bae-render

GPU 合成与渲染图（工作流 04）。接口说明以代码里的文档注释为准（`cargo doc -p bae-render --open`），这里只列其他工作流最常用的部分。

## 数据流

```
Stage ──► CompEvaluator（AnimEvaluator = 03 的 bae-anim；MockEvaluator 只留给测试）──► RenderComp
      ──► graph::build（节点、ROI 协商、剔除、缓存键）──► RenderGraph
      ──► Renderer（共享 GpuContext，RGBA16F 预乘缓冲）──► 纹理
      ──► 视口（零拷贝）/ 导出（异步回读 ──► 08 的编码器）
```

## 给其他工作流

| 工作流 | 用什么 | 说明 |
|---|---|---|
| 03 动画 | `AnimEvaluator`（`anim.rs`） | 把 `bae_anim::EvaluatedComp` 转成 `RenderComp`：世界矩阵 → 2D 仿射（3D 图层另存 4×4），活动摄像机、启用的灯光，运动模糊图层在 03 给的采样时间上的变换，解析纯色/素材/预合成来源，token → 枚举。收到 `StageEvent` 时调用 `CompEvaluator::invalidate`。 |
| 12 / 09fe9cc0 查看器（摄像机视图、3D 坐标轴手柄） | `bae_render::camera`、`three_d::Homography` | 见下面「3D 与摄像机」。 |
| 05 矢量/文字 | `LayerSource`、`SourceProvider`、`LayerContent::Custom` | 形状和文字图层的求值结果放进 `CustomContent { provider: "bae.vector", data, hash }`，05 注册一个 `SourceProvider`，按请求区域把像素画进 `EncodeContext::allocate` 分配的纹理。 |
| 07 效果 / 19 插件 | `Effect`、`EffectLibrary`、`EffectParams` | 参数定义用 `bae_model::EffectDef`（与界面、Agent 共用）；`Effect` 负责边界变化（`output_bounds`）、输入区域（`input_roi`）和 GPU 编码（`render`）。 |
| 08 媒体 | `LayerContent::Footage` | 素材帧通过 bae-media 解码成共享设备上的 RGBA16F 预乘纹理，直接采样，不复制。 |
| 12 动效界面 | `FrameCache::cached_times`、`Renderer::invalidate` | 时间轴的「已缓存帧」绿条；把 `StageEvent` 交给渲染器做精确失效。 |
| 05 / 07 / 19 测试 | `bae_render::testing` | 不开窗口、不建渲染器：`render_source`、`render_effect`、`with_encode_context`（自己的编码代码）、`upload`、`read_pixels`。 |
| 17 Agent | `bae render`、`Renderer::read_rgba8` | 无窗口渲染 PNG（`--json` 输出每帧耗时和警告），Agent 用它「看」画面；进程内可以直接回读 8 位 RGBA。 |

## 约定

- 工作缓冲：预乘 alpha，RGBA16F（可选 RGBA32F），存工作空间的编码值（默认与 AE 一样非线性混合）。
- 坐标：合成空间 y 向下；像素 `(x, y)` 覆盖 `[x, x+1) × [y, y+1)`。
- `GpuImage`：`rect` 是图像所在空间的像素坐标，纹素 (0,0) 从 `(rect.x0, rect.y0)` 开始，每个纹素 `pixel_scale` 个像素（半分辨率 2，矢量图层按 4 倍密度栅格化时 0.25），纹素数是 `texel_size()`。用 `GpuImage::new` / `from_texture` 构造。
- 图层源按 `SourceRequest.scale`（期望密度 = 预览分辨率 × 图层在屏幕上的放大倍数）出图，矢量/文字据此连续栅格化；素材可以忽略它返回原始分辨率。
- 图层顺序：`RenderComp::layers[0]` 是最上面的图层（AE 序号 1），与 `layers` 文件夹的子 Prim 顺序一致。
- 混合模式、轨道遮罩、遮罩模式的 token 与 schema 一致（`multiply`、`luma_inverted` …）。
- 缓存键：`(Prim, 时间, 参数哈希)`；`StageEvent` 按「最具体的依赖 + 该 Prim 自己的时间基准」精确失效（图层属性用图层时间）。

## 3D 与摄像机（经典 3D，2.5D）

规格：`docs/spec/ae/compositing-3d-color.md` AE-CMP-400 … 441，`transform-keyframes-expressions.md` AE-ANI-007 … 022、102 … 105。坐标与 AE 相同：+x 右、+y 下、+z 远离观众，合成像素。

输入（`RenderComp` / `RenderLayer`，纯数据）：

| 字段 | 含义 |
|---|---|
| `RenderLayer::matrix_3d: Option<DMat4>` | 3D 图层：图层空间 → 合成空间的 4×4（锚点、缩放、X/Y/Z 旋转、方向、位置、父级）。`None` 是 2D 图层，用 `transform`。 |
| `RenderComp::camera: Option<Camera>` | 活动摄像机（AE-CMP-423）；`None` = AE 默认摄像机（`Camera::default_for(w, h)`：50 mm，`zoom = W·50/36`，位于 `(W/2, H/2, −zoom)`，z = 0 与 2D 逐像素一致）。 |
| `RenderComp::lights: Vec<Light>`、`RenderLayer::material` | 启用的灯光（点光、聚光、平行光、环境光）和图层的材质选项；3D 图层按 AE 的经典 3D 光照着色，带阴影（见下面「灯光」「阴影」）。 |
| `RenderComp::motion_blur: Option<Arc<CompMotionBlur>>` | 本帧的采样时间（03 的 `MotionBlur::sample_times`：快门角度、相位、每帧样本数来自合成设置）和每个时间的活动摄像机、自适应采样上限。 |
| `RenderLayer::motion_samples` | 图层在每个采样时间的变换；图层开关和合成总开关都开时才有（AE-ANI-102）。 |

给界面（查看器的摄像机视图、3D 坐标轴手柄、摄像机工具）的函数，都在 `bae_render::camera`，不依赖 GPU：

- `Camera::project(point, comp_size)` / `Camera::unproject(pixel, depth, comp_size)`：合成空间的点 ↔ 合成像素（拖动 3D 手柄时用 `unproject` 把鼠标位置放回某个深度）。
- `Camera::from_transform(parent, position, point_of_interest, orientation, rotation, zoom)`：按 AE 的摄像机变换建摄像机（双节点传目标点，单节点传 `None`），自定义视图（正面、左侧、顶部……）也用它。
- `Camera::plane_homography(layer_world, comp_size)`：一个 3D 图层平面到合成像素的射影映射（`three_d::Homography`，可以 `map`、`inverse`），画图层外框、命中测试用。
- 正交视图（AE 的正面、左侧、顶部、背面、右侧、底部，AE-CMP-410）：`Camera::orthographic(OrthoView::Top, centre, scale)`。`centre` 是落在画面中央的合成空间点，平移就改它（比如设成鼠标下那个像素的 `unproject`）；`scale` 是放大倍数（1 = 100%，正面 + 合成中心 + 1 时 z = 0 与 2D 一致）。没有透视，也不按深度裁剪，深度排序照常。查看器渲染某个视图时，把 `RenderComp::camera` 换成这个摄像机即可；`OrthoView::ALL` 是菜单顺序，`OrthoView::rotation()` 给出视图的朝向。`Camera::eye()` 是深度排序和打光用的观察点（正交视图在视线反方向很远处）。
- 镜头换算（摄像机设置对话框，AE-CMP-418）：`zoom_from_focal_length`、`focal_length_from_zoom`、`angle_of_view`、`zoom_from_angle_of_view`、`FilmMeasure`、`PRESETS`、`default_zoom`。
- `look_at`、`xyz_rotation`：双节点摄像机的瞄准旋转和 AE 的 X/Y/Z 旋转顺序（与 bae-anim 一致）。

渲染：

- 图层栈按 AE 的 3D 组划分（`three_d::segments`，AE-CMP-406）：相邻的 3D 图层一组，2D 图层、调整图层、带效果/蒙版/轨道遮罩的 3D 预合成把组隔开；摄像机、灯光、空对象不隔开。各段按图层顺序合成。
- 组内按摄像机深度排序（`three_d::depth_order`）：图层都是平面四边形，用 BSP 得到逐像素准确的前后顺序；互相穿插的图层沿交线切开（AE-CMP-405），切口抗锯齿、两侧覆盖率相加为 1；共面图层按图层顺序（上面的在前）。混合模式在组内照常起作用，能看到组下面的图层（AE-CMP-407 的「组画在下面的结果上」）。
- 3D 图层是一次 draw：顶点阶段输出齐次坐标（w = 摄像机深度，在近平面裁剪，摄像机后面的部分不画），片元阶段用逆射影映射回图层、用局部雅可比量边缘过渡，与 2D 的采样规则一致。正对摄像机的平面按仿射画，所以 z = 0 的 3D 图层与 2D 逐像素相同。
- 还没有：景深、折叠变换（预合成里的 3D 图层并入外层组）。

### 灯光

- 模型在 `bae_render::light`（模块文档写了公式，`light::shade` 是 CPU 参考实现），全部在 AE 26.2.1 上实测过：漫射是 Lambert，镜面是 Blinn-Phong（半角向量），指数就是「镜面反光度」的百分数；聚光的羽化在余弦上做 smoothstep（内角 = 外角 ×（1 − 羽化））；「平滑」衰减是半径到半径 + 衰减距离的 smoothstep，「反向平方限制」是半径外的 (R/d)²；法线朝向观察者（双面）；金属质感把高光颜色从白色混到图层颜色；8/16 bpc 截断到 1。
- 什么时候打光：3D 图层、材质选项「接受灯光」开、合成里有启用的灯光。打开了「调整图层」开关的灯光只照它下面的图层；照不到的图层是暗的（有灯光但没照到），不是不打光。2D 图层永远不打光。
- GPU：打光的图层用单独的 `fs_lit` 管线，每个 draw 带一条光照记录（世界坐标映射、法线、观察点、材质、灯光列表），不打光的图层没有任何额外开销。运动模糊的图层按帧时间的几何打光（每个采样位置取图层上同一点的光照）。

### 阴影

- 条件（AE-CMP-437）：灯光「投影」开（环境光不投影），投射图层「投影」开，接受图层「接受阴影」开，两者在同一个 3D 组里（2D 图层隔开的组之间没有阴影）。
- 模型（在 AE 26.2.1 里标定，见 `light` 模块文档）：每个像素朝每盏投影灯光发一条光线，和每个投射图层的平面求交；挡住的量是投射图层在交点处的覆盖率（alpha × 不透明度），这盏灯的贡献乘以 `1 − 阴影深度 · a · (1 − 透光率 · 投射图层自身颜色)`（彩色透光，像彩色玻璃）。「阴影扩散」等于一个半边长为扩散值的方形面光源：覆盖率在灯光投到投射平面的方框里取平均（实测是线性过渡，宽度 = 2 · 扩散 · 投射到接受的距离 / 灯光到投射的距离）；扩散为 0 时也有约 1 个接受像素的柔边（AE 阴影图的过滤）。
- GPU：纯色投射图层解析计算；有图像的投射图层（素材、文字、预合成，蒙版和效果之后的画面）先拷进这个合成的阴影图集（`exec/shadow.rs`），着色器在图集里按方框采样。每个接受图层只带可能投到它身上的投射图层（CPU 上按灯光剔除在它背后或影子落不到它上面的）。
- 还没有：「仅」（投影仅 / 接受阴影仅）、阴影图分辨率选项、投射图层的蒙版（目前按图层画面，不含没有效果时才在绘制里应用的蒙版）。

## 运动模糊

- 图层开关和合成「启用运动模糊」都开时生效（AE-ANI-102）。采样时间、快门角度/相位、每帧样本数用 03 的结果（`bae_anim::MotionBlur`）。
- 和 AE 一样只采样变换：图层内容（素材、蒙版、效果）在帧时间渲染一次，再按每个采样的变换叠加；2D 图层按 AE 的自适应规则加样本（每像素位移一个样本，上限是合成的自适应采样上限，`motion::adaptive_samples`），3D 图层固定用每帧样本数，摄像机运动也会模糊 3D 图层。
- 在 GPU 上累加：合成着色器对每个像素循环所有采样位置、在预乘空间里等权平均，然后照常乘不透明度、轨道遮罩、做混合。每个模糊图层仍是一次 draw，没有额外的纹理和 pass，也没有回读。不动的图层（所有采样相同）按普通图层画。
- 还没有：形状/文字路径动画、蒙版动画、嵌套合成内容的逐采样重渲染（AE 对连续栅格化图层会这样做），帧混合。

## 已实现（第一阶段）

- 纯色、素材（bae-media 解码，零拷贝采样）、预合成（按 ROI 只渲染需要的部分）；变换（解析边缘覆盖，放大时与 AE 一样按源像素过渡，缩小时框滤波超采样）、不透明度。
- 混合模式：正常、相加、相乘、滤色、叠加（与 AE 26.2.1 逐像素对照，见 `tests/ae/README.md`）；其余回退为正常并警告。8/16 bpc 截断与 32 bpc 浮点两种语义。
- 轨道遮罩（Alpha / 亮度 × 正常 / 反转）、保留基础透明度。
- 纹理池、帧缓存（预算 + StageEvent 精确失效）、半分辨率等降采样、ROI。
- `bae render <工程> --comp <路径> [--frame N | --frames A-B|all] [-o out_####.png] [--format png|tiff|exr|h264|hevc|prores|prores4444] [--compression 档位] [--bits 8|16|32] [--threads N] [--resolution half] [--depth 8|16|32] [--alpha straight|premultiplied|background] [--roi X,Y,W,H] [--json]`。导出是流水线（异步回读 + 每核一个编码线程），`--json` 给出各阶段耗时；进程内用 `bae_render::export_frames`（渲染队列也用它）。
- 基准 B1：`cargo xtask bench -p bae-render`，方法和结果见 `docs/perf.md`。

之后又做了：蒙版、效果执行、调整图层、其余混合模式、3D 图层与摄像机、正交视图、运动模糊、灯光着色（见上）。还没做：视口纹理输出、线性混合、阴影、景深。
