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
- [ ] Agent 活动记录、提案面板（等 17 的 PR #20 合并后接 `AuditLog` 和 `proposal::proposals`）
- [ ] 首次启动选择岗位、编辑工作区对话框、界面亮度设置
- [ ] 输入法验证报告
- [ ] 原生多窗口（浮动面板目前画在主窗口内）

### 进展记录

- 2026-11-19：建立 Cargo 工作区骨架（`crates/*`）与 `bae-ui` crate；主题 token 取自本机 After Effects 2026 默认亮度下的实测像素值（面板 #1d1d1d、文字 #b1b1b1、强调蓝 #5895ed、焦点框 1pt、面板间距 4pt、标题栏 19pt、工具栏 40pt、图层行 17pt）。界面字体使用系统字体（SF / 苹方），不打包字体。
- 停靠方案：egui_dock 的标签样式无法还原 AE 的标签（下划线激活态、☰ 面板菜单、× 关闭、焦点蓝框、堆叠面板），因此在 `bae-ui` 内实现与 AE 工作区文件同构的 Splitter/Frame 停靠树。如需回到 egui_dock，替换范围只在 `bae-ui::dock`。

- 2026-11-19：完成第一批外壳模块。
  - `bae-ui::dock`：与 AE 工作区同构的 Splitter/Frame 树。拆分条拖动（4pt 黑缝即拖动区）；标签（激活态 1pt 下划线、☰ 面板菜单、×、标签色块、锁）；焦点框 1pt #5794ec；堆叠面板组（折叠标题 24pt，可拖动展开面板的高度）；拖动标签到其他 frame 的中间或四边重新停靠，拖出窗口变成浮动窗口（egui 多视口）；`~` 键最大化面板组；布局可存成 JSON 再恢复（`Layout::to_json` / `from_json`）。
  - `bae-ui::titlebar`：保留系统窗口（圆角、阴影、边缘缩放），隐藏系统标题和红绿灯，自己画 19pt 标题栏和 9pt 红绿灯；关闭、最小化、全屏、拖动窗口、双击缩放都走 `ViewportCommand`。
  - `bae-ui::widgets`：上面列出的控件和工作区切换栏（默认 ☰ / 审阅 / …，☰ 菜单含重置、保存、编辑工作区）。
  - `bae-ui::icons`：约 90 个图标，16×16 网格矢量绘制。
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
  - 历史面板：`bae_ui::history::HistoryLog`（应用资源）订阅 Stage 事件，显示命令名、作者（你 / 协作者 / Agent xxx）、撤销/重做；向 02 提了 `StageEvent.label` 和列出撤销栈的请求。

## 请求（其他工作流在这里留言）

- 致 01 core/foundation：本分支临时加入了根 `Cargo.toml`（成员 `crates/*`，因为空通配会报错）和 `rust-toolchain.toml`，以便 UI crate 能单独构建；foundation 合并后以 foundation 的版本为准。
