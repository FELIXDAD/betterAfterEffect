# After Effects 实机界面与功能参考数据（Windows）

> 来源：Adobe After Effects 26.0x67 (zh_CN) | 平台：Windows 11 Pro (10.0.26200, x64) | 提取日期：2026-11-21

本目录存放从 Windows 本机真实安装的 Adobe After Effects 26.0（构建号 `26.0x67`，简体中文界面）通过脚本与只读系统接口提取的**事实与结构参考数据**，用于 BAE 在数据模型、图层属性、内置效果、渲染输出、菜单/面板、工具与快捷键方面做对照查漏。

## 使用原则与合规边界

> **重要说明**：本目录中的显示标签来自 After Effects 26.0x67 的 `zh_CN` 界面，仅用于对照查漏；对照时请以 `matchName`、菜单 `commandId`、快捷键命令标识等英文内部标识为稳定键；BAE 自己的界面文字（中英文）必须按 [`docs/glossary.md`](../../glossary.md) 独立撰写，**不能**从这份数据直接复制。

- **仅含事实和结构数据**：名称、层级、`matchName`、属性类型、默认值、最小/最大值、单位、下拉选项（`Property.propertyParameters`）、是否可关键帧/表达式、默认快捷键、菜单树与 Win32 `commandId`。
- **不含版权原文与报错原文**：不包含任何 Adobe 说明文字、帮助文本、工具提示或报错原文；不可应用/跳过的条目统一使用本仓库短标记（如 `{"unavailable": "modal_file_dialog_on_apply"}`、`{"unavailable": "requires_c4d_footage"}`、`{"unavailable": "cannot_add"}`）。不含截图、图标、预设原文件或工程文件。
- **稳定英文键覆盖率**：
  - 效果与效果参数：100% 带 `matchName`（749 条目录、439 个展开效果、5,637 个叶子参数）。
  - 图层与属性树：100% 带 `matchName`（9 类图层、1,758 个变体叶子属性）。
  - 菜单命令（`menus.json`）：863 / 863 个真实菜单命令均带 Win32 `commandId`（覆盖率 **100%**；含 4 个 MRU/扩展脱敏占位项共 867 项，占位项显式标为 `"commandId": null`，总体 863 / 867 = **99.5%**）。
  - 快捷键与工具（`shortcuts.json`）：100% 以 AE 英文命令标识符为键（32 个上下文、669 条命令、27 项工具）。
- **已脱敏与分离第三方**：移除了本机路径、用户名、GUID、设备信息、最近打开/导入记录（MRU）、第三方 CEP 扩展与第三方 ScriptUI 脚本；本机安装的非 Adobe 第三方插件仅在 `effects/third-party.json` 与 `effects.md` 中单独标注目录，未展开参数。

## 文件清单

| 路径 | 内容 | 规模 / 条目数 |
|---|---|---|
| [`meta.json`](meta.json) | AE 版本/构建号、语言、Windows 版本、提取方式、3D 渲染器列表、全量统计计数 | 1 份元数据 |
| [`effects.md`](effects.md) | 效果目录总表（按分类列出 Adobe 内置/随附效果、参数数、跳过项与本机第三方插件区分表） | 749 条（自带+随附 445，第三方 304） |
| [`effects/`](effects/) | 按分类拆分的 24 份紧凑 JSON 参数树 + `third-party.json` 目录表 | 成功展开 439 个自带/随附效果，共 5,637 个叶子参数（含 940 个下拉选项表）；跳过 6 个 |
| [`layers.md`](layers.md) | 全部图层类型的顶层属性组汇总与形状/文字动画器已展开清单 | 9 类图层 |
| [`layers/`](layers/) | 按图层类型拆分的 9 份完整属性树 JSON：`solid`（含 2D、3D+遮罩及 3 种 3D 渲染器几何/材质选项）、`text`（含动画器/3 种选择器/全部动画属性）、`shape`（含组/4 种路径/4 种描边填充+虚线/10 种操作器）、`camera`（双节点/单节点）、`light`（平行/聚光/点光/环境/Environment）、`null`、`adjustment`、`footage`（图像+音频）、`precomp`（含时间重映射与音频） | 9 份 JSON，共 1,758 个变体叶子属性 |
| [`render-output.json`](render-output.json) | 渲染设置与输出模块模板列表、`STRING_SETTABLE` 可设置键与取值范围、当前默认值 | 渲染设置 6 模板 / 17 键；输出模块 18 模板 / 11 键 |
| [`shortcuts.json`](shortcuts.json) / [`shortcuts.md`](shortcuts.md) | 默认键盘快捷键（不含用户自定义覆盖），按上下文分组 | 32 个上下文，669 条命令 |
| [`menus.json`](menus.json) / [`menus.md`](menus.md) | 主窗口完整菜单树（含快捷键提示与 Win32 Command ID） | 9 个顶层菜单，94 个子菜单，867 个菜单项 |
| [`panels-tools.md`](panels-tools.md) | 工具列表（来自 `CEggAppTool`）、面板列表（来自「窗口」菜单）与内置工作区 | 27 项工具/Gizmo、8 个核心面板、25 个功能面板、2 个内置 ScriptUI 面板、16 个工作区 |
| [`preferences.json`](preferences.json) | 脱敏后的 AE 首选项分区、键名与默认值/占位类型 | 128 个分区，975 个键 |

## 提取方法与缺失项说明

1. **元数据、效果目录/参数、图层属性树、渲染/输出设置**：通过 `AfterFX.exe -r tools/ae/reference/extract_ae_data.jsx` 在临时合成中创建各类图层与效果，调用 ExtendScript DOM 递归导出。AE 26.0 的 `Property.propertyParameters` 可直接读取效果下拉菜单的选项列表，本次共提取 940 个下拉参数选项。
2. **主菜单树**：在 AE 打开临时合成时，由 `tools/ae/reference/extract_ae_menus.ps1` 通过 Windows `user32.dll`（`GetMenu` / `GetMenuItemCount` / `GetMenuString` / `GetSubMenu` / `GetMenuItemID`）只读递归读取主窗口菜单句柄，无需点击菜单或抢占前台窗口。
3. **默认快捷键与首选项**：由 `tools/ae/reference/parse_ae_configs.py` 直接离线解析 `%APPDATA%\Adobe\After Effects\26.0\aeks\` 中的默认快捷键文件及首选项文本文件，并执行路径/个人标识/第三方分区脱敏。
4. **跳过的 6 个自带/随附效果**（无法在无交互纯色层上自动应用）：
   - `CINEMA 4D Effect`（Cineware）：需要 `.c4d` 素材图层，加到纯色层会中止脚本。
   - `ADBE Apply Color LUT2` / `ADBE Apply Color LUT`：应用时立即弹出原生文件选择对话框。
   - `ADBE Basic Text2` / `ADBE Numbers2` / `ADBE Path Text`：应用时立即弹出字体/文字设置模态框。

## 如何复现

所有提取与构建脚本均位于仓库的 [`tools/ae/reference/`](../../../tools/ae/reference/)（不在 `docs/reference/` 下，且不改动 `tools/ae/aectl`）：

- `tools/ae/reference/extract_ae_data.jsx`：ExtendScript 主提取脚本（支持 `BAE_AE_ONLY_EFFECTS`、`BAE_AE_START_INDEX`、`BAE_AE_SKIP_EFFECTS` 环境变量，便于按需断点续跑）。
- `tools/ae/reference/extract_ae_menus.ps1`：只读 Win32 `GetMenu` 菜单树提取脚本（附只读 UIA 兜底）。
- `tools/ae/reference/quit_ae.jsx`：不保存关闭临时工程并干净退出 AE。
- `tools/ae/reference/run_ae_extraction.ps1`：带共享资源锁（`E:\bae-test\locks\ae.lock`）、Windows 注册表崩溃标志清理（`HKCU:\Software\Adobe\After Effects\AppStates`）与卡死检测的自动运行器。
- `tools/ae/reference/parse_ae_configs.py`：离线解析并脱敏默认快捷键与首选项。
- `tools/ae/reference/build_reference_docs.py`：汇总原始提取结果，生成本目录下的全部 JSON/Markdown 文件并校验体积预算（单文件 < 1 MB，总计 < 5 MB）。

复现命令示例（Windows PowerShell，临时文件写到仓库外 `E:\bae-test\ref-ae\`）：

```powershell
# 1. 离线解析默认快捷键与首选项
python tools/ae/reference/parse_ae_configs.py

# 2. 启动 AE 提取元数据、图层、渲染设置、菜单与效果参数（需先在 AE 首选项开启允许脚本写文件）
powershell -ExecutionPolicy Bypass -File tools/ae/reference/run_ae_extraction.ps1 `
  -SkipEffects "CINEMA 4D Effect,ADBE Apply Color LUT2,ADBE Apply Color LUT,ADBE Basic Text2,ADBE Numbers2,ADBE Path Text"

# 3. 转换为 docs/reference/ae/ 下的结构化参考文件
python tools/ae/reference/build_reference_docs.py
```

### Windows 上无人值守调用 AE 的关键点

- **防止 `-r` 启动卡在安全模式模态框**：若上次 AE 被强杀，Windows 注册表会留下 `HKCU:\Software\Adobe\After Effects\26.0`（`ForceQuitOccured` / `AbortOccured` / `CrashOccurred`）以及 `HKCU:\Software\Adobe\After Effects\AppStates\*` 会话子键，导致下次启动弹出 `#32770` 安全模式框并阻塞 `-r`。启动前（以及异常强杀后）必须同时清理这两处注册表项（见 `run_ae_extraction.ps1` 中的 `Clear-AeCrashFlags`）。
- **脚本内弹框抑制与安全退出**：脚本入口调用 `app.beginSuppressDialogs()`，结束时在 `finally` 调用 `app.endSuppressDialogs(false)`；用完后通过 `quit_ae.jsx` 先执行 `app.project.close(CloseOptions.DO_NOT_SAVE_CHANGES)` 再 `app.quit()` 正常退出，并及时释放 `ae.lock`。

## 如何用于 BAE 对照查漏

建议按模块将本目录数据与 [`docs/spec/ae/`](../../spec/ae/) 及对应 crate 逐项比对：

| BAE 规格 / 模块 | 对照本目录文件 | 重点查漏项 |
|---|---|---|
| [`docs/spec/ae/project-comp-layers.md`](../../spec/ae/project-comp-layers.md) / `bae-model` | `layers/*.json`, `layers.md`, `render-output.json` | 核对 9 类图层（含 AE 2026 的 Environment Light、单/双节点摄像机、3 种 3D 渲染器几何/材质选项）的属性 `matchName`、维度、默认值与渲染/输出设置键 |
| [`docs/spec/ae/shapes.md`](../../spec/ae/shapes.md) / 矢量渲染 | `layers/shape.json` | 核对 4 种路径（Rect/Ellipse/Star/Path）、4 种描边填充（含 Stroke Dashes/Taper/Wave）、10 种形状操作器（Merge/Offset/PB/Repeater/RC/Trim/Twist/Roughen/Wiggler/Zigzag）的完整参数和默认值 |
| [`docs/spec/ae/text.md`](../../spec/ae/text.md) / 文本引擎 | `layers/text.json` | 核对 `TextDocument` 默认字段、Path Options、More Options、文本动画器属性与 3 种选择器（Range/Wiggly/Expression）参数树 |
| [`docs/spec/ae/transform-keyframes-expressions.md`](../../spec/ae/transform-keyframes-expressions.md) / `bae-anim` | `layers/*.json`, `effects/expression-controls.json`, `shortcuts.json` | 核对 `canVaryOverTime` / `canSetExpression` / `isSeparationLeader`、表达式控制效果与时间轴/关键帧快捷键 |
| [`docs/spec/ae/compositing-3d-color.md`](../../spec/ae/compositing-3d-color.md) / `render/effects` | `effects/*.json`, `effects.md` | 按分类核对 AE 内置效果 `matchName`、参数个数、取值范围、单位和 940 处下拉枚举项，识别 BAE 尚未实现或参数名不一致的效果 |
| `ui/shell` / `ui/motion` / 交互设计 | `menus.json`, `panels-tools.md`, `shortcuts.md`, `preferences.json` | 核对菜单层级与常用快捷键覆盖率、8 个核心面板 + 25 个功能面板 + 27 个工具/Gizmo 切换逻辑以及首选项配置项 |
