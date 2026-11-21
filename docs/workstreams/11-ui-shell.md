# 11 · 界面外壳与工作区

| 项 | 内容 |
|---|---|
| 分支 | `ui/shell` |
| Orca 工作区 | `bae-11-ui-shell` |
| 阶段 | P1 |
| 依赖 | 01、02（命令） |
| 负责目录 | `crates/bae-ui/`、`assets/icons/`、`docs/glossary.md` |

## 目标

搭好 AE/Ai 风格的界面框架：可停靠面板、按岗位的工作区、AE/Ai 键位、深色主题和通用控件。12/13/14 在这个框架上做各自的工作区。

## 要做

- **外壳**：原生菜单栏（muda）；多窗口（浮动面板）；egui_dock 停靠系统；`Panel` trait 与面板注册。
- **工作区**：预设（分镜、动效、剪辑、审阅、Agent）；用户自定义、保存、重置；首次启动时选择岗位，进入对应的默认工作区。
- **主题**：深色主题的颜色、字号、间距、圆角 token，贴近 AE/Ai；界面亮度可调（AE 有这个设置）；自绘 SVG 图标并打包成图集。**不复制 Adobe 图标。**
- **通用控件**：AE 式可拖动数值（按住拖动改数值）、颜色拾取器、属性行（带秒表/关键帧导航）、树、带开关列的列表、时间码输入、下拉菜单、搜索框。
- **快捷键**：按工作区切换的键位表（AE 键位、Ai 键位），可以自定义，有冲突检查；全部绑定到命令 ID。
- **撤销与历史**：撤销/重做菜单；历史面板（显示命令名和作者，包括其他协作者和 Agent）。
- **Agent 工作区的基础面板**：Agent 活动记录、提案列表（接受/拒绝/合并），数据来自 17。
- **国际化**：中文/英文；建立 `docs/glossary.md`，收录 Adobe 官方中文版术语对照。
- **输入法验证**：确认中文输入法（拼音、候选框位置）在 egui 中可用；不达标就写报告并提出替换方案（见架构文档第 1 节）。

## 对外接口

- `Panel` trait、工作区注册、主题 token、通用控件、快捷键绑定 API。

## 验收

- 能拖动、停靠、组合面板，布局保存后重启能恢复。
- 切换工作区时快捷键随之切换。
- 一位 AE 用户看截图能认出各面板的位置和作用。
- 界面帧率 60 fps，空闲时不重绘。

## 不做

- 具体业务面板：时间轴、查看器（12）；工具栏、画板（13）；剪辑（14）。

## 进度

- [x] 外壳与停靠（bae-ui 已接进 `apps/bae`：自绘标题栏、工具栏、工作区栏、停靠树、状态栏；原生菜单项都是命令 id）
- [x] 主题与图标（颜色与尺寸取自本机 AE 2026；图标用 Painter 自绘，不打包图片）
- [x] 通用控件（第一批：热文本、秒表、关键帧导航、展开箭头、勾选框、下拉框、搜索框、开关格、时间码）
- [x] `Panel` trait 与面板 / 工作区 / 键位表注册（PR #23，`docs/spec/ui-shell.md`）
- [x] 快捷键系统（按工作区切换键位表；AE 全局键位；面板作用域；自定义与冲突检查；快捷键编辑器）
- [x] 工作区预设（动效、审阅、Agent）与保存、重置（AE 语义：当前布局自动记住，"保存"后"重置"回到保存的布局）
- [x] 历史面板（命令名 + 作者，含协作者和 Agent；数据来自 Stage 事件）
- [x] Agent 活动记录、提案面板（`bae_ui::agent::AgentFeed`；`apps/bae` 用 bae-agent 的 `AuditLog` 和 `proposal::proposals` 实现）
- [x] 首次启动选择岗位、编辑工作区对话框（排序、显示/隐藏、另存为新工作区、删除自定义工作区）
- [ ] 界面亮度设置（主题色现在是常量，需要把绘制统一经过一个亮度函数）
- [x] 输入法验证报告（模拟测试通过，候选框改为跟随光标；真实拼音端到端待授权后确认，见下文）
- [ ] 原生多窗口（浮动面板目前画在主窗口内）
- [x] 工程文件配套（01 的 `app/project-files` 分支代改，2026-11-21，11 当时没有 worker）：新增 `ids::CLOSE_PROJECT`（关闭项目）；`HistoryLog::clear`（打开另一个工程时清空，序号继续增加，按序号缓存的面板会刷新）；`i18n::has_translation`（测试中文覆盖时不必切换全局语言）；欢迎框改成 `egui::Modal`，加关闭按钮，Esc 和点框外都能关掉，关掉时保持当前工作区。

### 进展记录

- 2026-11-19：建立 Cargo 工作区骨架（`crates/*`）与 `bae-ui` crate；主题 token 取自本机 After Effects 2026 默认亮度下的实测像素值（面板 #1d1d1d、文字 #b1b1b1、强调蓝 #5895ed、焦点框 1pt、面板间距 4pt、标题栏 19pt、工具栏 40pt、图层行 17pt）。界面字体使用系统字体（SF / 苹方），不打包字体。
- 停靠方案：egui_dock 的标签样式无法还原 AE 的标签（下划线激活态、☰ 面板菜单、× 关闭、焦点蓝框、堆叠面板），因此在 `bae-ui` 内实现与 AE 工作区文件同构的 Splitter/Frame 停靠树。如需回到 egui_dock，替换范围只在 `bae-ui::dock`。

- 2026-11-19：完成第一批外壳模块。
  - `bae-ui::dock`：与 AE 工作区同构的 Splitter/Frame 树。拆分条拖动（4pt 黑缝即拖动区）；标签（激活态 1pt 下划线、☰ 面板菜单、×、标签色块、锁）；焦点框 1pt #5794ec；堆叠面板组（折叠标题 24pt，可拖动展开面板的高度）；拖动标签到其他 frame 的中间或四边重新停靠，拖出窗口变成浮动窗口（egui 多视口）；`~` 键最大化面板组；布局可存成 JSON 再恢复（`Layout::to_json` / `from_json`）。
  - `bae-ui::titlebar`：保留系统窗口（圆角、阴影、边缘缩放），隐藏系统标题和红绿灯，自己画 19pt 标题栏和 9pt 红绿灯；关闭、最小化、全屏、拖动窗口、双击缩放都走 `ViewportCommand`。
  - `bae-ui::widgets`：上面列出的控件和工作区切换栏（默认 ☰ / 审阅 / …，☰ 菜单含重置、保存、编辑工作区）。
  - `bae-ui::icons`：约 90 个图标。有 SVG 的图标由 `tools/icons/build_icons.py` 转成压缩的填充路径数据（`assets/icons/icons.bin`），运行时按实际像素光栅化进图集、按主题色着色；其余仍是 16×16 网格的 Painter 绘制（见 `assets/icons/README.md`）。
  - `bae-ui::i18n` 与 `docs/glossary.md`：中文术语都和本机 AE 2026 简体中文界面词典核对过。
  - 示例：`cargo run -p bae-ui --example shell`（`--zh` 中文，`--reset` 恢复默认布局）。
  - 与 AE 截图逐像素对比：五个 frame 的位置误差 ≤1pt（有单元测试）；标题、标签文字、×、色块、锁、☰、下划线的位置误差 ≤1.5pt。AE 用 Adobe Clean 字体，我们用系统 SF，11.5pt 时文字宽度与 AE 相差约 1%。
  - 测试：拆分条拖动、标签拖到 frame 边缘重新停靠都用真实的 egui 指针事件在无窗口环境下模拟。
- 待做（当时）：原生菜单栏（muda）、快捷键系统、`Panel` 注册与工作区预设、历史面板、Agent 面板、输入法验证、界面亮度设置。

- 2026-11-20（接手）：
  - PR #23 `[interface]`：`Panel` / `PanelKind` / `PanelContext`（只读 Stage + `invoke(命令 id)`）、`WorkspacePreset`、`Keymap` / `Binding` / `Chord` / `KeymapOverrides`、`ActionInfo`，都注册到 bae-app 的 `Registry`；`UiModule` 给布局里没人注册的面板补占位。说明见 `docs/spec/ui-shell.md`。
  - `bae_ui::Workbench`：标题栏 + 工具栏 + 工作区栏 + 停靠区 + 状态栏；面板实例按需创建，关闭或切走时保存视图状态；命令 id 解析顺序：焦点面板 → 外壳动作 → `bae-model` 命令。
  - `apps/bae` 改用 Workbench（去掉 egui_dock 占位）：`Document`（Project + CommandRegistry，撤销/重做走 `Stage::undo/redo`）；菜单项 id = 命令 id；根视口命令走 `egui_winit::process_viewport_commands`；窗口用 `titlebar::viewport_builder` 创建；工作区状态存 `<state>/workspaces.json`。`--smoke-test` 和 `--headless --smoke-test` 都通过，headless 冒烟测试会轮流切换三个工作区；`BAE_SMOKE_SCREENSHOT=<path>` 可以把最后一帧存成 BMP。
  - 快捷键：键位表随工作区切换；文本框有焦点时不触发打字键和文本编辑键；Cmd+C/X/V 在非文本状态下变成 `edit.copy/cut/paste`；快捷键编辑器（Cmd+Alt+'）可以录制、删除、全部重置，冲突时先确认再替换。
  - `KeymapBindings`：模块往已有键位表追加快捷键（12 的时间轴键位），不替换整个键位表；插件卸载时一并撤销。
  - 首次启动（没有 `workspaces.json`）弹出"欢迎使用 BAE"选岗位；编辑工作区对话框；窗口 > 工作区 菜单加"另存为新工作区…""编辑工作区…"。
  - 历史面板：`bae_ui::history::HistoryLog`（应用资源）订阅 Stage 事件，显示命令名、作者（你 / 协作者 / Agent xxx）、撤销/重做；向 02 提了 `StageEvent.label` 和列出撤销栈的请求。

### 中文输入法验证报告（2026-11-20）

**结论：暂不提替换方案。** egui 0.36 / egui-winit 0.36 / winit 0.30.13 的输入法路径是完整的，组字、提交、退格、候选框定位在模拟测试里都对；BAE 修正了候选框锚点。**还缺真实拼音输入法上的端到端确认**：本机运行 Agent 的终端没有"辅助功能 / 发送事件"权限（`CGPreflightPostEventAccess()` 为 false），合成按键进不了应用，所以没能自动打出拼音。授权后用 `crates/bae-ui/tools/ime_driver.swift` 可以直接跑（用法见文件头）。

**核对过的代码路径**

- 文本框获得焦点时 egui 输出 `PlatformOutput::ime`，egui-winit 据此调用 `Window::set_ime_allowed(true)` 和 `set_ime_cursor_area`；失焦时关闭。winit 的 `Ime::Preedit/Commit` 转成 egui 的 `ImeEvent::Preedit/Commit`，`TextEdit` 显示带下划线的预编辑文字，提交后替换。
- macOS：winit 的 `firstRectForCharacterRange` 返回 `set_ime_cursor_area` 给的矩形（视图坐标 → 屏幕坐标）。BAE 用 `fullsize_content_view` 隐藏系统标题栏，内容视图覆盖整个窗口，坐标系一致，候选框不会因为自绘标题栏偏移 19pt。
- Windows：winit 在 IME 处理过的按键上仍会发 `KeyboardInput`（winit #4508），egui-winit 0.36 按 `VK_PROCESSKEY` 过滤掉了。
- egui-winit 源码里记录了上游在 macOS 15 简体双拼、Windows 11 微软拼音、fcitx5 上实测的事件序列；模拟测试按 macOS 的序列（空格选词：`Preedit("")` → `Commit("你好")`；退格删光：`Preedit("")`）编写。

**BAE 做的处理**

1. **候选框跟随光标。** egui-winit 把整个文本框的矩形（`IMEOutput::rect`）交给系统，候选框会出现在文本框左下角，不跟光标走；AE 和系统原生文本框都在光标下方。`Workbench` 每帧把它换成光标矩形（`workbench::ime_follows_caret`）。12 号以后在合成查看器里直接编辑文字时，只要用 egui 的 `TextEdit` 或者自己设置 `PlatformOutput::ime`，都会经过这一步。
2. **组字时不触发快捷键。** 文本框有焦点时，`U`、`J`、`K` 这类不带修饰键的快捷键、方向键/退格/回车，以及 Cmd+Z/A/C/V/X 都留给文本框（macOS 上组字时 winit 本来就不发按键事件；这里再保险一次）。
3. 测试：`workbench::tests::ime_composition_in_text_fields`（打开快捷键编辑器 → 搜索框自动聚焦 → 组字 "ni'hao" 期间按 U 不触发快捷键 → 候选框矩形等于光标矩形且在文本框内 → 提交 "你好" → 退格删光预编辑不留字 → 光标后移）。

**需要人工确认的清单**（授权后脚本覆盖前 3 条）

1. 简体拼音：候选框出现在光标正下方（搜索框、以后的图层重命名和文字工具）。
2. 空格 / 数字选词、回车上屏英文、Esc 取消、退格删到空。
3. 组字时按 AE 快捷键字母不触发命令。
4. Shift 切换中英文、全角标点。
5. Retina 与外接 1x 屏、全屏、窗口在副屏时的候选框位置。
6. Windows 11 微软拼音（`VK_PROCESSKEY` 过滤、候选框位置）。
7. 多行文本（文字图层的源文本）：换行后候选框跟到新行。

**如果真实测试不达标**：先在外壳层面修（egui-winit 是小 crate，必要时 fork 或给上游提 PR），其次对文字工具这类重度输入场景用原生文本控件（NSTextView / Win32 EDIT）叠在 egui 上只负责输入；目前没有证据需要替换整个界面框架。

## 请求（其他工作流在这里留言）

- 致 01 core/foundation：本分支临时加入了根 `Cargo.toml`（成员 `crates/*`，因为空通配会报错）和 `rust-toolchain.toml`，以便 UI crate 能单独构建；foundation 合并后以 foundation 的版本为准。

### 14 ui/edit → 11（2026-11-20，Pr 剪辑规划）

依据：`docs/spec/pr/`（Premiere Pro 基础功能清单）和 `docs/spec/pr/edit-model.md`（剪辑数据模型），PR #73。

1. **键位按平台覆盖**：Pr 的一些默认键在 macOS 和 Windows 上修饰键不对应（微移剪辑：macOS `Cmd+←/→`、Windows `Alt+←/→`；外滑：macOS `Opt+Cmd+←/→`、Windows `Ctrl+Alt+←/→`），`Binding` 需要能写"只在某个平台生效"或按平台给不同 `Chord`（`docs/spec/pr/shortcuts.md` 开头）。
2. **共用面板的作用域跟随工作区键位表**：项目面板、效果控件这类两个工作区都用的面板，面板作用域的绑定应该随当前工作区的键位表变化（同一个键在剪辑和动效里含义不同，PR-KEY-014）。
3. **时间码控件**：支持相对输入（`+10`、`-1.00`）、时间轴获得焦点时直接敲数字进入跳转、显示格式（丢帧 / 非丢帧时间码、帧、英尺+帧）和起始时间码偏移（PR-MON-003）。
4. **术语表**：请把 `docs/spec/pr/README.md` 第 8 节的剪辑术语（素材箱、序列、源监视器、节目监视器、外滑、内滑、提升、提取……）合并进 `docs/glossary.md`；中文已按 Pr 2026 自带的简体中文词典核对。
5. （以后）工具键的"按住临时切换、松开恢复"（弹簧键）。

### 12 ui/motion → 11（2026-11-20）

1. **模块级全局动作处理。** 动效工作区的快捷键（空格播放、J/K、F9、U/P/S/R/T/A 等，`motion.*`）现在只能经过获得焦点的面板的 `on_action` 处理；焦点在历史记录、Agent 面板时这些键不生效，而 AE 里它们在任何面板都有效。希望 `bae-ui` 提供一个注册表（例如 `Registry<dyn ActionHandler>`，`fn handle(&self, inv, cx: &mut PanelContext) -> bool`），在"焦点面板"之后、"外壳动作"之前调用。
2. **工具栏工具。** `Workbench` 的工具选择现在是内部状态；动效工作区需要读取当前工具（选取、手形、缩放、旋转、锚点、形状、钢笔、文字），并在选中工具时接收 `V`、`H`、`Z`、`W`、`Y`、`Q`、`G`、`Cmd+T` 等快捷键。希望把当前工具放进应用资源（或提供注册 / 查询接口）。
3. 术语：12 新增的界面文字（`crates/bae-ui-motion/src/i18n.rs`）可以并入 `docs/glossary.md`，12 不直接改术语表。
