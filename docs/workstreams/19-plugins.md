# 19 · 自有插件体系

| 项 | 内容 |
|---|---|
| 分支 | `core/plugins` |
| Orca 工作区 | `bae-19-plugins` |
| 阶段 | P1（插件接口在契约阶段先定好，本工作流负责实现） |
| 依赖 | 01、02、04 |
| 负责目录 | `crates/bae-plugin/`、`crates/bae-plugin-sdk/`、`sdk/c/`、`plugins/`、`docs/plugins/` |

## 目标

建立完善的自有插件体系：插件宿主、开发接口（Rust SDK + 稳定 C ABI + 脚本 API）、示例插件和开发文档。内置功能也通过它注册，第三方插件和内置功能走同一条路径。设计见 `docs/ARCHITECTURE.md` 第 14 节。

## 要做

- **清单与发现**：`bae-plugin.toml` 格式（ID、版本、SDK/ABI 版本、类型、入口、权限、平台）；用户级、系统级、工程内三级插件目录；`.baeplugin` 包的安装与卸载；版本与依赖检查。
- **注册**：插件注册到各注册表（效果、命令、导入导出器、编解码器、面板、工作区、表达式函数）；支持运行时加载/卸载，不需要重启。
- **声明式 GPU 插件**：清单 + WGSL + 参数定义 → 自动变成渲染图里的效果节点，与内置效果同一路径、零额外开销；着色器编译错误要有清楚的报告。
- **原生插件**：稳定 C ABI（`sdk/c/bae_plugin.h`，用 cbindgen 从 Rust 生成）+ `bae-plugin-sdk` Rust 封装；套件（suite）+ 版本 + 能力协商；libloading 加载；可以拿到原生 Metal / D3D12 设备和纹理句柄（经 wgpu-hal）；插件声明线程安全性、ROI、位深、是否支持 GPU。
- **进程外模式（可选）**：给不稳定的原生插件用，通过共享内存传帧；默认进程内。
- **脚本插件**：JavaScript（QuickJS，与表达式引擎共用运行时），对象模型尽量对齐 AE 脚本（app.project、items、layers、properties），只能通过命令修改工程。
- **缺失插件占位**：工程里用到但没安装的插件，数据完整保留，界面显示占位。
- **示例插件（`plugins/`）**：一个 WGSL 效果、一个原生 CPU 效果、一个原生 GPU 互操作效果、一个导入器、一个命令插件、一个脚本插件。
- **文档（`docs/plugins/`，中文）**：概念、快速开始、清单参考、ABI 与套件参考、效果插件指南（参数、ROI、多线程、GPU）、脚本 API 参考、发布与安装。
- **dogfooding**：至少把 07 的一部分内置效果改成通过插件接口注册，证明接口够用。

## 对外接口

- `PluginHost`、`PluginManifest`、各插件类型的 trait；C ABI 头文件；Rust SDK。
- 给 20（AE 兼容层）：效果插件生命周期与 AE `EffectMain` 命令的对应关系。

## 验收

- 声明式 WGSL 插件与同功能内置效果的渲染耗时差 < 2%。
- 原生示例插件用旧版 SDK 编译后，在新版宿主上照常运行（ABI 稳定性测试）。
- 运行时加载/卸载插件，不重启、不泄漏 GPU 资源。
- 插件崩溃（进程外模式）不影响主程序。

## 不做

- 插件市场、签名与付费（以后再定）；AE 插件兼容层（20）。

## 进度

- [x] 设计规格 `docs/spec/plugin-architecture.md`（v0.1，已对齐合并后的 `bae_render::Effect` 和 `bae_model::EffectDef`）
- [x] C ABI 头文件 `sdk/c/bae_plugin.h` 雏形（ABI 0.1）+ 自检（`sdk/c/tests/check.sh`：结构体布局、常量、签名、C++17、迷你宿主跑通纯 C 示例）
- [x] 清单格式与校验（`bae_plugin::manifest`，错误带行列号）；参数 → `bae_model::EffectDef`（`effect_def`）
- [x] 声明式 WGSL 约定：前导代码、模块拼装、naga 校验并把错误定位回作者文件、uniform 布局（`bae_plugin::wgsl`）
- [ ] 声明式 WGSL 效果的 GPU 执行（`DeclarativeEffect` 实现 `bae_render::Effect`）
- [ ] 发现、加载、注册与运行时卸载（`PluginHost`）
- [ ] 开发文档 `docs/plugins/`
- [ ] 原生插件加载（libloading）+ Rust SDK
- [ ] 脚本插件
- [ ] 其余示例插件

### 0.0.1 记录

- 接口 PR：清单、效果注册路径（`EffectDef` + `bae_render::Effect`）、参数描述、WGSL 约定、C ABI 雏形。
- 根 `Cargo.toml` 加了 `bae-plugin`、`bae-plugin-sdk` 两个路径依赖，以及 `naga = 30.0.2`（wgpu 已经链接的同一份，只开 `wgsl-in`）和 `semver = 1.0.28` 两个版本固定。
- 自动化脚本的清单表从 `[[script]]` 改名为 `[[automation]]`（`[script]` 是运行时配置表，TOML 里两者不能同名）。
- 给 02 的请求：R02-8（字符串/素材参数）、R02-9（相对坐标点参数默认值）；给 04 的请求：R04-11（`EffectParams` 补字段）、R04-12（在 crate 外构造 `EncodeContext`）。

## 请求（其他工作流在这里留言）
