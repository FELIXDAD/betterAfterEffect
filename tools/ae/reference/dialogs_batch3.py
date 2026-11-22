#!/usr/bin/env python3
"""Batch 3 Windows AE 26.0 dialog builders.

Covers:
  - Keyframe Interpolation (关键帧插值)
  - Keyframe Velocity (关键帧速度)
  - Go to Time (转到时间)
  - Markers: Composition Marker & Layer Marker (合成标记 / 图层标记)
  - Adobe Color Picker (拾色器)
"""

from __future__ import annotations

from typing import Any

from dialogs_batch2 import COMMON_SOURCE


LABEL_COLOR_OPTIONS: list[dict[str, str]] = [
    {"zh": "无", "en": "None"},
    {"zh": "红色", "en": "Red"},
    {"zh": "黄色", "en": "Yellow"},
    {"zh": "浅绿色", "en": "Aqua"},
    {"zh": "粉色", "en": "Pink"},
    {"zh": "淡紫色", "en": "Lavender"},
    {"zh": "桃红色", "en": "Peach"},
    {"zh": "海泡沫", "en": "Sea Foam"},
    {"zh": "蓝色", "en": "Blue"},
    {"zh": "绿色", "en": "Green"},
    {"zh": "紫色", "en": "Purple"},
    {"zh": "橙色", "en": "Orange"},
    {"zh": "棕色", "en": "Brown"},
    {"zh": "紫红色", "en": "Fuchsia"},
    {"zh": "青色", "en": "Cyan"},
    {"zh": "砂岩", "en": "Sandstone"},
    {"zh": "深绿色", "en": "Dark Green"},
]


def build_keyframe_interpolation(raw: dict[str, Any]) -> dict[str, Any]:
    rect = raw["kfInterp_pos"]["rect"]
    return {
        "schemaVersion": "1.0",
        "id": "keyframe-interpolation",
        "title": {"zh": "关键帧插值", "en": "Keyframe Interpolation"},
        "triggers": [
            {"action": "Keyframe Interpolation", "shortcutWin": "Ctrl+Alt+K", "menuZh": "动画 ▸ 关键帧插值..."},
        ],
        "source": COMMON_SOURCE,
        "geometry": {
            "physicalPxAt144Dpi": {"w": rect["w"], "h": rect["h"]},
            "logicalPxAt96Dpi": {"w": round(rect["w"] / 1.5), "h": round(rect["h"] / 1.5)},
        },
        "persistence": {
            "remembersLastValues": False,
            "notesZh": "打开时直接反映所选关键帧的当前插值类型（多选混合状态时显示“当前设置”），不记住上次弹窗选择。",
        },
        "buttonBar": {
            "position": "bottom_right",
            "right": [
                {"id": "ok", "type": "button", "zh": "确定", "en": "OK", "role": "accept"},
                {"id": "cancel", "type": "button", "zh": "取消", "en": "Cancel", "role": "reject"},
            ],
        },
        "tabs": [],
        "controls": [
            {
                "id": "temporal_interpolation",
                "type": "dropdown",
                "zh": "临时插值:",
                "en": "Temporal Interpolation:",
                "defaultValue": {"zh": "线性", "en": "Linear"},
                "options": [
                    {"zh": "当前设置", "en": "Current Settings"},
                    {"zh": "线性", "en": "Linear"},
                    {"zh": "贝塞尔曲线", "en": "Bezier"},
                    {"zh": "连续贝塞尔曲线", "en": "Continuous Bezier"},
                    {"zh": "自动贝塞尔曲线", "en": "Auto Bezier"},
                    {"zh": "定格", "en": "Hold"},
                ],
                "enableRuleZh": "选中任意关键帧时始终可用（注：zh_CN 界面将 Temporal 译为“临时插值:”）。",
            },
            {
                "id": "spatial_interpolation",
                "type": "dropdown",
                "zh": "空间插值:",
                "en": "Spatial Interpolation:",
                "defaultValue": {"zh": "线性", "en": "Linear"},
                "options": [
                    {"zh": "当前设置", "en": "Current Settings"},
                    {"zh": "线性", "en": "Linear"},
                    {"zh": "贝塞尔曲线", "en": "Bezier"},
                    {"zh": "连续贝塞尔曲线", "en": "Continuous Bezier"},
                    {"zh": "自动贝塞尔曲线", "en": "Auto Bezier"},
                ],
                "enableRuleZh": "仅空间属性（位置/锚点/目标点/效果点等）可用；非空间属性（缩放/不透明度/旋转）置灰禁用且值为空；不含“定格”项。",
            },
            {
                "id": "roving",
                "type": "dropdown",
                "zh": "浮动:",
                "en": "Roving:",
                "defaultValue": {"zh": "锁定到时间", "en": "Lock to Time"},
                "options": [
                    {"zh": "当前设置", "en": "Current Settings"},
                    {"zh": "漂浮穿梭时间", "en": "Rove Across Time"},
                    {"zh": "锁定到时间", "en": "Lock to Time"},
                ],
                "enableRuleZh": "仅空间属性的中间关键帧（非首帧、非末帧）可用；非空间属性或首/末关键帧置灰禁用。",
            },
        ],
    }


def build_keyframe_velocity(raw: dict[str, Any]) -> dict[str, Any]:
    r_pos = raw["kfVel_pos"]["rect"]
    r_scl = raw["kfVel_scale"]["rect"]
    return {
        "schemaVersion": "1.0",
        "id": "keyframe-velocity",
        "title": {"zh": "关键帧速度", "en": "Keyframe Velocity"},
        "triggers": [
            {"action": "Keyframe Velocity", "shortcutWin": "Ctrl+Shift+K", "menuZh": "动画 ▸ 关键帧速度..."},
        ],
        "source": COMMON_SOURCE,
        "geometry": {
            "physicalPxAt144Dpi": {"w": r_pos["w"], "h": r_pos["h"]},
            "logicalPxAt96Dpi": {"w": round(r_pos["w"] / 1.5), "h": round(r_pos["h"] / 1.5)},
            "multiDim2dPhysicalPxAt144Dpi": {"w": r_scl["w"], "h": r_scl["h"]},
            "multiDim2dLogicalPxAt96Dpi": {"w": round(r_scl["w"] / 1.5), "h": round(r_scl["h"] / 1.5)},
        },
        "persistence": {
            "remembersLastValues": False,
            "notesZh": "实时读取当前选中关键帧的传入/传出速度、影响百分比与连续性状态；不同属性维度会动态调整窗口高度与行数。",
        },
        "buttonBar": {
            "position": "bottom_right",
            "right": [
                {"id": "ok", "type": "button", "zh": "确定", "en": "OK", "role": "accept"},
                {"id": "cancel", "type": "button", "zh": "取消", "en": "Cancel", "role": "reject"},
            ],
        },
        "tabs": [],
        "controls": [
            {
                "id": "keyframe_type_header",
                "type": "static_text",
                "zh": "关键帧类型: <属性名>",
                "en": "Keyframe Type: <Property Name>",
                "notesZh": "实测显示“关键帧类型: 位置”、“关键帧类型: 缩放”、“关键帧类型: 不透明度”。",
            },
            {
                "id": "lock_dimensions_in_out",
                "type": "checkbox_pair",
                "zh": "锁定维度（传入 / 传出各一）",
                "en": "Lock Dimensions (Incoming & Outgoing)",
                "defaultValue": False,
                "enableRuleZh": "仅非空间多维属性（如 2D/3D 缩放）显示；单维属性（不透明度）与空间属性（位置）隐藏此控件。",
            },
            {
                "id": "incoming_velocity",
                "type": "number_scrub_group",
                "internalName": "VelInfGroup1..3_velocityValue",
                "zh": "传入速度（空间显示“速度:”；多维按“维度: X/Y/Z”分行）",
                "en": "Incoming Velocity (Speed: for spatial; Dimension: X/Y/Z rows for multi-dim)",
                "unit": "property_units/sec (px/sec for spatial)",
                "defaultValue": "<当前关键帧传入速度>",
                "notesZh": "左右双栏对称排布（左栏传入速度，右栏传出速度）；线性位置关键帧显示路径标量速率（px/s），缩放/不透明度显示带符号变化率（%/s）。",
            },
            {
                "id": "incoming_influence",
                "type": "number_scrub_group",
                "internalName": "VelInfGroup1..3_influenceValue",
                "zh": "传入影响:",
                "en": "Incoming Influence:",
                "unit": "%",
                "defaultValue": 33.33,
                "range": {"min": 0.1, "max": 100.0, "uiLinearDisplay": 0.01},
                "notesZh": "ExtendScript KeyframeEase 实测硬边界为 0.1–100%；线性关键帧在 UI 中初始显示 0.01%，缓动关键帧默认 33.33%。",
            },
            {
                "id": "outgoing_velocity",
                "type": "number_scrub_group",
                "internalName": "VelInfGroup4..6_velocityValue",
                "zh": "传出速度（结构同左栏）",
                "en": "Outgoing Velocity (mirrors Incoming)",
                "unit": "property_units/sec (px/sec for spatial)",
                "defaultValue": "<当前关键帧传出速度>",
            },
            {
                "id": "outgoing_influence",
                "type": "number_scrub_group",
                "internalName": "VelInfGroup4..6_influenceValue",
                "zh": "传出影响:",
                "en": "Outgoing Influence:",
                "unit": "%",
                "defaultValue": 33.33,
                "range": {"min": 0.1, "max": 100.0, "uiLinearDisplay": 0.01},
            },
            {
                "id": "continuous_lock",
                "type": "checkbox",
                "zh": "连续（锁定传出到传入）",
                "en": "Continuous (Lock Outgoing to Incoming)",
                "defaultValue": False,
                "enableRuleZh": "勾选后自动将传出速度锁定为与传入速度一致（转为连续贝塞尔）。",
            },
            {
                "id": "keyframe_status_footer",
                "type": "static_text",
                "zh": "对于在时间 <时间码> 的关键帧 #<序号>",
                "en": "For keyframe #<Index> at time <Timecode>",
                "notesZh": "右下按钮上方只读提示当前编辑的关键帧序号与时间位置。",
            },
        ],
    }


def build_go_to_time(raw: dict[str, Any]) -> dict[str, Any]:
    rect = raw["goToTime"]["rect"]
    return {
        "schemaVersion": "1.0",
        "id": "go-to-time",
        "title": {"zh": "转到时间", "en": "Go to Time"},
        "triggers": [
            {"action": "Go to Time", "shortcutWin": "Alt+Shift+J", "menuZh": "视图 ▸ 转到时间(G)..."},
        ],
        "source": COMMON_SOURCE,
        "geometry": {
            "physicalPxAt144Dpi": {"w": rect["w"], "h": rect["h"]},
            "logicalPxAt96Dpi": {"w": round(rect["w"] / 1.5), "h": round(rect["h"] / 1.5)},
        },
        "persistence": {
            "remembersLastValues": False,
            "notesZh": "每次打开自动填入当前合成的播放头时间（CTI）并全选，不保留上次输入值。",
        },
        "buttonBar": {
            "position": "bottom_right",
            "right": [
                {"id": "ok", "type": "button", "zh": "确定", "en": "OK", "role": "accept"},
                {"id": "cancel", "type": "button", "zh": "取消", "en": "Cancel", "role": "reject"},
            ],
        },
        "tabs": [],
        "controls": [
            {
                "id": "time_input",
                "type": "timecode_input",
                "zh": "时间码输入",
                "en": "Time Input",
                "defaultValue": "<当前合成播放头时间>",
                "notesZh": "支持绝对时间码/帧数输入与相对偏移（如 +10、-15 帧，+2. 表示 +2 秒）；右侧静态文本实时显示解析结果与帧率基准（如“是 0:00:02:00  基础 30”）。",
            }
        ],
    }


def build_markers(raw: dict[str, Any]) -> dict[str, Any]:
    r_comp = raw["marker_comp"]["rect"]
    r_layer = raw["marker_layer"]["rect"]
    return {
        "schemaVersion": "1.0",
        "id": "markers",
        "title": {"zh": "合成标记 / 图层标记", "en": "Composition Marker / Layer Marker"},
        "triggers": [
            {
                "action": "Add & Edit Composition Marker",
                "shortcutWin": "Alt+Numpad *",
                "menuZh": "未选中图层时按 Alt+Numpad *（或双击合成标记）",
            },
            {
                "action": "Add & Edit Layer Marker",
                "shortcutWin": "Alt+Numpad *",
                "menuZh": "选中图层时按 Alt+Numpad *（或双击图层标记）",
            },
        ],
        "source": COMMON_SOURCE,
        "geometry": {
            "physicalPxAt144Dpi": {"w": r_comp["w"], "h": r_comp["h"]},
            "logicalPxAt96Dpi": {"w": round(r_comp["w"] / 1.5), "h": round(r_comp["h"] / 1.5)},
            "layerMarkerPhysicalPxAt144Dpi": {"w": r_layer["w"], "h": r_layer["h"]},
            "layerMarkerLogicalPxAt96Dpi": {"w": round(r_layer["w"] / 1.5), "h": round(r_layer["h"] / 1.5)},
        },
        "persistence": {
            "remembersLastValues": False,
            "persistsPerMarker": True,
            "notesZh": "新建标记默认时间为当前播放头、持续时间 0:00:00:00、注释为空、标签为“无”；AE 26 已精简移除旧版章节/Web 链接/Flash 提示点字段。",
        },
        "buttonBar": {
            "position": "bottom_right",
            "right": [
                {"id": "ok", "type": "button", "zh": "确定", "en": "OK", "role": "accept"},
                {"id": "cancel", "type": "button", "zh": "取消", "en": "Cancel", "role": "reject"},
            ],
        },
        "tabs": [
            {
                "id": "composition_marker",
                "zh": "合成标记（标题：合成标记）",
                "en": "Composition Marker",
                "controls": [
                    {
                        "id": "time",
                        "type": "timecode_scrub",
                        "zh": "时间:",
                        "en": "Time:",
                        "defaultValue": "<当前播放头时间>",
                        "notesZh": "右侧只读显示解析时间与基准帧率（如“是 0:00:02:00  基础 30”）。",
                    },
                    {
                        "id": "duration",
                        "type": "timecode_scrub",
                        "zh": "持续时间:",
                        "en": "Duration:",
                        "defaultValue": "0:00:00:00",
                        "notesZh": "非零值会在时间轴上显示带范围条的区间标记。",
                    },
                    {
                        "id": "comment",
                        "type": "multiline_text",
                        "zh": "注释:",
                        "en": "Comment:",
                        "defaultValue": "",
                    },
                    {
                        "id": "label",
                        "type": "dropdown",
                        "zh": "标签:",
                        "en": "Label:",
                        "defaultValue": {"zh": "无", "en": "None"},
                        "options": LABEL_COLOR_OPTIONS,
                        "notesZh": "共 17 项：无 + 16 个 AE 标签颜色。",
                    },
                    {
                        "id": "protected_region",
                        "type": "checkbox",
                        "zh": "受保护区域",
                        "en": "Protected Region",
                        "defaultValue": False,
                        "enableRuleZh": "合成标记独有控件；用于响应设计时间（Responsive Design - Time）锁定时间段不被拉伸。",
                    },
                ],
            },
            {
                "id": "layer_marker",
                "zh": "图层标记（标题：图层标记）",
                "en": "Layer Marker",
                "controls": [
                    {
                        "id": "time",
                        "type": "timecode_scrub",
                        "zh": "时间:",
                        "en": "Time:",
                        "defaultValue": "<当前播放头时间>",
                    },
                    {
                        "id": "duration",
                        "type": "timecode_scrub",
                        "zh": "持续时间:",
                        "en": "Duration:",
                        "defaultValue": "0:00:00:00",
                    },
                    {
                        "id": "comment",
                        "type": "multiline_text",
                        "zh": "注释:",
                        "en": "Comment:",
                        "defaultValue": "",
                    },
                    {
                        "id": "label",
                        "type": "dropdown",
                        "zh": "标签:",
                        "en": "Label:",
                        "defaultValue": {"zh": "无", "en": "None"},
                        "options": LABEL_COLOR_OPTIONS,
                    },
                    {
                        "id": "marker_kind",
                        "type": "radio_group",
                        "zh": "标记类型",
                        "en": "Marker Type",
                        "defaultValue": {"zh": "注释标记", "en": "Comment Marker"},
                        "options": [
                            {"id": "comment", "zh": "注释标记", "en": "Comment Marker"},
                            {"id": "segmentation", "zh": "分段标记", "en": "Segmentation Marker"},
                        ],
                        "enableRuleZh": "图层标记独有控件（替代合成标记的“受保护区域”）；分段标记用于时间重映射/关键帧分段。",
                    },
                ],
            },
        ],
    }


def build_color_picker(raw: dict[str, Any]) -> dict[str, Any]:
    rect = raw["colorPicker_8bpc"]["rect"]
    return {
        "schemaVersion": "1.0",
        "id": "color-picker",
        "title": {"zh": "<调用属性名>（如“背景颜色”、“灯光颜色”、“颜色”）", "en": "<Property Name> (e.g. Background Color, Light Color, Color)"},
        "triggers": [
            {
                "action": "Open Adobe Color Picker",
                "shortcutWin": None,
                "menuZh": "点击任意颜色色块（如合成设置背景颜色、纯色/灯光/效果/文字颜色）",
            }
        ],
        "source": COMMON_SOURCE,
        "geometry": {
            "physicalPxAt144Dpi": {"w": rect["w"], "h": rect["h"]},
            "logicalPxAt96Dpi": {"w": round(rect["w"] / 1.5), "h": round(rect["h"] / 1.5)},
        },
        "persistence": {
            "remembersLastValues": True,
            "notesZh": "记住上次选中的色彩轴单选（默认 H）及窗口屏幕位置；颜色值与预览状态实时绑定调用方属性。",
        },
        "buttonBar": {
            "position": "right_column",
            "right": [
                {"id": "ok", "type": "button", "zh": "确定", "en": "OK", "role": "accept"},
                {"id": "cancel", "type": "button", "zh": "取消", "en": "Cancel", "role": "reject"},
            ],
            "notesZh": "按钮位于对话框右侧顶部纵向排列（确定在上、取消在下），下方同列依次为吸管图标按钮与预览复选框。",
        },
        "tabs": [],
        "controls": [
            {
                "id": "color_field_and_slider",
                "type": "custom_color_canvas",
                "zh": "二维色域面板 + 垂直色相/通道滑条 + 新/旧色块对比",
                "en": "2D Color Field + Vertical Channel Slider + New/Current Swatches",
                "notesZh": "位于左侧区域；垂直滑条与二维色域随右侧 H/S/B/R/G/B 单选切换坐标轴。",
            },
            {
                "id": "channel_axis_radio",
                "type": "radio_group",
                "zh": "色彩轴单选（H / S / B / R / G / B）",
                "en": "Color Axis Radio (H / S / B / R / G / B)",
                "defaultValue": {"zh": "H", "en": "H"},
                "options": [
                    {"zh": "H", "en": "H"},
                    {"zh": "S", "en": "S"},
                    {"zh": "B", "en": "B"},
                    {"zh": "R", "en": "R"},
                    {"zh": "G", "en": "G"},
                    {"zh": "B", "en": "B"},
                ],
            },
            {
                "id": "hsb_inputs",
                "type": "number_scrub_group",
                "internalName": "HotText_Hue / HotText_Sat / HotText_Bri",
                "zh": "H / S / B 数值",
                "en": "H / S / B Numeric Scrubs",
                "unit": "H: 0–360°, S: 0–100%, B: 0–100%",
                "range": {"hue": [0, 360], "sat": [0, 100], "bri": [0, 100]},
            },
            {
                "id": "rgb_inputs",
                "type": "number_scrub_group",
                "internalName": "HotText_Red / HotText_Green / HotText_Blue",
                "zh": "R / G / B 数值",
                "en": "R / G / B Numeric Scrubs",
                "unit": "8 bpc: 0–255; 16 bpc: 0–32768; 32 bpc float: 0.0–1.0+ (HDR)",
                "notesZh": "实测 32 bpc 项目的灯光/效果颜色自动切为浮点 0.0–1.0+（白色显示 1/1/1）；合成背景颜色在 32 bpc 下仍保持 8 位 0–255。",
            },
            {
                "id": "hex_input",
                "type": "text_input",
                "zh": "#",
                "en": "#",
                "defaultValue": "000000",
                "notesZh": "6 位十六进制 RGB 颜色码（如 FFFFFF / 000000），与 HSB/RGB 实时双向同步。",
            },
            {
                "id": "eyedropper",
                "type": "icon_button",
                "zh": "吸管",
                "en": "Eyedropper",
                "notesZh": "位于右列取消按钮下方，支持从屏幕任意位置取色。",
            },
            {
                "id": "preview",
                "type": "checkbox",
                "zh": "预览",
                "en": "Preview",
                "defaultValue": True,
                "notesZh": "默认勾选，位于右列吸管下方，实时将颜色变化反馈到合成查看器。",
            },
        ],
    }
