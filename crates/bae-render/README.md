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
| 03 动画 | `AnimEvaluator`（`anim.rs`） | 把 `bae_anim::EvaluatedComp` 转成 `RenderComp`：世界矩阵 → 2D 仿射，解析纯色/素材/预合成来源，token → 枚举。收到 `StageEvent` 时调用 `CompEvaluator::invalidate`。 |
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

## 已实现（第一阶段）

- 纯色、素材（bae-media 解码，零拷贝采样）、预合成（按 ROI 只渲染需要的部分）；变换（解析边缘覆盖，放大时与 AE 一样按源像素过渡，缩小时框滤波超采样）、不透明度。
- 混合模式：正常、相加、相乘、滤色、叠加（与 AE 26.2.1 逐像素对照，见 `tests/ae/README.md`）；其余回退为正常并警告。8/16 bpc 截断与 32 bpc 浮点两种语义。
- 轨道遮罩（Alpha / 亮度 × 正常 / 反转）、保留基础透明度。
- 纹理池、帧缓存（预算 + StageEvent 精确失效）、半分辨率等降采样、ROI。
- `bae render <工程> --comp <路径> [--frame N | --frames A-B|all] [-o out_####.png] [--format png|tiff|exr|h264|hevc|prores|prores4444] [--compression 档位] [--bits 8|16|32] [--threads N] [--resolution half] [--depth 8|16|32] [--alpha straight|premultiplied|background] [--roi X,Y,W,H] [--json]`。导出是流水线（异步回读 + 每核一个编码线程），`--json` 给出各阶段耗时；进程内用 `bae_render::export_frames`（渲染队列也用它）。
- 基准 B1：`cargo xtask bench -p bae-render`，方法和结果见 `docs/perf.md`。

还没做：遮罩、效果执行（接口已定）、调整图层、3D/摄像机、运动模糊、其余 33 种混合模式、视口纹理输出、线性混合。
