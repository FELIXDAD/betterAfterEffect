#!/usr/bin/env python3
"""Batch 2 Windows AE 26.0 dialog builders and generic markdown renderer.

Covers:
  - Pre-compose (预合成)
  - Project Settings (项目设置)
  - Interpret Footage (解释素材)
  - Time Stretch (时间延长 / 时间伸缩)
  - Auto-Orientation (自动方向 / 自动定向)
"""

from __future__ import annotations

from pathlib import Path
from typing import Any


COMMON_SOURCE: dict[str, str] = {
    "application": "Adobe After Effects 26.0 (26.0x67)",
    "platform": "Windows 11 Pro x64 (3840x2160 @ 150% DPI)",
    "localeObserved": "zh_CN",
    "method": "Live MSAA inspection + interactive tab/radio/dropdown probing + ExtendScript range probing",
}

PAR_OPTIONS: list[dict[str, Any]] = [
    {"zh": "方形像素", "en": "Square Pixels", "value": 1.0},
    {"zh": "D1/DV NTSC (0.91)", "en": "D1/DV NTSC (0.91)", "value": 0.909091},
    {"zh": "D1/DV NTSC 宽银幕  (1.21)", "en": "D1/DV NTSC Widescreen (1.21)", "value": 1.212121},
    {"zh": "D1/DV PAL (1.09)", "en": "D1/DV PAL (1.09)", "value": 1.094017},
    {"zh": "D1/DV PAL 宽银幕  (1.46)", "en": "D1/DV PAL Widescreen (1.46)", "value": 1.458689},
    {"zh": "HDV 1080/DVCPRO HD 720 (1.33)", "en": "HDV 1080/DVCPRO HD 720 (1.33)", "value": 1.333333},
    {"zh": "DVCPRO HD 1080 (1.5)", "en": "DVCPRO HD 1080 (1.5)", "value": 1.5},
    {"zh": "变形 2:1 (2)", "en": "Anamorphic 2:1 (2)", "value": 2.0},
]


def build_pre_compose(raw: dict[str, Any]) -> dict[str, Any]:
    rect = raw["preCompose_singleSolid"]["initial"]["rect"]
    return {
        "schemaVersion": "1.0",
        "id": "pre-compose",
        "title": {"zh": "预合成", "en": "Pre-compose"},
        "triggers": [
            {"action": "Pre-compose", "shortcutWin": "Ctrl+Shift+C", "menuZh": "图层 ▸ 预合成(P)..."},
        ],
        "source": COMMON_SOURCE,
        "geometry": {
            "physicalPxAt144Dpi": {"w": rect["w"], "h": rect["h"]},
            "logicalPxAt96Dpi": {"w": round(rect["w"] / 1.5), "h": round(rect["h"] / 1.5)},
        },
        "persistence": {
            "remembersLastValues": True,
            "persistsAcrossRestarts": False,
            "notesZh": "单选模式（保留/移动）与两个复选框在可用条件下记住上次选择；新合成名称每次按当前选中图层动态生成（单图层为“<图层名> 合成 1”，多图层为“预合成 1”）。",
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
                "id": "new_comp_name",
                "type": "text_input",
                "zh": "新合成名称:",
                "en": "New composition name:",
                "defaultValue": {
                    "singleLayerZh": "<图层名> 合成 1",
                    "singleLayerEn": "<Layer Name> Comp 1",
                    "multiLayerZh": "预合成 1",
                    "multiLayerEn": "Pre-comp 1",
                },
                "notesZh": "实测选中单图层 MySolid 默认“MySolid 合成 1”，选中单形状层 ShapeLayer 默认“ShapeLayer 合成 1”，选中多图层默认“预合成 1”。",
            },
            {
                "id": "attributes_mode",
                "type": "radio_group",
                "zh": "属性处理方式",
                "en": "Attributes Placement",
                "defaultValue": {"zh": "将所有属性移动到新合成", "en": "Move all attributes into the new composition"},
                "options": [
                    {
                        "id": "leave_all",
                        "zh": "保留“<当前合成名>”中的所有属性",
                        "en": "Leave all attributes in '<Comp Name>'",
                        "enableRuleZh": "仅当选中单个带源图层（素材/纯色/嵌套合成）时可用；多选图层或选中无源图层（形状/文字/空对象/摄像机/灯光/调整图层）时置灰禁用。",
                    },
                    {
                        "id": "move_all",
                        "zh": "将所有属性移动到新合成",
                        "en": "Move all attributes into the new composition",
                        "enableRuleZh": "始终可用；当“保留所有属性”不可用时强制选中此项。",
                    },
                ],
            },
            {
                "id": "adjust_duration",
                "type": "checkbox",
                "zh": "将合成持续时间调整为所选图层的时间范围",
                "en": "Adjust composition duration to the time span of the selected layers",
                "defaultValue": False,
                "enableRuleZh": "仅当选中“将所有属性移动到新合成”时可用；切换到“保留所有属性”时立即置灰禁用。",
            },
            {
                "id": "open_new_comp",
                "type": "checkbox",
                "zh": "打开新合成",
                "en": "Open New Composition",
                "defaultValue": False,
                "enableRuleZh": "始终可用；勾选后确定时会立即在查看器/时间轴打开新预合成。",
            },
        ],
    }


def build_project_settings(raw: dict[str, Any], extra: dict[str, Any]) -> dict[str, Any]:
    ps = raw["projectSettings"]
    rect = ps["initial"]["rect"]
    ocio_controls = extra["projectSettings_extra"]["colorTab_ocio"]
    ocio_combos = [c for c in ocio_controls if c["role"] == 46 and c["rel"]["y"] > 120]
    ocio_configs = ocio_combos[1]["options"]
    ocio_wcs = ocio_combos[4]["options"]
    ocio_dcs = ocio_combos[6]["options"]

    adobe_color_controls = ps["tabs"][2]["controls"]
    adobe_combos = [c for c in adobe_color_controls if c["role"] == 46 and c["rel"]["y"] > 120]
    adobe_wcs_count = len(adobe_combos[4]["options"])

    return {
        "schemaVersion": "1.0",
        "id": "project-settings",
        "title": {"zh": "项目设置", "en": "Project Settings"},
        "triggers": [
            {"action": "Project Settings", "shortcutWin": "Ctrl+Alt+Shift+K", "menuZh": "文件 ▸ 项目设置..."},
        ],
        "source": COMMON_SOURCE,
        "geometry": {
            "physicalPxAt144Dpi": {"w": rect["w"], "h": rect["h"]},
            "logicalPxAt96Dpi": {"w": round(rect["w"] / 1.5), "h": round(rect["h"] / 1.5)},
        },
        "persistence": {
            "remembersLastValues": False,
            "persistsPerProject": True,
            "notesZh": "各项设置随项目（.aep）保存，不跨新建项目继承上次值；新建空白项目默认 8 bpc、时间码、Adobe 色彩管理（工作空间无）、48.000 kHz、JavaScript。",
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
                "id": "video_rendering_and_effects",
                "zh": "视频渲染和效果",
                "en": "Video Rendering and Effects",
                "controls": [
                    {
                        "id": "renderer",
                        "type": "dropdown",
                        "zh": "使用范围:",
                        "en": "Use:",
                        "defaultValue": {"zh": "Mercury GPU 加速 (CUDA)", "en": "Mercury GPU Acceleration (CUDA)"},
                        "options": [
                            {"zh": "Mercury GPU 加速 (CUDA)", "en": "Mercury GPU Acceleration (CUDA)"},
                            {"zh": "仅 Mercury 软件", "en": "Mercury Software Only"},
                        ],
                        "notesZh": "GPU 选项按硬件/平台动态显示（NVIDIA Windows 为 CUDA，macOS 为 Metal，OpenCL 卡为 OpenCL）。",
                    }
                ],
            },
            {
                "id": "time_display_style",
                "zh": "时间显示样式",
                "en": "Time Display Style",
                "controls": [
                    {
                        "id": "time_display_mode",
                        "type": "radio_group",
                        "zh": "显示样式",
                        "en": "Time Display Mode",
                        "defaultValue": {"zh": "时间码", "en": "Timecode"},
                        "options": [
                            {"id": "timecode", "zh": "时间码", "en": "Timecode"},
                            {"id": "frames", "zh": "帧数", "en": "Frames"},
                        ],
                        "notesZh": "实测切换“时间码”与“帧数”时，两组子控件均保持可编辑状态，由当前单选决定生效模式。",
                    },
                    {
                        "id": "footage_start_time",
                        "type": "dropdown",
                        "zh": "素材开始时间:",
                        "en": "Footage Start Time:",
                        "defaultValue": {"zh": "使用媒体源", "en": "Use Media Source"},
                        "options": [
                            {"zh": "使用媒体源", "en": "Use Media Source"},
                            {"zh": "00:00:00:000", "en": "00:00:00:000"},
                        ],
                    },
                    {
                        "id": "default_timecode_base",
                        "type": "number_scrub",
                        "internalName": "timecodeBase",
                        "zh": "默认基准:",
                        "en": "Default Base:",
                        "unit": "fps",
                        "defaultValue": 30,
                        "range": {"min": 1, "max": 999, "type": "integer"},
                    },
                    {
                        "id": "use_feet_frames",
                        "type": "checkbox",
                        "zh": "使用英尺数 + 帧数:",
                        "en": "Use Feet + Frames:",
                        "defaultValue": False,
                    },
                    {
                        "id": "feet_frames_film_type",
                        "type": "dropdown",
                        "zh": "胶片规格",
                        "en": "Film Type",
                        "defaultValue": {"zh": "35 毫米", "en": "35mm"},
                        "options": [
                            {"zh": "16 毫米", "en": "16mm"},
                            {"zh": "35 毫米", "en": "35mm"},
                        ],
                    },
                    {
                        "id": "frame_count",
                        "type": "dropdown",
                        "zh": "帧计数:",
                        "en": "Frame Count:",
                        "defaultValue": {"zh": "时间码转换", "en": "Timecode Conversion"},
                        "options": [
                            {"zh": "开始位置 0", "en": "Start at 0"},
                            {"zh": "开始位置 1", "en": "Start at 1"},
                            {"zh": "时间码转换", "en": "Timecode Conversion"},
                        ],
                    },
                ],
            },
            {
                "id": "color",
                "zh": "颜色",
                "en": "Color",
                "controls": [
                    {
                        "id": "color_engine",
                        "type": "dropdown",
                        "zh": "颜色引擎：",
                        "en": "Color Engine:",
                        "defaultValue": {"zh": "Adobe color managed", "en": "Adobe color managed"},
                        "options": [
                            {"zh": "Adobe color managed", "en": "Adobe color managed"},
                            {"zh": "OCIO color managed", "en": "OCIO color managed"},
                        ],
                        "notesZh": "切换引擎时会弹出确认提示框（标题“颜色引擎的更改”，按钮“确定”/“取消”）。",
                    },
                    {
                        "id": "ocio_configuration",
                        "type": "dropdown",
                        "zh": "OCIO 配置 -",
                        "en": "OCIO Configuration:",
                        "defaultValue": {"zh": "ACES 1.2", "en": "ACES 1.2"},
                        "options": [{"zh": x, "en": x} for x in ocio_configs],
                        "enableRuleZh": "仅当颜色引擎为 OCIO color managed 时可用；Adobe 模式下置灰。",
                    },
                    {
                        "id": "custom_ocio_config",
                        "type": "file_picker_row",
                        "zh": "自定义 OCIO 配置：",
                        "en": "Custom OCIO Configuration:",
                        "buttons": [
                            {"zh": "选择", "en": "Choose"},
                            {"zh": "重新加载", "en": "Reload"},
                        ],
                        "defaultValue": "",
                        "enableRuleZh": "仅当 OCIO 配置选择 Custom 时启用输入框、选择与重新加载按钮；否则全部置灰。",
                    },
                    {
                        "id": "bit_depth",
                        "type": "dropdown",
                        "zh": "位深度:",
                        "en": "Depth:",
                        "defaultValue": {"zh": "每通道 8 位", "en": "8 bits per channel", "bpc": 8},
                        "options": [
                            {"zh": "每通道 8 位", "en": "8 bits per channel", "bpc": 8},
                            {"zh": "每通道 16 位", "en": "16 bits per channel", "bpc": 16},
                            {"zh": "每通道 32 位（浮点）", "en": "32 bits per channel (float)", "bpc": 32},
                        ],
                    },
                    {
                        "id": "lut_interpolation",
                        "type": "dropdown",
                        "zh": "3D LUT 插值：",
                        "en": "3D LUT Interpolation:",
                        "defaultValue": {
                            "zh": "Tetrahedral (requires GPU acceleration)",
                            "en": "Tetrahedral (requires GPU acceleration)",
                        },
                        "options": [
                            {"zh": "Trilinear", "en": "Trilinear"},
                            {
                                "zh": "Tetrahedral (requires GPU acceleration)",
                                "en": "Tetrahedral (requires GPU acceleration)",
                            },
                        ],
                    },
                    {
                        "id": "working_color_space",
                        "type": "dropdown",
                        "zh": "工作色彩空间:",
                        "en": "Working Color Space:",
                        "defaultValue": {
                            "adobeZh": "无",
                            "adobeEn": "None",
                            "ocioDefault": "compositing_linear: ACES - ACEScg",
                        },
                        "adobeOptionCountObserved": adobe_wcs_count,
                        "adobeBuiltInHighlights": [
                            {"zh": "无", "en": "None"},
                            {"zh": "Adobe RGB (1998)", "en": "Adobe RGB (1998)"},
                            {"zh": "Apple RGB", "en": "Apple RGB"},
                            {"zh": "ColorMatch RGB", "en": "ColorMatch RGB"},
                            {"zh": "image P3", "en": "image P3"},
                            {"zh": "ProPhoto RGB", "en": "ProPhoto RGB"},
                            {"zh": "sRGB IEC61966-2.1", "en": "sRGB IEC61966-2.1"},
                            {"zh": "ACEScct", "en": "ACEScct"},
                            {"zh": "ACEScg ACES Working Space AMPAS S-2014-004", "en": "ACEScg ACES Working Space AMPAS S-2014-004"},
                        ],
                        "ocioAces12Options": ocio_wcs,
                        "notesZh": "Adobe 模式下包含“无”与内置/系统 ICC 配置文件（本机实测 103 项）；OCIO ACES 1.2 默认列出 17 项角色/工作空间。",
                    },
                    {
                        "id": "show_all_working_spaces",
                        "type": "checkbox",
                        "zh": "全部显示",
                        "en": "Show All",
                        "defaultValue": False,
                        "enableRuleZh": "Adobe 模式下置灰；OCIO 模式下可用，勾选后展开全部 OCIO 色彩空间。",
                    },
                    {
                        "id": "assume_working_gamma",
                        "type": "dropdown",
                        "zh": "采用工作灰度系数:",
                        "en": "Assume Working Gamma:",
                        "defaultValue": {"zh": "2.4 (Rec. 709)", "en": "2.4 (Rec. 709)"},
                        "options": [
                            {"zh": "2.2 (sRGB)", "en": "2.2 (sRGB)"},
                            {"zh": "2.4 (Rec. 709)", "en": "2.4 (Rec. 709)"},
                        ],
                        "enableRuleZh": "仅当 Adobe 模式且工作色彩空间为“无”时可用；选择任意色彩空间或切至 OCIO 后置灰。",
                    },
                    {
                        "id": "linearize_working_space",
                        "type": "checkbox",
                        "zh": "线性化工作色彩空间",
                        "en": "Linearize Working Space",
                        "defaultValue": False,
                        "enableRuleZh": "仅当 Adobe 模式且工作色彩空间不为“无”时可用；工作空间为“无”或 OCIO 模式下置灰。",
                    },
                    {
                        "id": "blend_colors_gamma_1",
                        "type": "checkbox",
                        "zh": "使用 1.0 灰度系数混合颜色",
                        "en": "Blend Colors Using 1.0 Gamma",
                        "defaultValue": False,
                        "enableRuleZh": "Adobe 模式下可用（勾选线性化工作色彩空间后置灰）；OCIO 模式下置灰。",
                    },
                    {
                        "id": "compensate_scene_referred",
                        "type": "checkbox",
                        "zh": "补偿场景引用的配置文件",
                        "en": "Compensate for Scene-referred Profiles",
                        "defaultValue": True,
                        "enableRuleZh": "默认勾选；仅当 Adobe 模式且工作色彩空间不为“无”时可用，否则置灰。",
                    },
                    {
                        "id": "display_color_space",
                        "type": "dropdown",
                        "zh": "显示色彩空间：",
                        "en": "Display Color Space:",
                        "defaultValue": {"ocioDefault": "ACES/sRGB"},
                        "ocioAces12Options": ocio_dcs,
                        "enableRuleZh": "Adobe 模式下置灰为空；OCIO 模式下可用（ACES 1.2 共 27 项，默认 ACES/sRGB）。",
                    },
                    {
                        "id": "color_info",
                        "type": "readonly_text",
                        "internalName": "colorInfo",
                        "zh": "色彩管理状态说明",
                        "en": "Color Management Status Info",
                        "notesZh": "底部只读摘要区域，随引擎和工作空间动态更新色彩管理开关状态。",
                    },
                ],
            },
            {
                "id": "audio",
                "zh": "音频",
                "en": "Audio",
                "controls": [
                    {
                        "id": "sample_rate",
                        "type": "dropdown",
                        "zh": "采样率:",
                        "en": "Sample Rate:",
                        "defaultValue": {"zh": "48.000 kHz", "en": "48.000 kHz", "hz": 48000},
                        "options": [
                            {"zh": "22.050 kHz", "en": "22.050 kHz", "hz": 22050},
                            {"zh": "32.000 kHz", "en": "32.000 kHz", "hz": 32000},
                            {"zh": "44.100 kHz", "en": "44.100 kHz", "hz": 44100},
                            {"zh": "48.000 kHz", "en": "48.000 kHz", "hz": 48000},
                            {"zh": "96.000 kHz", "en": "96.000 kHz", "hz": 96000},
                        ],
                    }
                ],
            },
            {
                "id": "expressions",
                "zh": "表达式",
                "en": "Expressions",
                "controls": [
                    {
                        "id": "expressions_engine",
                        "type": "dropdown",
                        "zh": "表达式引擎:",
                        "en": "Expressions Engine:",
                        "defaultValue": {"zh": "JavaScript", "en": "JavaScript", "apiValue": "javascript-1.0"},
                        "options": [
                            {"zh": "旧版 ExtendScript", "en": "Legacy ExtendScript", "apiValue": "extendscript"},
                            {"zh": "JavaScript", "en": "JavaScript", "apiValue": "javascript-1.0"},
                        ],
                    }
                ],
            },
        ],
    }


def build_interpret_footage(raw: dict[str, Any]) -> dict[str, Any]:
    seq = raw["interpretFootage_seq"]
    rect = seq["initial"]["rect"]
    color_controls = seq["tabs"][1]["controls"]
    color_combos = [c for c in color_controls if c["role"] == 46 and c["rel"]["y"] > 120]
    override_count = len(color_combos[0]["options"])

    return {
        "schemaVersion": "1.0",
        "id": "interpret-footage",
        "title": {"zh": "解释素材: <文件名>", "en": "Interpret Footage: <Filename>"},
        "triggers": [
            {
                "action": "Interpret Footage - Main",
                "shortcutWin": "Ctrl+Alt+G",
                "menuZh": "文件 ▸ 解释素材(G) ▸ 主要...",
            }
        ],
        "source": COMMON_SOURCE,
        "geometry": {
            "physicalPxAt144Dpi": {"w": rect["w"], "h": rect["h"]},
            "logicalPxAt96Dpi": {"w": round(rect["w"] / 1.5), "h": round(rect["h"] / 1.5)},
        },
        "persistence": {
            "remembersLastValues": False,
            "persistsPerItem": True,
            "notesZh": "设置随单个素材项保存；可通过“记住解释”（Ctrl+Alt+C）/“应用解释”（Ctrl+Alt+V）手动复制，图像序列默认帧率继承自“首选项 ▸ 导入”。",
        },
        "buttonBar": {
            "position": "bottom",
            "left": [
                {"id": "preview", "type": "checkbox", "zh": "预览", "en": "Preview", "defaultValue": True},
            ],
            "right": [
                {"id": "ok", "type": "button", "zh": "确定", "en": "OK", "role": "accept"},
                {"id": "cancel", "type": "button", "zh": "取消", "en": "Cancel", "role": "reject"},
            ],
        },
        "tabs": [
            {
                "id": "main_options",
                "zh": "主要选项",
                "en": "Main Options",
                "controls": [
                    {
                        "id": "alpha_mode",
                        "type": "radio_group",
                        "zh": "Alpha",
                        "en": "Alpha",
                        "defaultValue": {"zh": "直接 - 无遮罩", "en": "Straight - Unmatted"},
                        "options": [
                            {"id": "ignore", "zh": "忽略", "en": "Ignore"},
                            {"id": "straight", "zh": "直接 - 无遮罩", "en": "Straight - Unmatted"},
                            {"id": "premultiplied", "zh": "预乘 - 有彩色遮罩:", "en": "Premultiplied - Matted With Color:"},
                        ],
                        "enableRuleZh": "素材含 Alpha 通道时可用；无 Alpha 通道时整组置灰。",
                    },
                    {
                        "id": "invert_alpha",
                        "type": "checkbox",
                        "zh": "反转 Alpha",
                        "en": "Invert Alpha",
                        "defaultValue": False,
                        "enableRuleZh": "选择“忽略”时置灰，选择“直接”或“预乘”时可用。",
                    },
                    {
                        "id": "premultiplied_color",
                        "type": "color_swatch_with_eyedropper",
                        "zh": "遮罩颜色 + 吸管",
                        "en": "Matte Color Swatch + Eyedropper",
                        "defaultValue": {"hex": "#000000"},
                        "enableRuleZh": "仅当选择“预乘 - 有彩色遮罩:”时生效。",
                    },
                    {
                        "id": "guess_alpha",
                        "type": "button",
                        "zh": "猜测",
                        "en": "Guess",
                        "enableRuleZh": "含 Alpha 通道时可用，自动判断直接或预乘。",
                    },
                    {
                        "id": "frame_rate_mode",
                        "type": "radio_group",
                        "zh": "帧速率",
                        "en": "Frame Rate",
                        "defaultValue": {"sequenceZh": "假定此帧速率:", "videoZh": "使用文件中的帧速率:"},
                        "options": [
                            {"id": "from_file", "zh": "使用文件中的帧速率:", "en": "Use frame rate from file:"},
                            {
                                "id": "assume_or_conform",
                                "zh": "假定此帧速率:（静帧/视频显示“匹配帧速率:”）",
                                "en": "Assume this frame rate: / Conform to frame rate:",
                            },
                        ],
                        "enableRuleZh": "静帧图像整组置灰；图像序列无内嵌帧率，故“使用文件中的帧速率”置灰、“假定此帧速率”默认选中。",
                    },
                    {
                        "id": "conform_frame_rate",
                        "type": "number_input",
                        "zh": "帧速率数值",
                        "en": "Frame Rate Value",
                        "unit": "fps",
                        "unitZh": "帧/秒",
                        "defaultValue": 30,
                        "range": {"min": 0.01, "max": 999, "apiMin": 0},
                        "enableRuleZh": "静帧时置灰；ExtendScript conformFrameRate 实测范围 0–999（0 表示使用源帧率，UI 最小 0.01）。",
                    },
                    {
                        "id": "drop_frame",
                        "type": "dropdown",
                        "zh": "丢帧模式",
                        "en": "Drop Frame Mode",
                        "defaultValue": {"zh": "丢帧", "en": "Drop Frame"},
                        "options": [
                            {"zh": "丢帧", "en": "Drop Frame"},
                            {"zh": "无丢帧", "en": "Non-Drop Frame"},
                        ],
                        "enableRuleZh": "仅在 29.97 / 59.94 等支持丢帧的帧率下可用，30 fps 与静帧下置灰。",
                    },
                    {
                        "id": "start_timecode_mode",
                        "type": "radio_group",
                        "zh": "开始时间码",
                        "en": "Start Timecode",
                        "defaultValue": {"zh": "使用文件中的源时间码:", "en": "Use Source Timecode from File:"},
                        "options": [
                            {"id": "from_file", "zh": "使用文件中的源时间码:", "en": "Use Source Timecode from File:"},
                            {"id": "override", "zh": "覆盖开始时间码:", "en": "Override Start Timecode:"},
                        ],
                        "enableRuleZh": "静帧图像整组置灰；序列和视频可用。",
                    },
                    {
                        "id": "override_start_timecode",
                        "type": "timecode_scrub",
                        "zh": "覆盖时间码数值",
                        "en": "Override Start Timecode Value",
                        "defaultValue": "0:00:00:00",
                        "enableRuleZh": "静帧时置灰；右侧附基准帧率只读显示（如“是 0:00:00:00  基础 30”）。",
                    },
                    {
                        "id": "separate_fields",
                        "type": "dropdown",
                        "zh": "分离场:",
                        "en": "Separate Fields:",
                        "defaultValue": {"zh": "关", "en": "Off"},
                        "options": [
                            {"zh": "关", "en": "Off"},
                            {"zh": "高场优先", "en": "Upper Field First"},
                            {"zh": "低场优先", "en": "Lower Field First"},
                        ],
                    },
                    {
                        "id": "preserve_edges",
                        "type": "checkbox",
                        "zh": "保留边缘（仅最佳品质）",
                        "en": "Preserve Edges (Best Quality Only)",
                        "defaultValue": False,
                        "enableRuleZh": "分离场为“关”时置灰；选择“高场优先”或“低场优先”后可用。",
                    },
                    {
                        "id": "remove_pulldown",
                        "type": "dropdown",
                        "zh": "移除 Pulldown:",
                        "en": "Remove Pulldown:",
                        "defaultValue": {"zh": "关", "en": "Off"},
                        "options": [
                            {"zh": "关", "en": "Off"},
                            {"zh": "WSSWW", "en": "WSSWW"},
                            {"zh": "SSWWW", "en": "SSWWW"},
                            {"zh": "SWWWS", "en": "SWWWS"},
                            {"zh": "WWWSS", "en": "WWWSS"},
                            {"zh": "WWSSW", "en": "WWSSW"},
                            {"zh": "WWWSW - 24P 高级", "en": "WWWSW - 24P Advance"},
                            {"zh": "WWSWW - 24P 高级", "en": "WWSWW - 24P Advance"},
                            {"zh": "WSWWW - 24P 高级", "en": "WSWWW - 24P Advance"},
                            {"zh": "SWWWW - 24P 高级", "en": "SWWWW - 24P Advance"},
                            {"zh": "WWWWS - 24P 高级", "en": "WWWWS - 24P Advance"},
                        ],
                        "enableRuleZh": "静帧时置灰；视频/序列需启用分离场后用于 3:2 / 24Pa 下拉移除。",
                    },
                    {
                        "id": "guess_pulldown_buttons",
                        "type": "button_row",
                        "zh": "猜测 3:2 Pulldown / 猜测 24Pa Pulldown",
                        "en": "Guess 3:2 Pulldown / Guess 24Pa Pulldown",
                        "buttons": [
                            {"zh": "猜测 3:2 Pulldown", "en": "Guess 3:2 Pulldown"},
                            {"zh": "猜测 24Pa Pulldown", "en": "Guess 24Pa Pulldown"},
                        ],
                        "enableRuleZh": "静帧时均置灰；序列/视频按场分离与帧率条件启用。",
                    },
                    {
                        "id": "pixel_aspect_ratio",
                        "type": "dropdown",
                        "zh": "像素长宽比:",
                        "en": "Pixel Aspect Ratio:",
                        "defaultValue": {"zh": "方形像素", "en": "Square Pixels", "value": 1.0},
                        "range": {"min": 0.01, "max": 100.0},
                        "options": PAR_OPTIONS,
                    },
                    {
                        "id": "loop",
                        "type": "number_input",
                        "zh": "循环:",
                        "en": "Loop:",
                        "unitZh": "次",
                        "unit": "times",
                        "defaultValue": 1,
                        "range": {"min": 1, "max": 9999, "type": "integer"},
                        "enableRuleZh": "静帧图像时置灰；序列/视频可用，ExtendScript 实测范围 1–9999。",
                    },
                    {
                        "id": "more_options",
                        "type": "button",
                        "zh": "更多选项...",
                        "en": "More Options...",
                        "enableRuleZh": "仅当格式提供专属解码选项（如 Cineon/DPX/EXR/R3D/ProRes RAW）时可用；PNG 序列与静帧置灰。",
                    },
                ],
            },
            {
                "id": "color",
                "zh": "颜色",
                "en": "Color",
                "controls": [
                    {
                        "id": "preserve_rgb",
                        "type": "checkbox",
                        "zh": "保留 RGB（禁用此项的颜色管理转换）",
                        "en": "Preserve RGB (Disable Color Management Conversions for This Item)",
                        "defaultValue": False,
                        "enableRuleZh": "项目未开启色彩管理时置灰；开启项目色彩管理后可用。",
                    },
                    {
                        "id": "media_color_space",
                        "type": "static_text",
                        "zh": "媒体色彩空间：",
                        "en": "Media Color Space:",
                        "defaultValue": {"zh": "无", "en": "None"},
                    },
                    {
                        "id": "override_media_color_space",
                        "type": "dropdown",
                        "zh": "覆盖媒体色彩空间：",
                        "en": "Override Media Color Space:",
                        "defaultValue": {"zh": "工作色彩空间 - 无", "en": "Working Color Space - None"},
                        "optionCountObserved": override_count,
                        "builtInPrefixOptions": [
                            {"zh": "工作色彩空间 - <当前工作空间>", "en": "Working Color Space - <Current>"},
                            {"zh": "嵌入", "en": "Embedded"},
                            {"zh": "其他", "en": "Other"},
                        ],
                        "enableRuleZh": "项目未开启色彩管理或勾选“保留 RGB”时置灰。",
                    },
                    {
                        "id": "show_all_color_spaces",
                        "type": "checkbox",
                        "zh": "全部显示",
                        "en": "Show All",
                        "defaultValue": False,
                        "enableRuleZh": "项目启用 OCIO 色彩管理时可用，Adobe 模式下置灰。",
                    },
                    {
                        "id": "interpret_as_linear_light",
                        "type": "dropdown",
                        "zh": "解释为线性光：",
                        "en": "Interpret As Linear Light:",
                        "defaultValue": {"zh": "对 32 bpc 打开", "en": "On for 32 bpc"},
                        "options": [
                            {"zh": "关", "en": "Off"},
                            {"zh": "开", "en": "On"},
                            {"zh": "对 32 bpc 打开", "en": "On for 32 bpc"},
                        ],
                    },
                ],
            },
        ],
    }


def build_time_stretch(raw: dict[str, Any]) -> dict[str, Any]:
    rect = raw["timeStretch"]["initial"]["rect"]
    return {
        "schemaVersion": "1.0",
        "id": "time-stretch",
        "title": {"zh": "时间延长", "en": "Time Stretch"},
        "triggers": [
            {
                "action": "Time Stretch",
                "shortcutWin": None,
                "menuZh": "图层 ▸ 时间 ▸ 时间伸缩(C)...",
                "notesZh": "菜单显示“时间伸缩(C)...”，弹窗标题实测为“时间延长”。",
            }
        ],
        "source": COMMON_SOURCE,
        "geometry": {
            "physicalPxAt144Dpi": {"w": rect["w"], "h": rect["h"]},
            "logicalPxAt96Dpi": {"w": round(rect["w"] / 1.5), "h": round(rect["h"] / 1.5)},
        },
        "persistence": {
            "remembersLastValues": False,
            "notesZh": "拉伸因数与新持续时间始终读取当前选中图层的实时 stretch/duration（默认 100%）；“固定位置”单选默认“图层进入点”（同会话记住上次选择）。",
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
                "id": "original_duration",
                "type": "static_text",
                "zh": "原持续时间:",
                "en": "Original Duration:",
                "defaultValue": "<当前图层未拉伸持续时间>",
            },
            {
                "id": "stretch_factor",
                "type": "number_scrub",
                "internalName": "StretchFactor",
                "zh": "拉伸因数 -",
                "en": "Stretch Factor:",
                "unit": "%",
                "defaultValue": 100,
                "range": {"min": -9900, "max": 9900, "deadZoneClamp": "±1"},
                "notesZh": "ExtendScript layer.stretch 实测范围为 -9900% 至 +9900%；(-1, 1) 区间会被钳位到 ±1%（负值表示时间倒放）；与“新持续时间”双向联动。",
            },
            {
                "id": "new_duration",
                "type": "timecode_scrub",
                "zh": "新持续时间:",
                "en": "New Duration:",
                "defaultValue": "<当前图层持续时间>",
                "notesZh": "与“拉伸因数”双向联动，右侧只读显示合成帧率基准（如“是 0:00:10:00  基础 30”）。",
            },
            {
                "id": "hold_in_place",
                "type": "radio_group",
                "zh": "固定位置",
                "en": "Hold in Place",
                "defaultValue": {"zh": "图层进入点", "en": "Layer In-point"},
                "options": [
                    {"id": "layer_in_point", "zh": "图层进入点", "en": "Layer In-point"},
                    {"id": "current_frame", "zh": "当前帧", "en": "Current Frame"},
                    {"id": "layer_out_point", "zh": "图层输出点", "en": "Layer Out-point"},
                ],
            },
        ],
    }


def build_auto_orient(raw: dict[str, Any]) -> dict[str, Any]:
    rect = raw["autoOrient_3d"]["initial"]["rect"]
    return {
        "schemaVersion": "1.0",
        "id": "auto-orient",
        "title": {"zh": "自动方向", "en": "Auto-Orientation"},
        "triggers": [
            {
                "action": "Auto-Orient",
                "shortcutWin": "Ctrl+Alt+O",
                "menuZh": "图层 ▸ 变换(T) ▸ 自动定向...",
                "notesZh": "菜单显示“自动定向...”，弹窗标题实测为“自动方向”。",
            }
        ],
        "source": COMMON_SOURCE,
        "geometry": {
            "physicalPxAt144Dpi": {"w": rect["w"], "h": rect["h"]},
            "logicalPxAt96Dpi": {"w": round(rect["w"] / 1.5), "h": round(rect["h"] / 1.5)},
        },
        "persistence": {
            "remembersLastValues": False,
            "persistsPerLayer": True,
            "notesZh": "直接读取并写回当前选中图层的 autoOrient 属性（新建图层默认“关”）。",
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
                "id": "auto_orient_mode",
                "type": "radio_group",
                "zh": "自动方向模式",
                "en": "Auto-Orientation Mode",
                "defaultValue": {"zh": "关", "en": "Off"},
                "options": [
                    {"id": "off", "zh": "关", "en": "Off", "visibilityZh": "2D 与 3D 图层均显示"},
                    {"id": "along_path", "zh": "沿路径定向", "en": "Orient Along Path", "visibilityZh": "2D 与 3D 图层均显示"},
                    {
                        "id": "towards_camera",
                        "zh": "定位于摄像机",
                        "en": "Orient Towards Camera",
                        "visibilityZh": "仅 3D 图层显示；2D 图层隐藏该项（不渲染）。",
                    },
                ],
            },
            {
                "id": "orient_each_char",
                "type": "checkbox",
                "zh": "独立定向每个字符（需要逐字 3D 化）",
                "en": "Orient Each Character Independently (requires Per-character 3D)",
                "defaultValue": False,
                "enableRuleZh": "2D 图层完全隐藏；3D 非文字图层显示但置灰禁用；仅 3D 文字图层启用可勾选（与“定位于摄像机”配合，可自动开启逐字 3D）。",
            },
        ],
    }


def _fmt_default(dv: Any) -> str:
    if dv is None:
        return "—"
    if isinstance(dv, dict):
        if "zh" in dv:
            return str(dv["zh"])
        if "singleLayerZh" in dv:
            return f"单层 `{dv['singleLayerZh']}` / 多层 `{dv['multiLayerZh']}`"
        if "adobeZh" in dv:
            return f"Adobe `{dv['adobeZh']}` / OCIO `{dv['ocioDefault']}`"
        if "sequenceZh" in dv:
            return f"序列 `{dv['sequenceZh']}` / 视频 `{dv['videoZh']}`"
        if "ocioDefault" in dv:
            return f"OCIO `{dv['ocioDefault']}`"
        if "hex" in dv:
            return f"`{dv['hex']}`"
        return str(dv)
    if isinstance(dv, bool):
        return "勾选" if dv else "未勾选"
    return str(dv)


def _fmt_spec(ctl: dict[str, Any]) -> str:
    parts: list[str] = []
    if "range" in ctl:
        rg = ctl["range"]
        if "min" in rg and "max" in rg:
            u = ctl.get("unit") or ctl.get("unitZh") or ""
            parts.append(f"{rg['min']}–{rg['max']}{(' ' + u) if u else ''}")
        elif ctl.get("unit"):
            parts.append(str(ctl["unit"]))
    elif ctl.get("unit"):
        parts.append(str(ctl["unit"]))
    if "options" in ctl:
        parts.append(" / ".join(o["zh"] for o in ctl["options"]))
    elif "ocioAces12Options" in ctl:
        n = len(ctl["ocioAces12Options"])
        if "adobeOptionCountObserved" in ctl:
            parts.append(f"Adobe {ctl['adobeOptionCountObserved']} 项；OCIO ACES 1.2 共 {n} 项")
        else:
            parts.append(f"OCIO ACES 1.2 共 {n} 项（如 {', '.join(ctl['ocioAces12Options'][:3])} 等）")
    elif "optionCountObserved" in ctl:
        if "builtInPrefixOptions" in ctl:
            prefix = "/".join(p["zh"] for p in ctl["builtInPrefixOptions"])
            parts.append(f"共 {ctl['optionCountObserved']} 项（{prefix} + ICC 列表）")
        else:
            parts.append(f"共 {ctl['optionCountObserved']} 项")
    if "buttons" in ctl:
        parts.append("按钮: " + " / ".join(b["zh"] for b in ctl["buttons"]))
    return "；".join(parts) if parts else "—"


def _render_controls_table(controls: list[dict[str, Any]]) -> list[str]:
    lines = [
        "| 顺序 | 控件类型 | 中文标签 | 英文标签 | 默认值 | 范围 / 单位 / 选项 | 联动与置灰规则 |",
        "|---:|---|---|---|---|---|---|",
    ]
    for idx, ctl in enumerate(controls, 1):
        dv = _fmt_default(ctl.get("defaultValue"))
        spec = _fmt_spec(ctl)
        rule = ctl.get("enableRuleZh") or ctl.get("notesZh") or "—"
        if ctl.get("type") == "radio_group" and rule == "—":
            opt_rules = [f"{o['zh']}：{o.get('enableRuleZh') or o.get('visibilityZh')}" for o in ctl.get("options", []) if o.get("enableRuleZh") or o.get("visibilityZh")]
            if opt_rules:
                rule = "；".join(opt_rules)
        lines.append(f"| {idx} | `{ctl['type']}` | {ctl['zh']} | {ctl['en']} | {dv} | {spec} | {rule} |")
    lines.append("")
    return lines


def render_generic_dialog_md(index_num: int, d: dict[str, Any]) -> list[str]:
    lines: list[str] = []
    lines.append(f"## {index_num}. {d['title']['zh']}（{d['title']['en']}）")
    lines.append("")
    lines.append(f"- **结构化数据**：[`dialogs/{d['id']}.json`](dialogs/{d['id']}.json)")
    trigs = []
    for t in d["triggers"]:
        sc = f"（`{t['shortcutWin']}`）" if t.get("shortcutWin") else "（无默认快捷键）"
        extra = f" — {t['notesZh']}" if t.get("notesZh") else ""
        trigs.append(f"`{t['menuZh']}`{sc}{extra}")
    lines.append(f"- **入口**：{'；'.join(trigs)}")
    phys = d["geometry"]["physicalPxAt144Dpi"]
    logi = d["geometry"]["logicalPxAt96Dpi"]
    lines.append(f"- **窗口尺寸**：`{phys['w']} × {phys['h']}` 物理像素 @ 144 DPI（`{logi['w']} × {logi['h']}` 逻辑像素 @ 96 DPI）")

    tabs = d.get("tabs") or []
    if tabs:
        tab_s = "｜".join(f"`{t['zh']}`（{t['en']}）" for t in tabs)
        label = "变体" if d["id"] == "markers" else "页签"
        lines.append(f"- **{label}（{len(tabs)} 个）**：{tab_s}")
    else:
        lines.append("- **页签**：无（单页对话框）")

    bb = d["buttonBar"]
    if bb.get("position") == "right_column":
        right_s = "右侧顶部纵向排列 " + " / ".join(f"`{b['zh']}`（{b['en']}）" for b in bb["right"])
        extra_bb = f"（{bb['notesZh']}）" if bb.get("notesZh") else ""
        lines.append(f"- **按钮栏**：{right_s}{extra_bb}")
    else:
        left_s = ""
        if bb.get("left"):
            left_s = "左下 " + " / ".join(f"`{b['zh']}`（{b['en']}）" for b in bb["left"]) + "；"
        right_s = "右下 " + " + ".join(f"`{b['zh']}`（{b['en']}）" for b in bb["right"]) + " 水平并排"
        lines.append(f"- **底部按钮栏**：{left_s}{right_s}。")
    lines.append(f"- **记住上次的值**：{d['persistence']['notesZh']}")
    lines.append("")

    has_submenu = bool(d.get("fileImportSubmenu"))
    if has_submenu:
        lines.append(f"### {index_num}.1 `文件 ▸ 导入(I)` 子菜单条目")
        lines.append("")
        lines.append("| 顺序 | 中文菜单项 | 英文菜单项 | 默认快捷键 | Win32 Command ID |")
        lines.append("|---:|---|---|---|---|")
        for si, item in enumerate(d["fileImportSubmenu"], 1):
            sc = f"`{item['shortcutWin']}`" if item.get("shortcutWin") else "—"
            lines.append(f"| {si} | `{item['zh']}` | `{item['en']}` | {sc} | `{item['commandId']}` |")
        lines.append("")

    if tabs:
        sub_label = "变体" if d["id"] == "markers" else "页签"
        for ti, tab in enumerate(tabs, 1):
            lines.append(f"### {index_num}.{ti} {sub_label}：{tab['zh']}（{tab['en']}）")
            lines.append("")
            lines.extend(_render_controls_table(tab["controls"]))
    else:
        if has_submenu:
            lines.append(f"### {index_num}.2 对话框控件（Dialog Controls）")
            lines.append("")
        lines.extend(_render_controls_table(d["controls"]))

    for ri, rd in enumerate(d.get("relatedDialogs") or [], 1):
        sub_no = (len(tabs) + ri) if tabs else ((2 + ri) if has_submenu else ri)
        r_phys = rd["geometry"]["physicalPxAt144Dpi"]
        r_logi = rd["geometry"]["logicalPxAt96Dpi"]
        lines.append(f"### {index_num}.{sub_no} 关联对话框：{rd['title']['zh']}（{rd['title']['en']}）")
        lines.append("")
        lines.append(
            f"- **入口**：`{rd['trigger']}`；**窗口尺寸**：`{r_phys['w']} × {r_phys['h']}` 物理像素 @ 144 DPI（`{r_logi['w']} × {r_logi['h']}` 逻辑像素 @ 96 DPI）"
        )
        if rd.get("builtInTemplates"):
            tpl_s = "、".join(f"`{t['zh']}`（{t['en']}）" for t in rd["builtInTemplates"])
            lines.append(f"- **内置模板（{len(rd['builtInTemplates'])} 个）**：{tpl_s}")
        rbb = rd.get("buttonBar") or {}
        if rbb:
            r_left = ("左下 " + " / ".join(f"`{b['zh']}`（{b['en']}）" for b in rbb["left"]) + "；") if rbb.get("left") else ""
            r_right = "右下 " + " + ".join(f"`{b['zh']}`（{b['en']}）" for b in rbb.get("right", [])) + " 水平并排"
            lines.append(f"- **底部按钮栏**：{r_left}{r_right}。")
        lines.append("")
        lines.extend(_render_controls_table(rd["controls"]))

    return lines
