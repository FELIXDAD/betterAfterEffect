# After Effects 26 交互实测：新建图层、时间轴与拖放（第 3 批）

> **数据来源**：Windows 11 Pro x64（3840×2160 @ 150% DPI）下对 Adobe After Effects 26.0（`26.0x67`，`zh_CN`）实机发送 Win32 键鼠事件与 `WM_COMMAND`，由 ExtendScript 实时回读合成、图层、关键帧与效果状态，并采集模态对话框控件树。
> **测试合成基准**：`1920×1080`，`30 fps`，时长 `10 s`（300 帧），缩放 `100%`。
> **实测用例总数**：`48` 个，完整结构化 JSON 见 [`layers-timeline-dnd.json`](layers-timeline-dnd.json)。

## 1. 新建图层（默认值、时间范围与模态对话框）

### 1.1 全局规则（实测确认）
- **时间范围与播放头无关**：无论播放头位于 `t=0` 还是 `t=2.0s`（见 `new_layer_null_at_t2`），所有新建图层默认铺满整个合成：`inPoint = 0`、`outPoint = comp.duration (10)`、`startTime = 0`。
- **选中状态**：新建后仅新图层处于选中状态（原选中图层全部取消选中）。
- **插入位置（实测 4 图层 `[S1..S4]`）**：
  - 未选中任何图层（`insert_null_none_sel`）→ 插入到最顶层 **`index = 1`**。
  - 选中单个图层 `L2`（`insert_null_sel2`、`insert_shape_sel2`）→ 插入到 **`index = 2`（紧邻 `L2` 上方）**，原 `L2` 下移至 `L3`。
  - 多选非连续图层 `L2 + L4`（`insert_null_sel2_sel4`）→ 插入到 **`index = 2`（紧邻最上方选中图层之上）**。

### 1.2 七类新建图层精确默认值（`1920×1080 @ 30fps, 10s`）

| 图层类型 | 快捷键 / `commandId` | 模态框 | 默认名称 (`zh_CN`) | `layerType` | `label` | 尺寸 `(W×H)` | 默认 `AnchorPoint` / POI | 默认 `Position` | `Opacity` | 关键特异点 |
|---|---|---|---|---|---:|---|---|---|---|---|
| 纯色 Solid | `Ctrl+Y` / `11862018` | 有 | `红色 纯色 1` | `AVLayer` | `1` | `1920×1080` | `[960, 540, 0]` | `[960, 540, 0]` | `100` | 尺寸默认与合成一致；记忆上次颜色（本机实测红 `[1,0,0]`） |
| 文本 Text | `Ctrl+Alt+Shift+T` / `11862017` | 无 | `<空文本图层>` | `TextLayer` | `1` | `1920×1080` | `[0, 0, 0]` | `[960, 540, 0]` | `100` | 空文本层 `Anchor=[0,0,0]`、`Position=合成中心` |
| 形状 Shape | 无 / `11862022` | 无 | `形状图层 1` | `ShapeLayer` | `8` | `1920×1080` | `[0, 0, 0]` | `[960, 540, 0]` | `100` | 空形状层 `Anchor=[0,0,0]`、`Position=合成中心`、`label=8`(蓝) |
| 空对象 Null | `Ctrl+Alt+Shift+Y` / `11862021` | 无 | `空 1` | `NullLayer` | `1` | `100×100` | `[0, 0, 0]` | `[960, 540, 0]` | `0` | **`100×100`、`Anchor=[0,0,0]`(左上角而非中心)、`Opacity=0`** |
| 调整图层 Adjustment | `Ctrl+Alt+Y` / `11862023` | 无 | `调整图层 1` | `AdjustmentLayer` | `5` | `1920×1080` | `[960, 540, 0]` | `[960, 540, 0]` | `100` | 实质为合成尺寸白底纯色（`adjustmentLayer=true`），`label=5`(淡紫) |
| 摄像机 Camera | `Ctrl+Alt+Shift+C` / `11862020` | 有 | `摄像机 1` | `CameraLayer` | `4` | `-` | `[960, 540, 0]` | `[960, 540, -3555.55556]` | `100` | `POI=[cx,cy,0]`，`Position=[cx,cy,-zoom]`；`zoom = comp.w × focalLength / filmSize` |
| 灯光 Light | `Ctrl+Alt+Shift+L` / `11862019` | 有 | `聚光 1` | `LightLayer` | `6` | `-` | `[960, 540, 0]` | `[1040, 460, -666.66667]` | `100` | 默认聚光 `SPOT`；`POI=[cx,cy,0]`，**`Position=[cx+80, cy-80, -666.66667]`** |

### 1.3 摄像机与灯光专属参数默认值

- **摄像机（`CameraLayer`）**：
  - `Zoom` = `3555.55556` px（由当前预设焦距 `66.67 mm` 与胶片宽度 `36.00 mm` 计算：`1920 × 66.6667 / 36 = 3555.5556`；50mm 预设对应 `2666.6667`）。
  - `Depth of Field` = `0`（关闭），`Focus Distance` = `3555.55556`（锁定等于 Zoom），`Aperture` = `25.30934`，`Blur Level` = `100%`。
  - 光圈/散景默认：`Iris Shape = 1`、`Rotation = 0`、`Roundness = 0`、`Aspect Ratio = 1`、`Diffraction Fringe = 0`、`Highlight Gain = 0`、`Highlight Threshold = 1`、`Highlight Saturation = 0`。
  - 若当前合成中不存在任何 3D 图层，确定后 AE 26 会在每个会话首次弹出一条提示对话框提醒摄像机仅影响 3D 图层（可勾选不再显示）。
- **灯光（`LightLayer`）**：
  - 默认类型：`SPOT`（聚光灯），`Intensity = 100%`，`Color = [1, 1, 1, 1]`（纯白）。
  - 聚光参数：`Cone Angle = 90°`，`Cone Feather = 50%`。
  - 衰减与阴影：`Falloff Type = 1`（无），`Falloff Start (半径) = 500`，`Falloff Distance (衰减距离) = 500`，`Casts Shadows = 0`（关闭），`Shadow Darkness = 100%`，`Shadow Diffusion = 0`。
  - 与摄像机相同，无 3D 图层时会话首次会弹出提示对话框。

### 1.4 模态对话框控件清单（纯色 / 摄像机 / 灯光）

#### 纯色设置对话框（`Ctrl+Y`）

| 序号 | 控件类型 | 标签 / 名称 | 默认值 / 状态 |
|---:|---|---|---|
| 1 | `label` | `名称:` | `` |
| 2 | `edit` | `名称:` | `红色 纯色 1` |
| 3 | `label` | `宽度:` | `` |
| 4 | `spinbox` | `LayoutSolidBase_width` | `1920` |
| 5 | `label` | `高度:` | `` |
| 6 | `spinbox` | `LayoutSolidBase_height` | `1080` |
| 7 | `checkbox` | `将长宽比锁定为 16:9 (1.78)` | `未勾选` |
| 8 | `label` | `单位:` | `` |
| 9 | `label` | `像素长宽比:` | `` |
| 10 | `label` | `宽度:` | `` |
| 11 | `label` | `合成的 100.0%` | `` |
| 12 | `label` | `高度:` | `` |
| 13 | `label` | `合成的 100.0%` | `` |
| 14 | `label` | `画面长宽比:` | `` |
| 15 | `label` | `16:9 (1.78)` | `` |
| 16 | `button` | `制作合成大小` | `` |
| 17 | `button` | `UI_Button` | `` |
| 18 | `button` | `LayoutSolidBase_ColorPickerControl_colorButton` | `` |
| 19 | `checkbox` | `预览` | `未勾选` |
| 20 | `button` | `确定` | `` |
| 21 | `button` | `取消` | `` |

#### 摄像机设置对话框（`Ctrl+Alt+Shift+C`）

| 序号 | 控件类型 | 标签 / 名称 | 默认值 / 状态 |
|---:|---|---|---|
| 1 | `label` | `名称:` | `` |
| 2 | `edit` | `名称:` | `摄像机 1` |
| 3 | `label` | `预设:` | `` |
| 4 | `button` | `savePreset` | `` |
| 5 | `button` | `deletePreset` | `` |
| 6 | `label` | `缩放:` | `` |
| 7 | `spinbox` | `zoom` | `1254.32` |
| 8 | `label` | `胶片大小:` | `` |
| 9 | `spinbox` | `filmSize` | `36.00` |
| 10 | `label` | `视角:` | `` |
| 11 | `spinbox` | `angleOfView` | `30.22` |
| 12 | `label` | `合成大小` | `` |
| 13 | `label` | `677.33 毫米` | `` |
| 14 | `label` | `焦距:` | `` |
| 15 | `spinbox` | `focalLength` | `66.67` |
| 16 | `checkbox` | `启用景深` | `未勾选` |
| 17 | `label` | `焦距:` | `` |
| 18 | `spinbox` | `focusDistance` | `1254.32` |
| 19 | `checkbox` | `锁定到缩放` | `勾选` |
| 20 | `label` | `光圈:` | `` |
| 21 | `spinbox` | `aperture` | `8.93` |
| 22 | `label` | `光圈大小:` | `` |
| 23 | `spinbox` | `fStop` | `7.5` |
| 24 | `label` | `模糊层次:` | `` |
| 25 | `spinbox` | `blurLevel` | `100.0` |
| 26 | `label` | `单位:` | `` |
| 27 | `label` | `量度胶片大小:` | `` |
| 28 | `label` | `类型:` | `` |
| 29 | `checkbox` | `基于当前视图创建摄像机` | `未勾选` |
| 30 | `button` | `确定` | `` |
| 31 | `button` | `取消` | `` |

#### 灯光设置对话框（`Ctrl+Alt+Shift+L`）

| 序号 | 控件类型 | 标签 / 名称 | 默认值 / 状态 |
|---:|---|---|---|
| 1 | `label` | `名称:` | `` |
| 2 | `edit` | `名称:` | `聚光 1` |
| 3 | `label` | `灯光类型:` | `` |
| 4 | `label` | `颜色:` | `` |
| 5 | `button` | `UI_Button` | `` |
| 6 | `button` | `CPControlWithLabel_colorButton` | `` |
| 7 | `label` | `强度:` | `` |
| 8 | `spinbox` | `intensity` | `100` |
| 9 | `label` | `锥形角度:` | `` |
| 10 | `spinbox` | `coneAngle` | `90` |
| 11 | `label` | `锥形羽化:` | `` |
| 12 | `spinbox` | `coneFeather` | `50` |
| 13 | `label` | `衰减:` | `` |
| 14 | `label` | `半径:` | `` |
| 15 | `spinbox` | `falloffStart` | `500` |
| 16 | `label` | `衰减距离:` | `` |
| 17 | `spinbox` | `falloffDistance` | `500` |
| 18 | `checkbox` | `投影` | `未勾选` |
| 19 | `label` | `阴影深度:` | `` |
| 20 | `spinbox` | `shadowDarkness` | `100` |
| 21 | `label` | `阴影扩散:` | `` |
| 22 | `spinbox` | `shadowDiffusion` | `0` |
| 23 | `checkbox` | `预览` | `未勾选` |
| 24 | `button` | `确定` | `` |
| 25 | `button` | `取消` | `` |

## 2. 复制图层与拆分图层

| 操作 | 快捷键 / `commandId` | 初始状态 | 实测结果 | 实现要点 |
|---|---|---|---|---|
| 复制图层 Duplicate | `Ctrl+D` / `196624` | `[S1..S4]`，选中 `L2 (S2)` | `L2` 为新副本（`sel=true`），原 `S2` 下移至 `L3`（`sel=false`） | **插在原图层正上方**（同 index），仅选中副本 |
| 拆分图层 Split Layer | `Ctrl+Shift+D` / `196625` | `SplitLayer`（`in=0, out=10`，含 `1s/5s` 关键帧与高斯模糊），`t=3.0s` | `L1`: `in=3, out=10, st=0, sel=true`；`L2`: `in=0, out=3, st=0, sel=false` | **后半段新建并放在正上方 `L1` 且选中**，前半段保留在 `L2`；**两半均完整保留全部关键帧（含裁剪区间外的关键帧）与全部效果** |

## 3. 时间轴：工作区与图层条操作

### 3.1 核心不变量：右端 1 帧包含规则（1-Frame Inclusive End）

AE 26 在 30 fps（每帧 `1/30 = 0.03333 s`）下，所有“对齐/裁剪到播放头作为**终点**”的快捷键均**包含播放头当前这一帧**：
- **起点类命令**（`B`、`[`、`Alt+[`）：精确对齐 `playhead`（例如 `t = 3.0s` → `3.0`）。
- **终点类命令**（`N`、`]`、`Alt+]`）：对齐到 **`playhead + 1/fps`**（例如 `t = 3.0s` → `3.03333s`；`t = 5.0s` → `5.03333s`），确保播放头所在帧仍处于可见/渲染范围内。

### 3.2 工作区（`B` / `N` / 修剪合成至工作区）

| 用例 | 操作 | 实测合成时长 `duration` | 实测工作区 `(start, dur)` | 图层 `TrimLayer` (`in=2, out=6, st=0`) 结果 |
|---|---|---:|---|---|
| `workarea_B_2s_N_5s` | `t=2.0s` 按 `B`，`t=5.0s` 按 `N` | `10` | `(2, 3.03333)`（91 帧） | 不变 `in=2, out=6, st=0` |
| `workarea_trim_comp` | `Ctrl+Shift+X`（`327685`） | `3.03333` | `(0, 3.03333)` | `in=0, out=3.03333, startTime=-2` |

修剪合成公式：`newDuration = oldWorkAreaDuration`，`workAreaStart = 0`，每个图层的 `startTime -= oldWorkAreaStart`，`inPoint`/`outPoint` 按 `[0, newDuration]` 裁剪。

### 3.3 图层条快捷键与鼠标拖拽（初始 `TrimLayer`: `in=2, out=6, st=0`，播放头 `t=3.0s`）

| 用例 | 操作 | `inPoint` | `outPoint` | `startTime` | 行为规则 |
|---|---|---:|---:|---:|---|
| `layer_move_in_bracket` | `[`（移动入点到播放头） | `3` | `7` | `1` | 平移整个图层条，保持时长不变：`delta = playhead - inPoint` |
| `layer_move_out_bracket` | `]`（移动出点到播放头） | `-0.96667` | `3.03333` | `-2.96667` | **1 帧包含**：`out = playhead + 1/fps`，整体平移 `delta = (playhead + 1/fps) - outPoint` |
| `layer_trim_in_alt_bracket` | `Alt+[`（修剪入点到播放头） | `3` | `6` | `0` | 仅改 `inPoint = playhead`，`startTime` 与 `outPoint` 不变 |
| `layer_trim_out_alt_bracket` | `Alt+]`（修剪出点到播放头） | `2` | `3.03333` | `0` | **1 帧包含**：仅改 `outPoint = playhead + 1/fps`，`startTime` 与 `inPoint` 不变 |
| `layer_bar_drag_body_plus1_5s` | 鼠标拖拽图层条主体 `+1.5s` | `3.5` | `7.5` | `1.5` | `in`/`out`/`startTime` 同幅平移 `+1.5s` |
| `layer_bar_drag_left_trim_to_3s` | 鼠标拖拽左边缘 `2.0s → 3.0s` | `3` | `6` | `0` | 仅修改 `inPoint = 3.0`，`startTime` 不变 |
| `layer_bar_drag_right_trim_to_7_5s` | 鼠标拖拽右边缘 `6.0s → 7.5s` | `2` | `7.5` | `0` | 仅修改 `outPoint = 7.5`，`startTime` 不变 |
| `layer_bar_shift_snap_to_playhead` | `Shift`+拖拽主体靠近 `t=3.0s` | `3` | `7` | `1` | 图层条入点自动吸附到播放头精确值 `3.0s` |

## 4. 时间轴：关键帧缓动、定格、微调、拖拽与比例缩放

基准：`KfLayer` 位置属性 3 个线性关键帧：`K1 = 30f (1.0s) [400,540]`、`K2 = 90f (3.0s) [960,540]`、`K3 = 150f (5.0s) [1520,540]`。线性默认时间影响（influence）为 **`16.66667%`（`1/6`）**。

### 4.1 缓动、定格与时间反向

| 用例 | 快捷键 / `commandId` | 目标 | 实测 `inInterp` / `outInterp` | 实测 `inEase` / `outEase` (`speed, influence`) | 说明 |
|---|---|---|---|---|---|
| `kf_easy_ease_f9` | `F9` / `11075597` | 仅选中 `K2` | `BEZIER` / `BEZIER` | `(0, 33.33333)` / `(0, 33.33333)` | 两侧均置 `BEZIER`，速度清零，影响 **`33.33333%` (`1/3`)** |
| `kf_easy_ease_in_shift_f9` | `Shift+F9` / `11075595` | 仅选中 `K2` | `BEZIER` / `LINEAR` | `(0, 33.33333)` / `(280, 16.66667)` | 仅入侧变为 `BEZIER {0, 33.33333}`，出侧保持 `LINEAR` |
| `kf_easy_ease_out_ctrl_shift_f9` | `Ctrl+Shift+F9` / `11075596` | 仅选中 `K2` | `LINEAR` / `BEZIER` | `(280, 16.66667)` / `(0, 33.33333)` | 仅出侧变为 `BEZIER {0, 33.33333}`，入侧保持 `LINEAR` |
| `kf_toggle_hold_ctrl_alt_h` | `Ctrl+Alt+H` / `851976` | 仅选中 `K2` | `LINEAR` / `HOLD` | `(280, 16.66667)` / `(0, 16.66667)` | 仅出侧变为 `HOLD {0, 16.66667}`，入侧保持 `LINEAR` |
| `kf_time_reverse` | 菜单 / `11075592` | 全选 3 帧 | 保持 `LINEAR` | 保持原缓动 | 时间不变，数值首尾反转：`30f:[1520, 540], 90f:[960, 540], 150f:[400, 540]`，保持全选 |

### 4.2 关键帧微调、框选、拖拽、吸附与 `Alt` 时间比例缩放

| 用例 | 操作 | 实测帧号 `[K1, K2, K3]` | 实现要点 |
|---|---|---|---|
| `kf_nudge_alt_right_1f` | `Alt+Right`（选中 `K2 @ 90f`） | `[30, 91, 150]` | 单帧微调 `+1 frame` |
| `kf_nudge_alt_shift_right_10f` | `Alt+Shift+Right`（接上 `91f`） | `[30, 101, 150]` | 大步微调 `+10 frames` |
| `kf_drag_single_3s_to_4s` | 鼠标单帧拖拽 `3.0s → 4.0s` | `[30, 120, 150]` | 单选拖动仅移动该帧 |
| `kf_shift_drag_snap_playhead` | `Shift`+拖拽 `K2` 至 `3.96s`（播放头在 `4.0s`） | `[30, 120, 150]` | 吸附到播放头精确帧 `120f (4.0s)` |
| `kf_marquee_select_3` | 时间轴空白处框选 `[0.5s..5.8s]` | `[30, 90, 150]` | 命中区间内全部关键帧并高亮选中 |
| `kf_drag_all3_plain_plus2s` | 全选 3 帧，普通拖拽 `K3` 从 `5s → 7s` | `[90, 150, 210]` | 全部选中关键帧刚性平移 `+2.0s` |
| `kf_alt_drag_before_down` | 全选 3 帧，**按下鼠标前先按住 `Alt`** 拖 `K3` `5s → 7s` | `[30, 120, 210]` | **按首帧 `1s` 为锚点等比缩放（`1.5×`）**，`K2` 由 `3s` 精确缩放到 `4s (120f)` |
| `kf_alt_drag_after_down` | 全选 3 帧，**开始拖拽后再按 `Alt`** 拖 `K3` `5s → 7s` | `[90, 150, 210]` | **不进入缩放模式**，保持刚性平移 `[90, 150, 210]`（修饰键须在 mouse-down 时判定） |

## 5. 拖放行为（Project 面板 → 合成视口 / 时间轴）

测试素材：项目根目录 `01_DragA`、`02_DragB`（均为 `300×200` 纯色素材项）。

| 用例 | 拖放路径与修饰键 | 结果图层栈（自上而下） | 关键规则 |
|---|---|---|---|
| `dnd_project_to_viewer_at_500_400` | Project `01_DragA` → 合成视口 `(500, 400)` | `L1:01_DragA`(P=[500, 400],sel=True) | 落点即图层 `Position = [500, 400]`，`in=0, out=10` |
| `dnd_project_to_viewer_shift_500_400` | Project `01_DragA` + `Shift` → 合成视口 `(500, 400)` | `L1:01_DragA`(P=[500, 400],sel=True) | 仍落在 `[500, 400]`（`Shift` 不强制居中到视口中心） |
| `dnd_project_multi_to_viewer_500_400` | Project 多选 `[01_DragA, 02_DragB]` → 合成视口 `(500, 400)` | `L1:01_DragA`(P=[500, 400],sel=True) / `L2:02_DragB`(P=[500, 400],sel=True) | 两层均落在 `[500, 400]`，保持项目列表先后顺序（`A` 在 `L1`、`B` 在 `L2`），全部选中 |
| `dnd_project_to_timeline_empty` | Project `01_DragA` → 空时间轴区域 | `L1:01_DragA`(P=[960, 540],sel=True) | **自动居中到合成中心 `[960, 540]`** |
| `dnd_project_to_timeline_between_L1_L2` | Project `01_DragA` → 时间轴 `L1` 与 `L2` 之间 | `L1:Existing1`(P=[960, 540],sel=False) / `L2:01_DragA`(P=[960, 540],sel=True) / `L3:Existing2`(P=[960, 540],sel=False) | 精确插入 `L2`（居中 `[960, 540]`），仅选中新插入图层 |
| `dnd_project_to_timeline_bottom` | Project `01_DragA` → 时间轴最底图层下方 | `L1:Existing1`(P=[960, 540],sel=False) / `L2:Existing2`(P=[960, 540],sel=False) / `L3:01_DragA`(P=[960, 540],sel=True) | 追加至最底 `L3`（居中 `[960, 540]`），仅选中新插入图层 |
| `dnd_project_multi_to_timeline` | Project 多选 `[01_DragA, 02_DragB]` → 时间轴 | `L1:01_DragA`(P=[960, 540],sel=True) / `L2:02_DragB`(P=[960, 540],sel=True) | 保持列表顺序（`L1=01_DragA`, `L2=02_DragB`），全部居中 `[960, 540]` 且全部选中 |

## 6. 效果添加与堆叠顺序

| 用例 | 操作 | 实测 `effects` 栈 | 行为规则 |
|---|---|---|---|
| `effects_apply_first_gaussian_blur` | 单选 1 图层，添加 `高斯模糊`（`460652560`） | `L1`:[1:ADBE Gaussian Blur 2(en=True)] | 新增至 `index=1`，默认 `enabled=true`，图层保持选中 |
| `effects_apply_second_levels` | 继续添加 `色阶`（`461307923`） | `L1`:[1:ADBE Gaussian Blur 2(en=True), 2:ADBE Easy Levels2(en=True)] | **追加至效果栈底部**（`index=1` 高斯模糊、`index=2` 色阶），先添加先执行 |
| `effects_apply_multi_selected` | 多选 `L1 + L2`，添加 `高斯模糊` | `L1`:[1:ADBE Gaussian Blur 2(en=True)] ; `L2`:[1:ADBE Gaussian Blur 2(en=True)] | **所有选中图层同时添加该效果**，保持原多选状态 |
| `effects_apply_none_selected` | 未选中任何图层，触发 `高斯模糊` | `L1`:[无] | 静默忽略（不添加、不报错） |

