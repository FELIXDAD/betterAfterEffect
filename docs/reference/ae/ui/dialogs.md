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
| 7 | 关键帧插值（Keyframe Interpolation） | `Ctrl+Alt+K` | [`dialogs/keyframe-interpolation.json`](dialogs/keyframe-interpolation.json) |
| 8 | 关键帧速度（Keyframe Velocity） | `Ctrl+Shift+K` | [`dialogs/keyframe-velocity.json`](dialogs/keyframe-velocity.json) |
| 9 | 转到时间（Go to Time） | `Alt+Shift+J` | [`dialogs/go-to-time.json`](dialogs/go-to-time.json) |
| 10 | 合成标记 / 图层标记（Composition Marker / Layer Marker） | `Alt+Numpad * / Alt+Numpad *` | [`dialogs/markers.json`](dialogs/markers.json) |
| 11 | <调用属性名>（如“背景颜色”、“灯光颜色”、“颜色”）（<Property Name> (e.g. Background Color, Light Color, Color)） | `点击任意颜色色块（如合成设置背景颜色、纯色/灯光/效果/文字颜色）` | [`dialogs/color-picker.json`](dialogs/color-picker.json) |
| 12 | 渲染设置（Render Settings） | `渲染队列面板 ▸ 点击渲染设置蓝字（如“最佳设置”） / 编辑 ▸ 模板 ▸ 渲染设置(R)... ▸ 编辑...` | [`dialogs/render-settings.json`](dialogs/render-settings.json) |
| 13 | 输出模块设置（Output Module Settings） | `渲染队列面板 ▸ 点击输出模块蓝字（如“H.264 - 匹配渲染设置 - 40 Mbps”） / 编辑 ▸ 模板 ▸ 输出模块(M)... ▸ 编辑...` | [`dialogs/output-module-settings.json`](dialogs/output-module-settings.json) |
| 14 | 键盘快捷键（Keyboard Shortcuts） | `Ctrl+Alt+'` | [`dialogs/keyboard-shortcuts.json`](dialogs/keyboard-shortcuts.json) |
| 15 | 导入文件（及文件 ▸ 导入 选项）（Import File (and File > Import Options)） | `Ctrl+I / Ctrl+Alt+I` | [`dialogs/import-file.json`](dialogs/import-file.json) |

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
| 3 | `dropdown` | 覆盖媒体色彩空间： | Override Media Color Space: | 工作色彩空间 - 无 | 共 105 项（工作色彩空间 - <当前工作空间>/嵌入/其他 + ICC 列表） | 项目未开启色彩管理或勾选“保留 RGB”时置灰。 |
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

## 7. 关键帧插值（Keyframe Interpolation）

- **结构化数据**：[`dialogs/keyframe-interpolation.json`](dialogs/keyframe-interpolation.json)
- **入口**：`动画 ▸ 关键帧插值...`（`Ctrl+Alt+K`）
- **窗口尺寸**：`474 × 422` 物理像素 @ 144 DPI（`316 × 281` 逻辑像素 @ 96 DPI）
- **页签**：无（单页对话框）
- **底部按钮栏**：右下 `确定`（OK） + `取消`（Cancel） 水平并排。
- **记住上次的值**：打开时直接反映所选关键帧的当前插值类型（多选混合状态时显示“当前设置”），不记住上次弹窗选择。

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `dropdown` | 临时插值: | Temporal Interpolation: | 线性 | 当前设置 / 线性 / 贝塞尔曲线 / 连续贝塞尔曲线 / 自动贝塞尔曲线 / 定格 | 选中任意关键帧时始终可用（注：zh_CN 界面将 Temporal 译为“临时插值:”）。 |
| 2 | `dropdown` | 空间插值: | Spatial Interpolation: | 线性 | 当前设置 / 线性 / 贝塞尔曲线 / 连续贝塞尔曲线 / 自动贝塞尔曲线 | 仅空间属性（位置/锚点/目标点/效果点等）可用；非空间属性（缩放/不透明度/旋转）置灰禁用且值为空；不含“定格”项。 |
| 3 | `dropdown` | 浮动: | Roving: | 锁定到时间 | 当前设置 / 漂浮穿梭时间 / 锁定到时间 | 仅空间属性的中间关键帧（非首帧、非末帧）可用；非空间属性或首/末关键帧置灰禁用。 |

## 8. 关键帧速度（Keyframe Velocity）

- **结构化数据**：[`dialogs/keyframe-velocity.json`](dialogs/keyframe-velocity.json)
- **入口**：`动画 ▸ 关键帧速度...`（`Ctrl+Shift+K`）
- **窗口尺寸**：`904 × 401` 物理像素 @ 144 DPI（`603 × 267` 逻辑像素 @ 96 DPI）
- **页签**：无（单页对话框）
- **底部按钮栏**：右下 `确定`（OK） + `取消`（Cancel） 水平并排。
- **记住上次的值**：实时读取当前选中关键帧的传入/传出速度、影响百分比与连续性状态；不同属性维度会动态调整窗口高度与行数。

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `static_text` | 关键帧类型: <属性名> | Keyframe Type: <Property Name> | — | — | 实测显示“关键帧类型: 位置”、“关键帧类型: 缩放”、“关键帧类型: 不透明度”。 |
| 2 | `checkbox_pair` | 锁定维度（传入 / 传出各一） | Lock Dimensions (Incoming & Outgoing) | 未勾选 | — | 仅非空间多维属性（如 2D/3D 缩放）显示；单维属性（不透明度）与空间属性（位置）隐藏此控件。 |
| 3 | `number_scrub_group` | 传入速度（空间显示“速度:”；多维按“维度: X/Y/Z”分行） | Incoming Velocity (Speed: for spatial; Dimension: X/Y/Z rows for multi-dim) | <当前关键帧传入速度> | property_units/sec (px/sec for spatial) | 左右双栏对称排布（左栏传入速度，右栏传出速度）；线性位置关键帧显示路径标量速率（px/s），缩放/不透明度显示带符号变化率（%/s）。 |
| 4 | `number_scrub_group` | 传入影响: | Incoming Influence: | 33.33 | 0.1–100.0 % | ExtendScript KeyframeEase 实测硬边界为 0.1–100%；线性关键帧在 UI 中初始显示 0.01%，缓动关键帧默认 33.33%。 |
| 5 | `number_scrub_group` | 传出速度（结构同左栏） | Outgoing Velocity (mirrors Incoming) | <当前关键帧传出速度> | property_units/sec (px/sec for spatial) | — |
| 6 | `number_scrub_group` | 传出影响: | Outgoing Influence: | 33.33 | 0.1–100.0 % | — |
| 7 | `checkbox` | 连续（锁定传出到传入） | Continuous (Lock Outgoing to Incoming) | 未勾选 | — | 勾选后自动将传出速度锁定为与传入速度一致（转为连续贝塞尔）。 |
| 8 | `static_text` | 对于在时间 <时间码> 的关键帧 #<序号> | For keyframe #<Index> at time <Timecode> | — | — | 右下按钮上方只读提示当前编辑的关键帧序号与时间位置。 |

## 9. 转到时间（Go to Time）

- **结构化数据**：[`dialogs/go-to-time.json`](dialogs/go-to-time.json)
- **入口**：`视图 ▸ 转到时间(G)...`（`Alt+Shift+J`）
- **窗口尺寸**：`610 × 229` 物理像素 @ 144 DPI（`407 × 153` 逻辑像素 @ 96 DPI）
- **页签**：无（单页对话框）
- **底部按钮栏**：右下 `确定`（OK） + `取消`（Cancel） 水平并排。
- **记住上次的值**：每次打开自动填入当前合成的播放头时间（CTI）并全选，不保留上次输入值。

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `timecode_input` | 时间码输入 | Time Input | <当前合成播放头时间> | — | 支持绝对时间码/帧数输入与相对偏移（如 +10、-15 帧，+2. 表示 +2 秒）；右侧静态文本实时显示解析结果与帧率基准（如“是 0:00:02:00  基础 30”）。 |

## 10. 合成标记 / 图层标记（Composition Marker / Layer Marker）

- **结构化数据**：[`dialogs/markers.json`](dialogs/markers.json)
- **入口**：`未选中图层时按 Alt+Numpad *（或双击合成标记）`（`Alt+Numpad *`）；`选中图层时按 Alt+Numpad *（或双击图层标记）`（`Alt+Numpad *`）
- **窗口尺寸**：`673 × 572` 物理像素 @ 144 DPI（`449 × 381` 逻辑像素 @ 96 DPI）
- **变体（2 个）**：`合成标记（标题：合成标记）`（Composition Marker）｜`图层标记（标题：图层标记）`（Layer Marker）
- **底部按钮栏**：右下 `确定`（OK） + `取消`（Cancel） 水平并排。
- **记住上次的值**：新建标记默认时间为当前播放头、持续时间 0:00:00:00、注释为空、标签为“无”；AE 26 已精简移除旧版章节/Web 链接/Flash 提示点字段。

### 10.1 变体：合成标记（标题：合成标记）（Composition Marker）

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `timecode_scrub` | 时间: | Time: | <当前播放头时间> | — | 右侧只读显示解析时间与基准帧率（如“是 0:00:02:00  基础 30”）。 |
| 2 | `timecode_scrub` | 持续时间: | Duration: | 0:00:00:00 | — | 非零值会在时间轴上显示带范围条的区间标记。 |
| 3 | `multiline_text` | 注释: | Comment: |  | — | — |
| 4 | `dropdown` | 标签: | Label: | 无 | 无 / 红色 / 黄色 / 浅绿色 / 粉色 / 淡紫色 / 桃红色 / 海泡沫 / 蓝色 / 绿色 / 紫色 / 橙色 / 棕色 / 紫红色 / 青色 / 砂岩 / 深绿色 | 共 17 项：无 + 16 个 AE 标签颜色。 |
| 5 | `checkbox` | 受保护区域 | Protected Region | 未勾选 | — | 合成标记独有控件；用于响应设计时间（Responsive Design - Time）锁定时间段不被拉伸。 |

### 10.2 变体：图层标记（标题：图层标记）（Layer Marker）

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `timecode_scrub` | 时间: | Time: | <当前播放头时间> | — | — |
| 2 | `timecode_scrub` | 持续时间: | Duration: | 0:00:00:00 | — | — |
| 3 | `multiline_text` | 注释: | Comment: |  | — | — |
| 4 | `dropdown` | 标签: | Label: | 无 | 无 / 红色 / 黄色 / 浅绿色 / 粉色 / 淡紫色 / 桃红色 / 海泡沫 / 蓝色 / 绿色 / 紫色 / 橙色 / 棕色 / 紫红色 / 青色 / 砂岩 / 深绿色 | — |
| 5 | `radio_group` | 标记类型 | Marker Type | 注释标记 | 注释标记 / 分段标记 | 图层标记独有控件（替代合成标记的“受保护区域”）；分段标记用于时间重映射/关键帧分段。 |

## 11. <调用属性名>（如“背景颜色”、“灯光颜色”、“颜色”）（<Property Name> (e.g. Background Color, Light Color, Color)）

- **结构化数据**：[`dialogs/color-picker.json`](dialogs/color-picker.json)
- **入口**：`点击任意颜色色块（如合成设置背景颜色、纯色/灯光/效果/文字颜色）`（无默认快捷键）
- **窗口尺寸**：`738 × 493` 物理像素 @ 144 DPI（`492 × 329` 逻辑像素 @ 96 DPI）
- **页签**：无（单页对话框）
- **按钮栏**：右侧顶部纵向排列 `确定`（OK） / `取消`（Cancel）（按钮位于对话框右侧顶部纵向排列（确定在上、取消在下），下方同列依次为吸管图标按钮与预览复选框。）
- **记住上次的值**：记住上次选中的色彩轴单选（默认 H）及窗口屏幕位置；颜色值与预览状态实时绑定调用方属性。

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `custom_color_canvas` | 二维色域面板 + 垂直色相/通道滑条 + 新/旧色块对比 | 2D Color Field + Vertical Channel Slider + New/Current Swatches | — | — | 位于左侧区域；垂直滑条与二维色域随右侧 H/S/B/R/G/B 单选切换坐标轴。 |
| 2 | `radio_group` | 色彩轴单选（H / S / B / R / G / B） | Color Axis Radio (H / S / B / R / G / B) | H | H / S / B / R / G / B | — |
| 3 | `number_scrub_group` | H / S / B 数值 | H / S / B Numeric Scrubs | — | H: 0–360°, S: 0–100%, B: 0–100% | — |
| 4 | `number_scrub_group` | R / G / B 数值 | R / G / B Numeric Scrubs | — | 8 bpc: 0–255; 16 bpc: 0–32768; 32 bpc float: 0.0–1.0+ (HDR) | 实测 32 bpc 项目的灯光/效果颜色自动切为浮点 0.0–1.0+（白色显示 1/1/1）；合成背景颜色在 32 bpc 下仍保持 8 位 0–255。 |
| 5 | `text_input` | # | # | 000000 | — | 6 位十六进制 RGB 颜色码（如 FFFFFF / 000000），与 HSB/RGB 实时双向同步。 |
| 6 | `icon_button` | 吸管 | Eyedropper | — | — | 位于右列取消按钮下方，支持从屏幕任意位置取色。 |
| 7 | `checkbox` | 预览 | Preview | 勾选 | — | 默认勾选，位于右列吸管下方，实时将颜色变化反馈到合成查看器。 |

## 12. 渲染设置（Render Settings）

- **结构化数据**：[`dialogs/render-settings.json`](dialogs/render-settings.json)
- **入口**：`渲染队列面板 ▸ 点击渲染设置蓝字（如“最佳设置”）`（无默认快捷键）；`编辑 ▸ 模板 ▸ 渲染设置(R)... ▸ 编辑...`（无默认快捷键）
- **窗口尺寸**：`957 × 899` 物理像素 @ 144 DPI（`638 × 599` 逻辑像素 @ 96 DPI）
- **页签**：无（单页对话框）
- **底部按钮栏**：右下 `确定`（OK） + `取消`（Cancel） 水平并排。
- **记住上次的值**：单次弹窗修改仅作用于当前渲染队列项；新建渲染队列项按“渲染设置模板”对话框中指定的默认模板（影片默认“最佳设置”、帧默认“当前设置”、预渲染默认“最佳设置”、影片代理默认“草图设置”、静止代理默认“最佳设置”）初始化，模板本身跨重启持久化。

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `dropdown` | 品质: | Quality: | 最佳 | 当前设置 / 最佳 / 草图 / 线框 | 默认值以“最佳设置”模板为例。 |
| 2 | `dropdown` | 分辨率: | Resolution: | 完整 | 当前设置 / 完整 / 二分之一 / 三分之一 / 四分之一 / 自定义... | 选择“自定义...”时弹出自定义分辨率子对话框，指定水平/垂直采样步长；下方只读显示实际渲染尺寸。 |
| 3 | `dropdown` | 磁盘缓存: | Disk Cache: | 只读 | 当前设置 / 只读 | — |
| 4 | `dropdown` | 代理使用: | Proxy Use: | 不使用代理 | 当前设置 / 使用所有代理 / 仅使用合成代理 / 不使用代理 | — |
| 5 | `dropdown` | 效果: | Effects: | 当前设置 | 当前设置 / 全部开启 / 全部关闭 | — |
| 6 | `dropdown` | 单独开关: | Solo Switches: | 当前设置 | 当前设置 / 全部关闭 | — |
| 7 | `dropdown` | 引导层: | Guide Layers: | 全部关闭 | 当前设置 / 全部关闭 | — |
| 8 | `dropdown` | 颜色深度: | Color Depth: | 当前设置 | 当前设置 / 每通道 8 位 / 每通道 16 位 / 每通道 32 位 | — |
| 9 | `dropdown` | 帧混合: | Frame Blending: | 对选中图层打开 | 当前设置 / 对选中图层打开 / 对所有图层关闭 | 位于下半区“时间采样”（Time Sampling）分组。 |
| 10 | `dropdown` | 场渲染: | Field Render: | 关 | 关 / 高场优先 / 低场优先 | — |
| 11 | `dropdown` | 3:2 Pulldown: | 3:2 Pulldown: | 关 | 关 / WSSWW / SSWWW / SWWWS / WWWSS / WWSSW | “场渲染”为“关”时置灰禁用（实测 NOFOCUS）；选择高场优先或低场优先后启用。 |
| 12 | `dropdown` | 运动模糊: | Motion Blur: | 对选中图层打开 | 当前设置 / 对选中图层打开 / 对所有图层关闭 | — |
| 13 | `dropdown` | 时间跨度: | Time Span: | 仅工作区域 | 合成长度 / 仅工作区域 / 自定义... | 选择“自定义...”时弹出自定义时间跨度子对话框（开始/结束/持续时间）。 |
| 14 | `radio_group` | 帧速率 | Frame Rate | 使用合成的帧速率 | 使用合成的帧速率 / 使用此帧速率: | — |
| 15 | `number_scrub` | 使用此帧速率（数值） | Use this frame rate (value) | 30 | 1–999 fps | 选中“使用此帧速率:”或直接编辑数值时自动切换至自定义帧率。 |
| 16 | `checkbox` | 跳过现有文件（允许多机渲染） | Skip existing files (allows multi-machine rendering) | 未勾选 | — | 位于底部“选项”分组，仅序列输出时生效。 |

### 12.1 关联对话框：渲染设置模板（Render Settings Templates）

- **入口**：`编辑 ▸ 模板 ▸ 渲染设置(R)...`；**窗口尺寸**：`1185 × 851` 物理像素 @ 144 DPI（`790 × 567` 逻辑像素 @ 96 DPI）
- **内置模板（5 个）**：`DV 设置`（DV Settings）、`多机设置`（Multi-Machine Settings）、`当前设置`（Current Settings）、`最佳设置`（Best Settings）、`草图设置`（Draft Settings）
- **底部按钮栏**：左下 `全部保存...`（Save All...） / `加载...`（Load...）；右下 `确定`（OK） + `取消`（Cancel） 水平并排。

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `dropdown` | 影片默认值: | Movie Default: | 最佳设置 | — | — |
| 2 | `dropdown` | 帧默认值: | Frame Default: | 当前设置 | — | — |
| 3 | `dropdown` | 预渲染默认值: | Pre-Render Default: | 最佳设置 | — | — |
| 4 | `dropdown` | 影片代理默认值: | Movie Proxy Default: | 草图设置 | — | — |
| 5 | `dropdown` | 静止代理默认值: | Still Proxy Default: | 最佳设置 | — | — |
| 6 | `dropdown` | 设置名称: | Settings Name: | 最佳设置 | 按钮: 新建... / 编辑... / 复制 / 删除 | — |

## 13. 输出模块设置（Output Module Settings）

- **结构化数据**：[`dialogs/output-module-settings.json`](dialogs/output-module-settings.json)
- **入口**：`渲染队列面板 ▸ 点击输出模块蓝字（如“H.264 - 匹配渲染设置 - 40 Mbps”）`（无默认快捷键）；`编辑 ▸ 模板 ▸ 输出模块(M)... ▸ 编辑...`（无默认快捷键）
- **窗口尺寸**：`922 × 1090` 物理像素 @ 144 DPI（`615 × 727` 逻辑像素 @ 96 DPI）
- **页签（2 个）**：`主要选项`（Main Options）｜`颜色`（Color）
- **底部按钮栏**：右下 `确定`（OK） + `取消`（Cancel） 水平并排。
- **记住上次的值**：单次弹窗修改仅作用于当前输出模块；新建输出模块按“输出模块模板”中指定的默认模板初始化（影片与保存当前预览默认“H.264 - 匹配渲染设置 - 40 Mbps”、帧默认“Photoshop”、预渲染与影片代理默认“无损”），模板跨重启持久化。

### 13.1 页签：主要选项（Main Options）

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `dropdown` | 格式: | Format: | H.264 | AIFF / AVI / DPX/Cineon 序列 / H.264 / IFF 序列 / JPEG 序列 / MP3 / OpenEXR 序列 / PNG 序列 / Photoshop 序列 / QuickTime / Radiance 序列 / SGI 序列 / TIFF 序列 / Targa 序列 / WAV | 共 16 种输出格式；切换为纯音频格式（AIFF/MP3/WAV）时禁用视频输出分组，切换为图像序列时启用“开始 #”和“使用合成帧编号”。 |
| 2 | `checkbox` | 包括项目链接 | Include Project Link | 勾选 | — | — |
| 3 | `dropdown` | 渲染后动作: | Post-Render Action: | 无 | 无 / 导入 / 导入和替换用法 / 设置代理 | — |
| 4 | `checkbox` | 包括源 XMP 元数据 | Include Source XMP Metadata | 未勾选 | — | — |
| 5 | `checkbox` | 视频输出 | Video Output | 勾选 | — | 取消勾选或选择纯音频格式时，下方通道/深度/颜色/格式选项/调整大小/裁剪全部置灰。 |
| 6 | `dropdown` | 通道: | Channels: | RGB | RGB / Alpha / RGB + Alpha | 格式不支持 Alpha（如 H.264/JPEG）时仅 RGB 可选或回退为 RGB。 |
| 7 | `dropdown` | 深度: | Depth: | 数百万种颜色 | 256 色 / 数百万种颜色 / 数百万种以上颜色 / 数万亿种颜色 / 数万亿种以上颜色 / 浮点 / 浮点+ / 256 种灰色 / 浮点灰度 | 带“+”项表示含 Alpha 通道，与“通道”联动过滤。 |
| 8 | `dropdown` | 颜色: | Color: | 预乘（遮罩） | 直接（无遮罩） / 预乘（遮罩） | 仅当通道包含 Alpha 时对输出有实质影响。 |
| 9 | `number_scrub` | 开始 #: | Starting #: | 0 | 0–9999999 | 影片格式（如 H.264/QuickTime/AVI）下置灰禁用；图像序列且未勾选“使用合成帧编号”时启用。 |
| 10 | `checkbox` | 使用合成帧编号 | Use Comp Frame Number | 勾选 | — | 影片格式下置灰禁用；图像序列下启用，勾选时覆盖“开始 #”。 |
| 11 | `button` | 格式选项... | Format Options... | — | — | 打开当前视频格式的编解码器设置子对话框；按钮下方只读显示编码摘要（H.264 默认模板显示“VBR，目标 40 Mbps”）。 |
| 12 | `checkbox` | 调整大小 | Resize | 未勾选 | — | 未勾选时，锁定长宽比、宽高数值框、预设下拉、调整大小后品质全部置灰。 |
| 13 | `checkbox` | 锁定长宽比为 <W>:<H> (<ratio>) | Lock Aspect Ratio to <W>:<H> (<ratio>) | 勾选 | — | 仅勾选“调整大小”后启用。 |
| 14 | `number_scrub` | 调整大小到（宽度） | Resize to (Width) | 320 | 4–30000 px | 仅勾选“调整大小”后启用；模板内默认 320×240，实际队列项默认同步当前合成尺寸。 |
| 15 | `number_scrub` | 调整大小到（高度） | Resize to (Height) | 240 | 4–30000 px | 仅勾选“调整大小”后启用。 |
| 16 | `dropdown` | 调整大小预设 | Resize Preset | 自定义 | 共 26 项 | 共 26 项（自定义 + 与合成设置完全相同的 25 个内置预设）；仅勾选“调整大小”后启用。 |
| 17 | `dropdown` | 调整大小后的品质: | Resize Quality: | 高 | 低 / 高 | 仅勾选“调整大小”后启用。 |
| 18 | `checkbox` | 裁剪 | Crop | 未勾选 | — | 未勾选时，“使用目标区域”和顶部/左侧/底部/右侧数值框全部置灰。 |
| 19 | `checkbox` | 使用目标区域 | Use Region of Interest | 未勾选 | — | 仅勾选“裁剪”后启用；勾选时自动填充合成当前 ROI 边界。 |
| 20 | `number_scrub_group` | 顶部 / 左侧 / 底部 / 右侧 | Top / Left / Bottom / Right | 0 | px | 仅勾选“裁剪”后启用；正值向内裁剪、负值向外扩展。 |
| 21 | `dropdown` | 音频输出 | Audio Output | 自动音频输出 | 打开音频输出 / 自动音频输出 / 关闭音频输出 | 选择“关闭音频输出”或格式不支持音频（图像序列）时，下方采样率/位深/声道/格式选项置灰。 |
| 22 | `dropdown` | 音频采样率 | Audio Sample Rate | 48.000 kHz | 16.000 kHz / 22.050 kHz / 24.000 kHz / 32.000 kHz / 44.100 kHz / 48.000 kHz | — |
| 23 | `dropdown` | 音频位深度 | Audio Bit Depth | 16 位 | 8 位 / 16 位 / 32 位 | — |
| 24 | `dropdown` | 音频声道 | Audio Channels | 立体声 | 单声道 / 立体声 | — |
| 25 | `button` | 格式选项...（音频） | Format Options... (Audio) | — | — | — |

### 13.2 页签：颜色（Color）

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `checkbox` | 保留 RGB（禁用此项的颜色管理转换） | Preserve RGB (Disable Color Management Conversions for This Item) | 未勾选 | — | 勾选后禁用输出色彩空间转换与下拉选择。 |
| 2 | `dropdown` | 输出色彩空间： | Output Color Space: | 工作空间 | 共 104 项（工作空间/其他 + ICC 列表） | 项目未开启色彩管理或勾选“保留 RGB”时置灰（实测 Adobe 默认无色彩管理下 NOFOCUS）。 |
| 3 | `checkbox` | 全部显示 | Show All | 未勾选 | — | 仅项目启用 OCIO 色彩管理时启用，Adobe 模式下置灰。 |
| 4 | `dropdown` | 转换为线性光: | Convert to Linear Light: | 对 32 bpc 打开 | 关 / 开 / 对 32 bpc 打开 | — |
| 5 | `checkbox` | 嵌入色彩空间 | Embed Color Profile | 未勾选 | — | 仅当输出格式支持嵌入 ICC 配置文件（如 PNG/TIFF/PSD/JPEG/QuickTime）时启用；H.264 等不支持格式下置灰禁用。 |

### 13.3 关联对话框：输出模块模板（Output Module Templates）

- **入口**：`编辑 ▸ 模板 ▸ 输出模块(M)...`；**窗口尺寸**：`1314 × 779` 物理像素 @ 144 DPI（`876 × 519` 逻辑像素 @ 96 DPI）
- **内置模板（12 个）**：`多机序列`（Multi-Machine Sequence）、`带有 Alpha 的 TIFF 序列`（TIFF Sequence with Alpha）、`AIFF 48kHz`（AIFF 48kHz）、`H.264 - 匹配渲染设置 - 15 Mbps`（H.264 - Match Render Settings - 15 Mbps）、`H.264 - 匹配渲染设置 - 40 Mbps`（H.264 - Match Render Settings - 40 Mbps）、`H.264 - 匹配渲染设置 - 5 Mbps`（H.264 - Match Render Settings - 5 Mbps）、`Photoshop`（Photoshop）、`具有 Alpha 的高品质`（High Quality with Alpha）、`使用 Alpha 无损耗`（Lossless with Alpha）、`仅 Alpha`（Alpha Only）、`无损`（Lossless）、`高品质`（High Quality）
- **底部按钮栏**：左下 `全部保存...`（Save All...） / `加载...`（Load...）；右下 `确定`（OK） + `取消`（Cancel） 水平并排。

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `dropdown` | 影片默认值: | Movie Default: | H.264 - 匹配渲染设置 - 40 Mbps | — | — |
| 2 | `dropdown` | 帧默认值: | Frame Default: | Photoshop | — | — |
| 3 | `dropdown` | 保存当前预览默认值: | Save Current Preview Default: | H.264 - 匹配渲染设置 - 40 Mbps | — | — |
| 4 | `dropdown` | 预渲染默认值: | Pre-Render Default: | 无损 | — | — |
| 5 | `dropdown` | 影片代理默认值: | Movie Proxy Default: | 无损 | — | — |
| 6 | `dropdown` | 设置名称: | Settings Name: | H.264 - 匹配渲染设置 - 40 Mbps | 按钮: 新建... / 编辑... / 复制 / 删除 | — |

## 14. 键盘快捷键（Keyboard Shortcuts）

- **结构化数据**：[`dialogs/keyboard-shortcuts.json`](dialogs/keyboard-shortcuts.json)
- **入口**：`编辑 ▸ 键盘快捷键`（`Ctrl+Alt+'`）
- **窗口尺寸**：`2302 × 1339` 物理像素 @ 144 DPI（`1535 × 893` 逻辑像素 @ 96 DPI）
- **页签**：无（单页对话框）
- **底部按钮栏**：右下 `确定`（OK） + `取消`（Cancel） 水平并排。
- **记住上次的值**：所选快捷键预设与自定义绑定跨重启持久化保存为用户目录 `aeks/` 下的 `.txt` 预设文件；修改内置 `After Effects 默认值.txt` 时自动切换为 `自定义.txt`（或用户另存的新预设）。

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `dropdown` | 预设: | Keyboard Layout Preset: | After Effects 默认值.txt | After Effects 默认值.txt / 自定义.txt | 列出内置只读预设及所有用户自定义 `.txt` 预设文件。 |
| 2 | `button` | 另存为... | Save As... | — | — | — |
| 3 | `button` | 删除 | Delete | — | — | 选中内置 `After Effects 默认值.txt` 时置灰禁用；选中自定义/用户预设时可用。 |
| 4 | `button` | 复制到剪贴板 | Copy to Clipboard | — | — | 将当前预设的完整快捷键表复制为纯文本。 |
| 5 | `dropdown` | 键盘布局 - | Keyboard Layout: | en | en / pt / ja / ko / sv / ru / fr / es / nb / it / de | 共 11 种物理键盘区域布局代码，仅影响可视化键盘绘制与键帽标签。 |
| 6 | `dropdown` | 命令: | Commands: | 应用程序 | 应用程序 / 合成面板 / 图层面板 / 字符和段落面板 / 应用 / 效果控件面板 / 时间轴面板 / 查看器面板 / 流程图面板 / 渲染队列面板 / 绘画面板 / 跟踪器面板 / 项目面板 | 共 13 个作用域；切换后联动刷新可视化键盘高亮（紫色=全局应用快捷键，绿色=当前面板覆盖快捷键）。 |
| 7 | `custom_keyboard_map` | 可视化键盘布局 | Visual Keyboard Layout | — | — | 按主键盘、功能区、方向键、小键盘分布；点击任意键位可在右下查看其全部修饰键组合，支持从下方命令树拖拽到键帽上绑定；下方附 3 色图例（应用全局快捷键 / 面板焦点覆盖快捷键 / 按下辅助按键）。 |
| 8 | `text_input` | 搜索框 | Search |  | — | 按命令名称或快捷键文字实时过滤下方命令树。 |
| 9 | `tree_table` | 命令与快捷键列表 | Command / Shortcut List | — | — | 位于左下区域（宽 1170 px @ 144 DPI），按菜单与面板分组折叠展示命令与绑定快捷键胶囊，支持双击/点击胶囊录制或删除多个快捷键。 |
| 10 | `table` | 键: <所选键位> | Key: <Selected Key> | — | — | 位于右下区域，展示当前所选物理键搭配 Ctrl / Shift / Alt 等所有修饰键组合的对应命令及冲突提示。 |

## 15. 导入文件（及文件 ▸ 导入 选项）（Import File (and File > Import Options)）

- **结构化数据**：[`dialogs/import-file.json`](dialogs/import-file.json)
- **入口**：`文件 ▸ 导入(I) ▸ 文件...`（`Ctrl+I`）；`文件 ▸ 导入(I) ▸ 多个文件...`（`Ctrl+Alt+I`）
- **窗口尺寸**：`1800 × 1461` 物理像素 @ 144 DPI（`1200 × 974` 逻辑像素 @ 96 DPI）
- **页签**：无（单页对话框）
- **底部按钮栏**：右下 `导入文件夹`（Import Folder） + `导入`（Import） + `取消`（Cancel） 水平并排。
- **记住上次的值**：Windows 原生文件对话框记住上次浏览目录与窗口尺寸；“导入为”与“创建合成”在同会话内按所选文件类型动态联动（“多个文件...”会在导入一批后循环重新打开弹窗直至点击“完成”/取消）。

### 15.1 `文件 ▸ 导入(I)` 子菜单条目

| 顺序 | 中文菜单项 | 英文菜单项 | 默认快捷键 | Win32 Command ID |
|---:|---|---|---|---|
| 1 | `文件...` | `File...` | `Ctrl+I` | `9764865` |
| 2 | `多个文件...` | `Multiple Files...` | `Ctrl+Alt+I` | `9764866` |
| 3 | `从 Libraries 中...` | `From Libraries...` | — | `9764867` |
| 4 | `导入 Adobe Premiere 项目...` | `Adobe Premiere Pro Project...` | — | `9764868` |
| 5 | `Vanishing Point (.vpe)...` | `Vanishing Point (.vpe)...` | — | `9764869` |
| 6 | `占位符...` | `Placeholder...` | — | `9764870` |
| 7 | `纯色...` | `Solid...` | — | `9764871` |

### 15.2 对话框控件（Dialog Controls）

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `dropdown` | 格式: | Enable: / Format: | <按所选文件自动检测> | — | 未选中文件时置灰禁用（实测 NOFOCUS）；选中受支持文件后按文件扩展名自动匹配对应导入器。 |
| 2 | `dropdown` | 导入为: | Import As: | 素材 | 素材 / 合成 / 合成 - 保持图层大小 | 未选中分层文件（如 PSD/AI/AEP/FLA）时置灰禁用；选中多层文档时启用，选为合成方式时还会弹出图层选项子对话框。 |
| 3 | `checkbox` | <格式> 序列（未选中时显示“序列不可用”） | [Format] Sequence (displays 'Sequence' disabled when unavailable) | 未勾选 | — | 位于“序列选项:”分组；未选中图像文件或所选格式不支持序列时显示“序列不可用”并置灰；选中带编号的静帧图像时自动勾选并显示具体格式（如“PNG 序列”）。 |
| 4 | `checkbox` | 强制按字母顺序排列 | Force alphabetical order | 未勾选 | — | 仅当上方“<格式> 序列”可用且勾选时启用；其余状态置灰禁用。 |
| 5 | `checkbox` | 创建合成 | Create Composition | 未勾选 | — | 位于“导入选项:”分组，始终可用；勾选后导入完成自动以素材参数新建合成。 |
| 6 | `combobox` | 文件名(N): | File name: |  | — | Windows 原生文件对话框底栏输入/历史下拉。 |
| 7 | `dropdown` | 文件类型(T): | Files of type: | 所有可接受的文件 | — | 位于文件名右侧，按格式类别筛选文件列表。 |

### 15.3 关联对话框：新占位符（New Placeholder）

- **入口**：`文件 ▸ 导入(I) ▸ 占位符...`；**窗口尺寸**：`667 × 485` 物理像素 @ 144 DPI（`445 × 323` 逻辑像素 @ 96 DPI）
- **底部按钮栏**：右下 `确定`（OK） + `取消`（Cancel） 水平并排。

| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |
|---:|---|---|---|---|---|---|
| 1 | `text_input` | 名称: | Name: | 缺少名称 | — | — |
| 2 | `number_scrub` | 宽度: | Width: | 1920 | 4–30000 px | — |
| 3 | `number_scrub` | 高度: | Height: | 1080 | 4–30000 px | — |
| 4 | `number_scrub` | 帧速率: | Frame Rate: | 30 | 1–999 fps | — |
| 5 | `timecode_scrub` | 持续时间: | Duration: | 0:01:00:00 | — | 右侧只读显示时间码基准（如“是 0:01:00:00  基础 30”）。 |
