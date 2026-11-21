# After Effects macOS 菜单栏结构与 Windows 差异对照

> 来源：Adobe After Effects 26.2.1（build `26.2.1.2`，实机运行语言 `en_US`）| 平台：macOS 26.7.3 (25G342, arm64) | 提取日期：2026-11-21
> 对照基准：Adobe After Effects 26.0x67 (`zh_CN`) | 平台：Windows 11 Pro (10.0.26200, x64)

完整脱敏后的 macOS 菜单树见 [`menus.json`](menus.json)（共 **10** 个顶层应用菜单、**87** 个子菜单、**886** 个叶子菜单项）。

## 1. 提取与验证方法

1. **实机 `AXMenuBar` 全量读取（已成功）**：通过 `tools/ae/reference/dump_mac_menus.swift` 调用 macOS `AXUIElement` 只读遍历运行中 AE 26.2.1 的主菜单栏（含所有子菜单、系统注入项及实时生效的快捷键修饰符与字形）。
2. **ExtendScript `app.findMenuCommandId()` 逐项探测**：通过 `tools/ae/aectl` 对所有菜单项标题查询 AE 内部命令 ID（`aeCommandId`）。注意：Windows 版 `../menus.json` 的 `commandId` 来自 Win32 `GetMenuItemID`（`HMENU` 句柄编码，如 `131074`），而 macOS `menus.json` 的 `aeCommandId` 是 AE 跨平台内部命令 ID（如 `New Project = 2`、`New Composition... = 2000`、`Reveal in Finder = 2562`）；由 Cocoa 原生处理的菜单项（`About`、`Hide`、`Quit`、`AutoFill`）在 `findMenuCommandId()` 中返回 `0`。
3. **应用包资源核对**：读取 `Contents/Resources/After Effects.nib` 与 `Contents/Dictionaries/zh_CN/after_effects_zh_CN.dat`，确认官方简体中文译名与 `{{*MSWindows*}}` 平台分叉文案。

## 2. 顶层菜单栏对比

- **macOS（10 个应用菜单，不含左上角苹果图标菜单）**：`After Effects` · `File` · `Edit` · `Composition` · `Layer` · `Effect` · `Animation` · `View` · `Window` · `Help`
- **Windows（9 个菜单）**：`文件(F)` · `编辑(E)` · `合成(C)` · `图层(L)` · `效果(T)` · `动画(A)` · `视图(V)` · `窗口` · `帮助(H)`
- **关键差异**：macOS 首位新增 `After Effects` 应用菜单，且所有菜单与子菜单标题均**不带** Windows 的 `Alt` 助记符后缀（如 `(F)`、`(E)`、`(C)`）。

## 3. macOS 独占 `After Effects` 应用菜单

| 英文标签 | 官方中文标签（对照） | 默认快捷键 | 关闭「使用系统快捷键」后 | `aeCommandId` | Windows 对应位置 |
|---|---|---|---|---:|---|
| `About After Effects...` | 关于 After Effects... | *(无)* | — | 0 | `帮助(H) > 关于 After Effects...` |
| `Settings…` | 设置…（NIB 原文为 Preferences，macOS 13+ 系统自动改名为 Settings…/设置…；对话框窗口标题仍为“首选项”） | *(无)* | — | *(子菜单)* | `编辑(E) > 首选项(F)` |
| `Services` | 服务 | *(无)* | — | *(子菜单)* | *(macOS 独有)* |
| `Hide After Effects` | 隐藏 After Effects | `Cmd+H` | `Ctrl+Cmd+H` | 0 | *(macOS 独有)* |
| `Hide Others` | 隐藏其他 | `Option+Cmd+H` | `Ctrl+Option+Cmd+H` | 0 | *(macOS 独有)* |
| `Show All` | 全部显示 | *(无)* | — | 0 | *(macOS 独有)* |
| `Quit After Effects` | 退出 After Effects | `Cmd+Q` | — | 0 | `文件(F) > 退出(X)` |

## 4. 首选项 / 设置（Settings…）在 Mac 上的位置、命名与 16 个页面

- **菜单位置**：位于 **`After Effects ▸ Settings…`**（中文 macOS 13+ 下为 **`After Effects ▸ 设置…`**），而非 Windows 的 `编辑(E) ▸ 首选项(F)`。
- **命名细节**：`After Effects.nib` 内部原始标题为 `Preferences`，在 macOS 13（Ventura）及更高版本（本机为 macOS 26.7.3）上系统 AppKit 自动将其显示为 **`Settings…` / `设置…`**；但打开后的对话框窗口标题在 AE 词典（`$$$/AE/Dialog/PrefsDialog`）中仍为「首选项」/ *Preferences*。
- **快捷键**：仍沿用 AE 传统的 **`Option+Cmd+;`**（绑定在第一页 `Startup & Repair...` 上），**不是** macOS 常见应用的 `Cmd+,`。
- **页面清单（共 16 页，按分隔线分 5 组，顺序与 Windows 完全一致）**：

| 分组 | 英文页面名 | 中文页面名（对照） | 快捷键 | `aeCommandId` |
|---:|---|---|---|---:|
| 1 | `Startup & Repair...` | 启动和修复... | `Option+Cmd+;` | 3201 |
| 1 | `Project...` | 项目... | *(无)* | 3202 |
| 1 | `Composition...` | 合成... | *(无)* | 3203 |
| 1 | `Previews...` | 预览... | *(无)* | 3204 |
| 2 | `Appearance...` | 外观... | *(无)* | 3205 |
| 2 | `Grids & Guides...` | 网格和参考线... | *(无)* | 3206 |
| 2 | `Labels...` | 标签... | *(无)* | 3207 |
| 2 | `Type...` | 类型... | *(无)* | 3208 |
| 3 | `Import...` | 导入... | *(无)* | 3209 |
| 3 | `Export...` | 导出... | *(无)* | 3210 |
| 4 | `Audio...` | 音频... | *(无)* | 3211 |
| 4 | `Disk...` | 磁盘... | *(无)* | 3212 |
| 4 | `Memory & CPU...` | 内存和 CPU... | *(无)* | 3213 |
| 4 | `Video...` | 视频... | *(无)* | 3214 |
| 5 | `3D...` | 3D… | *(无)* | 3215 |
| 5 | `Scripting & Expressions...` | 脚本和表达式... | *(无)* | 3216 |

## 5. 相对 Windows `menus.json` 的全部结构与文案差异

### 5.1 移入 `After Effects` 应用菜单（原位置移除）

| Windows 位置 | macOS 处理方式 | macOS 实际位置 |
|---|---|---|
| `文件(F) > 退出(X)` | 移至应用菜单 | `After Effects > Quit After Effects` |
| `编辑(E) > 首选项(F)` | 移至应用菜单并改名 | `After Effects > Settings…` |
| `帮助(H) > 关于 After Effects...` | 移至应用菜单 | `After Effects > About After Effects...` |

### 5.2 平台文案差异（Finder vs 资源管理器）

AE 简体中文词典（`$$$/AE/MenuID/Various/RevealInFileSystem`）使用 `{{*MSWindows*}}` 分叉，macOS 简体中文界面实际显示为 **「在 Finder 中显示」**（保留英文 `Finder`，未译为“访达”）：

| macOS 菜单路径 | macOS 中文标签 | Windows 菜单路径 | `aeCommandId` |
|---|---|---|---:|
| `File > Reveal in Finder` | 在 Finder 中显示 | `文件(F) > 在资源管理器中显示` | 2562 |
| `Layer > Reveal in Finder` | 在 Finder 中显示 | `图层(L) > 在资源管理器中显示` | 2562 |
| `Layer > Reveal > Reveal in Finder` | 在 Finder 中显示 | `图层(L) > 显示 > 在资源管理器中显示` | 2562 |

### 5.3 macOS 系统自动注入的菜单项 & `Window` 菜单说明

| macOS 菜单路径 | 快捷键 | `aeCommandId` | 来源说明 |
|---|---|---:|---|
| `Edit > AutoFill > (Contact… / Passwords… / Credit Card…)` | *(无)* | 0 | macOS AppKit 自动向 Edit 菜单注入的自动填充子菜单 |
| `Edit > Start Dictation…` | `Fn+D` | -196648 | macOS AppKit 自动向 Edit 菜单注入的听写项 |
| `View > Enter Full Screen` | `Fn+F` | -589862 | macOS AppKit 自动向 View 菜单注入的系统全屏切换 |

- **关于 `Window`（窗口）菜单**：与普通 macOS 文档应用不同，AE 的 `Window` 菜单完全用于工作区、扩展、面板、查看器和 ScriptUI 面板，**不包含**标准 Cocoa 的 `Minimize`（最小化）、`Zoom`（缩放）或 `Bring All to Front`（前置全部窗口）菜单项；但 `Cmd+M`（最小化）在「使用系统快捷键」开启时仍会被系统/AE 拦截，因此 `Add to Render Queue` 被动态改绑为 `Ctrl+Cmd+M`。

### 5.4 菜单栏实时快捷键提示差异（重点）

即使 `aeks` 配置文件写的是 `Cmd+H` / `Cmd+M` / `Option+Cmd+H`，当 macOS 独占偏好 `Pref_USE_STD_SYSTEM_SHORTCUTS_v2` 为开启（默认）时，AE 会在菜单栏中**动态将冲突命令加上 `Control`（`⌃`）**；另外若干带小键盘的命令在 Mac 菜单栏优先展示无小键盘替代键：

| 菜单路径 | macOS 实机菜单显示 | Windows 菜单显示 | 原因 |
|---|---|---|---|
| `File > Replace Footage > File...` | `Ctrl+Cmd+H` | `Ctrl+H` | `Pref_USE_STD_SYSTEM_SHORTCUTS_v2=true` 时 `Cmd+H` 让位给「隐藏 After Effects」，动态改绑为 `Ctrl+Cmd+H` |
| `Composition > Add to Render Queue` | `Ctrl+Cmd+M` | `Ctrl+M` | `Pref_USE_STD_SYSTEM_SHORTCUTS_v2=true` 时 `Cmd+M` 让位给 macOS 最小化，动态改绑为 `Ctrl+Cmd+M` |
| `Animation > Toggle Hold Keyframe` | `Ctrl+Option+Cmd+H` | `Ctrl+Alt+H` | `Pref_USE_STD_SYSTEM_SHORTCUTS_v2=true` 时 `Option+Cmd+H` 让位给「隐藏其他」，动态改绑为 `Ctrl+Option+Cmd+H` |
| `Edit > Purge > All Memory` | `Ctrl+FwdDel` | `Ctrl+Alt+Numpad /` | 菜单优先显示 `macControl+PadClear`（`⌃⌦`），而非小键盘 `/` |
| `Composition > Save Current Preview...` | `Ctrl+Shift+C` | `Ctrl+数字小键盘 0` | 菜单优先显示 MacBook 无小键盘绑定 `macControl+Shift+C`，而非小键盘 `0` |
| `Layer > Markers > Add Marker` | `Ctrl+8` | `Numpad *` | 菜单优先显示 MacBook 无小键盘绑定 `macControl+8`，而非小键盘 `*` |
| `View > Use Display Color Management` | `Ctrl+Option+/` | `Shift+Numpad /` | 菜单优先显示 MacBook 无小键盘绑定 `macControl+Option+/`，而非 `Shift+Numpad /` |
| `After Effects > Settings… > Startup & Repair...` | `Option+Cmd+;` | `Ctrl+Alt+;` | AE 保留传统 `Option+Cmd+;`，**不是** macOS 常规的 `Cmd+,` |

### 5.5 版本与安装环境差异（AE 26.2.1 vs Win 26.0x67，非平台差异）

| 菜单路径 | 快捷键 | `aeCommandId` | 说明 |
|---|---|---:|---|
| `Edit > Quick Apply...` | `Cmd+Return` | 10809 | AE 26.2 新增命令（中文「快速应用...」），Windows 26.0x67 快照尚未包含 |
| `Edit > Paste Reversed Keyframes` | `Shift+Cmd+V` | 54 | 26.0 已有此菜单项但无快捷键，26.2 新增默认快捷键 `Shift+Cmd+V` |
| `File > New > New Team Project... / File > Open Team Project... / Edit > Team Project (6 items)` | *(无)* | 10428 | Creative Cloud 团队项目启用时显示 |
| `Help > Adobe Crash Reporter... / Manage My Account... / Sign Out... / Updates...` | *(无)* | 4009 | 实机 Mac 26.2.1 帮助菜单底部包含账号与更新项；Windows 26.0x67 快照未包含 |

> 备注：未打开菜单弹窗时，Cocoa 延迟填充的 `Edit ▸ Label` 16 个颜色槽在 `AXMenuBar` 中显示为占位 `Unused`（对应 `aeCommandId=2351`），展开后按偏好填充 16 种标签色（与 Windows `menus.json` 相同）。内置 22 个 `Effect` 分类子菜单与 Windows 数量完全一致（本机 Mac 仅含内置 `Mocha AE`，未装第三方插件）。
