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

- [ ] 工作区与工具链
- [ ] bae-platform
- [ ] bae-gpu
- [ ] apps/bae 空窗口
- [ ] CI 与体积检查
- [ ] xtask

## 请求（其他工作流在这里留言）
