# 依赖版本规范

负责：01 `core/foundation`。本文说明根 `Cargo.toml` 里 `[workspace.dependencies]` 每一项为什么是这个版本、开了哪些 feature，以及怎么升级。

最后核对：2026-11-20。版本和依赖关系来自 crates.io 稀疏索引（`curl -s https://index.crates.io/<前缀>/<名字>`，每行一个版本的 JSON，含 `vers` 和 `deps[{name, req}]`）、本地 registry 源码（`~/.cargo/registry/src/index.crates.io-*/`）和 `cargo tree --locked`。文中标「核对：」的地方写明了用的命令。

## 1. 规则

- **全工作区只有一套版本。** 所有第三方依赖的版本只写在根 `Cargo.toml` 的 `[workspace.dependencies]`。`Cargo.lock` 提交进仓库，CI 一律带 `--locked`。
- **crate 里只写 `foo.workspace = true`。** 可以加 feature（`foo = { workspace = true, features = ["..."] }`），也可以写 `optional = true`；不写版本号，也不写 `default-features`，需要什么 feature 就在 `features` 里列出来。
- **新增依赖**：先看下面的表里有没有同类库；确实需要时在根 `Cargo.toml` 加一行 pin，同时在本文第 5 节加一行，PR 里写明 `cargo xtask size` 的体积变化（AGENTS.md）。根 `Cargo.toml` 归 01，改动尽量小并在 PR 里说明。
- **pin 表可以先于使用者存在。** `[workspace.lints.cargo]` 里 `unused_workspace_dependencies = "allow"`：后面的工作流直接用这里定好的版本，不要自己另选。
- **工具链** 固定在 `rust-toolchain.toml`（`1.100`），`[workspace.package] rust-version` 与它一致。它必须不低于所有依赖里最高的 MSRV（目前是 egui 0.36.2 系列的 1.95，vello 0.12.0 是 1.89）。升级工具链单独开 PR。

## 2. 「只有一个 wgpu」

渲染器（vello、`bae-render`）、界面（egui-wgpu）和插件共用同一个 `wgpu::Device`，也就是 `bae_gpu::GpuContext`。视口纹理在 GPU 上直接交给界面显示（零拷贝），插件拿到的也是同一个设备。只要依赖图里出现两份 wgpu（或 wgpu-core / wgpu-hal / naga），两边的类型就互不兼容，代码要么编译不过，要么只能绕道 CPU 拷贝，而且二进制里会多出一整套 wgpu（几 MB）。

怎么保证：

1. **编译期：`tools/dep-probe`（包名 `bae-dep-probe`）。** 它不进入发布产物。工作区的 `default-members` 只有 `apps/bae`，所以它只在带 `--workspace` 或 `-p bae-dep-probe` 的 check / clippy / test 里编译（CI 的 clippy 和 test 都带 `--workspace`）。版本一旦不一致就编译失败：
   - 类型相等断言：我们的 `wgpu::{Instance, Device, Queue, TextureView, TextureFormat, CommandEncoder}` 与 `vello::wgpu::…`、`egui_wgpu::wgpu::…` 是同一类型；`peniko::{Color, Brush, Fill, ImageData}`、`kurbo::{Affine, BezPath, Rect}` 与 `vello::peniko` / `vello::kurbo` / `peniko::kurbo` 里的是同一类型；`fontique::Collection` 与 `parley::fontique::Collection`、`parley::FontData` 与 `peniko::FontData` / `vello::peniko::FontData`、`fontique::Blob<u8>` 与 `peniko::Blob<u8>` 是同一类型。
   - 真实调用的类型检查：在同一个 `&wgpu::Device` 上调用 `vello::Renderer::new` 和 `egui_wgpu::Renderer::new`；vello `render_to_texture` 的输出用 `egui_wgpu::Renderer::register_native_texture` 注册成 `egui::TextureId`（视口路径）；把 `egui::TexturesDelta` / `egui::ClippedPrimitive` 交给 egui-wgpu（证明 egui 与 egui-wgpu 用的是同一个 epaint）；把 parley 排好的字形连同 `run.font()` 直接画进 `vello::Scene`。
   - BAE 从不调用这些函数；唯一会运行的是一个单元测试，只调用其中不需要 GPU 的几个。
   - 看不到的：skrifa / read-fonts、harfrust、naga、wgpu-hal 的类型不出现在 vello / parley / egui 的公开接口里，它们的重复只能靠下面的 `cargo tree` 发现。其中字体这一组（skrifa / read-fonts / harfrust）由 CI 的「One font stack in the app」一步检查：`cargo tree -p bae -d` 里出现第二个版本就失败。
2. **CI：`cargo tree`。** `.github/workflows-pending/ci.yml`（启用后移到 `.github/workflows/`，见该目录的 README）的「One wgpu in the dependency graph」一步在 macOS 和 Windows 上各跑一次：

   ```sh
   cargo tree --locked -p bae-dep-probe -i wgpu --edges normal
   ```

   依赖图里有两个 wgpu 版本时，这条命令直接报错（``specification `wgpu` is ambiguous``），CI 失败。升级依赖时还要人工看一遍重复依赖（第 7 节）：

   ```sh
   cargo tree --workspace -i wgpu -e normal --locked
   cargo tree --workspace -d -e normal --locked
   cargo tree --workspace -d -e normal --locked --target x86_64-pc-windows-msvc
   ```

当前结果（核对：`cargo tree -p bae-dep-probe -i wgpu --edges normal --locked`、`cargo tree --workspace -i wgpu -e normal --locked`、`grep -c '^name = "wgpu"$' Cargo.lock`）：

```text
wgpu v30.0.2
├── bae-dep-probe v0.1.0 (tools/dep-probe)
├── bae-gpu v0.1.0 (crates/bae-gpu)
│   └── bae v0.1.0 (apps/bae)
├── egui-wgpu v0.36.2
│   ├── bae v0.1.0 (apps/bae)
│   └── bae-dep-probe v0.1.0 (tools/dep-probe)
└── vello v0.12.0
    └── bae-dep-probe v0.1.0 (tools/dep-probe)
```

`Cargo.lock` 里 `wgpu`、`wgpu-core`、`wgpu-hal`、`naga`、`egui`、`winit` 各只有一份（wgpu / naga 都是 30.0.2，winit 0.30.13）。

## 3. 怎么升级

1. **成组升级，不单独升某一个。** 下面每一组要一起动：
   - wgpu 组：`wgpu`、`wgpu-hal`、`wgpu-types`（三者版本号相同）。
   - Linebender 组：`vello`、`peniko`、`kurbo`、`skrifa`、`parley`、`fontique`。
   - egui 组：`egui`、`egui-wgpu`、`egui-winit`、`egui_kittest`、`egui_dock`，以及 `winit`、`raw-window-handle`。
2. **先查索引确认能对上。** 关键是三条边：vello 要求的 wgpu、egui-wgpu 要求的 wgpu、egui_dock 要求的 egui。例如：

   ```sh
   curl -s https://index.crates.io/ve/ll/vello      | tail -1   # 最新 vello 的 deps 里 wgpu / peniko / skrifa 的 req
   curl -s https://index.crates.io/eg/ui/egui-wgpu  | tail -1   # 最新 egui-wgpu 的 wgpu / winit req
   curl -s https://index.crates.io/eg/ui/egui_dock  | tail -1   # 最新 egui_dock 的 egui req
   ```

   三者对不上就整组停在能对上的最高版本（现在就是这种情况：egui 0.37.0 已发布，但 egui_dock 0.21.1 只支持 egui ^0.36，所以 egui 组停在 0.36.2）。
3. 改根 `Cargo.toml`，`cargo update -p <crate>`，然后：
   `cargo check -p bae-dep-probe`、`cargo tree --workspace -i wgpu -e normal --locked`、`cargo tree --workspace -d -e normal --locked`（两个目标平台都看）。
4. `cargo xtask size` 在升级前后各跑一次，PR 里写 release 体积的变化。
5. 更新本文第 5、6、7 节。

## 4. default-features 策略

为了让二进制小、编译快，大多数 pin 关掉了默认 feature，只开需要的。成员 crate 需要更多 feature 时在自己的 `Cargo.toml` 里加，不要改根上的默认值。

| crate | 关掉的默认 feature | 我们开的 | 原因 |
|---|---|---|---|
| wgpu | 默认里的 `gles`、`vulkan`、`webgpu` | `std`、`parking_lot`、`wgsl`、`metal`、`dx12` | 桌面首版只要 Metal（macOS）和 D3D12（Windows）。Vulkan 通过 `bae-gpu` 的 `vulkan` feature 打开（`wgpu/vulkan` + `wgpu-hal/vulkan`），留给 Linux / Android 或 Windows 备用；GLES、WebGPU 不编译 |
| wgpu-hal | （无默认） | 由 `bae-gpu` 按平台开 `metal` / `dx12` | 原生 Metal / D3D12 句柄互操作（ARCHITECTURE §14、§15） |
| wgpu-types | 无实际差别（默认只有 `std`） | `std` | 只要类型、不要设备的 crate 用 |
| vello | `wgpu_default`（= `wgpu/default`，会打开 wgpu 的全部后端） | `wgpu` | 防止 vello 把 Vulkan / GLES / WebGPU 带回来 |
| parley、fontique | 无实际差别（默认就是 `system`） | `std`、`system` | 写明，避免上游改默认值时悄悄变化 |
| egui-wgpu | `wgpu/default`、`wgpu/webgl`、`fragile-send-sync-non-atomic-wasm`、`macos-window-resize-jitter-fix` | 无；不开 `winit` | 默认值会打开 wgpu 全部后端。BAE 自己管理表面（`bae-gpu`），不用 `egui_wgpu::winit::Painter`；注意 `macos-window-resize-jitter-fix` 只作用于这个 Painter（macOS 缩放窗口时 `presentsWithTransaction` + 3 个 drawable），`bae-gpu` 需要自己实现同样的处理 |
| egui-winit | `wayland`、`x11`、`winit/default` | `clipboard`、`links`；`apps/bae` 另开 `accesskit` | 只做 macOS / Windows。`accesskit`（读屏、UI 自动化）在 `apps/bae` 打开（#116，2026-11-21）：VoiceOver / 讲述人和 Windows 上的自动化验收读控件标签 |
| winit | `x11`、`wayland`、`wayland-dlopen`、`wayland-csd-adwaita` | `rwh_06` | 同上，只保留 raw-window-handle 0.6 接口 |
| egui | `default_fonts` | 无 | 不内嵌 egui 的 1.4 MB 字体（Ubuntu-Light、Hack、两个 emoji 字体）。界面全部用系统字体：系统界面字体、简体中文字体、符号字体，由 bae-ui `theme::system_ui_fonts` 经 fontique 查找（见 bae-ui 的 `ui_text` 覆盖测试）。测试也不打开它（bae-ui 示例用的 eframe 同样不带 `default_fonts`），所以单独测一个 crate 和整个工作区一起测看到的字体相同，都和发布产物一样；测试里的 egui 上下文用 `bae_ui::testing::ctx_with_fonts()` 装好系统字体 |
| egui_kittest | （无默认） | 无 | 界面测试只用无窗口模式；需要截图对比的 crate 自己开 `wgpu` / `snapshot` |
| muda | `gtk3` | 无 | Linux 菜单后端，不需要 |
| rfd | `xdg-portal`、`wayland` | 无 | Linux 对话框后端，不需要；应用只在 macOS / Windows 上依赖它 |
| tracing-subscriber | 无实际差别 | `std`、`fmt`、`ansi`、`registry`、`tracing-log`、`smallvec` | 明确**不开** `env-filter`（会带入 `regex-automata`）；日志过滤用 `tracing_subscriber::filter::Targets`（见 `bae-platform::logging`） |
| clap | 无实际差别 | 默认那几个 + `string` | 不开 `derive`（不用过程宏，命令行用 builder 写，子命令在运行时注册）；`string` 用于运行时生成的子命令名和说明 |
| image | `rayon`、`default-formats` | 无 | 08 只开需要的格式（PNG、JPEG、WebP…） |
| exr | `rayon` | 无 | 并行由我们自己的线程池决定 |
| symphonia | 全部解码器（flac、vorbis、mkv、ogg、wav、pcm、adpcm…） | 无 | 08 按需开 |
| objc2-* | 默认会打开整个框架的所有类（编译很慢） | `std` | 用到哪个类在成员 crate 里开对应 feature |
| windows、windows-sys | `windows` 默认只有 `std` | 无 | 用到哪个 API 在成员 crate 里开（如 `bae-platform` 开 `Win32_System_Console`） |
| cbindgen | `clap`（命令行） | 无 | 只作为库在 build 脚本里生成 `bae_plugin.h` |
| zip | `aes-crypto`、`bzip2`、`deflate64`、`lzma`、`ppmd`、`time`、`zstd`、`xz` | `deflate-flate2-zlib-rs` | 只要 deflate（纯 Rust 的 zlib-rs 后端） |
| futures | `async-await`、`executor` | `std` | 执行器用 tokio 或 pollster |
| tokio | （默认本来就空） | 无 | 10 自己开 `rt`、`net` 等；17 只在测试里用 |
| rmcp | `base64`、`macros`、`server` | 无 | 只做 17 的 dev-dependency（`client` + `transport-async-rw`），不链接进应用 |
| criterion | `rayon`、`plotters` | `cargo_bench_support` | 不生成图表 |

## 5. 全部 pin

「使用方」一栏：crate 名是现在已经在用的（2026-11-20），数字是计划使用的工作流。「最新」指核对时 crates.io 上最新的非预发布版本。

### GPU、渲染、文字

| crate | 版本 | features | 使用方 | 版本原因 |
|---|---|---|---|---|
| wgpu | 30.0.2 | 见第 4 节 | bae-gpu、dep-probe；04、05、06、07、19 | 最新。vello 0.12.0 要求 `^30.0.0`，egui-wgpu 0.36.2 要求 `^30.0` |
| wgpu-hal | 30.0.2 | 无默认 | bae-gpu（按平台 `metal` / `dx12`）；19、20 | 必须与 wgpu 同版本 |
| wgpu-types | 30.0.2 | `std` | bae-gpu（加 `serde`，用于 `gpu-info --json`）；19、04（只要类型的地方） | 必须与 wgpu 同版本 |
| vello | 0.12.0 | `wgpu` | dep-probe；04、05 | 最新。要求 wgpu `^30.0.0`、peniko `^0.6.1`、skrifa `^0.49.0` |
| peniko | 0.6.1 | 默认 | dep-probe；04、05、16 | 最新；= vello 0.12.0 的要求。带入 kurbo `^0.13.1`、color `^0.3.3` |
| vello_cpu | 0.1.0 | 关默认，只开 `std`、`u8_pipeline` | 05（bae-vector / bae-text 的 `render`：软件适配器上的矢量和文字栅格化） | = egui 0.36.2 的 epaint 已链接的版本（不新增 crate；`vendor/epaint` 没改这一项）。WARP 等软件适配器上 Vello 的计算管线会让进程崩溃，05 在那里改用它 |
| kurbo | 0.13.1 | 默认 | dep-probe；03、05、13、16 | 最新；= peniko 0.6.1 的要求 |
| parley | 0.13.0 | `std`、`system` | dep-probe；05 | 最新。经 parley_engine 要求 skrifa `^0.49.0`、harfrust `^0.15.0` |
| fontique | 0.13.0 | `std`、`system` | dep-probe；05 | = parley 0.13.0 的要求。要求 read-fonts `^0.46.0` |
| skrifa | 0.49.0 | 默认 | dep-probe；05、16 | 0.50.0 已发布，但 vello 0.12.0 和 parley_engine 0.13.0 都要求 `^0.49.0`（0.50 用 read-fonts 0.47，会多出一份）。egui 的 epaint 通过 `vendor/epaint` 补丁也用这一份（见第 7 节） |

### 界面与窗口

| crate | 版本 | features | 使用方 | 版本原因 |
|---|---|---|---|---|
| egui | 0.36.2 | 关默认（不要 `default_fonts`） | bae、dep-probe；11、12、13、14、19 | 0.37.0 已发布，egui_dock 0.22.0（2026-11-20）也已支持 `^0.37`，但 epaint 0.37.0 仍用 skrifa 0.44 / harfrust 0.12，升级不能去掉字体重复。epaint 用根 `Cargo.toml` 的 `[patch.crates-io]` 换成 `vendor/epaint`（0.36.2 + 上游 egui#8874 的字体依赖升级，见 `vendor/epaint/BAE-PATCH.md`）；egui 组升到包含 #8874 的版本时删掉补丁 |
| egui-wgpu | 0.36.2 | 无默认 | bae、dep-probe | 跟 egui。要求 wgpu `^30.0`、winit `^0.30.13`（可选） |
| egui-winit | 0.36.2 | `clipboard`、`links` | bae | 跟 egui。要求 winit `^0.30.13`、raw-window-handle `^0.6.2` |
| egui_dock | 0.21.1 | 默认 | bae；12、13、14 | 最新，要求 egui `^0.36`。`bae-ui`（11）用自己的停靠实现，不用它 |
| egui_kittest | 0.36.2 | 无默认 | 11、12、13、14（测试） | 跟 egui |
| winit | 0.30.13 | `rwh_06` | bae | 0.30 系列最新；egui-winit / egui-wgpu 0.36.2 要求 `^0.30.13`。0.31 仍是 beta（0.31.0-beta.4） |
| raw-window-handle | 0.6.2 | 默认 | 01（bae-gpu 表面）、19 | 最新；wgpu 30 和 egui-winit 0.36 都要求 `^0.6.2` |
| muda | 0.21.2 | 无默认 | bae；11 | 最新。要求 windows-sys `>=0.60, <=0.61`、objc2 `^0.6.1` |
| rfd | 0.18.0 | 无默认（不编 Linux 的 GTK / xdg-portal 后端） | bae（只在 macOS 和 Windows 上依赖；文件 ▸ 打开 / 另存为、未保存修改提示） | 最新；用的 objc2 0.6 / objc2-app-kit 0.3 / windows-sys 0.61 都是树里已有的版本，只新增 objc2-uniform-type-identifiers 0.3.2（macOS）。不开 `common-controls-v6`：TaskDialog 需要可执行文件带 comctl32 v6 清单，否则启动时找不到入口 |

### 文档模型、协同、脚本

| crate | 版本 | features | 使用方 | 版本原因 |
|---|---|---|---|---|
| loro | 1.18.0 | 默认（`counter`） | 02、09 | 最新 |
| rquickjs | 0.14.0 | 默认（`std`） | 03、19 | 最新 |

### 序列化、错误、日志、命令行

| crate | 版本 | features | 使用方 | 版本原因 |
|---|---|---|---|---|
| serde | 1.0.229 | `derive` | bae、bae-gpu、bae-platform、xtask；全部 | 最新 |
| serde_json | 1.0.154 | 默认 | bae、bae-gpu、bae-platform、xtask；全部 | 最新 |
| schemars | 1.2.3 | 默认 | 02、17（JSON Schema） | 最新 |
| toml | 1.2.1 | 默认 | xtask；19（`bae-plugin.toml`） | 最新（索引里的版本号是 `1.2.1+spec-1.1.0`） |
| thiserror | 2.0.21 | 默认 | bae-app、bae-gpu、bae-platform；库 crate | 最新 |
| anyhow | 1.0.104 | 默认 | bae、bae-platform、xtask；应用和工具 | 最新 |
| tracing | 0.1.44 | 默认 | bae、bae-gpu、bae-platform；全部 | 最新 |
| tracing-subscriber | 0.3.23 | 见第 4 节 | bae-platform | 最新 |
| clap | 4.6.8 | 见第 4 节 | bae、bae-platform | 4.6 系列；补丁版本由 `Cargo.lock` 决定（核对时最新 4.6.9） |

### 数学、内存、集合、并发

| crate | 版本 | features | 使用方 | 版本原因 |
|---|---|---|---|---|
| glam | 0.34.3 | 默认 | 03、04 | 最新 |
| bytemuck | 1.25.2 | `derive` | 04、05、07（GPU 缓冲） | 最新；vello / egui-wgpu 也用它 |
| half | 2.7.1 | `bytemuck` | 04、06、08（f16 像素） | 最新 |
| smallvec | 1.16.6 | `union`、`const_generics` | 02、03、04 | 最新 1.x |
| rustc-hash | 2.1.3 | 默认 | 02、04 | 最新（naga / wgpu-core 自己还用 1.1，见第 7 节） |
| indexmap | 2.14.2 | 默认 | bae-app；02 | 最新 |
| parking_lot | 0.12.5 | 默认 | bae-app、bae-gpu | 0.13.0 已发布，但 wgpu-core / wgpu-hal 30 要求 `^0.12.3`，用 0.12 避免两份 |
| arc-swap | 1.9.5 | 默认 | bae-app | 最新 |
| crossbeam-channel | 0.5.17 | 默认 | 04、08、09 | 最新 |
| rayon | 1.12.0 | 默认 | 04、08 | 最新 |
| uuid | 1.29.3 | `v4`、`v7`、`serde` | 02、09 | 最新；v7 是按时间排序的 ID |

### 媒体与音频

| crate | 版本 | features | 使用方 | 版本原因 |
|---|---|---|---|---|
| image | 0.25.10 | 无默认 | 08、17 | 最新 |
| exr | 1.74.3 | 无默认 | 08 | 最新 |
| png | 0.18.1 | 默认 | 08、17（`render.frame` 返回 PNG） | 最新；= vello 0.12.0 用的版本 |
| zune-jpeg | 0.5.15 | 默认 | 08 | 最新 |
| symphonia | 0.6.1 | 无默认 | 08 | 最新 |
| cpal | 0.18.2 | 默认 | 08 | 最新 |

### 平台绑定

| crate | 版本 | features | 使用方 | 版本原因 |
|---|---|---|---|---|
| objc2 | 0.6.5 | 默认 | 01、08 | 最新；wgpu-hal、fontique、muda 用 0.6 |
| objc2-foundation、objc2-core-foundation、objc2-app-kit、objc2-metal、objc2-quartz-core、objc2-av-foundation、objc2-core-video、objc2-core-media、objc2-video-toolbox | 0.3.2 | `std` | 01（bae-platform、bae-gpu）、08（AVFoundation / VideoToolbox） | 最新；与 objc2 0.6 配套 |
| windows | 0.62.2 | 无默认 | bae-gpu（Windows，D3D12 句柄）；08（Media Foundation）；bae（Windows 另存为对话框，`Win32_UI_Shell_Common`） | 最新；wgpu-hal 要求 `^0.62`，fontique 要求 `^0.62.2` |
| windows-core | 0.62.2 | 无默认（bae 开 `std`，树里已经开着） | bae（Windows：`#[implement]` 实现 IFileDialogEvents，宏展开里写的是 `windows_core`） | windows 0.62.2 本来就依赖它，不新增 crate |
| windows-sys | 0.61.2 | 无默认 | bae-platform（Windows 控制台） | 最新；muda 要求 `>=0.60, <=0.61` |
| directories | 6.0.0 | 默认 | bae-platform | 最新 |
| libloading | 0.9.1 | 默认 | 19、20 | 最新 |
| libc | 0.2.191 | 默认 | 10（bae-hub 在 Unix 上处理 SIGTERM / SIGINT，`docker stop` 和 systemd 用） | `Cargo.lock` 里已有的版本（parking_lot、getrandom 等都依赖它），直接依赖不增加体积 |
| notify | 8.2.2 | 关默认，只开 `macos_fsevent` | 09（`bae-sync` 的可选 feature `watch`，共享目录模式的文件监听） | 8.x 最新稳定版（9.0 还是 rc）；macOS 走 FSEvents（objc2-core-services，和已 pin 的 objc2 0.3 同一套），Windows 走 ReadDirectoryChangesW |
| tungstenite | 0.30.0 | 关默认，只开 `handshake`（不带 TLS） | 09（`bae-sync` 的可选 feature `hub`，连协作服务的 WebSocket 客户端）；10（bae-hub 经 `bae-sync` 的 `HubTransport::accept` 用它做服务端） | 最新；同步 API，放在 `ThreadedTransport` 的后台线程里跑；axum 的 WebSocket 底层也是它。局域网 Hub 暂不需要 TLS |

### 插件、打包、哈希

| crate | 版本 | features | 使用方 | 版本原因 |
|---|---|---|---|---|
| cbindgen | 0.29.4 | 无默认 | 19（生成 `bae_plugin.h`） | 最新 |
| zip | 9.8.0 | `deflate-flate2-zlib-rs` | xtask（`dist`）；19（`.baeplugin`）、21（`.baez`） | 9.8 系列；补丁版本由 `Cargo.lock` 决定（核对时最新 9.8.1） |
| blake3 | 1.8.7 | 默认 | 04（缓存键）、09、21（素材哈希） | 最新 |

### 异步与服务

| crate | 版本 | features | 使用方 | 版本原因 |
|---|---|---|---|---|
| pollster | 0.4.0 | 默认 | bae、bae-gpu（等待适配器 / 设备请求） | **待定**：1.0.3 已发布，eframe 0.36.2 用 `^1.0`。依赖图里只有我们自己用 0.4，可以直接升到 1.0（见第 9 节） |
| futures | 0.3.34 | `std` | 09、10、17 | 最新 |
| tokio | 1.54.1 | 无默认 | 10、17（测试） | 最新 |
| rmcp | 3.11.0 | 无默认 | 17（测试：用官方 SDK 的客户端检查互通） | 最新 |

### 测试与基准

| crate | 版本 | features | 使用方 | 版本原因 |
|---|---|---|---|---|
| criterion | 0.8.2 | `cargo_bench_support` | 04（`cargo xtask bench`） | 最新 |
| insta | 1.49.0 | `json` | 02、15（快照测试） | 最新 |
| tempfile | 3.27.0 | 默认 | bae-platform（测试） | 最新 |

ARCHITECTURE 里提到但还没有 pin 的：`psd`（08）。bae-hub 不用 axum / tokio（协调人决定：直接用 `bae-sync` 的 `HubTransport`，和客户端同一个 WebSocket 库）。用到时按第 1 节加。

## 6. 兼容矩阵

核对：sparse index 中各版本的 `deps[].req`。

| crate | pin | 它对矩阵里其他 crate 的要求 | 谁对它有要求 |
|---|---|---|---|
| wgpu | 30.0.2 | wgpu-core / wgpu-hal / wgpu-types `^30.0.0`；raw-window-handle `^0.6.2` | vello 0.12.0 `^30.0.0`；egui-wgpu 0.36.2 `^30.0`（0.37.0 也是 `^30.0`） |
| naga | 30.0.2（不单独 pin） | naga-types `^30.0.0` | wgpu-core 30.0.2、wgpu-hal 30.0.2 `^30.0.0` |
| vello | 0.12.0 | wgpu `^30.0.0`、peniko `^0.6.1`、skrifa `^0.49.0`、vello_encoding / vello_shaders `^0.12.0` | — |
| peniko | 0.6.1 | kurbo `^0.13.1`、color `^0.3.3`、linebender_resource_handle `^0.1.1` | vello、vello_encoding `^0.6.1`；vello_common 0.1.0（epaint 0.36.2 → vello_cpu 0.1.0）`^0.6.1` |
| kurbo | 0.13.1 | — | peniko 0.6.1 `^0.13.1` |
| skrifa | 0.49.0 | read-fonts `^0.46.0` | vello、vello_encoding、parley_engine 0.13.0 `^0.49.0`；`vendor/epaint` `^0.49.0`（crates.io 上的 epaint 0.36.2 / 0.37.0 要 `^0.44.0`，见第 7 节） |
| parley | 0.13.0 | fontique `^0.13.0`、parley_engine `^0.13.0`（→ skrifa `^0.49.0`、harfrust `^0.15.0`）、linebender_resource_handle `^0.1.1` | — |
| fontique | 0.13.0 | read-fonts `^0.46.0`、linebender_resource_handle `^0.1.1`、objc2 `^0.6.5`（Apple）、windows `^0.62.2`（Windows） | parley、parley_engine `^0.13.0` |
| egui | 0.36.2 | epaint / emath `^0.36.2`（epaint 用 `vendor/epaint`：skrifa `^0.49.0`、harfrust `^0.15.0`、vello_cpu `^0.1.0`；原版是 skrifa `^0.44.0`、harfrust `^0.12.0`） | egui-winit `^0.36.2`、egui_kittest `^0.36.2`、eframe 0.36.2 `^0.36.2`、egui_dock 0.21.1 `^0.36` |
| egui-wgpu | 0.36.2 | wgpu `^30.0`、epaint `^0.36.2`、winit `^0.30.13`（可选） | eframe 0.36.2 `^0.36.2` |
| egui-winit | 0.36.2 | egui `^0.36.2`、winit `^0.30.13`、raw-window-handle `^0.6.2` | eframe 0.36.2 `^0.36.2` |
| egui_dock | 0.21.1 | egui `^0.36` | —（它是 egui 停在 0.36 的原因） |
| winit | 0.30.13 | raw-window-handle `^0.6`（`rwh_06`）、objc2 `^0.5.2` / objc2-app-kit `^0.2.2`（macOS）、windows-sys `^0.52.0`（Windows） | egui-winit、egui-wgpu（`winit` feature）、eframe 0.36.2 `^0.30.13` |
| raw-window-handle | 0.6.2 | — | wgpu 30 `^0.6.2`、egui-winit 0.36 `^0.6.2`、winit 0.30（`rwh_06`） |

目前卡住升级的边：

- egui 0.37.0 ← egui_dock 0.21.1 只支持 `^0.36`（egui_dock 0.22.0 已解开这条边；但 0.37 的 epaint 仍是旧字体栈，升级要等包含 egui#8874 的版本才能去掉 `vendor/epaint`）。
- skrifa 0.50.0 ← vello 0.12.0、parley 0.13.0 要求 `^0.49.0`。
- winit 0.31 ← 仍是 beta，egui-winit 0.36 / 0.37 都要求 `^0.30.13`。
- parking_lot 0.13 ← wgpu-core / wgpu-hal 30 要求 `^0.12.3`。

## 7. 当前的重复依赖

核对：`cargo tree -p <pkg> -d --edges normal --locked --depth 0`，以及加 `--target x86_64-pc-windows-msvc`（2026-11-20，体积清理 PR 之后）。`syn`、`heck`、`thiserror-impl`、`enum-as-inner` 的多份只出现在过程宏（编译期），不进二进制，下面不再列。

**字体栈已经只有一份。** egui 0.36.2 原版的 epaint 用 skrifa 0.44 + harfrust 0.12（+ read-fonts 0.41），vello 0.12 / parley 0.13 用 skrifa 0.49 + harfrust 0.15（+ read-fonts 0.46），两套同时进 `bae` 时约多 0.9 MiB。现在 epaint 由 `vendor/epaint` 补丁成后一套（为什么不升 egui、不降 vello/parley，见 `vendor/epaint/BAE-PATCH.md`），CI 的「One font stack in the app」一步防止回退。

`bae` 应用：

| 重复 | 来源 | 影响与处理 |
|---|---|---|
| objc2 0.5.3 / 0.6.5、objc2-foundation 0.2 / 0.3、objc2-app-kit 0.2 / 0.3、block2 0.5 / 0.6（macOS） | winit 0.30 用 objc2 0.5；wgpu-hal、muda、fontique 用 0.6 | 等 winit 0.31 正式版和对应的 egui-winit |
| bitflags 1.3.2 / 2.x（macOS） | winit 0.30 → core-graphics 0.23 | 同上 |
| windows-sys 0.52.0 / 0.61.2（Windows） | winit 0.30.13 用 0.52；muda、egui-winit 和我们用 0.61 | 同上。只是 FFI 声明，LTO 后不占体积 |
| thiserror 1 / 2、itertools 0.11 / 0.12 / 0.15、heapless 0.7 / 0.8 / 0.9（+ hash32 0.2 / 0.3）、getrandom 0.2 / 0.3 / 0.4、rand_core 0.6 / 0.9 | loro 1.18（loro-internal、generic-btree、serde_columnar、postcard、imbl）；egui 用 itertools 0.15；ahash 用 getrandom 0.3，uuid / zip 用 0.4 | loro、uuid、zip 都已是最新版。多为泛型代码，实际重复很少 |
| hashbrown 0.16.1 / 0.17.1 | loro-kv-store → quick_cache 0.6；Windows 上还有 wgpu-hal 的 gpu-allocator 0.28 | 很小；随 loro / wgpu 升级 |
| miniz_oxide 0.8.9 / 0.9.1 | png 0.18.1、exr 1.74 直接用 0.8；png 打开了 flate2 的默认后端（0.9） | flate2 实际走 zlib-rs，0.9 的代码被 LTO 去掉；等 png / exr 升级 |
| rustc-hash 1.1.0 / 2.1.3 | naga、naga-types、wgpu-core 用 1.1 | 很小；随 wgpu 升级 |
| constant_time_eq 0.4 / 0.6 | blake3 1.8.7 用 0.4，zip 9.8 用 0.6 | 很小 |

`bae-dep-probe`：只剩 miniz_oxide、rustc-hash 和（Windows）hashbrown，来源同上。

## 8. 不放进工作区的东西

按 AGENTS.md 和 ARCHITECTURE §11，下列运行时一律不加，也不能通过某个依赖的默认 feature 间接带进来：

- Chromium / WebView（包括 CEF、wry / Tauri 一类封装）
- Qt
- 静态链接的 FFmpeg（FFmpeg 只作为可选的、按需下载的插件）
- OpenColorIO（`bae-color` 自己实现常用子集）
- OpenUSD
- 内置的中文字体（用系统字体）

`eframe` 不进 `[workspace.dependencies]`：应用由 `apps/bae` 自己用 egui-winit + egui-wgpu 驱动，跑在 `bae_gpu` 的共享设备上。

## 9. 待处理

- **pollster**：建议升到 1.0.3（MSRV 1.69，接口仍是 `block_on`），与 eframe 0.36.2 一致，也避免 `bae-ui` 示例里出现两份。由 01 决定。
- **macOS 缩放抖动**：egui-wgpu 的 `macos-window-resize-jitter-fix` 只在 `egui_wgpu::winit::Painter` 里生效，BAE 不用这个 Painter。`bae-gpu` 的表面管理需要自己在拖动缩放期间打开 `CAMetalLayer.presentsWithTransaction` 并把帧延迟提到 2（参考 egui-wgpu 0.36.2 `src/winit.rs`）。
- **读屏（accesskit）**：已开启（#116）。窗口先隐藏创建，accesskit 适配器建好后再显示（适配器必须在窗口第一次显示前创建）；设备丢失重建时把适配器移到新的 egui 状态。体积见 PR。
- **浮动面板的原生多窗口**：见第 10 节。
- **以后可选：`panic = "abort"`**（0.0.1 之后再评估，协调人 2026-11-21 决定 0.0.1 不做）。可省 Windows 7.1 MiB、macOS 3.2 MiB（PR #89 实测：Windows 27.98 → 20.86 MiB，macOS 21.65 → 18.44 MiB；Windows 省得多，是因为 MSVC 的 unwind 要给每个析构清理生成单独的 funclet）。前提是下面两处依赖 unwind 的地方有了替代方案：`bae-platform` 崩溃处理里用 `catch_unwind` 兜住保存回调（`run_emergency_saves`），以及 Agent / MCP（tokio 任务）、插件的 panic 隔离，防止一个工具调用拖垮整个应用。例如在 panic hook 里自动保存，并给插件和 Agent 设计新的隔离策略。不能只为了体积改。

## 10. ui/shell（PR #1）迁移说明

核对：只读查看 `ui/shell` 分支的工作区 `bae-11-ui-shell`（提交 `b2c1c1d`）里的 `Cargo.toml`、`crates/bae-ui/Cargo.toml`、`crates/bae-ui/src`、`Cargo.lock` 和 PR #1 的说明。

**现状**

- 在 foundation 之前，该分支临时加了根 `Cargo.toml`（`members = ["crates/*"]`，自己的 `[workspace.dependencies]`：`egui = "0.36"`、`eframe = { version = "0.36", default-features = false, features = ["wgpu", "accesskit", "default_fonts"] }`、`serde`、`serde_json`）、`rust-toolchain.toml`（`channel = "stable"`）和 `Cargo.lock`。
- `crates/bae-ui` 的依赖是 `egui`、`serde`、`serde_json`（都写成 `.workspace = true`）；`eframe` 只是 `examples/shell.rs` 的 dev-dependency。`bae-ui/src` 里不引用 eframe。它的 `Cargo.lock` 解析出 egui / egui-wgpu / egui-winit / eframe 0.36.2、wgpu 30.0.2、winit 0.30.13，和 foundation 一致。
- 停靠是自己实现的（`bae-ui::dock`，与 AE 工作区文件同构的 Splitter / Frame 树），不用 egui_dock。
- 标题栏（`bae-ui::titlebar`）自己画；关闭、最小化、缩放、全屏、拖动窗口都通过 `ctx.send_viewport_cmd(ViewportCommand::…)` 发出；macOS 上用 `titlebar::viewport_builder(title)` 隐藏系统标题栏（`with_fullsize_content_view` 等）。
- 浮动面板用 egui 多视口：`ctx.show_viewport_immediate(…)`，并已处理 `ViewportClass::EmbeddedWindow`（后端不支持多窗口时由 egui 画成窗口内的浮动窗）。
- PR #1 里的体积数据：`examples/shell` release 构建 8.6 MiB（绝大部分是 eframe / wgpu）。

**迁移步骤**

1. rebase 到 `dev` 时，根 `Cargo.toml`、`rust-toolchain.toml`、`Cargo.lock` 一律用 foundation 的版本，丢掉 `ui/shell` 自己的（冲突时取 `dev` 一侧）。然后不带 `--locked` 跑一次 `cargo check -p bae-ui`，让 `Cargo.lock` 补上 bae-ui 这一项，并确认 lock 里没有别的变化。
2. `crates/bae-ui/Cargo.toml` 改成：

   ```toml
   [package]
   name = "bae-ui"
   description = "…"
   version.workspace = true
   edition.workspace = true
   rust-version.workspace = true
   license.workspace = true
   repository.workspace = true
   publish.workspace = true

   [dependencies]
   egui.workspace = true
   serde.workspace = true
   serde_json.workspace = true

   [lints]
   workspace = true
   ```

   打开工作区 lints 后预计要修几处 clippy 警告（`cargo clippy -p bae-ui --all-targets -- -D warnings`）。
3. egui 仍是 0.36.x（foundation pin 的是 0.36.2，与它 lock 里的版本相同），代码应该不需要改。
4. 示例 `examples/shell.rs`，二选一：
   - **推荐：把外壳放进 `apps/bae` 运行**，删掉示例和 eframe。正式应用用 egui-winit + egui-wgpu 在 `bae_gpu` 的共享设备上驱动 `bae-ui`，所以 `bae-ui` 不能依赖 `eframe::App`、`eframe::Frame`、eframe 的 storage 这类 API（现在的 `src/` 已经满足）。
   - 保留示例：foundation 不 pin eframe，改成显式 dev-dependency：

     ```toml
     [dev-dependencies]
     eframe = { version = "0.36.2", default-features = false, features = ["wgpu_no_default_features"] }
     wgpu.workspace = true   # 让示例用工作区的 wgpu 后端（metal + dx12）
     ```

     eframe 0.36.2 在桌面上经 egui-wgpu `^0.36.2` 使用 wgpu `^30.0`，所以仍然只有一个 wgpu。不要用 `features = ["wgpu"]`：它会打开 `egui-wgpu/default`，进而打开 wgpu 的全部后端（Vulkan、GLES、WebGPU），也是一份 wgpu，但编译量大得多。另外 eframe 0.36.2 用 pollster `^1.0`，在我们升级 pollster 之前示例里会多一份 pollster（只影响示例）。
5. 标题栏：`apps/bae` 的外壳负责执行根视口的 `ViewportCommand`。每帧取 `FullOutput.viewport_output` 里 `ViewportId::ROOT` 那一项的 `commands`，交给 `egui_winit::process_viewport_commands(ctx, &mut viewport_info, commands, &window, &mut actions_requested)`；创建根窗口时用 `egui_winit::create_winit_window_attributes(ctx, bae_ui::titlebar::viewport_builder(title))`，macOS 的隐藏标题栏设置才会生效。
6. 浮动面板：`apps/bae` 先调用 `ctx.set_embed_viewports(true)`，egui 会把额外视口画成主窗口里的浮动窗（`bae-ui::dock` 已处理 `ViewportClass::EmbeddedWindow`）。原生多窗口（每个视口一个 winit 窗口和一个 surface，共用同一个设备）作为后续请求，记在 01 任务书里。
7. 体积：PR #1 里 8.6 MiB 的示例测量以后由 `cargo xtask size`（对整个 `bae` 应用）代替；`serde_json` 的增量从 `cargo xtask size` 的 per-crate 分解（cargo-bloat）里看。
8. `egui_dock` 留在工作区 pin 里给其他界面 crate（12、13、14）用，`bae-ui` 不用它也没关系。
