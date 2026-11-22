# After Effects 26 交互实测：选取（V）、旋转（W）、锚点（Y）、对齐吸附与缩放

> 来源：Adobe After Effects 26.0x67 (`zh_CN`) | 平台：Windows 11 Pro x64（150% DPI） | 测试合成：`1920×1080`、30 fps、10 s、`zoom = 1.0` | 实测日期：2026-11-22
>
> 方法：Windows 原生鼠标/键盘输入 + MSAA 工具栏读取 + ExtendScript DOM 精确数值回读，共 **44** 组实测用例。结构化数据见 [`transform-tools.json`](transform-tools.json)。

---

## 一、核心规则速查（给开发直接照着实现）

### 1. 选取工具（`V`）

- **拖拽移动**：`Position += (dx, dy)`，不改 `Anchor` / `Scale` / `Rotation`。
- **`Shift` 拖拽移动约束**：**仅锁定水平或垂直轴（无 45° 对角线吸附）**！按 `|dx| >= |dy|` 判水平、否则判垂直（实测 `dx=200, dy=180` 仍锁定为 `[+200, 0]`）。
- **键盘微调**：方向键每次 `1 px`，`Shift + 方向键` 每次 `10 px`（合成像素）。
- **空白处单击 / 框选**：单击画布空白处清空选中（`selectedLayerIndices = []`）；拖拽框选选中矩形覆盖到的全部图层。
- **边界框缩放手柄（极其关键）**：
  - **始终以图层当前锚点（Anchor Point）为缩放中心，`Layer.Position` 永不改变**（无论锚点居中还是在左上角 `A=[0,0]` 均如此；实测 `Ctrl` / `Alt` 不改变缩放支点）。
  - **自由角点缩放**：`sx = (curX - ancX) / (startX - ancX) * 100%`，`sy` 同理（实测 400×300 图层拖右下角 `(1160,690)→(1360,740)` 得 `S = [200%, 133.3333%]`，`P` 保持 `[960, 540]`）。
  - **`Shift` 等比角点缩放**：取 **`max(|sx|, |sy|)`** 作为统一缩放比例（实测宽拉 `200%/133.33%` 和高拉 `125%/200%` 均得 `[200%, 200%]`）。
  - **边中点手柄缩放**：仅缩放对应单轴；拖过锚点另一侧产生负缩放镜像（实测拖至 `-400 px` 得 `S = [-200%, 100%]`）。
  - **手柄命中半径**：150% DPI 下在边内 `6 px` 命中缩放手柄、`10 px` 已落入拖拽移动区域，即命中半径约 **`6–9 物理 px`（约 `5–6 逻辑 DIP`）**。
- **自动关键帧**：当 `Position` 已开秒表（`numKeys >= 1`）且播放头位于新时间点（`t = 1.0s`），拖拽松手自动在 `t = 1.0s` 新增关键帧。

### 2. 旋转工具（`W`）

- **旋转中心**：严格围绕图层当前 `Anchor Point` / `Position` 旋转（实测 `A=[0,0], P=[760,390]` 绕左上角旋转 `+90°`，`P` 不变）。
- **方向与角度**：合成空间顺时针为正角度（实测 3 点钟拖至 6 点钟精确为 `+90°`）。
- **`Shift` 旋转约束**：以 **`45°` 整数倍**吸附（实测 `~38.66°` 精确吸附为 `45°`）。
- **自动关键帧**：`Rotation` 开秒表时在新时间点拖拽自动打关键帧。

### 3. 向后平移 / 锚点工具（`Y`）与居中快捷键

- **核心补偿公式（保持画面视觉位置纹丝不动）**：拖动锚点在合成空间位移 `Δp_comp` 时：
  - `Layer.Position += Δp_comp`
  - `Layer.AnchorPoint += M_layer_linear⁻¹ · Δp_comp`
  - 实测验证：`Scale = [150%, 100%]` 时锚点在合成空间拖动 `(+150, +50)` → `Position` 增加 `(+150, +50)` 变为 `[1110, 590]`，`AnchorPoint` 增加 `(+150/1.5, +50/1.0) = (+100, +50)` 变为 `[300, 200]`。
- **`Shift` 拖拽锚点**：同样约束为水平/垂直单轴移动。
- **开启「对齐」后拖拽锚点**：可精准吸附到图层自身 9 点边界特征（四角、四边中点、中心；实测距右下角 `(6, 5) px` 精准吸附至 `A=[400, 300], P=[1160, 690]`）。
- **锚点居中（两种等效触发）**：
  - **`Ctrl + Alt + Home`** 与 **`Ctrl + 双击` 工具栏锚点工具图标（`Y`）** 行为完全一致：将 `AnchorPoint` 移至 `sourceRect` 几何中心，并同步补偿 `Position` 使画面不动（实测形状图层 `A=[0,0], P=[960,540], gP=[-360,-190]` → `A=[-360,-190], P=[600,350]`）。
- **图层居中到合成**：**`Ctrl + Home`** 将 `Layer.Position` 设为 `[comp.width/2, comp.height/2]`（`[960, 540]`），不改 `AnchorPoint`。

### 4. 对齐吸附（工具栏「对齐」复选框 & `Ctrl` 临时反转）

- **工具栏控件**：主开关复选框 `对齐`（EN: `Snapping`），右侧两个子切换按钮：`沿扩展到图层边界以外的边缘对齐`（默认未选）、`对齐并显示折叠合成和文本图层内的特性`（默认选中）。
- **「抓点决定吸附特征」机制（核心差异，极易实现错）**：
  - AE 拖动图层时，**只吸附距离鼠标按下点最近的那个特征点**（9 点特征：4 角、4 边中点、中心/锚点），而不是同时拿图层的全部边去碰其他图层。
  - 实测验证：
    - 抓住 `MovingBox` **中心** `(600,540)` 拖到距 `TargetBox` 左边 `1100` 差 `6 px`（`1094`）→ `MovingBox` 中心吸附到 `1100`；
    - 抓住 `MovingBox` **右边内侧 25 px** `(775,540)` 拖到右边距 `1100` 差 `6 px` → `MovingBox` 右边贴齐 `1100`（`P = [900, 540]`）；
    - 抓住 `MovingBox` **右下角内侧 25 px** `(775,665)` 拖近 `TargetBox` 左上角 `(1100,390)` → 右下角精准贴齐左上角（`P = [900, 240]`）。
- **吸附目标范围**：吸附到**其他可见图层的 9 点特征**（以及用 `Y` 工具时吸附到自身 9 点特征）；**无参考线时，工具栏「对齐」不吸附合成中心 `(960,540)` 或合成边缘 `(0,540)`**（实测距合成中心 `5 px` 和距合成左边缘 `5 px` 均未吸附）。
- **`Ctrl`（Mac `Cmd`）拖拽中临时反转**：
  - 「对齐」关闭 + 按住 `Ctrl` 拖拽 → **临时开启吸附**（实测 `1094 → 1100`）；
  - 「对齐」开启 + 按住 `Ctrl` 拖拽 → **临时关闭吸附**（实测保持 `1094`）。
- **吸附半径阈值**：在 150% DPI、`zoom = 1.0` 下，距离 **`≤ 20 物理 px`**（约 `13.3 逻辑 DIP`）全部触发吸附，**`24 物理 px`**（`16 DIP`）不吸附（实测阶梯：`16 px ✓`、`20 px ✓`、`24 px ✗`）。

### 5. 缩放工具（`Z`）

- 单击画布放大一档（`1.0 → 2.0`），`Alt + 单击` 缩小一档（`2.0 → 1.0 → 0.5`）。

---

## 二、全部实测用例明细（44 组）

| 编号 | 工具 | 输入动作 | 实测结果（`A` / `P` / `S` / `R` / `zoom` / 关键帧） |
|---|---|---|---|
| `sel_drag_move` | `selection` | 400x300 Solid @ [960,540]: drag (960,540)->(1160,640) | `sel=[1] A=[200, 150] P=[1160, 640] S=[100, 100] R=0` |
| `sel_drag_shift_horiz` | `selection` | 400x300 Solid @ [960,540]: Shift-drag (960,540)->(1160,590) (dx=200,dy=50) | `sel=[1] A=[200, 150] P=[1160, 540] S=[100, 100] R=0` |
| `sel_drag_shift_diag` | `selection` | 400x300 Solid @ [960,540]: Shift-drag (960,540)->(1160,720) (dx=200,dy=180) | `sel=[1] A=[200, 150] P=[1160, 540] S=[100, 100] R=0` |
| `sel_scale_br_free` | `selection` | 400x300 Solid @ [960,540]: drag BR handle (1160,690)->(1360,740) | `sel=[1] A=[200, 150] P=[960, 540] S=[200, 133.3333] R=0` |
| `sel_scale_br_shift` | `selection` | 400x300 Solid @ [960,540]: Shift-drag BR handle (1160,690)->(1360,740) (wide) | `sel=[1] A=[200, 150] P=[960, 540] S=[200, 200] R=0` |
| `sel_scale_br_shift_tall` | `selection` | 400x300 Solid @ [960,540]: Shift-drag BR handle (1160,690)->(1210,840) (tall) | `sel=[1] A=[200, 150] P=[960, 540] S=[200, 200] R=0` |
| `sel_scale_br_ctrl` | `selection` | 400x300 Solid @ [960,540]: Ctrl-drag BR handle (1160,690)->(1360,740) | `sel=[1] A=[200, 150] P=[960, 540] S=[200, 133.3333] R=0` |
| `sel_scale_br_alt` | `selection` | 400x300 Solid @ [960,540]: Alt-drag BR handle (1160,690)->(1360,740) | `sel=[1] A=[200, 150] P=[960, 540] S=[200, 133.3333] R=0` |
| `sel_scale_br_shift_ctrl` | `selection` | 400x300 Solid @ [960,540]: Shift+Ctrl-drag BR handle (1160,690)->(1360,740) | `sel=[1] A=[200, 150] P=[960, 540] S=[200, 200] R=0` |
| `sel_scale_offcenter_anchor` | `selection` | 400x300 Solid A=[0,0] P=[760,390]: drag BR handle (1160,690)->(1360,840) | `sel=[1] A=[0, 0] P=[760, 390] S=[150, 150] R=0` |
| `sel_scale_edge_right` | `selection` | 400x300 Solid @ [960,540]: drag Right edge (1160,540)->(1360,540) | `sel=[1] A=[200, 150] P=[960, 540] S=[200, 100] R=0` |
| `sel_scale_edge_flip` | `selection` | 400x300 Solid @ [960,540]: drag Right edge across anchor (1160,540)->(560,540) | `sel=[1] A=[200, 150] P=[960, 540] S=[-200, 100] R=0` |
| `sel_drag_keyframe_auto` | `selection` | Position keyframe at t=0 [960,540], playhead t=1.0: drag (+200,+100) | `sel=[1] A=[200, 150] P=[1160, 640] S=[100, 100] R=0 keys={'position': [{'time': 0, 'value': [960, 540, 0]}, {'time': 1, 'value': [1160, 640, 0]}]}` |
| `sel_nudge_right_1px` | `selection` | 400x300 Solid @ [960,540]: Right Arrow key | `sel=[1] A=[200, 150] P=[961, 540] S=[100, 100] R=0` |
| `sel_nudge_shift_right_10px` | `selection` | After 1px nudge: Shift + Right Arrow key | `sel=[1] A=[200, 150] P=[971, 540] S=[100, 100] R=0` |
| `sel_click_empty_deselect` | `selection` | Click empty canvas (100,100) with Solid selected | `sel=[] A=[200, 150] P=[960, 540] S=[100, 100] R=0` |
| `sel_marquee_two_boxes` | `selection` | Marquee drag (300,200)->(1500,600) enclosing 2 unselected 200x200 solids | `sel=[1, 2] A=[100, 100] P=[1300, 400] S=[100, 100] R=0` |
| `handle_hit_probe_inside_6px` | `selection` | Click 6px inside right edge (794,540) and drag +100px | `sel=[1] A=[200, 150] P=[600, 540] S=[150, 100] R=0` |
| `handle_hit_probe_inside_10px` | `selection` | Click 10px inside right edge (790,540) and drag +100px | `sel=[1] A=[200, 150] P=[700, 540] S=[100, 100] R=0` |
| `rot_drag_90cw` | `rotation` | 400x300 Solid @ [960,540]: drag (1160,540)->(960,740) (+90 deg CW) | `sel=[1] A=[200, 150] P=[960, 540] S=[100, 100] R=90` |
| `rot_drag_shift_snap45` | `rotation` | 400x300 Solid @ [960,540]: Shift-drag (1160,540)->(1100,650) (~38.66 deg) | `sel=[1] A=[200, 150] P=[960, 540] S=[100, 100] R=45` |
| `rot_drag_offcenter_anchor` | `rotation` | 400x300 Solid A=[0,0] P=[760,390]: drag (960,390)->(760,590) around TL anchor | `sel=[1] A=[0, 0] P=[760, 390] S=[100, 100] R=90` |
| `rot_drag_keyframe_auto` | `rotation` | Rotation keyframe at t=0 (0 deg), playhead t=1.0: drag +90 deg CW | `sel=[1] A=[200, 150] P=[960, 540] S=[100, 100] R=90 keys={'rotation': [{'time': 0, 'value': 0}, {'time': 1, 'value': 90}]}` |
| `panbehind_drag_solid` | `pan_behind` | 400x300 Solid @ A=[200,150] P=[960,540]: drag anchor (960,540)->(1060,600) | `sel=[1] A=[300, 210] P=[1060, 600] S=[100, 100] R=0` |
| `panbehind_shift_horiz` | `pan_behind` | 400x300 Solid @ A=[200,150] P=[960,540]: Shift-drag anchor (960,540)->(1060,570) | `sel=[1] A=[300, 150] P=[1060, 540] S=[100, 100] R=0` |
| `panbehind_drag_scaled150` | `pan_behind` | 400x300 Solid Scale=[150%,100%]: drag anchor (960,540)->(1110,590) (+150,+50) | `sel=[1] A=[300, 200] P=[1110, 590] S=[150, 100] R=0` |
| `snap_on_panbehind_to_br_corner` | `pan_behind` | Snapping ON: drag anchor (960,540) to (1154,685) (6px/5px from BR corner) | `sel=[1] A=[400, 300] P=[1160, 690] S=[100, 100] R=0` |
| `panbehind_ctrl_dblclick_shape` | `pan_behind` | Shape Layer (Rect gP=[-360,-190] A=[0,0] P=[960,540]): Ctrl+DoubleClick Pan Behind tool icon | `sel=[1] A=[-360, -190] P=[600, 350] S=[100, 100] R=0` |
| `shortcut_ctrl_alt_home_shape` | `centering` | Shape Layer (Rect gP=[-360,-190] A=[0,0] P=[960,540]): Ctrl+Alt+Home | `sel=[1] A=[-360, -190] P=[600, 350] S=[100, 100] R=0` |
| `shortcut_ctrl_home_solid` | `centering` | 400x300 Solid @ P=[400,300]: Ctrl+Home | `sel=[1] A=[200, 150] P=[960, 540] S=[100, 100] R=0` |
| `snap_off_near_center_5px` | `snapping` | Snapping OFF: drag layer center (700,540)->(955,540) (5px from comp center) | `sel=[1] A=[200, 150] P=[955, 540] S=[100, 100] R=0` |
| `snap_on_grab_center_to_comp_center_5px` | `snapping` | Snapping ON: drag layer center (600,540)->(955,540) (5px from comp center) | `sel=[1] A=[200, 150] P=[955, 540] S=[100, 100] R=0` |
| `snap_on_center_to_comp_left_5px` | `snapping` | Snapping ON: drag layer center (600,540)->(5,540) (5px from comp left edge) | `sel=[1] A=[200, 150] P=[5, 540] S=[100, 100] R=0` |
| `snap_on_grab_center_to_target_left_6px` | `snapping` | Snapping ON: drag MovingBox center (600,540)->(1094,540) (6px from TargetBox left=1100) | `sel=[1] A=[200, 150] P=[1100, 540] S=[100, 100] R=0` |
| `snap_on_grab_inside_right_25px_to_target_left` | `snapping` | Snapping ON: grab 25px inside right edge (775,540)->(1069,540) (right edge 6px from TargetBox left=1100) | `sel=[1] A=[200, 150] P=[900, 540] S=[100, 100] R=0` |
| `snap_on_grab_inside_br_to_target_tl` | `snapping` | Snapping ON: grab 25px inside BR corner (775,665)->(1070,370) (BR 5px from TargetBox TL=[1100,390]) | `sel=[1] A=[200, 150] P=[900, 240] S=[100, 100] R=0` |
| `snap_off_ctrl_center_to_target_left_6px` | `snapping` | Snapping OFF + Ctrl mid-drag: MovingBox center (600,540)->(1094,540) | `sel=[1] A=[200, 150] P=[1100, 540] S=[100, 100] R=0` |
| `snap_on_ctrl_center_to_target_left_6px` | `snapping` | Snapping ON + Ctrl mid-drag: MovingBox center (600,540)->(1094,540) | `sel=[1] A=[200, 150] P=[1094, 540] S=[100, 100] R=0` |
| `snap_center_to_target_left_d16` | `snapping` | Snapping ON threshold probe: 16px from TargetBox left=1100 | `sel=[1] A=[200, 150] P=[1100, 540] S=[100, 100] R=0` |
| `snap_center_to_target_left_d20` | `snapping` | Snapping ON threshold probe: 20px from TargetBox left=1100 | `sel=[1] A=[200, 150] P=[1100, 540] S=[100, 100] R=0` |
| `snap_center_to_target_left_d24` | `snapping` | Snapping ON threshold probe: 24px from TargetBox left=1100 | `sel=[1] A=[200, 150] P=[1076, 540] S=[100, 100] R=0` |
| `zoom_click_in` | `zoom` | Zoom tool click at (960,540) from zoom=1.0 | `sel=[1] A=[200, 150] P=[960, 540] S=[100, 100] R=0 zoom=2` |
| `zoom_alt_click_out` | `zoom` | Zoom tool Alt+click at (960,540) from zoom=2.0 | `sel=[1] A=[200, 150] P=[960, 540] S=[100, 100] R=0` |
| `zoom_alt_click_out_2` | `zoom` | Zoom tool Alt+click at (960,540) from zoom=1.0 | `sel=[1] A=[200, 150] P=[960, 540] S=[100, 100] R=0 zoom=0.5` |
