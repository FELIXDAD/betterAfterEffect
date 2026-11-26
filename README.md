# betterAfterEffects（BAE）

一个以 GPU 运算为基础、轻量、跨平台、支持多人协同和 Agent 操作的动画制作应用。目标是让分镜、动效、剪辑在同一个平台、同一个工程里工作，并且能和 After Effects / Illustrator 工程互相转换。

当前开发版本为 0.1.0。[0.1.0 发布说明](docs/release/0.1.0.md) · [更新记录](CHANGELOG.md)

## 下载后首次打开

请从项目的 [GitHub Releases](https://github.com/FELIXDAD/betterAfterEffects/releases) 选择与你的系统对应的文件。

- **Windows x64**。将 ZIP 完整解压到一个文件夹，双击 `bae.exe`。
- **Apple Silicon Mac**。打开 0.1.0 的 DMG，将 `BAE.app` 拖到“应用程序”，推出磁盘映像后打开应用。0.0.1 提供 ZIP，解压后同样安装。

系统要求和首次打开提示见各版本说明。[0.1.0](docs/release/0.1.0.md) · [0.0.1](docs/releases/0.0.1.md)

## 文档

- [产品愿景与需求](docs/VISION.md)
- [技术架构](docs/ARCHITECTURE.md)
- [工作流拆分与分支](docs/WORKSTREAMS.md)（每个分支的任务书在 [docs/workstreams/](docs/workstreams/)）
- [给开发 Agent 的规则](AGENTS.md)

## 构建与运行

### 准备

- 用 [rustup](https://rustup.rs) 安装 Rust。工具链版本固定在 `rust-toolchain.toml`（Rust 1.100，带 rustfmt、clippy 和 aarch64-apple-darwin / x86_64-pc-windows-msvc 两个目标），第一次运行 `cargo` 时会自动安装。
- macOS 12 及以上：安装 Xcode 命令行工具（`xcode-select --install`）。
- Windows 10 21H2 及以上 / Windows 11（x64）：安装 Visual Studio Build Tools 的「使用 C++ 的桌面开发」（MSVC 和 Windows SDK）。

### 常用命令

```sh
cargo run -p bae                         # 打开界面（工作区默认成员就是 bae，所以 cargo run 也可以）
cargo run -p bae -- --headless           # 不开窗口运行（批处理 / CI）
cargo run -p bae -- gpu-info             # 显卡、后端、特性和限制
cargo run -p bae -- gpu-info --json      # 同上，输出 JSON
cargo run -p bae -- --smoke-test 60      # 渲染 60 帧后退出（冒烟测试）
cargo run -p bae -- --version

cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all

cargo xtask size                         # release 构建 + 体积报告，并对照 tools/size-budget/budget.toml 检查
cargo xtask dist                         # 打包：macOS 生成 ad-hoc 签名的 BAE.app 和 DMG，Windows 生成 zip
cargo xtask bench                        # 性能基准入口（criterion）
```

- `cargo xtask size`：M0 阶段应用本体（strip 后的 release 可执行文件）的预算是 15 MB（按 MiB 计，见 [技术架构](docs/ARCHITECTURE.md) 第 11 节和 `tools/size-budget/budget.toml`），超出就失败。装了 cargo-bloat（`cargo install cargo-bloat`）时会附带按 crate 的体积分解。新增依赖的 PR 要写明这个命令给出的体积变化。
- `cargo xtask dist` 的产物在 `<target 目录>/dist/`。macOS 包组装好后用 ad-hoc 签名（`codesign --sign -`，必须在 macOS 上打包），并用 `codesign --verify --deep --strict` 校验；它没有经过公证，第一次打开要右键选“打开”，或者到“系统设置 ▸ 隐私与安全性”里点“仍要打开”。Windows 的 exe 没有签名。安装器、Developer ID 签名和公证以后再做。
- 运行 `cargo xtask help` 查看全部参数。

### 环境变量

| 变量 | 作用 |
|---|---|
| `BAE_LOG` | 日志过滤，例如 `info,bae_gpu=debug`。命令行 `--log` 优先于它；都没设置时再看 `RUST_LOG` |
| `BAE_HOME` | 把配置、数据、缓存、日志、插件目录都放到这一个目录下（便携模式、测试） |
| `BAE_GPU_BACKEND` | 指定图形后端：`metal`、`dx12` 或 `vulkan`（`vulkan` 需要用 `--features bae-gpu/vulkan` 构建） |
| `BAE_GPU_ADAPTER` | 只用名字里包含这段文字的显卡（不区分大小写）；`software` 表示软件光栅器（WARP 等） |
| `BAE_GPU_POWER` | 显卡偏好：`high`（独显，默认）或 `low`（集显） |
| `BAE_GPU_VALIDATION` | `1` 打开、`0` 关闭图形 API 校验层（Metal 校验层 / D3D12 调试层）；debug 构建默认打开 |

### 日志和崩溃报告

每次运行在用户日志目录里写一个 `bae-<UTC 时间>-<进程号>.log`，崩溃报告在它下面的崩溃目录：

- macOS：`~/Library/Logs/BAE/`，崩溃报告在 `~/Library/Logs/BAE/Crashes/`
- Windows：`%LOCALAPPDATA%\BAE\Logs\`，崩溃报告在 `%LOCALAPPDATA%\BAE\Logs\Crashes\`
- 设置了 `BAE_HOME` 时：`<BAE_HOME>/logs/` 和 `<BAE_HOME>/logs/crashes/`

完整的目录表（配置、缓存、插件目录等）见 `crates/bae-platform/src/dirs.rs`。

### 构建配置

- debug 构建里所有第三方依赖按 `opt-level = 2` 优化（根 `Cargo.toml` 的 `[profile.dev.package."*"]`），工作区自己的 crate 不优化、带行号信息。第一次构建慢一些，但 wgpu / egui / vello 在 debug 下也够流畅，日常开发用 debug 构建即可。
- release 构建用 fat LTO、`codegen-units = 1` 并去掉符号，构建时间较长，主要给 `cargo xtask size` / `dist` 和性能测试用。

### 相关文档

- [依赖版本规范](docs/spec/dependencies.md)：每个依赖为什么是这个版本、开了哪些 feature、怎么升级，以及「只有一个 wgpu」的约束。
- [应用外壳](docs/spec/app-shell.md)：`apps/bae` 的启动流程、窗口和事件循环、模块注册。

## License

[Apache License 2.0](LICENSE)
