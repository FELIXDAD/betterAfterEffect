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

- [x] 公共接口（`crates/bae-media`）：`MediaFormat` / `MediaSource`（按帧号随机访问，`decode_gpu` / `decode_cpu`）、`MediaInfo`（色彩标签按 H.273/CICP、时码、帧率、帧数、像素长宽比、alpha 直通/预乘/忽略）、`FrameRequest`、`GpuFrame`（RGBA16F 工作格式，默认预乘）/ `CpuFrame`、`MediaFormats`（先按文件头识别、再按扩展名）、`FrameCache`（按显存字节的 LRU）、`MediaModule`（内置格式注册到 `Registry<dyn MediaFormat>`，插件注册到同一处）。时间用 `bae-model` 的 `Time`（flicks）和 `FrameRate`。
- [x] 序列帧：`name_0001.png` / `shot.1001.exr` 识别（数字取文件名最后一段，补零位宽固定），缺帧报告，目录扫描分组；`ImageSequenceSource`（序列默认 30 fps，可设）。
- [x] 图像格式：PNG（8/16 位、调色板、tRNS、cICP/sRGB/gAMA/cHRM/iCCP、像素比）、JPEG（zune-jpeg，ICC、JFIF 像素比）、WebP（image-webp）、OpenEXR（half/float，数据窗口放进显示窗口，色度、时码、像素比）。GPU 上传后用 compute pass 转成 RGBA16F 并处理 alpha（预乘/反预乘/强制不透明）。
- [x] macOS 视频解码：AVFoundation + VideoToolbox（H.264、HEVC、ProRes），IOSurface 支撑的 CVPixelBuffer → CVMetalTexture → wgpu 纹理（wgpu-hal，零拷贝），再转工作格式；8 位源用 BGRA，10 位以上和带 alpha 的 ProRes 4444 用 64RGBAHalf；帧精确 seek（向前小步顺读，其余重建 reader 并从目标帧内 1/8 帧处开始）；29.97 等非整数帧率逐帧验证。
- [ ] Windows 视频解码：目前只有占位（`media_foundation`，能通过 `cargo clippy --target x86_64-pc-windows-msvc -p bae-media`），实现放下一个 PR。
- [ ] TIFF、PSD（保留图层）、HEIC
- [x] 跳帧速度：拖动播放头（连续随机跳帧）不再每次重建 `AVAssetReader`：输出开了 `supportsRandomAccess`，跳帧读单帧时间范围，读完确认范围已读空后用 `resetForReadingTimeRanges` 换位置；往后顺读时切换成开放范围。4K 实测（机器负载较高时 A/B 交替各跑两轮）：拖动 ProRes 422 / 4444 约 8–9 ms/帧（原来 40–54 ms），H.264（GOP 30）约 27 ms/帧（原来约 46 ms，余下主要是从关键帧解码）；顺序解码速度不变。从播放状态跳出的第一次仍要重建（约 40–65 ms）。
- [ ] 进一步：播放中跳帧也不重建（有界的播放窗口），以及长 GOP 素材的拖动缓存 / 预读
- [ ] 预读（后台线程提前解码）、YUV 直出（420v/x420 双平面 + 着色器转换，进一步降低 4K 解码开销）、QuickTime 时码轨、关键帧表（`sync_frames`）
- [x] 编码导出接口（`export`）：`EncodeSettings`（对应 AE 输出模块：编码、尺寸、帧率、RGB / RGB+Alpha 及直通/预乘、位深、色彩标签、起始编号与时码）、`Encoder` / `EncoderFormat`（注册到 `Registry<dyn EncoderFormat>`）、`MediaEncoders`、`convert_frame`（CPU 像素格式与 alpha 转换）、`FrameReadback`（GPU 帧异步回读，默认 2 帧在途）、`render`（逐帧生产 → 回读 → 编码，进度回调、取消时删除半成品）、`RenderQueue`（运行时队列：排队、取消、重排、重新排队、结果）。工程里保存哪些渲染任务属于文档模型，这里只负责执行。
- [x] PNG 序列（8/16 位，RGB 或 RGBA，sRGB/gAMA/pHYs）、OpenEXR 序列（half/float，None/RLE/ZIPS/ZIP/PIZ，色度、像素比、逐帧时码）编码；文件名支持 `name_[####].png`、`name_####.png`、`name_0000.png`。
- [x] macOS 编码（`apple::AvWriterFormat`）：AVAssetWriter 驱动 VideoToolbox，H.264 / HEVC 优先硬件编码，ProRes Proxy / LT / 422 / HQ / 4444 / 4444 XQ；8 位用 32BGRA，ProRes 和 10 位 HEVC 用 64RGBAHalf；ProRes 4444 直通 alpha，HEVC alpha 可选直通 / 预乘；按 H.273 写色彩标签（原色、传递函数、矩阵），像素比、码率、质量、关键帧间隔。解码端识别 HEVC alpha 及其直通 / 预乘。
- [ ] 编码导出后续：GPU 端转换后直接写进 IOSurface（省掉 CPU 转换和拷贝）、时码轨、音频混入、Windows Media Foundation 编码
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

- `bae-media` 依赖 `bae-model`（只用 `Time` / `FrameRate`），按 02 的要求避免两套时间体系。
- 媒体帧号一律从素材第一帧起算、从 0 开始；序列 `name_0101.png` 的第 0 帧是 0101。
- 颜色只报告不转换：`GpuFrame` 的值保持素材自身的编码（不线性化），由 06 根据 `ColorInfo` 处理。未标记的素材报告为 `Unspecified`，默认规则由 06 决定。
- VideoToolbox 把「最大关键帧间隔」当上限，实际可能更密（设 10 时给出每 9 帧一个）；裁片一律以样本表里的关键帧为准。
- 待用户决定：序列缺帧时是报错（现在的做法，合成里显示占位）还是沿用前一帧。

## 请求（其他工作流在这里留言）
