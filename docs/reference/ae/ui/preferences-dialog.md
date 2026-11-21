# After Effects 26.0 设置（首选项）对话框参考（Windows）

> 来源：Adobe After Effects 26.0 (`26.0x67`, `zh_CN`) | 平台：Windows 11 Pro (10.0.26200, x64) | DPI：144 (150%) | 提取日期：2026-11-22

本文档由 `tools/ae/reference/build_preferences_dialog_doc.py` 与 `preferences-dialog.json` 同步生成，记录 Windows 版 AE 26.0 首选项对话框的整体框架、16 页目录、全部控件结构、默认值、下拉项、范围/单位、联动置灰关系与 `preferences.json` 键映射。不包含 Adobe 帮助/提示/报错原文、截图或本机路径。

## 1. 关键核对结论（供 #161 对照）

- **对话框尺寸与模态**：固定大小模态框（`#32770`），96 DPI 逻辑尺寸 `948 × 801` px（150% DPI 物理尺寸 `1422 × 1202` px）。
- **按钮位置与顺序**：右上角纵向一列（非底部一行），自上而下依次为：确定 (OK)、取消 (Cancel)、上一步 (Previous)、下一步 (Next)。上一步/下一步在首页与末页均保持可用并循环切换。
- **默认值核对**：**自动保存间隔** = `5 分钟`（第 2 页「项目」▸「自动保存」）；**最大项目版本** = `3`（第 2 页「项目」▸「自动保存」）；**撤消级别** = `99`（AE 26 已将“撤消级别”从首选项对话框 UI 中移除，仅保留后台键值，默认 99。 对应 `[General Section] Levels of Undo v2`）。
- **侧边栏 16 页顺序**：左侧列表框为平铺 16 项（列表框内无分割线；「编辑 ▸ 首选项」子菜单中按 4 / 4 / 2 / 4 / 2 分 5 组）。

## 2. 对话框框架与 16 页目录

### 2.1 右上角纵向按钮列

| 顺序 | ID | 中文 | 英文 | 角色 / 快捷键 | 行为 |
|---:|---|---|---|---|---|
| 1 | `ok` | 确定 | OK | `primary_default` / `Enter` | 提交/放弃全部修改 |
| 2 | `cancel` | 取消 | Cancel | `cancel` / `Escape` | 提交/放弃全部修改 |
| 3 | `previous` | 上一步 | Previous | `navigate_prev` / `—` | 循环切换（首页/末页均不置灰） |
| 4 | `next` | 下一步 | Next | `navigate_next` / `—` | 循环切换（首页/末页均不置灰） |

### 2.2 左侧 16 页顺序与菜单 Command ID

| 序号 | 页面 ID | 中文 | 英文 | 菜单分组 | Win32 Command ID |
|---:|---|---|---|---:|---:|
| 1 | `startup_repair` | 启动和修复 | Startup & Repair (`Ctrl+Alt+;`) | 第 1 组 | `24182785` |
| 2 | `project` | 项目 | Project | 第 1 组 | `24182786` |
| 3 | `composition` | 合成 | Composition | 第 1 组 | `24182787` |
| 4 | `previews` | 预览 | Previews | 第 1 组 | `24182788` |
| 5 | `appearance` | 外观 | Appearance | 第 2 组 | `24182790` |
| 6 | `grids_guides` | 网格和参考线 | Grids & Guides | 第 2 组 | `24182791` |
| 7 | `labels` | 标签 | Labels | 第 2 组 | `24182792` |
| 8 | `type` | 类型 | Type | 第 2 组 | `24182793` |
| 9 | `import` | 导入 | Import | 第 3 组 | `24182795` |
| 10 | `export` | 导出 | Export | 第 3 组 | `24182796` |
| 11 | `audio` | 音频 | Audio | 第 4 组 | `24182798` |
| 12 | `disk` | 磁盘 | Disk | 第 4 组 | `24182799` |
| 13 | `memory_cpu` | 内存和 CPU | Memory & CPU | 第 4 组 | `24182800` |
| 14 | `video` | 视频 | Video | 第 4 组 | `24182801` |
| 15 | `3d` | 3D | 3D | 第 5 组 | `24182803` |
| 16 | `scripting_expressions` | 脚本和表达式 | Scripting & Expressions | 第 5 组 | `24182804` |

## 3. 旧版 19 页 → AE 26（16 页）迁移对照表

| 旧版 AE 页面 | AE 26 落点页面 ID | 变更说明 |
|---|---|---|
| 常规 (General) | `startup_repair`, `project`, `composition`, `appearance` | 拆入启动和修复（主屏幕）、项目（保留工作区/Dynamic Link）、合成（图层创建/拆分/插值/蒙版/同步时间/双击行为）、外观（路径点大小/系统拾色器/禁用缩览图/工具提示/引导标记）；撤消级别已移出 UI（后台默认 99）。 |
| 显示 (Display) | `appearance` | 并入“外观”（运动路径、在信息面板和流程图中显示渲染进度、硬件加速面板、时间轴同时显示时间码和帧）。 |
| 导入 (Import) | `import` | 保留为第 9 页“导入”，合并硬件加速解码与自动重新加载素材。 |
| 输出 (Output) | `export`, `audio` | 主体改名为第 10 页“导出”；其中的“音频块持续时间”迁至第 11 页“音频”的“导出”分组。 |
| 网格和参考线 (Grids & Guides) | `grids_guides` | 保留为第 6 页，新增“透明度设置”网格大小/颜色分组。 |
| 标签 (Labels) | `labels` | 保留为第 7 页，含 3D 模型、其他 3D 及标记（注释/片段）默认标签。 |
| 媒体和磁盘缓存 (Media & Disk Cache) | `disk` | 改名为第 12 页“磁盘”，新增从磁盘缓存预览与无损压缩帧选项。 |
| 视频预览 (Video Preview) | `video` | 改名为第 14 页“视频”，新增 HDR/扩展显示范围预览开关。 |
| 外观 (Appearance) | `appearance` | 保留为第 5 页，旧版亮度滑块改为 3 卡片“颜色主题”（最暗/深色/灯光）+ 无障碍色彩对比度，并吸纳原“显示”页与部分“常规”页选项。 |
| 新建项目 (New Project) | `project` | 并入第 2 页“项目”（New Project 与 Solids Folder 分组）。 |
| 自动保存 (Auto-Save) | `project` | 并入第 2 页“项目”（自动保存与自动保存位置分组，默认 5 分钟 / 3 个版本）。 |
| 内存和性能 (Memory & Performance) | `memory_cpu` | 改名为第 13 页“内存和 CPU”，并吸纳“GPU 信息...”按钮（原在预览页）。 |
| 音频硬件 (Audio Hardware) | `audio` | 并入第 11 页“音频”（音频硬件分组）。 |
| 音频输出映射 (Audio Output Mapping) | `audio` | 并入第 11 页“音频”（输出渠道映射分组）。 |
| 同步设置 (Sync Settings) | （已移除） | AE 26 已移除此页。 |
| 类型 (Type) | `type` | 保留为第 8 页（字体菜单预览大小、近期字体数量、缺失字体提醒开关；旧版文本引擎单选已移除）。 |
| 3D | `3d` | 保留为第 15 页（摄像机导航、推拉镜头、3D 缓存）。 |
| 脚本和表达式 (Scripting & Expressions) | `scripting_expressions` | 保留为第 16 页。 |
| 通知 (Notifications) | `export` | 并入第 10 页“导出”（渲染通知分组）。 |
| 启动和修复 (Startup & Repair) | `startup_repair` | 提升为第 1 页（默认打开页，Ctrl+Alt+;）。 |

## 4. 逐页控件明细

### 4.1 第 1 页：启动和修复 (Startup & Repair) — `startup_repair`

#### 启动选项 / Startup Options

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `enable_home_screen` | `checkbox` | 启用主屏幕 | Enable Home Screen | 勾选 | — | `[General Section] Show Welcome Screen` |
| `show_system_compatibility_issues` | `checkbox` | 显示系统兼容性问题 | Show System Compatibility Issues | 勾选 | — | — |

#### 修复选项 / Repair Options

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `start_in_safe_mode` | `button` | 在安全模式下启动... | Start in Safe Mode... | — | — | — |
| `empty_all_caches` | `button` | 清空所有缓存... | Empty All Caches... | — | — | — |

#### 偏好设置管理 / Preferences Management

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `reveal_preferences_in_explorer` | `button` | 在资源管理器中显示首选项 | Reveal Preferences in Explorer | — | macOS 显示为 Reveal Preferences in Finder | — |
| `reset_all_preferences` | `button` | 将所有偏好设置重置为默认值... | Reset All Preferences to Default... | — | — | — |
| `migrate_previous_version_settings` | `button` | 迁移早期版本设置... | Migrate Previous Version Settings... | — | — | — |

**页脚控件：**
- `file_bug_report_link` (`link`): 请提交错误报告。 / File a Bug Report.（底部附错误报告外链及附带首选项/工程文件的说明）

### 4.2 第 2 页：项目 (Project) — `project`

#### Project Options / Project Options（AE 26.0x67 zh_CN 界面中此分组标题保留英文原文）

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `preserve_current_workspace_on_open` | `checkbox` | 打开项目时保留当前工作区 | Preserve Current Workspace When Opening Projects | 勾选 | — | — |
| `dynamic_link_highest_number` | `checkbox` | 与 After Effects 链接的 Dynamic Link 将项目文件名与最大编号结合使用 | Dynamic Link with After Effects Uses Project Filename with Highest Number | 不勾选 | — | — |

#### New Project / New Project（AE 26.0x67 zh_CN 界面中此分组标题保留英文原文）

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `new_project_loads_template` | `checkbox` | 新建项目加载模板 | New Project Loads Template | 不勾选 | — | `[Template Project] Enabled` |
| `choose_project_template` | `button` | 选择项目模板... | Choose Project Template... | — | 启用条件：`new_project_loads_template == True` | `[Template Project] Location` |
| `project_template_path_display` | `readonly_text` | — | — | `` | 显示所选模板文件路径 | — |

#### Solids Folder / Solids Folder（AE 26.0x67 zh_CN 界面中此分组标题保留英文原文）

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `new_project_solids_folder` | `text_input` | 新建项目纯色文件夹 | New Project Solids Folder | `纯色` / `Solids` | — | `[Template Project] New Project Solids Folder` |

#### 自动保存 / Auto-Save

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `auto_save_every_enabled` | `checkbox` | 保存间隔: | Save Every: | 勾选 | — | `[Auto Save] Enable Auto Save3` |
| `auto_save_interval_minutes` | `number_input` | 保存间隔数值 | Save Every Interval | `5` 分钟 | 范围 `1..1440` 分钟；启用条件：`auto_save_every_enabled == True` | `[Auto Save] Auto Save Interval` |
| `auto_save_on_render_queue` | `checkbox` | 启动渲染队列时保存 | Save When Starting Render Queue | 勾选 | — | `[Auto Save] Enable Auto Save RQ2` |
| `max_project_versions` | `number_input` | 最大项目版本: | Maximum Project Versions: | `3` | 范围 `1..999` | `[Auto Save] Auto Save Max Project Vers` |

#### 自动保存位置 / Auto-Save Location

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `auto_save_location_mode` | `radio_group` | 自动保存位置 | Auto-Save Location | 项目旁边 (`Next to Project`) | 项目旁边 (Next to Project) / 自定义位置 (Custom Location) | `[Auto Save] Save To Custom Location` |
| `choose_auto_save_folder` | `button` | 选择文件夹... | Choose Folder... | — | 启用条件：`auto_save_location_mode == custom_location` | `[Auto Save] Auto Save Folder` |
| `auto_save_folder_display` | `readonly_text` | 自定义自动保存路径 | Custom Auto-Save Path | `<Documents>/Adobe/After Effects` | — | — |

### 4.3 第 3 页：合成 (Composition) — `composition`

#### 合成选项 / Composition Options

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `center_anchor_point_new_shape` | `checkbox` | 在新形状图层上居中放置锚点 | Center Anchor Point in New Shape Layers | 不勾选 | — | `[General Section] Shape Layer Center Anchor Point` |
| `create_layers_at_comp_start` | `checkbox` | 在合成开始时创建图层 | Create Layers at Composition Start Time | 勾选 | — | `[General Section] Create New Layers At Time Zero` |
| `create_split_layers_above_original` | `checkbox` | 在原始图层上创建拆分图层 | Create Split Layers Above Original Layer | 勾选 | — | `[Split Layer] Grow Layer Stack Upwards` |
| `default_spatial_interpolation_linear` | `checkbox` | 默认的空间插值为线性 | Default Spatial Interpolation to Linear | 勾选 | — | — |
| `default_position_separate_dimensions` | `checkbox` | 分隔尺寸的默认位置属性 | Default Position Properties to Separated Dimensions | 不勾选 | — | `[General Section] Always Separate Dimensions` |
| `pen_shortcut_toggles_mask_feather` | `checkbox` | 钢笔工具快捷方式在钢笔和蒙版羽化工具之间切换 | Pen Tool Shortcut Toggles Between Pen and Mask Feather Tools | 勾选 | — | — |
| `preserve_constant_vertex_feather_count` | `checkbox` | 在编辑蒙版时保持固定的顶点和羽化点数 | Preserve Constant Vertex and Feather Point Count when Editing Masks | 勾选 | — | — |
| `show_bounding_box_precomp_layers` | `checkbox` | 显示预合成中图层的定界框 | Show Bounding Box for Layers in Pre-comps | 勾选 | — | `[Bounding Box] Show Sub Layers v2` |
| `switches_affect_nested_comps` | `checkbox` | 开关影响嵌套的合成 | Switches Affect Nested Comps | 勾选 | — | — |
| `synchronize_time_all_related_items` | `checkbox` | 同步所有相关项目的时间 | Synchronize Time of All Related Items | 勾选 | — | `[General Section] Synchronize Time` |
| `use_system_shortcut_keys` | `checkbox` | 使用系统快捷键 | Use System Shortcut Keys | 不勾选 | Windows 隐藏（macOS 专用）；Windows 上隐藏（STATE_SYSTEM_INVISIBLE）；仅 macOS 显示 | — |

#### 双击打开图层（使用 Alt 键进行反转） / Double-Click to Open Layers (Use Alt to Reverse)

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `double_click_footage_layer` | `dropdown` | 在素材图层上打开: | On a Footage Layer Open: | 图层面板 (`Layer Panel`) | 图层面板 (Layer Panel) / “源素材”面板 (Source Footage Panel) | — |
| `double_click_comp_layer` | `dropdown` | 在复合图层上打开: | On a Composition Layer Open: | “源合成”面板 (`Source Composition Panel`) | 图层面板 (Layer Panel) / “源合成”面板 (Source Composition Panel) | — |
| `double_click_paint_roto_opens_layer` | `checkbox` | 使用绘图、Roto 笔刷和调整边缘工具双击时将打开“图层”面板 | Double-Clicking with Paint, Roto Brush, and Refine Edge Tools Opens the Layer Panel | 勾选 | — | — |

#### 合成图层 / Synthetic Layers

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `synthetic_layers_duration_mode` | `radio_group` | 合成图层初始长度 | Synthetic Layers Initial Duration | 合成的长度 (`Length of Composition`) | 合成的长度 (Length of Composition) / 指定时长 (Fixed Duration) | — |
| `synthetic_layers_duration_timecode` | `timecode_input` | 指定时间码 | Duration Timecode | `0:00:01:00` | — | — |

### 4.4 第 4 页：预览 (Previews) — `previews`

#### 自适应分辨率 / Adaptive Resolution

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `adaptive_resolution_priority` | `slider` | 优先考虑： | Prioritize: | `11` | 范围 `0..18`；AE 26 离散滑块，默认值 11 对应最低 1/6 分辨率与 3D 渲染质量上限 7 | `[Realtime Update Preference Section] Minimum Adaptive Resolution Limit` |
| `adaptive_resolution_3d_renderer` | `dropdown` | 3D 渲染器可降低： | 3D Renderers Can Reduce: | 分辨率和质量 (`Resolution & Quality`) | 分辨率和质量 (Resolution & Quality) / 仅渲染质量 (Render Quality Only) | `[Realtime Update Preference Section] Adaptive Resolution Renderer Behavior` |

#### 查看器质量 / Viewer Quality

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `zoom_quality` | `dropdown` | 缩放质量: | Zoom Quality: | 更准确 (`More Accurate`) | 更快 (Faster) / 除缓存预览之外更准确 (More Accurate Except Cached Preview) / 更准确 (More Accurate) | — |
| `color_management_quality` | `dropdown` | 色彩管理品质: | Color Management Quality: | 除缓存预览之外更准确 (`More Accurate Except Cached Preview`) | 更快 (Faster) / 除缓存预览之外更准确 (More Accurate Except Cached Preview) / 更准确 (More Accurate) | — |

#### 合成开关 / Composition Switches

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `auto_enable_frame_blending_motion_blur` | `checkbox` | 自动启用帧混合和运动模糊渲染 | Automatically Enable Frame Blending and Motion Blur Rendering | 勾选 | — | `[Audio Settings Preference Section] Link Blend and Blur of Layers and Composition` |

#### 音频 / Audio

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `mute_audio_when_not_realtime` | `checkbox` | 非实时预览时将音频静音 | Mute Audio When Preview is Not Realtime | 不勾选 | — | `[Audio Settings Preference Section] Mute Audio When Preview Not Realtime v2` |

#### 空闲时缓存帧 / Cache Frames When Idle

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `idle_delay_before_caching` | `number_input` | 缓存开始前的空闲延迟： | Idle Delay Before Caching Starts: | `8` 秒 | 范围 `0..999` 秒 | `[Speculative Preview Preference Section] Idle Delay Before Caching Starts` |
| `cache_frames_order` | `dropdown` | 缓存帧： | Cache Frames: | 围绕当前时间 (`Around Current Time`) | 从当前时间 (From Current Time) / 围绕当前时间 (Around Current Time) / 从范围的起点 (From Start of Range) | `[Speculative Preview Preference Section] Caching Behavior` |
| `cache_range` | `dropdown` | 缓存范围： | Cache Range: | 工作区域按当前时间延伸 (`Work Area Extended by Current Time`) | 工作区 (Work Area) / 工作区域按当前时间延伸 (Work Area Extended by Current Time) / 整体持续时间 (Entire Duration) | `[Speculative Preview Preference Section] Cache Range` |

#### 放大率 / Magnification

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `magnification_type` | `dropdown` | 类型: | Type: | 平滑 (`Smooth`) | 旧版 (Legacy) / 平滑 (Smooth) | `[Magnification] Magnification Using Legacy` |
| `magnification_behavior` | `dropdown` | 行为: | Behavior: | 跟随光标 (`Follow Cursor`) | 居中 (Centered) / 跟随光标 (Follow Cursor) | `[Magnification] Magnification Behavior (0` |
| `magnification_trackpad_scroll` | `dropdown` | 触控板滚动: | Trackpad Scroll: | 平移 (`Pan`) | 缩放 (Zoom) / 平移 (Pan) | `[Magnification] Magnification Scrolling (0` |

### 4.5 第 5 页：外观 (Appearance) — `appearance`

#### 外观选项 / Appearance Options

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `path_point_and_handle_size` | `number_input` | 路径点和手柄大小: | Path Point and Handle Size: | `5` 像素 | 范围 `4..15` 像素 | `[General Section] Path Point and Handle Radius (pixels)` |
| `hardware_accelerate_panels` | `checkbox` | 硬件加速合成、图层和素材面板 | Hardware Accelerate Composition, Layer, and Footage Panels | 勾选 | — | `[Hardware Accelerated Viewer 2] Enabled` |
| `use_system_color_picker` | `checkbox` | 使用系统拾色器 | Use System Color Picker | 不勾选 | — | — |
| `show_rendering_progress_in_info_flowchart` | `checkbox` | 在信息面板和流程图中显示渲染进度 | Show Rendering Progress in Info Panel and Flowchart | 不勾选 | — | `[Main Pref Section v2] Pref_SHOW_INFO_FLOATER_PROGRESS` |
| `disable_thumbnails_in_project_panel` | `checkbox` | 在项目面板中禁用缩览图 | Disable Thumbnails in Project Panel | 不勾选 | — | `[Main Pref Section v2] Pref_RENDER_THUMBNAILS` |
| `show_both_timecode_and_frames_in_timeline` | `checkbox` | 在时间轴面板中同时显示时间码和帧 | Show Both Timecode and Frames in Timeline Panel | 勾选 | — | `[Prefs.Display] Show Alternate Time Display` |
| `show_tool_tips` | `checkbox` | 显示工具提示 | Show Tool Tips | 勾选 | — | — |
| `show_coachmarks` | `checkbox` | 显示引导标记 | Show Coachmarks | 不勾选 | — | `[Coachmarks Settings] Enabled` |
| `reset_coachmarks` | `button` | 重置引导提示标记 | Reset Coachmarks | — | — | — |

#### 主题 / Theme

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `color_theme` | `theme_picker` | 颜色主题 | Color Theme | 最暗 (`Darkest`) | 最暗 (Darkest) / 深色 (Dark) / 灯光 (Light)；3 个缩略图卡片单选（AE 26 取代旧版亮度连续滑块；zh_CN 将 Light 译作“灯光”） | `[Main Pref Section v2] User Interface Brightness (4) [0.0..1.0]` |
| `accessibility_color_contrast` | `checkbox` | 无障碍色彩对比度 | Accessibility Color Contrast | 勾选 | — | — |

#### 颜色 / Colors

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `use_label_color_for_handles_and_paths` | `checkbox` | 对图层手柄和路径使用标签颜色 | Use Label Color for Layer Handles and Paths | 勾选 | — | — |
| `use_label_color_for_related_tabs` | `checkbox` | 对相关选项卡使用标签颜色 | Use Label Color for Related Tabs | 勾选 | — | — |
| `cycle_mask_colors` | `checkbox` | 循环蒙版颜色（使用标签颜色） | Cycle Mask Colors (Use Label Colors) | 勾选 | — | `[Mask Colors] Cycle Mask Label Colors` |
| `use_contrasting_color_for_mask_path` | `checkbox` | 为蒙版路径使用对比度颜色 | Use Contrasting Color for Mask Path | 勾选 | 启用条件：`cycle_mask_colors == True` | `[Mask Colors] Use Contrasting Color For Mask Path Enabled` |
| `use_gradients` | `checkbox` | 使用渐变色 | Use Gradients | 勾选 | — | — |

#### 运动路径 / Motion Path

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `motion_path_mode` | `radio_group` | 运动路径显示方式 | Motion Path Display Mode | 指定时长 (`No More Than (Time)`) | 没有运动路径 (No Motion Path) / 所有关键帧 (All Keyframes) / 不超过 (No More Than (Keyframes)) / 指定时长 (No More Than (Time)) | — |
| `motion_path_max_keyframes` | `number_input` | 最大关键帧数 | Max Keyframes | `5` 个关键帧 | 范围 `1..999` 个关键帧 | — |
| `motion_path_time_window` | `timecode_input` | 运动路径时长 | Motion Path Duration | `0:00:15:00` | — | — |

### 4.6 第 6 页：网格和参考线 (Grids & Guides) — `grids_guides`

#### 网格 / Grid

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `grid_color` | `color_swatch` | 颜色: | Color: | `#008000` | — | `[Main Pref Section v2] Pref_GRID_COLOR` |
| `grid_style` | `dropdown` | 样式: | Style: | 线条 (`Lines`) | 线条 (Lines) / 虚线 (Dashed Lines) / 点 (Dots) | `[Main Pref Section v2] Pref_GRID_STYLE` |
| `gridline_every` | `number_input` | 网格线间隔: | Gridline Every: | `80` 像素 | 范围 `1..9999` 像素 | `[Main Pref Section v2] Pref_GRID_PIX_PER_DIV` |
| `grid_subdivisions` | `number_input` | 次分隔线: | Subdivisions: | `4` | 范围 `1..99` | `[Main Pref Section v2] Pref_GRID_SUB_PER_DIV` |

#### 对称网格 / Proportional Grid

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `proportional_grid_horizontal` | `number_input` | 水平: | Horizontal: | `8` | 范围 `1..99` | `[Main Pref Section v2] Pref_ANIMGRID_CELLS_HORIZ` |
| `proportional_grid_vertical` | `number_input` | 垂直: | Vertical: | `6` | 范围 `1..99` | `[Main Pref Section v2] Pref_ANIMGRID_CELLS_VERT v2` |

#### 参考线 / Guides

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `guides_color` | `color_swatch` | 颜色: | Color: | `#20E0FF` | — | `[Main Pref Section v2] Pref_GUIDE_COLOR` |
| `guides_style` | `dropdown` | 样式: | Style: | 线条 (`Lines`) | 线条 (Lines) / 虚线 (Dashed Lines) | `[Main Pref Section v2] Pref_GUIDE_STYLE` |

#### 安全边距 / Safe Margins

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `action_safe` | `number_input` | 动作安全: | Action-safe: | `10` % | 范围 `0..100` % | `[Grids And Guides v2] Action Safe Margin % (0-100)` |
| `title_safe` | `number_input` | 字幕安全: | Title-safe: | `20` % | 范围 `0..100` % | `[Grids And Guides v2] Title Safe Margin % (0-100)` |
| `center_cut_action_safe` | `number_input` | 中心剪切动作安全: | Center-Cut Action-safe: | `32.5` % | 范围 `0..100` % | `[Grids And Guides v2] 4:3 Action Safe Margin % (0-100)` |
| `center_cut_title_safe` | `number_input` | 中心剪切字幕安全: | Center-Cut Title-safe: | `40` % | 范围 `0..100` % | `[Grids And Guides v2] 4:3 Title Safe Margin % (0-100)` |

#### 透明度设置 / Transparency Settings

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `transparency_grid_size` | `dropdown` | 网格大小: | Grid Size: | 中 (`Medium`) | 小 (Small) / 中 (Medium) / 大 (Large) | `[General Section] Transparency Grid Size` |
| `transparency_grid_colors_preset` | `dropdown` | 网格颜色： | Grid Colors: | 自定义... (`Custom...`) | 自定义... (Custom...) / 灯光 (Light) / 中 (Medium) / 深色 (Dark) / 红色 (Red) / 橙色 (Orange) / 绿色 (Green) / 蓝色 (Blue) / 紫色 (Purple) | `[Main Pref Section v2] Pref_TRANSPARENCY_GRID_COLOR_POPUP_INDEX` |
| `transparency_grid_color_1` | `color_swatch` | 棋盘格色 1 | Grid Color 1 | `#CCCCCC` | — | `[Main Pref Section v2] Pref_TRANSPARENCY_GRID_COLOR1` |
| `transparency_grid_color_2` | `color_swatch` | 棋盘格色 2 | Grid Color 2 | `#FFFFFF` | — | `[Main Pref Section v2] Pref_TRANSPARENCY_GRID_COLOR2 v2` |

### 4.7 第 7 页：标签 (Labels) — `labels`

#### 标签默认值 / Label Defaults（全部 16 个下拉框共用 17 项标签色板（无 + 16 色）；左列 7 项 + 右列 7 项 + 底部嵌套“标记”子分组 2 项）

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `label_default_comp` | `dropdown` | 合成: | Composition: | 砂岩 (`Sandstone`, `15`) | 引用 `labelPalette17`（无 + 16 色） | `[Label Preference Indices Section 5] Comp Label Index 2` |
| `label_default_video` | `dropdown` | 视频: | Video: | 浅绿色 (`Aqua`, `3`) | 引用 `labelPalette17`（无 + 16 色） | `[Label Preference Indices Section 5] Video Label Index 2` |
| `label_default_audio` | `dropdown` | 音频: | Audio: | 海泡沫 (`Sea Foam`, `7`) | 引用 `labelPalette17`（无 + 16 色） | `[Label Preference Indices Section 5] Audio Label Index 2` |
| `label_default_still` | `dropdown` | 静止图像: | Still: | 淡紫色 (`Lavender`, `5`) | 引用 `labelPalette17`（无 + 16 色） | `[Label Preference Indices Section 5] Still Label Index 2` |
| `label_default_folder` | `dropdown` | 文件夹: | Folder: | 黄色 (`Yellow`, `2`) | 引用 `labelPalette17`（无 + 16 色） | `[Label Preference Indices Section 5] Folder Label Index 2` |
| `label_default_null` | `dropdown` | 空对象: | Null: | 红色 (`Red`, `1`) | 引用 `labelPalette17`（无 + 16 色） | `[Label Preference Indices Section 5] Null Label Index` |
| `label_default_3d_model` | `dropdown` | 3D 模型: | 3D Model: | 青色 (`Cyan`, `14`) | 引用 `labelPalette17`（无 + 16 色） | `[Label Preference Indices Section 5] 3D Model Label Index` |
| `label_default_solid` | `dropdown` | 纯色: | Solid: | 红色 (`Red`, `1`) | 引用 `labelPalette17`（无 + 16 色） | `[Label Preference Indices Section 5] Solid Label Index 2` |
| `label_default_camera` | `dropdown` | 摄像机: | Camera: | 粉色 (`Pink`, `4`) | 引用 `labelPalette17`（无 + 16 色） | `[Label Preference Indices Section 5] Camera Label Index 2` |
| `label_default_light` | `dropdown` | 灯光: | Light: | 桃红色 (`Peach`, `6`) | 引用 `labelPalette17`（无 + 16 色） | `[Label Preference Indices Section 5] Light Label Index 2` |
| `label_default_shape` | `dropdown` | 形状: | Shape: | 蓝色 (`Blue`, `8`) | 引用 `labelPalette17`（无 + 16 色） | `[Label Preference Indices Section 5] Shape Label Index 2` |
| `label_default_adjustment` | `dropdown` | 调整: | Adjustment: | 淡紫色 (`Lavender`, `5`) | 引用 `labelPalette17`（无 + 16 色） | `[Label Preference Indices Section 5] Adjustment Label Index 2` |
| `label_default_text` | `dropdown` | 文本: | Text: | 红色 (`Red`, `1`) | 引用 `labelPalette17`（无 + 16 色） | `[Label Preference Indices Section 5] Text Label Index` |
| `label_default_other_3d` | `dropdown` | 其他 3D: | Other 3D: | 绿色 (`Green`, `9`) | 引用 `labelPalette17`（无 + 16 色） | `[Label Preference Indices Section 5] Materials Label Index` |
| `label_default_marker_comment` | `dropdown` | 注释: | Comment: | 无 (`None`, `0`) | 引用 `labelPalette17`（无 + 16 色） | `[Label Preference Indices Section 5] Comment Label Index` |
| `label_default_marker_segmentation` | `dropdown` | 片段: | Segmentation: | 淡紫色 (`Lavender`, `5`) | 引用 `labelPalette17`（无 + 16 色） | `[Label Preference Indices Section 5] Segmentation Label Index` |

#### 标签颜色 / Label Colors（左列 #1-#8、右列 #9-#16；每项由 [名称输入框 + 色块按钮 + 吸管按钮] 组成）

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `label_color_1` | `label_color_entry` | 红色 | Red | `#B53838` | — | `[Label Preference Color Section 5] Label Color ID 2 # 1` |
| `label_color_2` | `label_color_entry` | 黄色 | Yellow | `#E4D84C` | — | `[Label Preference Color Section 5] Label Color ID 2 # 2` |
| `label_color_3` | `label_color_entry` | 浅绿色 | Aqua | `#A9CBC7` | — | `[Label Preference Color Section 5] Label Color ID 2 # 3` |
| `label_color_4` | `label_color_entry` | 粉色 | Pink | `#E5BCC9` | — | `[Label Preference Color Section 5] Label Color ID 2 # 4` |
| `label_color_5` | `label_color_entry` | 淡紫色 | Lavender | `#A9A9CA` | — | `[Label Preference Color Section 5] Label Color ID 2 # 5` |
| `label_color_6` | `label_color_entry` | 桃红色 | Peach | `#E7C19E` | — | `[Label Preference Color Section 5] Label Color ID 2 # 6` |
| `label_color_7` | `label_color_entry` | 海泡沫 | Sea Foam | `#B3C7B3` | — | `[Label Preference Color Section 5] Label Color ID 2 # 7` |
| `label_color_8` | `label_color_entry` | 蓝色 | Blue | `#677DE0` | — | `[Label Preference Color Section 5] Label Color ID 2 # 8` |
| `label_color_9` | `label_color_entry` | 绿色 | Green | `#4AA44C` | — | `[Label Preference Color Section 5] Label Color ID 2 # 9` |
| `label_color_10` | `label_color_entry` | 紫色 | Purple | `#8E2C9A` | — | `[Label Preference Color Section 5] Label Color ID 2 # 10` |
| `label_color_11` | `label_color_entry` | 橙色 | Orange | `#E8920D` | — | `[Label Preference Color Section 5] Label Color ID 2 # 11` |
| `label_color_12` | `label_color_entry` | 棕色 | Brown | `#7F452A` | — | `[Label Preference Color Section 5] Label Color ID 2 # 12` |
| `label_color_13` | `label_color_entry` | 紫红色 | Fuchsia | `#F46DD6` | — | `[Label Preference Color Section 5] Label Color ID 2 # 13` |
| `label_color_14` | `label_color_entry` | 青色 | Cyan | `#3DA2A5` | — | `[Label Preference Color Section 5] Label Color ID 2 # 14` |
| `label_color_15` | `label_color_entry` | 砂岩 | Sandstone | `#A89677` | — | `[Label Preference Color Section 5] Label Color ID 2 # 15` |
| `label_color_16` | `label_color_entry` | 深绿色 | Dark Green | `#1E401E` | — | `[Label Preference Color Section 5] Label Color ID 2 # 16` |

### 4.8 第 8 页：类型 (Type) — `type`

#### 字体菜单 / Font Menu

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `font_preview_size` | `dropdown` | 预览大小 | Preview Size | 小 (`Small`) | 小 (Small) / 中 (Medium) / 大 (Large) / 特大 (Extra Large) | `[General Section] Font Panel Preview Size` |
| `recent_fonts_count` | `dropdown` | 要显示的近期字体数量 | Number of Recent Fonts to Display | 14 (`14`) | 共 16 项：0 (0)、1 (1)、2 (2)、3 (3)、4 (4)、5 (5) 等（完整列表见 JSON） | `[General Section] Font Panel MRU Size` |

#### “解析字体”对话框 / Resolve Fonts Dialog

| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |
|---|---|---|---|---|---|---|
| `do_not_warn_missing_fonts` | `checkbox` | 打开项目时，不要提醒我缺失字体。 | Do not warn me about missing fonts when opening a project. | 不勾选 | — | `[General Section] Hide Missing Font Dialog` |
