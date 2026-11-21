# Adobe Illustrator 界面与功能参考资料（Windows 实机提取）

> 来源：Adobe Illustrator 2026 (30.1.0, build 136R) | 语言：zh_CN | 平台：Microsoft Windows 11 专业版 Build 26200 | 提取日期：2026-11-21

本目录收录从 Windows 实机安装的 **Adobe Illustrator 2026（30.1.0，build 136R，zh_CN）** 中提取的界面层级、键盘快捷键、工具与面板、效果菜单、预设目录、枚举标识以及文档对象模型（DOM）等结构化事实数据，供 BAE 设计/分镜工作区与 Illustrator 互转（见 `docs/interop/ai-mapping.md`、`crates/bae-interop-ai`）对照查漏使用。

## 使用原则与合规边界

> **重要说明**：本目录中的中文显示标签来自 Illustrator 30.1.0 的中文界面（`zh_CN`），只用于对照查漏；对照时请以英文内部标识（`id`：如 `executeMenuCommand` 命令名、`.kys` 工具/动作 ID、ExtendScript DOM 类型与枚举名）为稳定键；BAE 自己的界面文字（中英文）必须按 [`docs/glossary.md`](../../glossary.md) 独立撰写，**不能**从这份数据复制。

- **仅含事实与结构数据**：只保留菜单/命令/工具/面板/效果/枚举名称与内部 ID、层级、属性名、读写权限、类型、默认值与快捷键绑定。不包含任何截图、图标、字体列表、预设原文件或 Adobe 官方说明、帮助、工具提示或报错原文。
- **不含 Illustrator 报错原文**：`dom.json` 与 `dom.md` 中无法在当前实例上读取的属性（共 7 类 39 处）统一使用本仓库短标记 `{"unavailable": "<tag>"}`（`wrapped_items_only` 18 处、`text_frame_kind` 15 处、`undefined_for_text_range` 2 处、`error_2072` / `no_active_dataset` / `no_manual_kerning` / `no_associated_file` 各 1 处），不再保留任何 Adobe 原始报错文本。

## 稳定英文键（`id`）覆盖率

所有无法拿到稳定、可验证英文内部标识的项（例如动态枚举的预设库/工作区/脚本条目、禁用分组标题、字号/旋转角度数值字面量、以及仅带动态序号 `_GenericPluginMenuItem N` 或未公开命令名的菜单项）均显式标为 `"id": null`（Markdown 表格中显示为 `—`）。

| 数据域 | 标识来源 | 覆盖率 | 说明 |
|---|---|---|---|
| **菜单命令** (`menus.json` / `menus.md`) | `.kys` `/Menus` + 公开 `executeMenuCommand` 标识 | **558 / 801（69.7%）**；静态项 **558 / 631（88.4%）** | 其余 170 项为色板/画笔/符号/图形样式库、工作区与脚本动态项；未覆盖的 73 个静态项主要为字号（16）、旋转角度（19）、分组标题（4）及未公开/动态插件项（34），均标 `null` |
| **默认快捷键** (`shortcuts.json` / `shortcuts.md`) | 默认 `.kys` 预设（`/Menus`、`/Tools`） | 菜单 **603 / 603（100%）**；工具动作 **130 / 130（100%）** | 全量以 `.kys` 内部 ID 为键，其中 398 条菜单命令已与 `menus.json` 路径交叉关联 |
| **工具** (`panels-tools.json` / `panels-tools.md`) | 工具面板预设 + `.kys` `/Tools` | Default **91 / 91（100%）**；Basic **37 / 37（100%）**；Getting Started **34 / 34（100%）**；额外 `/Tools` 动作 **44 / 44（100%）** | 全部为内部工具/动作英文 ID（如 `Adobe Select Tool`、`Adobe Pen Tool`） |
| **面板** (`panels-tools.json` / `panels-tools.md`) | 「窗口」菜单 + `.kys` / `executeMenuCommand` | 顶层面板/命令 **41 / 44（93.2%）**；「文字」子面板 **8 / 8（100%）**；合计 **49 / 52（94.2%）** | 仅 3 个动态界面条开关（上下文任务栏、帮助栏、隐藏生成式菜单）无稳定 ID，标 `null` |
| **效果** (`effects.json` / `effects.md`) | 「效果」菜单 + `.kys` / `executeMenuCommand` | Illustrator 效果 **59 / 59（100%）**；Photoshop 效果 **57 / 57（100%）**；顶部通用命令 **3 / 3（100%）**；合计 **119 / 119（100%）** | Photoshop 适配器 ID 中的尾部空格 FourCC（如 `Live PSAdapter_plugin_Gls `、`Live PSAdapter_plugin_Ct  `）在 JSON 中原样保留 |
| **DOM 与枚举** (`dom.json`、`catalogs.json`) | ExtendScript 反射 + `omv.xml` 枚举标识 | **34 / 34 类、779 属性、231 方法、135 枚举（100%）** | 全量为原生英文 ExtendScript 标识符 |

## 文件索引与统计

| 文件 | 格式 | 内容概要 | 关键统计 |
|---|---|---|---|
| `meta.json` | JSON | Illustrator 版本/构建号、脚本版本、Windows 版本、提取日期、方法与总览计数（含 `id` 覆盖统计） | 7 类数据，ExtendScript 脚本错误 0 条 |
| `menus.json` / `menus.md` | JSON / MD | 完整主窗口菜单树（含英文 `id`、Win32 `commandId`、快捷键提示与分隔线），Markdown 以表格呈现 | 9 个顶层菜单、122 个子菜单、801 个命令项（558 项含 `id`，134 项含快捷键提示）、125 条分隔线 |
| `shortcuts.json` / `shortcuts.md` | JSON / MD | 默认快捷键集（`.kys`）解析后的命令 `id`、关联菜单路径、工具动作 `id`、默认按键与上下文 | 603 条菜单命令（184 条已分配）、130 条工具/交互动作（58 条已分配） |
| `panels-tools.json` / `panels-tools.md` | JSON / MD | 高级（Default）/ 基本（Basic）/ 快速入门（Getting Started）工具栏槽位与内部工具 `id`、`.kys` 额外交互动作、以及「窗口」菜单面板清单（含 `id`） | Default 30 槽位 / 91 工具、Basic 37 工具、Getting Started 34 工具（7 组）、额外动作 44 项、「窗口」顶层面板/命令 44 项（41 含 `id`）+ 「文字」子面板 8 项（8 含 `id`） |
| `effects.json` / `effects.md` | JSON / MD | 「效果」菜单拆解后的 Illustrator 效果与 Photoshop 效果完整分组清单及内部命令 `id` | Illustrator 效果 59 项（8 组）、Photoshop 效果 57 项（9 组）、顶部通用项 3 项，119/119 项含 `id` |
| `catalogs.json` | JSON | 可用字体数量（仅计数）、文档配置、预设库名称、应用级预设列表、135 个脚本枚举标识、SaveOptions / ExportOptions 类属性默认值 | 652 个可用字体（仅数量）、7 个文档配置、194 个内置预设库项、135 个枚举、4 个 SaveOptions 类、21 个 ExportOptions 类 |
| `dom.json` / `dom.md` | JSON / MD | 在临时空白文档中通过 ExtendScript 实际创建并反射读取的 DOM 对象属性（读写权限、声明类型、运行时类型、示例值或 `{"unavailable": "..."}` 标记）与方法列表 | 34 个 DOM 对象类型/实例、779 个属性、231 个方法 |

所有文件单文件均远小于 1 MB，总计约 0.76 MB。

## 提取方法与已知限制

1. **菜单树提取**：Illustrator 2026 主窗口采用自绘 `OWL.MenuBar` 控件（`GetMenu()` 返回空句柄，UI Automation 只读遍历也无法展开子项）。经协调确认后，在无前台冲突的受控时段，使用只读键盘方式（`Alt+<助记键>` 打开顶层弹出菜单 → Win32 `MN_GETHMENU` 递归读取 `#32768` 菜单句柄与全部子菜单 → `Esc` 关闭，不点击或执行任何菜单项）完整采集 9 个顶层菜单。
2. **快捷键与工具栏**：解析安装目录 `Presets/zh_CN/` 下默认 `.kys` 与工具面板预设文件中的结构定义，转换为项目自有的 JSON / Markdown 格式，并正确还原 `.kys` 中转义的尾部空格标识符。
3. **菜单内部 `id` 关联**：优先对照本机 Illustrator 30.1.0 默认 `.kys` `/Menus` 节（按快捷键与菜单层级校验，0 冲突），并结合 Adobe 公开脚本参考中的 `executeMenuCommand` 命令名补充 CC 之后新增的静态命令，维护于 `tools/ai/reference/menu_command_ids.json`。
4. **DOM 与目录**：通过 Windows COM（`Illustrator.Application`）执行 ExtendScript，在临时文档中创建文档、画板、图层、路径、复合路径、编组、剪切蒙版组、点/区域/路径文字、渐变、颜色、专色、图案、符号、画笔、色板、图形样式、光栅图及矩阵等对象并反射属性与方法，最后不保存关闭临时文档。属性读取异常统一由脚本分类为 `{"unavailable": "<tag>"}` 短标记。
5. **枚举标识**：由于 Illustrator 30.1.0 的 ExtendScript 宿主对 C++ 枚举成员不支持 `for..in` / `reflect.properties` 遍历，`catalogs.json` 中的 135 个枚举名称及其成员标识符从本机 Illustrator 2026 自带的 `omv.xml` 中**仅提取枚举类名与成员标识符**（不读取任何 `<shortdesc>` 描述文字）。
6. **已省略的本机动态项与已知限制**：
   - `文件 -> 最近打开的文件`（20 条本机 MRU 文件路径）已省略，标记为 `omitted_dynamic: "local_recent_files"`。
   - `文字 -> 字体` 与 `文字 -> 最近使用的字体` 已省略，仅在 `catalogs.json` 中记录 `available_font_count: 652`。
   - `窗口 -> 扩展功能` 中的本机第三方扩展已省略，标记为 `omitted_dynamic: "local_installed_extensions"`。
   - 四个面板库子菜单中的空 `用户定义` 子菜单已标记为 `omitted_dynamic: "user_defined_library"`。
   - `dom.json` 中的 `PatternColor` 在未挂载到填充对象时，ExtendScript `reflect.properties` 返回 0 个属性（已如实保留，未手工补写）。

## 如何复现提取

可复现脚本位于 `tools/ai/reference/`（不修改 `tools/ai/` 下其他目录或文件）：

- `tools/ai/reference/extract_ai_dom_and_catalogs.jsx`：ExtendScript（ES3），提取应用元信息、Save/ExportOptions 默认值、应用预设与临时文档 DOM 反射（不可读属性直接输出 `{"unavailable": "<tag>"}`）。
- `tools/ai/reference/extract_ai_menus.ps1`：纯只读 Win32 `GetMenu` + UI Automation 探测脚本（适合旧版标准菜单栏或作为无焦点探测）。
- `tools/ai/reference/extract_ai_menus_keyboard.ps1`：针对 Illustrator 2026 `OWL.MenuBar` 的键盘只读 `Alt+<key>` + `MN_GETHMENU` 菜单提取脚本（需要短暂前台窗口访问，且不执行任何命令）。
- `tools/ai/reference/menu_command_ids.json`：经 `.kys` 校验的菜单路径到 `executeMenuCommand` 英文内部 `id` 映射表。
- `tools/ai/reference/build_ai_reference.py`：读取原始 JSON（或通过 `--from-existing` 读取已有提取产物）、本机 `.kys` / 工具预设 / `Presets` 目录结构与 `omv.xml` 枚举标识，脱敏、附加内部 `id` 并生成 `docs/reference/ai/` 下的全部 JSON 与 Markdown 文件。
- `tools/ai/reference/run_extraction.ps1`：串联上述步骤的 PowerShell 入口（对 Illustrator 2026 可加 `-UseKeyboardMenus`）。

在装有 Adobe Illustrator 的 Windows 机器上运行完整提取：

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

若仅更新映射表或生成格式，也可从已提取产物离线重建（无需启动 Illustrator）：

```powershell
python tools/ai/reference/build_ai_reference.py `
  --from-existing docs/reference/ai `
  --output-dir docs/reference/ai `
  --extracted-date 2026-11-21
```

## 与 BAE 对照查漏指引

结合 `docs/interop/ai-mapping.md` 与 `crates/bae-interop-ai`，建议按以下维度使用本目录数据做设计核对（均以英文内部 `id` 为对照键）：

1. **AI 互转与桥接脚本（`crates/bae-interop-ai` / `bridges/ai/`）**
   - 对照 `dom.json` / `dom.md` 检查 ExtendScript 实际暴露的属性名与读写类型：例如 `Artboard.artboardRect`、`Layer` / `GroupItem.clipped`（剪切蒙版组）、`PathItem` / `PathPoint`（`anchor`、`leftDirection`、`rightDirection`、`pointType`）、`CompoundPathItem`、`TextFrame`（`kind` 区分点/区域/路径文字）以及 `CharacterAttributes` / `ParagraphAttributes`。
   - 对照 `catalogs.json` 中的 `enums` 检查枚举取值：如 `BlendModes`（16 种混合模式与 BAE 形状图层支持范围对齐）、`StrokeCap` / `StrokeJoin`、`GradientType`、`TextType`、`Justification`、`DocumentColorSpace`、`ExportType` / `ExportForScreensType`。
   - 对照 `catalogs.json` 中的 `save_options` 与 `export_options` 核对 PDF 兼容（`PDFSaveOptions` / `IllustratorSaveOptions.pdfCompatible`）与 SVG / PNG / WebP / AVIF / PSD 导出选项默认值。
2. **设计/分镜工作区的工具与交互对齐**
   - 对照 `panels-tools.json` / `panels-tools.md` 的 30 个高级工具栏槽位（91 个工具）和 Basic（37 个工具）/ Getting Started（34 个工具，7 个功能组）评估 BAE 矢量设计工具集的优先级（选择、钢笔/曲率、基本形状、文字、路径查找器/形状生成器、渐变、画板、变换）。
   - 对照 `shortcuts.json` / `shortcuts.md` 核对高频单键与组合键（如 `V`、`A`、`P`、`T`、`M`、`L`、`G`、`I`、`Shift+O`、`Shift+X`、`D`、`X`、`Ctrl+G`、`Ctrl+7` 等），避免与 BAE 时间线/合成快捷键冲突或保持矢量编辑习惯一致。
3. **面板、外观与实时效果边界**
   - 对照 `panels-tools.json` / `panels-tools.md` 中的「窗口」菜单及其内部 ID（如 `AdobeLayerPalette1`、`Style Palette`、`Adobe Stroke Palette`、`Adobe Gradient Palette`、`Adobe Transparency Palette Menu Item`、`AdobePathfinderPalette1`、`AdobeAlignObjects2`、`AdobeTransformObjects1`、`Adobe Property Palette`、`Adobe Artboard Palette`、`Adobe Swatches Menu Item` 等）确认设计工作区需要提供哪些检查器面板。
   - 对照 `effects.json` / `effects.md` 中的 59 项 Illustrator 效果（特别是 `Live Adobe Geometry3D *`、`Live Deform *`、`Live Transform`、`Live Offset Path`、`Live Pathfinder *`、`Live Adobe Drop Shadow` / `Live Feather` / `Live Adobe Round Corners`）和 57 项 Photoshop 效果，明确哪些在互转时按展开外观导入、哪些在 BAE 侧映射为原生效果或提示不支持。
4. **菜单命令完整度核查**
   - 对照 `menus.json` / `menus.md` 的「对象」「文字」「选择」「视图」菜单，按英文 `id`（如 `group`、`makeMask`、`compoundPath`、`expandStyle`、`Horizontal Align Left`、`outline`、`showguide` 等）排查高频操作在 BAE 中的对应命令。
