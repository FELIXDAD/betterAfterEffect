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
- 下一步：遮罩（05 的路径栅格化或自带）、效果框架执行与三个参考效果、调整图层、其余混合模式（继续用 AE 对照）、视口纹理输出（零拷贝给 12）、B1 与 AE 渲染速度对比、B3。

- [x] 渲染图与纹理池
- [ ] 混合模式、遮罩、轨道遮罩
- [ ] 3D 与运动模糊
- [ ] 效果框架与参考效果
- [ ] 缓存与预览
- [x] bae render 命令行（PNG）
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
