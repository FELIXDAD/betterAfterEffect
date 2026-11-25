# 04 · GPU 合成与渲染图

| 项 | 内容 |
|---|---|
| 分支 | `render/compositor` |
| Orca 工作区 | `bae-04-compositor` |
| 阶段 | P1（性能是最高优先级，这是核心） |
| 依赖 | 01（bae-gpu）、02、03（EvaluatedComp，未完成前可以先模拟） |
| 负责目录 | `crates/bae-render/`、`benches/`、`docs/perf.md` |

## 目标

把 `EvaluatedComp` 在 GPU 上渲染成画面，实时预览和最终渲染都要显著快于 AE。

## 要做

- **渲染图**：节点包括图层、预合成、效果、遮罩、调整图层、轨道遮罩；按依赖调度；纹理池复用；工作缓冲 RGBA16F，可选 32F。
- **合成**：AE 全部混合模式、保留基础透明度、轨道遮罩（Alpha / 亮度 / 反转）、遮罩（相加、相减、交集、差值；羽化、扩展、不透明度）、调整图层、3D 图层按深度排序和摄像机投影（2.5D）、运动模糊（用 03 给的采样时间）。
- **非线性混合与线性化工作空间两种模式**，保证 AE 工程导入后颜色一致。
- **效果框架**：`Effect` trait（参数定义 → 属性 schema、WGSL 着色器、多 pass、需要扩展的边界/ROI）。先实现 3 个参考效果：高斯模糊、填充、变换。07 基于它扩充效果库。
- **插件与兼容预留**：效果接口按 19 的插件接口设计（内置效果和插件效果同一路径）；渲染图支持"CPU 节点"（GPU 纹理与 CPU 缓冲区之间异步转换，含通道顺序、位深、取值范围、预乘方式）；支持效果声明对其他图层、其他时间的依赖；渲染请求携带降采样因子、像素宽高比、时间；不线程安全的效果串行调用。这些是将来兼容 AE 插件的基础（见架构文档第 15 节），不能影响不使用它们的工程的性能。
- **图层源接口**：矢量（05）、媒体帧（08）、文字（05）都通过统一的"图层源"接口接入。
- **缓存**：帧缓存和中间结果缓存，有内存/显存预算（类似 AE 的 RAM 预览）；StageEvent 精确失效；给时间轴提供"已缓存帧"数据（AE 的绿条）。
- **预览**：分辨率降级（完整、1/2、1/3、1/4、自动）、感兴趣区域、后台预渲染。
- **视口输出**：渲染到与 egui 共享设备的纹理，零拷贝显示。
- **无窗口渲染**：`bae render` 命令行，先输出 PNG 序列（视频编码交给 08）。
- **基准**：建立架构文档第 11 节的 B1–B3 基准工程和 bench 程序，接入 CI 记录趋势；在 `docs/perf.md` 写清楚与 AE 对比的测量方法。

## 对外接口

- `Renderer::render_frame(comp, time, quality, color_ctx) -> 纹理`
- `Effect` trait、图层源 trait、视口纹理句柄、缓存状态查询。

## 验收

- B1 在 Apple M1 和 RTX 3060 级别的机器上，1080p 全分辨率预览达到实时（≥ 30 fps）。
- 混合模式和轨道遮罩有 golden image 测试（与 AE 的输出对比）。
- 改一个图层的属性，只有受影响的部分重新渲染。

## 不做

- 具体的效果库（07）、色彩变换的实现（06，本工作流只预留变换节点）、编码（08）。

## 进度

**2026-11-20**

- PR 1 [interface]：`crates/bae-render` 公共接口。渲染输入 `RenderComp`/`RenderLayer`（纯数据，03 合并前用 `MockEvaluator` 按 schema v1 从 Stage 求值）、AE 全部 38 种混合模式 token、轨道遮罩/遮罩模式、`RenderRequest`（分辨率、ROI、位深、RGBA16F/32F、色彩上下文占位）、`LayerSource`/`SourceProvider`（05/08/插件的图层源接口）、`Effect`/`EffectLibrary`（参数定义复用 `bae_model::EffectDef`）、渲染图节点模型与 ROI 协商（合成、纯色、图层源、效果、轨道遮罩；剔除；缓存键）、`FrameCache`（内存预算 LRU + StageEvent 精确失效）、`fixture::SceneBuilder`（用命令搭测试/基准工程）。
- PR 1 已合并（#16）。
- PR 2（#44，分支 `render/compositor-gpu`）：GPU 执行。纹理池（分桶复用，租约计数）、合成着色器（一个图层一次 draw，连续纯色合批；正常/相加(32 bpc)/滤色走固定管线混合，其余只拷贝图层覆盖区域的底图再混合）、轨道遮罩、保留基础透明度、8/16 bpc 截断、素材经 bae-media 零拷贝采样、03 的 `Evaluator` 适配器 `AnimEvaluator`、`bae render` 输出 PNG（`apps/bae` 加了一行 `add_module`）、B1 基准与 `cargo xtask bench`、`docs/perf.md`。
- 与 AE 26.2.1 对照（`crates/bae-render/tests/ae/`，24 个用例，AE 输出存为 golden 进 CI）：5 种混合模式、4 种轨道遮罩、保留基础透明度、亚像素/旋转/缩放/预合成放大最大差 ≤ 1；修正了相加（AE 是预乘直接相加）和放大时的边缘过渡（按源像素）。缩小的软边纹理还差最多 10。
- B1（Apple M4）：完整分辨率预览 852 fps，半分辨率 1333 fps，单帧延迟 p50 2.2 ms，导出（求值+渲染+回读）259 fps。形状、文字还是代替品，没有模糊和发光。
- 从 `.baez` 打开的工程：素材经 21 的 `bae_package::AssetLocator` 定位，包内条目解到缓存目录再交给 bae-media（序列帧整目录解出）；两个求值器和 `bae render` 都用 `bae_render::AssetFiles`。
- 回应请求（#44）：05 的 1（保留 `LayerSource::bounds()`）、2（`AnimEvaluator` 把形状/文字图层转成 `bae.shape` / `bae.text` 的 `Custom` 内容，`bae render` 从应用的 `Registry<dyn SourceProvider>` 取 provider，`apps/bae` 注册了 05 的 `ShapeProvider`）、3（`SourceRequest.scale` 改为期望密度 = 预览分辨率 × 图层在屏幕上的放大倍数，矢量源按最终尺寸栅格化）、4（`bae_render::testing`）；19 的 R04-11（`EffectParams` 加图层范围、帧率、time_step、线性混合、渲染目的、快门角度/相位、每实例随机种子）、R04-12（`testing::with_encode_context` / `render_effect` / `upload` / `read_pixels`）。`GpuImage.rect` 恢复为 #16 文档里的像素坐标（`texel_size()` 由 `pixel_scale` 算出）。
**2026-11-21**

- 导出流水线（分支 `render/export-pipeline`，[interface]）：`bae render` 不再逐帧串行。新的公共模块 `bae_render::export`：`ExportReadback`（输出转换写进复用的目标纹理，3 个轮换暂存缓冲异步映射，只等最老那帧的那次提交；`MappedFrame` 可以在别的线程拷出，缓冲还回来复用）、`export_frames`（求值 → 渲染 → 异步回读 → 编码线程 → bae-media 并行序列写出；逐帧回调、取消标志；2026-11-21 起停止和出错都保留已写出的帧）、`ExportSettings` / `ExportFormat` / `ExportAlpha` / `ExportStats` / `ExportReport`，给 `bae render` 和接下来的渲染队列共用。`frame_file` 移到 `export`，命名规则与 bae-media 的序列写出一致（文件名里第一段 `#`，AE 的 `[####]` 去掉方括号）。`RenderError::Canceled`。输出转换着色器支持 BGRA8 / RGBA16F / RGBA32F 目标，浮点目标不截断超白。
- `bae render` 新参数：`--format png|tiff|exr|h264|hevc|prores|prores4444`（默认按扩展名）、`--compression`、`--bits`、`--threads`；`--json` 增加 `stages`（求值、渲染、等 GPU、拷出、等编码器、编码、写盘）、`throughput`、`output`。`render_ms` 不再包含回读。
- B1 300 帧（Apple M4）：PNG 5.03 s → 0.47 s（10.7×），每帧 16.65 → 1.42 ms；TIFF 不压缩 1.26 s（受磁盘限制）；EXR ZIP 2.19 s。M4 上现在是 GPU 受限。基准新增 `export_fps_async`（672 fps，同步回读 359 fps）。详见 `docs/perf.md`。
- 改了 08 的目录（`crates/bae-media` 导出部分）和根 `Cargo.toml` 的一行（`png` 回到 opt-level 3），原因写在 PR 里。
- 应用内的导出入口（单独 PR）：「合成 ▸ 添加到渲染队列」（Cmd/Ctrl+M）和渲染队列面板，代码在 `crates/bae-ui-motion/src/render_queue.rs`（12 已停，菜单、快捷键、面板都在它的注册表里）。按「渲染」时给工程拍快照，后台线程用自己的 `Renderer` 跑 `export_frames`，界面不卡；进度条、停止（保留已渲出的帧，显示「已停止（N/M 帧）」）、失败原因、在访达 / 资源管理器中显示。队列是应用运行时状态，不进工程文件。

- 蒙版（分支 `render/masks`，[interface]，由 F 代做）：`RenderLayer.masks` 开始渲染。路径用 kurbo 展平（与 05 同一个库，不新增依赖；bae-vector 依赖 bae-render，反过来依赖会成环），GPU 上按精确面积累加绕数（非零环绕），扩展是圆角描边轮廓的并 / 差，X/Y 分开的高斯羽化（σ = 0.38 × 羽化，大羽化按轴降采样），7 种模式按 AE-CMP-038 的公式用固定管线混合合成，反转、蒙版不透明度；蒙版在图层空间、效果之前（有效果时先合成一张带蒙版的图层图像，没有效果时图层绘制直接采样蒙版）；一帧所有蒙版共用图集（每页 3 个 pass）；蒙版的支撑范围缩小图层边界。新公共接口：`graph::NodeKind::Mask`、`LayerOp.mask`、`bae_render::mask`、`fixture::{SceneBuilder::mask, rect_path, ellipse_path}`、`TexturePool::acquire_texels`。14 个 GPU 像素测试（`tests/masks.rs`），8 个 AE 对照用例等装了 AE 的机器出 golden（`tests/ae/README.md`）。基准：1080p 50 个图层、60 个每帧都变的羽化蒙版 247 fps（M1 Pro，`docs/perf.md`）。还没有：可变宽度羽化（P1）、羽化衰减"线性"、蒙版运动模糊。
- 下一步：效果框架执行与三个参考效果、调整图层、其余混合模式（继续用 AE 对照）、视口纹理输出（零拷贝给 12）、B1 与 AE 渲染速度对比、B3。

**2026-11-24**

- 3D 图层与运动模糊（分支 `render/3d-motion-blur`，[interface]）：
  - 新公共模块 `camera`（AE 摄像机模型：默认摄像机、单/双节点、方向和 X/Y/Z 旋转、`project` / `unproject` / `plane_homography`、镜头换算和预设）、`three_d`（3D 组划分、射影映射、BSP 深度排序与相交切分）、`light`（灯光和材质选项，只有数据）、`motion`（采样数据、AE 的自适应样本数）。`RenderLayer::matrix_3d` / `motion_samples` / `material`，`RenderComp::camera` / `lights` / `motion_blur`；`AnimEvaluator` 从 03 的结果填这些（活动摄像机、启用的灯光、在 03 给的采样时间上求值）。
  - GPU：3D 图层一次 draw，齐次坐标 + 近平面裁剪，逐像素逆射影映射；相交切块带图层空间半平面，切口抗锯齿；运动模糊在合成着色器里逐像素累加所有样本（单独的 `fs_motion` 管线），不加 pass、不回读。
  - 与 AE 26.2.1 对照：新增 17 个用例（`tests/ae/`），3D 多数最大差 ≤ 4，2D 运动模糊 ≤ 1–5。实测确认并改正了规格里的几处推测：样本时间是 `t_open + dur·k/N`（改了 03 的 `MotionBlur::sample_times`，一行）、2D 自适应样本数 `floor(d/step)+1`（32 像素以下 step 1.3，以上 1.9）、相交平局 1 像素深度归上层、未折叠的 3D 预合成与其他 3D 图层一起排序。
  - 基准 `benches/three_d.rs`（M4）：3D 579 fps、2D 运动模糊 24.5 fps（50 个大图层 × 约 27 样本）、B1 不回退（960 fps）。
  - 还没有：灯光着色与阴影、景深、折叠变换、形状/文字/蒙版动画的逐样本重渲染、帧混合；`threed_floor`（极度透视的斜边抗锯齿比 AE 宽，最大差 25）和 `mb_camera`（15）的剩余差。
  - 给 09fe9cc0 的查看器（摄像机视图、3D 坐标轴手柄）：接口说明见 `crates/bae-render/README.md`「3D 与摄像机」。
- 请求 03：运动模糊现在在每个采样时间把整个合成求值一遍（含嵌套合成）。希望 `bae-anim` 提供只求变换（含父级和摄像机）的逐样本接口，例如 `EvaluatedLayer` 带每个采样时间的世界矩阵（AE-ANI-105 的实现提示）。

**2026-11-25**

- 正交视图、灯光着色、自适应采样限制（分支 `render/3d-ortho-lights`，[interface]）：
  - `camera::OrthoView` + `Camera::orthographic(view, centre, scale)`：AE 的正面、左侧、顶部、背面、右侧、底部，给查看器的 3D 视图菜单（A）。没有透视、不按深度裁剪，深度排序照常。
  - 灯光着色：点光、聚光、平行光、环境光（强度、颜色、衰减、锥形和羽化）照 3D 图层，用材质选项（接受灯光、环境、漫射、镜面强度、反光度、金属质感）。模型先在 AE 里逐项标定（漫射 Lambert、镜面 Blinn-Phong 指数 = 反光度 %、聚光在余弦上 smoothstep、平滑衰减是 smoothstep、反向平方），8 个 AE 对照用例最大差 1–4。`light::shade` 是 CPU 参考；打光的图层用单独的 `fs_lit` 管线（B1 不受影响）。规格 AE-CMP-432/433/435/436 按实测改了。材质的反射系数进了模型 schema。
  - 合成的 `motion_blur_adaptive_limit`（16–256，默认 128）进 schema，bae-anim 读它；合成设置命令和对话框的范围从 64–256 改成 AE 的 16–256。
  - 修了 #288 的一个 3D 问题：切开的图层在切口处有半透明的缝；现在先画的一块越过切口、后画的一块做抗锯齿，屏幕上不重叠的图层不再互相切。
  - 阴影（投射 / 接受、阴影深度和扩散、透光率）拆到下一个 PR。
- 阴影（分支 `render/3d-shadows`，叠在 `render/3d-ortho-lights` 上）：灯光「投影」、图层「投影」/「接受阴影」、阴影深度、阴影扩散、透光率，同一 3D 组内。先用 18 个标定合成在 AE 里拟合（扩散 = 方形面光源、透光 = 投射图层自身颜色、深度和不透明度线性），7 个 AE 对照用例最大差 1–33（差都在阴影边缘，AE 阴影图的过滤）。纯色投射图层解析，带图像的经每个合成的阴影图集（`exec/shadow.rs`）；CPU 上剔除不可能投影的投射图层。M1 的 3D（2.5D）到此齐了；还没有「仅」模式和阴影图分辨率选项。

- [x] 渲染图与纹理池
- [ ] 混合模式、遮罩、轨道遮罩
- [x] 3D 与运动模糊（灯光着色、景深除外）
- [ ] 效果框架与参考效果
- [ ] 缓存与预览
- [x] bae render 命令行（PNG）
- [x] 导出流水线：异步回读 + 并行编码，PNG / TIFF / EXR / macOS 影片（见下）
- [ ] 基准与 CI 趋势

## 请求（其他工作流在这里留言）

### 06 render/color → 04（2026-11-20）

bae-color 接口已提 PR（`[interface]`），接法见 `crates/bae-color/README.md`。建议：

1. `RenderRequest.color` 改用 `bae_color::ColorContext`（它不是 `Copy`，可以放 `Arc<ColorContext>`）；帧缓存键用 `render_fingerprint()`，显示/视图变换不进缓存键（AE-CMP-457）。
2. 现在的 `BlendSpace { Encoded, Linear }` 换成 `bae_color::BlendSpace`：`Stored` / `LinearBuffers` 直接混合缓冲区里的值；`LinearBlend` 在混合前后套 `blend_linearization()` 的 decode / encode。注意"线性化工作空间"时缓冲区本身已经是线性的，不能再转换一次。
3. 素材节点挂 `interpret::input_transform(...)`；相邻的色彩节点用 `ColorTransform::then` 合并，`is_identity()` 时删掉节点；`to_wgsl(prefix)` 生成的片段拼进采样素材的 pass；视口最后一个 pass 用 `display_transform()`；渲染队列用 `output_transform()`。
4. 依赖 bae-color 时打开 `features = ["media"]`，就能 `SourceTags::from(&media_info.color)`。

### 21 io/packaging → 04（2026-11-20）

工程可以直接从 `.baez` 打开（素材在包里，不解包）。这时 `AssetResolver::new(base_dir)` 找不到包内素材。建议求值/渲染找素材时改用 `bae_package::AssetLocator`（`AssetLocator::for_project(&project)`，`locate(&asset_file)`）：

- `AssetLocation::File { path, .. }`：和现在一样按路径打开；
- `AssetLocation::Packaged { entry }`：`PackageReader::entry(entry)` 给出 `.baez` 里的字节偏移和长度（不压缩，可直接定位读取）；如果解码器只能接受文件路径，用 `PackageReader::materialize(entry, cache_dir)` 解出到缓存目录。

`bae-package` 不依赖 GPU，bae-render 可以直接依赖。

### 19 core/plugins → 04（2026-11-20）

声明式 WGSL 效果（`bae_plugin::DeclarativeEffect`）直接实现 `bae_render::Effect`，与 07 的内置效果走同一条路径。约定见 `docs/spec/plugin-architecture.md` §11。两点请求：

1. **R04-11 `EffectParams` 补充字段**（都有 AE 对应项）：请求比例下的图层范围（layer bounds）、合成帧率和 `time_step`、工作空间是否线性化、最终渲染/预览标志和质量、快门角度/相位、每个效果实例稳定的随机种子。WGSL 前导里的 `BaeFrame` 已经留了这些字段，在补上之前插件宿主用输入范围代替图层范围，未知的填 0。
2. **R04-12 在 bae-render 之外构造 `EncodeContext`。** 现在 `ImageAllocator` 是 `pub(crate)`，效果 crate（19、07）没法写 GPU 单元测试。建议公开 `ImageAllocator`，或者加一个带简单纹理池的 `EncodeContext::for_tests(gpu, encoder, format)`。在此之前，19 把 GPU 编码部分拆成不依赖 `EncodeContext` 的函数来测试。

### 05 render/vector-text → 04（2026-11-20）

形状图层的图层源已提 PR（`[interface] vector`），接法见 `crates/bae-vector/README.md`。请求：

1. **`LayerSource::bounds()`**：形状和文字画在图层原点周围（AE 新建形状图层锚点 (0,0)、位置在合成中心），一半内容在负坐标，`[0, w) × [0, h)` 会把它裁掉。05 的 PR 里在 `source.rs` 加了带默认实现的 `fn bounds(&self) -> IRect`（默认 `IRect::from_size(size())`），`graph.rs` 里图层源的内容矩形改用它（两处、向后兼容）。如果你们有别的做法，请改掉，05 跟着调整。
2. **适配 03 的 `EvaluatedComp` 时**：`kind == shape` 的图层生成 `LayerContent::Custom { provider: "bae.shape", data: Arc<bae_anim::EvaluatedLayer>, hash: contents 的哈希, size: None }`；`kind == text` 的图层同样，provider 是 `"bae.text"`，哈希覆盖 `text`、`text_animators`。provider 由 apps/bae 注册（`bae_vector::render::ShapeProvider`、`bae_text::render::TextProvider`）。MockEvaluator 里"shape/text layers are drawn by 05 (not yet)"的警告可以换成这条路径。
3. **连续栅格化**：AE 的形状/文字图层总是按最终变换后的分辨率栅格化，放大不会糊。现在图层源只在图层空间按 `SourceRequest.scale` 栅格化。建议 `SourceRequest` 加上图层到输出的变换（或者至少是各轴缩放），矢量源据此直接按最终分辨率画（`ImageAlpha` / `GpuImage` 不变）。
4. **测试入口**：`EncodeContext` 的 `allocator` 是 `pub(crate)`，别的 crate 无法通过 trait 调 `LayerSource::render` 做测试。希望提供一个无窗口的测试入口，例如 `bae_render::testing::render_source(gpu, &dyn LayerSource, &SourceRequest) -> GpuImage`。05 目前直接测自己的 `ShapeSource::render_on`。

### 19 core/plugins → 04（2026-11-20，第二条）

**R04-13 从应用注册表取效果。** 插件效果（以及以后 07 用 `DeclarativeEffect::load_builtin` 注册的内置声明式效果）发布在应用的运行时注册表 `bae_app::Registry<dyn bae_render::Effect>` 里（ID = match name），定义在 `Registry<bae_model::EffectDef>` 里，做法和你们的 `Registry<dyn SourceProvider>` 一样。希望 `bae render` 和交互预览构建 `Renderer` 时，把 `app.registry::<dyn Effect>().snapshot()` 里的效果放进 `renderer.effects_mut()`（或者渲染图直接查快照），这样插件效果在命令行渲染和预览里都能用；卸载插件后重建渲染图即可。

### 20 interop/aex-compat → 04（2026-11-20）

AE 插件兼容层的预留请求，详见 `docs/spec/aex-compat.md` §4.5–§4.7、§5.2、§6。19 已提的 R04-1/2/3/4/5/7/9/11 我们附议（CPU 节点的 GPU 转换着色器和测量数据在 `crates/bae-aex-host`），只补增量：

1. **R04-05 CPU 节点渲染时按需拉取输入，且不会死锁（现在）。** 预渲染声明的输入在渲染时才真正渲染，声明了却没取用的不渲染；插件节点在工作线程里阻塞等上游时，这个线程能就地执行上游子图（或线程池补一个线程），线程池被插件节点占满也不会死锁。上游可能又是同一个插件模块的另一个实例（重入）。
2. **R04-01 预渲染只求边界（约定）。** 节点的 `pre_render` 可能被调用多次、也可能只为求边界而不跟随渲染，结果可以丢弃；每次计划最多执行一次渲染。
3. **验收请求。** 带与不带 CPU 节点机制编译，B1–B3 的帧时间在测量噪声内相同（不用 AE 插件的工程零代价）。

P3 再做（现在别堵死）：R04-06 节点自带暂存内存（进程外零拷贝）、R04-08 缓存键附加节点提供的字节、R04-10 "输入在一段时间内是否变化"查询、R04-13 插件内存计入缓存预算、R04-16 非像素缓存条目、R04-17 节点输出质量状态（失败 / 水印 / 降级）。

### 14 ui/edit → 04（2026-11-20，Pr 剪辑规划）

依据：`docs/spec/pr/`（Premiere Pro 基础功能清单）和 `docs/spec/pr/edit-model.md`（剪辑数据模型），PR #73。序列渲染的分工：14 的 `bae_edit::query::frame_at(seq, t)` 给出纯数据（从下到上的视频项：来源、源时间、片段时间、变换和不透明度、效果列表、过渡状态），04 把它变成渲染图（edit-model.md 第 10 节）。

1. **（0.0.1）渲染一帧序列**：把每个视频项当成一个 2D 素材图层（来源是 Asset、Comp 或嵌套 Sequence），按顺序叠加；`bae render` 支持 `--sequence <路径>`；节目监视器用同一条路径出画面。嵌套序列和动效合成作为图层源递归渲染，缓存按 StageEvent 失效（改动合成后序列里马上更新，是 14 的验收项）。
2. **（M2）两个输入的过渡节点**：过渡区间内同时渲染 A、B 两个片段（各自带效果和变换），按进度交给过渡效果合成；先支持交叉溶解和渐隐为黑色。过渡效果由 07 / 19 按 `EffectDef` 的"过渡"种类注册。
3. **（M4）剪辑的渲染顺序**：标准效果按 `effects/` 顺序先做，运动和不透明度最后做（Pr 的规则，PR-FX-007）；调整图层片段对下方所有轨道的合成结果做效果；**效果级蒙版**（效果只在蒙版内生效，再与原图混合，PR-FX-006）。
4. **（M4）渲染条状态**：按序列时间范围查询"已缓存 / 可实时 / 需要渲染"，给时间轴画 Pr 式渲染条；Enter 触发后台把入出点范围渲染进缓存（与 AE 的 RAM 预览同一机制）。
5. **（M4）渲染请求的"忽略效果"标志**：全局 FX 静音（PR-FX-009），以及导出时的后处理节点（叠加时间码、水印、LUT，PR-EXP-014）。

### 12 ui/motion（2026-11-20）：合成查看器接 GPU 预览

12 的合成查看器（`crates/bae-ui-motion/src/viewer.rs`）现在用 egui 画简化的矢量预览。希望 04 提供一个应用资源（例如 `Arc<CompPreview>`），查看器每帧调用：

- `request(comp: &PrimPath, time: Time, resolution: u32 /* 1、2、3、4 */, region: Option<Rect>) -> Option<ViewportTextureId>`：渲染（或从帧缓存取）这一帧，发布到 `ViewportTextureRegistry`，返回视口 id；渲染尚未完成时返回上一帧的 id。
- 一个"已缓存帧"的查询（`cached_times(comp)`），时间轴用来画绿条。
- 查看器拖动、数值拖动时的实时预览需要"临时覆盖"：查看器会把被拖动图层的变换覆盖到 `EvaluatedComp` 上（见 `viewer::apply_overrides`），所以接口最好能接收调用方给的 `EvaluatedComp`（或一个覆盖列表），而不是只接收 stage。

有了这个资源，查看器会优先显示视口纹理，没有时退回现在的简化预览。
