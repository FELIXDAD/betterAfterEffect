#!/usr/bin/env python3
"""Build structured Windows AE 26.0 Dialog reference docs (JSON + MD).

Reads live MSAA/ExtendScript captures from --raw-dir plus repo reference docs and emits:
  - docs/reference/ae/ui/dialogs/<dialog-id>.json
  - docs/reference/ae/ui/dialogs.md

Compliance: Emits only structural UI facts (titles, tabs, control types, short zh/en
labels, default values, dropdown option lists, numeric ranges/units, enable/disable
linkages, button layout, and persistence rules). Does not include Adobe help/warning
prose, screenshots, icons, or local paths.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from typing import Any


PAR_VALUE_MAP: dict[str, tuple[str, float]] = {
    "方形像素": ("Square Pixels", 1.0),
    "D1/DV NTSC (0.91)": ("D1/DV NTSC (0.91)", 0.909091),
    "D1/DV NTSC 宽银幕  (1.21)": ("D1/DV NTSC Widescreen (1.21)", 1.212121),
    "D1/DV PAL (1.09)": ("D1/DV PAL (1.09)", 1.094017),
    "D1/DV PAL 宽银幕  (1.46)": ("D1/DV PAL Widescreen (1.46)", 1.458689),
    "HDV 1080/DVCPRO HD 720 (1.33)": ("HDV 1080/DVCPRO HD 720 (1.33)", 1.333333),
    "DVCPRO HD 1080 (1.5)": ("DVCPRO HD 1080 (1.5)", 1.5),
    "变形 2:1 (2)": ("Anamorphic 2:1 (2)", 2.0),
}

PRESET_EN_MAP: dict[str, str] = {
    "HD  •  1920x1080 • 24 fps": "HD  •  1920x1080 • 24 fps",
    "HD  •  1920x1080 • 25 fps": "HD  •  1920x1080 • 25 fps",
    "HD  •  1920x1080 • 29.97 fps": "HD  •  1920x1080 • 29.97 fps",
    "UHD 4K)  •  3840x2160 • 23.976 fps": "UHD (4K)  •  3840x2160 • 23.976 fps",
    "UHD 4K)  •  3840x2160 • 25 fps": "UHD (4K)  •  3840x2160 • 25 fps",
    "UHD 4K)  •  3840x2160 • 29.97 fps": "UHD (4K)  •  3840x2160 • 29.97 fps",
    "UHD (8K)  •  7680x4320 • 23.976 fps": "UHD (8K)  •  7680x4320 • 23.976 fps",
    "社交媒体纵向 HD  •  1080x1920 • 30 fps": "Social Media Portrait HD  •  1080x1920 • 30 fps",
    "社交媒体横向 HD  •  1920x1080 • 30 fps": "Social Media Landscape HD  •  1920x1080 • 30 fps",
    "社交媒体纵向  •  720x1280 • 30 fps": "Social Media Portrait  •  720x1280 • 30 fps",
    "社交媒体横向  •  1280x720 • 30 fps": "Social Media Landscape  •  1280x720 • 30 fps",
    "社交媒体方形  •  1080x1080 • 30 fps": "Social Media Square  •  1080x1080 • 30 fps",
    "HDV/HDTV  •  1280x720 • 25 fps": "HDV/HDTV  •  1280x720 • 25 fps",
    "HDV/HDTV  •  1280x720 • 29.97 fps": "HDV/HDTV  •  1280x720 • 29.97 fps",
    "HDV  •  1440x1080 (1.33) • 25 fps": "HDV  •  1440x1080 (1.33) • 25 fps",
    "HDV  •  1440x1080 (1.33) • 29.97 fps": "HDV  •  1440x1080 (1.33) • 29.97 fps",
    "DVCPRO HD  •  960x720 (1.33) • 23.976 fps": "DVCPRO HD  •  960x720 (1.33) • 23.976 fps",
    "DVCPRO HD  •  960x720 (1.33) • 25 fps": "DVCPRO HD  •  960x720 (1.33) • 25 fps",
    "DVCPRO HD  •  960x720 (1.33) • 29.97 fps": "DVCPRO HD  •  960x720 (1.33) • 29.97 fps",
    "DVCPRO HD  •  1440x1080 (1.33) • 25 fps": "DVCPRO HD  •  1440x1080 (1.33) • 25 fps",
    "DVCPRO HD  •  1280x1080 (1.5) • 29.97 fps": "DVCPRO HD  •  1280x1080 (1.5) • 29.97 fps",
    "Cineon 1/2  •  1828x1332 • 24 fps": "Cineon Half  •  1828x1332 • 24 fps",
    "Cineon 完整  •  3656x2664 • 24 fps": "Cineon Full  •  3656x2664 • 24 fps",
    "胶片 (2K)  •  2048x1556 • 24 fps": "Film (2K)  •  2048x1556 • 24 fps",
    "胶片 (4K)  •  4096x3112 • 24 fps": "Film (4K)  •  4096x3112 • 24 fps",
}


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8-sig") as f:
        return json.load(f)


def build_composition_settings(raw_dir: Path, repo_root: Path) -> dict[str, Any]:
    raw = load_json(raw_dir / "comp_settings_zh.json")
    prefs = load_json(repo_root / "docs/reference/ae/preferences.json")

    # Build full 29-entry preset menu order (25 presets + 4 separators) enriched with live measured values
    preset_by_name: dict[str, dict[str, Any]] = {}
    for rec in raw["presets"]:
        preset_by_name[rec["selectedPreset"]] = rec

    raw_pref_presets = prefs["sections"]["Composition Preset Names Section v11"]
    ordered_pref_names = [
        raw_pref_presets[k] for k in sorted(raw_pref_presets.keys()) if k != "_file"
    ]

    preset_entries: list[dict[str, Any]] = [
        {
            "kind": "custom",
            "zh": "自定义",
            "en": "Custom",
        }
    ]
    for entry in ordered_pref_names:
        if entry == "-":
            preset_entries.append({"kind": "separator"})
            continue
        rec = preset_by_name[entry]
        par_zh = rec["pixelAspectRatio"].strip()
        par_en, par_num = PAR_VALUE_MAP.get(par_zh, (par_zh, 1.0))
        fps_str = str(rec["frameRate"])
        fps_val: int | float = int(fps_str) if fps_str.isdigit() else float(fps_str)
        preset_entries.append(
            {
                "kind": "preset",
                "zh": entry,
                "en": PRESET_EN_MAP.get(entry, entry),
                "width": int(rec["width"]),
                "height": int(rec["height"]),
                "pixelAspectRatio": {
                    "zh": par_zh,
                    "en": par_en,
                    "value": par_num,
                },
                "frameAspectRatio": rec["frameAspectRatio"],
                "frameRate": fps_val,
                "dropFrameDefault": "non_drop_frame",
                "dropFrameEnabled": bool(rec["dropFrameEnabled"]),
            }
        )

    return {
        "schemaVersion": "1.0",
        "id": "composition-settings",
        "title": {"zh": "合成设置", "en": "Composition Settings"},
        "triggers": [
            {"action": "New Composition", "shortcutWin": "Ctrl+N", "menuZh": "合成 ▸ 新建合成..."},
            {"action": "Edit Composition Settings", "shortcutWin": "Ctrl+K", "menuZh": "合成 ▸ 合成设置..."},
        ],
        "source": {
            "application": "Adobe After Effects 26.0 (26.0x67)",
            "platform": "Windows 11 Pro x64 (3840x2160 @ 150% DPI)",
            "localeObserved": "zh_CN",
            "method": "Live MSAA inspection + interactive ComboBox/tab probing + ExtendScript range probing",
        },
        "geometry": {
            "physicalPxAt144Dpi": {"w": 894, "h": 812},
            "logicalPxAt96Dpi": {"w": 596, "h": 541},
        },
        "persistence": {
            "remembersLastValues": True,
            "persistsAcrossRestarts": True,
            "prefSection": "Composition Pref Section",
            "notesZh": "跨重启持久化记住上次新建合成的尺寸、像素长宽比、锁定长宽比、帧速率、丢帧、分辨率、开始时间码、持续时间、背景颜色、高级选项卡参数及 3D 渲染器选择。",
        },
        "headerControls": [
            {
                "id": "comp_name",
                "type": "text_input",
                "zh": "合成名称:",
                "en": "Composition Name:",
                "defaultValue": {"newCompZh": "合成 1", "newCompEn": "Comp 1", "editComp": "<当前合成名称>"},
                "notesZh": "位于页签栏上方、全页签共享；新建合成按 合成 1/2/3... 递增。",
            }
        ],
        "buttonBar": {
            "position": "bottom",
            "left": [
                {
                    "id": "preview",
                    "type": "checkbox",
                    "zh": "预览",
                    "en": "Preview",
                    "defaultNewComp": False,
                    "defaultEditComp": True,
                    "enableRuleZh": "新建合成（Ctrl+N）时未勾选且置灰禁用；编辑已有合成（Ctrl+K）时默认勾选且可用，用于实时预览尺寸/背景色等修改。",
                }
            ],
            "right": [
                {"id": "ok", "type": "button", "zh": "确定", "en": "OK", "role": "accept"},
                {"id": "cancel", "type": "button", "zh": "取消", "en": "Cancel", "role": "reject"},
            ],
        },
        "tabs": [
            {
                "id": "basic",
                "zh": "基本",
                "en": "Basic",
                "controls": [
                    {
                        "id": "preset",
                        "type": "dropdown",
                        "zh": "预设:",
                        "en": "Preset:",
                        "defaultValue": "persisted_last_or_custom",
                        "optionsRef": "presets",
                        "notesZh": "共 26 个可选条目（自定义 + 25 个内置预设，内含 4 条分组分隔线，总菜单项 30 条）；修改宽度/高度/像素长宽比/帧速率偏离预设后自动切为“自定义”。",
                    },
                    {
                        "id": "save_custom_preset",
                        "type": "icon_button",
                        "zh": "保存自定义预设",
                        "en": "Save Custom Preset",
                        "enableRuleZh": "始终可用；点击将当前尺寸/PAR/帧速率存为用户预设。",
                    },
                    {
                        "id": "delete_custom_preset",
                        "type": "icon_button",
                        "zh": "删除自定义预设",
                        "en": "Delete Custom Preset",
                        "enableRuleZh": "选中“自定义”或任意内置预设时置灰禁用；仅当选中用户自建预设时可用。",
                    },
                    {
                        "id": "width",
                        "type": "number_scrub",
                        "internalName": "widthNumEdit",
                        "zh": "宽度:",
                        "en": "Width:",
                        "unit": "px",
                        "range": {"min": 1, "max": 30000, "uiTypicalMin": 4, "step": 1, "type": "integer"},
                        "defaultValue": "persisted",
                    },
                    {
                        "id": "height",
                        "type": "number_scrub",
                        "internalName": "heightNumEdit",
                        "zh": "高度:",
                        "en": "Height:",
                        "unit": "px",
                        "range": {"min": 1, "max": 30000, "uiTypicalMin": 4, "step": 1, "type": "integer"},
                        "defaultValue": "persisted",
                    },
                    {
                        "id": "lock_aspect_ratio",
                        "type": "checkbox",
                        "zh": "锁定长宽比为 <W>:<H> (<ratio>)",
                        "en": "Lock Aspect Ratio to <W>:<H> (<ratio>)",
                        "defaultValue": False,
                        "notesZh": "动态显示当前宽高比（如 16:9 (1.78) 或 4:3 (1.33)）；勾选后改宽度或高度时另一边按比例联动。",
                    },
                    {
                        "id": "pixel_aspect_ratio",
                        "type": "dropdown",
                        "zh": "像素长宽比:",
                        "en": "Pixel Aspect Ratio:",
                        "defaultValue": {"zh": "方形像素", "en": "Square Pixels", "value": 1.0},
                        "range": {"min": 0.01, "max": 100.0},
                        "options": [
                            {"zh": "方形像素", "en": "Square Pixels", "value": 1.0},
                            {"zh": "D1/DV NTSC (0.91)", "en": "D1/DV NTSC (0.91)", "value": 0.909091},
                            {"zh": "D1/DV NTSC 宽银幕  (1.21)", "en": "D1/DV NTSC Widescreen (1.21)", "value": 1.212121},
                            {"zh": "D1/DV PAL (1.09)", "en": "D1/DV PAL (1.09)", "value": 1.094017},
                            {"zh": "D1/DV PAL 宽银幕  (1.46)", "en": "D1/DV PAL Widescreen (1.46)", "value": 1.458689},
                            {"zh": "HDV 1080/DVCPRO HD 720 (1.33)", "en": "HDV 1080/DVCPRO HD 720 (1.33)", "value": 1.333333},
                            {"zh": "DVCPRO HD 1080 (1.5)", "en": "DVCPRO HD 1080 (1.5)", "value": 1.5},
                            {"zh": "变形 2:1 (2)", "en": "Anamorphic 2:1 (2)", "value": 2.0},
                        ],
                    },
                    {
                        "id": "frame_aspect_ratio",
                        "type": "static_text",
                        "zh": "画面长宽比:",
                        "en": "Frame Aspect Ratio:",
                        "format": "<W>:<H> (<ratio>)",
                        "notesZh": "只读计算值，位于像素长宽比右侧（如 16:9 (1.78)、9:16 (0.56)、457:333 (1.37)）。",
                    },
                    {
                        "id": "frame_rate",
                        "type": "text_input_with_dropdown",
                        "zh": "帧速率:",
                        "en": "Frame Rate:",
                        "unitZh": "帧/秒",
                        "unitEn": "Frames per Second",
                        "range": {"min": 1.0, "max": 999.0},
                        "defaultValue": "persisted",
                        "quickPickOptions": [8, 12, 15, 23.976, 24, 25, 29.97, 30, 50, 59.94, 60, 120],
                        "notesZh": "左侧为自由输入的帧速率文本框，紧邻右侧为小箭头快捷预设下拉（12 档常用帧率）。",
                    },
                    {
                        "id": "drop_frame",
                        "type": "dropdown",
                        "zh": "丢帧模式（无独立标签，位于“帧/秒”右侧）",
                        "en": "Drop Frame mode (unlabeled, right of Frames per Second)",
                        "defaultValue": {"zh": "无丢帧", "en": "Non-Drop Frame"},
                        "options": [
                            {"zh": "丢帧", "en": "Drop Frame"},
                            {"zh": "无丢帧", "en": "Non-Drop Frame"},
                        ],
                        "enableRuleZh": "仅当帧速率为 29.97 或 59.94 fps 时启用；其他所有帧速率（23.976/24/25/30/50/60/120 等）均置灰禁用并锁定为“无丢帧”。",
                    },
                    {
                        "id": "resolution",
                        "type": "dropdown",
                        "zh": "分辨率:",
                        "en": "Resolution:",
                        "defaultValue": {"zh": "完整", "en": "Full"},
                        "options": [
                            {"zh": "完整", "en": "Full", "factor": [1, 1]},
                            {"zh": "二分之一", "en": "Half", "factor": [2, 2]},
                            {"zh": "三分之一", "en": "Third", "factor": [3, 3]},
                            {"zh": "四分之一", "en": "Quarter", "factor": [4, 4]},
                            {"zh": "自定义...", "en": "Custom...", "opensSubDialog": "custom-resolution"},
                        ],
                        "summaryFormatZh": "<W> x <H>，<size> MB/KB（每 8bpc 帧）",
                        "summaryFormatEn": "<W> x <H>, <size> MB/KB per 8bpc frame",
                    },
                    {
                        "id": "start_timecode",
                        "type": "timecode_input",
                        "zh": "开始时间码:",
                        "en": "Start Timecode:",
                        "defaultValue": "0:00:00:00",
                        "noteFormatZh": "是 <TC>  基础 <fps>",
                        "noteFormatEn": "is <TC>  Base <fps>",
                        "notesZh": "右侧提示中文实测在“基础”前有两个半角空格（例如 `是 0:00:00:00  基础 30`）。",
                    },
                    {
                        "id": "duration",
                        "type": "timecode_input",
                        "zh": "持续时间:",
                        "en": "Duration:",
                        "defaultValue": "persisted",
                        "range": {"minSeconds": 0.0, "maxSeconds": 10800.0, "maxTimecode": "3:00:00:00"},
                        "noteFormatZh": "是 <TC>  基础 <fps>",
                        "noteFormatEn": "is <TC>  Base <fps>",
                    },
                    {
                        "id": "background_color",
                        "type": "color_picker_row",
                        "zh": "背景颜色:",
                        "en": "Background Color:",
                        "defaultValue": {"hex": "#000000", "zh": "黑色", "en": "Black"},
                        "subControls": ["color_swatch_button", "eyedropper_button", "color_name_label"],
                    },
                ],
            },
            {
                "id": "advanced",
                "zh": "高级",
                "en": "Advanced",
                "controls": [
                    {
                        "id": "anchor",
                        "type": "anchor_grid_3x3",
                        "internalNames": ["pic0", "pic1", "pic2", "pic3", "pic4", "pic5", "pic6", "pic7", "pic8"],
                        "zh": "锚点:",
                        "en": "Anchor:",
                        "defaultValue": "center (pic4)",
                        "enableRuleZh": "新建合成（Ctrl+N）时始终置灰禁用；编辑已有合成（Ctrl+K）时仅在修改了宽度或高度后启用，用于决定画布裁切/扩展时的对齐锚点。",
                    },
                    {
                        "id": "preserve_frame_rate",
                        "type": "checkbox",
                        "zh": "在嵌套时或在渲染队列中，保留帧速率",
                        "en": "Preserve frame rate when nested or in render queue",
                        "defaultValue": False,
                    },
                    {
                        "id": "preserve_resolution",
                        "type": "checkbox",
                        "zh": "在嵌套时保留分辨率",
                        "en": "Preserve resolution when nested",
                        "defaultValue": False,
                    },
                    {
                        "id": "shutter_angle",
                        "type": "number_scrub",
                        "internalName": "shutterAngleNumEdit",
                        "zh": "快门角度:",
                        "en": "Shutter Angle:",
                        "unit": "°",
                        "defaultValue": 180,
                        "range": {"min": 0, "max": 720, "type": "integer"},
                    },
                    {
                        "id": "shutter_phase",
                        "type": "number_scrub",
                        "internalName": "shutterPhaseNumEdit",
                        "zh": "快门相位:",
                        "en": "Shutter Phase:",
                        "unit": "°",
                        "defaultValue": 0,
                        "range": {"min": -360, "max": 360, "type": "integer"},
                        "notesZh": "AE 26 实测新合成默认值为 0°（部分早期文档常记为 -90°）。",
                    },
                    {
                        "id": "samples_per_frame",
                        "type": "number_scrub",
                        "internalName": "threeDsamplesNumEdit",
                        "zh": "每帧样本:",
                        "en": "Samples Per Frame:",
                        "defaultValue": 16,
                        "range": {"min": 2, "max": 64, "type": "integer"},
                    },
                    {
                        "id": "adaptive_sample_limit",
                        "type": "number_scrub",
                        "internalName": "max2DsamplesNumEdit",
                        "zh": "自适应采样限制:",
                        "en": "Adaptive Sample Limit:",
                        "defaultValue": 128,
                        "range": {"min": 64, "max": 256, "type": "integer"},
                        "notesZh": "AE 26 实测下限为 64（非 2），上限 256，默认 128。",
                    },
                ],
            },
            {
                "id": "3d_renderer",
                "zh": "3D 渲染器",
                "en": "3D Renderer",
                "controls": [
                    {
                        "id": "renderer",
                        "type": "dropdown",
                        "zh": "渲染器:",
                        "en": "Renderer:",
                        "defaultValue": {"zh": "经典 3D", "en": "Classic 3D", "matchName": "ADBE Advanced 3d"},
                        "options": [
                            {
                                "zh": "经典 3D",
                                "en": "Classic 3D",
                                "matchName": "ADBE Advanced 3d",
                                "optionsButtonEnabled": True,
                                "optionsSubDialog": "classic-3d-options",
                                "showsFeatureComparisonLists": False,
                            },
                            {
                                "zh": "高级 3D",
                                "en": "Advanced 3D",
                                "matchName": "ADBE Calder",
                                "optionsButtonEnabled": True,
                                "optionsSubDialog": "advanced-3d-options",
                                "showsFeatureComparisonLists": True,
                            },
                            {
                                "zh": "Cinema 4D",
                                "en": "Cinema 4D",
                                "matchName": "ADBE Ernst",
                                "optionsButtonEnabled": True,
                                "optionsSubDialog": "cinema-4d-options",
                                "showsFeatureComparisonLists": True,
                            },
                        ],
                    },
                    {
                        "id": "options_button",
                        "type": "button",
                        "zh": "选项...",
                        "en": "Options...",
                        "enableRuleZh": "AE 26 中三个渲染器（经典 3D、高级 3D、Cinema 4D）下均启用，分别弹出对应的渲染器选项子对话框。",
                    },
                    {
                        "id": "about_button",
                        "type": "button",
                        "zh": "关于...",
                        "en": "About...",
                        "enableRuleZh": "三个渲染器下均启用。",
                    },
                    {
                        "id": "feature_comparison",
                        "type": "two_column_list",
                        "internalNames": ["RendererEnabled", "RendererDisabled"],
                        "zh": "已启用功能 / 已禁用功能",
                        "en": "Enabled Features / Disabled Features",
                        "visibilityRuleZh": "选中“经典 3D”时隐藏，仅在选中“高级 3D”或“Cinema 4D”时显示左右两栏能力对照表。",
                    },
                ],
            },
        ],
        "presets": preset_entries,
        "subDialogs": [
            {
                "id": "custom-resolution",
                "title": {"zh": "自定义分辨率", "en": "Custom Resolution"},
                "trigger": "基本页签 ▸ 分辨率 ▸ 自定义...",
                "buttonBar": {"position": "bottom-right", "buttons": ["确定 / OK", "取消 / Cancel"]},
                "controls": [
                    {
                        "id": "horizontal_step",
                        "type": "number_scrub",
                        "internalName": "horizontalNumber",
                        "zh": "渲染每 [N] 个像素（水平）。",
                        "en": "Render every [N] pixel(s) horizontally.",
                        "defaultValue": 1,
                        "range": {"min": 1, "max": 99, "type": "integer"},
                    },
                    {
                        "id": "vertical_step",
                        "type": "number_scrub",
                        "internalName": "verticalNumber",
                        "zh": "渲染每 [N] 个像素（垂直）。",
                        "en": "Render every [N] pixel(s) vertically.",
                        "defaultValue": 1,
                        "range": {"min": 1, "max": 99, "type": "integer"},
                    },
                ],
            },
            {
                "id": "classic-3d-options",
                "title": {"zh": "经典的 3D 渲染器选项", "en": "Classic 3D Renderer Options"},
                "trigger": "3D 渲染器页签 ▸ 经典 3D ▸ 选项...",
                "buttonBar": {"position": "bottom-right", "buttons": ["确定 / OK", "取消 / Cancel"]},
                "controls": [
                    {
                        "id": "shadow_map_resolution",
                        "type": "dropdown",
                        "zh": "阴影图分辨率:",
                        "en": "Shadow Map Resolution:",
                        "defaultValue": {"zh": "合成大小", "en": "Comp Size"},
                        "options": [
                            {"zh": "合成大小", "en": "Comp Size"},
                            {"zh": "250", "en": "250"},
                            {"zh": "500", "en": "500"},
                            {"zh": "750", "en": "750"},
                            {"zh": "1000", "en": "1000"},
                            {"zh": "1500", "en": "1500"},
                            {"zh": "2000", "en": "2000"},
                            {"zh": "3000", "en": "3000"},
                            {"zh": "4000", "en": "4000"},
                        ],
                    }
                ],
            },
            {
                "id": "advanced-3d-options",
                "title": {"zh": "高级 3D 渲染器选项", "en": "Advanced 3D Renderer Options"},
                "trigger": "3D 渲染器页签 ▸ 高级 3D ▸ 选项...",
                "buttonBar": {"position": "bottom-right", "buttons": ["确定 / OK", "取消 / Cancel"]},
                "controls": [
                    {
                        "id": "render_quality",
                        "type": "number_scrub_with_slider",
                        "internalName": "htQualitySamples / slQualitySamples",
                        "zh": "渲染品质",
                        "en": "Render Quality",
                        "defaultValue": 8,
                    },
                    {
                        "id": "shadow_resolution",
                        "type": "dropdown",
                        "zh": "分辨率:",
                        "en": "Resolution:",
                        "defaultValue": {"zh": "正常 (16 MB)", "en": "Normal (16 MB)"},
                        "options": [
                            {"zh": "0.5 倍 (2 MB)", "en": "0.5x (2 MB)"},
                            {"zh": "正常 (16 MB)", "en": "Normal (16 MB)"},
                            {"zh": "双倍 (128 MB)", "en": "Double (128 MB)"},
                        ],
                    },
                    {
                        "id": "shadow_smoothness",
                        "type": "number_scrub_with_slider",
                        "internalName": "htShadowSamples / slShadowSamples",
                        "zh": "平滑度",
                        "en": "Smoothness",
                        "defaultValue": 3,
                    },
                    {
                        "id": "shadow_box_size",
                        "type": "number_scrub",
                        "internalName": "htShadowSize",
                        "zh": "投射框尺寸",
                        "en": "Shadow Box Size",
                        "defaultValue": "max(compWidth, compHeight)",
                    },
                    {
                        "id": "shadow_box_center",
                        "type": "vector3_scrub",
                        "internalNames": ["shadowX", "shadowY", "shadowZ"],
                        "zh": "投射框中心 (X / Y / Z)",
                        "en": "Shadow Box Center (X / Y / Z)",
                        "defaultValue": ["compWidth / 2", "compHeight / 2", 0],
                    },
                ],
            },
            {
                "id": "cinema-4d-options",
                "title": {"zh": "Cinema 4D 渲染器选项", "en": "Cinema 4D Renderer Options"},
                "trigger": "3D 渲染器页签 ▸ Cinema 4D ▸ 选项...",
                "buttonBar": {"position": "bottom-right", "buttons": ["确定 / OK", "取消 / Cancel"]},
                "controls": [
                    {
                        "id": "quality",
                        "type": "number_scrub_with_slider",
                        "internalName": "htRenderQuality / slRenderQuality",
                        "zh": "品质:",
                        "en": "Quality:",
                        "defaultValue": 25,
                        "range": {"min": 1, "max": 100},
                        "sliderMarkers": {
                            "topZh": ["草图", "典型", "极致"],
                            "topEn": ["Draft", "Typical", "Extreme"],
                            "bottomZh": ["速度", "品质"],
                            "bottomEn": ["Speed", "Quality"],
                        },
                    },
                    {
                        "id": "derived_readout",
                        "type": "readonly_metrics",
                        "zh": "派生参数只读展示（随品质滑块联动）",
                        "en": "Derived read-only metrics (driven by Quality slider)",
                        "fields": [
                            {"zh": "光线阈值:", "en": "Ray Threshold:", "atDefault25": 0.818},
                            {"zh": "光线深度:", "en": "Ray Depth:", "atDefault25": 18},
                            {"zh": "反射深度:", "en": "Reflection Depth:", "atDefault25": 3},
                            {"zh": "阴影深度:", "en": "Shadow Depth:", "atDefault25": 9},
                            {"zh": "图层采样:", "en": "Layer Sampling:", "atDefault25": 2.18},
                        ],
                    },
                ],
            },
        ],
    }


def render_comp_settings_md(cs: dict[str, Any]) -> list[str]:
    lines: list[str] = []
    lines.append(f"## 1. {cs['title']['zh']}（{cs['title']['en']}）")
    lines.append("")
    lines.append(f"- **结构化数据**：[`dialogs/composition-settings.json`](dialogs/composition-settings.json)")
    lines.append("- **入口**：`合成 ▸ 新建合成...`（`Ctrl+N`）、`合成 ▸ 合成设置...`（`Ctrl+K`）")
    lines.append("- **窗口尺寸**：`894 × 812` 物理像素 @ 144 DPI（`596 × 541` 逻辑像素 @ 96 DPI）")
    lines.append("- **页签（3 个）**：`基本`（Basic）｜`高级`（Advanced）｜`3D 渲染器`（3D Renderer）")
    lines.append("- **顶部共享行**：`合成名称:` / `Composition Name:` 文本框（新建默认 `合成 1` / `Comp 1`，编辑显示当前合成名）")
    lines.append("- **底部按钮栏**：左下 `预览`（Preview）复选框（`Ctrl+N` 时未勾选且置灰；`Ctrl+K` 时默认勾选且可用）；右下 `确定`（OK）+ `取消`（Cancel）水平并排。")
    lines.append(f"- **记住上次的值**：**是，跨重启持久化**（写入偏好文件 `[{cs['persistence']['prefSection']}]`）。")
    lines.append("")

    for tab in cs["tabs"]:
        lines.append(f"### 1.{cs['tabs'].index(tab) + 1} 页签：{tab['zh']}（{tab['en']}）")
        lines.append("")
        lines.append("| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |")
        lines.append("|---:|---|---|---|---|---|---|")
        for idx, ctl in enumerate(tab["controls"], 1):
            dv = ctl.get("defaultValue", "—")
            if isinstance(dv, dict):
                dv_s = dv.get("zh") or dv.get("newCompZh") or dv.get("hex") or str(dv)
            elif dv == "persisted":
                dv_s = "记住上次值"
            elif dv == "persisted_last_or_custom":
                dv_s = "记住上次值 / 自定义"
            else:
                dv_s = str(dv)

            spec_parts: list[str] = []
            if "range" in ctl:
                rg = ctl["range"]
                if "min" in rg and "max" in rg:
                    u = ctl.get("unit") or ctl.get("unitZh") or ""
                    spec_parts.append(f"{rg['min']}–{rg['max']}{(' ' + u) if u else ''}")
                elif "maxTimecode" in rg:
                    spec_parts.append(f"0–{rg['maxTimecode']}（{int(rg['maxSeconds'])} s）")
            if "options" in ctl:
                opts_s = " / ".join(o["zh"] for o in ctl["options"])
                spec_parts.append(opts_s)
            elif "quickPickOptions" in ctl:
                spec_parts.append("快捷档: " + ", ".join(str(x) for x in ctl["quickPickOptions"]))
            elif ctl.get("optionsRef") == "presets":
                spec_parts.append("自定义 + 25 个内置预设（见下表）")
            spec_str = "；".join(spec_parts) if spec_parts else "—"

            rule = ctl.get("enableRuleZh") or ctl.get("visibilityRuleZh") or ctl.get("notesZh") or "—"
            lines.append(f"| {idx} | `{ctl['type']}` | {ctl['zh']} | {ctl['en']} | {dv_s} | {spec_str} | {rule} |")
        lines.append("")

    # Full 25 presets table
    lines.append("### 1.4 合成预设完整列表（实测 25 项 + 4 条分隔线）")
    lines.append("")
    lines.append("| # | 预设名称（zh_CN） | 预设名称（en_US） | 宽 × 高 | 像素长宽比（数值） | 画面长宽比 | 帧速率 | 丢帧下拉可用 |")
    lines.append("|---:|---|---|---:|---|---|---:|:---:|")
    row_no = 0
    for p in cs["presets"]:
        if p["kind"] == "custom":
            lines.append("| 0 | `自定义` | `Custom` | — | — | — | — | 随帧率 |")
        elif p["kind"] == "separator":
            lines.append("| — | *（分隔线）* | *（Separator）* | — | — | — | — | — |")
        else:
            row_no += 1
            par = p["pixelAspectRatio"]
            df_s = "✅ 可用" if p["dropFrameEnabled"] else "置灰（无丢帧）"
            lines.append(
                f"| {row_no} | `{p['zh']}` | `{p['en']}` | {p['width']} × {p['height']} | {par['zh']} (`{par['value']}`) | {p['frameAspectRatio']} | {p['frameRate']} | {df_s} |"
            )
    lines.append("")

    # Sub-dialogs
    lines.append("### 1.5 子对话框")
    lines.append("")
    for sd in cs["subDialogs"]:
        lines.append(f"#### {sd['title']['zh']}（{sd['title']['en']}）")
        lines.append(f"- **入口**：{sd['trigger']}；**按钮**：右下 `确定` / `取消`")
        for c in sd["controls"]:
            if c["type"] == "readonly_metrics":
                f_str = "、".join(f"{f['zh']} {f['atDefault25']}" for f in c["fields"])
                lines.append(f"- **{c['zh']}**（{c['en']}）：默认品质 25 下实测 {f_str}")
            else:
                extra = ""
                if "options" in c:
                    extra = "，选项：" + " / ".join(o["zh"] for o in c["options"])
                if "range" in c:
                    extra += f"，范围：{c['range']['min']}–{c['range']['max']}"
                dv = c.get("defaultValue")
                if isinstance(dv, dict):
                    dv = dv.get("zh", str(dv))
                lines.append(f"- **{c['zh']}**（{c['en']}，`{c['type']}`）：默认 `{dv}`{extra}")
        lines.append("")

    return lines


def render_index_md(dialogs: list[dict[str, Any]]) -> str:
    lines: list[str] = [
        "# Adobe After Effects 26 对话框完整参考（Windows）",
        "",
        "> **采集环境**：Windows 11 Pro x64（3840×2160 @ 150% DPI），Adobe After Effects 26.0（`26.0x67`，实测界面语言 `zh_CN`，附标准 `en_US` 对照）。",
        "> **采集方式**：Win32 / MSAA 实时控件树 + 交互式下拉/页签/子对话框探测 + ExtendScript 边界实测（零截图）。",
        "> **说明**：纯色设置、摄像机设置、灯光设置已在 PR #192（`docs/reference/ae/interaction/layers-timeline-dnd.md`）采集，本目录不再重复。",
        "",
        "## 已采集对话框目录",
        "",
        "| # | 对话框（中 / 英） | 快捷键 / 入口 | 结构化 JSON |",
        "|---:|---|---|---|",
    ]
    for i, d in enumerate(dialogs, 1):
        trig = " / ".join(t.get("shortcutWin") or t.get("menuZh", "") for t in d.get("triggers", []))
        lines.append(f"| {i} | {d['title']['zh']}（{d['title']['en']}） | `{trig}` | [`dialogs/{d['id']}.json`](dialogs/{d['id']}.json) |")
    lines.append("")

    for i, d in enumerate(dialogs, 1):
        if d["id"] == "composition-settings":
            lines.extend(render_comp_settings_md(d))
        else:
            from dialogs_batch2 import render_generic_dialog_md
            lines.extend(render_generic_dialog_md(i, d))

    return "\n".join(lines).rstrip() + "\n"


def _write_json(path: Path, payload: dict[str, Any]) -> None:
    with path.open("w", encoding="utf-8", newline="\n") as f:
        json.dump(payload, f, ensure_ascii=False, indent=2)
        f.write("\n")
    print(f"Wrote {path} ({path.stat().st_size} bytes)")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--raw-dir", type=Path, default=Path(os.environ.get("AE_RAW_DIR", ".")))
    parser.add_argument("--repo-root", type=Path, default=Path("."))
    parser.add_argument("--out-dir", type=Path, default=Path("docs/reference/ae/ui"))
    args = parser.parse_args()

    dialogs_dir = args.out_dir / "dialogs"
    dialogs_dir.mkdir(parents=True, exist_ok=True)

    cs = build_composition_settings(args.raw_dir, args.repo_root)
    dialogs: list[dict[str, Any]] = [cs]

    b2_path = args.raw_dir / "dialogs_batch2_zh.json"
    b2_extra_path = args.raw_dir / "dialogs_batch2_extra.json"
    if b2_path.exists() and b2_extra_path.exists():
        from dialogs_batch2 import (
            build_auto_orient,
            build_interpret_footage,
            build_pre_compose,
            build_project_settings,
            build_time_stretch,
        )

        raw_b2 = load_json(b2_path)
        raw_b2_extra = load_json(b2_extra_path)
        dialogs.extend(
            [
                build_pre_compose(raw_b2),
                build_project_settings(raw_b2, raw_b2_extra),
                build_interpret_footage(raw_b2),
                build_time_stretch(raw_b2),
                build_auto_orient(raw_b2),
            ]
        )

    b3_path = args.raw_dir / "dialogs_batch3_zh.json"
    if b3_path.exists():
        from dialogs_batch3 import (
            build_color_picker,
            build_go_to_time,
            build_keyframe_interpolation,
            build_keyframe_velocity,
            build_markers,
        )

        raw_b3 = load_json(b3_path)
        dialogs.extend(
            [
                build_keyframe_interpolation(raw_b3),
                build_keyframe_velocity(raw_b3),
                build_go_to_time(raw_b3),
                build_markers(raw_b3),
                build_color_picker(raw_b3),
            ]
        )

    b4_path = args.raw_dir / "dialogs_batch4_zh.json"
    if b4_path.exists():
        from dialogs_batch4 import (
            build_import_file,
            build_keyboard_shortcuts,
            build_output_module_settings,
            build_render_settings,
        )

        raw_b4 = load_json(b4_path)
        dialogs.extend(
            [
                build_render_settings(raw_b4),
                build_output_module_settings(raw_b4),
                build_keyboard_shortcuts(raw_b4),
                build_import_file(raw_b4),
            ]
        )

    for d in dialogs:
        _write_json(dialogs_dir / f"{d['id']}.json", d)

    md_path = args.out_dir / "dialogs.md"
    with md_path.open("w", encoding="utf-8", newline="\n") as f:
        f.write(render_index_md(dialogs))

    print(f"Wrote {md_path} ({md_path.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
