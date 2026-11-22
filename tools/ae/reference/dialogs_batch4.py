#!/usr/bin/env python3
"""Batch 4 Windows AE 26.0 dialog builders.

Covers:
  - Render Settings (渲染设置) + Render Settings Templates (渲染设置模板)
  - Output Module Settings (输出模块设置) + Output Module Templates (输出模块模板)
  - Keyboard Shortcuts (键盘快捷键)
  - File > Import options & Import File dialog (文件 ▸ 导入 / 导入文件) + New Placeholder (新占位符)
"""

from __future__ import annotations

from typing import Any

from dialogs_batch2 import COMMON_SOURCE


def _find_combo_options(controls: list[dict[str, Any]], val_prefix: str) -> list[str]:
    for c in controls:
        if c.get("roleName") == "ComboBox" and str(c.get("value", "")).startswith(val_prefix):
            return list(c.get("options") or [])
    return []


def build_render_settings(raw: dict[str, Any]) -> dict[str, Any]:
    rs = raw["renderSettings"]
    rect = rs["editorInitial"]["rect"]
    t_rect = rs["templatesDialog"]["rect"]
    return {
        "schemaVersion": "1.0",
        "id": "render-settings",
        "title": {"zh": "渲染设置", "en": "Render Settings"},
        "triggers": [
            {
                "action": "Render Settings (Render Queue item)",
                "shortcutWin": None,
                "menuZh": "渲染队列面板 ▸ 点击渲染设置蓝字（如“最佳设置”）",
            },
            {
                "action": "Render Settings Templates",
                "shortcutWin": None,
                "menuZh": "编辑 ▸ 模板 ▸ 渲染设置(R)... ▸ 编辑...",
            },
        ],
        "source": COMMON_SOURCE,
        "geometry": {
            "physicalPxAt144Dpi": {"w": rect["w"], "h": rect["h"]},
            "logicalPxAt96Dpi": {"w": round(rect["w"] / 1.5), "h": round(rect["h"] / 1.5)},
        },
        "persistence": {
            "remembersLastValues": False,
            "persistsViaTemplates": True,
            "notesZh": "单次弹窗修改仅作用于当前渲染队列项；新建渲染队列项按“渲染设置模板”对话框中指定的默认模板（影片默认“最佳设置”、帧默认“当前设置”、预渲染默认“最佳设置”、影片代理默认“草图设置”、静止代理默认“最佳设置”）初始化，模板本身跨重启持久化。",
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
                "id": "quality",
                "type": "dropdown",
                "zh": "品质:",
                "en": "Quality:",
                "defaultValue": {"zh": "最佳", "en": "Best"},
                "options": [
                    {"zh": "当前设置", "en": "Current Settings"},
                    {"zh": "最佳", "en": "Best"},
                    {"zh": "草图", "en": "Draft"},
                    {"zh": "线框", "en": "Wireframe"},
                ],
                "notesZh": "默认值以“最佳设置”模板为例。",
            },
            {
                "id": "resolution",
                "type": "dropdown",
                "zh": "分辨率:",
                "en": "Resolution:",
                "defaultValue": {"zh": "完整", "en": "Full"},
                "options": [
                    {"zh": "当前设置", "en": "Current Settings"},
                    {"zh": "完整", "en": "Full"},
                    {"zh": "二分之一", "en": "Half"},
                    {"zh": "三分之一", "en": "Third"},
                    {"zh": "四分之一", "en": "Quarter"},
                    {"zh": "自定义...", "en": "Custom..."},
                ],
                "notesZh": "选择“自定义...”时弹出自定义分辨率子对话框，指定水平/垂直采样步长；下方只读显示实际渲染尺寸。",
            },
            {
                "id": "disk_cache",
                "type": "dropdown",
                "zh": "磁盘缓存:",
                "en": "Disk Cache:",
                "defaultValue": {"zh": "只读", "en": "Read Only"},
                "options": [
                    {"zh": "当前设置", "en": "Current Settings"},
                    {"zh": "只读", "en": "Read Only"},
                ],
            },
            {
                "id": "proxy_use",
                "type": "dropdown",
                "zh": "代理使用:",
                "en": "Proxy Use:",
                "defaultValue": {"zh": "不使用代理", "en": "Use No Proxies"},
                "options": [
                    {"zh": "当前设置", "en": "Current Settings"},
                    {"zh": "使用所有代理", "en": "Use All Proxies"},
                    {"zh": "仅使用合成代理", "en": "Use Comp Proxies Only"},
                    {"zh": "不使用代理", "en": "Use No Proxies"},
                ],
            },
            {
                "id": "effects",
                "type": "dropdown",
                "zh": "效果:",
                "en": "Effects:",
                "defaultValue": {"zh": "当前设置", "en": "Current Settings"},
                "options": [
                    {"zh": "当前设置", "en": "Current Settings"},
                    {"zh": "全部开启", "en": "All On"},
                    {"zh": "全部关闭", "en": "All Off"},
                ],
            },
            {
                "id": "solo_switches",
                "type": "dropdown",
                "zh": "单独开关:",
                "en": "Solo Switches:",
                "defaultValue": {"zh": "当前设置", "en": "Current Settings"},
                "options": [
                    {"zh": "当前设置", "en": "Current Settings"},
                    {"zh": "全部关闭", "en": "All Off"},
                ],
            },
            {
                "id": "guide_layers",
                "type": "dropdown",
                "zh": "引导层:",
                "en": "Guide Layers:",
                "defaultValue": {"zh": "全部关闭", "en": "All Off"},
                "options": [
                    {"zh": "当前设置", "en": "Current Settings"},
                    {"zh": "全部关闭", "en": "All Off"},
                ],
            },
            {
                "id": "color_depth",
                "type": "dropdown",
                "zh": "颜色深度:",
                "en": "Color Depth:",
                "defaultValue": {"zh": "当前设置", "en": "Current Settings"},
                "options": [
                    {"zh": "当前设置", "en": "Current Settings"},
                    {"zh": "每通道 8 位", "en": "8 bits per channel"},
                    {"zh": "每通道 16 位", "en": "16 bits per channel"},
                    {"zh": "每通道 32 位", "en": "32 bits per channel"},
                ],
            },
            {
                "id": "frame_blending",
                "type": "dropdown",
                "zh": "帧混合:",
                "en": "Frame Blending:",
                "defaultValue": {"zh": "对选中图层打开", "en": "On for Checked Layers"},
                "options": [
                    {"zh": "当前设置", "en": "Current Settings"},
                    {"zh": "对选中图层打开", "en": "On for Checked Layers"},
                    {"zh": "对所有图层关闭", "en": "Off for All Layers"},
                ],
                "notesZh": "位于下半区“时间采样”（Time Sampling）分组。",
            },
            {
                "id": "field_render",
                "type": "dropdown",
                "zh": "场渲染:",
                "en": "Field Render:",
                "defaultValue": {"zh": "关", "en": "Off"},
                "options": [
                    {"zh": "关", "en": "Off"},
                    {"zh": "高场优先", "en": "Upper Field First"},
                    {"zh": "低场优先", "en": "Lower Field First"},
                ],
            },
            {
                "id": "pulldown_3_2",
                "type": "dropdown",
                "zh": "3:2 Pulldown:",
                "en": "3:2 Pulldown:",
                "defaultValue": {"zh": "关", "en": "Off"},
                "options": [
                    {"zh": "关", "en": "Off"},
                    {"zh": "WSSWW", "en": "WSSWW"},
                    {"zh": "SSWWW", "en": "SSWWW"},
                    {"zh": "SWWWS", "en": "SWWWS"},
                    {"zh": "WWWSS", "en": "WWWSS"},
                    {"zh": "WWSSW", "en": "WWSSW"},
                ],
                "enableRuleZh": "“场渲染”为“关”时置灰禁用（实测 NOFOCUS）；选择高场优先或低场优先后启用。",
            },
            {
                "id": "motion_blur",
                "type": "dropdown",
                "zh": "运动模糊:",
                "en": "Motion Blur:",
                "defaultValue": {"zh": "对选中图层打开", "en": "On for Checked Layers"},
                "options": [
                    {"zh": "当前设置", "en": "Current Settings"},
                    {"zh": "对选中图层打开", "en": "On for Checked Layers"},
                    {"zh": "对所有图层关闭", "en": "Off for All Layers"},
                ],
            },
            {
                "id": "time_span",
                "type": "dropdown",
                "zh": "时间跨度:",
                "en": "Time Span:",
                "defaultValue": {"zh": "仅工作区域", "en": "Work Area Only"},
                "options": [
                    {"zh": "合成长度", "en": "Length of Comp"},
                    {"zh": "仅工作区域", "en": "Work Area Only"},
                    {"zh": "自定义...", "en": "Custom..."},
                ],
                "notesZh": "选择“自定义...”时弹出自定义时间跨度子对话框（开始/结束/持续时间）。",
            },
            {
                "id": "frame_rate_mode",
                "type": "radio_group",
                "zh": "帧速率",
                "en": "Frame Rate",
                "defaultValue": {"zh": "使用合成的帧速率", "en": "Use comp's frame rate"},
                "options": [
                    {"id": "use_comp_fps", "zh": "使用合成的帧速率", "en": "Use comp's frame rate"},
                    {"id": "use_this_fps", "zh": "使用此帧速率:", "en": "Use this frame rate:"},
                ],
            },
            {
                "id": "frame_rate_value",
                "type": "number_scrub",
                "internalName": "frameRate",
                "zh": "使用此帧速率（数值）",
                "en": "Use this frame rate (value)",
                "unit": "fps",
                "defaultValue": 30,
                "range": {"min": 1, "max": 999, "type": "float"},
                "enableRuleZh": "选中“使用此帧速率:”或直接编辑数值时自动切换至自定义帧率。",
            },
            {
                "id": "skip_existing_files",
                "type": "checkbox",
                "zh": "跳过现有文件（允许多机渲染）",
                "en": "Skip existing files (allows multi-machine rendering)",
                "defaultValue": False,
                "notesZh": "位于底部“选项”分组，仅序列输出时生效。",
            },
        ],
        "relatedDialogs": [
            {
                "id": "render-settings-templates",
                "title": {"zh": "渲染设置模板", "en": "Render Settings Templates"},
                "trigger": "编辑 ▸ 模板 ▸ 渲染设置(R)...",
                "geometry": {
                    "physicalPxAt144Dpi": {"w": t_rect["w"], "h": t_rect["h"]},
                    "logicalPxAt96Dpi": {"w": round(t_rect["w"] / 1.5), "h": round(t_rect["h"] / 1.5)},
                },
                "builtInTemplates": [
                    {"zh": "DV 设置", "en": "DV Settings"},
                    {"zh": "多机设置", "en": "Multi-Machine Settings"},
                    {"zh": "当前设置", "en": "Current Settings"},
                    {"zh": "最佳设置", "en": "Best Settings"},
                    {"zh": "草图设置", "en": "Draft Settings"},
                ],
                "controls": [
                    {
                        "id": "movie_default",
                        "type": "dropdown",
                        "zh": "影片默认值:",
                        "en": "Movie Default:",
                        "defaultValue": {"zh": "最佳设置", "en": "Best Settings"},
                    },
                    {
                        "id": "frame_default",
                        "type": "dropdown",
                        "zh": "帧默认值:",
                        "en": "Frame Default:",
                        "defaultValue": {"zh": "当前设置", "en": "Current Settings"},
                    },
                    {
                        "id": "prerender_default",
                        "type": "dropdown",
                        "zh": "预渲染默认值:",
                        "en": "Pre-Render Default:",
                        "defaultValue": {"zh": "最佳设置", "en": "Best Settings"},
                    },
                    {
                        "id": "movie_proxy_default",
                        "type": "dropdown",
                        "zh": "影片代理默认值:",
                        "en": "Movie Proxy Default:",
                        "defaultValue": {"zh": "草图设置", "en": "Draft Settings"},
                    },
                    {
                        "id": "still_proxy_default",
                        "type": "dropdown",
                        "zh": "静止代理默认值:",
                        "en": "Still Proxy Default:",
                        "defaultValue": {"zh": "最佳设置", "en": "Best Settings"},
                    },
                    {
                        "id": "settings_name",
                        "type": "dropdown",
                        "zh": "设置名称:",
                        "en": "Settings Name:",
                        "defaultValue": {"zh": "最佳设置", "en": "Best Settings"},
                        "buttons": [
                            {"zh": "新建...", "en": "New..."},
                            {"zh": "编辑...", "en": "Edit..."},
                            {"zh": "复制", "en": "Duplicate"},
                            {"zh": "删除", "en": "Delete"},
                        ],
                    },
                ],
                "buttonBar": {
                    "position": "bottom",
                    "left": [
                        {"id": "save_all", "type": "button", "zh": "全部保存...", "en": "Save All..."},
                        {"id": "load", "type": "button", "zh": "加载...", "en": "Load..."},
                    ],
                    "right": [
                        {"id": "ok", "type": "button", "zh": "确定", "en": "OK", "role": "accept"},
                        {"id": "cancel", "type": "button", "zh": "取消", "en": "Cancel", "role": "reject"},
                    ],
                },
            }
        ],
    }


def build_output_module_settings(raw: dict[str, Any]) -> dict[str, Any]:
    om = raw["outputModuleSettings"]
    rect = om["editorInitial"]["rect"]
    t_rect = om["templatesDialog"]["rect"]
    color_ctls = om["editorTabs"][1]["controls"]
    color_space_count = len(_find_combo_options(color_ctls, "工作空间")) or 104

    return {
        "schemaVersion": "1.0",
        "id": "output-module-settings",
        "title": {"zh": "输出模块设置", "en": "Output Module Settings"},
        "triggers": [
            {
                "action": "Output Module Settings (Render Queue item)",
                "shortcutWin": None,
                "menuZh": "渲染队列面板 ▸ 点击输出模块蓝字（如“H.264 - 匹配渲染设置 - 40 Mbps”）",
            },
            {
                "action": "Output Module Templates",
                "shortcutWin": None,
                "menuZh": "编辑 ▸ 模板 ▸ 输出模块(M)... ▸ 编辑...",
            },
        ],
        "source": COMMON_SOURCE,
        "geometry": {
            "physicalPxAt144Dpi": {"w": rect["w"], "h": rect["h"]},
            "logicalPxAt96Dpi": {"w": round(rect["w"] / 1.5), "h": round(rect["h"] / 1.5)},
        },
        "persistence": {
            "remembersLastValues": False,
            "persistsViaTemplates": True,
            "notesZh": "单次弹窗修改仅作用于当前输出模块；新建输出模块按“输出模块模板”中指定的默认模板初始化（影片与保存当前预览默认“H.264 - 匹配渲染设置 - 40 Mbps”、帧默认“Photoshop”、预渲染与影片代理默认“无损”），模板跨重启持久化。",
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
                "id": "main_options",
                "zh": "主要选项",
                "en": "Main Options",
                "controls": [
                    {
                        "id": "format",
                        "type": "dropdown",
                        "zh": "格式:",
                        "en": "Format:",
                        "defaultValue": {"zh": "H.264", "en": "H.264"},
                        "options": [
                            {"zh": "AIFF", "en": "AIFF"},
                            {"zh": "AVI", "en": "AVI"},
                            {"zh": "DPX/Cineon 序列", "en": "DPX/Cineon Sequence"},
                            {"zh": "H.264", "en": "H.264"},
                            {"zh": "IFF 序列", "en": "IFF Sequence"},
                            {"zh": "JPEG 序列", "en": "JPEG Sequence"},
                            {"zh": "MP3", "en": "MP3"},
                            {"zh": "OpenEXR 序列", "en": "OpenEXR Sequence"},
                            {"zh": "PNG 序列", "en": "PNG Sequence"},
                            {"zh": "Photoshop 序列", "en": "Photoshop Sequence"},
                            {"zh": "QuickTime", "en": "QuickTime"},
                            {"zh": "Radiance 序列", "en": "Radiance Sequence"},
                            {"zh": "SGI 序列", "en": "SGI Sequence"},
                            {"zh": "TIFF 序列", "en": "TIFF Sequence"},
                            {"zh": "Targa 序列", "en": "Targa Sequence"},
                            {"zh": "WAV", "en": "WAV"},
                        ],
                        "notesZh": "共 16 种输出格式；切换为纯音频格式（AIFF/MP3/WAV）时禁用视频输出分组，切换为图像序列时启用“开始 #”和“使用合成帧编号”。",
                    },
                    {
                        "id": "include_project_link",
                        "type": "checkbox",
                        "zh": "包括项目链接",
                        "en": "Include Project Link",
                        "defaultValue": True,
                    },
                    {
                        "id": "post_render_action",
                        "type": "dropdown",
                        "zh": "渲染后动作:",
                        "en": "Post-Render Action:",
                        "defaultValue": {"zh": "无", "en": "None"},
                        "options": [
                            {"zh": "无", "en": "None"},
                            {"zh": "导入", "en": "Import"},
                            {"zh": "导入和替换用法", "en": "Import & Replace Usage"},
                            {"zh": "设置代理", "en": "Set Proxy"},
                        ],
                    },
                    {
                        "id": "include_source_xmp",
                        "type": "checkbox",
                        "zh": "包括源 XMP 元数据",
                        "en": "Include Source XMP Metadata",
                        "defaultValue": False,
                    },
                    {
                        "id": "video_output",
                        "type": "checkbox",
                        "zh": "视频输出",
                        "en": "Video Output",
                        "defaultValue": True,
                        "enableRuleZh": "取消勾选或选择纯音频格式时，下方通道/深度/颜色/格式选项/调整大小/裁剪全部置灰。",
                    },
                    {
                        "id": "channels",
                        "type": "dropdown",
                        "zh": "通道:",
                        "en": "Channels:",
                        "defaultValue": {"zh": "RGB", "en": "RGB"},
                        "options": [
                            {"zh": "RGB", "en": "RGB"},
                            {"zh": "Alpha", "en": "Alpha"},
                            {"zh": "RGB + Alpha", "en": "RGB + Alpha"},
                        ],
                        "enableRuleZh": "格式不支持 Alpha（如 H.264/JPEG）时仅 RGB 可选或回退为 RGB。",
                    },
                    {
                        "id": "depth",
                        "type": "dropdown",
                        "zh": "深度:",
                        "en": "Depth:",
                        "defaultValue": {"zh": "数百万种颜色", "en": "Millions of Colors"},
                        "options": [
                            {"zh": "256 色", "en": "256 Colors"},
                            {"zh": "数百万种颜色", "en": "Millions of Colors"},
                            {"zh": "数百万种以上颜色", "en": "Millions of Colors+"},
                            {"zh": "数万亿种颜色", "en": "Trillions of Colors"},
                            {"zh": "数万亿种以上颜色", "en": "Trillions of Colors+"},
                            {"zh": "浮点", "en": "Floating Point"},
                            {"zh": "浮点+", "en": "Floating Point+"},
                            {"zh": "256 种灰色", "en": "256 Grays"},
                            {"zh": "浮点灰度", "en": "Floating Point Gray"},
                        ],
                        "notesZh": "带“+”项表示含 Alpha 通道，与“通道”联动过滤。",
                    },
                    {
                        "id": "color_alpha_mode",
                        "type": "dropdown",
                        "zh": "颜色:",
                        "en": "Color:",
                        "defaultValue": {"zh": "预乘（遮罩）", "en": "Premultiplied (Matted)"},
                        "options": [
                            {"zh": "直接（无遮罩）", "en": "Straight (Unmatted)"},
                            {"zh": "预乘（遮罩）", "en": "Premultiplied (Matted)"},
                        ],
                        "enableRuleZh": "仅当通道包含 Alpha 时对输出有实质影响。",
                    },
                    {
                        "id": "starting_number",
                        "type": "number_scrub",
                        "internalName": "enStartingNumber",
                        "zh": "开始 #:",
                        "en": "Starting #:",
                        "defaultValue": 0,
                        "range": {"min": 0, "max": 9999999, "type": "integer"},
                        "enableRuleZh": "影片格式（如 H.264/QuickTime/AVI）下置灰禁用；图像序列且未勾选“使用合成帧编号”时启用。",
                    },
                    {
                        "id": "use_comp_frame_number",
                        "type": "checkbox",
                        "zh": "使用合成帧编号",
                        "en": "Use Comp Frame Number",
                        "defaultValue": True,
                        "enableRuleZh": "影片格式下置灰禁用；图像序列下启用，勾选时覆盖“开始 #”。",
                    },
                    {
                        "id": "video_format_options",
                        "type": "button",
                        "zh": "格式选项...",
                        "en": "Format Options...",
                        "notesZh": "打开当前视频格式的编解码器设置子对话框；按钮下方只读显示编码摘要（H.264 默认模板显示“VBR，目标 40 Mbps”）。",
                    },
                    {
                        "id": "resize",
                        "type": "checkbox",
                        "zh": "调整大小",
                        "en": "Resize",
                        "defaultValue": False,
                        "enableRuleZh": "未勾选时，锁定长宽比、宽高数值框、预设下拉、调整大小后品质全部置灰。",
                    },
                    {
                        "id": "resize_lock_aspect",
                        "type": "checkbox",
                        "zh": "锁定长宽比为 <W>:<H> (<ratio>)",
                        "en": "Lock Aspect Ratio to <W>:<H> (<ratio>)",
                        "defaultValue": True,
                        "enableRuleZh": "仅勾选“调整大小”后启用。",
                    },
                    {
                        "id": "resize_width",
                        "type": "number_scrub",
                        "internalName": "enStretchToWidth",
                        "zh": "调整大小到（宽度）",
                        "en": "Resize to (Width)",
                        "unit": "px",
                        "defaultValue": 320,
                        "range": {"min": 4, "max": 30000, "type": "integer"},
                        "enableRuleZh": "仅勾选“调整大小”后启用；模板内默认 320×240，实际队列项默认同步当前合成尺寸。",
                    },
                    {
                        "id": "resize_height",
                        "type": "number_scrub",
                        "internalName": "enStretchToHeight",
                        "zh": "调整大小到（高度）",
                        "en": "Resize to (Height)",
                        "unit": "px",
                        "defaultValue": 240,
                        "range": {"min": 4, "max": 30000, "type": "integer"},
                        "enableRuleZh": "仅勾选“调整大小”后启用。",
                    },
                    {
                        "id": "resize_preset",
                        "type": "dropdown",
                        "zh": "调整大小预设",
                        "en": "Resize Preset",
                        "defaultValue": {"zh": "自定义", "en": "Custom"},
                        "optionCountObserved": 26,
                        "notesZh": "共 26 项（自定义 + 与合成设置完全相同的 25 个内置预设）；仅勾选“调整大小”后启用。",
                    },
                    {
                        "id": "resize_quality",
                        "type": "dropdown",
                        "zh": "调整大小后的品质:",
                        "en": "Resize Quality:",
                        "defaultValue": {"zh": "高", "en": "High"},
                        "options": [
                            {"zh": "低", "en": "Low"},
                            {"zh": "高", "en": "High"},
                        ],
                        "enableRuleZh": "仅勾选“调整大小”后启用。",
                    },
                    {
                        "id": "crop",
                        "type": "checkbox",
                        "zh": "裁剪",
                        "en": "Crop",
                        "defaultValue": False,
                        "enableRuleZh": "未勾选时，“使用目标区域”和顶部/左侧/底部/右侧数值框全部置灰。",
                    },
                    {
                        "id": "use_roi",
                        "type": "checkbox",
                        "zh": "使用目标区域",
                        "en": "Use Region of Interest",
                        "defaultValue": False,
                        "enableRuleZh": "仅勾选“裁剪”后启用；勾选时自动填充合成当前 ROI 边界。",
                    },
                    {
                        "id": "crop_edges",
                        "type": "number_scrub_group",
                        "zh": "顶部 / 左侧 / 底部 / 右侧",
                        "en": "Top / Left / Bottom / Right",
                        "unit": "px",
                        "defaultValue": 0,
                        "fields": [
                            {"internalName": "enCropTop", "zh": "顶部:", "en": "Top:"},
                            {"internalName": "enCropLeft", "zh": "左侧:", "en": "Left:"},
                            {"internalName": "enCropBottom", "zh": "底部:", "en": "Bottom:"},
                            {"internalName": "enCropRight", "zh": "右侧:", "en": "Right:"},
                        ],
                        "enableRuleZh": "仅勾选“裁剪”后启用；正值向内裁剪、负值向外扩展。",
                    },
                    {
                        "id": "audio_output_mode",
                        "type": "dropdown",
                        "zh": "音频输出",
                        "en": "Audio Output",
                        "defaultValue": {"zh": "自动音频输出", "en": "Audio Output Auto"},
                        "options": [
                            {"zh": "打开音频输出", "en": "Audio Output On"},
                            {"zh": "自动音频输出", "en": "Audio Output Auto"},
                            {"zh": "关闭音频输出", "en": "Audio Output Off"},
                        ],
                        "enableRuleZh": "选择“关闭音频输出”或格式不支持音频（图像序列）时，下方采样率/位深/声道/格式选项置灰。",
                    },
                    {
                        "id": "audio_sample_rate",
                        "type": "dropdown",
                        "zh": "音频采样率",
                        "en": "Audio Sample Rate",
                        "defaultValue": {"zh": "48.000 kHz", "en": "48.000 kHz"},
                        "options": [
                            {"zh": "16.000 kHz", "en": "16.000 kHz"},
                            {"zh": "22.050 kHz", "en": "22.050 kHz"},
                            {"zh": "24.000 kHz", "en": "24.000 kHz"},
                            {"zh": "32.000 kHz", "en": "32.000 kHz"},
                            {"zh": "44.100 kHz", "en": "44.100 kHz"},
                            {"zh": "48.000 kHz", "en": "48.000 kHz"},
                        ],
                    },
                    {
                        "id": "audio_bit_depth",
                        "type": "dropdown",
                        "zh": "音频位深度",
                        "en": "Audio Bit Depth",
                        "defaultValue": {"zh": "16 位", "en": "16 Bit"},
                        "options": [
                            {"zh": "8 位", "en": "8 Bit"},
                            {"zh": "16 位", "en": "16 Bit"},
                            {"zh": "32 位", "en": "32 Bit"},
                        ],
                    },
                    {
                        "id": "audio_channels",
                        "type": "dropdown",
                        "zh": "音频声道",
                        "en": "Audio Channels",
                        "defaultValue": {"zh": "立体声", "en": "Stereo"},
                        "options": [
                            {"zh": "单声道", "en": "Mono"},
                            {"zh": "立体声", "en": "Stereo"},
                        ],
                    },
                    {
                        "id": "audio_format_options",
                        "type": "button",
                        "zh": "格式选项...（音频）",
                        "en": "Format Options... (Audio)",
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
                        "enableRuleZh": "勾选后禁用输出色彩空间转换与下拉选择。",
                    },
                    {
                        "id": "output_color_space",
                        "type": "dropdown",
                        "zh": "输出色彩空间：",
                        "en": "Output Color Space:",
                        "defaultValue": {"zh": "工作空间", "en": "Working Space"},
                        "optionCountObserved": color_space_count,
                        "builtInPrefixOptions": [
                            {"zh": "工作空间", "en": "Working Space"},
                            {"zh": "其他", "en": "Other"},
                        ],
                        "enableRuleZh": "项目未开启色彩管理或勾选“保留 RGB”时置灰（实测 Adobe 默认无色彩管理下 NOFOCUS）。",
                    },
                    {
                        "id": "show_all_color_spaces",
                        "type": "checkbox",
                        "zh": "全部显示",
                        "en": "Show All",
                        "defaultValue": False,
                        "enableRuleZh": "仅项目启用 OCIO 色彩管理时启用，Adobe 模式下置灰。",
                    },
                    {
                        "id": "convert_to_linear_light",
                        "type": "dropdown",
                        "zh": "转换为线性光:",
                        "en": "Convert to Linear Light:",
                        "defaultValue": {"zh": "对 32 bpc 打开", "en": "On for 32 bpc"},
                        "options": [
                            {"zh": "关", "en": "Off"},
                            {"zh": "开", "en": "On"},
                            {"zh": "对 32 bpc 打开", "en": "On for 32 bpc"},
                        ],
                    },
                    {
                        "id": "embed_color_profile",
                        "type": "checkbox",
                        "zh": "嵌入色彩空间",
                        "en": "Embed Color Profile",
                        "defaultValue": False,
                        "enableRuleZh": "仅当输出格式支持嵌入 ICC 配置文件（如 PNG/TIFF/PSD/JPEG/QuickTime）时启用；H.264 等不支持格式下置灰禁用。",
                    },
                ],
            },
        ],
        "relatedDialogs": [
            {
                "id": "output-module-templates",
                "title": {"zh": "输出模块模板", "en": "Output Module Templates"},
                "trigger": "编辑 ▸ 模板 ▸ 输出模块(M)...",
                "geometry": {
                    "physicalPxAt144Dpi": {"w": t_rect["w"], "h": t_rect["h"]},
                    "logicalPxAt96Dpi": {"w": round(t_rect["w"] / 1.5), "h": round(t_rect["h"] / 1.5)},
                },
                "builtInTemplates": [
                    {"zh": "多机序列", "en": "Multi-Machine Sequence"},
                    {"zh": "带有 Alpha 的 TIFF 序列", "en": "TIFF Sequence with Alpha"},
                    {"zh": "AIFF 48kHz", "en": "AIFF 48kHz"},
                    {"zh": "H.264 - 匹配渲染设置 - 15 Mbps", "en": "H.264 - Match Render Settings - 15 Mbps"},
                    {"zh": "H.264 - 匹配渲染设置 - 40 Mbps", "en": "H.264 - Match Render Settings - 40 Mbps"},
                    {"zh": "H.264 - 匹配渲染设置 - 5 Mbps", "en": "H.264 - Match Render Settings - 5 Mbps"},
                    {"zh": "Photoshop", "en": "Photoshop"},
                    {"zh": "具有 Alpha 的高品质", "en": "High Quality with Alpha"},
                    {"zh": "使用 Alpha 无损耗", "en": "Lossless with Alpha"},
                    {"zh": "仅 Alpha", "en": "Alpha Only"},
                    {"zh": "无损", "en": "Lossless"},
                    {"zh": "高品质", "en": "High Quality"},
                ],
                "controls": [
                    {
                        "id": "movie_default",
                        "type": "dropdown",
                        "zh": "影片默认值:",
                        "en": "Movie Default:",
                        "defaultValue": {"zh": "H.264 - 匹配渲染设置 - 40 Mbps", "en": "H.264 - Match Render Settings - 40 Mbps"},
                    },
                    {
                        "id": "frame_default",
                        "type": "dropdown",
                        "zh": "帧默认值:",
                        "en": "Frame Default:",
                        "defaultValue": {"zh": "Photoshop", "en": "Photoshop"},
                    },
                    {
                        "id": "save_current_preview_default",
                        "type": "dropdown",
                        "zh": "保存当前预览默认值:",
                        "en": "Save Current Preview Default:",
                        "defaultValue": {"zh": "H.264 - 匹配渲染设置 - 40 Mbps", "en": "H.264 - Match Render Settings - 40 Mbps"},
                    },
                    {
                        "id": "prerender_default",
                        "type": "dropdown",
                        "zh": "预渲染默认值:",
                        "en": "Pre-Render Default:",
                        "defaultValue": {"zh": "无损", "en": "Lossless"},
                    },
                    {
                        "id": "movie_proxy_default",
                        "type": "dropdown",
                        "zh": "影片代理默认值:",
                        "en": "Movie Proxy Default:",
                        "defaultValue": {"zh": "无损", "en": "Lossless"},
                    },
                    {
                        "id": "settings_name",
                        "type": "dropdown",
                        "zh": "设置名称:",
                        "en": "Settings Name:",
                        "defaultValue": {"zh": "H.264 - 匹配渲染设置 - 40 Mbps", "en": "H.264 - Match Render Settings - 40 Mbps"},
                        "buttons": [
                            {"zh": "新建...", "en": "New..."},
                            {"zh": "编辑...", "en": "Edit..."},
                            {"zh": "复制", "en": "Duplicate"},
                            {"zh": "删除", "en": "Delete"},
                        ],
                    },
                ],
                "buttonBar": {
                    "position": "bottom",
                    "left": [
                        {"id": "save_all", "type": "button", "zh": "全部保存...", "en": "Save All..."},
                        {"id": "load", "type": "button", "zh": "加载...", "en": "Load..."},
                    ],
                    "right": [
                        {"id": "ok", "type": "button", "zh": "确定", "en": "OK", "role": "accept"},
                        {"id": "cancel", "type": "button", "zh": "取消", "en": "Cancel", "role": "reject"},
                    ],
                },
            }
        ],
    }


def build_keyboard_shortcuts(raw: dict[str, Any]) -> dict[str, Any]:
    ks = raw["keyboardShortcuts"]
    rect = ks["rect"]
    return {
        "schemaVersion": "1.0",
        "id": "keyboard-shortcuts",
        "title": {"zh": "键盘快捷键", "en": "Keyboard Shortcuts"},
        "triggers": [
            {
                "action": "Keyboard Shortcuts",
                "shortcutWin": "Ctrl+Alt+'",
                "menuZh": "编辑 ▸ 键盘快捷键",
            }
        ],
        "source": COMMON_SOURCE,
        "geometry": {
            "physicalPxAt144Dpi": {"w": rect["w"], "h": rect["h"]},
            "logicalPxAt96Dpi": {"w": round(rect["w"] / 1.5), "h": round(rect["h"] / 1.5)},
        },
        "persistence": {
            "remembersLastValues": True,
            "persistsAcrossRestarts": True,
            "notesZh": "所选快捷键预设与自定义绑定跨重启持久化保存为用户目录 `aeks/` 下的 `.txt` 预设文件；修改内置 `After Effects 默认值.txt` 时自动切换为 `自定义.txt`（或用户另存的新预设）。",
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
                "id": "preset",
                "type": "dropdown",
                "zh": "预设:",
                "en": "Keyboard Layout Preset:",
                "defaultValue": {"zh": "After Effects 默认值.txt", "en": "After Effects Default.txt"},
                "options": [
                    {"zh": "After Effects 默认值.txt", "en": "After Effects Default.txt"},
                    {"zh": "自定义.txt", "en": "Custom.txt"},
                ],
                "notesZh": "列出内置只读预设及所有用户自定义 `.txt` 预设文件。",
            },
            {
                "id": "save_as",
                "type": "button",
                "zh": "另存为...",
                "en": "Save As...",
            },
            {
                "id": "delete_preset",
                "type": "button",
                "zh": "删除",
                "en": "Delete",
                "enableRuleZh": "选中内置 `After Effects 默认值.txt` 时置灰禁用；选中自定义/用户预设时可用。",
            },
            {
                "id": "copy_to_clipboard",
                "type": "button",
                "zh": "复制到剪贴板",
                "en": "Copy to Clipboard",
                "notesZh": "将当前预设的完整快捷键表复制为纯文本。",
            },
            {
                "id": "keyboard_layout",
                "type": "dropdown",
                "zh": "键盘布局 -",
                "en": "Keyboard Layout:",
                "defaultValue": {"zh": "en", "en": "en"},
                "options": [
                    {"zh": "en", "en": "en"},
                    {"zh": "pt", "en": "pt"},
                    {"zh": "ja", "en": "ja"},
                    {"zh": "ko", "en": "ko"},
                    {"zh": "sv", "en": "sv"},
                    {"zh": "ru", "en": "ru"},
                    {"zh": "fr", "en": "fr"},
                    {"zh": "es", "en": "es"},
                    {"zh": "nb", "en": "nb"},
                    {"zh": "it", "en": "it"},
                    {"zh": "de", "en": "de"},
                ],
                "notesZh": "共 11 种物理键盘区域布局代码，仅影响可视化键盘绘制与键帽标签。",
            },
            {
                "id": "command_scope",
                "type": "dropdown",
                "zh": "命令:",
                "en": "Commands:",
                "defaultValue": {"zh": "应用程序", "en": "Application"},
                "options": [
                    {"zh": "应用程序", "en": "Application"},
                    {"zh": "合成面板", "en": "Composition Panel"},
                    {"zh": "图层面板", "en": "Layer Panel"},
                    {"zh": "字符和段落面板", "en": "Character and Paragraph Panels"},
                    {"zh": "应用", "en": "App"},
                    {"zh": "效果控件面板", "en": "Effect Controls Panel"},
                    {"zh": "时间轴面板", "en": "Timeline Panel"},
                    {"zh": "查看器面板", "en": "Viewer Panel"},
                    {"zh": "流程图面板", "en": "Flowchart Panel"},
                    {"zh": "渲染队列面板", "en": "Render Queue Panel"},
                    {"zh": "绘画面板", "en": "Paint Panel"},
                    {"zh": "跟踪器面板", "en": "Tracker Panel"},
                    {"zh": "项目面板", "en": "Project Panel"},
                ],
                "notesZh": "共 13 个作用域；切换后联动刷新可视化键盘高亮（紫色=全局应用快捷键，绿色=当前面板覆盖快捷键）。",
            },
            {
                "id": "visual_keyboard",
                "type": "custom_keyboard_map",
                "zh": "可视化键盘布局",
                "en": "Visual Keyboard Layout",
                "notesZh": "按主键盘、功能区、方向键、小键盘分布；点击任意键位可在右下查看其全部修饰键组合，支持从下方命令树拖拽到键帽上绑定；下方附 3 色图例（应用全局快捷键 / 面板焦点覆盖快捷键 / 按下辅助按键）。",
            },
            {
                "id": "search_box",
                "type": "text_input",
                "zh": "搜索框",
                "en": "Search",
                "defaultValue": "",
                "notesZh": "按命令名称或快捷键文字实时过滤下方命令树。",
            },
            {
                "id": "command_tree",
                "type": "tree_table",
                "zh": "命令与快捷键列表",
                "en": "Command / Shortcut List",
                "notesZh": "位于左下区域（宽 1170 px @ 144 DPI），按菜单与面板分组折叠展示命令与绑定快捷键胶囊，支持双击/点击胶囊录制或删除多个快捷键。",
            },
            {
                "id": "key_modifier_list",
                "type": "table",
                "zh": "键: <所选键位>",
                "en": "Key: <Selected Key>",
                "notesZh": "位于右下区域，展示当前所选物理键搭配 Ctrl / Shift / Alt 等所有修饰键组合的对应命令及冲突提示。",
            },
        ],
    }


def build_import_file(raw: dict[str, Any]) -> dict[str, Any]:
    imp = raw["importFile"]
    rect = imp["rect"]
    ph = raw["importPlaceholder"]
    ph_rect = ph["rect"]

    return {
        "schemaVersion": "1.0",
        "id": "import-file",
        "title": {"zh": "导入文件（及文件 ▸ 导入 选项）", "en": "Import File (and File > Import Options)"},
        "triggers": [
            {"action": "Import File", "shortcutWin": "Ctrl+I", "menuZh": "文件 ▸ 导入(I) ▸ 文件..."},
            {"action": "Import Multiple Files", "shortcutWin": "Ctrl+Alt+I", "menuZh": "文件 ▸ 导入(I) ▸ 多个文件..."},
        ],
        "source": COMMON_SOURCE,
        "geometry": {
            "physicalPxAt144Dpi": {"w": rect["w"], "h": rect["h"]},
            "logicalPxAt96Dpi": {"w": round(rect["w"] / 1.5), "h": round(rect["h"] / 1.5)},
        },
        "persistence": {
            "remembersLastValues": True,
            "persistsAcrossRestarts": True,
            "notesZh": "Windows 原生文件对话框记住上次浏览目录与窗口尺寸；“导入为”与“创建合成”在同会话内按所选文件类型动态联动（“多个文件...”会在导入一批后循环重新打开弹窗直至点击“完成”/取消）。",
        },
        "buttonBar": {
            "position": "bottom_right",
            "right": [
                {"id": "import_folder", "type": "button", "zh": "导入文件夹", "en": "Import Folder"},
                {"id": "import", "type": "button", "zh": "导入", "en": "Import", "role": "accept"},
                {"id": "cancel", "type": "button", "zh": "取消", "en": "Cancel", "role": "reject"},
            ],
        },
        "fileImportSubmenu": [
            {"zh": "文件...", "en": "File...", "shortcutWin": "Ctrl+I", "commandId": 9764865},
            {"zh": "多个文件...", "en": "Multiple Files...", "shortcutWin": "Ctrl+Alt+I", "commandId": 9764866},
            {"zh": "从 Libraries 中...", "en": "From Libraries...", "shortcutWin": None, "commandId": 9764867},
            {"zh": "导入 Adobe Premiere 项目...", "en": "Adobe Premiere Pro Project...", "shortcutWin": None, "commandId": 9764868},
            {"zh": "Vanishing Point (.vpe)...", "en": "Vanishing Point (.vpe)...", "shortcutWin": None, "commandId": 9764869},
            {"zh": "占位符...", "en": "Placeholder...", "shortcutWin": None, "commandId": 9764870},
            {"zh": "纯色...", "en": "Solid...", "shortcutWin": None, "commandId": 9764871},
        ],
        "tabs": [],
        "controls": [
            {
                "id": "format_importer",
                "type": "dropdown",
                "zh": "格式:",
                "en": "Enable: / Format:",
                "defaultValue": "<按所选文件自动检测>",
                "enableRuleZh": "未选中文件时置灰禁用（实测 NOFOCUS）；选中受支持文件后按文件扩展名自动匹配对应导入器。",
            },
            {
                "id": "import_as",
                "type": "dropdown",
                "zh": "导入为:",
                "en": "Import As:",
                "defaultValue": {"zh": "素材", "en": "Footage"},
                "options": [
                    {"zh": "素材", "en": "Footage"},
                    {"zh": "合成", "en": "Composition"},
                    {"zh": "合成 - 保持图层大小", "en": "Composition - Retain Layer Sizes"},
                ],
                "enableRuleZh": "未选中分层文件（如 PSD/AI/AEP/FLA）时置灰禁用；选中多层文档时启用，选为合成方式时还会弹出图层选项子对话框。",
            },
            {
                "id": "sequence_checkbox",
                "type": "checkbox",
                "zh": "<格式> 序列（未选中时显示“序列不可用”）",
                "en": "[Format] Sequence (displays 'Sequence' disabled when unavailable)",
                "defaultValue": False,
                "enableRuleZh": "位于“序列选项:”分组；未选中图像文件或所选格式不支持序列时显示“序列不可用”并置灰；选中带编号的静帧图像时自动勾选并显示具体格式（如“PNG 序列”）。",
            },
            {
                "id": "force_alphabetical_order",
                "type": "checkbox",
                "zh": "强制按字母顺序排列",
                "en": "Force alphabetical order",
                "defaultValue": False,
                "enableRuleZh": "仅当上方“<格式> 序列”可用且勾选时启用；其余状态置灰禁用。",
            },
            {
                "id": "create_composition",
                "type": "checkbox",
                "zh": "创建合成",
                "en": "Create Composition",
                "defaultValue": False,
                "enableRuleZh": "位于“导入选项:”分组，始终可用；勾选后导入完成自动以素材参数新建合成。",
            },
            {
                "id": "file_name",
                "type": "combobox",
                "zh": "文件名(N):",
                "en": "File name:",
                "defaultValue": "",
                "notesZh": "Windows 原生文件对话框底栏输入/历史下拉。",
            },
            {
                "id": "files_of_type",
                "type": "dropdown",
                "zh": "文件类型(T):",
                "en": "Files of type:",
                "defaultValue": {"zh": "所有可接受的文件", "en": "All Acceptable Files"},
                "notesZh": "位于文件名右侧，按格式类别筛选文件列表。",
            },
        ],
        "relatedDialogs": [
            {
                "id": "new-placeholder",
                "title": {"zh": "新占位符", "en": "New Placeholder"},
                "trigger": "文件 ▸ 导入(I) ▸ 占位符...",
                "geometry": {
                    "physicalPxAt144Dpi": {"w": ph_rect["w"], "h": ph_rect["h"]},
                    "logicalPxAt96Dpi": {"w": round(ph_rect["w"] / 1.5), "h": round(ph_rect["h"] / 1.5)},
                },
                "controls": [
                    {
                        "id": "name",
                        "type": "text_input",
                        "zh": "名称:",
                        "en": "Name:",
                        "defaultValue": {"zh": "缺少名称", "en": "Missing Name"},
                    },
                    {
                        "id": "width",
                        "type": "number_scrub",
                        "internalName": "enWidth",
                        "zh": "宽度:",
                        "en": "Width:",
                        "unit": "px",
                        "defaultValue": 1920,
                        "range": {"min": 4, "max": 30000, "type": "integer"},
                    },
                    {
                        "id": "height",
                        "type": "number_scrub",
                        "internalName": "enHeight",
                        "zh": "高度:",
                        "en": "Height:",
                        "unit": "px",
                        "defaultValue": 1080,
                        "range": {"min": 4, "max": 30000, "type": "integer"},
                    },
                    {
                        "id": "frame_rate",
                        "type": "number_scrub",
                        "internalName": "enFPS",
                        "zh": "帧速率:",
                        "en": "Frame Rate:",
                        "unit": "fps",
                        "defaultValue": 30,
                        "range": {"min": 1, "max": 999, "type": "float"},
                    },
                    {
                        "id": "duration",
                        "type": "timecode_scrub",
                        "zh": "持续时间:",
                        "en": "Duration:",
                        "defaultValue": "0:01:00:00",
                        "notesZh": "右侧只读显示时间码基准（如“是 0:01:00:00  基础 30”）。",
                    },
                ],
                "buttonBar": {
                    "position": "bottom_right",
                    "right": [
                        {"id": "ok", "type": "button", "zh": "确定", "en": "OK", "role": "accept"},
                        {"id": "cancel", "type": "button", "zh": "取消", "en": "Cancel", "role": "reject"},
                    ],
                },
            }
        ],
    }
