# BAE 插件开发文档

BAE 的效果、导入导出器、命令、面板都可以由插件提供。内置功能和第三方插件走同一套注册接口：插件效果和内置效果在渲染图里是同一种节点，没有"二等公民"。

规范性的设计文档是英文的 [`docs/spec/plugin-architecture.md`](../spec/plugin-architecture.md)（下文简称"规格"），本目录是面向插件作者的中文指南。

## 三种插件形态

| 形态 | 写什么 | 适合 | 性能 | 当前状态（插件 API 1.0） |
|---|---|---|---|---|
| **声明式 GPU 插件**（首选） | 清单 + WGSL 着色器 + 参数表，不含本地代码 | 大部分效果、生成器、转场 | 与内置效果同一条执行路径 | **可用**：加载、校验、在 GPU 上执行、运行时卸载 |
| 原生插件 | 动态库（`.dylib` / `.dll`），通过稳定的 C ABI（`sdk/c/bae_plugin.h`）接入；Rust 用 `bae-plugin-sdk` | CPU 算法、已有的 C/C++ 代码、编解码、导入导出、原生 GPU 互操作 | 可信插件进程内直接调用；其他插件进程外运行（崩溃不影响 BAE） | **可用**：ABI 1.0 稳定；CPU 效果加载、渲染、崩溃隔离、失败时停用并提示；GPU 路径在后续版本 |
| 脚本 / 脚本插件 | JavaScript（QuickJS，和表达式共用引擎），对象模型参照 AE 脚本 | 自动化、批处理、命令 | 按需调用 | 文件 ▸ 脚本、MCP `script.run` 和清单注册的菜单命令、快捷键、自动化脚本均可用。见[脚本开发指南](scripting.md)。 |

不确定用哪种时：能写成"每个输出像素读几个输入像素、做一些数学"的效果，都用声明式。

## 文档目录

| 文档 | 内容 |
|---|---|
| [快速开始](quickstart.md) | 十分钟写出第一个声明式效果（色调分离），检查、测试、安装 |
| [清单参考](manifest.md) | `bae-plugin.toml` 的全部键：插件信息、效果、参数类型、ROI、多个 pass、权限、依赖、本地化 |
| [WGSL 参考](wgsl.md) | 声明式效果的着色器约定：坐标系、宿主生成的前导代码、`params`、辅助函数、像素式与计算式、诊断信息 |
| [原生插件（C ABI）](native-c.md) | 入口函数、版本协商、套件、效果 vtable 与 AE 命令的对应、ABI 规则、进程内 / 进程外、失败处理、纯 C 示例 |
| [原生插件（Rust）](native-rust.md) | 用 `bae-plugin-sdk` 写原生效果：参数、渲染、线程、错误和 panic、调试 |
| [脚本](scripting.md) | 运行 AE 风格脚本（文件 ▸ 脚本、脚本文件夹、MCP `script.run`）、支持的 AE 脚本 API、撤销与错误、和 AE 的差别 |
| [打包、安装与版本](publishing.md) | 插件目录、`.baeplugin` 包、版本选择、卸载与"缺失插件"占位、兼容性承诺 |

## 示例

| 目录 | 形态 | 演示 |
|---|---|---|
| [`plugins/examples/posterize`](../../plugins/examples/posterize) | 声明式，像素式 | 最小的完整效果（快速开始用它） |
| [`plugins/examples/vignette`](../../plugins/examples/vignette) | 声明式，像素式 | 点、弹出菜单、颜色参数；线性光混合 |
| [`plugins/examples/box-blur`](../../plugins/examples/box-blur) | 声明式，计算式 | 两个 pass + 中间纹理；ROI 扩展 |
| [`plugins/examples/align-to-grid`](../../plugins/examples/align-to-grid) | 脚本 | 命令（菜单、快捷键、MCP 工具）+ 自动化脚本 |
| [`plugins/examples/scripts`](../../plugins/examples/scripts) | 脚本（`.jsx`） | 下三分之一字幕、弹跳关键帧、批量重命名、对齐网格 |
| [`sdk/c/examples/invert`](../../sdk/c/examples/invert) | 原生（纯 C） | 不用任何 SDK 直接实现 C ABI |
| [`plugins/examples/duotone-rs`](../../plugins/examples/duotone-rs) | 原生（Rust，`bae-plugin-sdk`） | 颜色、浮点参数；进程外运行 |

所有示例都有自动测试：`cargo test -p bae-plugin` 用宿主代码加载每个示例并在 GPU 上和 CPU 参考结果对比（原生示例由 `--test native` 编译、加载、渲染）；`sdk/c/tests/check.sh` 编译纯 C 示例并用迷你宿主跑通 ABI 调用流程。

## 几个基本概念

- **插件（包）**：一个目录（或 `.baeplugin` zip 包），根目录有 `bae-plugin.toml`。一个插件 ID、一个版本。
- **贡献（contribution）**：插件提供的一项扩展，比如一个效果、一个命令。一个插件可以有多个。
- **完整 ID**：`<插件 ID>/<贡献 ID>`，例如 `com.example.posterize/posterize`。工程文件里记录的就是它（效果的 `match_name`），所以发布后不能再改。
- **参数 ID**：每个参数有一个稳定的数字 ID（对应 AE 的参数索引）。工程按 ID 保存参数值；ID 永不复用、永不改类型。
- **插件 API 版本**：清单里的 `api = "1.0"`。同一个数字同时覆盖 C ABI、WGSL 前导代码和脚本 API。1.0 从 M1 起稳定，主版本 1 内只做追加；按 0.1 预发布版写的插件仍然可以加载。

## 在 BAE 里的位置

```text
bae-plugin.toml + shaders ──Package::load──► 校验、编译 ──► EffectDef（参数定义）
                                                        └─► DeclarativeEffect（GPU 实现）
EffectDef         ──► bae_model::EffectRegistry   效果控件、效果和预设、Agent、AE 导入、缺失占位
DeclarativeEffect ──► bae_render::EffectLibrary   渲染图（和 07 的内置效果同一个 trait）
```

原生效果走同一条路：`bae_plugin_entry` → `global_setup` / `define_params` 得到参数定义 → `NativeEffect`（GPU → CPU → 插件 → GPU 的渲染节点）注册进同一个 `EffectLibrary`。

宿主代码在 `crates/bae-plugin`（`PluginHost` 负责发现、选版本、注册和卸载；`native` 模块负责原生插件）。应用里由 `PluginModule` 接入：插件效果发布在应用的运行时注册表 `bae_app::Registry<EffectDef>` 和 `Registry<dyn bae_render::Effect>` 里（所有者是 `Owner::Plugin(<插件 ID>)`），和内置模块注册的效果在一起；命令行有 `bae plugin check`、`bae plugin list`，以及进程外插件用的隐藏子命令 `bae plugin runner`。插件加载失败、崩溃、被停用时，`PluginRuntime::take_notices()` 给出提示，应用显示在状态栏。
