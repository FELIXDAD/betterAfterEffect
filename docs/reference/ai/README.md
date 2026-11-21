# Adobe Illustrator 界面与功能参考资料（Windows 实机提取）

> 来源：Adobe Illustrator 2026 (30.1.0, build 136R) | 语言：zh_CN | 平台：Microsoft Windows 11 专业版 Build 26200 | 提取日期：2026-11-21

本目录收录从 Windows 实机安装的 **Adobe Illustrator 2026（30.1.0，build 136R，zh_CN）** 中提取的界面层级、键盘快捷键、工具与面板、效果菜单、预设目录、枚举标识以及文档对象模型（DOM）等结构化事实数据，供 BAE 设计/分镜工作区与 Illustrator 互转（见 `docs/interop/ai-mapping.md`、`crates/bae-interop-ai`）对照查漏使用。

## 文件索引与统计

| 文件 | 格式 | 内容概要 | 关键统计 |
|---|---|---|---|
| `meta.json` | JSON | Illustrator 版本/构建号、脚本版本、Windows 版本、提取日期、方法与总览计数 | 7 类数据，ExtendScript 脚本错误 0 条 |
| `menus.json` / `menus.md` | JSON / MD | 完整主窗口菜单树（含快捷键提示与分隔线），涵盖文件、编辑、对象、文字、选择、效果、视图、窗口、帮助 | 9 个顶层菜单、122 个子菜单、801 个命令项（134 项含快捷键提示）、125 条分隔线 |
| `shortcuts.json` / `shortcuts.md` | JSON / MD | 默认快捷键集（`.kys`）解析后的命令 ID、工具动作 ID、默认按键与上下文 | 603 条菜单命令（184 条已分配）、130 条工具/交互动作（58 条已分配） |
| `panels-tools.md` | MD | 高级（Default）/ 基本（Basic）/ 快速入门（Getting Started）工具栏槽位与内部工具 ID、`.kys` 额外交互动作、以及「窗口」菜单面板清单 | Default 30 槽位 / 91 个工具、Basic 37 个工具、Getting Started 34 个工具（7 组）、额外动作 44 项、「窗口」菜单 44 个面板/命令项 + 9 个子菜单 |
| `effects.md` | MD | 「效果」菜单拆解后的 Illustrator 效果与 Photoshop 效果完整分组清单 | Illustrator 效果 59 项（8 组）、Photoshop 效果 57 项（9 组）、顶部通用项 3 项 |
| `catalogs.json` | JSON | 可用字体数量（仅计数）、文档配置、预设库名称、应用级预设列表、135 个脚本枚举标识、SaveOptions / ExportOptions 类属性默认值 | 652 个可用字体（仅数量）、7 个文档配置、194 个内置预设库项、135 个枚举、4 个 SaveOptions 类、21 个 ExportOptions 类 |
| `dom.json` / `dom.md` | JSON / MD | 在临时空白文档中通过 ExtendScript 实际创建并反射读取的 DOM 对象属性（读写权限、声明类型、运行时类型、示例值）与方法列表 | 34 个 DOM 对象类型/实例、779 个属性、231 个方法 |

所有文件单文件均远小于 1 MB，总计约 0.61 MB。

## 提取方法与合规说明

1. **仅提取事实与结构数据**：只保留菜单/命令/工具/枚举名称、层级、属性名、读写权限、类型、默认值与快捷键绑定。不包含任何截图、图标、字体列表、预设原文件或 Adobe 官方说明/帮助/工具提示原文。
2. **菜单树提取**：Illustrator 2026 主窗口采用自绘 `OWL.MenuBar` 控件（`GetMenu()` 返回空句柄，UI Automation 只读遍历也无法展开子项）。经协调确认后，在无前台冲突的受控时段，使用只读键盘方式（`Alt+<助记键>` 打开顶层弹出菜单 → Win32 `MN_GETHMENU` 递归读取 `#32768` 菜单句柄与全部子菜单 → `Esc` 关闭，不点击或执行任何菜单项）完整采集 9 个顶层菜单。
3. **快捷键与工具栏**：解析安装目录 `Presets/zh_CN/` 下默认 `.kys` 与工具面板预设文件中的结构定义，转换为项目自有的 JSON / Markdown 格式。
4. **DOM 与目录**：通过 Windows COM（`Illustrator.Application`）执行 ExtendScript，在临时文档中创建文档、画板、图层、路径、复合路径、编组、剪切蒙版组、点/区域/路径文字、渐变、颜色、专色、图案、符号、画笔、色板、图形样式、光栅图及矩阵等对象并反射属性与方法，最后不保存关闭临时文档。
5. **枚举标识**：由于 Illustrator 30.1.0 的 ExtendScript 宿主对 C++ 枚举成员不支持 `for..in` / `reflect.properties` 遍历，`catalogs.json` 中的 135 个枚举名称及其成员标识符从本机 Illustrator 2026 自带的 `omv.xml` 中**仅提取枚举类名与成员标识符**（不读取任何 `<shortdesc>` 描述文字）。
6. **已省略的本机动态项与已知限制**：
   - `文件 -> 最近打开的文件`（20 条本机 MRU 文件路径）已省略，标记为 `omitted_dynamic: "local_recent_files"`。
   - `文字 -> 字体` 与 `文字 -> 最近使用的字体` 已省略，仅在 `catalogs.json` 中记录 `available_font_count: 652`。
   - `窗口 -> 扩展功能` 中的本机第三方扩展已省略，标记为 `omitted_dynamic: "local_installed_extensions"`。
   - 四个面板库子菜单中的空 `用户定义` 子菜单已标记为 `omitted_dynamic: "user_defined_library"`。
   - `dom.json` 中的 `PatternColor` 在未挂载到填充对象时，ExtendScript `reflect.properties` 返回 0 个属性（已如实保留，未手工补写）。

## 如何复现提取

可复现脚本位于 `tools/ai/reference/`（不修改 `tools/ai/` 下其他目录或文件）：

- `tools/ai/reference/extract_ai_dom_and_catalogs.jsx`：ExtendScript（ES3），提取应用元信息、Save/ExportOptions 默认值、应用预设与临时文档 DOM 反射。
- `tools/ai/reference/extract_ai_menus.ps1`：纯只读 Win32 `GetMenu` + UI Automation 探测脚本（适合旧版标准菜单栏或作为无焦点探测）。
- `tools/ai/reference/extract_ai_menus_keyboard.ps1`：针对 Illustrator 2026 `OWL.MenuBar` 的键盘只读 `Alt+<key>` + `MN_GETHMENU` 菜单提取脚本（需要短暂前台窗口访问，且不执行任何命令）。
- `tools/ai/reference/build_ai_reference.py`：读取原始 JSON、本机 `.kys` / 工具预设 / `Presets` 目录结构与 `omv.xml` 枚举标识，脱敏并生成 `docs/reference/ai/` 下的全部 JSON 与 Markdown 文件。
- `tools/ai/reference/run_extraction.ps1`：串联上述步骤的 PowerShell 入口（对 Illustrator 2026 可加 `-UseKeyboardMenus`）。

在装有 Adobe Illustrator 的 Windows 机器上运行示例：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/ai/reference/run_extraction.ps1 -UseKeyboardMenus
```

若已有原始提取文件，可单独重建目标目录：

```powershell
python tools/ai/reference/build_ai_reference.py `
  --menus-raw E:\bae-test\ref-ai\menus_raw.json `
  --dom-raw E:\bae-test\ref-ai\dom_and_catalogs_raw.json `
  --output-dir docs/reference/ai
```

## 与 BAE 对照查漏指引

结合 `docs/interop/ai-mapping.md` 与 `crates/bae-interop-ai`，建议按以下维度使用本目录数据做设计核对：

1. **AI 互转与桥接脚本（`crates/bae-interop-ai` / `bridges/ai/`）**
   - 对照 `dom.json` / `dom.md` 检查 ExtendScript 实际暴露的属性名与读写类型：例如 `Artboard.artboardRect`、`Layer` / `GroupItem.clipped`（剪切蒙版组）、`PathItem` / `PathPoint`（`anchor`、`leftDirection`、`rightDirection`、`pointType`）、`CompoundPathItem`、`TextFrame`（`kind` 区分点/区域/路径文字）以及 `CharacterAttributes` / `ParagraphAttributes`。
   - 对照 `catalogs.json` 中的 `enums` 检查枚举取值：如 `BlendModes`（16 种混合模式与 BAE 形状图层支持范围对齐）、`StrokeCap` / `StrokeJoin`、`GradientType`、`TextType`、`Justification`、`DocumentColorSpace`、`ExportType` / `ExportForScreensType`。
   - 对照 `catalogs.json` 中的 `save_options` 与 `export_options` 核对 PDF 兼容（`PDFSaveOptions` / `IllustratorSaveOptions.pdfCompatible`）与 SVG / PNG / WebP / AVIF / PSD 导出选项默认值。
2. **设计/分镜工作区的工具与交互对齐**
   - 对照 `panels-tools.md` 的 30 个高级工具栏槽位（91 个工具）和 Basic（37 个工具）/ Getting Started（34 个工具，7 个功能组）评估 BAE 矢量设计工具集的优先级（选择、钢笔/曲率、基本形状、文字、路径查找器/形状生成器、渐变、画板、变换）。
   - 对照 `shortcuts.md` / `shortcuts.json` 核对高频单键与组合键（如 `V`、`A`、`P`、`T`、`M`、`L`、`G`、`I`、`Shift+O`、`Shift+X`、`D`、`X`、`Ctrl+G`、`Ctrl+7` 等），避免与 BAE 时间线/合成快捷键冲突或保持矢量编辑习惯一致。
3. **面板、外观与实时效果边界**
   - 对照 `panels-tools.md` 中的「窗口」菜单（图层、外观、描边、渐变、透明度、路径查找器、对齐、变换、属性、画板、色板、符号、图形样式、文字子面板等）确认设计工作区需要提供哪些检查器面板。
   - 对照 `effects.md` 中的 59 项 Illustrator 效果（特别是 3D、变形、扭曲和变换、路径偏移/轮廓化、路径查找器、风格化投影/羽化/圆角）和 57 项 Photoshop 效果，明确哪些在互转时按展开外观导入、哪些在 BAE 侧映射为原生效果或提示不支持。
4. **菜单命令完整度核查**
   - 对照 `menus.md` / `menus.json` 的「对象」「文字」「选择」「视图」菜单，排查编组/剪切蒙版/复合路径/扩展外观/排列顺序/对齐分布/文本转轮廓/参考线与网格等高频操作在 BAE 中的对应命令。
