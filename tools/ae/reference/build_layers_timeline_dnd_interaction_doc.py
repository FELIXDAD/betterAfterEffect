#!/usr/bin/env python3
"""Build structured AE 26 New Layers, Timeline & Drag-and-Drop interaction docs.

Reads live-measured ExtendScript comp dumps from --raw-dir and emits:
  - docs/reference/ae/interaction/layers-timeline-dnd.json
  - docs/reference/ae/interaction/layers-timeline-dnd.md
"""

from __future__ import annotations

import argparse
import json
import os
import re
from pathlib import Path
from typing import Any


def round_num(val: Any) -> Any:
    if isinstance(val, float):
        r = round(val, 5)
        return int(r) if r.is_integer() else r
    if isinstance(val, list):
        return [round_num(x) for x in val]
    if isinstance(val, dict):
        return {k: round_num(v) for k, v in val.items()}
    return val


def load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8-sig") as f:
        return json.load(f)


# MSAA role IDs used by AE's Win32 modal dialogs
ROLE_MAP = {
    0: "edit",
    41: "label",
    43: "button",
    44: "checkbox",
    52: "spinbox",
}

# Controls to exclude: container wrappers and long explanatory notice text
CONTROL_SKIP_NAMES = {"OS_ViewContainer", " "}


def parse_dialog_controls(raw_list: list[str] | None) -> list[dict[str, Any]]:
    """Parse raw MSAA control strings into clean structured control metadata.

    Omits container wrappers, screen coordinates, and multi-clause advisory prose.
    """
    if not raw_list:
        return []
    pat = re.compile(r'^r=(\d+)\s+st=0x([0-9a-fA-F]+)\s+name="(.*)"\s+val="(.*)"\s+ctr=\(')
    out: list[dict[str, Any]] = []
    for line in raw_list:
        m = pat.match(line.replace("\r", " ").replace("\n", " "))
        if not m:
            continue
        role_id = int(m.group(1))
        state = int(m.group(2), 16)
        name = m.group(3).strip()
        val = m.group(4).strip()
        if role_id in (9, 10) or not name or name in CONTROL_SKIP_NAMES:
            continue
        # Exclude long advisory / warning sentences (keep concise field labels & controls)
        if role_id == 41 and (len(name) > 24 or "注意:" in name or "注意：" in name):
            continue
        ctrl_type = ROLE_MAP.get(role_id, f"role_{role_id}")
        rec: dict[str, Any] = {
            "type": ctrl_type,
            "name": name,
        }
        if val and val != name:
            rec["defaultValue"] = val
        if ctrl_type == "checkbox":
            rec["checked"] = bool(state & 0x10)
        out.append(rec)
    return out


def summarize_layer(ly: dict[str, Any]) -> dict[str, Any]:
    rec: dict[str, Any] = {
        "index": ly["index"],
        "name": ly["name"],
        "matchName": ly.get("matchName"),
        "layerType": ly.get("layerType"),
        "selected": ly["selected"],
        "enabled": ly.get("enabled"),
        "label": ly.get("label"),
        "threeDLayer": ly.get("threeDLayer"),
        "inPoint": round_num(ly.get("inPoint")),
        "outPoint": round_num(ly.get("outPoint")),
        "startTime": round_num(ly.get("startTime")),
        "width": ly.get("width"),
        "height": ly.get("height"),
        "transform": round_num(ly.get("transform", {})),
    }
    if ly.get("solidColor") is not None:
        rec["solidColor"] = round_num(ly["solidColor"])
    if ly.get("cameraOptions"):
        rec["cameraOptions"] = round_num(ly["cameraOptions"])
    if ly.get("lightType") is not None:
        rec["lightType"] = ly["lightType"]
    if ly.get("lightOptions"):
        rec["lightOptions"] = round_num(ly["lightOptions"])
    if ly.get("effects"):
        rec["effects"] = round_num(ly["effects"])
    if ly.get("keys"):
        rec["keys"] = round_num(ly["keys"])
    return rec


def summarize_dump(dump: dict[str, Any], include_dialog: bool = False) -> dict[str, Any]:
    comp = dump.get("comp", {})
    out: dict[str, Any] = {
        "comp": {
            "width": comp.get("width"),
            "height": comp.get("height"),
            "frameRate": round_num(comp.get("frameRate")),
            "duration": round_num(comp.get("duration")),
            "time": round_num(comp.get("time")),
            "workAreaStart": round_num(comp.get("workAreaStart")),
            "workAreaDuration": round_num(comp.get("workAreaDuration")),
            "numLayers": comp.get("numLayers"),
            "selectedLayerIndices": comp.get("selectedLayerIndices", []),
        },
        "layers": [summarize_layer(ly) for ly in dump.get("layers", [])],
    }
    if include_dialog and dump.get("dialogControls"):
        out["dialogControls"] = parse_dialog_controls(dump["dialogControls"])
    return out


def build_dataset(raw_dir: Path) -> dict[str, Any]:
    b3a = load_json(raw_dir / "batch3a.json")
    b3f = load_json(raw_dir / "batch3a_fix.json")
    b3tl = load_json(raw_dir / "batch3a_tl.json")
    b3t = load_json(raw_dir / "batch3b_timeline.json")
    b3d = load_json(raw_dir / "batch3b_dnd.json")
    b3fin = load_json(raw_dir / "batch3b_finish.json")
    b3kf = load_json(raw_dir / "batch3b_kf_alt_scale.json")

    # For new_layer_light, only retain the newly created top light (L1) as clean single-layer baseline
    light_clean = dict(b3f["new_layer_light"])
    light_clean["layers"] = [b3f["new_layer_light"]["layers"][0]]
    light_clean["comp"] = dict(b3f["new_layer_light"]["comp"])
    light_clean["comp"]["numLayers"] = 1
    light_clean["comp"]["selectedLayerIndices"] = [1]

    case_specs: list[tuple[str, str, str, dict[str, Any], bool]] = [
        # 1. New Layer defaults & modal dialogs
        ("new_layer_solid", "new_layers", "Layer > New > Solid (Ctrl+Y, cmd 11862018) on 1920x1080@30fps 10s comp at t=0", b3a["new_layer_solid"], True),
        ("new_layer_text", "new_layers", "Layer > New > Text (Ctrl+Alt+Shift+T, cmd 11862017) at t=0", b3a["new_layer_text"], False),
        ("new_layer_shape", "new_layers", "Layer > New > Shape Layer (cmd 11862022) at t=0", b3a["new_layer_shape"], False),
        ("new_layer_null", "new_layers", "Layer > New > Null Object (Ctrl+Alt+Shift+Y, cmd 11862021) at t=0", b3a["new_layer_null"], False),
        ("new_layer_adjustment", "new_layers", "Layer > New > Adjustment Layer (Ctrl+Alt+Y, cmd 11862023) at t=0", b3a["new_layer_adjustment"], False),
        ("new_layer_camera", "new_layers", "Layer > New > Camera (Ctrl+Alt+Shift+C, cmd 11862020) at t=0", b3f["new_layer_camera"], True),
        ("new_layer_light", "new_layers", "Layer > New > Light (Ctrl+Alt+Shift+L, cmd 11862019) at t=0", light_clean, True),
        ("new_layer_null_at_t2", "new_layers", "Layer > New > Null Object with playhead at t=2.0s (verifies full-comp span)", b3a["new_layer_null_at_t2"], False),

        # 2. Layer insertion order & duplication / splitting
        ("insert_null_none_sel", "layer_insertion", "4 solids [S1..S4], none selected: New Null -> inserted at L1 (top)", b3a["insert_null_none_sel"], False),
        ("insert_null_sel2", "layer_insertion", "4 solids [S1..S4], L2 (S2) selected: New Null -> inserted at L2 (above S2)", b3tl["insert_null_sel2"], False),
        ("insert_null_sel2_sel4", "layer_insertion", "4 solids [S1..S4], L2 & L4 selected: New Null -> inserted at L2 (above topmost selected)", b3tl["insert_null_sel2_sel4"], False),
        ("insert_shape_sel2", "layer_insertion", "4 solids [S1..S4], L2 (S2) selected: New Shape -> inserted at L2 (above S2)", b3tl["insert_shape_sel2"], False),
        ("layer_duplicate_ctrl_d_sel2", "layer_insertion", "4 solids [S1..S4], L2 (S2) selected: Edit > Duplicate (Ctrl+D, cmd 196624)", b3tl["layer_duplicate_ctrl_d_sel2"], False),
        ("layer_split_ctrl_shift_d", "layer_insertion", "Solid with 2 keyframes (1s, 5s) + Gaussian Blur, playhead t=3.0s: Split Layer (Ctrl+Shift+D, cmd 196625)", b3a["layer_split_ctrl_shift_d"], False),

        # 3. Work area & Trim Comp to Work Area
        ("workarea_B_2s_N_5s", "timeline_workarea", "10s@30fps comp: B at t=2.0s, N at t=5.0s -> workArea=[2.0, 5.03333] (91f inclusive)", b3d["workarea_B_2s_N_5s"], False),
        ("workarea_trim_comp", "timeline_workarea", "After B=2.0s & N=5.0s: Trim Comp to Work Area (Ctrl+Shift+X, cmd 327685)", b3d["workarea_trim_comp"], False),

        # 4. Layer bar shortcuts & direct mouse drags
        ("layer_move_in_bracket", "timeline_layer_bar", "TrimLayer (in=2, out=6, st=0), playhead t=3.0s: '[' aligns inPoint to 3.0s", b3d["layer_move_in_bracket"], False),
        ("layer_move_out_bracket", "timeline_layer_bar", "TrimLayer (in=2, out=6, st=0), playhead t=3.0s: ']' aligns outPoint to 3.03333s (1f inclusive)", b3d["layer_move_out_bracket"], False),
        ("layer_trim_in_alt_bracket", "timeline_layer_bar", "TrimLayer (in=2, out=6, st=0), playhead t=3.0s: Alt+[ trims inPoint to 3.0s", b3t["layer_trim_in_alt_bracket"], False),
        ("layer_trim_out_alt_bracket", "timeline_layer_bar", "TrimLayer (in=2, out=6, st=0), playhead t=3.0s: Alt+] trims outPoint to 3.03333s (1f inclusive)", b3t["layer_trim_out_alt_bracket"], False),
        ("layer_bar_drag_body_plus1_5s", "timeline_layer_bar", "TrimLayer (in=2, out=6, st=0): mouse drag bar body +1.5s", b3t["layer_bar_drag_body_plus1_5s"], False),
        ("layer_bar_drag_left_trim_to_3s", "timeline_layer_bar", "TrimLayer (in=2, out=6, st=0): mouse drag left edge 2.0s -> 3.0s", b3t["layer_bar_drag_left_trim_to_3s"], False),
        ("layer_bar_drag_right_trim_to_7_5s", "timeline_layer_bar", "TrimLayer (in=2, out=6, st=0): mouse drag right edge 6.0s -> 7.5s", b3t["layer_bar_drag_right_trim_to_7_5s"], False),
        ("layer_bar_shift_snap_to_playhead", "timeline_layer_bar", "TrimLayer (in=2, out=6, st=0), playhead t=3.0s: Shift-drag body near playhead -> inPoint snaps to 3.0s", b3t["layer_bar_shift_snap_to_playhead"], False),

        # 5. Keyframe easing, hold, nudge, reverse, marquee, drag & Alt time-scaling
        ("kf_easy_ease_f9", "timeline_keyframes", "3 Position keyframes [1s, 3s, 5s], middle selected: Easy Ease (F9, cmd 11075597)", b3a["kf_easy_ease_f9"], False),
        ("kf_easy_ease_in_shift_f9", "timeline_keyframes", "Middle keyframe selected: Easy Ease In (Shift+F9, cmd 11075595)", b3a["kf_easy_ease_in_shift_f9"], False),
        ("kf_easy_ease_out_ctrl_shift_f9", "timeline_keyframes", "Middle keyframe selected: Easy Ease Out (Ctrl+Shift+F9, cmd 11075596)", b3a["kf_easy_ease_out_ctrl_shift_f9"], False),
        ("kf_toggle_hold_ctrl_alt_h", "timeline_keyframes", "Middle keyframe selected: Toggle Hold Keyframe (Ctrl+Alt+H, cmd 851976)", b3a["kf_toggle_hold_ctrl_alt_h"], False),
        ("kf_time_reverse", "timeline_keyframes", "All 3 keyframes selected: Time-Reverse Keyframes (cmd 11075592)", b3a["kf_time_reverse"], False),
        ("kf_nudge_alt_right_1f", "timeline_keyframes", "Middle keyframe @ 90f selected: Alt+Right nudges +1 frame -> 91f", b3t["kf_nudge_alt_right_1f"], False),
        ("kf_nudge_alt_shift_right_10f", "timeline_keyframes", "After +1f nudge (@ 91f): Alt+Shift+Right nudges +10 frames -> 101f", b3t["kf_nudge_alt_shift_right_10f"], False),
        ("kf_drag_single_3s_to_4s", "timeline_keyframes", "Single keyframe drag from 3.0s (90f) -> 4.0s (120f)", b3t["kf_drag_single_3s_to_4s"], False),
        ("kf_shift_drag_snap_playhead", "timeline_keyframes", "Shift-drag keyframe from 3.0s near playhead t=4.0s (dropped at 3.96s) -> snaps to 4.0s (120f)", b3t["kf_shift_drag_snap_playhead"], False),
        ("kf_marquee_select_3", "timeline_keyframes", "Marquee drag across Position row [0.5s..5.8s] -> selects all 3 keyframes [30f, 90f, 150f]", b3kf["kf_marquee_select_3"], False),
        ("kf_drag_all3_plain_plus2s", "timeline_keyframes", "All 3 keyframes [1s, 3s, 5s] selected: plain drag last keyframe 5s -> 7s shifts all rigidly to [3s, 5s, 7s]", b3kf["kf_drag_all3_plain_plus2s"], False),
        ("kf_alt_drag_before_down", "timeline_keyframes", "All 3 keyframes [1s, 3s, 5s] selected: Alt held BEFORE mouse-down dragging 5s -> 7s scales proportionally to [1s, 4s, 7s]", b3kf["kf_alt_drag_before_down"], False),
        ("kf_alt_drag_after_down", "timeline_keyframes", "All 3 keyframes [1s, 3s, 5s] selected: Alt pressed AFTER mouse-down stays rigid shift [3s, 5s, 7s]", b3kf["kf_alt_drag_after_down"], False),

        # 6. Drag-and-Drop (Project -> Comp Viewer & Timeline)
        ("dnd_project_to_viewer_at_500_400", "drag_and_drop", "Drag 01_DragA from Project -> Comp Viewer at (500, 400)", b3d["dnd_project_to_viewer_at_500_400"], False),
        ("dnd_project_to_viewer_shift_500_400", "drag_and_drop", "Shift-drag 01_DragA from Project -> Comp Viewer at (500, 400) (position stays at drop point)", b3d["dnd_project_to_viewer_shift_500_400"], False),
        ("dnd_project_multi_to_viewer_500_400", "drag_and_drop", "Multi-select [01_DragA, 02_DragB] dragged from Project -> Comp Viewer at (500, 400)", b3fin["dnd_project_to_viewer_shift_after_500_400"], False),
        ("dnd_project_to_timeline_empty", "drag_and_drop", "Drag 01_DragA from Project -> empty Timeline area (centered at [960, 540])", b3d["dnd_project_to_timeline_empty"], False),
        ("dnd_project_to_timeline_between_L1_L2", "drag_and_drop", "Drag 01_DragA from Project -> Timeline gap between L1 (Existing1) and L2 (Existing2)", b3d["dnd_project_to_timeline_between_L1_L2"], False),
        ("dnd_project_to_timeline_bottom", "drag_and_drop", "Drag 01_DragA from Project -> Timeline below bottom layer (L2)", b3d["dnd_project_to_timeline_bottom"], False),
        ("dnd_project_multi_to_timeline", "drag_and_drop", "Multi-select [01_DragA, 02_DragB] dragged from Project -> Timeline", b3d["dnd_project_multi_to_timeline"], False),

        # 7. Effects application & stack order
        ("effects_apply_first_gaussian_blur", "effects_application", "1 solid selected: apply Effect > Blur & Sharpen > Gaussian Blur (cmd 460652560)", b3fin["effects_apply_first_gaussian_blur"], False),
        ("effects_apply_second_levels", "effects_application", "After Gaussian Blur: apply Effect > Color Correction > Levels (cmd 461307923) -> appended at index 2", b3fin["effects_apply_second_levels"], False),
        ("effects_apply_multi_selected", "effects_application", "2 solids [Existing1, Existing2] selected: apply Gaussian Blur -> applied to both", b3fin["effects_apply_multi_selected"], False),
        ("effects_apply_none_selected", "effects_application", "0 layers selected: apply Gaussian Blur -> no-op", b3fin["effects_apply_none_selected"], False),
    ]

    cases = []
    for cid, group, desc, raw_dump, inc_dlg in case_specs:
        cases.append({
            "id": cid,
            "group": group,
            "description": desc,
            "measured": summarize_dump(raw_dump, include_dialog=inc_dlg),
        })

    return {
        "schemaVersion": "1.0",
        "source": {
            "application": "Adobe After Effects 26.0 (26.0x67)",
            "platform": "Windows 11 Pro x64 (3840x2160 @ 150% DPI)",
            "locale": "zh_CN",
            "testComp": {"width": 1920, "height": 1080, "frameRate": 30, "duration": 10},
            "method": "Live Win32 input + WM_COMMAND + ExtendScript comp state dump + MSAA modal dialog capture",
        },
        "summary": {
            "totalMeasuredCases": len(cases),
            "groups": [
                "new_layers",
                "layer_insertion",
                "timeline_workarea",
                "timeline_layer_bar",
                "timeline_keyframes",
                "drag_and_drop",
                "effects_application",
            ],
            "keyRules": {
                "newLayerTiming": "All 7 new layer types span full comp (inPoint=0, outPoint=comp.duration, startTime=0), regardless of current playhead time.",
                "newLayerInsertionIndex": "0 selected -> index 1 (top); single L_k selected -> index k (directly above L_k); multi-select -> directly above topmost selected layer. Selection resets to ONLY the newly created layer.",
                "nullLayerDefaults": "100x100 solid with anchorPoint=[0,0,0] (top-left, NOT [50,50]), position=compCenter, opacity=0, label=1.",
                "adjustmentLayerDefaults": "comp-sized solid with anchorPoint=compCenter, position=compCenter, opacity=100, label=5.",
                "cameraDefaults": "CameraLayer, label=4, POI/anchorPoint=compCenter, position=[cx, cy, -zoom] where zoom = comp.width * focalLength / filmSize; Depth of Field off (0), Blur Level 100.",
                "lightDefaults": "LightLayer (default Spot), label=6, POI/anchorPoint=compCenter, position=[cx+80, cy-80, -666.66667], Intensity=100, Color=[1,1,1,1], ConeAngle=90, ConeFeather=50, Falloff=None(1), CastsShadows=0.",
                "duplicateLayer": "Ctrl+D (cmd 196624) inserts duplicate directly ABOVE the original (at original index, shifting original down by 1) and selects only the new duplicate.",
                "splitLayer": "Ctrl+Shift+D (cmd 196625) keeps lower/earlier half at original index+1 (outPoint=playhead, unselected) and places new upper/later half directly ABOVE at original index (inPoint=playhead, selected). Both halves retain ALL keyframes (including outside trimmed span) and ALL effects.",
                "oneFrameInclusiveEndRule": "N (work area end), ']' (align layer out), and Alt+] (trim layer out) are ALL 1-frame inclusive: end = playhead + 1/fps so the frame at the playhead remains visible. B, '[', and Alt+[ align exactly to playhead.",
                "trimCompToWorkArea": "Ctrl+Shift+X (cmd 327685) sets comp.duration = workAreaDuration, workAreaStart = 0, shifts every layer's startTime by -oldWorkAreaStart, and clamps inPoint/outPoint to [0, newDuration].",
                "keyframeEasingDefaults": "Linear default temporal influence = 16.66667% (1/6). Easy Ease (F9) sets both sides to BEZIER {speed:0, influence:33.33333} (1/3). Shift+F9 affects in-side only; Ctrl+Shift+F9 affects out-side only. Ctrl+Alt+H sets outInterp=HOLD (inInterp unchanged).",
                "keyframeAltTimeScaling": "Alt MUST be held BEFORE mouse-down on the first or last keyframe of a multi-keyframe selection to enter proportional time-scaling anchored at the opposite end; pressing Alt after drag start keeps rigid translation.",
                "keyframeNudge": "Alt+Left/Right nudges selected keyframes by +/-1 frame; Alt+Shift+Left/Right nudges by +/-10 frames.",
                "projectDragAndDrop": "Project -> Comp Viewer places layer(s) at exact drop coordinate (in comp space); Project -> Timeline always centers layer(s) at comp center [cx, cy] and inserts at the drop slot (empty/between/bottom). Multi-select preserves Project list order top-to-bottom and selects all dropped layers.",
                "effectsStackOrder": "Applying an effect appends it to the BOTTOM of the layer's Effects stack (index 1 = first applied, index 2 = second applied) with enabled=true. Multi-layer selection applies the effect to every selected layer; 0 layers selected is a no-op.",
            },
        },
        "cases": cases,
    }


def render_markdown(data: dict[str, Any]) -> str:
    by_id = {c["id"]: c for c in data["cases"]}

    def fmt_vec(v: list[Any] | None) -> str:
        if not v:
            return "-"
        return "[" + ", ".join(str(x) for x in v) + "]"

    lines: list[str] = []
    lines.append("# After Effects 26 交互实测：新建图层、时间轴与拖放（第 3 批）")
    lines.append("")
    lines.append("> **数据来源**：Windows 11 Pro x64（3840×2160 @ 150% DPI）下对 Adobe After Effects 26.0（`26.0x67`，`zh_CN`）实机发送 Win32 键鼠事件与 `WM_COMMAND`，由 ExtendScript 实时回读合成、图层、关键帧与效果状态，并采集模态对话框控件树。")
    lines.append("> **测试合成基准**：`1920×1080`，`30 fps`，时长 `10 s`（300 帧），缩放 `100%`。")
    lines.append(f"> **实测用例总数**：`{data['summary']['totalMeasuredCases']}` 个，完整结构化 JSON 见 [`layers-timeline-dnd.json`](layers-timeline-dnd.json)。")
    lines.append("")

    # Section 1: New Layers
    lines.append("## 1. 新建图层（默认值、时间范围与模态对话框）")
    lines.append("")
    lines.append("### 1.1 全局规则（实测确认）")
    lines.append("- **时间范围与播放头无关**：无论播放头位于 `t=0` 还是 `t=2.0s`（见 `new_layer_null_at_t2`），所有新建图层默认铺满整个合成：`inPoint = 0`、`outPoint = comp.duration (10)`、`startTime = 0`。")
    lines.append("- **选中状态**：新建后仅新图层处于选中状态（原选中图层全部取消选中）。")
    lines.append("- **插入位置（实测 4 图层 `[S1..S4]`）**：")
    lines.append("  - 未选中任何图层（`insert_null_none_sel`）→ 插入到最顶层 **`index = 1`**。")
    lines.append("  - 选中单个图层 `L2`（`insert_null_sel2`、`insert_shape_sel2`）→ 插入到 **`index = 2`（紧邻 `L2` 上方）**，原 `L2` 下移至 `L3`。")
    lines.append("  - 多选非连续图层 `L2 + L4`（`insert_null_sel2_sel4`）→ 插入到 **`index = 2`（紧邻最上方选中图层之上）**。")
    lines.append("")
    lines.append("### 1.2 七类新建图层精确默认值（`1920×1080 @ 30fps, 10s`）")
    lines.append("")
    lines.append("| 图层类型 | 快捷键 / `commandId` | 模态框 | 默认名称 (`zh_CN`) | `layerType` | `label` | 尺寸 `(W×H)` | 默认 `AnchorPoint` / POI | 默认 `Position` | `Opacity` | 关键特异点 |")
    lines.append("|---|---|---|---|---|---:|---|---|---|---|---|")

    layer_rows = [
        ("纯色 Solid", "`Ctrl+Y` / `11862018`", "有", "new_layer_solid", "尺寸默认与合成一致；记忆上次颜色（本机实测红 `[1,0,0]`）"),
        ("文本 Text", "`Ctrl+Alt+Shift+T` / `11862017`", "无", "new_layer_text", "空文本层 `Anchor=[0,0,0]`、`Position=合成中心`"),
        ("形状 Shape", "无 / `11862022`", "无", "new_layer_shape", "空形状层 `Anchor=[0,0,0]`、`Position=合成中心`、`label=8`(蓝)"),
        ("空对象 Null", "`Ctrl+Alt+Shift+Y` / `11862021`", "无", "new_layer_null", "**`100×100`、`Anchor=[0,0,0]`(左上角而非中心)、`Opacity=0`**"),
        ("调整图层 Adjustment", "`Ctrl+Alt+Y` / `11862023`", "无", "new_layer_adjustment", "实质为合成尺寸白底纯色（`adjustmentLayer=true`），`label=5`(淡紫)"),
        ("摄像机 Camera", "`Ctrl+Alt+Shift+C` / `11862020`", "有", "new_layer_camera", "`POI=[cx,cy,0]`，`Position=[cx,cy,-zoom]`；`zoom = comp.w × focalLength / filmSize`"),
        ("灯光 Light", "`Ctrl+Alt+Shift+L` / `11862019`", "有", "new_layer_light", "默认聚光 `SPOT`；`POI=[cx,cy,0]`，**`Position=[cx+80, cy-80, -666.66667]`**"),
    ]
    for title, sc, modal, cid, note in layer_rows:
        ly = by_id[cid]["measured"]["layers"][0]
        sz = f"{ly['width']}×{ly['height']}" if ly["width"] else "-"
        tf = ly["transform"]
        lines.append(
            f"| {title} | {sc} | {modal} | `{ly['name']}` | `{ly['layerType']}` | `{ly['label']}` | `{sz}` | `{fmt_vec(tf.get('anchorPoint'))}` | `{fmt_vec(tf.get('position'))}` | `{tf.get('opacity')}` | {note} |"
        )
    lines.append("")

    lines.append("### 1.3 摄像机与灯光专属参数默认值")
    lines.append("")
    cam = by_id["new_layer_camera"]["measured"]["layers"][0]["cameraOptions"]
    lgt = by_id["new_layer_light"]["measured"]["layers"][0]["lightOptions"]
    lines.append("- **摄像机（`CameraLayer`）**：")
    lines.append(f"  - `Zoom` = `{cam['ADBE Camera Zoom']}` px（由当前预设焦距 `66.67 mm` 与胶片宽度 `36.00 mm` 计算：`1920 × 66.6667 / 36 = 3555.5556`；50mm 预设对应 `2666.6667`）。")
    lines.append(f"  - `Depth of Field` = `{cam['ADBE Camera Depth of Field']}`（关闭），`Focus Distance` = `{cam['ADBE Camera Focus Distance']}`（锁定等于 Zoom），`Aperture` = `{cam['ADBE Camera Aperture']}`，`Blur Level` = `{cam['ADBE Camera Blur Level']}%`。")
    lines.append(f"  - 光圈/散景默认：`Iris Shape = {cam['ADBE Iris Shape']}`、`Rotation = 0`、`Roundness = 0`、`Aspect Ratio = 1`、`Diffraction Fringe = 0`、`Highlight Gain = 0`、`Highlight Threshold = 1`、`Highlight Saturation = 0`。")
    lines.append("  - 若当前合成中不存在任何 3D 图层，确定后 AE 26 会在每个会话首次弹出一条提示对话框提醒摄像机仅影响 3D 图层（可勾选不再显示）。")
    lines.append("- **灯光（`LightLayer`）**：")
    lines.append(f"  - 默认类型：`SPOT`（聚光灯），`Intensity = {lgt['ADBE Light Intensity']}%`，`Color = {lgt['ADBE Light Color']}`（纯白）。")
    lines.append(f"  - 聚光参数：`Cone Angle = {lgt['ADBE Light Cone Angle']}°`，`Cone Feather = {lgt['ADBE Light Cone Feather 2']}%`。")
    lines.append(f"  - 衰减与阴影：`Falloff Type = {lgt['ADBE Light Falloff Type']}`（无），`Falloff Start (半径) = {lgt['ADBE Light Falloff Start']}`，`Falloff Distance (衰减距离) = {lgt['ADBE Light Falloff Distance']}`，`Casts Shadows = {lgt['ADBE Casts Shadows']}`（关闭），`Shadow Darkness = {lgt['ADBE Light Shadow Darkness']}%`，`Shadow Diffusion = {lgt['ADBE Light Shadow Diffusion']}`。")
    lines.append("  - 与摄像机相同，无 3D 图层时会话首次会弹出提示对话框。")
    lines.append("")

    lines.append("### 1.4 模态对话框控件清单（纯色 / 摄像机 / 灯光）")
    lines.append("")
    for cid, heading in [
        ("new_layer_solid", "纯色设置对话框（`Ctrl+Y`）"),
        ("new_layer_camera", "摄像机设置对话框（`Ctrl+Alt+Shift+C`）"),
        ("new_layer_light", "灯光设置对话框（`Ctrl+Alt+Shift+L`）"),
    ]:
        lines.append(f"#### {heading}")
        lines.append("")
        lines.append("| 序号 | 控件类型 | 标签 / 名称 | 默认值 / 状态 |")
        lines.append("|---:|---|---|---|")
        for idx, ctrl in enumerate(by_id[cid]["measured"].get("dialogControls", []), 1):
            val = ctrl.get("defaultValue", "")
            if ctrl["type"] == "checkbox":
                val = "勾选" if ctrl.get("checked") else "未勾选"
            lines.append(f"| {idx} | `{ctrl['type']}` | `{ctrl['name']}` | `{val}` |")
        lines.append("")

    # Section 2: Duplicate & Split
    lines.append("## 2. 复制图层与拆分图层")
    lines.append("")
    lines.append("| 操作 | 快捷键 / `commandId` | 初始状态 | 实测结果 | 实现要点 |")
    lines.append("|---|---|---|---|---|")
    lines.append("| 复制图层 Duplicate | `Ctrl+D` / `196624` | `[S1..S4]`，选中 `L2 (S2)` | `L2` 为新副本（`sel=true`），原 `S2` 下移至 `L3`（`sel=false`） | **插在原图层正上方**（同 index），仅选中副本 |")
    lines.append("| 拆分图层 Split Layer | `Ctrl+Shift+D` / `196625` | `SplitLayer`（`in=0, out=10`，含 `1s/5s` 关键帧与高斯模糊），`t=3.0s` | `L1`: `in=3, out=10, st=0, sel=true`；`L2`: `in=0, out=3, st=0, sel=false` | **后半段新建并放在正上方 `L1` 且选中**，前半段保留在 `L2`；**两半均完整保留全部关键帧（含裁剪区间外的关键帧）与全部效果** |")
    lines.append("")

    # Section 3: Timeline Work Area & Layer Bar
    lines.append("## 3. 时间轴：工作区与图层条操作")
    lines.append("")
    lines.append("### 3.1 核心不变量：右端 1 帧包含规则（1-Frame Inclusive End）")
    lines.append("")
    lines.append("AE 26 在 30 fps（每帧 `1/30 = 0.03333 s`）下，所有“对齐/裁剪到播放头作为**终点**”的快捷键均**包含播放头当前这一帧**：")
    lines.append("- **起点类命令**（`B`、`[`、`Alt+[`）：精确对齐 `playhead`（例如 `t = 3.0s` → `3.0`）。")
    lines.append("- **终点类命令**（`N`、`]`、`Alt+]`）：对齐到 **`playhead + 1/fps`**（例如 `t = 3.0s` → `3.03333s`；`t = 5.0s` → `5.03333s`），确保播放头所在帧仍处于可见/渲染范围内。")
    lines.append("")

    lines.append("### 3.2 工作区（`B` / `N` / 修剪合成至工作区）")
    lines.append("")
    lines.append("| 用例 | 操作 | 实测合成时长 `duration` | 实测工作区 `(start, dur)` | 图层 `TrimLayer` (`in=2, out=6, st=0`) 结果 |")
    lines.append("|---|---|---:|---|---|")
    wa1 = by_id["workarea_B_2s_N_5s"]["measured"]
    wa2 = by_id["workarea_trim_comp"]["measured"]
    lines.append(f"| `workarea_B_2s_N_5s` | `t=2.0s` 按 `B`，`t=5.0s` 按 `N` | `{wa1['comp']['duration']}` | `({wa1['comp']['workAreaStart']}, {wa1['comp']['workAreaDuration']})`（91 帧） | 不变 `in=2, out=6, st=0` |")
    ly2 = wa2["layers"][0]
    lines.append(f"| `workarea_trim_comp` | `Ctrl+Shift+X`（`327685`） | `{wa2['comp']['duration']}` | `({wa2['comp']['workAreaStart']}, {wa2['comp']['workAreaDuration']})` | `in={ly2['inPoint']}, out={ly2['outPoint']}, startTime={ly2['startTime']}` |")
    lines.append("")
    lines.append("修剪合成公式：`newDuration = oldWorkAreaDuration`，`workAreaStart = 0`，每个图层的 `startTime -= oldWorkAreaStart`，`inPoint`/`outPoint` 按 `[0, newDuration]` 裁剪。")
    lines.append("")

    lines.append("### 3.3 图层条快捷键与鼠标拖拽（初始 `TrimLayer`: `in=2, out=6, st=0`，播放头 `t=3.0s`）")
    lines.append("")
    lines.append("| 用例 | 操作 | `inPoint` | `outPoint` | `startTime` | 行为规则 |")
    lines.append("|---|---|---:|---:|---:|---|")
    bar_cases = [
        ("layer_move_in_bracket", "`[`（移动入点到播放头）", "平移整个图层条，保持时长不变：`delta = playhead - inPoint`"),
        ("layer_move_out_bracket", "`]`（移动出点到播放头）", "**1 帧包含**：`out = playhead + 1/fps`，整体平移 `delta = (playhead + 1/fps) - outPoint`"),
        ("layer_trim_in_alt_bracket", "`Alt+[`（修剪入点到播放头）", "仅改 `inPoint = playhead`，`startTime` 与 `outPoint` 不变"),
        ("layer_trim_out_alt_bracket", "`Alt+]`（修剪出点到播放头）", "**1 帧包含**：仅改 `outPoint = playhead + 1/fps`，`startTime` 与 `inPoint` 不变"),
        ("layer_bar_drag_body_plus1_5s", "鼠标拖拽图层条主体 `+1.5s`", "`in`/`out`/`startTime` 同幅平移 `+1.5s`"),
        ("layer_bar_drag_left_trim_to_3s", "鼠标拖拽左边缘 `2.0s → 3.0s`", "仅修改 `inPoint = 3.0`，`startTime` 不变"),
        ("layer_bar_drag_right_trim_to_7_5s", "鼠标拖拽右边缘 `6.0s → 7.5s`", "仅修改 `outPoint = 7.5`，`startTime` 不变"),
        ("layer_bar_shift_snap_to_playhead", "`Shift`+拖拽主体靠近 `t=3.0s`", "图层条入点自动吸附到播放头精确值 `3.0s`"),
    ]
    for cid, op, rule in bar_cases:
        ly = by_id[cid]["measured"]["layers"][0]
        lines.append(f"| `{cid}` | {op} | `{ly['inPoint']}` | `{ly['outPoint']}` | `{ly['startTime']}` | {rule} |")
    lines.append("")

    # Section 4: Keyframes
    lines.append("## 4. 时间轴：关键帧缓动、定格、微调、拖拽与比例缩放")
    lines.append("")
    lines.append("基准：`KfLayer` 位置属性 3 个线性关键帧：`K1 = 30f (1.0s) [400,540]`、`K2 = 90f (3.0s) [960,540]`、`K3 = 150f (5.0s) [1520,540]`。线性默认时间影响（influence）为 **`16.66667%`（`1/6`）**。")
    lines.append("")
    lines.append("### 4.1 缓动、定格与时间反向")
    lines.append("")
    lines.append("| 用例 | 快捷键 / `commandId` | 目标 | 实测 `inInterp` / `outInterp` | 实测 `inEase` / `outEase` (`speed, influence`) | 说明 |")
    lines.append("|---|---|---|---|---|---|")
    for cid, sc, target, note in [
        ("kf_easy_ease_f9", "`F9` / `11075597`", "仅选中 `K2`", "两侧均置 `BEZIER`，速度清零，影响 **`33.33333%` (`1/3`)**"),
        ("kf_easy_ease_in_shift_f9", "`Shift+F9` / `11075595`", "仅选中 `K2`", "仅入侧变为 `BEZIER {0, 33.33333}`，出侧保持 `LINEAR`"),
        ("kf_easy_ease_out_ctrl_shift_f9", "`Ctrl+Shift+F9` / `11075596`", "仅选中 `K2`", "仅出侧变为 `BEZIER {0, 33.33333}`，入侧保持 `LINEAR`"),
        ("kf_toggle_hold_ctrl_alt_h", "`Ctrl+Alt+H` / `851976`", "仅选中 `K2`", "仅出侧变为 `HOLD {0, 16.66667}`，入侧保持 `LINEAR`"),
    ]:
        k2 = by_id[cid]["measured"]["layers"][0]["keys"]["position"][1]
        ie = k2["inEase"][0]
        oe = k2["outEase"][0]
        lines.append(
            f"| `{cid}` | {sc} | {target} | `{k2['inInterp']}` / `{k2['outInterp']}` | `({ie['speed']}, {ie['influence']})` / `({oe['speed']}, {oe['influence']})` | {note} |"
        )
    rev_kfs = by_id["kf_time_reverse"]["measured"]["layers"][0]["keys"]["position"]
    rev_vals = ", ".join(f"{k['frame']}f:{k['value'][:2]}" for k in rev_kfs)
    lines.append(f"| `kf_time_reverse` | 菜单 / `11075592` | 全选 3 帧 | 保持 `LINEAR` | 保持原缓动 | 时间不变，数值首尾反转：`{rev_vals}`，保持全选 |")
    lines.append("")

    lines.append("### 4.2 关键帧微调、框选、拖拽、吸附与 `Alt` 时间比例缩放")
    lines.append("")
    lines.append("| 用例 | 操作 | 实测帧号 `[K1, K2, K3]` | 实现要点 |")
    lines.append("|---|---|---|---|")
    kf_drag_rows = [
        ("kf_nudge_alt_right_1f", "`Alt+Right`（选中 `K2 @ 90f`）", "单帧微调 `+1 frame`"),
        ("kf_nudge_alt_shift_right_10f", "`Alt+Shift+Right`（接上 `91f`）", "大步微调 `+10 frames`"),
        ("kf_drag_single_3s_to_4s", "鼠标单帧拖拽 `3.0s → 4.0s`", "单选拖动仅移动该帧"),
        ("kf_shift_drag_snap_playhead", "`Shift`+拖拽 `K2` 至 `3.96s`（播放头在 `4.0s`）", "吸附到播放头精确帧 `120f (4.0s)`"),
        ("kf_marquee_select_3", "时间轴空白处框选 `[0.5s..5.8s]`", "命中区间内全部关键帧并高亮选中"),
        ("kf_drag_all3_plain_plus2s", "全选 3 帧，普通拖拽 `K3` 从 `5s → 7s`", "全部选中关键帧刚性平移 `+2.0s`"),
        ("kf_alt_drag_before_down", "全选 3 帧，**按下鼠标前先按住 `Alt`** 拖 `K3` `5s → 7s`", "**按首帧 `1s` 为锚点等比缩放（`1.5×`）**，`K2` 由 `3s` 精确缩放到 `4s (120f)`"),
        ("kf_alt_drag_after_down", "全选 3 帧，**开始拖拽后再按 `Alt`** 拖 `K3` `5s → 7s`", "**不进入缩放模式**，保持刚性平移 `[90, 150, 210]`（修饰键须在 mouse-down 时判定）"),
    ]
    for cid, op, note in kf_drag_rows:
        kfs = by_id[cid]["measured"]["layers"][0]["keys"]["position"]
        fr = "[" + ", ".join(str(k["frame"]) for k in kfs) + "]"
        lines.append(f"| `{cid}` | {op} | `{fr}` | {note} |")
    lines.append("")

    # Section 5: Drag-and-Drop
    lines.append("## 5. 拖放行为（Project 面板 → 合成视口 / 时间轴）")
    lines.append("")
    lines.append("测试素材：项目根目录 `01_DragA`、`02_DragB`（均为 `300×200` 纯色素材项）。")
    lines.append("")
    lines.append("| 用例 | 拖放路径与修饰键 | 结果图层栈（自上而下） | 关键规则 |")
    lines.append("|---|---|---|---|")
    dnd_rows = [
        ("dnd_project_to_viewer_at_500_400", "Project `01_DragA` → 合成视口 `(500, 400)`", "落点即图层 `Position = [500, 400]`，`in=0, out=10`"),
        ("dnd_project_to_viewer_shift_500_400", "Project `01_DragA` + `Shift` → 合成视口 `(500, 400)`", "仍落在 `[500, 400]`（`Shift` 不强制居中到视口中心）"),
        ("dnd_project_multi_to_viewer_500_400", "Project 多选 `[01_DragA, 02_DragB]` → 合成视口 `(500, 400)`", "两层均落在 `[500, 400]`，保持项目列表先后顺序（`A` 在 `L1`、`B` 在 `L2`），全部选中"),
        ("dnd_project_to_timeline_empty", "Project `01_DragA` → 空时间轴区域", "**自动居中到合成中心 `[960, 540]`**"),
        ("dnd_project_to_timeline_between_L1_L2", "Project `01_DragA` → 时间轴 `L1` 与 `L2` 之间", "精确插入 `L2`（居中 `[960, 540]`），仅选中新插入图层"),
        ("dnd_project_to_timeline_bottom", "Project `01_DragA` → 时间轴最底图层下方", "追加至最底 `L3`（居中 `[960, 540]`），仅选中新插入图层"),
        ("dnd_project_multi_to_timeline", "Project 多选 `[01_DragA, 02_DragB]` → 时间轴", "保持列表顺序（`L1=01_DragA`, `L2=02_DragB`），全部居中 `[960, 540]` 且全部选中"),
    ]
    for cid, op, note in dnd_rows:
        lys = by_id[cid]["measured"]["layers"]
        stack = " / ".join(f"`L{l['index']}:{l['name']}`(P={l['transform']['position'][:2]},sel={l['selected']})" for l in lys)
        lines.append(f"| `{cid}` | {op} | {stack} | {note} |")
    lines.append("")

    # Section 6: Effects
    lines.append("## 6. 效果添加与堆叠顺序")
    lines.append("")
    lines.append("| 用例 | 操作 | 实测 `effects` 栈 | 行为规则 |")
    lines.append("|---|---|---|---|")
    fx_rows = [
        ("effects_apply_first_gaussian_blur", "单选 1 图层，添加 `高斯模糊`（`460652560`）", "新增至 `index=1`，默认 `enabled=true`，图层保持选中"),
        ("effects_apply_second_levels", "继续添加 `色阶`（`461307923`）", "**追加至效果栈底部**（`index=1` 高斯模糊、`index=2` 色阶），先添加先执行"),
        ("effects_apply_multi_selected", "多选 `L1 + L2`，添加 `高斯模糊`", "**所有选中图层同时添加该效果**，保持原多选状态"),
        ("effects_apply_none_selected", "未选中任何图层，触发 `高斯模糊`", "静默忽略（不添加、不报错）"),
    ]
    for cid, op, note in fx_rows:
        lys = by_id[cid]["measured"]["layers"]
        parts = []
        for l in lys:
            fxs = l.get("effects") or []
            fx_s = ", ".join(f"{e['index']}:{e['matchName']}(en={e['enabled']})" for e in fxs) if fxs else "无"
            parts.append(f"`L{l['index']}`:[{fx_s}]")
        lines.append(f"| `{cid}` | {op} | {' ; '.join(parts)} | {note} |")
    lines.append("")

    return "\n".join(lines) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--raw-dir",
        type=Path,
        default=Path(os.environ.get("AE_RAW_DIR", ".")),
        help="Directory containing live-measured Batch 3 raw JSON dumps",
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=Path("docs/reference/ae/interaction"),
        help="Output directory for generated reference JSON and Markdown",
    )
    args = parser.parse_args()

    dataset = build_dataset(args.raw_dir)
    args.out_dir.mkdir(parents=True, exist_ok=True)

    json_path = args.out_dir / "layers-timeline-dnd.json"
    md_path = args.out_dir / "layers-timeline-dnd.md"

    with json_path.open("w", encoding="utf-8", newline="\n") as f:
        json.dump(dataset, f, ensure_ascii=False, indent=2)
        f.write("\n")

    md_text = render_markdown(dataset)
    with md_path.open("w", encoding="utf-8", newline="\n") as f:
        f.write(md_text)

    print(f"Wrote {json_path} ({json_path.stat().st_size} bytes)")
    print(f"Wrote {md_path} ({md_path.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
