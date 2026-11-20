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
- [x] 声明式 WGSL 效果的 GPU 执行（`DeclarativeEffect` 实现 `bae_render::Effect`；GPU 测试对照 CPU 参考，含 RGBA16F/32F、半分辨率、ROI、identity）
- [x] 发现、版本选择、注册与运行时卸载（`PluginHost`；加载→渲染→卸载 100 次无 GPU 资源泄漏）
- [x] 内置效果的嵌入式声明式包（`DeclarativeEffect::load_builtin`，给 07）
- [x] 开发文档 `docs/plugins/`（概览、快速开始、清单参考、WGSL 参考、C ABI 入门、打包安装与版本）
- [x] 示例：posterize、vignette、box-blur（声明式），align-to-grid（脚本），invert（纯 C）
- [x] 应用接入：`PluginModule` + 启动时 `load_installed`（`AppDirs::plugin_dirs()` + `BAE_PLUGIN_PATH`），插件效果发布到 `bae_app::Registry<EffectDef>` / `Registry<dyn bae_render::Effect>`；`bae plugin check|list [--json]`
- [ ] 渲染耗时对照基准（声明式 vs 同功能内置效果 < 2%，等 04 的执行器和 07 的内置效果）
- [ ] 原生插件加载（libloading）+ Rust SDK
- [ ] 脚本插件
- [ ] 其余示例插件

### 0.0.1 记录

- 接口 PR：清单、效果注册路径（`EffectDef` + `bae_render::Effect`）、参数描述、WGSL 约定、C ABI 雏形。
- 根 `Cargo.toml` 加了 `bae-plugin`、`bae-plugin-sdk` 两个路径依赖，以及 `naga = 30.0.2`（wgpu 已经链接的同一份，只开 `wgsl-in`）和 `semver = 1.0.28` 两个版本固定。
- 自动化脚本的清单表从 `[[script]]` 改名为 `[[automation]]`（`[script]` 是运行时配置表，TOML 里两者不能同名）。
- 给 02 的请求：R02-8（字符串/素材参数）、R02-9（相对坐标点参数默认值）；给 04 的请求：R04-11（`EffectParams` 补字段）、R04-12（在 crate 外构造 `EncodeContext`）。
- 第三个 PR（应用接入，协调人决定由 19 做）：`apps/bae` 只改三行（依赖、`add_module`、启动时调用 `load_installed`）。`PluginHost` 改为通过 `EffectRegistrar` 注册（`EffectRegistries` 普通注册表 / `AppEffectRegistries` 应用注册表）。release 体积 15.98 → 16.72 MiB（含 bae-render 首次链接进应用）。给 04 的请求 R04-13：`bae render` 和渲染器从 `Registry<dyn Effect>` 取效果（和 `Registry<dyn SourceProvider>` 一样）。
- 第二个 PR：`DeclarativeEffect`、`PluginHost`、`load_builtin`、检查工具、posterize 示例、`docs/plugins/`。`EffectParams` 没有图层范围、帧率等信息前（R04-11），`BaeFrame.layer_bounds` 用输入范围代替，其余未知字段为 0；`layer` 参数和转场的 `to` 源要等渲染图支持（R04-1），现在绑定为空纹理。

## 请求（其他工作流在这里留言）

### 20 interop/aex-compat → 19（2026-11-20）

**回复 R20-1：已核对。** 19 规格 §9.1 的映射可以用，20 不需要改 19 的效果接口。`bae_aex_host::selector::LifecycleHook` 现在用 `BaeEffectVTable1` 的回调名，测试直接读 `sdk/c/bae_plugin.h`：回调名对不上、或 vtable 增减回调时测试会失败（#52 已合并）。唯一的语义差别（AE `SEQUENCE_FLATTEN` 可能破坏实例，而 `instance_serialize` 约定不破坏）由兼容层内部补齐。

新的请求，详见 `docs/spec/aex-compat.md` §2、§5.3：

1. **R19-03 动态贡献（现在定方向）。** 兼容层是一个单独分发的第一方原生插件包 `bae.aex-compat`。它扫描 AE 插件目录后才知道有哪些效果，没法在 manifest 里静态列出。请允许插件在加载后注册 / 注销任意数量的效果贡献（manifest 声明类似 `dynamic_contributions = true`），并参与版本选择、占位、插件管理器和工程插件锁。**match name 例外**：兼容层注册的是 AE 原始 match name，不加 `<plugin-id>/` 前缀，这样 15 导入时生成的占位在启用兼容层后直接变成可用效果，文档不用改；请 id 规则允许 `bae.aex-compat` 注册这种名字。注销沿用 `EffectRegistry::unregister_plugin`。
2. **R19-06 辅助进程能力（现在定形状）。** 兼容层的辅助进程复用 19 的进程外传输层。在 19 规格 §15.2 的基础上还需要：插件任意线程发起的同步回调；回调未返回时宿主可以再调用辅助进程（嵌套）；一个辅助进程多个在途调用；辅助进程有主线程消息循环，能显示插件自己的模态窗口（选项、许可激活），以 BAE 主窗口为父窗口。
3. **R19-08 没有 manifest 的模块的信任记录（现在定数据形状）。** 按二进制摘要、签名身份、架构和来源目录记录；打开或同步工程永远不自动加载未经同意的二进制。
4. **R19-05 自定义界面（ABI 0.2 设计时）。** `ui_event` / `bae.draw` 定稿时参考 `bae_aex_host::ui` 的事件和显示列表。
5. **R19-02（P3）。** `BaePreRenderResult` 将来要加"缓存键附加字节"，状态返回将来要能"成功但附带提示消息"；现在只要求这两个结构体保持可扩展。

### 14 ui/edit → 19（2026-11-20，Pr 剪辑规划）

依据：`docs/spec/pr/`（Premiere Pro 基础功能清单）和 `docs/spec/pr/edit-model.md`（剪辑数据模型），PR #73。

1. **（M3）过渡插件类型**：声明式 WGSL 插件增加"过渡"类型——两个输入纹理（A、B）+ 进度 uniform（0–1）+ 普通参数，注册成 02 新增的 `EffectDef` "过渡"种类。Pr 26 内置了 Film Impact 的约 100 个过渡和效果（清单见 `docs/spec/pr/effects-transitions.md` 附录 A），BAE 的同类过渡适合全部以这种插件形式补充。
2. **（以后）音频效果插件**：CPU 实现、按块处理采样（参数可动画），供混音引擎（08）调用；是否兼容 VST3 / AU 以后再定。

### 07 render/effects → 19（2026-11-20）

1. `DeclarativeEffect` 的管线缓存只按工作格式区分，没有按设备区分：设备丢失后重建 `GpuContext`、或同一进程里有两个 `GpuContext`（测试）时会用到别的设备的管线，wgpu 报 "Cannot get non-existent resource"。建议按 `GpuContext`（`Weak` 指针）或设备区分缓存。
2. `load_builtin` 改写了 `match_name`，建议同时给参数填 AE 参数 matchName `<matchName>-NNNN`（`EffectParamDef.match_name`）。
3. 每帧每个 pass 都 `create_buffer_init` 三个 uniform 缓冲；07 的 Rust 效果用的是一个 uniform 环形缓冲 + 动态偏移（`crates/bae-effects/src/gpu.rs`），如需要可以共用。

