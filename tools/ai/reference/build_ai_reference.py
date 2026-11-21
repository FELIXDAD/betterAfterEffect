"""Build structured Illustrator reference files under docs/reference/ai/ from raw extracted data."""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import xml.etree.ElementTree as ET
from datetime import date
from typing import Any


KYS_SPECIAL_KEYS = {
    9: "Tab",
    13: "Enter",
    14: "F1",
    15: "F2",
    16: "F3",
    17: "F4",
    18: "F5",
    19: "F6",
    20: "F7",
    21: "F8",
    22: "F9",
    23: "F10",
    24: "F11",
    25: "F12",
    27: "Esc",
    28: "Left",
    29: "Right",
    30: "Up",
    31: "Down",
    32: "Space",
    127: "Delete",
}


def decode_kys_key(key_code: int, rep_code: int, modifiers: int) -> str | None:
    """Decode Illustrator KYS modifier bitmask + key/represent code into a readable shortcut string."""
    if key_code == 0 and rep_code == 0:
        return None
    parts: list[str] = []
    if modifiers & 64:
        parts.append("Ctrl")
    if modifiers & 128:
        parts.append("Alt")
    if modifiers & 32:
        parts.append("Shift")

    code = rep_code if rep_code != 0 else key_code
    if code in KYS_SPECIAL_KEYS:
        key_name = KYS_SPECIAL_KEYS[code]
    elif 33 <= code <= 126:
        ch = chr(code)
        # Uppercase single ASCII letters for standard shortcut presentation.
        key_name = ch.upper() if "a" <= ch <= "z" else ch
    else:
        key_name = f"Key({code})"
    parts.append(key_name)
    return "+".join(parts)


def parse_kys_file(kys_path: pathlib.Path) -> dict[str, list[dict[str, Any]]]:
    """Parse an Illustrator .kys shortcut file into structured menu and tool shortcut lists."""
    text = kys_path.read_text(encoding="utf-8")
    sections: dict[str, list[dict[str, Any]]] = {"menus": [], "tools": []}
    for sec_key, out_key in (("Menus", "menus"), ("Tools", "tools")):
        m = re.search(rf"/{sec_key}\s*\{{(.*?)\n\}}", text, re.S)
        if not m:
            continue
        entries: list[dict[str, Any]] = []
        for em in re.finditer(r"\t/([^\n{]+?)\s*\{([^}]*)\}", m.group(1), re.S):
            cmd_id = em.group(1).strip().replace("\\ ", " ")
            fields: dict[str, int] = {}
            for fm in re.finditer(r"/(\w+)\s+(-?\d+)", em.group(2)):
                fields[fm.group(1)] = int(fm.group(2))
            ctx = fields.get("Context", 0)
            mod = fields.get("Modifiers", 0)
            rep = fields.get("Represent", 0)
            key = fields.get("Key", 0)
            shortcut = decode_kys_key(key, rep, mod)
            entries.append(
                {
                    "id": cmd_id,
                    "context": "text" if ctx == 1 else "global",
                    "shortcut": shortcut,
                    "assigned": shortcut is not None,
                }
            )
        # Preserve deterministic ordering: assigned first or alphabetical by id.
        entries.sort(key=lambda item: item["id"].lower())
        sections[out_key] = entries
    return sections


def parse_tool_presets(preset_path: pathlib.Path) -> dict[str, Any]:
    """Parse Illustrator Tool Panel Presets into structured toolbar collections and flyout groups."""
    text = preset_path.read_text(encoding="utf-8")
    collections: dict[str, Any] = {}
    for m in re.finditer(r"/collection(\d+)\s*\{(.*?)\n\}", text, re.S):
        body = m.group(2)
        nm = re.search(r"/collectionName\s*\[\s*\d+\s*([0-9a-fA-F\s]+)\]", body)
        cname = (
            bytes.fromhex(re.sub(r"\s+", "", nm.group(1))).decode("utf-8", errors="replace")
            if nm
            else f"collection{m.group(1)}"
        )

        # Collect flyout sets first: CustomToolboxItemSet<N>_<M> (Tool Name)
        sets: dict[str, list[str]] = {}
        for sm in re.finditer(r"/CustomToolboxItemSet(\d+)_(\d+)\s*\(([^)]+)\)", body):
            set_id = sm.group(1)
            sets.setdefault(set_id, []).append(sm.group(3))

        slots: list[dict[str, Any]] = []
        current_group: str | None = None
        for line in body.splitlines():
            s = line.strip()
            gm = re.match(r"/ToolGroup\d+\s*\(([^)]+)\)", s)
            if gm:
                current_group = gm.group(1)
                continue
            im = re.match(r"/CustomToolboxItem(\d+)\s*\(([^)]+)\)", s)
            if im:
                val = im.group(2)
                set_ref = re.match(r"CustomToolboxItemSet(\d+)_Count_\d+", val)
                if set_ref:
                    tools = sets.get(set_ref.group(1), [])
                else:
                    tools = [val]
                slot: dict[str, Any] = {"slot": int(im.group(1)), "tools": tools}
                if current_group:
                    slot["group"] = current_group
                slots.append(slot)
        collections[cname] = {
            "slot_count": len(slots),
            "tool_count": sum(len(sl["tools"]) for sl in slots),
            "slots": slots,
        }
    return collections


def scan_preset_libraries(install_dir: pathlib.Path, locale: str) -> dict[str, Any]:
    """List names and relative categories of built-in preset libraries (names only, no contents)."""
    presets_dir = install_dir / "Presets" / locale
    profiles_dir = install_dir / "Support Files" / "Required" / "New Document Profiles" / locale

    def list_tree(root: pathlib.Path) -> list[str]:
        if not root.exists():
            return []
        names: list[str] = []
        for p in sorted(root.rglob("*")):
            if p.is_file():
                rel = p.relative_to(root)
                # Strip file extension for clean preset library names, keep folder hierarchy.
                if rel.parent == pathlib.Path("."):
                    names.append(rel.stem)
                else:
                    names.append(f"{rel.parent.as_posix()}/{rel.stem}")
        return names

    # Support both zh_CN and en_US folder names gracefully.
    mapping = {
        "swatches": ["色板", "Swatches"],
        "brushes": ["画笔", "Brushes"],
        "graphic_styles": ["图形样式", "Graphic Styles"],
        "symbols": ["符号", "Symbols"],
        "workspaces": ["工作区", "Workspaces"],
        "scripts": ["脚本", "Scripts"],
        "actions": ["动作", "Actions"],
        "save_for_web": ["存储为 Web 所用格式设置", "Save for Web Settings"],
    }
    libraries: dict[str, list[str]] = {}
    for key, candidates in mapping.items():
        found: list[str] = []
        for cand in candidates:
            sub = presets_dir / cand
            if sub.exists():
                found = list_tree(sub)
                break
        libraries[key] = found

    doc_profiles = list_tree(profiles_dir)
    return {
        "document_profiles": doc_profiles,
        "preset_libraries": libraries,
    }


def parse_omv_enums(omv_path: pathlib.Path, filter_names: set[str] | None = None) -> dict[str, list[str]]:
    """Extract only enum class names and member identifiers from Illustrator's installed OMV XML (no prose)."""
    if not omv_path.exists():
        return {}
    ns = "{http://schema.adobe.com/omv/1.0/omv.xsd}"
    tree = ET.parse(omv_path)
    enums: dict[str, list[str]] = {}
    for c in tree.getroot().iter(f"{ns}classdef"):
        if c.attrib.get("enumeration") == "true":
            name = c.attrib.get("name", "")
            if not name or (filter_names is not None and name not in filter_names):
                continue
            members = [p.attrib["name"] for p in c.iter(f"{ns}property") if "name" in p.attrib]
            if members:
                enums[name] = members
    return dict(sorted(enums.items()))


def normalize_and_sanitize_menus(nodes: list[dict[str, Any]], parent_label: str = "") -> list[dict[str, Any]]:
    """Normalize Win32 shortcut delimiters (\x08 / \t), strip mnemonic suffixes, and omit local user-specific items."""
    out: list[dict[str, Any]] = []
    for node in nodes:
        t = node.get("type")
        if t == "separator":
            out.append({"type": "separator"})
            continue

        raw_label = str(node.get("label") or "")
        shortcut = node.get("shortcut")
        # Win32 menus often use ASCII BS (\x08) or Tab (\t) before the right-aligned shortcut hint.
        for delim in ("\x08", "\t"):
            if delim in raw_label:
                left, right = raw_label.split(delim, 1)
                raw_label = left
                if not shortcut:
                    shortcut = right.strip() or None
                break

        raw_label = raw_label.strip()
        mnemonic = None
        # Strip CJK Windows accelerator suffix such as "新建(N)..." or "图层(L)".
        mm = re.search(r"\(([A-Z0-9])\)(\.{3}|…)?$", raw_label)
        if mm:
            mnemonic = mm.group(1)
            ellipsis = mm.group(2) or ""
            raw_label = raw_label[: mm.start()].rstrip() + ellipsis

        entry: dict[str, Any] = {
            "type": t,
            "label": raw_label,
        }
        if mnemonic:
            entry["mnemonic"] = mnemonic
        if shortcut:
            entry["shortcut"] = shortcut
        if t == "item" and node.get("commandId"):
            entry["commandId"] = node["commandId"]

        if t == "submenu":
            # Sanitize local / machine-specific dynamic submenus.
            omitted_reason = None
            if raw_label in ("最近打开的文件", "Open Recent Files"):
                omitted_reason = "local_recent_files"
            elif raw_label in ("最近使用的字体", "Recent Fonts"):
                omitted_reason = "local_recent_fonts"
            elif raw_label in ("字体", "Font") and parent_label in ("文字", "Type"):
                omitted_reason = "installed_fonts"
            elif raw_label in ("扩展功能", "Extensions") and parent_label in ("窗口", "Window"):
                omitted_reason = "local_installed_extensions"
            elif raw_label in ("用户定义", "User Defined"):
                omitted_reason = "user_defined_library"

            if omitted_reason:
                entry["omitted_dynamic"] = omitted_reason
                entry["children"] = []
            else:
                entry["children"] = normalize_and_sanitize_menus(node.get("children", []), raw_label)
        out.append(entry)
    return out


def count_menu_nodes(nodes: list[dict[str, Any]]) -> dict[str, int]:
    """Count top-level menus, submenus, leaf items, items with shortcuts, and separators."""
    stats = {"submenus": 0, "items": 0, "with_shortcut": 0, "separators": 0}

    def walk(items: list[dict[str, Any]]) -> None:
        for it in items:
            t = it.get("type")
            if t == "separator":
                stats["separators"] += 1
            elif t == "submenu":
                stats["submenus"] += 1
                walk(it.get("children", []))
            else:
                stats["items"] += 1
                if it.get("shortcut"):
                    stats["with_shortcut"] += 1

    walk(nodes)
    stats["top_level"] = len(nodes)
    return stats


def escape_md(text: str | None) -> str:
    if not text:
        return ""
    return str(text).replace("|", "\\|").replace("\n", " ").strip()


def format_sample(val: Any) -> str:
    if val is None:
        return "null"
    if isinstance(val, bool):
        return "true" if val else "false"
    if isinstance(val, (int, float)):
        return str(val)
    if isinstance(val, list):
        return json.dumps(val, ensure_ascii=False)
    return str(val)


def render_menu_markdown(nodes: list[dict[str, Any]], source_line: str) -> str:
    """Render full menu tree into readable Markdown."""
    lines = [
        "# Adobe Illustrator 菜单树",
        "",
        source_line,
        "",
    ]
    stats = count_menu_nodes(nodes)
    lines.append(
        f"共 {stats['top_level']} 个顶层菜单，{stats['submenus']} 个子菜单节点，"
        f"{stats['items']} 个菜单命令（其中 {stats['with_shortcut']} 个带有快捷键提示）。"
    )
    lines.append("")

    def walk(items: list[dict[str, Any]], depth: int) -> None:
        for it in items:
            t = it.get("type")
            if t == "separator":
                lines.append(f"{'  ' * depth}- ---")
                continue
            label = escape_md(it.get("label", ""))
            sc = it.get("shortcut")
            sc_str = f" (`{sc}`)" if sc else ""
            if t == "submenu":
                children = it.get("children", [])
                omitted = it.get("omitted_dynamic")
                note = f" *(动态项省略: `{omitted}`)*" if omitted else ""
                lines.append(f"{'  ' * depth}- **{label}**{sc_str}{note}")
                if children:
                    walk(children, depth + 1)
            else:
                lines.append(f"{'  ' * depth}- {label}{sc_str}")

    for top in nodes:
        label = escape_md(top.get("label", ""))
        children = top.get("children", [])
        cstats = count_menu_nodes(children)
        lines.append(f"## {label}")
        lines.append("")
        lines.append(f"子项统计：{cstats['items']} 个命令，{cstats['submenus']} 个子菜单。")
        lines.append("")
        walk(children, 0)
        lines.append("")

    return "\n".join(lines).rstrip() + "\n"


def render_effects_markdown(menus: list[dict[str, Any]], source_line: str) -> tuple[str, dict[str, int]]:
    """Extract Effect (效果) menu into structured Illustrator Effects and Photoshop Effects Markdown."""
    effect_menu = None
    for top in menus:
        lbl = top.get("label", "")
        if lbl in ("效果", "Effect") or lbl.startswith("效果") or lbl.startswith("Effect"):
            effect_menu = top
            break

    lines = [
        "# Adobe Illustrator 效果菜单参考",
        "",
        source_line,
        "",
    ]
    counts = {"illustrator_groups": 0, "illustrator_effects": 0, "photoshop_groups": 0, "photoshop_effects": 0, "other_items": 0}
    if not effect_menu:
        lines.append("未在菜单树中检测到「效果 / Effect」顶层菜单。")
        return "\n".join(lines) + "\n", counts

    children = effect_menu.get("children", [])
    # Split sections by header items / separators:
    # Typically: top commands (Apply Last Effect, Effect) -> Illustrator Effects -> Photoshop Effects.
    sections: list[tuple[str, list[dict[str, Any]]]] = []
    current_title = "顶部命令与通用项"
    current_items: list[dict[str, Any]] = []

    for item in children:
        lbl = item.get("label", "")
        t = item.get("type")
        if t == "separator":
            continue
        if ("Illustrator 效果" in lbl or "Illustrator Effects" in lbl) and t != "submenu":
            if current_items:
                sections.append((current_title, current_items))
            current_title = "Illustrator 效果"
            current_items = []
            continue
        if ("Photoshop 效果" in lbl or "Photoshop Effects" in lbl) and t != "submenu":
            if current_items:
                sections.append((current_title, current_items))
            current_title = "Photoshop 效果"
            current_items = []
            continue
        current_items.append(item)
    if current_items:
        sections.append((current_title, current_items))

    def render_effect_list(items: list[dict[str, Any]], section_kind: str) -> None:
        for it in items:
            if it.get("type") == "separator":
                continue
            lbl = escape_md(it.get("label", ""))
            sc = f" (`{it['shortcut']}`)" if it.get("shortcut") else ""
            if it.get("type") == "submenu":
                if section_kind == "ai":
                    counts["illustrator_groups"] += 1
                elif section_kind == "ps":
                    counts["photoshop_groups"] += 1
                sub_items = [c for c in it.get("children", []) if c.get("type") != "separator"]
                if lines and lines[-1] != "":
                    lines.append("")
                lines.append(f"### {lbl}")
                lines.append("")
                for sub in sub_items:
                    slbl = escape_md(sub.get("label", ""))
                    if sub.get("type") == "submenu":
                        lines.append(f"- **{slbl}**")
                        for leaf in sub.get("children", []):
                            if leaf.get("type") == "separator":
                                continue
                            lines.append(f"  - {escape_md(leaf.get('label', ''))}")
                            if section_kind == "ai":
                                counts["illustrator_effects"] += 1
                            elif section_kind == "ps":
                                counts["photoshop_effects"] += 1
                            else:
                                counts["other_items"] += 1
                    else:
                        lines.append(f"- {slbl}")
                        if section_kind == "ai":
                            counts["illustrator_effects"] += 1
                        elif section_kind == "ps":
                            counts["photoshop_effects"] += 1
                        else:
                            counts["other_items"] += 1
                lines.append("")
            else:
                lines.append(f"- {lbl}{sc}")
                if section_kind == "ai":
                    counts["illustrator_effects"] += 1
                elif section_kind == "ps":
                    counts["photoshop_effects"] += 1
                else:
                    counts["other_items"] += 1

    # Summary placeholder replaced after counting
    summary_idx = len(lines)
    lines.append("")
    lines.append("")

    for sec_title, sec_items in sections:
        lines.append(f"## {sec_title}")
        lines.append("")
        kind = "ai" if "Illustrator" in sec_title else ("ps" if "Photoshop" in sec_title else "other")
        render_effect_list(sec_items, kind)
        if lines[-1] != "":
            lines.append("")

    lines[summary_idx] = (
        f"共提取：Illustrator 效果 {counts['illustrator_effects']} 项（{counts['illustrator_groups']} 组），"
        f"Photoshop 效果 {counts['photoshop_effects']} 项（{counts['photoshop_groups']} 组），"
        f"其他菜单项 {counts['other_items']} 项。"
    )
    return "\n".join(lines).rstrip() + "\n", counts


def render_shortcuts_markdown(kys_data: dict[str, list[dict[str, Any]]], source_line: str) -> str:
    """Render parsed KYS shortcuts into Markdown tables."""
    menu_entries = kys_data["menus"]
    tool_entries = kys_data["tools"]
    menu_assigned = [e for e in menu_entries if e["assigned"]]
    tool_assigned = [e for e in tool_entries if e["assigned"]]

    lines = [
        "# Adobe Illustrator 默认键盘快捷键",
        "",
        source_line,
        "",
        f"解析自默认快捷键集（`.kys`）：菜单命令共 {len(menu_entries)} 条（已分配默认快捷键 {len(menu_assigned)} 条），"
        f"工具及交互动作共 {len(tool_entries)} 条（已分配默认快捷键 {len(tool_assigned)} 条）。"
        f"完整列表（含未分配默认键位的命令 ID）见 `shortcuts.json`。",
        "",
        "## 工具快捷键（已分配）",
        "",
        "| 工具 / 动作 ID | 默认快捷键 | 上下文 |",
        "|---|---|---|",
    ]
    for e in tool_assigned:
        lines.append(f"| `{escape_md(e['id'])}` | `{escape_md(e['shortcut'])}` | {e['context']} |")

    lines.extend(
        [
            "",
            "## 菜单命令快捷键（已分配）",
            "",
            "| 命令 ID | 默认快捷键 | 上下文 |",
            "|---|---|---|",
        ]
    )
    for e in menu_assigned:
        lines.append(f"| `{escape_md(e['id'])}` | `{escape_md(e['shortcut'])}` | {e['context']} |")

    return "\n".join(lines).rstrip() + "\n"


def render_panels_tools_markdown(
    menus: list[dict[str, Any]],
    tool_presets: dict[str, Any],
    kys_data: dict[str, list[dict[str, Any]]],
    source_line: str,
) -> tuple[str, dict[str, int]]:
    """Render tool list (from tool presets + KYS) and panel list (from Window menu)."""
    shortcut_by_tool = {e["id"]: e["shortcut"] for e in kys_data["tools"] if e["shortcut"]}

    # Extract Window (窗口) menu items
    window_menu = None
    for top in menus:
        lbl = top.get("label", "")
        if lbl in ("窗口", "Window") or lbl.startswith("窗口") or lbl.startswith("Window"):
            window_menu = top
            break

    default_tb = tool_presets.get("Default", {})
    basic_tb = tool_presets.get("Basic", {})
    gs_tb = tool_presets.get("Getting Started", {})

    basic_tool_set = {t for sl in basic_tb.get("slots", []) for t in sl.get("tools", [])}
    default_tool_set = {t for sl in default_tb.get("slots", []) for t in sl.get("tools", [])}

    extra_kys_tools = [e for e in kys_data["tools"] if e["id"] not in default_tool_set]

    lines = [
        "# Adobe Illustrator 工具与面板列表",
        "",
        source_line,
        "",
        "## 工具列表（高级工具栏 Default 预设）",
        "",
        f"高级工具栏共 {default_tb.get('slot_count', 0)} 个槽位 / {default_tb.get('tool_count', 0)} 个工具；"
        f"基本工具栏（Basic）包含 {basic_tb.get('tool_count', 0)} 个工具；"
        f"快速入门（Getting Started）包含 {gs_tb.get('tool_count', 0)} 个工具。",
        "",
        "| 槽位 | 内部工具 ID | 默认快捷键 | Basic 包含 |",
        "|---:|---|---|---|",
    ]

    for sl in default_tb.get("slots", []):
        slot_idx = sl["slot"]
        for t_idx, tool_id in enumerate(sl["tools"]):
            sc = shortcut_by_tool.get(tool_id)
            sc_col = f"`{escape_md(sc)}`" if sc else "—"
            in_basic = "是" if tool_id in basic_tool_set else "否"
            slot_col = f"{slot_idx}" if t_idx == 0 else f"{slot_idx}.{t_idx}"
            lines.append(f"| {slot_col} | `{escape_md(tool_id)}` | {sc_col} | {in_basic} |")

    lines.extend(
        [
            "",
            "### 快速入门工具栏功能分组（Getting Started）",
            "",
            "| 分组 | 工具列表 |",
            "|---|---|",
        ]
    )
    gs_by_group: dict[str, list[str]] = {}
    for sl in gs_tb.get("slots", []):
        g = sl.get("group", "OTHER")
        gs_by_group.setdefault(g, []).extend(sl.get("tools", []))
    for g, tools in gs_by_group.items():
        tools_str = ", ".join(f"`{escape_md(t)}`" for t in tools)
        lines.append(f"| `{escape_md(g)}` | {tools_str} |")

    lines.extend(
        [
            "",
            "### 快捷键集中额外的工具与交互动作",
            "",
            f"除上述工具栏槽位外，`.kys` `/Tools` 节还定义了 {len(extra_kys_tools)} 项交互/模式/画笔/不透明度/混合模式动作（下表列出已分配快捷键的项及未分配项统计）：",
            "",
            "| 动作 ID | 默认快捷键 |",
            "|---|---|",
        ]
    )
    assigned_extra = [e for e in extra_kys_tools if e["assigned"]]
    unassigned_extra = [e for e in extra_kys_tools if not e["assigned"]]
    for e in assigned_extra:
        lines.append(f"| `{escape_md(e['id'])}` | `{escape_md(e['shortcut'])}` |")
    if unassigned_extra:
        lines.append("")
        lines.append(
            f"未分配默认快捷键的 `/Tools` 动作（{len(unassigned_extra)} 项，完整见 `shortcuts.json`）："
            + ", ".join(f"`{escape_md(e['id'])}`" for e in unassigned_extra)
            + "。"
        )

    # Panels from Window menu
    lines.extend(["", "## 面板与窗口菜单项（来自「窗口」菜单）", ""])
    panel_count = 0
    window_submenus = 0
    if not window_menu:
        lines.append("未在菜单树中检测到「窗口 / Window」顶层菜单。")
    else:
        lines.extend(
            [
                "| 菜单项 | 类型 | 快捷键 | 子项摘要 |",
                "|---|---|---|---|",
            ]
        )
        for item in window_menu.get("children", []):
            if item.get("type") == "separator":
                lines.append("| --- | 分隔线 | | |")
                continue
            lbl = escape_md(item.get("label", ""))
            sc = f"`{escape_md(item['shortcut'])}`" if item.get("shortcut") else "—"
            if item.get("type") == "submenu":
                window_submenus += 1
                if item.get("omitted_dynamic"):
                    preview = f"*(动态本地项 `{item['omitted_dynamic']}` 已省略)*"
                else:
                    sub_labels = [
                        escape_md(c.get("label", ""))
                        for c in item.get("children", [])
                        if c.get("type") != "separator"
                    ]
                    preview = "、".join(sub_labels[:12]) + (f" 等共 {len(sub_labels)} 项" if len(sub_labels) > 12 else "")
                lines.append(f"| **{lbl}** | 子菜单 | {sc} | {preview} |")
            else:
                panel_count += 1
                lines.append(f"| {lbl} | 面板/命令 | {sc} | |")

    counts = {
        "default_toolbar_slots": default_tb.get("slot_count", 0),
        "default_toolbar_tools": default_tb.get("tool_count", 0),
        "basic_toolbar_tools": basic_tb.get("tool_count", 0),
        "extra_kys_tool_actions": len(extra_kys_tools),
        "window_menu_items": panel_count,
        "window_menu_submenus": window_submenus,
    }
    return "\n".join(lines).rstrip() + "\n", counts


def render_dom_markdown(dom_data: dict[str, Any], source_line: str) -> str:
    """Render live-inspected DOM object schemas into Markdown tables."""
    lines = [
        "# Adobe Illustrator 文档对象模型（DOM）参考",
        "",
        source_line,
        "",
        f"通过 ExtendScript 在临时文档中实际创建各对象并反射读取其属性名、访问权限、声明类型、运行时类型与示例值。共覆盖 {len(dom_data)} 个对象实例/类型。",
        "",
        "## 索引",
        "",
        "| 键名 | ExtendScript `typename` | 属性数 | 方法数 |",
        "|---|---|---:|---:|",
    ]
    for key, obj in dom_data.items():
        lines.append(
            f"| `{escape_md(key)}` | `{escape_md(obj.get('typename', ''))}` | "
            f"{len(obj.get('properties', []))} | {len(obj.get('methods', []))} |"
        )
    lines.append("")

    for key, obj in dom_data.items():
        label = obj.get("label", key)
        typename = obj.get("typename", "")
        props = obj.get("properties", [])
        methods = obj.get("methods", [])
        lines.append(f"## {escape_md(label)} (`{escape_md(typename)}`)")
        lines.append("")
        lines.append("| 属性名 | 读写 | 声明类型 | 运行时类型 | 示例值 |")
        lines.append("|---|---|---|---|---|")
        for p in props:
            sample = p.get("sample")
            if p.get("runtimeType") == "unreadable":
                sample_str = f"*(不可读: {escape_md(p.get('readError', ''))})*"
            else:
                sample_str = f"`{escape_md(format_sample(sample))}`"
            lines.append(
                f"| `{escape_md(p['name'])}` | {escape_md(p.get('access', ''))} | "
                f"`{escape_md(p.get('declaredType', ''))}` | `{escape_md(p.get('runtimeType', ''))}` | {sample_str} |"
            )
        if methods:
            lines.append("")
            lines.append("方法：" + ", ".join(f"`{escape_md(m)}()`" for m in methods))
        lines.append("")

    return "\n".join(lines).rstrip() + "\n"


def write_json(path: pathlib.Path, data: Any) -> int:
    text = json.dumps(data, ensure_ascii=False, indent=2) + "\n"
    path.write_text(text, encoding="utf-8")
    return path.stat().st_size


def main() -> None:
    parser = argparse.ArgumentParser(description="Build Illustrator reference files under docs/reference/ai/.")
    parser.add_argument("--install-dir", default=r"C:\Program Files\Adobe\Adobe Illustrator 2026")
    parser.add_argument(
        "--omv-xml",
        default=r"C:\Program Files (x86)\Common Files\Adobe\Scripting Dictionaries CC\Illustrator 2026\omv.xml",
    )
    parser.add_argument("--menus-raw", required=True)
    parser.add_argument("--dom-raw", required=True)
    parser.add_argument("--output-dir", required=True)
    parser.add_argument("--extracted-date", default=date.today().isoformat())
    args = parser.parse_args()

    install_dir = pathlib.Path(args.install_dir)
    menus_raw = json.loads(pathlib.Path(args.menus_raw).read_text(encoding="utf-8"))
    dom_raw = json.loads(pathlib.Path(args.dom_raw).read_text(encoding="utf-8"))
    out_dir = pathlib.Path(args.output_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    ai_meta = dom_raw.get("meta", {})
    win_meta = menus_raw.get("windows", {})
    exe_meta = menus_raw.get("executable", {}) or {}
    locale = ai_meta.get("locale") or "zh_CN"

    version = ai_meta.get("version") or exe_meta.get("productVersion") or "unknown"
    build_number = ai_meta.get("buildNumber") or exe_meta.get("fileVersion") or ""
    app_name = exe_meta.get("productName") or ai_meta.get("appName") or "Adobe Illustrator"
    platform_str = (
        f"{win_meta.get('caption', 'Windows')} "
        f"({win_meta.get('version', '')}, Build {win_meta.get('buildNumber', '')}, {win_meta.get('architecture', '')})"
    )

    source_obj = {
        "application": app_name,
        "version": version,
        "build_number": build_number,
        "locale": locale,
        "platform": platform_str,
        "extracted_date": args.extracted_date,
    }
    source_line = (
        f"> 来源：{app_name} ({version}"
        + (f", build {build_number}" if build_number else "")
        + f") | 语言：{locale} | 平台：{win_meta.get('caption', 'Windows')} Build {win_meta.get('buildNumber', '')} | 提取日期：{args.extracted_date}"
    )

    # 1. Menus
    menus = menus_raw.get("menus", [])
    menu_method = menus_raw.get("method", "unknown")
    if not menus and menus_raw.get("uiaMenuBars"):
        # Pick the UIA menu bar with the most items.
        best_bar = max(menus_raw["uiaMenuBars"], key=lambda b: len(b.get("items", [])))
        menus = best_bar.get("items", [])
    menus = normalize_and_sanitize_menus(menus)

    menu_stats = count_menu_nodes(menus)
    write_json(
        out_dir / "menus.json",
        {
            "source": source_obj,
            "extraction_method": menu_method,
            "stats": menu_stats,
            "menus": menus,
        },
    )
    (out_dir / "menus.md").write_text(render_menu_markdown(menus, source_line), encoding="utf-8")

    # 2. Shortcuts (.kys)
    kys_candidates = list((install_dir / "Presets" / locale).rglob("*.kys"))
    if not kys_candidates:
        raise FileNotFoundError(f"No .kys file found under {install_dir / 'Presets' / locale}")
    kys_path = kys_candidates[0]
    kys_data = parse_kys_file(kys_path)
    shortcut_stats = {
        "menu_commands_total": len(kys_data["menus"]),
        "menu_commands_assigned": sum(1 for e in kys_data["menus"] if e["assigned"]),
        "tool_actions_total": len(kys_data["tools"]),
        "tool_actions_assigned": sum(1 for e in kys_data["tools"] if e["assigned"]),
    }
    write_json(
        out_dir / "shortcuts.json",
        {
            "source": source_obj,
            "preset_name": kys_path.stem,
            "stats": shortcut_stats,
            "menus": kys_data["menus"],
            "tools": kys_data["tools"],
        },
    )
    (out_dir / "shortcuts.md").write_text(render_shortcuts_markdown(kys_data, source_line), encoding="utf-8")

    # 3. Tool presets + Panels/Tools Markdown
    tool_preset_candidates = list((install_dir / "Presets" / locale).rglob("*工具面板预设*"))
    if not tool_preset_candidates:
        tool_preset_candidates = list((install_dir / "Presets" / locale).rglob("*Tool*Preset*"))
    tool_presets = parse_tool_presets(tool_preset_candidates[0]) if tool_preset_candidates else {}
    panels_tools_md, panels_tools_stats = render_panels_tools_markdown(menus, tool_presets, kys_data, source_line)
    (out_dir / "panels-tools.md").write_text(panels_tools_md, encoding="utf-8")

    # 4. Effects Markdown
    effects_md, effect_stats = render_effects_markdown(menus, source_line)
    (out_dir / "effects.md").write_text(effects_md, encoding="utf-8")

    # 5. Catalogs JSON
    fs_catalogs = scan_preset_libraries(install_dir, locale)
    script_catalogs = dom_raw.get("catalogs", {})
    enums_merged = dict(script_catalogs.get("enums", {}))
    omv_enums = parse_omv_enums(pathlib.Path(args.omv_xml))
    for k, v in omv_enums.items():
        if k not in enums_merged or not enums_merged[k]:
            enums_merged[k] = v
    enums_merged = dict(sorted(enums_merged.items()))

    catalogs_payload = {
        "source": source_obj,
        "available_font_count": ai_meta.get("availableFontCount", -1),
        "document_profiles": fs_catalogs["document_profiles"],
        "preset_libraries": fs_catalogs["preset_libraries"],
        "app_presets": script_catalogs.get("presets", {}),
        "enums": enums_merged,
        "save_options": script_catalogs.get("saveOptions", {}),
        "export_options": script_catalogs.get("exportOptions", {}),
    }
    write_json(out_dir / "catalogs.json", catalogs_payload)

    # 6. DOM JSON & Markdown
    dom_objects = dom_raw.get("dom", {})
    dom_stats = {
        "object_types": len(dom_objects),
        "total_properties": sum(len(o.get("properties", [])) for o in dom_objects.values()),
        "total_methods": sum(len(o.get("methods", [])) for o in dom_objects.values()),
    }
    write_json(
        out_dir / "dom.json",
        {
            "source": source_obj,
            "stats": dom_stats,
            "objects": dom_objects,
        },
    )
    (out_dir / "dom.md").write_text(render_dom_markdown(dom_objects, source_line), encoding="utf-8")

    # 7. Meta JSON
    lib_counts = {k: len(v) for k, v in fs_catalogs["preset_libraries"].items()}
    meta_payload = {
        "source": source_obj,
        "illustrator": {
            "product_name": app_name,
            "version": version,
            "build_number": build_number,
            "executable_version": exe_meta.get("fileVersion", ""),
            "scripting_version": ai_meta.get("scriptingVersion", ""),
            "extendscript_version": ai_meta.get("extendScriptVersion", ""),
            "ui_locale": locale,
        },
        "windows": {
            "caption": win_meta.get("caption", ""),
            "version": win_meta.get("version", ""),
            "build_number": win_meta.get("buildNumber", ""),
            "architecture": win_meta.get("architecture", ""),
        },
        "extraction": {
            "date": args.extracted_date,
            "methods": {
                "menus": menu_method,
                "shortcuts": "parsed_default_kys_preset",
                "tools": "parsed_tool_panel_preset_and_kys",
                "panels": "extracted_from_window_menu",
                "effects": "extracted_from_effect_menu",
                "catalogs": "com_extendscript_and_omv_enum_ids_and_presets_listing",
                "dom": "com_extendscript_temp_document_reflection",
            },
            "script_errors": dom_raw.get("errors", []),
        },
        "counts": {
            "menus": menu_stats,
            "shortcuts": shortcut_stats,
            "panels_and_tools": panels_tools_stats,
            "effects": effect_stats,
            "catalogs": {
                "available_font_count": ai_meta.get("availableFontCount", -1),
                "document_profiles": len(fs_catalogs["document_profiles"]),
                "preset_libraries": lib_counts,
                "enums": len(enums_merged),
                "save_option_classes": len(script_catalogs.get("saveOptions", {})),
                "export_option_classes": len(script_catalogs.get("exportOptions", {})),
            },
            "dom": dom_stats,
        },
    }
    write_json(out_dir / "meta.json", meta_payload)

    # Size audit
    total_bytes = 0
    print("=== Generated files ===")
    for f in sorted(out_dir.iterdir()):
        if f.is_file():
            sz = f.stat().st_size
            total_bytes += sz
            print(f"  {f.name:20s} {sz:8d} bytes ({sz/1024:.1f} KB)")
    print(f"Total: {total_bytes} bytes ({total_bytes/1024/1024:.2f} MB)")


if __name__ == "__main__":
    main()
