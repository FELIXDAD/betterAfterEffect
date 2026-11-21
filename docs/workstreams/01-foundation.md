# 01 · 基础工程与构建

| 项 | 内容 |
|---|---|
| 分支 | `core/foundation` |
| Orca 工作区 | `bae-01-foundation` |
| 阶段 | P0 |
| 依赖 | 无 |
| 负责目录 | 根 `Cargo.toml`、`rust-toolchain.toml`、`apps/bae/`、`crates/bae-platform/`、`crates/bae-gpu/`、`.github/`、`xtask/`、`tools/` |

## 目标

建立一个能构建、能启动、能在 CI 上检查体积的应用骨架，让其他工作流有地方落代码。不做任何业务功能。

## 要做

- **Cargo 工作区**：成员用通配 `apps/*`、`crates/*`、`services/*`；`[workspace.dependencies]` 统一依赖版本；`rust-toolchain.toml` 固定 stable 版本；release 配置按架构文档第 11 节设置。
- **bae-platform**：用户配置/缓存目录、日志（tracing，写文件 + 控制台）、panic 钩子写崩溃日志、单实例、命令行解析（clap）。
- **bae-gpu**：创建 wgpu 实例/适配器/设备，选择独显或集显的策略，能力检测与报告（`bae gpu-info`），窗口表面管理，设备丢失后恢复。
- **apps/bae**：winit 窗口 + bae-gpu 设备 + egui 空界面（一个占位面板）。空闲时不重绘，CPU 占用接近 0。
- **注册机制**：定义 `AppBuilder` 和 `AppModule` trait（`build(&mut AppBuilder)`），命令、面板、工作区、导入导出器都通过它注册，让 `apps/bae/main.rs` 只做组装、几乎不需要改。注册表要支持启动后再注册和卸载，因为 19 的插件宿主会在运行时往同一批注册表里加载第三方插件。内部"应用模块"与面向用户的"插件"命名不要混用。
- **插件目录**：bae-platform 提供用户级、系统级插件目录路径（工程内插件目录由 19 处理）。
- **命令行骨架**：`bae`（打开界面）、`bae --headless`、`bae --version`、`bae gpu-info`，并预留 `bae render`、`bae mcp`、`bae export-json` 的子命令入口给其他工作流填充。
- **CI（GitHub Actions）**：macOS arm64 与 Windows x64 两个平台；fmt、clippy、test、release 构建、体积报告与预算检查。
- **xtask**：`cargo xtask dist`（打包）、`cargo xtask size`（按 crate 统计体积，可以用 cargo-bloat）、`cargo xtask bench`（入口，供 04 填充）。
- **打包**：macOS 生成 .app（先不签名）、Windows 生成 zip；安装器和签名放到后面。

## 对外接口

- `bae_gpu::GpuContext`：device、queue、adapter 信息、能力标志。渲染和界面共用这一个设备。
- `AppBuilder` 注册接口与 `register` 约定。
- `bae_platform` 的路径、日志、设置读写。

## 验收

- 两个平台 CI 都通过。
- 双击能打开窗口，空闲时 CPU 占用接近 0，拖动窗口流畅。
- release 体积报告自动生成，应用本体 ≤ 15 MB（M0 预算）。
- 在 README 写清楚怎么构建和运行。

## 不做

- 业务功能、具体面板、渲染管线（04）、主题（11）。

## 进度

- [x] 工作区与工具链：虚拟工作区（`apps/*`、`crates/*`、`services/*`、`xtask`、`tools/dep-probe`）、统一依赖版本（见 `docs/spec/dependencies.md`）、`rust-toolchain.toml`（1.100）、dev/release/bench 配置。
- [x] bae-platform：目录（含用户级/系统级插件目录）、日志（stderr + 会话文件、轮转）、崩溃报告与紧急保存、单实例、版本化设置、可扩展的命令行子命令注册表。
- [x] bae-gpu：共享 `GpuContext`、适配器选择与软件回退、可选特性、设备丢失回调与重建、窗口表面（呈现模式、帧延迟、HDR/广色域格式发现）、GPU 计时、异步回读与上传、Metal/D3D12 原生句柄、视口纹理注册表、`bae gpu-info`。
- [x] apps/bae 空窗口：winit + egui-winit + egui-wgpu（共用设备）、egui_dock 占位工作区、状态栏、muda 原生菜单、事件驱动重绘、窗口位置记忆、`--headless`、`--smoke-test`。说明见 `docs/spec/app-shell.md`。
- [x] CI 与体积检查：`.github/workflows-pending/ci.yml`（macOS arm64 + Windows x64）。开发 Agent 的 GitHub 令牌没有 `workflow` 权限，推不了 `.github/workflows/`；有权限的人 `git mv` 一下即可启用（见 `.github/workflows-pending/README.md`）。
- [x] xtask：`size`（预算 `tools/size-budget/budget.toml`）、`dist`（macOS .app / Windows zip）、`bench`。

- [x] 0.0.1 前体积清理（PR #89，2026-11-21）：egui 的 epaint 用 `vendor/epaint` 补丁到 vello/parley 的字体栈（skrifa 0.49 / harfrust 0.15），去掉一整套重复字体代码；插件清单和音频标签解析的冷路径 crate 改 `opt-level = "s"`；CI 增加「One font stack in the app」检查。`bae`：macOS 22.60 → 21.65 MiB，Windows 29.09 → 27.98 MiB。B1 和 egui 排版性能不变或更快。
  - `panic = "abort"` 能再减 Windows 7.1 MiB、macOS 3.2 MiB，但崩溃恢复（`bae-platform` 的 `run_emergency_saves` 用 `catch_unwind`）和 MCP 服务的 tokio 任务隔离依赖 unwind。协调人决定 0.0.1 不做，0.0.1 之后先设计替代方案再评估（记在 `docs/spec/dependencies.md` 第 9 节）。egui 内置字体（`default_fonts`，1.41 MiB）是 emoji/符号的兜底字体，换成系统字体要 11 决定。windows-sys 0.52 / objc2 0.5 来自 winit 0.30，要等 winit 0.31 正式版。

- [x] 工程文件（分支 `app/project-files`，2026-11-21，0.0.1 硬阻塞）：文件菜单接到模型——新建项目（Cmd/Ctrl+Alt+N）、打开项目（Cmd/Ctrl+O，`.bae` 目录 / `.baez` 文件，经 `bae_package::open_project`）、关闭项目、保存（Cmd/Ctrl+S）、另存为（Cmd/Ctrl+Shift+S）；原生对话框用 rfd（只在 macOS / Windows 依赖，关掉 Linux 后端），放在 `files::FileDialogs` trait 后面；未保存修改由工程层版本向量推出，标题加 ` *`，关闭 / 新建 / 打开 / 退出前问 保存 / 不保存 / 取消；`bae <工程>` 和 `bae --headless [--smoke-test N] <工程>` 启动时打开工程。说明见 `docs/spec/app-shell.md`「工程文件」。
  - 已知限制：macOS 从程序坞退出、系统注销关机时 winit 0.30 不给拦截的机会，不会提示未保存的修改；第二个实例 `bae <工程>` 仍然只提示已在运行（转给已运行实例是后续工作）；Windows 的打开对话框选不了目录，`.bae` 工程要选里面的 `project.json`。

- [x] 关键帧保存核查（分支 `app/project-files-fix`，2026-11-21，#97 的疑点）：保存 / 序列化没有丢数据——#97 存盘前截图里第 3 帧的关键帧导航器是灰的、右箭头不可用，内存里本来就只有 1 个关键帧，第二次 Alt+Shift+P 没生效。加了往返回归测试（`apps/bae/src/save_roundtrip.rs`：.bae / .baez、另存为、Cmd/Ctrl+S、同值和不同值、相隔 3 帧、export-json、演示工程全量对比、无窗口驱动真实动效界面按键），并修了一个会丢最后一次输入的顺序问题（`files::CommitGate`，见 `docs/spec/app-shell.md`「工程文件」）。

后续（不阻塞合并）：

- 浮动面板的原生多窗口（每个 egui 视口一个 winit 窗口和 surface，共用同一设备）；目前用 `set_embed_viewports(true)` 画在主窗口内。
- 第二个实例把命令行参数转给已运行的实例。
- 设备丢失恢复后通知其他子系统重建 GPU 资源（与 04 一起定事件）。

## 请求（其他工作流在这里留言）

### 06 render/color → 01（2026-11-20）

bae-color 的 `DisplayTarget { monitor, surface, headroom }` 需要平台层的信息（架构文档第 12 节，这些不能放进 bae-color）：

1. **显示器色彩空间。** 窗口所在显示器的原色和白点（以及 ICC 是矩阵/TRC 型时的传递函数）：macOS 从 `NSScreen.colorSpace` / ColorSync 读，Windows 从 `DXGI_OUTPUT_DESC1`（`RedPrimary`…`WhitePoint`、`ColorSpace`）读。建议在 bae-gpu 的 `SurfaceInfo` 或 bae-platform 里加 `DisplayColorInfo { primaries: [[f32; 2]; 4], transfer_hint, sdr_white_nits, max_luminance_nits }`，窗口换屏或系统设置变化时发事件。
2. **EDR 余量会变**（macOS `maximumExtendedDynamicRangeColorComponentValue` 随亮度变化）：希望 `SurfaceInfo` 能刷新 `edr_headroom`，或者给个回调。
3. 交换链选中的格式和色彩空间对应 `bae_color::SurfaceEncoding`：`*Srgb` → `HardwareSrgb`，`Rgba16Float + ExtendedSrgbLinear` → `ExtendedLinear(srgb-linear)`，`ExtendedDisplayP3` → `ExtendedLinear(p3-d65-linear)`，`Bt2100Pq` → `Pq`。映射由 04/11 做也可以，bae-gpu 只要把选中的组合暴露出来（`best_hdr` 已经有一部分）。
