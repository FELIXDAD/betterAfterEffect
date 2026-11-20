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
- [ ] 跳帧速度：4K 随机跳帧目前约 75 ms/帧（基准见 `tests/video.rs` 的 `decode_throughput_4k`；顺序解码 4K H.264 约 300 fps、ProRes 约 200 fps，测试素材是纯色条，比真实素材容易），主要花在重建 `AVAssetReader` 上，连全帧内编码的 ProRes 也一样。下一步改用 `supportsRandomAccess` + `resetForReadingTimeRanges`，或直接用 `VTDecompressionSession` + 样本表。
- [ ] 预读（后台线程提前解码）、YUV 直出（420v/x420 双平面 + 着色器转换，进一步降低 4K 解码开销）、QuickTime 时码轨、关键帧表（`sync_frames`）
- [ ] 音频（接口已预留：`audio`）
- [ ] 编码导出（接口已预留：`export`）
- [ ] 代理与 FFmpeg 插件（`FrameRequest::downscale` 与格式注册表已预留）
- [ ] 裁片（接口已预留：`trim`，给 21）

### 说明与待定

- `bae-media` 依赖 `bae-model`（只用 `Time` / `FrameRate`），按 02 的要求避免两套时间体系。
- 媒体帧号一律从素材第一帧起算、从 0 开始；序列 `name_0101.png` 的第 0 帧是 0101。
- 颜色只报告不转换：`GpuFrame` 的值保持素材自身的编码（不线性化），由 06 根据 `ColorInfo` 处理。未标记的素材报告为 `Unspecified`，默认规则由 06 决定。
- 待用户决定：序列缺帧时是报错（现在的做法，合成里显示占位）还是沿用前一帧。

## 请求（其他工作流在这里留言）
