# bae-render

GPU 合成与渲染图（工作流 04）。接口说明以代码里的文档注释为准（`cargo doc -p bae-render --open`），这里只列其他工作流最常用的部分。

## 数据流

```
Stage ──► CompEvaluator（03 的 bae-anim；合并前用 MockEvaluator）──► RenderComp
      ──► graph::build（节点、ROI 协商、剔除、缓存键）──► RenderGraph
      ──► Renderer（共享 GpuContext，RGBA16F 预乘缓冲）──► 纹理
      ──► 视口（零拷贝）/ PNG / 编码器（08）
```

## 给其他工作流

| 工作流 | 用什么 | 说明 |
|---|---|---|
| 03 动画 | `RenderComp`、`RenderLayer`、`CompEvaluator` | `RenderComp` 是纯数据（不含 GPU 类型）。03 可以直接产出它，或者 04 写一个从 `EvaluatedComp` 转换的适配器。需要的字段见 03 任务书「请求」。 |
| 05 矢量/文字 | `LayerSource`、`SourceProvider`、`LayerContent::Custom` | 形状和文字图层的求值结果放进 `CustomContent { provider: "bae.vector", data, hash }`，05 注册一个 `SourceProvider`，按请求区域把像素画进 `EncodeContext::allocate` 分配的纹理。 |
| 07 效果 / 19 插件 | `Effect`、`EffectLibrary`、`EffectParams` | 参数定义用 `bae_model::EffectDef`（与界面、Agent 共用）；`Effect` 负责边界变化（`output_bounds`）、输入区域（`input_roi`）和 GPU 编码（`render`）。 |
| 08 媒体 | `LayerContent::Footage` | 素材帧通过 bae-media 解码成共享设备上的 RGBA16F 预乘纹理，直接采样，不复制。 |
| 12 动效界面 | `FrameCache::cached_times`、`Renderer::invalidate` | 时间轴的「已缓存帧」绿条；把 `StageEvent` 交给渲染器做精确失效。 |
| 17 Agent | `bae render` | 无窗口渲染 PNG，Agent 用它「看」画面。 |

## 约定

- 工作缓冲：预乘 alpha，RGBA16F（可选 RGBA32F），存工作空间的编码值（默认与 AE 一样非线性混合）。
- 坐标：合成空间 y 向下；像素 `(x, y)` 覆盖 `[x, x+1) × [y, y+1)`。
- 图层顺序：`RenderComp::layers[0]` 是最上面的图层（AE 序号 1），与 `layers` 文件夹的子 Prim 顺序一致。
- 混合模式、轨道遮罩、遮罩模式的 token 与 schema 一致（`multiply`、`luma_inverted` …）。
- 缓存键：`(Prim, 时间, 参数哈希)`；`StageEvent` 按「最具体的依赖 + 该 Prim 自己的时间基准」精确失效（图层属性用图层时间）。
