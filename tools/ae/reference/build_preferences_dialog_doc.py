#!/usr/bin/env python3
"""Build structured Windows AE 26.0 Preferences dialog reference docs (JSON + MD).

Data source: Live MSAA inspection (dump_preferences_msaa.ps1), live enable/disable
linkage probing, and preferences.json key cross-referencing on Adobe After Effects
26.0x67 (zh_CN) on Windows 11 Pro (x64, 150% DPI scaling).

Compliance: Contains only structural UI facts (control types, short zh/en labels,
default values, dropdown option lists, numeric ranges/units, color hexes, layout
geometry, enable/disable linkages, and preferences.json key mappings). Does not
include Adobe help/tooltip/warning prose, screenshots, icons, or local paths.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[3]
OUT_JSON = REPO_ROOT / "docs" / "reference" / "ae" / "ui" / "preferences-dialog.json"
OUT_MD = REPO_ROOT / "docs" / "reference" / "ae" / "ui" / "preferences-dialog.md"


def build_data() -> dict[str, Any]:
    return {
        "meta": {
            "application": "Adobe After Effects 26.0",
            "build": "26.0x67",
            "localeObserved": "zh_CN",
            "platform": "Windows 11 Pro (10.0.26200, x64)",
            "dpiObserved": 144,
            "dpiScale": 1.5,
            "extractedDate": "2026-11-22",
            "extractionMethod": [
                "Win32 WM_COMMAND menu invocation + MSAA (oleacc.dll AccessibleObjectFromWindow / AccessibleChildren) across all 16 active pages",
                "Live MSAA enable/disable linkage probing (toggling checkboxes/radios and diffing control focusable/unavailable state, cancelled without saving)",
                "Cross-referenced against docs/reference/ae/preferences.json and docs/reference/ae/menus.json",
            ],
            "sanitizationNote": (
                "Excludes Adobe help/tooltip/warning prose, screenshots, local filesystem paths, "
                "usernames, and machine-specific audio/video hardware device names."
            ),
        },
        "dialog": {
            "titleZh": "首选项",
            "titleEn": "Preferences",
            "windowClass": "#32770",
            "resizable": False,
            "modal": True,
            "sizeLogical96Dpi": {"w": 948, "h": 801},
            "sizePhysical144Dpi": {"w": 1422, "h": 1202},
            "layout": {
                "structure": "three_column",
                "descriptionZh": "三栏布局：左侧分类列表（16 项无分割线）+ 中间分组框内容区 + 右上角纵向按钮列（确定/取消/上一步/下一步）",
                "sidebarLogical": {"x": 20, "y": 45, "w": 210, "h": 724, "rowHeight": 24},
                "contentLogical": {"x": 250, "y": 45, "w": 593},
                "buttonColumnLogical": {
                    "position": "top_right_vertical",
                    "x": 861,
                    "yStart": 45,
                    "buttonWidth": 60,
                    "buttonHeight": 24,
                    "verticalGap": 8,
                },
            },
            "buttons": [
                {
                    "id": "ok",
                    "labelZh": "确定",
                    "labelEn": "OK",
                    "role": "primary_default",
                    "shortcut": "Enter",
                    "alwaysEnabled": True,
                },
                {
                    "id": "cancel",
                    "labelZh": "取消",
                    "labelEn": "Cancel",
                    "role": "cancel",
                    "shortcut": "Escape",
                    "alwaysEnabled": True,
                },
                {
                    "id": "previous",
                    "labelZh": "上一步",
                    "labelEn": "Previous",
                    "role": "navigate_prev",
                    "wrapsAround": True,
                    "alwaysEnabled": True,
                },
                {
                    "id": "next",
                    "labelZh": "下一步",
                    "labelEn": "Next",
                    "role": "navigate_next",
                    "wrapsAround": True,
                    "alwaysEnabled": True,
                },
            ],
            "sidebarGroups": [
                {
                    "groupIndex": 1,
                    "menuDividerAfter": True,
                    "pageIds": ["startup_repair", "project", "composition", "previews"],
                },
                {
                    "groupIndex": 2,
                    "menuDividerAfter": True,
                    "pageIds": ["appearance", "grids_guides", "labels", "type"],
                },
                {
                    "groupIndex": 3,
                    "menuDividerAfter": True,
                    "pageIds": ["import", "export"],
                },
                {
                    "groupIndex": 4,
                    "menuDividerAfter": True,
                    "pageIds": ["audio", "disk", "memory_cpu", "video"],
                },
                {
                    "groupIndex": 5,
                    "menuDividerAfter": False,
                    "pageIds": ["3d", "scripting_expressions"],
                },
            ],
            "sidebar": [
                {"index": 1, "id": "startup_repair", "labelZh": "启动和修复", "labelEn": "Startup & Repair", "menuCommandId": 24182785, "shortcut": "Ctrl+Alt+;"},
                {"index": 2, "id": "project", "labelZh": "项目", "labelEn": "Project", "menuCommandId": 24182786},
                {"index": 3, "id": "composition", "labelZh": "合成", "labelEn": "Composition", "menuCommandId": 24182787},
                {"index": 4, "id": "previews", "labelZh": "预览", "labelEn": "Previews", "menuCommandId": 24182788},
                {"index": 5, "id": "appearance", "labelZh": "外观", "labelEn": "Appearance", "menuCommandId": 24182790},
                {"index": 6, "id": "grids_guides", "labelZh": "网格和参考线", "labelEn": "Grids & Guides", "menuCommandId": 24182791},
                {"index": 7, "id": "labels", "labelZh": "标签", "labelEn": "Labels", "menuCommandId": 24182792},
                {"index": 8, "id": "type", "labelZh": "类型", "labelEn": "Type", "menuCommandId": 24182793},
                {"index": 9, "id": "import", "labelZh": "导入", "labelEn": "Import", "menuCommandId": 24182795},
                {"index": 10, "id": "export", "labelZh": "导出", "labelEn": "Export", "menuCommandId": 24182796},
                {"index": 11, "id": "audio", "labelZh": "音频", "labelEn": "Audio", "menuCommandId": 24182798},
                {"index": 12, "id": "disk", "labelZh": "磁盘", "labelEn": "Disk", "menuCommandId": 24182799},
                {"index": 13, "id": "memory_cpu", "labelZh": "内存和 CPU", "labelEn": "Memory & CPU", "menuCommandId": 24182800},
                {"index": 14, "id": "video", "labelZh": "视频", "labelEn": "Video", "menuCommandId": 24182801},
                {"index": 15, "id": "3d", "labelZh": "3D", "labelEn": "3D", "menuCommandId": 24182803},
                {"index": 16, "id": "scripting_expressions", "labelZh": "脚本和表达式", "labelEn": "Scripting & Expressions", "menuCommandId": 24182804},
            ],
        },
        "verificationAnswers": {
            "buttonLayout": (
                "右上角纵向一列（非底部一行），自上而下依次为：确定 (OK)、取消 (Cancel)、上一步 (Previous)、下一步 (Next)。"
                "上一步/下一步在首页与末页均保持可用并循环切换。"
            ),
            "defaultsCheck": {
                "levelsOfUndo": {
                    "inDialogUi": False,
                    "backendDefault": 99,
                    "preferencesKey": {"section": "General Section", "key": "Levels of Undo v2"},
                    "note": "AE 26 已将“撤消级别”从首选项对话框 UI 中移除，仅保留后台键值，默认 99。",
                },
                "autoSaveIntervalMinutes": {
                    "inDialogUi": True,
                    "pageId": "project",
                    "default": 5,
                    "unitEn": "minutes",
                    "preferencesKey": {"section": "Auto Save", "key": "Auto Save Interval"},
                },
                "maxProjectVersions": {
                    "inDialogUi": True,
                    "pageId": "project",
                    "default": 3,
                    "preferencesKey": {"section": "Auto Save", "key": "Auto Save Max Project Vers"},
                },
            },
        },
        "legacy19ToAe26Migration": [
            {"legacyZh": "常规 (General)", "ae26Pages": ["startup_repair", "project", "composition", "appearance"], "notes": "拆入启动和修复（主屏幕）、项目（保留工作区/Dynamic Link）、合成（图层创建/拆分/插值/蒙版/同步时间/双击行为）、外观（路径点大小/系统拾色器/禁用缩览图/工具提示/引导标记）；撤消级别已移出 UI（后台默认 99）。"},
            {"legacyZh": "显示 (Display)", "ae26Pages": ["appearance"], "notes": "并入“外观”（运动路径、在信息面板和流程图中显示渲染进度、硬件加速面板、时间轴同时显示时间码和帧）。"},
            {"legacyZh": "导入 (Import)", "ae26Pages": ["import"], "notes": "保留为第 9 页“导入”，合并硬件加速解码与自动重新加载素材。"},
            {"legacyZh": "输出 (Output)", "ae26Pages": ["export", "audio"], "notes": "主体改名为第 10 页“导出”；其中的“音频块持续时间”迁至第 11 页“音频”的“导出”分组。"},
            {"legacyZh": "网格和参考线 (Grids & Guides)", "ae26Pages": ["grids_guides"], "notes": "保留为第 6 页，新增“透明度设置”网格大小/颜色分组。"},
            {"legacyZh": "标签 (Labels)", "ae26Pages": ["labels"], "notes": "保留为第 7 页，含 3D 模型、其他 3D 及标记（注释/片段）默认标签。"},
            {"legacyZh": "媒体和磁盘缓存 (Media & Disk Cache)", "ae26Pages": ["disk"], "notes": "改名为第 12 页“磁盘”，新增从磁盘缓存预览与无损压缩帧选项。"},
            {"legacyZh": "视频预览 (Video Preview)", "ae26Pages": ["video"], "notes": "改名为第 14 页“视频”，新增 HDR/扩展显示范围预览开关。"},
            {"legacyZh": "外观 (Appearance)", "ae26Pages": ["appearance"], "notes": "保留为第 5 页，旧版亮度滑块改为 3 卡片“颜色主题”（最暗/深色/灯光）+ 无障碍色彩对比度，并吸纳原“显示”页与部分“常规”页选项。"},
            {"legacyZh": "新建项目 (New Project)", "ae26Pages": ["project"], "notes": "并入第 2 页“项目”（New Project 与 Solids Folder 分组）。"},
            {"legacyZh": "自动保存 (Auto-Save)", "ae26Pages": ["project"], "notes": "并入第 2 页“项目”（自动保存与自动保存位置分组，默认 5 分钟 / 3 个版本）。"},
            {"legacyZh": "内存和性能 (Memory & Performance)", "ae26Pages": ["memory_cpu"], "notes": "改名为第 13 页“内存和 CPU”，并吸纳“GPU 信息...”按钮（原在预览页）。"},
            {"legacyZh": "音频硬件 (Audio Hardware)", "ae26Pages": ["audio"], "notes": "并入第 11 页“音频”（音频硬件分组）。"},
            {"legacyZh": "音频输出映射 (Audio Output Mapping)", "ae26Pages": ["audio"], "notes": "并入第 11 页“音频”（输出渠道映射分组）。"},
            {"legacyZh": "同步设置 (Sync Settings)", "ae26Pages": [], "notes": "AE 26 已移除此页。"},
            {"legacyZh": "类型 (Type)", "ae26Pages": ["type"], "notes": "保留为第 8 页（字体菜单预览大小、近期字体数量、缺失字体提醒开关；旧版文本引擎单选已移除）。"},
            {"legacyZh": "3D", "ae26Pages": ["3d"], "notes": "保留为第 15 页（摄像机导航、推拉镜头、3D 缓存）。"},
            {"legacyZh": "脚本和表达式 (Scripting & Expressions)", "ae26Pages": ["scripting_expressions"], "notes": "保留为第 16 页。"},
            {"legacyZh": "通知 (Notifications)", "ae26Pages": ["export"], "notes": "并入第 10 页“导出”（渲染通知分组）。"},
            {"legacyZh": "启动和修复 (Startup & Repair)", "ae26Pages": ["startup_repair"], "notes": "提升为第 1 页（默认打开页，Ctrl+Alt+;）。"},
        ],
        "pages": [
            # ==================== PAGE 1: Startup & Repair ====================
            {
                "index": 1,
                "id": "startup_repair",
                "labelZh": "启动和修复",
                "labelEn": "Startup & Repair",
                "menuCommandId": 24182785,
                "shortcut": "Ctrl+Alt+;",
                "groups": [
                    {
                        "id": "startup_options",
                        "titleZh": "启动选项",
                        "titleEn": "Startup Options",
                        "controls": [
                            {
                                "id": "enable_home_screen",
                                "type": "checkbox",
                                "labelZh": "启用主屏幕",
                                "labelEn": "Enable Home Screen",
                                "default": True,
                                "preferencesKey": {"section": "General Section", "key": "Show Welcome Screen"},
                            },
                            {
                                "id": "show_system_compatibility_issues",
                                "type": "checkbox",
                                "labelZh": "显示系统兼容性问题",
                                "labelEn": "Show System Compatibility Issues",
                                "default": True,
                            },
                        ],
                    },
                    {
                        "id": "repair_options",
                        "titleZh": "修复选项",
                        "titleEn": "Repair Options",
                        "controls": [
                            {
                                "id": "start_in_safe_mode",
                                "type": "button",
                                "labelZh": "在安全模式下启动...",
                                "labelEn": "Start in Safe Mode...",
                                "hasNoteText": True,
                                "noteSummary": "以默认首选项且不加载脚本/扩展临时重启",
                            },
                            {
                                "id": "empty_all_caches",
                                "type": "button",
                                "labelZh": "清空所有缓存...",
                                "labelEn": "Empty All Caches...",
                                "hasNoteText": True,
                                "noteSummary": "清空全部缓存文件夹以排查启动问题或释放空间",
                            },
                        ],
                    },
                    {
                        "id": "preferences_management",
                        "titleZh": "偏好设置管理",
                        "titleEn": "Preferences Management",
                        "controls": [
                            {
                                "id": "reveal_preferences_in_explorer",
                                "type": "button",
                                "labelZh": "在资源管理器中显示首选项",
                                "labelEn": "Reveal Preferences in Explorer",
                                "platformNotes": "macOS 显示为 Reveal Preferences in Finder",
                                "hasNoteText": True,
                                "noteSummary": "提示手动编辑首选项文件存在风险",
                            },
                            {
                                "id": "reset_all_preferences",
                                "type": "button",
                                "labelZh": "将所有偏好设置重置为默认值...",
                                "labelEn": "Reset All Preferences to Default...",
                                "hasNoteText": True,
                                "noteSummary": "需重启生效，且会重置工作区",
                            },
                            {
                                "id": "migrate_previous_version_settings",
                                "type": "button",
                                "labelZh": "迁移早期版本设置...",
                                "labelEn": "Migrate Previous Version Settings...",
                                "hasNoteText": True,
                                "noteSummary": "从旧版 AE 迁移全部首选项",
                            },
                        ],
                    },
                ],
                "footerControls": [
                    {
                        "id": "file_bug_report_link",
                        "type": "link",
                        "labelZh": "请提交错误报告。",
                        "labelEn": "File a Bug Report.",
                        "hasNoteText": True,
                        "noteSummary": "底部附错误报告外链及附带首选项/工程文件的说明",
                    }
                ],
            },
            # ==================== PAGE 2: Project ====================
            {
                "index": 2,
                "id": "project",
                "labelZh": "项目",
                "labelEn": "Project",
                "menuCommandId": 24182786,
                "groups": [
                    {
                        "id": "project_options",
                        "titleZh": "Project Options",
                        "titleEn": "Project Options",
                        "note": "AE 26.0x67 zh_CN 界面中此分组标题保留英文原文",
                        "controls": [
                            {
                                "id": "preserve_current_workspace_on_open",
                                "type": "checkbox",
                                "labelZh": "打开项目时保留当前工作区",
                                "labelEn": "Preserve Current Workspace When Opening Projects",
                                "default": True,
                            },
                            {
                                "id": "dynamic_link_highest_number",
                                "type": "checkbox",
                                "labelZh": "与 After Effects 链接的 Dynamic Link 将项目文件名与最大编号结合使用",
                                "labelEn": "Dynamic Link with After Effects Uses Project Filename with Highest Number",
                                "default": False,
                            },
                        ],
                    },
                    {
                        "id": "new_project",
                        "titleZh": "New Project",
                        "titleEn": "New Project",
                        "note": "AE 26.0x67 zh_CN 界面中此分组标题保留英文原文",
                        "controls": [
                            {
                                "id": "new_project_loads_template",
                                "type": "checkbox",
                                "labelZh": "新建项目加载模板",
                                "labelEn": "New Project Loads Template",
                                "default": False,
                                "controlsEnablementOf": ["choose_project_template"],
                                "preferencesKey": {"section": "Template Project", "key": "Enabled"},
                            },
                            {
                                "id": "choose_project_template",
                                "type": "button",
                                "labelZh": "选择项目模板...",
                                "labelEn": "Choose Project Template...",
                                "defaultEnabled": False,
                                "enabledWhen": {"control": "new_project_loads_template", "equals": True},
                                "preferencesKey": {"section": "Template Project", "key": "Location"},
                            },
                            {
                                "id": "project_template_path_display",
                                "type": "readonly_text",
                                "labelZh": "",
                                "labelEn": "",
                                "default": "",
                                "note": "显示所选模板文件路径",
                            },
                        ],
                    },
                    {
                        "id": "solids_folder",
                        "titleZh": "Solids Folder",
                        "titleEn": "Solids Folder",
                        "note": "AE 26.0x67 zh_CN 界面中此分组标题保留英文原文",
                        "controls": [
                            {
                                "id": "new_project_solids_folder",
                                "type": "text_input",
                                "labelZh": "新建项目纯色文件夹",
                                "labelEn": "New Project Solids Folder",
                                "defaultZh": "纯色",
                                "defaultEn": "Solids",
                                "hasNoteText": True,
                                "noteSummary": "下方提示当前项目的纯色文件夹名称需在项目面板重命名修改",
                                "preferencesKey": {"section": "Template Project", "key": "New Project Solids Folder"},
                            },
                        ],
                    },
                    {
                        "id": "auto_save",
                        "titleZh": "自动保存",
                        "titleEn": "Auto-Save",
                        "controls": [
                            {
                                "id": "auto_save_every_enabled",
                                "type": "checkbox",
                                "labelZh": "保存间隔:",
                                "labelEn": "Save Every:",
                                "default": True,
                                "controlsEnablementOf": ["auto_save_interval_minutes"],
                                "preferencesKey": {"section": "Auto Save", "key": "Enable Auto Save3"},
                            },
                            {
                                "id": "auto_save_interval_minutes",
                                "internalName": "ProjectV2Pref_htSaveEveryNMins",
                                "type": "number_input",
                                "labelZh": "保存间隔数值",
                                "labelEn": "Save Every Interval",
                                "inlineWith": "auto_save_every_enabled",
                                "default": 5,
                                "range": {"min": 1, "max": 1440},
                                "unitZh": "分钟",
                                "unitEn": "minutes",
                                "enabledWhen": {"control": "auto_save_every_enabled", "equals": True},
                                "preferencesKey": {"section": "Auto Save", "key": "Auto Save Interval"},
                            },
                            {
                                "id": "auto_save_on_render_queue",
                                "type": "checkbox",
                                "labelZh": "启动渲染队列时保存",
                                "labelEn": "Save When Starting Render Queue",
                                "default": True,
                                "preferencesKey": {"section": "Auto Save", "key": "Enable Auto Save RQ2"},
                            },
                            {
                                "id": "max_project_versions",
                                "internalName": "ProjectV2Pref_htMaxProjVers",
                                "type": "number_input",
                                "labelZh": "最大项目版本:",
                                "labelEn": "Maximum Project Versions:",
                                "default": 3,
                                "range": {"min": 1, "max": 999},
                                "preferencesKey": {"section": "Auto Save", "key": "Auto Save Max Project Vers"},
                            },
                        ],
                    },
                    {
                        "id": "auto_save_location",
                        "titleZh": "自动保存位置",
                        "titleEn": "Auto-Save Location",
                        "controls": [
                            {
                                "id": "auto_save_location_mode",
                                "type": "radio_group",
                                "labelZh": "自动保存位置",
                                "labelEn": "Auto-Save Location",
                                "default": "next_to_project",
                                "options": [
                                    {"value": "next_to_project", "labelZh": "项目旁边", "labelEn": "Next to Project"},
                                    {"value": "custom_location", "labelZh": "自定义位置", "labelEn": "Custom Location"},
                                ],
                                "controlsEnablementOf": ["choose_auto_save_folder"],
                                "preferencesKey": {"section": "Auto Save", "key": "Save To Custom Location"},
                            },
                            {
                                "id": "choose_auto_save_folder",
                                "type": "button",
                                "labelZh": "选择文件夹...",
                                "labelEn": "Choose Folder...",
                                "defaultEnabled": False,
                                "enabledWhen": {"control": "auto_save_location_mode", "equals": "custom_location"},
                                "preferencesKey": {"section": "Auto Save", "key": "Auto Save Folder"},
                            },
                            {
                                "id": "auto_save_folder_display",
                                "type": "readonly_text",
                                "labelZh": "自定义自动保存路径",
                                "labelEn": "Custom Auto-Save Path",
                                "default": "<Documents>/Adobe/After Effects",
                            },
                        ],
                    },
                ],
            },
            # ==================== PAGE 3: Composition ====================
            {
                "index": 3,
                "id": "composition",
                "labelZh": "合成",
                "labelEn": "Composition",
                "menuCommandId": 24182787,
                "groups": [
                    {
                        "id": "composition_options",
                        "titleZh": "合成选项",
                        "titleEn": "Composition Options",
                        "controls": [
                            {
                                "id": "center_anchor_point_new_shape",
                                "type": "checkbox",
                                "labelZh": "在新形状图层上居中放置锚点",
                                "labelEn": "Center Anchor Point in New Shape Layers",
                                "default": False,
                                "preferencesKey": {"section": "General Section", "key": "Shape Layer Center Anchor Point"},
                            },
                            {
                                "id": "create_layers_at_comp_start",
                                "type": "checkbox",
                                "labelZh": "在合成开始时创建图层",
                                "labelEn": "Create Layers at Composition Start Time",
                                "default": True,
                                "preferencesKey": {"section": "General Section", "key": "Create New Layers At Time Zero"},
                            },
                            {
                                "id": "create_split_layers_above_original",
                                "type": "checkbox",
                                "labelZh": "在原始图层上创建拆分图层",
                                "labelEn": "Create Split Layers Above Original Layer",
                                "default": True,
                                "preferencesKey": {"section": "Split Layer", "key": "Grow Layer Stack Upwards"},
                            },
                            {
                                "id": "default_spatial_interpolation_linear",
                                "type": "checkbox",
                                "labelZh": "默认的空间插值为线性",
                                "labelEn": "Default Spatial Interpolation to Linear",
                                "default": True,
                            },
                            {
                                "id": "default_position_separate_dimensions",
                                "type": "checkbox",
                                "labelZh": "分隔尺寸的默认位置属性",
                                "labelEn": "Default Position Properties to Separated Dimensions",
                                "default": False,
                                "preferencesKey": {"section": "General Section", "key": "Always Separate Dimensions"},
                            },
                            {
                                "id": "pen_shortcut_toggles_mask_feather",
                                "type": "checkbox",
                                "labelZh": "钢笔工具快捷方式在钢笔和蒙版羽化工具之间切换",
                                "labelEn": "Pen Tool Shortcut Toggles Between Pen and Mask Feather Tools",
                                "default": True,
                            },
                            {
                                "id": "preserve_constant_vertex_feather_count",
                                "type": "checkbox",
                                "labelZh": "在编辑蒙版时保持固定的顶点和羽化点数",
                                "labelEn": "Preserve Constant Vertex and Feather Point Count when Editing Masks",
                                "default": True,
                            },
                            {
                                "id": "show_bounding_box_precomp_layers",
                                "type": "checkbox",
                                "labelZh": "显示预合成中图层的定界框",
                                "labelEn": "Show Bounding Box for Layers in Pre-comps",
                                "default": True,
                                "preferencesKey": {"section": "Bounding Box", "key": "Show Sub Layers v2"},
                            },
                            {
                                "id": "switches_affect_nested_comps",
                                "type": "checkbox",
                                "labelZh": "开关影响嵌套的合成",
                                "labelEn": "Switches Affect Nested Comps",
                                "default": True,
                            },
                            {
                                "id": "synchronize_time_all_related_items",
                                "type": "checkbox",
                                "labelZh": "同步所有相关项目的时间",
                                "labelEn": "Synchronize Time of All Related Items",
                                "default": True,
                                "preferencesKey": {"section": "General Section", "key": "Synchronize Time"},
                            },
                            {
                                "id": "use_system_shortcut_keys",
                                "type": "checkbox",
                                "labelZh": "使用系统快捷键",
                                "labelEn": "Use System Shortcut Keys",
                                "default": False,
                                "visibleOnWindows": False,
                                "platformNotes": "Windows 上隐藏（STATE_SYSTEM_INVISIBLE）；仅 macOS 显示",
                            },
                        ],
                    },
                    {
                        "id": "double_click_open_layers",
                        "titleZh": "双击打开图层（使用 Alt 键进行反转）",
                        "titleEn": "Double-Click to Open Layers (Use Alt to Reverse)",
                        "platformNotes": "macOS 提示为 Option 键",
                        "controls": [
                            {
                                "id": "double_click_footage_layer",
                                "type": "dropdown",
                                "labelZh": "在素材图层上打开:",
                                "labelEn": "On a Footage Layer Open:",
                                "default": "layer_panel",
                                "options": [
                                    {"value": "layer_panel", "labelZh": "图层面板", "labelEn": "Layer Panel"},
                                    {"value": "source_footage_panel", "labelZh": "“源素材”面板", "labelEn": "Source Footage Panel"},
                                ],
                            },
                            {
                                "id": "double_click_comp_layer",
                                "type": "dropdown",
                                "labelZh": "在复合图层上打开:",
                                "labelEn": "On a Composition Layer Open:",
                                "default": "source_comp_panel",
                                "options": [
                                    {"value": "layer_panel", "labelZh": "图层面板", "labelEn": "Layer Panel"},
                                    {"value": "source_comp_panel", "labelZh": "“源合成”面板", "labelEn": "Source Composition Panel"},
                                ],
                            },
                            {
                                "id": "double_click_paint_roto_opens_layer",
                                "type": "checkbox",
                                "labelZh": "使用绘图、Roto 笔刷和调整边缘工具双击时将打开“图层”面板",
                                "labelEn": "Double-Clicking with Paint, Roto Brush, and Refine Edge Tools Opens the Layer Panel",
                                "default": True,
                            },
                        ],
                    },
                    {
                        "id": "synthetic_layers",
                        "titleZh": "合成图层",
                        "titleEn": "Synthetic Layers",
                        "controls": [
                            {
                                "id": "synthetic_layers_duration_mode",
                                "type": "radio_group",
                                "labelZh": "合成图层初始长度",
                                "labelEn": "Synthetic Layers Initial Duration",
                                "default": "length_of_composition",
                                "options": [
                                    {"value": "length_of_composition", "labelZh": "合成的长度", "labelEn": "Length of Composition"},
                                    {"value": "fixed_duration", "internalName": "CompositionV2Pref_rbSyntheticLayersDuration", "labelZh": "指定时长", "labelEn": "Fixed Duration"},
                                ],
                                "hasNoteText": True,
                                "noteSummary": "控制新建纯色、文本、摄像机、灯光等合成图层的初始长度",
                            },
                            {
                                "id": "synthetic_layers_duration_timecode",
                                "type": "timecode_input",
                                "labelZh": "指定时间码",
                                "labelEn": "Duration Timecode",
                                "inlineWith": "synthetic_layers_duration_mode:fixed_duration",
                                "default": "0:00:01:00",
                                "timebaseLabelZh": "是 0:00:01:00  基础 30",
                                "timebaseLabelEn": "is 0:00:01:00  base 30",
                            },
                        ],
                    },
                ],
            },
            # ==================== PAGE 4: Previews ====================
            {
                "index": 4,
                "id": "previews",
                "labelZh": "预览",
                "labelEn": "Previews",
                "menuCommandId": 24182788,
                "groups": [
                    {
                        "id": "adaptive_resolution",
                        "titleZh": "自适应分辨率",
                        "titleEn": "Adaptive Resolution",
                        "controls": [
                            {
                                "id": "adaptive_resolution_priority",
                                "type": "slider",
                                "labelZh": "优先考虑：",
                                "labelEn": "Prioritize:",
                                "leftLabelZh": "交互性",
                                "leftLabelEn": "Interactivity",
                                "rightLabelZh": "质量",
                                "rightLabelEn": "Quality",
                                "default": 11,
                                "range": {"min": 0, "max": 18, "note": "AE 26 离散滑块，默认值 11 对应最低 1/6 分辨率与 3D 渲染质量上限 7"},
                                "preferencesKey": {"section": "Realtime Update Preference Section", "key": "Minimum Adaptive Resolution Limit"},
                            },
                            {
                                "id": "adaptive_resolution_3d_renderer",
                                "type": "dropdown",
                                "labelZh": "3D 渲染器可降低：",
                                "labelEn": "3D Renderers Can Reduce:",
                                "default": "resolution_and_quality",
                                "options": [
                                    {"value": "resolution_and_quality", "labelZh": "分辨率和质量", "labelEn": "Resolution & Quality"},
                                    {"value": "render_quality_only", "labelZh": "仅渲染质量", "labelEn": "Render Quality Only"},
                                ],
                                "preferencesKey": {"section": "Realtime Update Preference Section", "key": "Adaptive Resolution Renderer Behavior"},
                            },
                        ],
                    },
                    {
                        "id": "viewer_quality",
                        "titleZh": "查看器质量",
                        "titleEn": "Viewer Quality",
                        "controls": [
                            {
                                "id": "zoom_quality",
                                "type": "dropdown",
                                "labelZh": "缩放质量:",
                                "labelEn": "Zoom Quality:",
                                "default": "more_accurate",
                                "options": [
                                    {"value": "faster", "labelZh": "更快", "labelEn": "Faster"},
                                    {"value": "more_accurate_except_ram_preview", "labelZh": "除缓存预览之外更准确", "labelEn": "More Accurate Except Cached Preview"},
                                    {"value": "more_accurate", "labelZh": "更准确", "labelEn": "More Accurate"},
                                ],
                            },
                            {
                                "id": "color_management_quality",
                                "type": "dropdown",
                                "labelZh": "色彩管理品质:",
                                "labelEn": "Color Management Quality:",
                                "default": "more_accurate_except_ram_preview",
                                "options": [
                                    {"value": "faster", "labelZh": "更快", "labelEn": "Faster"},
                                    {"value": "more_accurate_except_ram_preview", "labelZh": "除缓存预览之外更准确", "labelEn": "More Accurate Except Cached Preview"},
                                    {"value": "more_accurate", "labelZh": "更准确", "labelEn": "More Accurate"},
                                ],
                            },
                        ],
                    },
                    {
                        "id": "composition_switches",
                        "titleZh": "合成开关",
                        "titleEn": "Composition Switches",
                        "controls": [
                            {
                                "id": "auto_enable_frame_blending_motion_blur",
                                "type": "checkbox",
                                "labelZh": "自动启用帧混合和运动模糊渲染",
                                "labelEn": "Automatically Enable Frame Blending and Motion Blur Rendering",
                                "default": True,
                                "preferencesKey": {"section": "Audio Settings Preference Section", "key": "Link Blend and Blur of Layers and Composition"},
                            },
                        ],
                    },
                    {
                        "id": "preview_audio",
                        "titleZh": "音频",
                        "titleEn": "Audio",
                        "controls": [
                            {
                                "id": "mute_audio_when_not_realtime",
                                "type": "checkbox",
                                "labelZh": "非实时预览时将音频静音",
                                "labelEn": "Mute Audio When Preview is Not Realtime",
                                "default": False,
                                "preferencesKey": {"section": "Audio Settings Preference Section", "key": "Mute Audio When Preview Not Realtime v2"},
                            },
                        ],
                    },
                    {
                        "id": "cache_frames_when_idle",
                        "titleZh": "空闲时缓存帧",
                        "titleEn": "Cache Frames When Idle",
                        "controls": [
                            {
                                "id": "idle_delay_before_caching",
                                "internalName": "PreviewsV2Pref_pIdleDelay",
                                "type": "number_input",
                                "labelZh": "缓存开始前的空闲延迟：",
                                "labelEn": "Idle Delay Before Caching Starts:",
                                "default": 8,
                                "unitZh": "秒",
                                "unitEn": "seconds",
                                "range": {"min": 0, "max": 999},
                                "preferencesKey": {"section": "Speculative Preview Preference Section", "key": "Idle Delay Before Caching Starts", "note": "键值以毫秒存储 (8000)"},
                            },
                            {
                                "id": "cache_frames_order",
                                "type": "dropdown",
                                "labelZh": "缓存帧：",
                                "labelEn": "Cache Frames:",
                                "default": "around_current_time",
                                "options": [
                                    {"value": "from_current_time", "labelZh": "从当前时间", "labelEn": "From Current Time"},
                                    {"value": "around_current_time", "labelZh": "围绕当前时间", "labelEn": "Around Current Time"},
                                    {"value": "from_start_of_range", "labelZh": "从范围的起点", "labelEn": "From Start of Range"},
                                ],
                                "preferencesKey": {"section": "Speculative Preview Preference Section", "key": "Caching Behavior"},
                            },
                            {
                                "id": "cache_range",
                                "type": "dropdown",
                                "labelZh": "缓存范围：",
                                "labelEn": "Cache Range:",
                                "default": "work_area_extended_by_current_time",
                                "options": [
                                    {"value": "work_area", "labelZh": "工作区", "labelEn": "Work Area"},
                                    {"value": "work_area_extended_by_current_time", "labelZh": "工作区域按当前时间延伸", "labelEn": "Work Area Extended by Current Time"},
                                    {"value": "entire_duration", "labelZh": "整体持续时间", "labelEn": "Entire Duration"},
                                ],
                                "preferencesKey": {"section": "Speculative Preview Preference Section", "key": "Cache Range"},
                                "hasNoteText": True,
                                "noteSummary": "说明需在合成▸预览菜单启用空闲缓存，且焦点在图层/素材面板时不缓存",
                            },
                        ],
                    },
                    {
                        "id": "magnification",
                        "titleZh": "放大率",
                        "titleEn": "Magnification",
                        "layout": "horizontal_row",
                        "controls": [
                            {
                                "id": "magnification_type",
                                "type": "dropdown",
                                "labelZh": "类型:",
                                "labelEn": "Type:",
                                "default": "smooth",
                                "options": [
                                    {"value": "legacy", "labelZh": "旧版", "labelEn": "Legacy"},
                                    {"value": "smooth", "labelZh": "平滑", "labelEn": "Smooth"},
                                ],
                                "preferencesKey": {"section": "Magnification", "key": "Magnification Using Legacy"},
                            },
                            {
                                "id": "magnification_behavior",
                                "type": "dropdown",
                                "labelZh": "行为:",
                                "labelEn": "Behavior:",
                                "default": "follow_cursor",
                                "options": [
                                    {"value": "centered", "labelZh": "居中", "labelEn": "Centered"},
                                    {"value": "follow_cursor", "labelZh": "跟随光标", "labelEn": "Follow Cursor"},
                                ],
                                "preferencesKey": {"section": "Magnification", "key": "Magnification Behavior (0"},
                            },
                            {
                                "id": "magnification_trackpad_scroll",
                                "type": "dropdown",
                                "labelZh": "触控板滚动:",
                                "labelEn": "Trackpad Scroll:",
                                "default": "pan",
                                "options": [
                                    {"value": "zoom", "labelZh": "缩放", "labelEn": "Zoom"},
                                    {"value": "pan", "labelZh": "平移", "labelEn": "Pan"},
                                ],
                                "preferencesKey": {"section": "Magnification", "key": "Magnification Scrolling (0"},
                            },
                        ],
                    },
                ],
            },
            # ==================== PAGE 5: Appearance ====================
            {
                "index": 5,
                "id": "appearance",
                "labelZh": "外观",
                "labelEn": "Appearance",
                "menuCommandId": 24182790,
                "groups": [
                    {
                        "id": "appearance_options",
                        "titleZh": "外观选项",
                        "titleEn": "Appearance Options",
                        "controls": [
                            {
                                "id": "path_point_and_handle_size",
                                "internalName": "AppearanceV2Pref_htMaskSelectionTolerance",
                                "type": "number_input",
                                "labelZh": "路径点和手柄大小:",
                                "labelEn": "Path Point and Handle Size:",
                                "default": 5,
                                "unitZh": "像素",
                                "unitEn": "pixels",
                                "range": {"min": 4, "max": 15},
                                "preferencesKey": {"section": "General Section", "key": "Path Point and Handle Radius (pixels)"},
                            },
                            {
                                "id": "hardware_accelerate_panels",
                                "type": "checkbox",
                                "labelZh": "硬件加速合成、图层和素材面板",
                                "labelEn": "Hardware Accelerate Composition, Layer, and Footage Panels",
                                "default": True,
                                "preferencesKey": {"section": "Hardware Accelerated Viewer 2", "key": "Enabled"},
                            },
                            {
                                "id": "use_system_color_picker",
                                "type": "checkbox",
                                "labelZh": "使用系统拾色器",
                                "labelEn": "Use System Color Picker",
                                "default": False,
                            },
                            {
                                "id": "show_rendering_progress_in_info_flowchart",
                                "type": "checkbox",
                                "labelZh": "在信息面板和流程图中显示渲染进度",
                                "labelEn": "Show Rendering Progress in Info Panel and Flowchart",
                                "default": False,
                                "preferencesKey": {"section": "Main Pref Section v2", "key": "Pref_SHOW_INFO_FLOATER_PROGRESS"},
                            },
                            {
                                "id": "disable_thumbnails_in_project_panel",
                                "type": "checkbox",
                                "labelZh": "在项目面板中禁用缩览图",
                                "labelEn": "Disable Thumbnails in Project Panel",
                                "default": False,
                                "preferencesKey": {"section": "Main Pref Section v2", "key": "Pref_RENDER_THUMBNAILS", "note": "键值 1 表示启用缩览图（与复选框取反）"},
                            },
                            {
                                "id": "show_both_timecode_and_frames_in_timeline",
                                "type": "checkbox",
                                "labelZh": "在时间轴面板中同时显示时间码和帧",
                                "labelEn": "Show Both Timecode and Frames in Timeline Panel",
                                "default": True,
                                "preferencesKey": {"section": "Prefs.Display", "key": "Show Alternate Time Display"},
                            },
                            {
                                "id": "show_tool_tips",
                                "type": "checkbox",
                                "labelZh": "显示工具提示",
                                "labelEn": "Show Tool Tips",
                                "default": True,
                            },
                            {
                                "id": "show_coachmarks",
                                "type": "checkbox",
                                "labelZh": "显示引导标记",
                                "labelEn": "Show Coachmarks",
                                "default": False,
                                "preferencesKey": {"section": "Coachmarks Settings", "key": "Enabled"},
                            },
                            {
                                "id": "reset_coachmarks",
                                "type": "button",
                                "labelZh": "重置引导提示标记",
                                "labelEn": "Reset Coachmarks",
                                "inlineWith": "show_coachmarks",
                            },
                        ],
                    },
                    {
                        "id": "theme",
                        "titleZh": "主题",
                        "titleEn": "Theme",
                        "controls": [
                            {
                                "id": "color_theme",
                                "internalName": "UI_ThemePicker",
                                "type": "theme_picker",
                                "labelZh": "颜色主题",
                                "labelEn": "Color Theme",
                                "default": "darkest",
                                "options": [
                                    {"value": "darkest", "labelZh": "最暗", "labelEn": "Darkest"},
                                    {"value": "dark", "labelZh": "深色", "labelEn": "Dark"},
                                    {"value": "light", "labelZh": "灯光", "labelEn": "Light"},
                                ],
                                "note": "3 个缩略图卡片单选（AE 26 取代旧版亮度连续滑块；zh_CN 将 Light 译作“灯光”）",
                                "preferencesKey": {"section": "Main Pref Section v2", "key": "User Interface Brightness (4) [0.0..1.0]"},
                            },
                            {
                                "id": "accessibility_color_contrast",
                                "type": "checkbox",
                                "labelZh": "无障碍色彩对比度",
                                "labelEn": "Accessibility Color Contrast",
                                "default": True,
                            },
                        ],
                    },
                    {
                        "id": "colors",
                        "titleZh": "颜色",
                        "titleEn": "Colors",
                        "controls": [
                            {
                                "id": "use_label_color_for_handles_and_paths",
                                "type": "checkbox",
                                "labelZh": "对图层手柄和路径使用标签颜色",
                                "labelEn": "Use Label Color for Layer Handles and Paths",
                                "default": True,
                            },
                            {
                                "id": "use_label_color_for_related_tabs",
                                "type": "checkbox",
                                "labelZh": "对相关选项卡使用标签颜色",
                                "labelEn": "Use Label Color for Related Tabs",
                                "default": True,
                            },
                            {
                                "id": "cycle_mask_colors",
                                "type": "checkbox",
                                "labelZh": "循环蒙版颜色（使用标签颜色）",
                                "labelEn": "Cycle Mask Colors (Use Label Colors)",
                                "default": True,
                                "controlsEnablementOf": ["use_contrasting_color_for_mask_path"],
                                "preferencesKey": {"section": "Mask Colors", "key": "Cycle Mask Label Colors"},
                            },
                            {
                                "id": "use_contrasting_color_for_mask_path",
                                "type": "checkbox",
                                "labelZh": "为蒙版路径使用对比度颜色",
                                "labelEn": "Use Contrasting Color for Mask Path",
                                "indented": True,
                                "default": True,
                                "enabledWhen": {"control": "cycle_mask_colors", "equals": True},
                                "preferencesKey": {"section": "Mask Colors", "key": "Use Contrasting Color For Mask Path Enabled"},
                            },
                            {
                                "id": "use_gradients",
                                "type": "checkbox",
                                "labelZh": "使用渐变色",
                                "labelEn": "Use Gradients",
                                "default": True,
                            },
                        ],
                    },
                    {
                        "id": "motion_path",
                        "titleZh": "运动路径",
                        "titleEn": "Motion Path",
                        "controls": [
                            {
                                "id": "motion_path_mode",
                                "type": "radio_group",
                                "labelZh": "运动路径显示方式",
                                "labelEn": "Motion Path Display Mode",
                                "default": "time_window",
                                "options": [
                                    {"value": "no_motion_path", "labelZh": "没有运动路径", "labelEn": "No Motion Path"},
                                    {"value": "all_keyframes", "labelZh": "所有关键帧", "labelEn": "All Keyframes"},
                                    {"value": "max_keyframes", "labelZh": "不超过", "labelEn": "No More Than (Keyframes)"},
                                    {"value": "time_window", "labelZh": "指定时长", "labelEn": "No More Than (Time)"},
                                ],
                            },
                            {
                                "id": "motion_path_max_keyframes",
                                "internalName": "AppearanceV2Pref_htNumKeyframes",
                                "type": "number_input",
                                "labelZh": "最大关键帧数",
                                "labelEn": "Max Keyframes",
                                "inlineWith": "motion_path_mode:max_keyframes",
                                "default": 5,
                                "unitZh": "个关键帧",
                                "unitEn": "Keyframes",
                                "range": {"min": 1, "max": 999},
                            },
                            {
                                "id": "motion_path_time_window",
                                "type": "timecode_input",
                                "labelZh": "运动路径时长",
                                "labelEn": "Motion Path Duration",
                                "inlineWith": "motion_path_mode:time_window",
                                "default": "0:00:15:00",
                                "timebaseLabelZh": "是 0:00:15:00  基础 30",
                                "timebaseLabelEn": "is 0:00:15:00  base 30",
                            },
                        ],
                    },
                ],
            },
            # ==================== PAGE 6: Grids & Guides ====================
            {
                "index": 6,
                "id": "grids_guides",
                "labelZh": "网格和参考线",
                "labelEn": "Grids & Guides",
                "menuCommandId": 24182791,
                "groups": [
                    {
                        "id": "grid",
                        "titleZh": "网格",
                        "titleEn": "Grid",
                        "layout": "two_column_grid",
                        "controls": [
                            {
                                "id": "grid_color",
                                "internalName": "GridsGuidesPref_CPControlWithLabel1_colorButton",
                                "type": "color_swatch",
                                "labelZh": "颜色:",
                                "labelEn": "Color:",
                                "hasEyedropper": True,
                                "defaultColorHex": "#008000",
                                "preferencesKey": {"section": "Main Pref Section v2", "key": "Pref_GRID_COLOR"},
                            },
                            {
                                "id": "grid_style",
                                "type": "dropdown",
                                "labelZh": "样式:",
                                "labelEn": "Style:",
                                "default": "lines",
                                "options": [
                                    {"value": "lines", "labelZh": "线条", "labelEn": "Lines"},
                                    {"value": "dashed_lines", "labelZh": "虚线", "labelEn": "Dashed Lines"},
                                    {"value": "dots", "labelZh": "点", "labelEn": "Dots"},
                                ],
                                "preferencesKey": {"section": "Main Pref Section v2", "key": "Pref_GRID_STYLE"},
                            },
                            {
                                "id": "gridline_every",
                                "internalName": "GridsGuidesPref_htGridSpacing",
                                "type": "number_input",
                                "labelZh": "网格线间隔:",
                                "labelEn": "Gridline Every:",
                                "default": 80,
                                "unitZh": "像素",
                                "unitEn": "pixels",
                                "range": {"min": 1, "max": 9999},
                                "preferencesKey": {"section": "Main Pref Section v2", "key": "Pref_GRID_PIX_PER_DIV"},
                            },
                            {
                                "id": "grid_subdivisions",
                                "internalName": "GridsGuidesPref_htGridSubdivision",
                                "type": "number_input",
                                "labelZh": "次分隔线:",
                                "labelEn": "Subdivisions:",
                                "default": 4,
                                "range": {"min": 1, "max": 99},
                                "preferencesKey": {"section": "Main Pref Section v2", "key": "Pref_GRID_SUB_PER_DIV"},
                            },
                        ],
                    },
                    {
                        "id": "proportional_grid",
                        "titleZh": "对称网格",
                        "titleEn": "Proportional Grid",
                        "layout": "horizontal_row",
                        "controls": [
                            {
                                "id": "proportional_grid_horizontal",
                                "internalName": "GridsGuidesPref_htGridHorz",
                                "type": "number_input",
                                "labelZh": "水平:",
                                "labelEn": "Horizontal:",
                                "default": 8,
                                "range": {"min": 1, "max": 99},
                                "preferencesKey": {"section": "Main Pref Section v2", "key": "Pref_ANIMGRID_CELLS_HORIZ"},
                            },
                            {
                                "id": "proportional_grid_vertical",
                                "internalName": "GridsGuidesPref_htGridVert",
                                "type": "number_input",
                                "labelZh": "垂直:",
                                "labelEn": "Vertical:",
                                "default": 6,
                                "range": {"min": 1, "max": 99},
                                "preferencesKey": {"section": "Main Pref Section v2", "key": "Pref_ANIMGRID_CELLS_VERT v2"},
                            },
                        ],
                    },
                    {
                        "id": "guides",
                        "titleZh": "参考线",
                        "titleEn": "Guides",
                        "layout": "horizontal_row",
                        "controls": [
                            {
                                "id": "guides_color",
                                "internalName": "GridsGuidesPref_CPControlWithLabel2_colorButton",
                                "type": "color_swatch",
                                "labelZh": "颜色:",
                                "labelEn": "Color:",
                                "hasEyedropper": True,
                                "defaultColorHex": "#20E0FF",
                                "preferencesKey": {"section": "Main Pref Section v2", "key": "Pref_GUIDE_COLOR"},
                            },
                            {
                                "id": "guides_style",
                                "type": "dropdown",
                                "labelZh": "样式:",
                                "labelEn": "Style:",
                                "default": "lines",
                                "options": [
                                    {"value": "lines", "labelZh": "线条", "labelEn": "Lines"},
                                    {"value": "dashed_lines", "labelZh": "虚线", "labelEn": "Dashed Lines"},
                                ],
                                "preferencesKey": {"section": "Main Pref Section v2", "key": "Pref_GUIDE_STYLE"},
                            },
                        ],
                    },
                    {
                        "id": "safe_margins",
                        "titleZh": "安全边距",
                        "titleEn": "Safe Margins",
                        "layout": "two_column_grid",
                        "controls": [
                            {
                                "id": "action_safe",
                                "internalName": "GridsGuidesPref_htActionSafe",
                                "type": "number_input",
                                "labelZh": "动作安全:",
                                "labelEn": "Action-safe:",
                                "default": 10,
                                "unitZh": "%",
                                "unitEn": "%",
                                "range": {"min": 0, "max": 100},
                                "preferencesKey": {"section": "Grids And Guides v2", "key": "Action Safe Margin % (0-100)"},
                            },
                            {
                                "id": "title_safe",
                                "internalName": "GridsGuidesPref_htTitleSafe",
                                "type": "number_input",
                                "labelZh": "字幕安全:",
                                "labelEn": "Title-safe:",
                                "default": 20,
                                "unitZh": "%",
                                "unitEn": "%",
                                "range": {"min": 0, "max": 100},
                                "preferencesKey": {"section": "Grids And Guides v2", "key": "Title Safe Margin % (0-100)"},
                            },
                            {
                                "id": "center_cut_action_safe",
                                "internalName": "GridsGuidesPref_htActionSafeSD",
                                "type": "number_input",
                                "labelZh": "中心剪切动作安全:",
                                "labelEn": "Center-Cut Action-safe:",
                                "default": 32.5,
                                "unitZh": "%",
                                "unitEn": "%",
                                "range": {"min": 0, "max": 100},
                                "preferencesKey": {"section": "Grids And Guides v2", "key": "4:3 Action Safe Margin % (0-100)"},
                            },
                            {
                                "id": "center_cut_title_safe",
                                "internalName": "GridsGuidesPref_htTitleSafeSD",
                                "type": "number_input",
                                "labelZh": "中心剪切字幕安全:",
                                "labelEn": "Center-Cut Title-safe:",
                                "default": 40,
                                "unitZh": "%",
                                "unitEn": "%",
                                "range": {"min": 0, "max": 100},
                                "hasNoteText": True,
                                "noteSummary": "说明中心剪切安全区用于 16:9 查看器预览 4:3 裁切范围",
                                "preferencesKey": {"section": "Grids And Guides v2", "key": "4:3 Title Safe Margin % (0-100)"},
                            },
                        ],
                    },
                    {
                        "id": "transparency_settings",
                        "titleZh": "透明度设置",
                        "titleEn": "Transparency Settings",
                        "controls": [
                            {
                                "id": "transparency_grid_size",
                                "type": "dropdown",
                                "labelZh": "网格大小:",
                                "labelEn": "Grid Size:",
                                "default": "medium",
                                "options": [
                                    {"value": "small", "labelZh": "小", "labelEn": "Small"},
                                    {"value": "medium", "labelZh": "中", "labelEn": "Medium"},
                                    {"value": "large", "labelZh": "大", "labelEn": "Large"},
                                ],
                                "preferencesKey": {"section": "General Section", "key": "Transparency Grid Size"},
                            },
                            {
                                "id": "transparency_grid_colors_preset",
                                "type": "dropdown",
                                "labelZh": "网格颜色：",
                                "labelEn": "Grid Colors:",
                                "default": "custom",
                                "options": [
                                    {"value": "custom", "labelZh": "自定义...", "labelEn": "Custom..."},
                                    {"value": "light", "labelZh": "灯光", "labelEn": "Light"},
                                    {"value": "medium", "labelZh": "中", "labelEn": "Medium"},
                                    {"value": "dark", "labelZh": "深色", "labelEn": "Dark"},
                                    {"value": "red", "labelZh": "红色", "labelEn": "Red"},
                                    {"value": "orange", "labelZh": "橙色", "labelEn": "Orange"},
                                    {"value": "green", "labelZh": "绿色", "labelEn": "Green"},
                                    {"value": "blue", "labelZh": "蓝色", "labelEn": "Blue"},
                                    {"value": "purple", "labelZh": "紫色", "labelEn": "Purple"},
                                ],
                                "preferencesKey": {"section": "Main Pref Section v2", "key": "Pref_TRANSPARENCY_GRID_COLOR_POPUP_INDEX"},
                            },
                            {
                                "id": "transparency_grid_color_1",
                                "internalName": "GridsGuidesPref_ColorPickerControl1_colorButton",
                                "type": "color_swatch",
                                "labelZh": "棋盘格色 1",
                                "labelEn": "Grid Color 1",
                                "hasEyedropper": True,
                                "defaultColorHex": "#CCCCCC",
                                "preferencesKey": {"section": "Main Pref Section v2", "key": "Pref_TRANSPARENCY_GRID_COLOR1"},
                            },
                            {
                                "id": "transparency_grid_color_2",
                                "internalName": "GridsGuidesPref_ColorPickerControl2_colorButton",
                                "type": "color_swatch",
                                "labelZh": "棋盘格色 2",
                                "labelEn": "Grid Color 2",
                                "hasEyedropper": True,
                                "inlineWith": "transparency_grid_color_1",
                                "defaultColorHex": "#FFFFFF",
                                "preferencesKey": {"section": "Main Pref Section v2", "key": "Pref_TRANSPARENCY_GRID_COLOR2 v2"},
                            },
                        ],
                    },
                ],
            },
            # ==================== PAGE 7: Labels ====================
            {
                "index": 7,
                "id": "labels",
                "labelZh": "标签",
                "labelEn": "Labels",
                "menuCommandId": 24182792,
                "sharedDropdownOptions": {
                    "labelPalette17": [
                        {"value": 0, "labelZh": "无", "labelEn": "None"},
                        {"value": 1, "labelZh": "红色", "labelEn": "Red", "colorHex": "#B53838"},
                        {"value": 2, "labelZh": "黄色", "labelEn": "Yellow", "colorHex": "#E4D84C"},
                        {"value": 3, "labelZh": "浅绿色", "labelEn": "Aqua", "colorHex": "#A9CBC7"},
                        {"value": 4, "labelZh": "粉色", "labelEn": "Pink", "colorHex": "#E5BCC9"},
                        {"value": 5, "labelZh": "淡紫色", "labelEn": "Lavender", "colorHex": "#A9A9CA"},
                        {"value": 6, "labelZh": "桃红色", "labelEn": "Peach", "colorHex": "#E7C19E"},
                        {"value": 7, "labelZh": "海泡沫", "labelEn": "Sea Foam", "colorHex": "#B3C7B3"},
                        {"value": 8, "labelZh": "蓝色", "labelEn": "Blue", "colorHex": "#677DE0"},
                        {"value": 9, "labelZh": "绿色", "labelEn": "Green", "colorHex": "#4AA44C"},
                        {"value": 10, "labelZh": "紫色", "labelEn": "Purple", "colorHex": "#8E2C9A"},
                        {"value": 11, "labelZh": "橙色", "labelEn": "Orange", "colorHex": "#E8920D"},
                        {"value": 12, "labelZh": "棕色", "labelEn": "Brown", "colorHex": "#7F452A"},
                        {"value": 13, "labelZh": "紫红色", "labelEn": "Fuchsia", "colorHex": "#F46DD6"},
                        {"value": 14, "labelZh": "青色", "labelEn": "Cyan", "colorHex": "#3DA2A5"},
                        {"value": 15, "labelZh": "砂岩", "labelEn": "Sandstone", "colorHex": "#A89677"},
                        {"value": 16, "labelZh": "深绿色", "labelEn": "Dark Green", "colorHex": "#1E401E"},
                    ]
                },
                "groups": [
                    {
                        "id": "label_defaults",
                        "titleZh": "标签默认值",
                        "titleEn": "Label Defaults",
                        "layout": "two_column_grid",
                        "note": "全部 16 个下拉框共用 17 项标签色板（无 + 16 色）；左列 7 项 + 右列 7 项 + 底部嵌套“标记”子分组 2 项",
                        "controls": [
                            {"id": "label_default_comp", "type": "dropdown", "column": "left", "labelZh": "合成:", "labelEn": "Composition:", "default": 15, "defaultLabelZh": "砂岩", "defaultLabelEn": "Sandstone", "optionsRef": "labelPalette17", "preferencesKey": {"section": "Label Preference Indices Section 5", "key": "Comp Label Index 2"}},
                            {"id": "label_default_video", "type": "dropdown", "column": "left", "labelZh": "视频:", "labelEn": "Video:", "default": 3, "defaultLabelZh": "浅绿色", "defaultLabelEn": "Aqua", "optionsRef": "labelPalette17", "preferencesKey": {"section": "Label Preference Indices Section 5", "key": "Video Label Index 2"}},
                            {"id": "label_default_audio", "type": "dropdown", "column": "left", "labelZh": "音频:", "labelEn": "Audio:", "default": 7, "defaultLabelZh": "海泡沫", "defaultLabelEn": "Sea Foam", "optionsRef": "labelPalette17", "preferencesKey": {"section": "Label Preference Indices Section 5", "key": "Audio Label Index 2"}},
                            {"id": "label_default_still", "type": "dropdown", "column": "left", "labelZh": "静止图像:", "labelEn": "Still:", "default": 5, "defaultLabelZh": "淡紫色", "defaultLabelEn": "Lavender", "optionsRef": "labelPalette17", "preferencesKey": {"section": "Label Preference Indices Section 5", "key": "Still Label Index 2"}},
                            {"id": "label_default_folder", "type": "dropdown", "column": "left", "labelZh": "文件夹:", "labelEn": "Folder:", "default": 2, "defaultLabelZh": "黄色", "defaultLabelEn": "Yellow", "optionsRef": "labelPalette17", "preferencesKey": {"section": "Label Preference Indices Section 5", "key": "Folder Label Index 2"}},
                            {"id": "label_default_null", "type": "dropdown", "column": "left", "labelZh": "空对象:", "labelEn": "Null:", "default": 1, "defaultLabelZh": "红色", "defaultLabelEn": "Red", "optionsRef": "labelPalette17", "preferencesKey": {"section": "Label Preference Indices Section 5", "key": "Null Label Index"}},
                            {"id": "label_default_3d_model", "type": "dropdown", "column": "left", "labelZh": "3D 模型:", "labelEn": "3D Model:", "default": 14, "defaultLabelZh": "青色", "defaultLabelEn": "Cyan", "optionsRef": "labelPalette17", "preferencesKey": {"section": "Label Preference Indices Section 5", "key": "3D Model Label Index"}},
                            {"id": "label_default_solid", "type": "dropdown", "column": "right", "labelZh": "纯色:", "labelEn": "Solid:", "default": 1, "defaultLabelZh": "红色", "defaultLabelEn": "Red", "optionsRef": "labelPalette17", "preferencesKey": {"section": "Label Preference Indices Section 5", "key": "Solid Label Index 2"}},
                            {"id": "label_default_camera", "type": "dropdown", "column": "right", "labelZh": "摄像机:", "labelEn": "Camera:", "default": 4, "defaultLabelZh": "粉色", "defaultLabelEn": "Pink", "optionsRef": "labelPalette17", "preferencesKey": {"section": "Label Preference Indices Section 5", "key": "Camera Label Index 2"}},
                            {"id": "label_default_light", "type": "dropdown", "column": "right", "labelZh": "灯光:", "labelEn": "Light:", "default": 6, "defaultLabelZh": "桃红色", "defaultLabelEn": "Peach", "optionsRef": "labelPalette17", "preferencesKey": {"section": "Label Preference Indices Section 5", "key": "Light Label Index 2"}},
                            {"id": "label_default_shape", "type": "dropdown", "column": "right", "labelZh": "形状:", "labelEn": "Shape:", "default": 8, "defaultLabelZh": "蓝色", "defaultLabelEn": "Blue", "optionsRef": "labelPalette17", "preferencesKey": {"section": "Label Preference Indices Section 5", "key": "Shape Label Index 2"}},
                            {"id": "label_default_adjustment", "type": "dropdown", "column": "right", "labelZh": "调整:", "labelEn": "Adjustment:", "default": 5, "defaultLabelZh": "淡紫色", "defaultLabelEn": "Lavender", "optionsRef": "labelPalette17", "preferencesKey": {"section": "Label Preference Indices Section 5", "key": "Adjustment Label Index 2"}},
                            {"id": "label_default_text", "type": "dropdown", "column": "right", "labelZh": "文本:", "labelEn": "Text:", "default": 1, "defaultLabelZh": "红色", "defaultLabelEn": "Red", "optionsRef": "labelPalette17", "preferencesKey": {"section": "Label Preference Indices Section 5", "key": "Text Label Index"}},
                            {"id": "label_default_other_3d", "type": "dropdown", "column": "right", "labelZh": "其他 3D:", "labelEn": "Other 3D:", "default": 9, "defaultLabelZh": "绿色", "defaultLabelEn": "Green", "optionsRef": "labelPalette17", "preferencesKey": {"section": "Label Preference Indices Section 5", "key": "Materials Label Index"}},
                            {"id": "label_default_marker_comment", "type": "dropdown", "subgroupZh": "标记", "subgroupEn": "Markers", "column": "left", "labelZh": "注释:", "labelEn": "Comment:", "default": 0, "defaultLabelZh": "无", "defaultLabelEn": "None", "optionsRef": "labelPalette17", "preferencesKey": {"section": "Label Preference Indices Section 5", "key": "Comment Label Index"}},
                            {"id": "label_default_marker_segmentation", "type": "dropdown", "subgroupZh": "标记", "subgroupEn": "Markers", "column": "right", "labelZh": "片段:", "labelEn": "Segmentation:", "default": 5, "defaultLabelZh": "淡紫色", "defaultLabelEn": "Lavender", "optionsRef": "labelPalette17", "preferencesKey": {"section": "Label Preference Indices Section 5", "key": "Segmentation Label Index"}},
                        ],
                    },
                    {
                        "id": "label_colors",
                        "titleZh": "标签颜色",
                        "titleEn": "Label Colors",
                        "layout": "two_column_grid",
                        "note": "左列 #1-#8、右列 #9-#16；每项由 [名称输入框 + 色块按钮 + 吸管按钮] 组成",
                        "controls": [
                            {"id": "label_color_1", "type": "label_color_entry", "index": 1, "column": "left", "labelZh": "红色", "labelEn": "Red", "defaultColorHex": "#B53838", "hasEyedropper": True, "preferencesKey": {"section": "Label Preference Color Section 5", "key": "Label Color ID 2 # 1"}},
                            {"id": "label_color_2", "type": "label_color_entry", "index": 2, "column": "left", "labelZh": "黄色", "labelEn": "Yellow", "defaultColorHex": "#E4D84C", "hasEyedropper": True, "preferencesKey": {"section": "Label Preference Color Section 5", "key": "Label Color ID 2 # 2"}},
                            {"id": "label_color_3", "type": "label_color_entry", "index": 3, "column": "left", "labelZh": "浅绿色", "labelEn": "Aqua", "defaultColorHex": "#A9CBC7", "hasEyedropper": True, "preferencesKey": {"section": "Label Preference Color Section 5", "key": "Label Color ID 2 # 3"}},
                            {"id": "label_color_4", "type": "label_color_entry", "index": 4, "column": "left", "labelZh": "粉色", "labelEn": "Pink", "defaultColorHex": "#E5BCC9", "hasEyedropper": True, "preferencesKey": {"section": "Label Preference Color Section 5", "key": "Label Color ID 2 # 4"}},
                            {"id": "label_color_5", "type": "label_color_entry", "index": 5, "column": "left", "labelZh": "淡紫色", "labelEn": "Lavender", "defaultColorHex": "#A9A9CA", "hasEyedropper": True, "preferencesKey": {"section": "Label Preference Color Section 5", "key": "Label Color ID 2 # 5"}},
                            {"id": "label_color_6", "type": "label_color_entry", "index": 6, "column": "left", "labelZh": "桃红色", "labelEn": "Peach", "defaultColorHex": "#E7C19E", "hasEyedropper": True, "preferencesKey": {"section": "Label Preference Color Section 5", "key": "Label Color ID 2 # 6"}},
                            {"id": "label_color_7", "type": "label_color_entry", "index": 7, "column": "left", "labelZh": "海泡沫", "labelEn": "Sea Foam", "defaultColorHex": "#B3C7B3", "hasEyedropper": True, "preferencesKey": {"section": "Label Preference Color Section 5", "key": "Label Color ID 2 # 7"}},
                            {"id": "label_color_8", "type": "label_color_entry", "index": 8, "column": "left", "labelZh": "蓝色", "labelEn": "Blue", "defaultColorHex": "#677DE0", "hasEyedropper": True, "preferencesKey": {"section": "Label Preference Color Section 5", "key": "Label Color ID 2 # 8"}},
                            {"id": "label_color_9", "type": "label_color_entry", "index": 9, "column": "right", "labelZh": "绿色", "labelEn": "Green", "defaultColorHex": "#4AA44C", "hasEyedropper": True, "preferencesKey": {"section": "Label Preference Color Section 5", "key": "Label Color ID 2 # 9"}},
                            {"id": "label_color_10", "type": "label_color_entry", "index": 10, "column": "right", "labelZh": "紫色", "labelEn": "Purple", "defaultColorHex": "#8E2C9A", "hasEyedropper": True, "preferencesKey": {"section": "Label Preference Color Section 5", "key": "Label Color ID 2 # 10"}},
                            {"id": "label_color_11", "type": "label_color_entry", "index": 11, "column": "right", "labelZh": "橙色", "labelEn": "Orange", "defaultColorHex": "#E8920D", "hasEyedropper": True, "preferencesKey": {"section": "Label Preference Color Section 5", "key": "Label Color ID 2 # 11"}},
                            {"id": "label_color_12", "type": "label_color_entry", "index": 12, "column": "right", "labelZh": "棕色", "labelEn": "Brown", "defaultColorHex": "#7F452A", "hasEyedropper": True, "preferencesKey": {"section": "Label Preference Color Section 5", "key": "Label Color ID 2 # 12"}},
                            {"id": "label_color_13", "type": "label_color_entry", "index": 13, "column": "right", "labelZh": "紫红色", "labelEn": "Fuchsia", "defaultColorHex": "#F46DD6", "hasEyedropper": True, "preferencesKey": {"section": "Label Preference Color Section 5", "key": "Label Color ID 2 # 13"}},
                            {"id": "label_color_14", "type": "label_color_entry", "index": 14, "column": "right", "labelZh": "青色", "labelEn": "Cyan", "defaultColorHex": "#3DA2A5", "hasEyedropper": True, "preferencesKey": {"section": "Label Preference Color Section 5", "key": "Label Color ID 2 # 14"}},
                            {"id": "label_color_15", "type": "label_color_entry", "index": 15, "column": "right", "labelZh": "砂岩", "labelEn": "Sandstone", "defaultColorHex": "#A89677", "hasEyedropper": True, "preferencesKey": {"section": "Label Preference Color Section 5", "key": "Label Color ID 2 # 15"}},
                            {"id": "label_color_16", "type": "label_color_entry", "index": 16, "column": "right", "labelZh": "深绿色", "labelEn": "Dark Green", "defaultColorHex": "#1E401E", "hasEyedropper": True, "preferencesKey": {"section": "Label Preference Color Section 5", "key": "Label Color ID 2 # 16"}},
                        ],
                    },
                ],
            },
            # ==================== PAGE 8: Type ====================
            {
                "index": 8,
                "id": "type",
                "labelZh": "类型",
                "labelEn": "Type",
                "menuCommandId": 24182793,
                "groups": [
                    {
                        "id": "font_menu",
                        "titleZh": "字体菜单",
                        "titleEn": "Font Menu",
                        "controls": [
                            {
                                "id": "font_preview_size",
                                "type": "dropdown",
                                "labelZh": "预览大小",
                                "labelEn": "Preview Size",
                                "default": "small",
                                "options": [
                                    {"value": "small", "labelZh": "小", "labelEn": "Small"},
                                    {"value": "medium", "labelZh": "中", "labelEn": "Medium"},
                                    {"value": "large", "labelZh": "大", "labelEn": "Large"},
                                    {"value": "extra_large", "labelZh": "特大", "labelEn": "Extra Large"},
                                ],
                                "preferencesKey": {"section": "General Section", "key": "Font Panel Preview Size"},
                            },
                            {
                                "id": "recent_fonts_count",
                                "type": "dropdown",
                                "labelZh": "要显示的近期字体数量",
                                "labelEn": "Number of Recent Fonts to Display",
                                "default": 14,
                                "options": [{"value": n, "labelZh": str(n), "labelEn": str(n)} for n in range(16)],
                                "preferencesKey": {"section": "General Section", "key": "Font Panel MRU Size"},
                            },
                        ],
                    },
                    {
                        "id": "resolve_fonts_dialog",
                        "titleZh": "“解析字体”对话框",
                        "titleEn": "Resolve Fonts Dialog",
                        "controls": [
                            {
                                "id": "do_not_warn_missing_fonts",
                                "type": "checkbox",
                                "labelZh": "打开项目时，不要提醒我缺失字体。",
                                "labelEn": "Do not warn me about missing fonts when opening a project.",
                                "default": False,
                                "preferencesKey": {"section": "General Section", "key": "Hide Missing Font Dialog"},
                            },
                        ],
                    },
                ],
            },
        ],
    }


def fmt_default(ctrl: dict[str, Any]) -> str:
    if "defaultColorHex" in ctrl:
        return f"`{ctrl['defaultColorHex']}`"
    if "defaultLabelZh" in ctrl and "defaultLabelEn" in ctrl:
        return f"{ctrl['defaultLabelZh']} (`{ctrl['defaultLabelEn']}`, `{ctrl['default']}`)"
    if "defaultZh" in ctrl and "defaultEn" in ctrl:
        return f"`{ctrl['defaultZh']}` / `{ctrl['defaultEn']}`"
    if "default" not in ctrl:
        return "—"
    v = ctrl["default"]
    if isinstance(v, bool):
        return "勾选" if v else "不勾选"
    if "options" in ctrl:
        for opt in ctrl["options"]:
            if opt.get("value") == v:
                return f"{opt['labelZh']} (`{opt['labelEn']}`)"
    unit = ctrl.get("unitZh", "")
    s = f"`{v}`"
    if unit:
        s += f" {unit}"
    return s


def fmt_options_or_range(ctrl: dict[str, Any]) -> str:
    parts: list[str] = []
    if "optionsRef" in ctrl:
        parts.append(f"引用 `{ctrl['optionsRef']}`（无 + 16 色）")
    elif "options" in ctrl:
        opts = [f"{o['labelZh']} ({o['labelEn']})" for o in ctrl["options"]]
        if len(opts) > 10:
            parts.append(f"共 {len(opts)} 项：" + "、".join(opts[:6]) + " 等（完整列表见 JSON）")
        else:
            parts.append(" / ".join(opts))
    if "range" in ctrl:
        r = ctrl["range"]
        if "min" in r and "max" in r:
            u = f" {ctrl['unitZh']}" if ctrl.get("unitZh") else ""
            parts.append(f"范围 `{r['min']}..{r['max']}`{u}")
        if r.get("note"):
            parts.append(r["note"])
    if "enabledWhen" in ctrl:
        ew = ctrl["enabledWhen"]
        parts.append(f"启用条件：`{ew['control']} == {ew['equals']}`")
    if ctrl.get("visibleOnWindows") is False:
        parts.append("Windows 隐藏（macOS 专用）")
    if ctrl.get("defaultEnabled") is False and "enabledWhen" not in ctrl:
        parts.append("默认置灰（依硬件/缓存状态）")
    if ctrl.get("platformNotes"):
        parts.append(ctrl["platformNotes"])
    if ctrl.get("note"):
        parts.append(ctrl["note"])
    return "；".join(parts) if parts else "—"


def fmt_pref_key(ctrl: dict[str, Any]) -> str:
    pk = ctrl.get("preferencesKey")
    if not pk:
        return "—"
    return f"`[{pk['section']}] {pk['key']}`"


def render_markdown(data: dict[str, Any]) -> str:
    lines: list[str] = []
    meta = data["meta"]
    dlg = data["dialog"]
    va = data["verificationAnswers"]

    lines.append("# After Effects 26.0 设置（首选项）对话框参考（Windows）")
    lines.append("")
    lines.append(
        f"> 来源：{meta['application']} (`{meta['build']}`, `{meta['localeObserved']}`) | "
        f"平台：{meta['platform']} | DPI：{meta['dpiObserved']} ({int(meta['dpiScale']*100)}%) | "
        f"提取日期：{meta['extractedDate']}"
    )
    lines.append("")
    lines.append(
        "本文档由 `tools/ae/reference/build_preferences_dialog_doc.py` 与 `preferences-dialog.json` 同步生成，"
        "记录 Windows 版 AE 26.0 首选项对话框的整体框架、16 页目录、全部控件结构、默认值、下拉项、范围/单位、联动置灰关系与 `preferences.json` 键映射。"
        "不包含 Adobe 帮助/提示/报错原文、截图或本机路径。"
    )
    lines.append("")

    # Section 1: Verification answers for D (#161)
    lines.append("## 1. 关键核对结论（供 #161 对照）")
    lines.append("")
    lines.append(f"- **对话框尺寸与模态**：固定大小模态框（`#32770`），96 DPI 逻辑尺寸 `{dlg['sizeLogical96Dpi']['w']} × {dlg['sizeLogical96Dpi']['h']}` px（150% DPI 物理尺寸 `{dlg['sizePhysical144Dpi']['w']} × {dlg['sizePhysical144Dpi']['h']}` px）。")
    lines.append(f"- **按钮位置与顺序**：{va['buttonLayout']}")
    undo = va["defaultsCheck"]["levelsOfUndo"]
    as_int = va["defaultsCheck"]["autoSaveIntervalMinutes"]
    as_ver = va["defaultsCheck"]["maxProjectVersions"]
    lines.append(
        f"- **默认值核对**："
        f"**自动保存间隔** = `{as_int['default']} 分钟`（第 2 页「项目」▸「自动保存」）；"
        f"**最大项目版本** = `{as_ver['default']}`（第 2 页「项目」▸「自动保存」）；"
        f"**撤消级别** = `{undo['backendDefault']}`（{undo['note']} 对应 `[{undo['preferencesKey']['section']}] {undo['preferencesKey']['key']}`）。"
    )
    lines.append("- **侧边栏 16 页顺序**：左侧列表框为平铺 16 项（列表框内无分割线；「编辑 ▸ 首选项」子菜单中按 4 / 4 / 2 / 4 / 2 分 5 组）。")
    lines.append("")

    # Section 2: Sidebar & Buttons
    lines.append("## 2. 对话框框架与 16 页目录")
    lines.append("")
    lines.append("### 2.1 右上角纵向按钮列")
    lines.append("")
    lines.append("| 顺序 | ID | 中文 | 英文 | 角色 / 快捷键 | 行为 |")
    lines.append("|---:|---|---|---|---|---|")
    for idx, b in enumerate(dlg["buttons"], 1):
        sc = b.get("shortcut", "—")
        beh = "循环切换（首页/末页均不置灰）" if b.get("wrapsAround") else "提交/放弃全部修改"
        lines.append(f"| {idx} | `{b['id']}` | {b['labelZh']} | {b['labelEn']} | `{b['role']}` / `{sc}` | {beh} |")
    lines.append("")

    lines.append("### 2.2 左侧 16 页顺序与菜单 Command ID")
    lines.append("")
    lines.append("| 序号 | 页面 ID | 中文 | 英文 | 菜单分组 | Win32 Command ID |")
    lines.append("|---:|---|---|---|---:|---:|")
    grp_map = {}
    for g in dlg["sidebarGroups"]:
        for pid in g["pageIds"]:
            grp_map[pid] = g["groupIndex"]
    for s in dlg["sidebar"]:
        sc = f" (`{s['shortcut']}`)" if s.get("shortcut") else ""
        lines.append(
            f"| {s['index']} | `{s['id']}` | {s['labelZh']} | {s['labelEn']}{sc} | 第 {grp_map[s['id']]} 组 | `{s['menuCommandId']}` |"
        )
    lines.append("")

    # Section 3: Legacy 19 -> AE 26 16 migration
    lines.append("## 3. 旧版 19 页 → AE 26（16 页）迁移对照表")
    lines.append("")
    lines.append("| 旧版 AE 页面 | AE 26 落点页面 ID | 变更说明 |")
    lines.append("|---|---|---|")
    for m in data["legacy19ToAe26Migration"]:
        targets = ", ".join(f"`{p}`" for p in m["ae26Pages"]) if m["ae26Pages"] else "（已移除）"
        lines.append(f"| {m['legacyZh']} | {targets} | {m['notes']} |")
    lines.append("")

    # Section 4: Per-page specs
    lines.append("## 4. 逐页控件明细")
    lines.append("")
    for page in data["pages"]:
        lines.append(f"### 4.{page['index']} 第 {page['index']} 页：{page['labelZh']} ({page['labelEn']}) — `{page['id']}`")
        lines.append("")
        for grp in page["groups"]:
            title_note = f"（{grp['note']}）" if grp.get("note") else ""
            lines.append(f"#### {grp['titleZh']} / {grp['titleEn']}{title_note}")
            lines.append("")
            lines.append("| 控件 ID | 类型 | 中文标签 | 英文标签 | 默认值 | 选项 / 范围 / 联动与平台说明 | `preferences.json` 键 |")
            lines.append("|---|---|---|---|---|---|---|")
            for ctrl in grp["controls"]:
                lines.append(
                    f"| `{ctrl['id']}` | `{ctrl['type']}` | {ctrl.get('labelZh', '') or '—'} | "
                    f"{ctrl.get('labelEn', '') or '—'} | {fmt_default(ctrl)} | "
                    f"{fmt_options_or_range(ctrl)} | {fmt_pref_key(ctrl)} |"
                )
            lines.append("")
        if page.get("footerControls"):
            lines.append("**页脚控件：**")
            for fc in page["footerControls"]:
                lines.append(f"- `{fc['id']}` (`{fc['type']}`): {fc['labelZh']} / {fc['labelEn']}（{fc.get('noteSummary', '')}）")
            lines.append("")

    return "\n".join(lines).rstrip() + "\n"


def main() -> None:
    data = build_data()
    OUT_JSON.parent.mkdir(parents=True, exist_ok=True)
    OUT_JSON.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    OUT_MD.write_text(render_markdown(data), encoding="utf-8")
    print(f"Wrote {OUT_JSON} ({OUT_JSON.stat().st_size} bytes, {len(data['pages'])} pages)")
    print(f"Wrote {OUT_MD} ({OUT_MD.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
