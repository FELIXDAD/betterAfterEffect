# Windows 实机：BAE 0.0.1 独立验收第二轮

## 环境信息

- **日期**：2026-11-21
- **分支 / 基线**：`origin/dev` @ **`fb08040ff6b0019763d1f941f87427af355d4af8`**（`fb08040`）
- **操作系统**：Microsoft Windows 11 专业版 10.0.26200（Build 26200，中文 zh-CN，`OSLanguage=2052`）
- **GPU**：NVIDIA GeForce RTX 5090 D v2（驱动 `32.0.16.1692`，D3D12）
- **显示 / DPI**：3840×2160，系统缩放 **150%**（`AppliedDPI=144`，窗口渲染 `@1.50x`）
- **构建**：`cargo build --release -p bae`（默认设置，未设 `WGPU_DX12_COMPILER`），`target/release/bae.exe` = **30,635,008 bytes（29.22 MB）**
- **输入法**：微软拼音（`0x8040804`）
- **操作与锁**：全程持有 `E:\bae-test\locks\gui.lock`（每 20 分钟刷新，测毕释放）；GUI 启动与操作经由 `orca computer`（`launch-app` / `get-app-state` / `press-key` / `hotkey` / `click` / `type-text`，`--restore-window`、`--screenshot-scale 1`）。

---

## 总表（A–H）

| 项 | 验收范围 | 结论 | 关联 Issue | 摘要 |
|---|---|---|---|---|
| **A** | 启动和欢迎框 | **通过** | — | 冷启动 0.94 s 弹出欢迎框；Esc、关闭按钮（×）、点框外均可关闭；Motion / Review / Agent 三个入口按钮均正确切换工作区；正常关闭后热启动 0.82 s 不再弹出欢迎框 |
| **B** | 新建合成、图层 ▸ 新建、菜单与快捷键 | **失败** | [#117](https://github.com/FELIXDAD/betterAfterEffect/issues/117), [#118](https://github.com/FELIXDAD/betterAfterEffect/issues/118) | 新建合成与 `Layer ▸ New` 全部 9 项图层创建及 `export-json` 均通过；无合成/有合成/有图层菜单置灰逻辑正确；`Edit ▸ Undo/Redo` 等 Shell 自有菜单项缺失快捷键显示（#117）；`Window` 菜单缺少 `Render Queue` 入口（#118）；`Ctrl+Shift+S` 在本机被系统全局热键占用（1409），归类为环境/自动化受限 |
| **C** | 时间轴改名（IME / 空格 / Emoji / 超长名） | 进行中 | — | — |
| **D** | 关键帧、播放、焦点与撤销重做 | 待测 | — | — |
| **E** | 保存和打开（边界与异常） | 待测 | — | — |
| **F** | 渲染队列（格式 / 停止 / 错误 / 满盘 / 多合成） | 待测 | — | — |
| **G** | 语言（`BAE_LANG=zh` / `en`） | 待测 | — | — |
| **H** | 150% DPI 清晰度与布局 | 待测 | — | — |

---

## A. 启动和欢迎框

**结论：通过**

测试前清空 `%LOCALAPPDATA%\BAE\State\` 下的 `workspaces.json` 与 `window.json` 以复现真实冷启动。

| 检查项 | 操作与验证方式 | 结果 | 实测现象与证据 |
|---|---|---|---|
| 冷启动 | 删除 `workspaces.json` / `window.json` 后 `orca computer launch-app bae.exe --activate` | **通过** | 0.94 s 出现主窗口 `BAE - Untitled Project`，弹出居中模态框 **Welcome to BAE**，状态栏显示 `NVIDIA GeForce RTX 5090 D v2 (D3D12, discrete) \| frame 7.68 ms \| 2160×1320 @1.50x`（见图 1） |
| 欢迎框 — Esc 关闭 | 欢迎框打开时按 `Escape` | **通过** | 模态框立即消失，停留在默认 `Motion` 工作区，界面可交互 |
| 热启动不再弹出 | 通过窗口关闭按钮正常退出，确认 `%LOCALAPPDATA%\BAE\State\workspaces.json` 已生成，再次启动 | **通过** | 0.82 s 打开主窗口，不再弹出欢迎框 |
| 欢迎框 — 关闭按钮（×） | 冷启动后 `Tab` ×1 聚焦右上角 `×` 按钮，按 `Enter` | **通过** | 模态框关闭，保留 `Motion` 工作区 |
| 欢迎框 — 点框外 | 冷启动后点击模态框外区域（窗口坐标 `150, 260`） | **通过** | 模态框关闭，背景区域（项目面板）正常响应焦点 |
| 三个入口按钮 — Motion | 冷启动后 `Tab` ×2 + `Enter` 选择 `Motion` | **通过** | 模态框关闭，`Motion` 工作区高亮（含 Project / Effect Controls、Composition、Timeline、Preview、Properties / Align / Audio / Effects & Presets） |
| 三个入口按钮 — Review | 冷启动后 `Tab` ×3 + `Enter` 选择 `Review` | **通过** | 模态框关闭，切换至 `Review` 工作区（大画幅 Composition、Timeline、Preview、History / Info） |
| 三个入口按钮 — Agent | 冷启动后 `Tab` ×4 + `Enter` 选择 `Agent` | **通过** | 模态框关闭，切换至 `Agent` 工作区（Composition、Timeline、Agent Activity、Proposals / History） |
| 未开放项置灰 | 观察 `Storyboard · coming later` 与 `Editing · coming later` | **通过** | 呈置灰不可点击状态，`Tab` 焦点自动跳过 |

![图 1：冷启动与 Welcome to BAE 欢迎框](./acceptance-round2-shots/01-a-welcome-modal.jpg)

---

## B. 新建合成、图层 ▸ 新建、菜单状态与快捷键

**结论：失败（bug：[#117](https://github.com/FELIXDAD/betterAfterEffect/issues/117)、[#118](https://github.com/FELIXDAD/betterAfterEffect/issues/118)）**

通过用户桌面 Win32 `GetMenuItemInfoW` 枚举全部菜单文本、快捷键字符串与启用状态，并在界面中逐一执行新建合成、新建 9 种图层及主要快捷键，最后保存至 `E:\bae-test\acc2\b_9layers.bae` 并用 `bae.exe export-json` 核验数据结构。

| 检查项 | 结果 | 实测现象与证据 |
|---|---|---|
| 新建合成（`Ctrl+N`） | **通过** | 创建 `Comp 1`（1920×1080，30.00 fps），标题变为 `BAE - Untitled Project *`，时间轴与查看器同步打开 |
| `Layer ▸ New` 全部 9 项 | **通过** | 经菜单逐一创建 `Text`、`Solid`、`Light`、`Camera`、`Null Object`、`Shape Layer`、`Adjustment Layer`、`Rectangle`、`Ellipse`，均无报错（见图 2）；单独验证 `Ellipse` 正确渲染为红色圆形（360×360）；`bae.exe export-json` 确认 9 层 `kind`（`text`/`solid`/`light`/`camera`/`null`/`shape`×3/`adjustment`）与 `solids` 资产完整落盘 |
| 无合成时 `Layer ▸ New` 置灰 | **通过** | `GetMenuItemInfoW` 实测无合成时 `Layer ▸ New` 下全部 9 项均为 `OFF`（`MFS_DISABLED`）；新建合成后全部变为 `ON` |
| 合成 / 动画 / 编辑 / 排列菜单可用状态 | **通过** | 无合成：`Trim Comp to Work Area`、`Add to Render Queue`、`Undo`/`Redo`、`Duplicate`/`Split Layer`、`Arrange` 4 项、`Animation` 5 项全 `OFF`；有合成无选中图层：`Composition` 3 项与 `Undo New Composition` 变为 `ON`，`Arrange`/`Duplicate`/`Split Layer`/`Animation` 仍 `OFF`；选中图层后：`Arrange` 4 项与 `Duplicate`/`Split Layer` 变为 `ON`，`Animation` 5 项因无选中关键帧仍 `OFF`（选中关键帧后的启用态将在 D 项一并复核） |
| 主要快捷键功能验证 | **通过**（`Ctrl+Shift+S` 自动化受限） | `Ctrl+N`（新建合成）、`Ctrl+Y`（新建纯色）、`Ctrl+D`（复制图层）、`Ctrl+Z`（撤销）、`Ctrl+Shift+Z`（重做）、`Ctrl+M`（加入渲染队列）、`Ctrl+S`（弹出 `Save As` 对话框并落盘）、`Ctrl+O`（弹出 `Open Project` 对话框，取消后 `IsWindowEnabled=True`）均生效；**`Ctrl+Shift+S` 自动化/环境受限**：经用户桌面 `RegisterHotKey(MOD_CONTROL\|MOD_SHIFT, 'S')` 确认返回 `ERROR_HOTKEY_ALREADY_REGISTERED (1409)`，被本机系统级热键拦截（`Ctrl+Shift+Z`/`Ctrl+Shift+D`/`Ctrl+S` 均空闲），非 BAE 缺陷 |
| 菜单显示快捷键与键位表一致性 | **失败（#117）** | 模块贡献项与 File 菜单项（`Ctrl+Alt+N` / `Ctrl+O` / `Ctrl+S` / `Ctrl+Shift+S` / `Ctrl+N` / `Ctrl+M` / `Ctrl+Y` / `Ctrl+D` / `Ctrl+Shift+D` / `F9` 等）显示一致；但 **`Edit ▸ Undo` 与 `Edit ▸ Redo` 未显示 `Ctrl+Z` / `Ctrl+Shift+Z`**（同属 Shell 硬编码的 `Keyboard Shortcuts`、`Exit` 及 `Window` 面板项也均缺失快捷键显示），已两次复现并提 [#117](https://github.com/FELIXDAD/betterAfterEffect/issues/117) |
| Window 菜单面板完整性 | **失败（#118）** | `Window` 菜单缺少 **`Render Queue`（渲染队列）** 入口（与 `render_queue.rs` 注释及 `Ctrl+Alt+0` 不一致），已提 [#118](https://github.com/FELIXDAD/betterAfterEffect/issues/118) |

![图 2：通过 Layer ▸ New 创建全部 9 种图层后的主界面](./acceptance-round2-shots/02-b-all-layers.jpg)
