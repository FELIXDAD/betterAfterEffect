# 首选项（macOS：设置）

AE 26：Windows 上是 编辑 ▸ 首选项 ▸ 各页，macOS 上是 After Effects ▸ 设置… ▸ 各页（macOS 13 起系统把 Preferences 菜单项显示成"设置…"）。两个平台打开的窗口标题都是"首选项"（`docs/reference/ae/mac/menus.md` 第 4 节）。BAE 的动作 id 是 `app.settings`，参数 `{"page": "<页面 id>"}`；Cmd+Option+; / Ctrl+Alt+; 打开"启动和修复"页。菜单入口和命令目录由 B 维护（`docs/ui/ae-commands.md`）。

## 数据来源

页面、分组、控件、中英文文字、默认值、范围、单位、下拉项、联动置灰都来自 R1 在 Windows AE 26.0 上逐页读取的 `docs/reference/ae/ui/preferences-dialog.json`（16 页、214 个控件）。`crates/bae-prefs/gen/pages.py` 把它生成为 `crates/bae-prefs/src/pages.rs`（页面表 `PAGES`、中文表 `ZH`、AE 中文版保留英文的文字 `SAME_IN_ZH`），不要手改生成文件：

```
python3 -I crates/bae-prefs/gen/pages.py docs/reference/ae/ui/preferences-dialog.json
```

生成时做的少量适配（都写在生成脚本开头）：

- 只读的路径显示（项目模板、自定义自动保存文件夹、磁盘缓存文件夹）变成路径值，旁边的"选择…"按钮由外壳弹出系统对话框（`choose:<键>`）。
- "在资源管理器中显示首选项"在 macOS 上显示为"在访达中显示首选项"；"使用系统快捷键"只在 macOS 上显示。
- 跟在复选框或单选项后面同一行的数值（"保存间隔"的分钟数等）、单选组，只把文字当作无障碍名称，不另外画出来，和 AE 一样。
- AE 按机器算的默认值（最大磁盘缓存大小 = 卷的 10%、为其他应用保留的内存）先用 100 GB 和 6 GB。
- R1 记录为英文的中文文字（表达式主题名、单位、"New Project"等）不注册翻译；界面已经有的译名（例如"新建项目"）优先。

| 页面 id | 英文 | 中文 |
|---|---|---|
| `startup_repair` | Startup & Repair | 启动和修复 |
| `project` | Project | 项目 |
| `composition` | Composition | 合成 |
| `previews` | Previews | 预览 |
| `appearance` | Appearance | 外观 |
| `grids_guides` | Grids & Guides | 网格和参考线 |
| `labels` | Labels | 标签 |
| `type` | Type | 类型 |
| `import` | Import | 导入 |
| `export` | Export | 导出 |
| `audio` | Audio | 音频 |
| `disk` | Disk | 磁盘 |
| `memory_cpu` | Memory & CPU | 内存和 CPU |
| `video` | Video | 视频 |
| `3d` | 3D | 3D |
| `scripting_expressions` | Scripting & Expressions | 脚本和表达式 |

左侧列表平铺 16 页、没有分割线（分组只在菜单里）。

## 对话框

- 固定大小 948 × 801（AE 在 96 DPI 下的大小），不能缩放；左侧页面列表宽 210、行高 24；中间内容宽 593；右上角一竖列按钮（60 × 24，间隔 8）：确定（默认，Enter）、取消（Esc）、上一步、下一步。上一步 / 下一步循环翻页，首页末页都不置灰。
- 模态：对话框打开时主窗口点不动、快捷键不生效，但不压暗。
- 确定才保存；取消、Esc、关闭按钮都不保存。
- 没有功能的按钮（在安全模式下启动、清空所有缓存、清空磁盘缓存、GPU 信息……）照 AE 放出来但置灰。

## 存储

- 文件：配置目录下的 `preferences.json`（`bae_platform::SettingsFile` 信封，`kind = "bae-preferences"`，`version = 1`），内容是 `"<页面 id>.<控件 id>": 值` 的平铺对象；下拉和单选存选项的值（`lines`、`custom_location`……），颜色存 `#rrggbb`，时间码存 `h:mm:ss:ff`。
- 读取宽松：缺的键用默认值；类型不对、选项不存在、时间码格式不对的键用默认值；数字超出范围时夹到范围内；不认识的键原样保留、写回。文件损坏时移到 `preferences.json.corrupt`，全部用默认值，不会崩。
- 冒烟测试不读也不写这个文件。
- AE 26 的界面里已经没有"撤消级别"。BAE 把它留在存储里（`general.levels_of_undo`，不显示），默认 1000（BAE 一直用的深度）；AE 底层的默认值是 99。

## 别的模块怎么读

```rust
let store = cx.resources.get::<Arc<bae_prefs::PrefsStore>>();
let prefs = store.current();            // Arc<Preferences>，便宜
prefs.bool(bae_prefs::keys::DEFAULT_SPATIAL_LINEAR);
// 变了再刷新缓存：比较 store.generation()，或 store.subscribe(|p| …)
```

常用的键在 `bae_prefs::keys`；其余按 `<页面 id>.<控件 id>`（见生成文件）。命令 crate 可以依赖 `bae-prefs`（不开 `file` 特性就没有平台依赖）。

## 已经生效的设置

| 页面 | 设置 | 接到哪里 |
|---|---|---|
| （不显示） | 撤消级别 | `Stage::set_undo_limit`（每个打开的项目都用） |
| 外观 | 颜色主题（最暗 / 深色 / 灯光） | 每帧细分后的顶点颜色统一经过 `theme::adjust`：只改灰色（面板、文字、分隔线），强调色、标签色、图像和查看器画面不变；"最暗"（默认）时完全不处理 |
| 标签 | 16 种标签颜色和名称 | `theme::label_color` / `theme::label_name`（时间轴、项目面板、查看器手柄等）。颜色保持 AE 默认值时按 AE 实测的屏幕颜色画，改过的按所选颜色画 |
| 网格和参考线 | 网格颜色、样式、网格线间隔、次分隔线；动作安全、字幕安全、中心剪切动作安全、中心剪切字幕安全 | 合成查看器的网格和"标题/动作安全"（`GridGuides`） |
| 项目 | 保存间隔（开关和分钟数）、最大项目版本、自动保存位置（项目旁边 / 自定义文件夹） | 有未保存的更改时，按间隔把副本写到项目旁边的 `BAE Auto-Save` 文件夹（或自定义文件夹），`<名字> auto-save N.bae`，满了覆盖最旧的；不改项目本身的位置和保存状态。未命名项目只在设了自定义文件夹时保存 |
| 启动和修复 | 在资源管理器 / 访达中显示首选项 | 打开配置目录并选中 `preferences.json` |

## 暂不生效（能改能存，等对应功能接）

上表以外的设置都暂不生效。需要别的工作流接的：

| 设置 | 谁接 |
|---|---|
| 外观：路径点和手柄大小、对图层手柄和路径使用标签颜色、运动路径、在时间轴中同时显示时间码和帧 | A（查看器）、C（时间轴） |
| 外观：循环蒙版颜色、为蒙版路径使用对比色 | F（蒙版） |
| 网格和参考线：参考线颜色和样式、对称网格、透明网格 | A（查看器） |
| 合成：在合成开始时间创建图层、默认的空间插值为线性、在原始图层上创建拆分图层、新形状图层锚点居中、位置默认分离维度、合成 / 素材图层双击打开、合成的合成图层持续时间 | bae-motion 的命令、A / C |
| 标签：标签默认值（合成、视频、纯色、形状、文本……） | bae-motion `layer.create`（`default_label` 现在写死）、项目面板（E） |
| 导入：静止素材持续时间、序列素材帧速率、未标记的 Alpha、拖动导入多项为、替换时的素材解释 | E（导入和解释素材） |
| 脚本和表达式：表达式编辑器字号、主题、各开关和颜色（对话框里已有实时预览） | C（表达式） |
| 项目：新建项目载入模板、纯色文件夹名 | 外壳 / bae-motion |
| 预览、类型、导出、音频、磁盘、内存和 CPU、视频、3D | 对应功能还没有 |

## 待核

- Mac 版和 Windows 版的差异由 R2 补（例如 macOS 上"使用系统快捷键"显示、音频设备类型是 CoreAudio）。
- 分组下方的说明文字：R1 只给了中文摘要，没有原文，暂时没画。
- "颜色主题"的三张缩略图卡片是自绘的近似样子；"灯光"主题是把灰色反相得到的近似效果。
- 安装的内存、可用内存、采样率这类从机器读的值显示为"—"。
