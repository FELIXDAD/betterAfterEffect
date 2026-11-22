# Adobe After Effects 26 对话框完整参考（Windows）

> **采集环境**：Windows 11 Pro x64（3840×2160 @ 150% DPI），Adobe After Effects 26.0（`26.0x67`，实测界面语言 `zh_CN`，附标准 `en_US` 对照）。
> **采集方式**：Win32 / MSAA 实时控件树 + 交互式下拉/页签/子对话框探测 + ExtendScript 边界实测（零截图）。
> **说明**：纯色设置、摄像机设置、灯光设置已在 PR #192（`docs/reference/ae/interaction/layers-timeline-dnd.md`）采集，本目录不再重复。

## 已采集对话框目录

| # | 对话框（中 / 英） | 快捷键 / 入口 | 结构化 JSON |
|---:|---|---|---|
| 1 | 合成设置（Composition Settings） | `Ctrl+N / Ctrl+K` | [`dialogs/composition-settings.json`](dialogs/composition-settings.json) |
| 2 | 预合成（Pre-compose） | `Ctrl+Shift+C` | [`dialogs/pre-compose.json`](dialogs/pre-compose.json) |
| 3 | 项目设置（Project Settings） | `Ctrl+Alt+Shift+K` | [`dialogs/project-settings.json`](dialogs/project-settings.json) |
| 4 | 解释素材: <文件名>（Interpret Footage: <Filename>） | `Ctrl+Alt+G` | [`dialogs/interpret-footage.json`](dialogs/interpret-footage.json) |
| 5 | 时间延长（Time Stretch） | `图层 ▸ 时间 ▸ 时间伸缩(C)...` | [`dialogs/time-stretch.json`](dialogs/time-stretch.json) |
| 6 | 自动方向（Auto-Orientation） | `Ctrl+Alt+O` | [`dialogs/auto-orient.json`](dialogs/auto-orient.json) |

## 1. 合成设置（Composition Settings）

- **结构化数据**：[`dialogs/composition-settings.json`](dialogs/composition-settings.json)
- **入口**：`合成 ▸ 新建合成...`（`Ctrl+N`）、`合成 ▸ 合成设置...`（`Ctrl+K`）
- **窗口尺寸**：`894 × 812` 物理像素 @ 144 DPI（`596 × 541` 逻辑像素 @ 96 DPI）
- **页签（3 个）**：`基本`（Basic）｜`高级`（Advanced）｜`3D 渲染器`（3D Renderer）
- **顶部共享行**：`合成名称:` / `Composition Name:` 文本框（新建默认 `合成 1` / `Comp 1`，编辑显示当前合成名）
- **底部按钮栏**：左下 `预览`（Preview）复选框（`Ctrl+N` 时未勾选且置灰；`Ctrl+K` 时默认勾选且可用）；右下 `确定`（OK）+ `取消`（Cancel）水平并排。
- **记住上次的值**：**是，跨重启持久化**（写入偏好文件 `[Composition Pref Section]`）。

### 1.1 页签：基本（Basic）

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `dropdown` | 预设: | Preset: | 记住上次值 / 自定义 | 自定义 + 25 个内置预设（见下表） | 共 26 个可选条目（自定义 + 25 个内置预设，内含 4 条分组分隔线，总菜单项 30 条）；修改宽度/高度/像素长宽比/帧速率偏离预设后自动切为“自定义”。 |
| 2 | `icon_button` | 保存自定义预设 | Save Custom Preset | — | — | 始终可用；点击将当前尺寸/PAR/帧速率存为用户预设。 |
| 3 | `icon_button` | 删除自定义预设 | Delete Custom Preset | — | — | 选中“自定义”或任意内置预设时置灰禁用；仅当选中用户自建预设时可用。 |
| 4 | `number_scrub` | 宽度: | Width: | 记住上次值 | 1–30000 px | — |
| 5 | `number_scrub` | 高度: | Height: | 记住上次值 | 1–30000 px | — |
| 6 | `checkbox` | 锁定长宽比为 <W>:<H> (<ratio>) | Lock Aspect Ratio to <W>:<H> (<ratio>) | False | — | 动态显示当前宽高比（如 16:9 (1.78) 或 4:3 (1.33)）；勾选后改宽度或高度时另一边按比例联动。 |
| 7 | `dropdown` | 像素长宽比: | Pixel Aspect Ratio: | 方形像素 | 0.01–100.0；方形像素 / D1/DV NTSC (0.91) / D1/DV NTSC 宽银幕  (1.21) / D1/DV PAL (1.09) / D1/DV PAL 宽银幕  (1.46) / HDV 1080/DVCPRO HD 720 (1.33) / DVCPRO HD 1080 (1.5) / 变形 2:1 (2) | — |
| 8 | `static_text` | 画面长宽比: | Frame Aspect Ratio: | — | — | 只读计算值，位于像素长宽比右侧（如 16:9 (1.78)、9:16 (0.56)、457:333 (1.37)）。 |
| 9 | `text_input_with_dropdown` | 帧速率: | Frame Rate: | 记住上次值 | 1.0–999.0 帧/秒；快捷档: 8, 12, 15, 23.976, 24, 25, 29.97, 30, 50, 59.94, 60, 120 | 左侧为自由输入的帧速率文本框，紧邻右侧为小箭头快捷预设下拉（12 档常用帧率）。 |
| 10 | `dropdown` | 丢帧模式（无独立标签，位于“帧/秒”右侧） | Drop Frame mode (unlabeled, right of Frames per Second) | 无丢帧 | 丢帧 / 无丢帧 | 仅当帧速率为 29.97 或 59.94 fps 时启用；其他所有帧速率（23.976/24/25/30/50/60/120 等）均置灰禁用并锁定为“无丢帧”。 |
| 11 | `dropdown` | 分辨率: | Resolution: | 完整 | 完整 / 二分之一 / 三分之一 / 四分之一 / 自定义... | — |
| 12 | `timecode_input` | 开始时间码: | Start Timecode: | 0:00:00:00 | — | 右侧提示中文实测在“基础”前有两个半角空格（例如 `是 0:00:00:00  基础 30`）。 |
| 13 | `timecode_input` | 持续时间: | Duration: | 记住上次值 | 0–3:00:00:00（10800 s） | — |
| 14 | `color_picker_row` | 背景颜色: | Background Color: | 黑色 | — | — |

### 1.2 页签：高级（Advanced）

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `anchor_grid_3x3` | 锚点: | Anchor: | center (pic4) | — | 新建合成（Ctrl+N）时始终置灰禁用；编辑已有合成（Ctrl+K）时仅在修改了宽度或高度后启用，用于决定画布裁切/扩展时的对齐锚点。 |
| 2 | `checkbox` | 在嵌套时或在渲染队列中，保留帧速率 | Preserve frame rate when nested or in render queue | False | — | — |
| 3 | `checkbox` | 在嵌套时保留分辨率 | Preserve resolution when nested | False | — | — |
| 4 | `number_scrub` | 快门角度: | Shutter Angle: | 180 | 0–720 ° | — |
| 5 | `number_scrub` | 快门相位: | Shutter Phase: | 0 | -360–360 ° | AE 26 实测新合成默认值为 0°（部分早期文档常记为 -90°）。 |
| 6 | `number_scrub` | 每帧样本: | Samples Per Frame: | 16 | 2–64 | — |
| 7 | `number_scrub` | 自适应采样限制: | Adaptive Sample Limit: | 128 | 64–256 | AE 26 实测下限为 64（非 2），上限 256，默认 128。 |

### 1.3 页签：3D 渲染器（3D Renderer）

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `dropdown` | 渲染器: | Renderer: | 经典 3D | 经典 3D / 高级 3D / Cinema 4D | — |
| 2 | `button` | 选项... | Options... | — | — | AE 26 中三个渲染器（经典 3D、高级 3D、Cinema 4D）下均启用，分别弹出对应的渲染器选项子对话框。 |
| 3 | `button` | 关于... | About... | — | — | 三个渲染器下均启用。 |
| 4 | `two_column_list` | 已启用功能 / 已禁用功能 | Enabled Features / Disabled Features | — | — | 选中“经典 3D”时隐藏，仅在选中“高级 3D”或“Cinema 4D”时显示左右两栏能力对照表。 |

### 1.4 合成预设完整列表（实测 25 项 + 4 条分隔线）

| # | 预设名称（zh_CN） | 预设名称（en_US） | 宽 × 高 | 像素长宽比（数值） | 画面长宽比 | 帧速率 | 丢帧下拉可用 |
|---:|---|---|---:|---|---|---:|:---:|
| 0 | `自定义` | `Custom` | — | — | — | — | 随帧率 |
| 1 | `HD  •  1920x1080 • 24 fps` | `HD  •  1920x1080 • 24 fps` | 1920 × 1080 | 方形像素 (`1.0`) | 16:9 (1.78) | 24 | 置灰（无丢帧） |
| 2 | `HD  •  1920x1080 • 25 fps` | `HD  •  1920x1080 • 25 fps` | 1920 × 1080 | 方形像素 (`1.0`) | 16:9 (1.78) | 25 | 置灰（无丢帧） |
| 3 | `HD  •  1920x1080 • 29.97 fps` | `HD  •  1920x1080 • 29.97 fps` | 1920 × 1080 | 方形像素 (`1.0`) | 16:9 (1.78) | 29.97 | ✅ 可用 |
| — | *（分隔线）* | *（Separator）* | — | — | — | — | — |
| 4 | `UHD 4K)  •  3840x2160 • 23.976 fps` | `UHD (4K)  •  3840x2160 • 23.976 fps` | 3840 × 2160 | 方形像素 (`1.0`) | 16:9 (1.78) | 23.976 | 置灰（无丢帧） |
| 5 | `UHD 4K)  •  3840x2160 • 25 fps` | `UHD (4K)  •  3840x2160 • 25 fps` | 3840 × 2160 | 方形像素 (`1.0`) | 16:9 (1.78) | 25 | 置灰（无丢帧） |
| 6 | `UHD 4K)  •  3840x2160 • 29.97 fps` | `UHD (4K)  •  3840x2160 • 29.97 fps` | 3840 × 2160 | 方形像素 (`1.0`) | 16:9 (1.78) | 29.97 | ✅ 可用 |
| 7 | `UHD (8K)  •  7680x4320 • 23.976 fps` | `UHD (8K)  •  7680x4320 • 23.976 fps` | 7680 × 4320 | 方形像素 (`1.0`) | 16:9 (1.78) | 23.976 | 置灰（无丢帧） |
| — | *（分隔线）* | *（Separator）* | — | — | — | — | — |
| 8 | `社交媒体纵向 HD  •  1080x1920 • 30 fps` | `Social Media Portrait HD  •  1080x1920 • 30 fps` | 1080 × 1920 | 方形像素 (`1.0`) | 9:16 (0.56) | 30 | 置灰（无丢帧） |
| 9 | `社交媒体横向 HD  •  1920x1080 • 30 fps` | `Social Media Landscape HD  •  1920x1080 • 30 fps` | 1920 × 1080 | 方形像素 (`1.0`) | 16:9 (1.78) | 30 | 置灰（无丢帧） |
| 10 | `社交媒体纵向  •  720x1280 • 30 fps` | `Social Media Portrait  •  720x1280 • 30 fps` | 720 × 1280 | 方形像素 (`1.0`) | 9:16 (0.56) | 30 | 置灰（无丢帧） |
| 11 | `社交媒体横向  •  1280x720 • 30 fps` | `Social Media Landscape  •  1280x720 • 30 fps` | 1280 × 720 | 方形像素 (`1.0`) | 16:9 (1.78) | 30 | 置灰（无丢帧） |
| 12 | `社交媒体方形  •  1080x1080 • 30 fps` | `Social Media Square  •  1080x1080 • 30 fps` | 1080 × 1080 | 方形像素 (`1.0`) | 1:1 (1.00) | 30 | 置灰（无丢帧） |
| — | *（分隔线）* | *（Separator）* | — | — | — | — | — |
| 13 | `HDV/HDTV  •  1280x720 • 25 fps` | `HDV/HDTV  •  1280x720 • 25 fps` | 1280 × 720 | 方形像素 (`1.0`) | 16:9 (1.78) | 25 | 置灰（无丢帧） |
| 14 | `HDV/HDTV  •  1280x720 • 29.97 fps` | `HDV/HDTV  •  1280x720 • 29.97 fps` | 1280 × 720 | 方形像素 (`1.0`) | 16:9 (1.78) | 29.97 | ✅ 可用 |
| 15 | `HDV  •  1440x1080 (1.33) • 25 fps` | `HDV  •  1440x1080 (1.33) • 25 fps` | 1440 × 1080 | HDV 1080/DVCPRO HD 720 (1.33) (`1.333333`) | 16:9 (1.78) | 25 | 置灰（无丢帧） |
| 16 | `HDV  •  1440x1080 (1.33) • 29.97 fps` | `HDV  •  1440x1080 (1.33) • 29.97 fps` | 1440 × 1080 | HDV 1080/DVCPRO HD 720 (1.33) (`1.333333`) | 16:9 (1.78) | 29.97 | ✅ 可用 |
| 17 | `DVCPRO HD  •  960x720 (1.33) • 23.976 fps` | `DVCPRO HD  •  960x720 (1.33) • 23.976 fps` | 960 × 720 | HDV 1080/DVCPRO HD 720 (1.33) (`1.333333`) | 16:9 (1.78) | 23.976 | 置灰（无丢帧） |
| 18 | `DVCPRO HD  •  960x720 (1.33) • 25 fps` | `DVCPRO HD  •  960x720 (1.33) • 25 fps` | 960 × 720 | HDV 1080/DVCPRO HD 720 (1.33) (`1.333333`) | 16:9 (1.78) | 25 | 置灰（无丢帧） |
| 19 | `DVCPRO HD  •  960x720 (1.33) • 29.97 fps` | `DVCPRO HD  •  960x720 (1.33) • 29.97 fps` | 960 × 720 | HDV 1080/DVCPRO HD 720 (1.33) (`1.333333`) | 16:9 (1.78) | 29.97 | ✅ 可用 |
| 20 | `DVCPRO HD  •  1440x1080 (1.33) • 25 fps` | `DVCPRO HD  •  1440x1080 (1.33) • 25 fps` | 1440 × 1080 | HDV 1080/DVCPRO HD 720 (1.33) (`1.333333`) | 16:9 (1.78) | 25 | 置灰（无丢帧） |
| 21 | `DVCPRO HD  •  1280x1080 (1.5) • 29.97 fps` | `DVCPRO HD  •  1280x1080 (1.5) • 29.97 fps` | 1280 × 1080 | DVCPRO HD 1080 (1.5) (`1.5`) | 16:9 (1.78) | 29.97 | ✅ 可用 |
| — | *（分隔线）* | *（Separator）* | — | — | — | — | — |
| 22 | `Cineon 1/2  •  1828x1332 • 24 fps` | `Cineon Half  •  1828x1332 • 24 fps` | 1828 × 1332 | 方形像素 (`1.0`) | 457:333 (1.37) | 24 | 置灰（无丢帧） |
| 23 | `Cineon 完整  •  3656x2664 • 24 fps` | `Cineon Full  •  3656x2664 • 24 fps` | 3656 × 2664 | 方形像素 (`1.0`) | 457:333 (1.37) | 24 | 置灰（无丢帧） |
| 24 | `胶片 (2K)  •  2048x1556 • 24 fps` | `Film (2K)  •  2048x1556 • 24 fps` | 2048 × 1556 | 方形像素 (`1.0`) | 512:389 (1.32) | 24 | 置灰（无丢帧） |
| 25 | `胶片 (4K)  •  4096x3112 • 24 fps` | `Film (4K)  •  4096x3112 • 24 fps` | 4096 × 3112 | 方形像素 (`1.0`) | 512:389 (1.32) | 24 | 置灰（无丢帧） |

### 1.5 子对话框

#### 自定义分辨率（Custom Resolution）
- **入口**：基本页签 ▸ 分辨率 ▸ 自定义...；**按钮**：右下 `确定` / `取消`
- **渲染每 [N] 个像素（水平）。**（Render every [N] pixel(s) horizontally.，`number_scrub`）：默认 `1`，范围：1–99
- **渲染每 [N] 个像素（垂直）。**（Render every [N] pixel(s) vertically.，`number_scrub`）：默认 `1`，范围：1–99

#### 经典的 3D 渲染器选项（Classic 3D Renderer Options）
- **入口**：3D 渲染器页签 ▸ 经典 3D ▸ 选项...；**按钮**：右下 `确定` / `取消`
- **阴影图分辨率:**（Shadow Map Resolution:，`dropdown`）：默认 `合成大小`，选项：合成大小 / 250 / 500 / 750 / 1000 / 1500 / 2000 / 3000 / 4000

#### 高级 3D 渲染器选项（Advanced 3D Renderer Options）
- **入口**：3D 渲染器页签 ▸ 高级 3D ▸ 选项...；**按钮**：右下 `确定` / `取消`
- **渲染品质**（Render Quality，`number_scrub_with_slider`）：默认 `8`
- **分辨率:**（Resolution:，`dropdown`）：默认 `正常 (16 MB)`，选项：0.5 倍 (2 MB) / 正常 (16 MB) / 双倍 (128 MB)
- **平滑度**（Smoothness，`number_scrub_with_slider`）：默认 `3`
- **投射框尺寸**（Shadow Box Size，`number_scrub`）：默认 `max(compWidth, compHeight)`
- **投射框中心 (X / Y / Z)**（Shadow Box Center (X / Y / Z)，`vector3_scrub`）：默认 `['compWidth / 2', 'compHeight / 2', 0]`

#### Cinema 4D 渲染器选项（Cinema 4D Renderer Options）
- **入口**：3D 渲染器页签 ▸ Cinema 4D ▸ 选项...；**按钮**：右下 `确定` / `取消`
- **品质:**（Quality:，`number_scrub_with_slider`）：默认 `25`，范围：1–100
- **派生参数只读展示（随品质滑块联动）**（Derived read-only metrics (driven by Quality slider)）：默认品质 25 下实测 光线阈值: 0.818、光线深度: 18、反射深度: 3、阴影深度: 9、图层采样: 2.18

## 2. 预合成（Pre-compose）

- **结构化数据**：[`dialogs/pre-compose.json`](dialogs/pre-compose.json)
- **入口**：`图层 ▸ 预合成(P)...`（`Ctrl+Shift+C`）
- **窗口尺寸**：`646 × 515` 物理像素 @ 144 DPI（`431 × 343` 逻辑像素 @ 96 DPI）
- **页签**：无（单页对话框）
- **底部按钮栏**：右下 `确定`（OK） + `取消`（Cancel） 水平并排。
- **记住上次的值**：单选模式（保留/移动）与两个复选框在可用条件下记住上次选择；新合成名称每次按当前选中图层动态生成（单图层为“<图层名> 合成 1”，多图层为“预合成 1”）。

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `text_input` | 新合成名称: | New composition name: | 单层 `<图层名> 合成 1` / 多层 `预合成 1` | — | 实测选中单图层 MySolid 默认“MySolid 合成 1”，选中单形状层 ShapeLayer 默认“ShapeLayer 合成 1”，选中多图层默认“预合成 1”。 |
| 2 | `radio_group` | 属性处理方式 | Attributes Placement | 将所有属性移动到新合成 | 保留“<当前合成名>”中的所有属性 / 将所有属性移动到新合成 | 保留“<当前合成名>”中的所有属性：仅当选中单个带源图层（素材/纯色/嵌套合成）时可用；多选图层或选中无源图层（形状/文字/空对象/摄像机/灯光/调整图层）时置灰禁用。；将所有属性移动到新合成：始终可用；当“保留所有属性”不可用时强制选中此项。 |
| 3 | `checkbox` | 将合成持续时间调整为所选图层的时间范围 | Adjust composition duration to the time span of the selected layers | 未勾选 | — | 仅当选中“将所有属性移动到新合成”时可用；切换到“保留所有属性”时立即置灰禁用。 |
| 4 | `checkbox` | 打开新合成 | Open New Composition | 未勾选 | — | 始终可用；勾选后确定时会立即在查看器/时间轴打开新预合成。 |

## 3. 项目设置（Project Settings）

- **结构化数据**：[`dialogs/project-settings.json`](dialogs/project-settings.json)
- **入口**：`文件 ▸ 项目设置...`（`Ctrl+Alt+Shift+K`）
- **窗口尺寸**：`1029 × 1174` 物理像素 @ 144 DPI（`686 × 783` 逻辑像素 @ 96 DPI）
- **页签（5 个）**：`视频渲染和效果`（Video Rendering and Effects）｜`时间显示样式`（Time Display Style）｜`颜色`（Color）｜`音频`（Audio）｜`表达式`（Expressions）
- **底部按钮栏**：右下 `确定`（OK） + `取消`（Cancel） 水平并排。
- **记住上次的值**：各项设置随项目（.aep）保存，不跨新建项目继承上次值；新建空白项目默认 8 bpc、时间码、Adobe 色彩管理（工作空间无）、48.000 kHz、JavaScript。

### 3.1 页签：视频渲染和效果（Video Rendering and Effects）

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `dropdown` | 使用范围: | Use: | Mercury GPU 加速 (CUDA) | Mercury GPU 加速 (CUDA) / 仅 Mercury 软件 | GPU 选项按硬件/平台动态显示（NVIDIA Windows 为 CUDA，macOS 为 Metal，OpenCL 卡为 OpenCL）。 |

### 3.2 页签：时间显示样式（Time Display Style）

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `radio_group` | 显示样式 | Time Display Mode | 时间码 | 时间码 / 帧数 | 实测切换“时间码”与“帧数”时，两组子控件均保持可编辑状态，由当前单选决定生效模式。 |
| 2 | `dropdown` | 素材开始时间: | Footage Start Time: | 使用媒体源 | 使用媒体源 / 00:00:00:000 | — |
| 3 | `number_scrub` | 默认基准: | Default Base: | 30 | 1–999 fps | — |
| 4 | `checkbox` | 使用英尺数 + 帧数: | Use Feet + Frames: | 未勾选 | — | — |
| 5 | `dropdown` | 胶片规格 | Film Type | 35 毫米 | 16 毫米 / 35 毫米 | — |
| 6 | `dropdown` | 帧计数: | Frame Count: | 时间码转换 | 开始位置 0 / 开始位置 1 / 时间码转换 | — |

### 3.3 页签：颜色（Color）

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `dropdown` | 颜色引擎： | Color Engine: | Adobe color managed | Adobe color managed / OCIO color managed | 切换引擎时会弹出确认提示框（标题“颜色引擎的更改”，按钮“确定”/“取消”）。 |
| 2 | `dropdown` | OCIO 配置 - | OCIO Configuration: | ACES 1.2 | ACES 1.2 / ACES 1.3 CG v1.0 / ACES 1.3 Studio v1.0 / Custom | 仅当颜色引擎为 OCIO color managed 时可用；Adobe 模式下置灰。 |
| 3 | `file_picker_row` | 自定义 OCIO 配置： | Custom OCIO Configuration: |  | 按钮: 选择 / 重新加载 | 仅当 OCIO 配置选择 Custom 时启用输入框、选择与重新加载按钮；否则全部置灰。 |
| 4 | `dropdown` | 位深度: | Depth: | 每通道 8 位 | 每通道 8 位 / 每通道 16 位 / 每通道 32 位（浮点） | — |
| 5 | `dropdown` | 3D LUT 插值： | 3D LUT Interpolation: | Tetrahedral (requires GPU acceleration) | Trilinear / Tetrahedral (requires GPU acceleration) | — |
| 6 | `dropdown` | 工作色彩空间: | Working Color Space: | Adobe `无` / OCIO `compositing_linear: ACES - ACEScg` | Adobe 103 项；OCIO ACES 1.2 共 17 项 | Adobe 模式下包含“无”与内置/系统 ICC 配置文件（本机实测 103 项）；OCIO ACES 1.2 默认列出 17 项角色/工作空间。 |
| 7 | `checkbox` | 全部显示 | Show All | 未勾选 | — | Adobe 模式下置灰；OCIO 模式下可用，勾选后展开全部 OCIO 色彩空间。 |
| 8 | `dropdown` | 采用工作灰度系数: | Assume Working Gamma: | 2.4 (Rec. 709) | 2.2 (sRGB) / 2.4 (Rec. 709) | 仅当 Adobe 模式且工作色彩空间为“无”时可用；选择任意色彩空间或切至 OCIO 后置灰。 |
| 9 | `checkbox` | 线性化工作色彩空间 | Linearize Working Space | 未勾选 | — | 仅当 Adobe 模式且工作色彩空间不为“无”时可用；工作空间为“无”或 OCIO 模式下置灰。 |
| 10 | `checkbox` | 使用 1.0 灰度系数混合颜色 | Blend Colors Using 1.0 Gamma | 未勾选 | — | Adobe 模式下可用（勾选线性化工作色彩空间后置灰）；OCIO 模式下置灰。 |
| 11 | `checkbox` | 补偿场景引用的配置文件 | Compensate for Scene-referred Profiles | 勾选 | — | 默认勾选；仅当 Adobe 模式且工作色彩空间不为“无”时可用，否则置灰。 |
| 12 | `dropdown` | 显示色彩空间： | Display Color Space: | OCIO `ACES/sRGB` | OCIO ACES 1.2 共 27 项（如 None, ACES/sRGB, ACES/DCDM 等） | Adobe 模式下置灰为空；OCIO 模式下可用（ACES 1.2 共 27 项，默认 ACES/sRGB）。 |
| 13 | `readonly_text` | 色彩管理状态说明 | Color Management Status Info | — | — | 底部只读摘要区域，随引擎和工作空间动态更新色彩管理开关状态。 |

### 3.4 页签：音频（Audio）

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `dropdown` | 采样率: | Sample Rate: | 48.000 kHz | 22.050 kHz / 32.000 kHz / 44.100 kHz / 48.000 kHz / 96.000 kHz | — |

### 3.5 页签：表达式（Expressions）

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `dropdown` | 表达式引擎: | Expressions Engine: | JavaScript | 旧版 ExtendScript / JavaScript | — |

## 4. 解释素材: <文件名>（Interpret Footage: <Filename>）

- **结构化数据**：[`dialogs/interpret-footage.json`](dialogs/interpret-footage.json)
- **入口**：`文件 ▸ 解释素材(G) ▸ 主要...`（`Ctrl+Alt+G`）
- **窗口尺寸**：`895 × 1249` 物理像素 @ 144 DPI（`597 × 833` 逻辑像素 @ 96 DPI）
- **页签（2 个）**：`主要选项`（Main Options）｜`颜色`（Color）
- **底部按钮栏**：左下 `预览`（Preview）；右下 `确定`（OK） + `取消`（Cancel） 水平并排。
- **记住上次的值**：设置随单个素材项保存；可通过“记住解释”（Ctrl+Alt+C）/“应用解释”（Ctrl+Alt+V）手动复制，图像序列默认帧率继承自“首选项 ▸ 导入”。

### 4.1 页签：主要选项（Main Options）

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `radio_group` | Alpha | Alpha | 直接 - 无遮罩 | 忽略 / 直接 - 无遮罩 / 预乘 - 有彩色遮罩: | 素材含 Alpha 通道时可用；无 Alpha 通道时整组置灰。 |
| 2 | `checkbox` | 反转 Alpha | Invert Alpha | 未勾选 | — | 选择“忽略”时置灰，选择“直接”或“预乘”时可用。 |
| 3 | `color_swatch_with_eyedropper` | 遮罩颜色 + 吸管 | Matte Color Swatch + Eyedropper | `#000000` | — | 仅当选择“预乘 - 有彩色遮罩:”时生效。 |
| 4 | `button` | 猜测 | Guess | — | — | 含 Alpha 通道时可用，自动判断直接或预乘。 |
| 5 | `radio_group` | 帧速率 | Frame Rate | 序列 `假定此帧速率:` / 视频 `使用文件中的帧速率:` | 使用文件中的帧速率: / 假定此帧速率:（静帧/视频显示“匹配帧速率:”） | 静帧图像整组置灰；图像序列无内嵌帧率，故“使用文件中的帧速率”置灰、“假定此帧速率”默认选中。 |
| 6 | `number_input` | 帧速率数值 | Frame Rate Value | 30 | 0.01–999 fps | 静帧时置灰；ExtendScript conformFrameRate 实测范围 0–999（0 表示使用源帧率，UI 最小 0.01）。 |
| 7 | `dropdown` | 丢帧模式 | Drop Frame Mode | 丢帧 | 丢帧 / 无丢帧 | 仅在 29.97 / 59.94 等支持丢帧的帧率下可用，30 fps 与静帧下置灰。 |
| 8 | `radio_group` | 开始时间码 | Start Timecode | 使用文件中的源时间码: | 使用文件中的源时间码: / 覆盖开始时间码: | 静帧图像整组置灰；序列和视频可用。 |
| 9 | `timecode_scrub` | 覆盖时间码数值 | Override Start Timecode Value | 0:00:00:00 | — | 静帧时置灰；右侧附基准帧率只读显示（如“是 0:00:00:00  基础 30”）。 |
| 10 | `dropdown` | 分离场: | Separate Fields: | 关 | 关 / 高场优先 / 低场优先 | — |
| 11 | `checkbox` | 保留边缘（仅最佳品质） | Preserve Edges (Best Quality Only) | 未勾选 | — | 分离场为“关”时置灰；选择“高场优先”或“低场优先”后可用。 |
| 12 | `dropdown` | 移除 Pulldown: | Remove Pulldown: | 关 | 关 / WSSWW / SSWWW / SWWWS / WWWSS / WWSSW / WWWSW - 24P 高级 / WWSWW - 24P 高级 / WSWWW - 24P 高级 / SWWWW - 24P 高级 / WWWWS - 24P 高级 | 静帧时置灰；视频/序列需启用分离场后用于 3:2 / 24Pa 下拉移除。 |
| 13 | `button_row` | 猜测 3:2 Pulldown / 猜测 24Pa Pulldown | Guess 3:2 Pulldown / Guess 24Pa Pulldown | — | 按钮: 猜测 3:2 Pulldown / 猜测 24Pa Pulldown | 静帧时均置灰；序列/视频按场分离与帧率条件启用。 |
| 14 | `dropdown` | 像素长宽比: | Pixel Aspect Ratio: | 方形像素 | 0.01–100.0；方形像素 / D1/DV NTSC (0.91) / D1/DV NTSC 宽银幕  (1.21) / D1/DV PAL (1.09) / D1/DV PAL 宽银幕  (1.46) / HDV 1080/DVCPRO HD 720 (1.33) / DVCPRO HD 1080 (1.5) / 变形 2:1 (2) | — |
| 15 | `number_input` | 循环: | Loop: | 1 | 1–9999 times | 静帧图像时置灰；序列/视频可用，ExtendScript 实测范围 1–9999。 |
| 16 | `button` | 更多选项... | More Options... | — | — | 仅当格式提供专属解码选项（如 Cineon/DPX/EXR/R3D/ProRes RAW）时可用；PNG 序列与静帧置灰。 |

### 4.2 页签：颜色（Color）

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `checkbox` | 保留 RGB（禁用此项的颜色管理转换） | Preserve RGB (Disable Color Management Conversions for This Item) | 未勾选 | — | 项目未开启色彩管理时置灰；开启项目色彩管理后可用。 |
| 2 | `static_text` | 媒体色彩空间： | Media Color Space: | 无 | — | — |
| 3 | `dropdown` | 覆盖媒体色彩空间： | Override Media Color Space: | 工作色彩空间 - 无 | 共 105 项（工作空间/嵌入/其他 + ICC 列表） | 项目未开启色彩管理或勾选“保留 RGB”时置灰。 |
| 4 | `checkbox` | 全部显示 | Show All | 未勾选 | — | 项目启用 OCIO 色彩管理时可用，Adobe 模式下置灰。 |
| 5 | `dropdown` | 解释为线性光： | Interpret As Linear Light: | 对 32 bpc 打开 | 关 / 开 / 对 32 bpc 打开 | — |

## 5. 时间延长（Time Stretch）

- **结构化数据**：[`dialogs/time-stretch.json`](dialogs/time-stretch.json)
- **入口**：`图层 ▸ 时间 ▸ 时间伸缩(C)...`（无默认快捷键） — 菜单显示“时间伸缩(C)...”，弹窗标题实测为“时间延长”。
- **窗口尺寸**：`685 × 509` 物理像素 @ 144 DPI（`457 × 339` 逻辑像素 @ 96 DPI）
- **页签**：无（单页对话框）
- **底部按钮栏**：右下 `确定`（OK） + `取消`（Cancel） 水平并排。
- **记住上次的值**：拉伸因数与新持续时间始终读取当前选中图层的实时 stretch/duration（默认 100%）；“固定位置”单选默认“图层进入点”（同会话记住上次选择）。

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `static_text` | 原持续时间: | Original Duration: | <当前图层未拉伸持续时间> | — | — |
| 2 | `number_scrub` | 拉伸因数 - | Stretch Factor: | 100 | -9900–9900 % | ExtendScript layer.stretch 实测范围为 -9900% 至 +9900%；(-1, 1) 区间会被钳位到 ±1%（负值表示时间倒放）；与“新持续时间”双向联动。 |
| 3 | `timecode_scrub` | 新持续时间: | New Duration: | <当前图层持续时间> | — | 与“拉伸因数”双向联动，右侧只读显示合成帧率基准（如“是 0:00:10:00  基础 30”）。 |
| 4 | `radio_group` | 固定位置 | Hold in Place | 图层进入点 | 图层进入点 / 当前帧 / 图层输出点 | — |

## 6. 自动方向（Auto-Orientation）

- **结构化数据**：[`dialogs/auto-orient.json`](dialogs/auto-orient.json)
- **入口**：`图层 ▸ 变换(T) ▸ 自动定向...`（`Ctrl+Alt+O`） — 菜单显示“自动定向...”，弹窗标题实测为“自动方向”。
- **窗口尺寸**：`493 × 388` 物理像素 @ 144 DPI（`329 × 259` 逻辑像素 @ 96 DPI）
- **页签**：无（单页对话框）
- **底部按钮栏**：右下 `确定`（OK） + `取消`（Cancel） 水平并排。
- **记住上次的值**：直接读取并写回当前选中图层的 autoOrient 属性（新建图层默认“关”）。

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `radio_group` | 自动方向模式 | Auto-Orientation Mode | 关 | 关 / 沿路径定向 / 定位于摄像机 | 关：2D 与 3D 图层均显示；沿路径定向：2D 与 3D 图层均显示；定位于摄像机：仅 3D 图层显示；2D 图层隐藏该项（不渲染）。 |
| 2 | `checkbox` | 独立定向每个字符（需要逐字 3D 化） | Orient Each Character Independently (requires Per-character 3D) | 未勾选 | — | 2D 图层完全隐藏；3D 非文字图层显示但置灰禁用；仅 3D 文字图层启用可勾选（与“定位于摄像机”配合，可自动开启逐字 3D）。 |
