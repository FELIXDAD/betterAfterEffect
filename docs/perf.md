# 性能：基准与测量方法

性能是第一优先级（`docs/VISION.md`）。目标：常见动效工程 1080p 实时预览（≥ 30 fps），最终渲染在同一台机器上比 AE 快 5 倍以上。本文写清楚测什么、怎么测、怎么和 AE 比，以及目前的结果。基准由工作流 04 维护。

## 1. 基准工程

| 基准 | 定义（ARCHITECTURE §11） | 现在的实现 |
|---|---|---|
| B1 动效图形 | 1080p30，10 秒，50 个形状图层 + 10 个文字图层 + 模糊和发光 | `fixture::b1`：50 个动画纯色（代替形状：位置 3 个关键帧、旋转、缩放、不透明度；每 5 个里有 1 个用相加/滤色/相乘/叠加）+ 10 个动画 RGBA 纹理（代替文字，带软边 alpha）+ 全屏背景。模糊和发光等 07。 |
| B2 素材合成 | 4K ProRes 素材 + 调色 + 遮罩 | 未做（遮罩、06 调色） |
| B3 压力测试 | 500 个图层，多层预合成嵌套 | 未做 |
| 蒙版 | 1080p 下 50 个带羽化蒙版的图层实时（0.0.1 蒙版验收项） | `benches/masks.rs`：50 个整屏纯色（不透明度 70%），每层一个羽化（20–120 px）的椭圆蒙版，**路径每帧都在动**（每帧所有蒙版都重新栅格化、重新羽化，最坏情况）；每 5 层多一个相减的矩形蒙版（走累加器），每 7 层反转（整层都要画），每 3 层有扩展且 X/Y 羽化不同。共 60 个蒙版。 |

形状（05）、文字（05）、效果（07）就绪后，B1 换成真实的形状和文字图层，指标名不变，历史结果里注明换代。

## 2. 指标

运行：`cargo xtask bench -p bae-render`（只跑 B1：`--bench b1`）。基准用 bench profile（release + fat LTO），GPU 后端校验关闭，帧缓存关闭（每帧都真正渲染）。

| 指标 | 含义 |
|---|---|
| `preview_fps_full` / `preview_fps_half` | 全部帧连续提交渲染（不等 GPU），最后等 GPU 完成；帧数 ÷ 总时间。对应时间轴播放 / RAM 预览的吞吐。 |
| `latency_ms_p50` / `p95` | 每帧渲染并等 GPU 完成后再渲染下一帧，单帧耗时的分位数。对应拖动时间指针时的跟手程度。 |
| `export_fps_full` | 求值 + 渲染 + 回读 8 位 RGBA（不含文件编码），每帧同步等回读完成。旧的 `bae render` 就是这样逐帧串行的。 |
| `export_fps_async` | 同上，但走导出流水线的异步回读（`ExportReadback`，3 个暂存缓冲在途）：GPU 渲第 N+1 帧时第 N 帧在映射、拷出。对应新 `bae render` 的计算部分上限。 |
| `eval_ms_mean` / `p95` | 03 的求值器（经适配器）每帧耗时。 |
| `graph_build_ms`、`encode_ms` | 建渲染图、编码 GPU 命令的 CPU 时间。 |
| `draws_per_frame`、`draw_calls_per_frame`、`passes_per_frame` | 图层绘制数、合批后的 draw call 数、render pass 数。 |

报告同时打印成表格并写到 `target/bench/b1.json`（`BAE_BENCH_OUT` 可改目录），CI 接入后按 JSON 记录趋势。`BAE_BENCH_FRAMES` 可以减少帧数做快速检查。

`bae render` 也打印每帧的求值和渲染耗时（`--json` 给 Agent 和脚本用），可以对任意工程做同样的测量，见下一节。

### `bae render` 的导出流水线与 `--json` 分段计时

导出（`bae_render::export_frames`，`bae render` 和以后的渲染队列共用）是流水线：

```text
渲染线程  求值 N ─ 渲染 N ─ 转换+拷贝 N（GPU）─ 取回 N−3 的映射 ─► 编码线程
GPU                 … 第 N−1 帧 … 第 N 帧 …
编码线程  从暂存缓冲拷出 ─► bae-media 序列写出（每个核一个线程，有界队列）压缩 + 写盘
```

- 回读：输出转换（预乘 → 直通 / 预乘 / 叠背景色，8 位或浮点）写进复用的目标纹理，再拷进 3 个轮换的暂存缓冲（`READBACK_DEPTH`），异步映射；只在三个都在途时等最老那一帧的那次提交，不等整个 GPU。映射好的缓冲交给编码线程拷出，拷完还回来复用。
- 编码：08 的序列写出（`ImageSequenceEncoder`）每个逻辑核一个线程（`--threads` 可改），队列每线程 1 帧；线程数再按 512 MiB 帧内存上限截断（4K 浮点帧也不会随核数涨内存），内存不随帧数增长。每个文件先在内存里压缩，再一次写盘。
- 格式：PNG（默认 `fast` = fdeflate + 自适应滤波；`--compression none|fastest|fast|balanced|high|0-9`）、TIFF（默认不压缩，和 AE 的 TIFF 公平对比；`lzw|deflate`）、EXR（默认 ZIP，`none|rle|zips|zip|piz`；`--bits 16|32`；EXR 一律预乘）；macOS 上还有 H.264 / HEVC / ProRes 422 HQ / ProRes 4444（08 的 AVFoundation 编码器）。`--format` 不给时按 `-o` 的扩展名选。
- 停止（渲染队列的「停止」、`export_frames` 的取消标志）：不再渲新帧，已经渲出的帧照样写完并保留（和 AE 一致），报告标记为已停止（渲染队列显示「已停止（N/M 帧）」）。AE 会为剩下的帧另建一个队列项，0.0.1 之后再做。
- 出错（写盘失败、磁盘满、某帧渲染失败）：停下并等编码线程写完，**保留已经写成功的文件**（出错帧之前的帧都会写出，写到一半的文件会删掉），错误里给出出错的文件或帧号（`frame N: …`），进程以非零码退出。

`--json` 的 `stages` 是每段的总毫秒数（除以帧数就是每帧）：

| 字段 | 含义 |
|---|---|
| `setup_ms` | 打开工程、建 GPU 设备和渲染器（第一帧之前）。进程启动不在内，固定开销用 `--frame 0` 的墙钟测。 |
| `eval_ms` | 求值。 |
| `render_ms` | 建渲染图、编码并提交 GPU 命令（CPU）。注意：旧版这个字段含同步回读，新版不含。 |
| `readback_wait_ms` | 渲染线程等 GPU（渲染 + 转换 + 拷贝）和映射的时间。大说明 GPU 是瓶颈。 |
| `readback_copy_ms` | 从暂存缓冲拷出（序列和影片在编码线程上，不占渲染线程）。 |
| `encoder_stall_ms` | 渲染线程等编码器（队列满）的时间。大说明编码 / 写盘是瓶颈。 |
| `encode_ms` / `write_ms` | 编码器的像素转换 + 压缩 / 写文件，**各线程相加**（并行，所以可以超过墙钟）。 |
| `drain_ms` | 最后一帧交出后等编码器写完。 |

另有 `throughput.fps`、`throughput.mb_per_s`、`output`（格式、压缩、文件数、字节数、编码线程数）和 `total_ms`（第一帧到最后一个字节写完）。

## 3. 与 AE 对比的方法

**画面一致性**（每次改混合、遮罩、采样都要跑）：

1. `cargo run -p bae-render --example ae_compare -- prepare` 生成两边共用的素材（灰阶渐变、彩色渐变、带 alpha 的纹理）。
2. `tools/ae/aectl run crates/bae-render/tests/ae/render_cases.jsx`：AE 按 `crates/bae-render/tests/ae/cases.json` 搭出同样的合成（8 bpc、不线性混合、无色彩管理、图层质量"最佳"），每个用 `saveFrameToPng` 输出第 0 帧（预乘 RGBA）。
3. `cargo run -p bae-render --example ae_compare -- compare`：BAE 用命令搭同样的工程，经 03 的求值器和渲染器输出预乘 PNG，逐像素对比，打印每个用例的最大差、平均差和差值超过 1/3 的像素数。

**速度**（同一台机器，AE 和 BAE 先后运行，不同时跑别的重负载）：

- 最终渲染：同一工程在 AE 渲染队列里渲染全部帧（最佳质量、完整分辨率、8 bpc），用脚本里的 `Date` 计时；BAE 用 `bae render <工程> --comp <合成> --frames all` 计时。两边输出格式要一致：PNG 对 PNG（`-o out_####.png`），不压缩对不压缩（AE「TIFF 序列」对 `bae render --format tiff`，两边都带 alpha）。
- 拆分：固定开销 = 只渲 1 帧（`--frame 0`）的墙钟；每帧 = (全部帧墙钟 − 固定开销) ÷ 帧数；BAE 的每帧再用 `--json` 的 `stages` 拆成求值 / 渲染 / 回读 / 编码 / 写盘。取 3 次中位数。不压缩的序列有 2.5 GB（B1），受磁盘写速度限制，记录磁盘型号和剩余空间。
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
| 导出，异步回读（2026-11-21） | 672 fps（同次同步回读 359 fps） | |
| 求值（bae-anim，61 个图层） | 0.098 ms/帧 | |
| 建图 / 编码 GPU 命令 | 0.008 / 0.25 ms | |
| 每帧 | 51 个图层绘制 → 21 个 draw call、8 个 render pass | |

说明：形状和文字还是纯色和纹理代替品，没有模糊和发光，所以这是合成本身的开销上限；真实的 B1 要等 05 和 07。render pass 数来自相乘、叠加这类要读底图的混合模式（每个这样的图层要拷贝它覆盖的那块底图，再开一个 pass）。

**与 AE 的速度对比**（Windows RTX 5090 D，`docs/test-reports/win5090/2026-11-21-speed-b1.md`，导出流水线之前的 dev）：AE 输出 PNG 22.645 s，BAE（逐帧串行 PNG）13.010 s，约 1.74 倍；固定开销 AE 约 7.88 s、BAE 约 0.96 s；扣掉后每帧 AE 约 49 ms、BAE 约 40 ms，其中 BAE 的求值、渲染、回读只占约 3 ms，其余是单线程 PNG 编码和写盘。流水线之后的数字等 5090 重测。画面一致性对比见 `crates/bae-render/tests/ae/README.md`。

### 蒙版（Apple M1 Pro，macOS，Metal，2026-11-21）

`cargo bench -p bae-render --bench masks`（120 帧，帧缓存关闭，每帧都真正渲染；JSON 写到 `target/bench/masks.json`）。

| 指标 | 结果 | 目标 |
|---|---:|---:|
| 完整分辨率预览（60 个蒙版每帧重算） | **247 fps** | ≥ 30 fps |
| 半分辨率预览 | 430 fps | |
| 同样的 50 个图层不加蒙版 | 174 fps | |
| 单帧延迟 p50 / p95 | 4.9 / 5.3 ms | < 33 ms |
| 建图（含路径展平、扩展描边）p50 / p95 | 0.27 / 0.28 ms | |
| 编码 GPU 命令 p50 | 0.26 ms | |
| 每帧 | 51 个节点 → 250 个 draw call、**5 个 render pass** | |

说明：

- 加了蒙版反而比不加快：蒙版的支撑范围（路径包围盒 + 扩展 + 3σ）会缩小图层的绘制范围，小蒙版在大图层上只画蒙版那一块。
- 做法（`crates/bae-render/src/mask.rs`）：路径在 CPU 上展平（kurbo，误差 1/20 像素），GPU 上按"每条边给它右边的像素加上精确面积"累加出绕数（抗锯齿是精确覆盖率，不是多重采样）；扩展用圆角连接的描边轮廓（扩大取并集、收缩减去），羽化是按轴分开的高斯模糊，大羽化按轴降采样，使每个方向的 σ 保持 3–6 个纹素（每遍最多 37 次采样，耗时和羽化大小无关）；7 种模式用固定管线混合合成。
- 一帧里所有蒙版共用图集（`exec/masks.rs`）：每页 3 个 pass（覆盖、解析 + 横向羽化、纵向羽化），多蒙版图层再加 1 个累加 pass。只有一个蒙版的图层直接在图层绘制时采样蒙版，不多一遍全屏。
- 优化过程（同一台机器、同一基准）：每个蒙版各 3 个 pass 时 50 fps（191 个 render pass/帧，编码 4.8 ms）→ 按轴降采样 78 fps → 图集 247 fps（5 个 pass/帧，编码 0.26 ms）。Apple GPU 上每个 render pass 都有固定开销，小 pass 多了是主要成本。
- 静态蒙版现在也每帧重算（渲染器只有整帧缓存，没有中间结果缓存）；按上面的数字这不是瓶颈，以后加中间缓存时蒙版节点已经标了 `cacheable`。

### 导出端到端（`bae render`，B1 300 帧，Apple M4 10 核，macOS，2026-11-21）

3 次取中位，墙钟含进程启动。之前 = dev 6aae5b8（逐帧串行：同步回读、单线程 PNG）；之后 = 本 PR（流水线 + 并行编码 + `png` 回到 opt-level 3）。

| | 墙钟 | 固定开销（`--frame 0`） | 每帧 | 加速 | 文件 |
|---|---:|---:|---:|---:|---:|
| 之前：PNG（fast） | 5.027 s | 0.033 s | 16.65 ms | 1× | 76 MB |
| 之后：PNG（fast，默认） | **0.468 s** | 0.043 s | **1.42 ms** | **10.7×** | 79 MB |
| 之后：PNG（fastest） | 0.452 s | 0.032 s | 1.40 ms | 11.1× | 88 MB |
| 之后：PNG（fast，8 线程） | 0.472 s | 0.034 s | 1.46 ms | 10.7× | 79 MB |
| 之后：TIFF 不压缩 | 1.262 s | 0.033 s | 4.10 ms | — | 2.49 GB |
| 之后：EXR（ZIP，half） | 2.185 s | 0.066 s | 7.06 ms | — | 193 MB |

之后每帧各阶段（ms/帧；编码、写盘是 10 个线程相加）：

| | 求值 | 渲染（CPU） | 等 GPU | 拷出 | 等编码器 | 编码 | 写盘 |
|---|---:|---:|---:|---:|---:|---:|---:|
| PNG fast | 0.20 | 0.61 | 0.43 | 0.85 | 0.003 | 5.61 | 0.46 |
| PNG fastest | 0.08 | 0.36 | 0.90 | 0.65 | 0.001 | 1.55 | 0.50 |
| TIFF 不压缩 | 0.16 | 0.56 | 0.52 | 0.81 | 2.34 | 0.68 | 33.5 |
| EXR ZIP | 0.22 | 1.10 | 0.13 | 1.12 | 5.02 | 68.9 | 0.95 |

之前每帧：求值 0.05 ms，渲染 + 同步回读 2.78 ms，PNG 编码 + 写盘约 13.8 ms（单线程；其中 `png` 在 opt-level "s" 下自适应滤波慢约 3.5 倍）。

说明：

- PNG：M4 上现在是 **GPU 受限**（渲染线程主要在等 GPU；B1 合成本身约 1.0–1.2 ms/帧，再加输出转换和拷贝）。编码器余量很大：fast 档 10 线程每秒能压约 1800 帧，`fastest` 和 8 线程都不改变墙钟。
- TIFF 不压缩：2.49 GB 写盘受磁盘限制（约 2 GB/s，本机磁盘剩余不足 5 GiB，第三次跑到 4.96 s）；编码只占 0.68 ms/帧。
- EXR ZIP：压缩（exr crate 的 zlib）是瓶颈；要快用 `--compression none`（文件 5 GB）或 `piz`。
- 计算部分单独看：`export_fps_full`（同步回读）359 fps → `export_fps_async`（流水线回读）672 fps（见下表）。

### 体积

`cargo xtask size`（dev 07862ec，M1 预算 30 MiB）：

| | release 二进制 |
|---|---:|
| dev，不含 `bae render` | 13.25 MiB |
| 加 `bae render`，不含素材解码（bae-render + bae-anim） | 13.72 MiB（+0.47 MiB） |
| 加素材解码（`media` 特性：bae-media 的 PNG/JPEG/WebP/EXR 和 macOS 视频） | 15.55 MiB（+2.30 MiB） |

导出流水线（2026-11-21，macOS arm64 strip 后，`cargo build --release -p bae`）：dev 6aae5b8 22,870,912 字节 → 23,568,144 字节，**+697,232 字节（+0.665 MiB）**。没有新依赖：以前 `bae render` 不调用 bae-media 的编码器，LTO 把它们整个删掉了；现在链接进来的是 EXR 写出（exr 的各种压缩器 +92 KiB，miniz_oxide +16 KiB）、PNG / TIFF / EXR 写出与并行写出（bae_media +104 KiB，含 macOS 影片编码器）、TIFF 编码（+17 KiB）、导出流水线（bae_render +33 KiB）和标准库的线程 / 通道实例化（cargo bloat 估算，.text 合计约 +387 KiB）。Windows 没有影片编码器那部分。`png` 从 opt-level "s" 改回 3：strip 后的二进制大小不变（同字节数）。

蒙版（render/masks，2026-11-21，macOS arm64 strip 后，`cargo xtask size`）：dev b7c37e2 24,238,352 字节 → 24,287,968 字节，**+49,616 字节（+48.5 KiB）**。没有新 crate（kurbo 已经随 bae-vector / vello 链接进应用）。

`cargo bloat`：bae-render 自身约 134 KiB，其余主要是第一次进入应用的 bae-media 解码器（exr、image-webp、zune-jpeg 等）和 bae-anim。素材解码是 bae-render 的 `media` 特性（默认开）；根 `Cargo.toml` 的条目关掉默认特性，`apps/bae` 显式打开，需要更小的二进制（例如只渲染形状的服务端）时可以不开。

## 5. 历史

| 日期 | 提交 | 机器 | preview full | preview half | latency p50 / p95 | export | eval | 说明 |
|---|---|---|---:|---:|---:|---:|---:|---|
| 2026-11-20 | PR 2 | Apple M4 | 852 fps | 1076 fps | 1.44 / 1.67 ms | 334 fps | 0.098 ms | 第一版：纯色 + 纹理代替形状和文字；`benches/results/2026-11-20-apple-m4.json` |
| 2026-11-21 | 导出流水线 | Apple M4 | 1004 fps | 1083 fps | 1.34 / 1.49 ms | 359 fps（异步回读 672 fps） | 0.033 ms | `bae render` B1 PNG 300 帧 5.03 s → 0.47 s；`benches/results/2026-11-21-apple-m4.json` |
| 2026-11-21 | render/masks | Apple M1 Pro | 247 fps | 430 fps | 4.9 / 5.3 ms | — | — | 蒙版基准（不是 B1）：50 个图层、60 个每帧都变的羽化蒙版；不加蒙版 174 fps |
