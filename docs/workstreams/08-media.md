# 08 · 媒体编解码与格式

| 项 | 内容 |
|---|---|
| 分支 | `io/media` |
| Orca 工作区 | `bae-08-media` |
| 阶段 | P1 |
| 依赖 | 01（bae-gpu） |
| 负责目录 | `crates/bae-media/` |

## 目标

用系统自带的硬件编解码读写视频，直接解码到 GPU 纹理；常用图像格式用纯 Rust 库。不内置 FFmpeg。

## 要做

- **图像**：PNG、JPEG、WebP、TIFF、EXR、PSD（保留图层）、HEIC（macOS 用系统框架）；序列帧识别（`name_0001.png`）。
- **视频解码**：
  - macOS：VideoToolbox / AVFoundation（H.264、HEVC、ProRes），经 IOSurface 零拷贝变成 Metal 纹理。
  - Windows：Media Foundation（H.264、HEVC），经 D3D 共享纹理零拷贝；ProRes 交给可选插件。
  - 帧精确的随机访问（seek）、解码缓存、预读。
- **代理**：生成低分辨率代理，预览时自动切换。
- **音频**：symphonia 解码，cpal 播放，与视频同步；为时间轴生成波形数据。
- **编码导出**：H.264、HEVC（硬件编码）；ProRes 422/4444（macOS 原生）；PNG、EXR 序列；带透明通道的输出；渲染队列的数据结构（任务、进度、取消）。
- **裁片能力（给 21 打包用）**：按源时间区间导出片段；优先无损直拷（按关键帧/GOP 边界切，对不齐时扩展到最近关键帧），必要时重新编码；查询关键帧位置。
- **可选 FFmpeg 插件**：动态加载 LGPL 构建的 FFmpeg，按需下载，不进主安装包；用于长尾格式。
- **元数据**：色彩空间（交给 06 解释）、时码、帧率、像素长宽比、alpha 类型（直通/预乘）。

## 对外接口

- `MediaSource`：打开、读取信息、`decode_frame(t) -> GPU 纹理`。
- `Encoder`：接收 GPU 帧（异步回读）并写文件。
- 实现 04 的图层源 trait（素材图层）。

## 验收

- 4K ProRes 422 在 macOS 上实时播放；4K H.264 在两个平台上实时播放。
- 随意拖动播放头时取帧准确（与帧号对得上）。
- 加入本工作流后 release 体积增加 ≤ 3 MB。

## 不做

- 剪辑界面（14）、色彩变换实现（06）。

## 进度

- 2026-11-25，Mac 渲染队列支持 QuickTime 的 ProRes 422、422 HQ、4444、H.264 和 HEVC，音轨可选 AAC 或 24 位 PCM。启动时在后台逐一准备五种系统编码器。已在 Apple Silicon 写出影片并重新解码音轨。

- [x] 公共接口（`crates/bae-media`）：`MediaFormat` / `MediaSource`（按帧号随机访问，`decode_gpu` / `decode_cpu`）、`MediaInfo`（色彩标签按 H.273/CICP、时码、帧率、帧数、像素长宽比、alpha 直通/预乘/忽略）、`FrameRequest`、`GpuFrame`（RGBA16F 工作格式，默认预乘）/ `CpuFrame`、`MediaFormats`（先按文件头识别、再按扩展名）、`FrameCache`（按显存字节的 LRU）、`MediaModule`（内置格式注册到 `Registry<dyn MediaFormat>`，插件注册到同一处）。时间用 `bae-model` 的 `Time`（flicks）和 `FrameRate`。
- [x] 序列帧：`name_0001.png` / `shot.1001.exr` 识别（数字取文件名最后一段，补零位宽固定），缺帧报告，目录扫描分组；`ImageSequenceSource`（序列默认 30 fps，可设）。
- [x] 图像格式：PNG（8/16 位、调色板、tRNS、cICP/sRGB/gAMA/cHRM/iCCP、像素比）、JPEG（zune-jpeg，ICC、JFIF 像素比）、WebP（image-webp）、OpenEXR（half/float，数据窗口放进显示窗口，色度、时码、像素比）。GPU 上传后用 compute pass 转成 RGBA16F 并处理 alpha（预乘/反预乘/强制不透明）。
- [x] macOS 视频解码：AVFoundation + VideoToolbox（H.264、HEVC、ProRes），IOSurface 支撑的 CVPixelBuffer → CVMetalTexture → wgpu 纹理（wgpu-hal，零拷贝），再转工作格式；8 位源用 BGRA，10 位以上和带 alpha 的 ProRes 4444 用 64RGBAHalf；帧精确 seek（向前小步顺读，其余重建 reader 并从目标帧内 1/8 帧处开始）；29.97 等非整数帧率逐帧验证。
- [x] Windows 视频解码：Media Foundation Source Reader 通过 DXGI 设备管理器在 D3D11 上解码 H.264、HEVC Main / Main 10。跳转复用同一 reader，由容器定位关键帧，再按显示时间戳取目标帧。按显示区域裁掉编码填充，视频帧数取视频轨长度。缺少 HEVC 解码器时，导入提示检查系统的 HEVC 视频扩展。
- [x] TIFF（静帧和序列）：8 / 16 位整数、32 位浮点，灰度 / RGB，有无 alpha；无压缩、LZW、Deflate、PackBits、JPEG。直接用 `tiff` crate 解码。alpha 方式读 `ExtraSamples`：1 = 预乘，2 或没有这个标签 = 直通，**明确写 0（未指定）= 预乘**——与 AE 2026 实际输出对比发现，AE 的 "TIFF Sequence with Alpha" 写的是预乘像素、标签却是 0，而 `image` 和 `tiff` 都会把这种「未指定」的第 4 个样本丢掉，原来读出来是不透明的；现在遇到这种文件时，先在内存里把这一个标签值改成 1 再解码。用 AE 渲染的文件核对过：alpha 128，颜色是预乘值。
- [ ] PSD（保留图层）、HEIC
- [x] 跳帧速度：拖动播放头（连续随机跳帧）不再每次重建 `AVAssetReader`：输出开了 `supportsRandomAccess`，跳帧读单帧时间范围，读完确认范围已读空后用 `resetForReadingTimeRanges` 换位置；往后顺读时切换成开放范围。4K 实测（机器负载较高时 A/B 交替各跑两轮）：拖动 ProRes 422 / 4444 约 8–9 ms/帧（原来 40–54 ms），H.264（GOP 30）约 27 ms/帧（原来约 46 ms，余下主要是从关键帧解码）；顺序解码速度不变。从播放状态跳出的第一次仍要重建（约 40–65 ms）。
- [x] 打开 `.baez` 里的素材：`MediaLocation`（磁盘文件，或容器里按「存储」方式放入的字节区间）+ `MediaLibrary::resolve`：嵌入条目按容器版本提取一次（保留条目目录结构，序列帧会落在同一目录；原子写入；不可信的条目名不能写到缓存目录外面），返回所有解码器（包括 AVFoundation）都能打开的路径。bae-media 不依赖 bae-package（21 依赖 08），由 bae-package 把 `AssetLocation::Packaged` 映射成 `MediaLocation::Embedded`。以后可以用 AVAssetResourceLoader 按字节区间直接读，省掉提取。
- [ ] 进一步：播放中跳帧也不重建（有界的播放窗口），以及长 GOP 素材的拖动缓存 / 预读
- [ ] 预读（后台线程提前解码）、YUV 直出（420v/x420 双平面 + 着色器转换，进一步降低 4K 解码开销）、QuickTime 时码轨、关键帧表（`sync_frames`）
- [x] 编码导出接口（`export`）：`EncodeSettings`（对应 AE 输出模块：编码、尺寸、帧率、RGB / RGB+Alpha 及直通/预乘、位深、色彩标签、起始编号与时码）、`Encoder` / `EncoderFormat`（注册到 `Registry<dyn EncoderFormat>`）、`MediaEncoders`、`convert_frame`（CPU 像素格式与 alpha 转换）、`FrameReadback`（GPU 帧异步回读，默认 2 帧在途）、`render`（逐帧生产 → 回读 → 编码，进度回调、取消时删除半成品）、`RenderQueue`（运行时队列：排队、取消、重排、重新排队、结果）。工程里保存哪些渲染任务属于文档模型，这里只负责执行。
- [x] PNG 序列（8/16 位，RGB 或 RGBA，sRGB/gAMA/pHYs）、OpenEXR 序列（half/float，None/RLE/ZIPS/ZIP/PIZ，色度、像素比、逐帧时码）编码；文件名支持 `name_[####].png`、`name_####.png`、`name_0000.png`。
- [x] macOS 编码（`apple::AvWriterFormat`）：AVAssetWriter 驱动 VideoToolbox，H.264 / HEVC 优先硬件编码，ProRes Proxy / LT / 422 / HQ / 4444 / 4444 XQ；8 位用 32BGRA，ProRes 和 10 位 HEVC 用 64RGBAHalf；ProRes 4444 直通 alpha，HEVC alpha 可选直通 / 预乘；按 H.273 写色彩标签（原色、传递函数、矩阵），像素比、码率、质量、关键帧间隔。解码端识别 HEVC alpha 及其直通 / 预乘。
- [x] 导出提速：`Encoder::input_format` 声明编码器要的像素格式和 alpha，渲染循环在回读前用 compute pass 转好（`MediaGpu::convert_for_export`；BGRA 用 RGBA8 纹理加通道交换，16 位用 rgba16uint），只回读最终字节，编码器直接借用不再逐像素转换；PNG / EXR 序列由最多 4 个线程并行压缩写盘（有界队列控制 4K 内存）。4K 实测（机器负载高，A/B 交替两轮）：H.264 25 → 47 fps，ProRes 422 HQ 23 → 62–69 fps，ProRes 4444 + alpha 21 → 50–66 fps，EXR half 7.3 → 12.6 fps。
- [x] 导出提速第二轮（04 的导出流水线 PR 顺带改，2026-11-21）：TIFF 序列编码（`VideoCodec::Tiff`，不压缩 / LZW / Deflate，8 / 16 位，ExtraSamples 标直通或预乘）；PNG 压缩档 `EncodeSettings::png_compression`（none / fastest / fast / balanced / high / zlib 1–9，默认 fast）；序列写出线程默认每个逻辑核一个（原来最多 4 个），`EncodeSettings::threads` 可改，按 512 MiB 帧内存（`SEQUENCE_QUEUE_BYTES`）截断；`Encoder::push_frame_owned`（默认实现转调 `push_frame`，序列编码器不再拷贝整帧，像素转换挪到写出线程）；每个文件先在内存里压缩再一次写盘，`EncodeOutput::stats`（编码 / 写盘时间、字节、线程数）；`export::write_image_file`（单张图写到指定路径）、`export::sequence_input_format`；写盘失败报 `MediaError::Write`（“cannot write …”）。
- [x] Windows 影片导出：渲染队列和 `bae render` 共用 Media Foundation Sink Writer，输出 8 位 RGB 的 H.264 / HEVC MP4。HEVC 需要系统提供硬件编码器。队列按本机编码器启用格式，ProRes 项提示需要 macOS。
- [ ] 编码导出后续：渲染直接写进 IOSurface 支撑的 pixel buffer（省掉回读和拷贝）、时码轨、音频混入。
- [x] 音频接口（`audio`）：`AudioSource`（按采样帧随机读取）、采样与 `Time` 精确换算、`MemoryAudio`、`Waveform`（min/max 峰值、降采样出缩放级别、按时间取可见区间）。
- [x] 音频文件解码（`FileAudio`，symphonia）：WAV、AIFF、MP4 / QuickTime / M4A（AAC）、MP3、FLAC，按采样帧随机读取（顺读继续解码，跳转时精确 seek）；`audio::probe` 只读文件头列出音频流，影片的 `MediaInfo.audio` 由它填写。symphonia 的 MP4 读取不处理编码器延迟，AAC 会比画面晚约 44 ms（48 kHz 下 2112 个采样）；新增的小型 box 读取器从音频轨的编辑列表或 iTunes 无缝信息（`iTunSMPB`）里取延迟和有效长度并裁掉。测试用代码生成 WAV，macOS 上测试时用系统的 `afconvert` 生成 AAC / FLAC / AIFF；AAC 在 0.5 s 处放一个脉冲，解码后位置仍在 0.5 s。
- [ ] cpal 播放、音画同步时钟；导出时混入音频
- [x] `MediaLibrary`（给 04 / 14 / 缩略图 / Agent）：线程安全、全部 `&self`；每个文件（按打开选项区分）只打开一次，超出上限时关掉最久没用的；素材时间 → 帧号（超出两端时停在首帧 / 末帧，与 AE 时间重映射一致）；解码结果进共享 `FrameCache`；`content_hash` 给渲染缓存用（文件版本、打开选项、交付方式、帧号）；`invalidate(path)` 也能命中以该文件为一帧的序列，并换新的内容哈希。`MediaModule` 在 `finish` 里提供共享的 `Arc<MediaLibrary>` 资源。04 的素材 `LayerSource` 只需要薄薄包一层（`frame_at` → `GpuImage::from_texture`，零拷贝），示例写在 `library.rs` 的文档里。
- [ ] 代理与 FFmpeg 插件（`FrameRequest::downscale` 与格式注册表已预留）
- [x] 裁片接口（`trim`，给 21）：`TrimRequest` 按源时间区间（`TimeRange`）+ 余量 + 模式；`plan_trim` 换算成整帧，直拷时扩展到 GOP 边界（起点取之前最近的关键帧，终点延到下一关键帧之前）；`gop_start` / `next_gop_start`；`Trimmer`（注册到 `Registry<dyn Trimmer>`）；`SequenceTrimmer`（序列只复制用到的帧，保留文件名）。
- [x] 影片关键帧表：`AvSource::sync_frames` 用 AVSampleCursor 读样本表（不读媒体数据），经编辑列表换算成帧号。
- [x] 影片裁片（`apple::AvTrimmer`）：直拷整 GOP 的压缩样本（视频和音频轨，AVAssetReader / AVAssetWriter 不带编码设置），或精确帧重编码成 ProRes（默认 422 HQ，有 alpha 时 4444，可在请求里指定编码）。
- [x] `MediaInfo::asset_attributes()`：按 `bae_model::schema` 的 Asset 属性名输出（kind、width、height、frame_rate、duration、pixel_aspect、alpha_mode、has_video、has_audio、sequence:*），测试里逐项对照 schema 的类型和枚举值。

### 与 AE 2026 渲染结果的对比（`tests/ae_reference.rs`，脚本 `tests/ae/render_refs.jsx`）

测试合成：左半边不透明纯色 (0.75, 0.25, 0.5)，右半边 50% 不透明度纯色 (0.2, 0.6, 0.9)；AE 模板 "H.264 - Match Render Settings - 15 Mbps"、"High Quality"（ProRes 422）、"High Quality with Alpha"（ProRes 4444）。

| | AE | bae-media |
|---|---|---|
| 1920×1080 标签（原色 / 传递 / 矩阵 / 范围） | 709 / 709 / 709 / 有限范围 | 相同 |
| 320×180 标签 | SMPTE-C（601）/ 709 / 601 / 有限范围 | 已改成相同（高度 < 720 用 601） |
| 纯色解码值（理论值 191.25, 63.75, 127.5） | H.264 190/64/127，ProRes 190.5/64.1/127.2 | 我们编码再解码：H.264 192/64/129，ProRes 191/64/128 |
| ProRes 4444 alpha | 默认预乘（"Premultiplied (Matted)"），文件里不标记 | 按设置写直通或预乘；读取时文件不标记，默认按直通，`CpuFrame::guess_alpha()` 能判断出预乘（AE「猜测」的做法） |

- 读 AE 的文件颜色正确（与理论值差 ≤ 1.3 个 8 位码值）；我们编码的标签与 AE 一致。
- `MediaInfo` 现在报告文件里真实的 YCbCr 矩阵和范围；交付的帧仍是全范围 RGB。

### 说明与待定

#### Windows 影片进 GPU

RTX 5090 D v2（驱动 616.92）上，两种解码器经过视频处理器输出的 D3D11 纹理均为 `MiscFlags = 0`。wgpu 的 D3D12 后端可以包装 `ID3D12Resource`，但这些系统分配的纹理不能导出 D3D12 所需的 NT 共享句柄。首版把有效画面读回，再上传一次并转成合成用的 RGBA16F。直接共享需要可控制的纹理分配，以及两个 GPU 队列间的同步。[微软的共享句柄要求](https://learn.microsoft.com/en-us/windows/win32/api/dxgi/nf-dxgi-idxgiresource-getsharedhandle)

Windows 11、D3D12、release 实测，单位为毫秒/帧。素材为 29.97 fps、GOP 12、带 B 帧的 48 帧测试图。预热后测 47 帧，每次等待 GPU 完成。单独上传复用一张已解码的 CPU 帧，顺播和跳帧都从源文件取帧，含上传和工作格式转换。跳帧按步长 17 在素材内循环。

| 素材 | 单独上传及转换 | 顺播 | 跳帧 |
|---|---:|---:|---:|
| H.264 1920×1080 | 1.30 | 8.75 | 11.95 |
| H.264 3840×2160 | 3.75 | 16.26 | 28.58 |
| HEVC Main 1920×1080 | 1.01 | 8.83 | 11.73 |
| HEVC Main 3840×2160 | 3.85 | 16.53 | 28.12 |

`crates/bae-media/tests/fixtures/generate_mf.py` 用外部 FFmpeg 生成素材。H.264 样片的视频比音频晚半秒开始，也先于音频结束，用于核对视频帧数和跨 GOP 跳转。仓库附有小尺寸样片，可以直接运行 `cargo test -p bae-media`。缺少 Media Foundation 或 D3D11 视频设备的 runner 会跳过相应端到端测试，缺少 HEVC 解码器或编码器时只跳过对应场景。加 `-- --nocapture` 可查看具体原因。复测上表时，用脚本的 `--size` 和 `--out` 生成大图，令 `BAE_MF_BENCH` 指向其中一个 MP4，运行 `cargo test --release -p bae-media --lib benchmark_upload -- --ignored --nocapture`。

#### 其他说明

- `bae-media` 依赖 `bae-model`（只用 `Time` / `FrameRate`），按 02 的要求避免两套时间体系。
- 媒体帧号一律从素材第一帧起算、从 0 开始；序列 `name_0101.png` 的第 0 帧是 0101。
- 颜色只报告不转换：`GpuFrame` 的值保持素材自身的编码（不线性化），由 06 根据 `ColorInfo` 处理。未标记的素材报告为 `Unspecified`，默认规则由 06 决定。
- VideoToolbox 把「最大关键帧间隔」当上限，实际可能更密（设 10 时给出每 9 帧一个）；裁片一律以样本表里的关键帧为准。
- **体积**：`bae-media` 合计 +2,819,536 字节（2.69 MiB，约 2.82 MB），离任务书的 3 MB 预算不远了。后面的格式（HEIC、PSD 合成图）优先用系统框架（macOS ImageIO / Windows WIC），不再加纯 Rust 解码器。
- 待用户决定：序列缺帧时是报错（现在的做法，合成里显示占位）还是沿用前一帧。

## 请求（其他工作流在这里留言）

- 21（总协调转达）：从 `.baez` 打开工程时，素材通过 `AssetLocator::locate` 定位，`MediaLibrary` 要能处理 Packaged 条目。——已完成：见上面的 `MediaLocation` / `MediaLibrary::resolve`。

### 14 ui/edit → 08（2026-11-20，Pr 剪辑规划）

依据：`docs/spec/pr/`（Premiere Pro 基础功能清单）和 `docs/spec/pr/edit-model.md`（剪辑数据模型），PR #73。剪辑工作区对 08 依赖最重，按优先级：

1. **（0.0.1 / M2）多轨混音引擎 + 播放时钟**：现在没有模块负责混音。建议在 `bae-media` 里加一个与界面无关的混音模块：输入是 14 的 `frame_at` / `segments` 给出的音频项（来源、源时间、声道映射 `source_channels`、增益 `audio:gain`、音量 `audio:level` 和声像 `audio:pan` 的关键帧、轨道静音/独奏），输出到 cpal；以音频设备为主时钟驱动视频播放（音画同步）。动效工作区的音频预览也用它。M4 再加轨道推子、轨道效果插槽、子混合、主轨表电平、写自动化（PR-AUD-006 至 PR-AUD-010）。
2. **（M2）导出时混入音频**：H.264 / HEVC / ProRes 导出带声音（`render` 循环里同时拉混音结果），否则剪辑导不出成片。
3. **（M4）J/K/L 倒放与快速穿梭**：往回读和跳帧解码（长 GOP 素材按 GOP 缓存解码结果）；倍速时音频跟着变速播放（PR-MON-005）。拖动播放头时的音频擦洗（PR-MON-021）。
4. **（M4）代理**：按收录预设转码生成代理（复用编码器和 `RenderQueue`）、连接代理、`MediaLibrary` 按全局开关解析到代理或原片，`content_hash` 区分两者（PR-EXP-001 至 PR-EXP-003）。代理路径记在 Asset 的 `proxy:file:*`（02 的请求）。
5. **（M4）波形峰值缓存落盘**（工程 `cache/`），长素材后台生成（PR-AUD-005、PR-AUD-022）。
6. **（以后）** 源时间码（QuickTime `tmcd` 轨，EDL / XML 需要）、ITU-R BS.1770 响度测量与导出响度标准化（PR-AUD-013、PR-AUD-018）、按音频波形对齐（多机位、合并剪辑）、MP4 章节标记写入（PR-MRK-003）。
