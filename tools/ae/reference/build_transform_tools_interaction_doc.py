#!/usr/bin/env python3
"""Build structured AE 26 Transform, Snapping & Zoom tool interaction docs.

Reads live-measured ExtendScript comp dumps from --raw-dir and emits:
  - docs/reference/ae/interaction/transform-tools.json
  - docs/reference/ae/interaction/transform-tools.md
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from typing import Any


def round_num(val: Any) -> Any:
    if isinstance(val, float):
        r = round(val, 4)
        return int(r) if r.is_integer() else r
    if isinstance(val, list):
        return [round_num(x) for x in val]
    if isinstance(val, dict):
        return {k: round_num(v) for k, v in val.items()}
    return val


def load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8-sig") as f:
        return json.load(f)


def summarize_dump(dump: dict[str, Any]) -> dict[str, Any]:
    comp = dump.get("comp", {})
    layers = []
    for ly in dump.get("layers", []):
        rec: dict[str, Any] = {
            "index": ly["index"],
            "name": ly["name"],
            "selected": ly["selected"],
            "transform": round_num(ly["transform"]),
        }
        if ly.get("sourceRect"):
            rec["sourceRect"] = round_num(ly["sourceRect"])
        if ly.get("keys"):
            rec["keys"] = round_num(ly["keys"])
        layers.append(rec)
    return {
        "time": round_num(comp.get("time", 0)),
        "zoom": round_num(comp.get("zoom")),
        "selectedLayerIndices": comp.get("selectedLayerIndices", []),
        "layers": layers,
    }


def build_dataset(raw_dir: Path) -> dict[str, Any]:
    b2 = load_json(raw_dir / "batch2_transform.json")
    sv = load_json(raw_dir / "batch2_snap_verify.json")
    se = load_json(raw_dir / "batch2_snap_exact.json")

    case_specs: list[tuple[str, str, str, dict[str, Any]]] = [
        # Selection tool (V)
        ("sel_drag_move", "selection", "400x300 Solid @ [960,540]: drag (960,540)->(1160,640)", b2["sel_drag_move"]),
        ("sel_drag_shift_horiz", "selection", "400x300 Solid @ [960,540]: Shift-drag (960,540)->(1160,590) (dx=200,dy=50)", b2["sel_drag_shift_horiz"]),
        ("sel_drag_shift_diag", "selection", "400x300 Solid @ [960,540]: Shift-drag (960,540)->(1160,720) (dx=200,dy=180)", b2["sel_drag_shift_diag"]),
        ("sel_scale_br_free", "selection", "400x300 Solid @ [960,540]: drag BR handle (1160,690)->(1360,740)", b2["sel_scale_br_free"]),
        ("sel_scale_br_shift", "selection", "400x300 Solid @ [960,540]: Shift-drag BR handle (1160,690)->(1360,740) (wide)", b2["sel_scale_br_shift"]),
        ("sel_scale_br_shift_tall", "selection", "400x300 Solid @ [960,540]: Shift-drag BR handle (1160,690)->(1210,840) (tall)", sv["sel_scale_br_shift_tall"]),
        ("sel_scale_br_ctrl", "selection", "400x300 Solid @ [960,540]: Ctrl-drag BR handle (1160,690)->(1360,740)", b2["sel_scale_br_ctrl"]),
        ("sel_scale_br_alt", "selection", "400x300 Solid @ [960,540]: Alt-drag BR handle (1160,690)->(1360,740)", sv["sel_scale_br_alt"]),
        ("sel_scale_br_shift_ctrl", "selection", "400x300 Solid @ [960,540]: Shift+Ctrl-drag BR handle (1160,690)->(1360,740)", b2["sel_scale_br_shift_ctrl"]),
        ("sel_scale_offcenter_anchor", "selection", "400x300 Solid A=[0,0] P=[760,390]: drag BR handle (1160,690)->(1360,840)", sv["sel_scale_offcenter_anchor"]),
        ("sel_scale_edge_right", "selection", "400x300 Solid @ [960,540]: drag Right edge (1160,540)->(1360,540)", b2["sel_scale_edge_right"]),
        ("sel_scale_edge_flip", "selection", "400x300 Solid @ [960,540]: drag Right edge across anchor (1160,540)->(560,540)", b2["sel_scale_edge_flip"]),
        ("sel_drag_keyframe_auto", "selection", "Position keyframe at t=0 [960,540], playhead t=1.0: drag (+200,+100)", b2["sel_drag_keyframe_auto"]),
        ("sel_nudge_right_1px", "selection", "400x300 Solid @ [960,540]: Right Arrow key", b2["sel_nudge_right_1px"]),
        ("sel_nudge_shift_right_10px", "selection", "After 1px nudge: Shift + Right Arrow key", b2["sel_nudge_shift_right_10px"]),
        ("sel_click_empty_deselect", "selection", "Click empty canvas (100,100) with Solid selected", b2["sel_click_empty_deselect"]),
        ("sel_marquee_two_boxes", "selection", "Marquee drag (300,200)->(1500,600) enclosing 2 unselected 200x200 solids", b2["sel_marquee_two_boxes"]),
        ("handle_hit_probe_inside_6px", "selection", "Click 6px inside right edge (794,540) and drag +100px", se["handle_hit_probe_inside_6px"]),
        ("handle_hit_probe_inside_10px", "selection", "Click 10px inside right edge (790,540) and drag +100px", se["handle_hit_probe_inside_10px"]),
        # Rotation tool (W)
        ("rot_drag_90cw", "rotation", "400x300 Solid @ [960,540]: drag (1160,540)->(960,740) (+90 deg CW)", b2["rot_drag_90cw"]),
        ("rot_drag_shift_snap45", "rotation", "400x300 Solid @ [960,540]: Shift-drag (1160,540)->(1100,650) (~38.66 deg)", b2["rot_drag_shift_snap45"]),
        ("rot_drag_offcenter_anchor", "rotation", "400x300 Solid A=[0,0] P=[760,390]: drag (960,390)->(760,590) around TL anchor", b2["rot_drag_offcenter_anchor"]),
        ("rot_drag_keyframe_auto", "rotation", "Rotation keyframe at t=0 (0 deg), playhead t=1.0: drag +90 deg CW", b2["rot_drag_keyframe_auto"]),
        # Pan Behind / Anchor Point tool (Y) & Centering
        ("panbehind_drag_solid", "pan_behind", "400x300 Solid @ A=[200,150] P=[960,540]: drag anchor (960,540)->(1060,600)", b2["panbehind_drag_solid"]),
        ("panbehind_shift_horiz", "pan_behind", "400x300 Solid @ A=[200,150] P=[960,540]: Shift-drag anchor (960,540)->(1060,570)", sv["panbehind_shift_horiz"]),
        ("panbehind_drag_scaled150", "pan_behind", "400x300 Solid Scale=[150%,100%]: drag anchor (960,540)->(1110,590) (+150,+50)", b2["panbehind_drag_scaled150"]),
        ("snap_on_panbehind_to_br_corner", "pan_behind", "Snapping ON: drag anchor (960,540) to (1154,685) (6px/5px from BR corner)", sv["snap_on_panbehind_to_br_corner"]),
        ("panbehind_ctrl_dblclick_shape", "pan_behind", "Shape Layer (Rect gP=[-360,-190] A=[0,0] P=[960,540]): Ctrl+DoubleClick Pan Behind tool icon", b2["panbehind_ctrl_dblclick_shape"]),
        ("shortcut_ctrl_alt_home_shape", "centering", "Shape Layer (Rect gP=[-360,-190] A=[0,0] P=[960,540]): Ctrl+Alt+Home", b2["shortcut_ctrl_alt_home_shape"]),
        ("shortcut_ctrl_home_solid", "centering", "400x300 Solid @ P=[400,300]: Ctrl+Home", b2["shortcut_ctrl_home_solid"]),
        # Snapping (对齐)
        ("snap_off_near_center_5px", "snapping", "Snapping OFF: drag layer center (700,540)->(955,540) (5px from comp center)", b2["snap_off_near_center_5px"]),
        ("snap_on_grab_center_to_comp_center_5px", "snapping", "Snapping ON: drag layer center (600,540)->(955,540) (5px from comp center)", sv["snap_on_grab_center_to_comp_center_5px"]),
        ("snap_on_center_to_comp_left_5px", "snapping", "Snapping ON: drag layer center (600,540)->(5,540) (5px from comp left edge)", se["snap_on_center_to_comp_left_5px"]),
        ("snap_on_grab_center_to_target_left_6px", "snapping", "Snapping ON: drag MovingBox center (600,540)->(1094,540) (6px from TargetBox left=1100)", sv["snap_on_grab_center_to_target_left_6px"]),
        ("snap_on_grab_inside_right_25px_to_target_left", "snapping", "Snapping ON: grab 25px inside right edge (775,540)->(1069,540) (right edge 6px from TargetBox left=1100)", se["snap_on_grab_inside_right_25px_to_target_left"]),
        ("snap_on_grab_inside_br_to_target_tl", "snapping", "Snapping ON: grab 25px inside BR corner (775,665)->(1070,370) (BR 5px from TargetBox TL=[1100,390])", se["snap_on_grab_inside_br_to_target_tl"]),
        ("snap_off_ctrl_center_to_target_left_6px", "snapping", "Snapping OFF + Ctrl mid-drag: MovingBox center (600,540)->(1094,540)", se["snap_off_ctrl_center_to_target_left_6px"]),
        ("snap_on_ctrl_center_to_target_left_6px", "snapping", "Snapping ON + Ctrl mid-drag: MovingBox center (600,540)->(1094,540)", se["snap_on_ctrl_center_to_target_left_6px"]),
        ("snap_center_to_target_left_d16", "snapping", "Snapping ON threshold probe: 16px from TargetBox left=1100", se["snap_center_to_target_left_d16"]),
        ("snap_center_to_target_left_d20", "snapping", "Snapping ON threshold probe: 20px from TargetBox left=1100", se["snap_center_to_target_left_d20"]),
        ("snap_center_to_target_left_d24", "snapping", "Snapping ON threshold probe: 24px from TargetBox left=1100", se["snap_center_to_target_left_d24"]),
        # Zoom tool (Z)
        ("zoom_click_in", "zoom", "Zoom tool click at (960,540) from zoom=1.0", b2["zoom_click_in"]),
        ("zoom_alt_click_out", "zoom", "Zoom tool Alt+click at (960,540) from zoom=2.0", b2["zoom_alt_click_out"]),
        ("zoom_alt_click_out_2", "zoom", "Zoom tool Alt+click at (960,540) from zoom=1.0", b2["zoom_alt_click_out_2"]),
    ]

    cases = [
        {"id": cid, "tool": tool, "action": action, "result": summarize_dump(dump)}
        for cid, tool, action, dump in case_specs
    ]

    return {
        "schemaVersion": 1,
        "source": {
            "application": "Adobe After Effects",
            "version": "26.0x67",
            "locale": "zh_CN",
            "platform": "Windows 11 Pro x64 (150% DPI scaling)",
            "method": "Live Windows SendInput/MSAA automation + ExtendScript DOM readback",
            "testComp": {"width": 1920, "height": 1080, "frameRate": 30, "duration": 10, "zoom": 1.0},
            "capturedDate": "2026-11-22",
        },
        "rules": {
            "selectionTool": {
                "shortcut": "V",
                "moveDrag": {
                    "free": "Position += (dx, dy); Anchor/Scale/Rotation unchanged",
                    "shiftConstraint": "axis lock ONLY to H or V (NO 45 deg diagonal): if |dx| >= |dy| then dy=0 else dx=0",
                    "arrowNudge": "Arrow = 1 px in comp space; Shift+Arrow = 10 px",
                    "emptyCanvasClick": "deselects all layers (selectedLayerIndices = [])",
                    "marqueeDrag": "selects all layers intersected by marquee rectangle",
                    "autoKeyframe": "when Position stopwatch is on (numKeys >= 1) and playhead is at a new time, drag automatically creates a new keyframe at comp.time",
                },
                "boundingBoxScale": {
                    "pivot": "ALWAYS pivots around the layer Anchor Point (Layer.Position NEVER changes), regardless of whether Anchor is centered or off-center",
                    "freeCorner": "sx = (cursorX - anchorCompX) / (handleStartX - anchorCompX) * 100%, sy analogous",
                    "shiftCorner": "uniform scale using max(|sx|, |sy|) preserving sign",
                    "ctrlAndAltCorner": "do NOT change pivot (already anchor-pivoted)",
                    "edgeHandle": "1D scale along edge axis only; dragging past anchor flips sign (negative Scale)",
                    "handleHitRadiusPhysicalPxAt150Dpi": "6..9 px (hit at 6px inside edge, miss at 10px inside edge; ~5-6 logical DIP)",
                },
            },
            "rotationTool": {
                "shortcut": "W",
                "pivot": "layer Anchor Point / Position in comp space",
                "freeDrag": "deltaDeg = atan2(curr - pivot) - atan2(start - pivot) clockwise positive",
                "shiftConstraint": "snaps Rotation to 45 deg increments (0, 45, 90, ...)",
                "autoKeyframe": "when Rotation stopwatch is on and playhead is at a new time, drag automatically creates a keyframe at comp.time",
            },
            "panBehindAnchorTool": {
                "shortcut": "Y",
                "formula": "Layer.Position += dComp; Layer.AnchorPoint += inv(M_layer_linear) * dComp (visual image in comp remains completely stationary)",
                "shiftConstraint": "H/V axis lock while dragging anchor",
                "snapping": "when Snapping (对齐) is ON, snaps Anchor Point to the layer's own 9-point bounding box features (corners, edge midpoints, center)",
                "centerAnchorShortcuts": {
                    "ctrlAltHome": "centers Anchor Point to sourceRect center while compensating Position so visual placement does not move",
                    "ctrlDoubleClickPanBehindIcon": "exact same result as Ctrl+Alt+Home (verified on Shape Layer: A=[0,0],P=[960,540] -> A=[-360,-190],P=[600,350])",
                },
                "centerInCompShortcut": {
                    "ctrlHome": "moves Layer.Position to [comp.width/2, comp.height/2] without changing Anchor Point",
                },
            },
            "snapping": {
                "toolbarCheckboxZh": "对齐",
                "toolbarSubTogglesZh": [
                    "沿扩展到图层边界以外的边缘对齐",
                    "对齐并显示折叠合成和文本图层内的特性",
                ],
                "grabbedFeatureSemantics": "snaps the layer feature handle (9-point: 4 corners, 4 edge midpoints, center/anchor) NEAREST to the mouse-down point on the dragged layer, NOT all edges simultaneously",
                "snapTargets": "other visible layers' 9-point bounding box features (corners, edge midpoints, center/anchor); does NOT snap to comp center or comp edges when no guides are present",
                "ctrlTempInvert": "holding Ctrl (Cmd on Mac) mid-drag temporarily ENABLES snapping when checkbox is OFF, and temporarily DISABLES snapping when checkbox is ON",
                "thresholdAt150DpiZoom1": "snaps at <= 20 physical px (~13.3 DIP), does NOT snap at 24 physical px (~16 DIP)",
            },
            "zoomTool": {
                "shortcut": "Z",
                "clickZoomIn": "doubles magnification step (1.0 -> 2.0)",
                "altClickZoomOut": "halves magnification step (2.0 -> 1.0 -> 0.5)",
            },
        },
        "measuredCases": cases,
    }


def render_markdown(data: dict[str, Any]) -> str:
    cases = data["measuredCases"]
    lines: list[str] = [
        "# After Effects 26 交互实测：选取（V）、旋转（W）、锚点（Y）、对齐吸附与缩放",
        "",
        "> 来源：Adobe After Effects 26.0x67 (`zh_CN`) | 平台：Windows 11 Pro x64（150% DPI） | 测试合成：`1920×1080`、30 fps、10 s、`zoom = 1.0` | 实测日期：2026-11-22",
        ">",
        "> 方法：Windows 原生鼠标/键盘输入 + MSAA 工具栏读取 + ExtendScript DOM 精确数值回读，共 **" + str(len(cases)) + "** 组实测用例。结构化数据见 [`transform-tools.json`](transform-tools.json)。",
        "",
        "---",
        "",
        "## 一、核心规则速查（给开发直接照着实现）",
        "",
        "### 1. 选取工具（`V`）",
        "",
        "- **拖拽移动**：`Position += (dx, dy)`，不改 `Anchor` / `Scale` / `Rotation`。",
        "- **`Shift` 拖拽移动约束**：**仅锁定水平或垂直轴（无 45° 对角线吸附）**！按 `|dx| >= |dy|` 判水平、否则判垂直（实测 `dx=200, dy=180` 仍锁定为 `[+200, 0]`）。",
        "- **键盘微调**：方向键每次 `1 px`，`Shift + 方向键` 每次 `10 px`（合成像素）。",
        "- **空白处单击 / 框选**：单击画布空白处清空选中（`selectedLayerIndices = []`）；拖拽框选选中矩形覆盖到的全部图层。",
        "- **边界框缩放手柄（极其关键）**：",
        "  - **始终以图层当前锚点（Anchor Point）为缩放中心，`Layer.Position` 永不改变**（无论锚点居中还是在左上角 `A=[0,0]` 均如此；实测 `Ctrl` / `Alt` 不改变缩放支点）。",
        "  - **自由角点缩放**：`sx = (curX - ancX) / (startX - ancX) * 100%`，`sy` 同理（实测 400×300 图层拖右下角 `(1160,690)→(1360,740)` 得 `S = [200%, 133.3333%]`，`P` 保持 `[960, 540]`）。",
        "  - **`Shift` 等比角点缩放**：取 **`max(|sx|, |sy|)`** 作为统一缩放比例（实测宽拉 `200%/133.33%` 和高拉 `125%/200%` 均得 `[200%, 200%]`）。",
        "  - **边中点手柄缩放**：仅缩放对应单轴；拖过锚点另一侧产生负缩放镜像（实测拖至 `-400 px` 得 `S = [-200%, 100%]`）。",
        "  - **手柄命中半径**：150% DPI 下在边内 `6 px` 命中缩放手柄、`10 px` 已落入拖拽移动区域，即命中半径约 **`6–9 物理 px`（约 `5–6 逻辑 DIP`）**。",
        "- **自动关键帧**：当 `Position` 已开秒表（`numKeys >= 1`）且播放头位于新时间点（`t = 1.0s`），拖拽松手自动在 `t = 1.0s` 新增关键帧。",
        "",
        "### 2. 旋转工具（`W`）",
        "",
        "- **旋转中心**：严格围绕图层当前 `Anchor Point` / `Position` 旋转（实测 `A=[0,0], P=[760,390]` 绕左上角旋转 `+90°`，`P` 不变）。",
        "- **方向与角度**：合成空间顺时针为正角度（实测 3 点钟拖至 6 点钟精确为 `+90°`）。",
        "- **`Shift` 旋转约束**：以 **`45°` 整数倍**吸附（实测 `~38.66°` 精确吸附为 `45°`）。",
        "- **自动关键帧**：`Rotation` 开秒表时在新时间点拖拽自动打关键帧。",
        "",
        "### 3. 向后平移 / 锚点工具（`Y`）与居中快捷键",
        "",
        "- **核心补偿公式（保持画面视觉位置纹丝不动）**：拖动锚点在合成空间位移 `Δp_comp` 时：",
        "  - `Layer.Position += Δp_comp`",
        "  - `Layer.AnchorPoint += M_layer_linear⁻¹ · Δp_comp`",
        "  - 实测验证：`Scale = [150%, 100%]` 时锚点在合成空间拖动 `(+150, +50)` → `Position` 增加 `(+150, +50)` 变为 `[1110, 590]`，`AnchorPoint` 增加 `(+150/1.5, +50/1.0) = (+100, +50)` 变为 `[300, 200]`。",
        "- **`Shift` 拖拽锚点**：同样约束为水平/垂直单轴移动。",
        "- **开启「对齐」后拖拽锚点**：可精准吸附到图层自身 9 点边界特征（四角、四边中点、中心；实测距右下角 `(6, 5) px` 精准吸附至 `A=[400, 300], P=[1160, 690]`）。",
        "- **锚点居中（两种等效触发）**：",
        "  - **`Ctrl + Alt + Home`** 与 **`Ctrl + 双击` 工具栏锚点工具图标（`Y`）** 行为完全一致：将 `AnchorPoint` 移至 `sourceRect` 几何中心，并同步补偿 `Position` 使画面不动（实测形状图层 `A=[0,0], P=[960,540], gP=[-360,-190]` → `A=[-360,-190], P=[600,350]`）。",
        "- **图层居中到合成**：**`Ctrl + Home`** 将 `Layer.Position` 设为 `[comp.width/2, comp.height/2]`（`[960, 540]`），不改 `AnchorPoint`。",
        "",
        "### 4. 对齐吸附（工具栏「对齐」复选框 & `Ctrl` 临时反转）",
        "",
        "- **工具栏控件**：主开关复选框 `对齐`（EN: `Snapping`），右侧两个子切换按钮：`沿扩展到图层边界以外的边缘对齐`（默认未选）、`对齐并显示折叠合成和文本图层内的特性`（默认选中）。",
        "- **「抓点决定吸附特征」机制（核心差异，极易实现错）**：",
        "  - AE 拖动图层时，**只吸附距离鼠标按下点最近的那个特征点**（9 点特征：4 角、4 边中点、中心/锚点），而不是同时拿图层的全部边去碰其他图层。",
        "  - 实测验证：",
        "    - 抓住 `MovingBox` **中心** `(600,540)` 拖到距 `TargetBox` 左边 `1100` 差 `6 px`（`1094`）→ `MovingBox` 中心吸附到 `1100`；",
        "    - 抓住 `MovingBox` **右边内侧 25 px** `(775,540)` 拖到右边距 `1100` 差 `6 px` → `MovingBox` 右边贴齐 `1100`（`P = [900, 540]`）；",
        "    - 抓住 `MovingBox` **右下角内侧 25 px** `(775,665)` 拖近 `TargetBox` 左上角 `(1100,390)` → 右下角精准贴齐左上角（`P = [900, 240]`）。",
        "- **吸附目标范围**：吸附到**其他可见图层的 9 点特征**（以及用 `Y` 工具时吸附到自身 9 点特征）；**无参考线时，工具栏「对齐」不吸附合成中心 `(960,540)` 或合成边缘 `(0,540)`**（实测距合成中心 `5 px` 和距合成左边缘 `5 px` 均未吸附）。",
        "- **`Ctrl`（Mac `Cmd`）拖拽中临时反转**：",
        "  - 「对齐」关闭 + 按住 `Ctrl` 拖拽 → **临时开启吸附**（实测 `1094 → 1100`）；",
        "  - 「对齐」开启 + 按住 `Ctrl` 拖拽 → **临时关闭吸附**（实测保持 `1094`）。",
        "- **吸附半径阈值**：在 150% DPI、`zoom = 1.0` 下，距离 **`≤ 20 物理 px`**（约 `13.3 逻辑 DIP`）全部触发吸附，**`24 物理 px`**（`16 DIP`）不吸附（实测阶梯：`16 px ✓`、`20 px ✓`、`24 px ✗`）。",
        "",
        "### 5. 缩放工具（`Z`）",
        "",
        "- 单击画布放大一档（`1.0 → 2.0`），`Alt + 单击` 缩小一档（`2.0 → 1.0 → 0.5`）。",
        "",
        "---",
        "",
        "## 二、全部实测用例明细（" + str(len(cases)) + " 组）",
        "",
        "| 编号 | 工具 | 输入动作 | 实测结果（`A` / `P` / `S` / `R` / `zoom` / 关键帧） |",
        "|---|---|---|---|",
    ]

    for c in cases:
        res = c["result"]
        ly = res["layers"][0] if res["layers"] else None
        if not ly:
            out_s = f"sel={res['selectedLayerIndices']} zoom={res['zoom']}"
        else:
            tr = ly["transform"]
            out_s = (
                f"sel={res['selectedLayerIndices']} "
                f"A={tr['anchorPoint'][:2]} P={tr['position'][:2]} "
                f"S={tr['scale'][:2]} R={tr['rotation']}"
            )
            if res["zoom"] != 1:
                out_s += f" zoom={res['zoom']}"
            if ly.get("keys"):
                out_s += f" keys={ly['keys']}"
        lines.append(f"| `{c['id']}` | `{c['tool']}` | {c['action']} | `{out_s}` |")

    lines.append("")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description="Build AE 26 Transform/Snapping/Zoom interaction docs.")
    parser.add_argument(
        "--raw-dir",
        type=Path,
        default=Path(os.environ.get("BAE_AE_RAW_DIR", ".")),
        help="Directory containing batch2_*.json files",
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=Path("docs/reference/ae/interaction"),
        help="Output directory inside repository",
    )
    args = parser.parse_args()

    data = build_dataset(args.raw_dir)
    args.out_dir.mkdir(parents=True, exist_ok=True)

    json_path = args.out_dir / "transform-tools.json"
    md_path = args.out_dir / "transform-tools.md"

    json_text = json.dumps(data, ensure_ascii=False, indent=2) + "\n"
    md_text = render_markdown(data)

    json_path.write_text(json_text, encoding="utf-8")
    md_path.write_text(md_text, encoding="utf-8")

    print(f"Wrote {json_path} ({len(json_text.encode('utf-8'))} bytes)")
    print(f"Wrote {md_path} ({len(md_text.encode('utf-8'))} bytes)")
    print(f"Total measured cases: {len(data['measuredCases'])}")


if __name__ == "__main__":
    main()
