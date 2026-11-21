# 更新记录

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [0.0.1] - 2026-11-2x

第一个可运行版本（技术预览）。完整说明：[docs/releases/0.0.1.md](docs/releases/0.0.1.md)。

### 新增

- AE 式动效工作区：项目面板、合成查看器（GPU 实时）、时间轴（关键帧、缓动、表达式、父级、修剪）、属性面板、预览播放，合成 / 图层 / 动画菜单。
- 工程文件：新建、打开（`.bae` / `.baez`）、保存、另存为，关闭前提示保存。
- 渲染队列：「合成 ▸ 添加到渲染队列」（Cmd/Ctrl+M），后台导出 PNG / TIFF / OpenEXR 序列。
- 命令行：`bae render`（流水线导出 PNG / TIFF / EXR，macOS 还支持 H.264 / HEVC / ProRes）、`bae export-json`、`bae mcp`、`bae plugin`、`bae gpu-info`。
- 数据模型：USD 式工程层叠、引用、Loro CRDT 存储、命令与按作者撤销、Prim schema、效果参数模型。
- 动画：与 AE 一致的关键帧插值、AE 表达式（QuickJS）、预合成、运动模糊采样。
- GPU 合成：20 种 AE 混合模式、矢量（Vello，没有显卡时用 vello_cpu）、文字（parley，中文回退）、色彩管理（OCIO 子集、LUT）。
- 媒体：图片与图片序列、macOS 视频解码和编码（H.264 / HEVC / ProRes）、音频解码与波形。
- 协同：共享目录（NAS）与 WebSocket Hub 同步、命名版本、差异、回退、在线状态；`bae-hub` 服务端与 Docker 部署。
- Agent：MCP 服务器（命令自动生成工具、提案层、权限、审计）、动画操作指南与 Skill。
- 插件：声明式 WGSL 效果、插件宿主与 C ABI 雏形、示例与文档；AE 插件兼容层的设计与骨架。
- 互转与打包：AE 导出脚本与交换格式、Ai / PDF 读取与 Illustrator 导出脚本、`.baez` 打包与素材收集。
- 工具：`tools/ae/aectl`、`tools/ai/aictl`（无人值守驱动 AE / Illustrator）；macOS arm64 与 Windows x64 的 CI。
- 效果库第一阶段（7 个颜色校正、高斯和方框模糊），尚未接入界面。

### 已知限制

- Windows 上影片解码和编码是占位，只支持图片序列。
- 效果库还不能在应用里使用；分镜和剪辑工作区未包含。
- 详见发布说明的"已知限制"一节。
