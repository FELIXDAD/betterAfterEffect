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

- betterAfterEffects 脚本在同次运行里复用项目条目、合成图层和关键帧列表，按索引创建所取的对象。增删或移动图层、修改关键帧后，脚本持有的集合会反映新的顺序和值。
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
- [x] 原生插件（M1）：C ABI 定为 **1.0**（0.1 预发布版仍接受）；libloading 加载、`bae.core` / `bae.param.define` / `bae.render` 套件、效果 CPU 路径（`NativeEffect`：GPU 读回 → `render_cpu` → 上传）；**进程外模式**（`bae plugin runner`，管道 + 共享内存，崩溃 / 卡死只结束子进程、自动重启、一分钟三次停用）；用户级插件默认进程外，随 BAE 安装的 / 系统级可信插件进程内；失败时效果直通输入、插件停用并在状态栏提示（`PluginRuntime::take_notices`）；Rust SDK（`bae_plugin_sdk::abi` 布局镜像 + `effect::CpuEffect` + `export_plugin!`，panic → `BAE_ERR_PANIC`）；示例 `plugins/examples/duotone-rs`（Rust，进程外）+ `sdk/c/examples/invert`（C，进程内）；文档 `docs/plugins/native-c.md`、`native-rust.md`
- [x] 脚本：AE 风格脚本 API 子集（`crates/bae-script`，QuickJS 与表达式共用）；文件 ▸ 脚本（运行脚本文件、安装脚本文件、脚本文件夹列表）；命令 `script.run`（MCP 工具，同一套命令）；一次运行 = 一个撤销步骤；文档 `docs/plugins/scripting.md`，示例 `plugins/examples/scripts/`
- [x] 脚本插件：清单里的 `[[command]]` / `[[automation]]` 通过 `bae-script` 注册成命令（ES 模块 + `import from "bae"`）
- [ ] 其余示例插件

### M1 记录（原生插件）

- 规格变更（D16–D18）：原生插件默认进程外运行，除非可信（随 BAE 安装 / 系统级且清单写 `in-process`，或用户设为进程内；开发用 `BAE_PLUGIN_IN_PROCESS`）。原因是协调人定的硬要求“插件崩溃不能拖垮主程序”，进程内崩溃无法恢复（规格 §16 第 5 条）。ARCHITECTURE §14“稳定性”一句（“原生插件默认在进程内运行”）需要单独 PR 改成“可信插件进程内，其余进程外”，留给领导决定。
- 进程外的运行器就是应用自己（隐藏子命令 `bae plugin runner`），打包不用多带一个可执行文件；`bae-plugin` 另有 `bae-plugin-runner` 二进制给测试和工具用（`BAE_PLUGIN_RUNNER`）。
- 根 `Cargo.toml`：`libloading` 固定改为 0.8.9（wgpu-hal 在 Windows 上已经链接的同一份，原来的 0.9.1 没人用）；新增 `memmap2 = 0.9.12`（fontique 已经链接的同一份）和测试用的 `cc = 1.11.2`（构建脚本已经在用）。
- `apps/bae` 只改状态栏：每帧取 `PluginRuntime::take_notices()`，显示“插件 X 无法加载 / 崩溃 / 已停用”（中英文）。
- 还没做：GPU 路径（`render_gpu`、`bae.gpu.*`）、实例状态、参数监视、图层参数输入、插件管理界面（启用 / 停用、进程内开关；接口是 `PluginRuntime` + `NativeConfig::in_process`，可交给 09fe9cc0）。

### 0.0.1 记录

- 接口 PR：清单、效果注册路径（`EffectDef` + `bae_render::Effect`）、参数描述、WGSL 约定、C ABI 雏形。
- 根 `Cargo.toml` 加了 `bae-plugin`、`bae-plugin-sdk` 两个路径依赖，以及 `naga = 30.0.2`（wgpu 已经链接的同一份，只开 `wgsl-in`）和 `semver = 1.0.28` 两个版本固定。
- 自动化脚本的清单表从 `[[script]]` 改名为 `[[automation]]`（`[script]` 是运行时配置表，TOML 里两者不能同名）。
- 给 02 的请求：R02-8（字符串/素材参数）、R02-9（相对坐标点参数默认值）；给 04 的请求：R04-11（`EffectParams` 补字段）、R04-12（在 crate 外构造 `EncodeContext`）。
- 第三个 PR（应用接入，协调人决定由 19 做）：`apps/bae` 只改三行（依赖、`add_module`、启动时调用 `load_installed`）。`PluginHost` 改为通过 `EffectRegistrar` 注册（`EffectRegistries` 普通注册表 / `AppEffectRegistries` 应用注册表）。release 体积 15.98 → 16.72 MiB（含 bae-render 首次链接进应用）。给 04 的请求 R04-13：`bae render` 和渲染器从 `Registry<dyn Effect>` 取效果（和 `Registry<dyn SourceProvider>` 一样）。
- 第二个 PR：`DeclarativeEffect`、`PluginHost`、`load_builtin`、检查工具、posterize 示例、`docs/plugins/`。`EffectParams` 没有图层范围、帧率等信息前（R04-11），`BaeFrame.layer_bounds` 用输入范围代替，其余未知字段为 0；`layer` 参数和转场的 `to` 源要等渲染图支持（R04-1），现在绑定为空纹理。

### 脚本（交接）

2026-11-24，分支 `plugins/scripting`（Draft PR）。

- **已完成**：`crates/bae-script`（无 GPU / 界面依赖）：AE 脚本 API 常用子集（`app.project`、items、`CompItem`、`layers.addSolid/addText/addNull/addShape/addCamera/addLight/add`、Layer 属性和顺序、`property()` / `setValue` / `setValueAtTime` / 关键帧 / 插值 / 表达式、效果组 `addProperty`、蒙版属性、`TextDocument`、`alert` / `$.writeln`、`bae.commands.invoke`）；命令 `script.run`（自动成为 MCP 工具）；文件 ▸ 脚本（运行脚本文件、安装脚本文件、脚本文件夹列表）；文档 `docs/plugins/scripting.md`；示例 `plugins/examples/scripts/`。`cargo test -p bae-script`（11 个）和 `bae-ui-motion` 的 `scripts` 测试通过，clippy 全工作区通过。
- **后续**：命令目录可以把“运行脚本文件…”“安装脚本文件…”标为 Done。Recent Script Files、表达式求值、工程选区和文件读写留给后续脚本 API。
- **关键设计**：一次运行 = 一个 `Stage` 事务 = 一个撤销步骤；所有修改都在同一事务里调用已注册命令（`cmd.run_json(tx, …)`，和 MCP 同一套）；读取走 `tx.stage()`，能看到本次运行自己的修改；出错时保留之前的修改并报告行号（AE 行为）；`script.run` 返回报告（log / alerts / error / result），界面在 `script.run` 后排一个 `motion.file.script_report` 动作，从 `ScriptConsole` 资源取报告弹窗；JS 对象模型写在 prelude 里，Rust 侧只有一个 `__host(method, json)`；每次运行一个独立 QuickJS 运行时（128 MiB、默认 30 s 中断）；经 `script.run` 时撤销名称是命令标题“运行脚本”，直接调 `bae_script::run` 时用第一个 `beginUndoGroup` 的名字。
- **文件**：`crates/bae-script/src/{lib.rs（ScriptModule、RunScript、ScriptConsole、run/run_in）, engine.rs（运行时、限制、错误行号）, host.rs（__host 方法 → 读取 / 命令）, convert.rs（值 ⇄ AE 单位）, prelude.js（AE 对象模型）}`、`crates/bae-script/tests/scripts.rs`、`crates/bae-ui-motion/src/scripts.rs`（菜单和动作）、`apps/bae/src/main.rs`（加 `ScriptModule`）。

### betterAfterEffects 脚本插件（2026-11-25，PR #329）

- 清单里的脚本命令和自动化脚本均注册到应用命令表，所有者是插件 ID。菜单、快捷键和 MCP 使用同一条命令。重新加载会替换该插件的入口，卸载会移除它们。
- 安装和热更新时先检查清单中的每个导出名。缺少导出会报告脚本文件和名称，旧入口继续可用。插件顶层代码只在调用时执行。
- 网格示例覆盖选区和显式图层路径、静态位置和带关键帧的位置。自动化入口报告偏离网格的图层。每次修改可一次撤销，插件入口出错会回滚本次事务。
- 原生插件的进程策略见 ARCHITECTURE §14 与 D16。原生实现由 PR #332 交付。

### 交接（2026-11-25，给接手的 Codex）

插件体系后续交给 5090 上的 Codex 接手。原生插件部分在 PR「plugins: native plugins (C ABI 1.0) …」（分支 `plugins/v1`）；脚本部分在 `plugins/scripting` 的 Draft PR（它自己的交接写在同一节里）。

**已完成（原生插件）**：见上面“进度”和下面“M1 记录”。可以直接用的入口：`PluginHost::load_from`（会自动加载原生库）、`bae_plugin::native::{load_plugin, NativeEffect, NativeConfig, Notices}`、`bae plugin runner|list|check`。

**剩余清单（按顺序做）**：
1. 插件管理界面（启用 / 停用、“在进程内运行”开关、显示 `bae plugin list` 的信息和 `Notice`）：数据在 `PluginRuntime`（`with_host`、`load`、`unload`、`take_notices`）；开关写进用户设置后传给 `NativeConfig::in_process`（`PluginHost::set_native_config`）。界面放 bae-ui-motion 或 bae-ui，命令（启用、停用）放在不依赖界面的 crate 里。
2. ARCHITECTURE §14“稳定性”一句按规格 D16 改（单独 PR，领导同意后再改）。
3. GPU 路径：`render_gpu` + `bae.gpu.compute`（宿主编码，进程外也能用），然后 `bae.gpu.native`（只能进程内，需要 `gpu_native` 权限）。规格 §12、§15.3。
4. 实例状态（`STATEFUL`：`instance_*`，状态 blob 存在效果 prim 上）和参数监视（`param_changed` + `bae.param.edit`），需要 02 的状态字段。现在加载时会拒绝 `STATEFUL` 效果。
5. 图层参数和转场输入（`checkout_input` 的 `PARAM_LAYER` / `TRANSITION_TO`，现在返回空）：用 `Effect::layer_dependencies` + `EncodeContext::effect_layer`，和声明式效果一样。
6. 其他贡献种类：命令（`BAE_KIND_COMMAND` + `bae.command`，注册成 `DynCommand`，MCP 自动可用）、导入器 / 导出器、表达式库。现在这些会出现在 `LibraryInfo::ignored` 里，并给出警告。
7. 进程外的性能：现在消息用 JSON，像素多拷贝一次（宿主缓冲区 → 共享内存）。要优化的话，改成直接把像素转换进共享内存，消息改成二进制。
8. 渲染耗时基准（声明式 vs 同功能内置效果 < 2%），以及 ABI 稳定性的 CI（保留用 1.0 头文件编译出来的示例二进制，在新宿主上跑）。
9. 其余示例：原生 GPU 互操作、导入器、命令插件。

**关键设计决定**（规格 §24.1 D16–D18）：
- 用户级 / 工程内的原生插件默认进程外运行，可信的才进程内（崩溃不能拖垮主程序是硬要求）。运行器就是 `bae plugin runner`，测试用 `bae-plugin-runner` 二进制（`BAE_PLUGIN_RUNNER`）。
- ABI 1.0 = 0.1 的布局，冻结；宿主还没提供的套件是“暂定”的；0.1 插件仍然接受（`HostApiSupport::compatible_0x_minors`）。
- 原生 CPU 效果就是一个 `bae_render::Effect`：提交这一帧的 encoder、读回输入、调用插件、上传结果。调用失败时直通输入，绝不让整帧失败；失败都发到 `Notices`。
- 参数以 `define_params` 为准；清单里的参数表是可选的列表副本，有的话必须一致（`check_listing`）。弹出菜单在文档里存 1 起的序号，传给插件时换成选项 ID。

**主要文件**：
- `sdk/c/bae_plugin.h`（ABI，权威）、`sdk/c/tests/`（C 侧自检）、`sdk/c/examples/invert`
- `crates/bae-plugin-sdk/src/abi.rs`（Rust 镜像 + 布局 / 常量测试）、`effect.rs`（`CpuEffect`、`export_plugin!`）、`version.rs`（版本规则）
- `crates/bae-plugin/src/native/`：`mod.rs`（配置、隔离策略、`Notices`、`NativePlugin`、`NativeEffect`、参数转换）、`library.rs`（进程内加载 + 宿主套件）、`runner.rs`（进程外：协议、runner 主循环、客户端、共享内存）、`pixels.rs`（格式转换）
- `crates/bae-plugin/src/host.rs`（`load_native`）、`module.rs`（CLI、`PluginRuntime::take_notices`）、`src/bin/runner.rs`
- 测试：`crates/bae-plugin/tests/native.rs`（会编译 C 插件、构建 `plugins/examples/duotone-rs`）、`tests/fixtures/crash/`
- 应用：`apps/bae/src/shell/mod.rs` 的 `show_plugin_notices`，`apps/bae/src/ui/workspace.rs` 的 `plugin_notice`

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

