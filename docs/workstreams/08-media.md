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

- [ ] 图像格式
- [ ] macOS 视频解码
- [ ] Windows 视频解码
- [ ] 音频
- [ ] 编码导出
- [ ] 代理与 FFmpeg 插件

## 请求（其他工作流在这里留言）
