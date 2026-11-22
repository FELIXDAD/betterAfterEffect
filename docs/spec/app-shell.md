# 应用外壳（apps/bae）

本文说明桌面应用 `bae` 的启动流程、模块注册、窗口与事件循环、GPU 共享和视口纹理。负责工作流：01 `core/foundation`。

## 1. 组成

| 位置 | 内容 |
|---|---|
| `crates/bae-app` | `AppBuilder`、`AppModule`、资源类型表（`Resources`）、运行时注册表（`Registry<T>`）。不依赖界面、GPU 和平台，任何 crate 都可以实现 `AppModule`。 |
| `crates/bae-platform` | 目录、日志、崩溃报告与紧急保存、单实例、设置文件、命令行（子命令注册表）。`PlatformModule` 注册保留子命令的占位。 |
| `crates/bae-gpu` | 全进程唯一的 `GpuContext`、窗口表面、能力报告、计时、回读/上传、原生句柄、`ViewportTextureRegistry`。`GpuModule` 注册 `bae gpu-info`。 |
| `apps/bae/src/main.rs` | **只做组装**：按顺序 `add_module(...)`，然后交给 `startup::run`。 |
| `apps/bae/src/startup.rs` | 解析命令行 → 目录 → 日志 → 崩溃钩子 → 分派到界面 / 无界面 / 子命令。 |
| `apps/bae/src/shell/` | winit 窗口、事件循环、原生菜单（muda）、窗口位置记忆、设备丢失恢复、`--smoke-test`。 |
| `apps/bae/src/ui/` | egui-wgpu 渲染器（与渲染共用设备）、视口纹理桥接、占位工作区（egui_dock）和状态栏。 |
| `apps/bae/src/headless.rs` | `bae --headless`：不创建窗口；`--smoke-test N` 在离屏纹理上渲染 N 帧界面。 |

## 2. 模块与注册表

```rust
pub trait AppModule: Send + Sync + 'static {
    fn name(&self) -> &str { /* 类型名 */ }
    fn build(&self, app: &mut AppBuilder);      // 插入资源、注册条目、add_module 依赖
    fn finish(&self, app: &mut AppBuilder) {}   // 所有模块 build 之后再运行，可读取别人的注册
}
```

- 每个功能 crate 提供一个模块，在 `build` 里注册命令、面板、工作区、导入器、导出器、效果、CLI 子命令等。`apps/bae/src/main.rs` 只多加一行 `add_module`。依赖关系由模块自己 `app.add_module(Dep)` 声明，重复添加是空操作。
- 注册表是 `Registry<T>`（`T` 可以是 `dyn Trait`）：读取无锁（`arc-swap` 快照），写入可以在**启动后任何时候**进行，也可以按 `Owner` 整体撤销。`App::unregister_owner(&Owner::plugin(id))` 一次清掉某个插件在所有注册表里的条目——19 号插件宿主在运行时加载 / 卸载第三方插件就走这条路。
- 命名：内部的是**应用模块**（`AppModule`，`Owner::Module`），面向用户的是**插件**（`bae-plugin`，`Owner::Plugin`）。代码里不要混用。
- 具体的条目类型由拥有它的工作流定义：命令（02 `bae-model`）、面板与工作区（11 `bae-ui`）、效果（04/07）、导入导出器（08/15/16）。外壳只提供机制。

### CLI 子命令

`bae-platform::cli` 维护 `Registry<CliSubcommand>`。保留名（`gpu-info`、`render`、`mcp`、`export-json`、`pack`）由 `PlatformModule` 注册成占位，运行时打印 “not implemented yet” 并以退出码 3 结束。工作流在自己的模块里用 `cli::implement(&registry, owner, CliSubcommand::new(...).with_args(...).with_run(...))` 接管，也可以注册全新的子命令。

```text
bae [PROJECT…]              打开界面（默认）
bae --headless              不创建窗口
bae --smoke-test <N>        渲染 N 帧后退出（0 = 成功），可与 --headless 组合
bae --log <FILTER>          日志过滤，例如 "info,bae_gpu=debug"
bae --version
bae gpu-info [--json]       GPU、特性、限制、格式支持
bae export-json <工程>       工程导出成可读 JSON
bae pack <工程> [-o 输出.baez] [--overwrite] [--used-only] [--fail-on-missing] [--json]
                            打包成自带素材的 .baez（bae-package），保留保存的视图
bae render | mcp …          其他工作流实现
```

## 3. 启动流程

1. Windows 上重新连接父进程控制台（release 是 GUI 子系统程序，双击不弹黑窗）。
2. 用注册表里的子命令解析命令行。
3. `AppDirs::resolve()`（`BAE_HOME` 可把所有目录放到一个根目录下）。
4. 日志：stderr + 每次会话一个文件（子命令默认只输出 warn 以上，不写文件）。
5. 崩溃钩子：写崩溃报告、运行紧急保存回调；上一次崩溃会在启动时提示。
6. 分派：子命令 → `cli::dispatch`；`--headless` → `headless::run`；否则 → `shell::run`。

## 4. 窗口与事件循环

- 一个 winit 窗口；`GpuContext::new_with_surface` 选择能向这个窗口呈现的适配器（默认高性能优先），创建的 `Arc<GpuContext>` 作为资源放进 `App`，界面、渲染、插件共用它的 device / queue。
- **事件驱动重绘**：`ControlFlow::Wait`。只有输入事件、egui 的重绘请求（通过 `set_request_repaint_callback` 投递到事件循环，支持延迟重绘）、窗口变化才会产生一帧。空闲时 CPU 占用接近 0。
- 呈现：默认 `PresentModePreference::Vsync`（FifoRelaxed / Fifo），帧延迟 2；表面格式默认 8 位非 sRGB（egui 在 gamma 空间混合）。最小化时暂停呈现；`Outdated` 自动重配；`Lost` 从窗口重建表面。
- DPI：由 egui-winit 跟随窗口缩放；表面按物理像素配置。
- 窗口位置、大小、最大化、全屏保存在 `<state>/window.json`（版本化设置文件），只有仍在某个显示器上的位置才会恢复；`--smoke-test` 不写入。
- 单实例：界面模式在 `<state>/bae-gui.lock` 上加锁，第二个实例提示后退出（以后改为把参数转给已运行的实例）。`--smoke-test` 和 `--headless` 不加锁。
- 原生菜单栏：muda。菜单项 id 就是命令 id（`file.open_project`、`app.quit` …）。"文件"菜单的项和各模块的菜单一样注册在 `Registry<MenuItemSpec>`（`shell::menu::file_items`），菜单上显示当前键位表绑定的快捷键；编辑菜单不设置原生快捷键，避免在 macOS 上抢走 egui 的 Cmd+Z / Cmd+C，快捷键由 11 的快捷键系统处理。
- egui 的根视口命令（自绘标题栏的拖动、最小化、最大化、关闭）通过 `egui_winit::process_viewport_commands` 执行；`ViewportCommand::Close` 会关闭应用（除非同一帧有 `CancelClose`）。
- 额外视口（浮动面板）：暂时 `ctx.set_embed_viewports(true)`，egui 把它们画成主窗口内的浮动窗。原生多窗口（每个视口一个 winit 窗口 + 一个 surface，共用同一设备）是后续工作。

### 工程文件

`apps/bae/src/document.rs`（文档层）和 `apps/bae/src/files.rs`（文件命令流程）：

| 命令 | 快捷键（同 AE） | 行为 |
|---|---|---|
| `file.new_project` 新建项目 | Cmd/Ctrl+Alt+N | 有未保存的修改时先问；换成未命名的空工程 |
| `file.open_project` 打开项目… | Cmd/Ctrl+O | 原生打开对话框；先读文件（坏文件直接报错，不动当前工程），再问未保存的修改，再切换。参数 `{"path": …}` 跳过对话框 |
| `file.close_project` 关闭项目 | 无（AE 没有默认快捷键） | 先问；留下一个未命名的空工程（同 AE） |
| `file.save` 保存 | Cmd/Ctrl+S | 写回原位置；未命名工程走"另存为" |
| `file.save_as` 另存为… | Cmd/Ctrl+Shift+S | 原生保存对话框，格式可选 `.bae`（目录）或 `.baez`（单文件） |

- 打开：`.bae` 目录、`.baez` 文件，或 `.bae` 目录里的 `project.json`，都经过 `bae_package::open_project`，`.baez` 里的素材原地读取（`AssetLocator`）。macOS 的打开面板可以直接选 `.bae` 目录；Windows 的对话框只能选文件或只能选目录，所以选 `.baez` 或 `project.json`。
- 保存只是序列化（`Project::save_as` / `write_baez`），不改工程数据。从 `.baez` 打开的工程，保存时包里的素材原样保留（同名 `assets/…`）；另存为 `.bae` 目录时这些素材解到目录的 `assets/` 下，相对路径仍然有效。
- 未保存的修改不另外记标志：比较每个工程层的 Loro 版本向量和层叠顺序与上次打开 / 保存时是否相同（命令、撤销、重做、远程导入都会推进版本）。保存后撤销算作新的修改。窗口标题在有修改时加 ` *`（AE 的写法），macOS 同时设置窗口的"已编辑"状态。
- 关闭工程、新建、打开、退出（菜单、Cmd/Ctrl+Q、关闭窗口）前，有修改时弹原生提示：保存 / 不保存 / 取消。macOS 是 NSAlert（保存是默认按钮）；Windows 是 MessageBox 的 是 / 否 / 取消（自定义按钮文字需要 comctl32 v6 清单，可执行文件目前没有）。"保存"失败或取消保存对话框时不关闭。
- 顺序（`files::CommitGate`）：文件命令和退出总在它们之前的编辑之后执行。同一帧里快捷键先于面板的提交被读到（输入数值、回车、Cmd/Ctrl+S 连按时是 `[file.save, attr.set_at]`），外壳把这一批的其他命令先跑；macOS 菜单和快捷键也排在上一帧的编辑后面。正在输入数值时按保存 / 退出，外壳先结束输入（字段失去焦点时提交，同 AE 的点别处），下一帧提交后再执行；窗口 300 ms 内不重绘（例如最小化）就不再等。
- 另存为到已有的 `.bae` 工程（#126）：`.bae` 在 Windows（以及不在 BAE.app 里运行的 macOS 程序）上是普通文件夹，保存对话框会走进去。选中的路径如果本身是 `.bae` 工程目录，或者在某个 `.bae` 工程里面（取最外层），就当作替换这个工程，绝不在工程里再嵌套一个工程；先问「“…”已存在。要替换它吗？」（BAE.app 里 `.bae` 是包，保存面板自己已经问过，就不再问）。替换先写到旁边的临时文件夹，成功后再换上，失败时原工程不动。路径落在当前工程自己的文件夹里时直接原位保存；名字像工程、但不是工程的非空文件夹拒绝写入。
- Windows 的另存为对话框不用 rfd，而是我们自己调 IFileSaveDialog，并注册 IFileDialogEvents（`shell::win_save`）。判断逻辑在 `shell::save_guard`，不依赖 Win32，所有平台都测：
  - 在对话框里选中 `proj.bae` 点保存、双击它，或者在对话框打开后用任何方式进入一个 `.bae` 工程目录：`OnFolderChanging` 拒绝进入，关掉对话框，返回这个工程的路径，再走上面的「要替换它吗？」。
  - 对话框打开时自己导航到起始文件夹，这一步不拦截（起始文件夹可能是系统记住的、在某个工程里面的位置）。
  - 名字像工程、但里面没有 `project.json` 的文件夹照常进入。
  - 输入不带扩展名的 `proj`，默认扩展名补成 `proj.bae` 后正好是已有工程：打开 `FOS_NOTESTFILECREATE`，系统不再先试着在文件夹上建文件、报「你没有权限在此位置中保存文件」；`OnFileOk` 认出这个工程，同样走替换确认。写入失败由另存为用我们自己的说法报告。
  - `OnOverwrite` 遇到工程目录时直接接受，不弹系统的覆盖提示（我们自己会问）；已有的 `.baez` 文件仍然由系统提示覆盖。
  - macOS 保持用 rfd。
- 对话框都在 `files::FileDialogs` trait 后面：外壳用 `shell::dialogs::Native`（rfd），冒烟测试用 `files::NoDialogs`（从不阻塞），测试注入路径和回答。
- 命令行：`bae <工程>` 启动时打开；打不开时窗口出来后弹错误提示，`--smoke-test` 时直接以 1 退出。`bae --headless <工程>` 打开并打印一行摘要；`bae --headless --smoke-test N <工程>` 带着这个工程渲染界面。
- 打开另一个工程时清空历史记录面板，并重置动效工作区的会话状态（选择、播放）。打开的合成按 AE 的做法恢复：保存时把打开着的合成和各自的当前时间记在 `project.json` 的 `view` 里（`view_state.rs`），打开时恢复；没有记录（或记录里的合成都不在了）就打开第一个合成。
- 打不开的工程（`open_error.rs`）：提示写明是哪个文件、出了什么问题——不存在、文件夹里没有 project.json、不是 BAE 项目、.baez 已损坏、project.json 已损坏、更新版本保存的、工程层数据缺失或损坏、没有读取权限——中英文都有，下面一行是技术原因。打开对话框、`bae <工程>`、`bae --headless <工程>`、`bae export-json`、`bae pack`、`bae mcp` 都用它；`bae render`（在 bae-render 里打开工程）通过应用放进去的 `bae_platform::cli::ProjectOpenError` 资源用同一套说法。`acceptance_cli::every_entry_point_explains_open_failures_the_same_way` 逐个入口检查。
- 从访达打开（macOS）：`BAE.app` 的 Info.plist 声明了 `.bae`（UTI `io.github.felixdad.bae.project`，作为包显示成一个文档）和 `.baez`（`io.github.felixdad.bae.package`）两种文档类型，见 `xtask/src/dist.rs`。双击、拖到程序坞图标、`open 工程.bae` 都不是命令行参数，而是 Apple 事件，由 `NSApplication` 交给代理的 `application:openURLs:`。winit 0.30 的代理（`WinitApplicationDelegate`）没有实现这个方法，`shell::open_documents` 在事件循环创建后、`run_app` 之前把它加到这个类上（winit 自己也是这样替换 `sendEvent:` 的），所以双击工程启动 BAE 的那一次也收得到。路径变成 `UserEvent::OpenFiles`，再变成普通的 `file.open_project`（经 `view_state::run_file_command`）：未保存的修改照常先问，坏文件照常报错（`open_error.rs`），打开后恢复上次打开的合成，一次只开第一个。BAE 已经在运行时，macOS 把事件发给正在运行的实例。Windows 的双击打开要靠安装器写文件关联，0.0.1 没有安装器，暂时只能在 BAE 里打开或用 `bae <工程>`。

### 设备丢失

`GpuContext::on_device_lost` 把丢失事件投递到事件循环。外壳保留窗口和界面状态，丢弃旧设备上的所有资源，用 `GpuContext::recreate` 的同一描述重新创建上下文、表面和 egui 渲染器，并把新的 `Arc<GpuContext>` 放回 `App`。连续失败 3 次后运行紧急保存并退出。其他持有旧上下文的子系统（渲染缓存、插件）需要在恢复后重建自己的资源：计划由 04 定义“GPU 上下文已更换”的事件。

## 5. 视口纹理（零拷贝显示渲染结果）

`bae_gpu::viewport::ViewportTextureRegistry`（资源类型 `Arc<ViewportTextureRegistry>`，由 `GpuModule` 插入）把“视口 id → `wgpu::TextureView`”交给界面，像素始终留在 GPU 上：

```text
bae-render ──publish(id, texture)──► ViewportTextureRegistry ◄──每帧同步── apps/bae 外壳（egui-wgpu）
                                            │ display_handle(id)
                                            ▼
                              界面面板：egui::Image(TextureId::User(handle))
```

| 角色 | 调用 |
|---|---|
| 渲染器 | `let id = registry.create("comp main")`；渲染到纹理后 `registry.publish(id, &texture, FilterMode::Linear)`；纹理对象改变（分辨率变化、重新分配）时再次 `publish`；不用时 `registry.remove(id)`。同一纹理里画新内容**不需要**重新发布。 |
| 外壳 | 每帧 `UiRenderer::sync_viewports`：注册表的 `generation()` 没变就什么都不做；新发布的视图用 `egui_wgpu::Renderer::register_native_texture` 注册，重新发布的用 `update_egui_texture_from_wgpu_texture` 重新绑定，删除的释放；然后 `set_display_handle(id, egui 的 User id)`。 |
| 界面面板 | `registry.display_handle(id)` 得到 `u64`，用 `egui::TextureId::User(handle)` 画图；`registry.get(id)` 给出尺寸和格式。 |

对发布纹理的要求：2D、带 `TEXTURE_BINDING`、可过滤的颜色格式（`Rgba8Unorm`、`Rgba16Float` …）、预乘 alpha、已经是界面期望的显示编码（06 的显示变换在发布之前完成）。注册表本身不依赖 egui，渲染 crate 只依赖 `bae-gpu`。

目前外壳发布一张占位测试图（`ui::publish_placeholder_comp`），让“注册表 → egui-wgpu → 面板”的路径在渲染器合并前就被 `--smoke-test` 覆盖。

## 6. 冒烟测试

| 命令 | 内容 |
|---|---|
| `bae --smoke-test 120` | 打开窗口，尽快渲染 120 帧（`NoVsync`）后以 0 退出。窗口不能呈现时（被遮挡、CI 桌面）改为离屏渲染，测试仍然覆盖完整界面路径；20 秒没有进展或出现 wgpu 校验错误则以 1 退出。 |
| `bae --headless --smoke-test 30` | 不创建窗口，在 1280×720 离屏纹理上渲染 30 帧界面，回读检查画面不是单色、没有校验错误。CI 使用这一项。 |
| `bae [--headless] --smoke-test 3 <工程>` | 同上，但先打开这个工程；打不开以 1 退出。 |

## 7. 给其他工作流

- 11 `ui/shell`：把 `apps/bae/src/ui/workspace.rs` 的占位工作区换成 `bae-ui` 的停靠和面板；`i18n.rs` 的小表换成 `bae-ui::i18n`；面板 / 工作区注册类型定义在 `bae-ui`，通过 `AppModule` 注册。迁移步骤见 `docs/spec/dependencies.md` 第 10 节。
- 04 `render/compositor`：通过 `Arc<GpuContext>` 资源拿设备，用视口纹理注册表显示结果；`bae render` 子命令用 `cli::implement` 接管。
- 17 `agent/api`：`bae mcp` 子命令同上；截图用 `bae_gpu::transfer::read_texture`。
- 19 `core/plugins`：`App::unregister_owner(&Owner::plugin(id))` 卸载插件；原生 GPU 句柄用 `GpuContext::native_device` / `native_texture`；插件目录用 `AppDirs::plugin_dirs()`。
