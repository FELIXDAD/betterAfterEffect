# 性能：基准与测量方法

性能是第一优先级（`docs/VISION.md`）。目标：常见动效工程 1080p 实时预览（≥ 30 fps），最终渲染在同一台机器上比 AE 快 5 倍以上。本文写清楚测什么、怎么测、怎么和 AE 比，以及目前的结果。基准由工作流 04 维护。

## 1. 基准工程

| 基准 | 定义（ARCHITECTURE §11） | 现在的实现 |
|---|---|---|
| B1 动效图形 | 1080p30，10 秒，50 个形状图层 + 10 个文字图层 + 模糊和发光 | `fixture::b1`：50 个动画纯色（代替形状：位置 3 个关键帧、旋转、缩放、不透明度；每 5 个里有 1 个用相加/滤色/相乘/叠加）+ 10 个动画 RGBA 纹理（代替文字，带软边 alpha）+ 全屏背景。模糊和发光等 07。 |
| B2 素材合成 | 4K ProRes 素材 + 调色 + 遮罩 | 未做（遮罩、06 调色） |
| B3 压力测试 | 500 个图层，多层预合成嵌套 | 未做 |

形状（05）、文字（05）、效果（07）就绪后，B1 换成真实的形状和文字图层，指标名不变，历史结果里注明换代。

## 2. 指标

运行：`cargo xtask bench -p bae-render`（只跑 B1：`--bench b1`）。基准用 bench profile（release + fat LTO），GPU 后端校验关闭，帧缓存关闭（每帧都真正渲染）。

| 指标 | 含义 |
|---|---|
| `preview_fps_full` / `preview_fps_half` | 全部帧连续提交渲染（不等 GPU），最后等 GPU 完成；帧数 ÷ 总时间。对应时间轴播放 / RAM 预览的吞吐。 |
| `latency_ms_p50` / `p95` | 每帧渲染并等 GPU 完成后再渲染下一帧，单帧耗时的分位数。对应拖动时间指针时的跟手程度。 |
| `export_fps_full` | 求值 + 渲染 + 回读 8 位 RGBA（不含文件编码）。对应最终渲染的计算部分。 |
| `eval_ms_mean` / `p95` | 03 的求值器（经适配器）每帧耗时。 |
| `graph_build_ms`、`encode_ms` | 建渲染图、编码 GPU 命令的 CPU 时间。 |
| `draws_per_frame`、`draw_calls_per_frame`、`passes_per_frame` | 图层绘制数、合批后的 draw call 数、render pass 数。 |

报告同时打印成表格并写到 `target/bench/b1.json`（`BAE_BENCH_OUT` 可改目录），CI 接入后按 JSON 记录趋势。`BAE_BENCH_FRAMES` 可以减少帧数做快速检查。

`bae render` 也打印每帧的求值和渲染耗时（`--json` 给 Agent 和脚本用），可以对任意工程做同样的测量。

## 3. 与 AE 对比的方法

**画面一致性**（每次改混合、遮罩、采样都要跑）：

1. `cargo run -p bae-render --example ae_compare -- prepare` 生成两边共用的素材（灰阶渐变、彩色渐变、带 alpha 的纹理）。
2. `tools/ae/aectl run crates/bae-render/tests/ae/render_cases.jsx`：AE 按 `crates/bae-render/tests/ae/cases.json` 搭出同样的合成（8 bpc、不线性混合、无色彩管理、图层质量"最佳"），每个用 `saveFrameToPng` 输出第 0 帧（预乘 RGBA）。
3. `cargo run -p bae-render --example ae_compare -- compare`：BAE 用命令搭同样的工程，经 03 的求值器和渲染器输出预乘 PNG，逐像素对比，打印每个用例的最大差、平均差和差值超过 1/3 的像素数。

**速度**（同一台机器，AE 和 BAE 先后运行，不同时跑别的重负载）：

- 最终渲染：同一工程在 AE 渲染队列里渲染全部帧（最佳质量、完整分辨率、8 bpc），用脚本里的 `Date` 计时；BAE 用 `bae render <工程> --comp <合成> --frames all` 计时。两边输出格式要一致（AE 的 PNG 序列需要输出模块模板；没有时用 TIFF 序列，并在结果里注明编码格式不同）。
- 预览：AE 的 RAM 预览帧率只能在界面里看，记录"缓存完成所需时间 ÷ 帧数"；BAE 用 `preview_fps_full`。
- 记录 AE 版本、项目位深、渲染器、机器型号、电源模式。

## 4. 结果

### B1（Apple M4，macOS，Metal，2026-11-20）

| 指标 | 结果 | 目标 |
|---|---:|---:|
| 完整分辨率预览 | 852 fps | ≥ 30 fps |
| 半分辨率预览 | 1076 fps | |
| 单帧延迟 p50 / p95 | 1.44 / 1.67 ms | < 33 ms |
| 导出（求值 + 渲染 + 回读） | 334 fps | |
| 求值（bae-anim，61 个图层） | 0.098 ms/帧 | |
| 建图 / 编码 GPU 命令 | 0.008 / 0.25 ms | |
| 每帧 | 51 个图层绘制 → 21 个 draw call、8 个 render pass | |

说明：形状和文字还是纯色和纹理代替品，没有模糊和发光，所以这是合成本身的开销上限；真实的 B1 要等 05 和 07。render pass 数来自相乘、叠加这类要读底图的混合模式（每个这样的图层要拷贝它覆盖的那块底图，再开一个 pass）。

**与 AE 的速度对比还没做**（方法见第 3 节）：B1 需要先能在 AE 里搭出同样的工程（15 的交换格式导入，或者从 `fixture::b1` 导出 JSON 给 JSX 搭建）。画面一致性对比已经做了，见 `crates/bae-render/tests/ae/README.md`。

### 体积

在 dev（21d55f3，已含 MCP、bae-color）上测 `cargo xtask size`：

| | release 二进制 |
|---|---:|
| dev，不含 `bae render` | 14.79 MiB |
| 加 `bae render`（bae-render + bae-anim，不含素材解码） | 15.22 MiB（+0.43 MiB） |
| 再加素材解码（bae-media：PNG/JPEG/WebP/EXR、macOS 视频） | 16.23 MiB（+1.44 MiB） |

`cargo bloat` 看 bae-render 自身约 134 KiB，其余是第一次进入应用的 bae-anim 和 bae-media 的解码器（exr、image-webp、zune-jpeg 等）。M0 的应用本体预算是 15 MiB，dev 已经只剩 0.21 MiB，所以：素材解码放在 bae-render 的 `media` 特性里（默认开，crate 自己的测试和基准都用），`apps/bae` 暂时不开，等 08 的媒体模块进入应用时一起开；M1 的应用本体预算单独提 PR 讨论。

## 5. 历史

| 日期 | 提交 | 机器 | preview full | preview half | latency p50 / p95 | export | eval | 说明 |
|---|---|---|---:|---:|---:|---:|---:|---|
| 2026-11-20 | PR 2 | Apple M4 | 852 fps | 1076 fps | 1.44 / 1.67 ms | 334 fps | 0.098 ms | 第一版：纯色 + 纹理代替形状和文字；`benches/results/2026-11-20-apple-m4.json` |
