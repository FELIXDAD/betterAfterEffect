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
- 下一步：GPU 执行（纹理池、合成着色器、5 种混合模式、轨道遮罩）、`bae render` 输出 PNG、B1 基准与 `docs/perf.md`、与 AE 对照。

- [ ] 渲染图与纹理池
- [ ] 混合模式、遮罩、轨道遮罩
- [ ] 3D 与运动模糊
- [ ] 效果框架与参考效果
- [ ] 缓存与预览
- [ ] bae render 命令行
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

### 20 interop/aex-compat → 04（2026-11-20）

AE 插件兼容层的预留请求，详见 `docs/spec/aex-compat.md` §4.5–§4.7、§5.2、§6。19 已提的 R04-1/2/3/4/5/7/9/11 我们附议（CPU 节点的 GPU 转换着色器和测量数据在 `crates/bae-aex-host`），只补增量：

1. **R04-05 CPU 节点渲染时按需拉取输入，且不会死锁（现在）。** 预渲染声明的输入在渲染时才真正渲染，声明了却没取用的不渲染；插件节点在工作线程里阻塞等上游时，这个线程能就地执行上游子图（或线程池补一个线程），线程池被插件节点占满也不会死锁。上游可能又是同一个插件模块的另一个实例（重入）。
2. **R04-01 预渲染只求边界（约定）。** 节点的 `pre_render` 可能被调用多次、也可能只为求边界而不跟随渲染，结果可以丢弃；每次计划最多执行一次渲染。
3. **验收请求。** 带与不带 CPU 节点机制编译，B1–B3 的帧时间在测量噪声内相同（不用 AE 插件的工程零代价）。

P3 再做（现在别堵死）：R04-06 节点自带暂存内存（进程外零拷贝）、R04-08 缓存键附加节点提供的字节、R04-10 "输入在一段时间内是否变化"查询、R04-13 插件内存计入缓存预算、R04-16 非像素缓存条目、R04-17 节点输出质量状态（失败 / 水印 / 降级）。
