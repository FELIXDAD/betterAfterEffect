"""Build structured Illustrator reference files under docs/reference/ai/ from raw or existing extracted data."""

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

DYNAMIC_SUBMENU_LABELS = {
    "图形样式库",
    "画笔库",
    "符号库",
    "色板库",
    "工作区",
    "脚本",
    "Graphic Style Libraries",
    "Brush Libraries",
    "Symbol Libraries",
    "Swatch Libraries",
    "Workspace",
    "Scripts",
}

STATIC_ITEMS_IN_DYNAMIC_SUBMENUS = {
    "其它库...",
    "Other Library...",
    "其它脚本...",
    "Other Script...",
    "新建工作区...",
    "New Workspace...",
    "管理工作区...",
    "Manage Workspaces...",
    "用户定义",
    "User Defined",
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
        # Match escaped characters (including trailing '\ ') before the delimiter space + '{'.
        for em in re.finditer(r"\t/((?:\\.|[^\n{])+?)[ \t]+\{([^}]*)\}", m.group(1), re.S):
            cmd_id = em.group(1).replace("\\ ", " ")
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
                if rel.parent == pathlib.Path("."):
                    names.append(rel.stem)
                else:
                    names.append(f"{rel.parent.as_posix()}/{rel.stem}")
        return names

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


def classify_read_error(message: str) -> str:
    """Convert any Illustrator property read error string into a short project-owned symbolic tag."""
    low = (message or "").lower()
    if "wrapped items" in low:
        return "wrapped_items_only"
    if "this type of text frame" in low:
        return "text_frame_kind"
    if "dataset was not found" in low:
        return "no_active_dataset"
    if "manual kerning" in low:
        return "no_manual_kerning"
    if "undefined for the text range" in low:
        return "undefined_for_text_range"
    if "no file associated" in low:
        return "no_associated_file"
    m = re.search(r"\b(\d{3,6})\b", message or "")
    if m:
        return f"error_{m.group(1)}"
    return "unreadable"


def sanitize_dom_objects(dom_objects: dict[str, Any]) -> dict[str, Any]:
    """Ensure DOM property entries never retain raw Illustrator error messages."""
    cleaned: dict[str, Any] = {}
    for key, obj in dom_objects.items():
        new_props: list[dict[str, Any]] = []
        for p in obj.get("properties", []):
            entry = {
                "name": p["name"],
                "access": p.get("access", "unknown"),
                "declaredType": p.get("declaredType", "unknown"),
                "runtimeType": p.get("runtimeType", "unknown"),
            }
            if entry["runtimeType"] == "unreadable" or "readError" in p:
                entry["runtimeType"] = "unreadable"
                existing_sample = p.get("sample")
                if isinstance(existing_sample, dict) and "unavailable" in existing_sample:
                    entry["sample"] = {"unavailable": str(existing_sample["unavailable"])}
                else:
                    entry["sample"] = {"unavailable": classify_read_error(str(p.get("readError", "")))}
            else:
                entry["sample"] = p.get("sample")
            new_props.append(entry)
        cleaned[key] = {
            "label": obj.get("label", key),
            "typename": obj.get("typename", ""),
            "properties": new_props,
            "methods": list(obj.get("methods", [])),
        }
    return cleaned


def load_menu_command_ids(map_path: pathlib.Path) -> dict[str, str]:
    """Load verified menu-path to executeMenuCommand ID map."""
    if not map_path.exists():
        return {}
    raw = json.loads(map_path.read_text(encoding="utf-8"))
    paths = raw.get("paths", raw) if isinstance(raw, dict) else {}
    return {str(k): str(v) for k, v in paths.items() if v}


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
        for delim in ("\x08", "\t"):
            if delim in raw_label:
                left, right = raw_label.split(delim, 1)
                raw_label = left
                if not shortcut:
                    shortcut = right.strip() or None
                break

        raw_label = raw_label.strip()
        mnemonic = node.get("mnemonic")
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
        if t == "item" and node.get("commandId") is not None:
            entry["commandId"] = node["commandId"]

        if t == "submenu":
            omitted_reason = node.get("omitted_dynamic")
            if not omitted_reason:
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


def attach_menu_ids(
    nodes: list[dict[str, Any]],
    menu_id_map: dict[str, str],
) -> tuple[list[dict[str, Any]], dict[str, str]]:
    """Attach stable English command id (or null) to every leaf menu item and build id -> menu_path lookup."""
    id_to_path: dict[str, str] = {}

    def walk(items: list[dict[str, Any]], path: list[str]) -> list[dict[str, Any]]:
        out: list[dict[str, Any]] = []
        for it in items:
            t = it.get("type")
            if t == "separator":
                out.append({"type": "separator"})
                continue
            label = it.get("label", "")
            cur_path = path + [label]
            if t == "submenu":
                entry: dict[str, Any] = {
                    "type": "submenu",
                    "label": label,
                }
                if it.get("mnemonic"):
                    entry["mnemonic"] = it["mnemonic"]
                if it.get("shortcut"):
                    entry["shortcut"] = it["shortcut"]
                if it.get("omitted_dynamic"):
                    entry["omitted_dynamic"] = it["omitted_dynamic"]
                entry["children"] = walk(it.get("children", []), cur_path)
                out.append(entry)
            else:
                path_str = " > ".join(cur_path)
                cmd_id = menu_id_map.get(path_str)
                entry = {
                    "type": "item",
                    "id": cmd_id,
                    "label": label,
                }
                if it.get("mnemonic"):
                    entry["mnemonic"] = it["mnemonic"]
                if it.get("shortcut"):
                    entry["shortcut"] = it["shortcut"]
                if it.get("commandId") is not None:
                    entry["commandId"] = it["commandId"]
                if cmd_id and cmd_id not in id_to_path:
                    id_to_path[cmd_id] = path_str
                out.append(entry)
        return out

    return walk(nodes, []), id_to_path


def is_dynamic_menu_path(path: list[str]) -> bool:
    """Return True if a leaf menu item is a dynamic preset library / workspace / script listing entry."""
    if len(path) < 2 or path[1] not in DYNAMIC_SUBMENU_LABELS:
        return False
    leaf = path[-1]
    if leaf in STATIC_ITEMS_IN_DYNAMIC_SUBMENUS:
        return False
    if leaf.startswith("重置") or leaf.startswith("Reset "):
        return False
    return True


def count_menu_nodes(nodes: list[dict[str, Any]]) -> dict[str, int]:
    """Count top-level menus, submenus, leaf items, items with id, static vs dynamic items, and separators."""
    stats = {
        "submenus": 0,
        "items": 0,
        "with_id": 0,
        "static_items": 0,
        "static_with_id": 0,
        "dynamic_items": 0,
        "with_shortcut": 0,
        "separators": 0,
    }

    def walk(items: list[dict[str, Any]], path: list[str]) -> None:
        for it in items:
            t = it.get("type")
            if t == "separator":
                stats["separators"] += 1
            elif t == "submenu":
                stats["submenus"] += 1
                walk(it.get("children", []), path + [it.get("label", "")])
            else:
                cur_path = path + [it.get("label", "")]
                stats["items"] += 1
                has_id = bool(it.get("id"))
                if has_id:
                    stats["with_id"] += 1
                if is_dynamic_menu_path(cur_path):
                    stats["dynamic_items"] += 1
                else:
                    stats["static_items"] += 1
                    if has_id:
                        stats["static_with_id"] += 1
                if it.get("shortcut"):
                    stats["with_shortcut"] += 1

    walk(nodes, [])
    stats["top_level"] = len(nodes)
    return stats


def escape_md(text: str | None) -> str:
    if not text:
        return ""
    return str(text).replace("|", "\\|").replace("\n", " ").strip()


def format_code_cell(val: str | None) -> str:
    if not val:
        return "—"
    return f"`{escape_md(val)}`"


def format_sample(val: Any) -> str:
    if val is None:
        return "null"
    if isinstance(val, bool):
        return "true" if val else "false"
    if isinstance(val, (int, float)):
        return str(val)
    if isinstance(val, list):
        return json.dumps(val, ensure_ascii=False)
    if isinstance(val, dict):
        return json.dumps(val, ensure_ascii=False)
    return str(val)


def render_menu_markdown(nodes: list[dict[str, Any]], source_line: str) -> str:
    """Render full menu tree into readable Markdown tables with internal command ID column."""
    lines = [
        "# Adobe Illustrator 菜单树",
        "",
        source_line,
        "",
    ]
    stats = count_menu_nodes(nodes)
    pct_all = (stats["with_id"] / stats["items"] * 100.0) if stats["items"] else 0.0
    pct_static = (stats["static_with_id"] / stats["static_items"] * 100.0) if stats["static_items"] else 0.0
    lines.append(
        f"共 {stats['top_level']} 个顶层菜单，{stats['submenus']} 个子菜单节点，"
        f"{stats['items']} 个菜单命令（含内部 ID {stats['with_id']} 项 / {pct_all:.1f}%；"
        f"其中静态菜单项 {stats['static_with_id']}/{stats['static_items']} 项含 ID / {pct_static:.1f}%，"
        f"其余 {stats['dynamic_items']} 项为预设库/工作区/脚本动态枚举项；{stats['with_shortcut']} 项带快捷键提示）。"
    )
    lines.append("")

    for top in nodes:
        top_label = top.get("label", "")
        children = top.get("children", [])
        cstats = count_menu_nodes([top])
        # Subtract the top-level submenu itself from submenu count
        sub_count = max(0, cstats["submenus"] - 1)
        lines.append(f"## {escape_md(top_label)}")
        lines.append("")
        lines.append(
            f"子项统计：{cstats['items']} 个命令（{cstats['with_id']} 个含内部 ID，{cstats['with_shortcut']} 个含快捷键提示），"
            f"{sub_count} 个子菜单。"
        )
        lines.append("")
        lines.append("| 菜单路径 | 内部 ID | 快捷键 |")
        lines.append("|---|---|---|")

        def walk_rows(items: list[dict[str, Any]], prefix: list[str]) -> None:
            for it in items:
                t = it.get("type")
                if t == "separator":
                    continue
                lbl = it.get("label", "")
                cur = prefix + [lbl]
                if t == "submenu":
                    omitted = it.get("omitted_dynamic")
                    if omitted:
                        path_md = escape_md(" > ".join(cur))
                        lines.append(f"| **{path_md}** *(动态项省略: `{omitted}`)* | — | — |")
                    else:
                        walk_rows(it.get("children", []), cur)
                else:
                    path_md = escape_md(" > ".join(cur))
                    id_md = format_code_cell(it.get("id"))
                    sc_md = format_code_cell(it.get("shortcut"))
                    lines.append(f"| {path_md} | {id_md} | {sc_md} |")

        walk_rows(children, [])
        lines.append("")

    return "\n".join(lines).rstrip() + "\n"


def build_effects_data(menus: list[dict[str, Any]], source_obj: dict[str, Any]) -> dict[str, Any]:
    """Extract Effect (效果) menu into structured JSON with stable internal executeMenuCommand IDs."""
    effect_menu = None
    for top in menus:
        lbl = top.get("label", "")
        if lbl in ("效果", "Effect") or lbl.startswith("效果") or lbl.startswith("Effect"):
            effect_menu = top
            break

    top_commands: list[dict[str, Any]] = []
    ai_groups: list[dict[str, Any]] = []
    ps_groups: list[dict[str, Any]] = []
    ai_ungrouped: list[dict[str, Any]] = []
    ps_ungrouped: list[dict[str, Any]] = []

    if effect_menu:
        section = "top"
        for item in effect_menu.get("children", []):
            t = item.get("type")
            lbl = item.get("label", "")
            if t == "separator":
                continue
            if ("Illustrator 效果" in lbl or "Illustrator Effects" in lbl) and t != "submenu":
                section = "ai"
                continue
            if ("Photoshop 效果" in lbl or "Photoshop Effects" in lbl) and t != "submenu":
                section = "ps"
                continue

            if section == "top":
                entry: dict[str, Any] = {"id": item.get("id"), "label": lbl}
                if item.get("shortcut"):
                    entry["shortcut"] = item["shortcut"]
                top_commands.append(entry)
            elif t == "submenu":
                group_items: list[dict[str, Any]] = []
                for sub in item.get("children", []):
                    if sub.get("type") == "separator":
                        continue
                    if sub.get("type") == "submenu":
                        sub_name = sub.get("label", "")
                        for leaf in sub.get("children", []):
                            if leaf.get("type") == "separator":
                                continue
                            group_items.append(
                                {
                                    "id": leaf.get("id"),
                                    "label": leaf.get("label", ""),
                                    "subgroup": sub_name,
                                }
                            )
                    else:
                        group_items.append(
                            {
                                "id": sub.get("id"),
                                "label": sub.get("label", ""),
                            }
                        )
                group_obj = {"group": lbl, "items": group_items}
                if section == "ai":
                    ai_groups.append(group_obj)
                else:
                    ps_groups.append(group_obj)
            else:
                leaf_entry = {"id": item.get("id"), "label": lbl}
                if section == "ai":
                    ai_ungrouped.append(leaf_entry)
                else:
                    ps_ungrouped.append(leaf_entry)

    if ai_ungrouped:
        ai_groups.append({"group": "其他（顶层）", "items": ai_ungrouped})
    if ps_ungrouped:
        ps_groups.insert(0, {"group": "通用（顶层）", "items": ps_ungrouped})

    ai_all = [it for g in ai_groups for it in g["items"]]
    ps_all = [it for g in ps_groups for it in g["items"]]
    all_items = top_commands + ai_all + ps_all
    stats = {
        "illustrator_groups": sum(1 for g in ai_groups if g["group"] != "其他（顶层）"),
        "illustrator_effects": len(ai_all),
        "illustrator_with_id": sum(1 for it in ai_all if it.get("id")),
        "photoshop_groups": sum(1 for g in ps_groups if g["group"] != "通用（顶层）"),
        "photoshop_effects": len(ps_all),
        "photoshop_with_id": sum(1 for it in ps_all if it.get("id")),
        "other_items": len(top_commands),
        "other_with_id": sum(1 for it in top_commands if it.get("id")),
        "total_items": len(all_items),
        "total_with_id": sum(1 for it in all_items if it.get("id")),
    }
    return {
        "source": source_obj,
        "stats": stats,
        "top_commands": top_commands,
        "illustrator_effects": ai_groups,
        "photoshop_effects": ps_groups,
    }


def render_effects_markdown(effects_data: dict[str, Any], source_line: str) -> str:
    """Render structured effects data into Markdown tables with internal command ID column."""
    stats = effects_data["stats"]
    pct = (stats["total_with_id"] / stats["total_items"] * 100.0) if stats["total_items"] else 0.0
    lines = [
        "# Adobe Illustrator 效果菜单参考",
        "",
        source_line,
        "",
        f"共提取：Illustrator 效果 {stats['illustrator_effects']} 项（{stats['illustrator_groups']} 组，"
        f"含内部 ID {stats['illustrator_with_id']} 项），"
        f"Photoshop 效果 {stats['photoshop_effects']} 项（{stats['photoshop_groups']} 组，"
        f"含内部 ID {stats['photoshop_with_id']} 项），"
        f"顶部通用命令 {stats['other_items']} 项（含内部 ID {stats['other_with_id']} 项）；"
        f"合计 {stats['total_with_id']}/{stats['total_items']} 项含内部 ID（{pct:.1f}%）。",
        "",
        "## 顶部命令与通用项",
        "",
        "| 命令 | 内部 ID | 快捷键 |",
        "|---|---|---|",
    ]
    for it in effects_data["top_commands"]:
        lines.append(
            f"| {escape_md(it['label'])} | {format_code_cell(it.get('id'))} | {format_code_cell(it.get('shortcut'))} |"
        )

    for sec_title, groups in (
        ("Illustrator 效果", effects_data["illustrator_effects"]),
        ("Photoshop 效果", effects_data["photoshop_effects"]),
    ):
        lines.extend(["", f"## {sec_title}"])
        for g in groups:
            lines.extend(["", f"### {escape_md(g['group'])}", "", "| 效果名称 | 内部 ID |", "|---|---|"])
            for it in g["items"]:
                lbl = f"{it['subgroup']} > {it['label']}" if it.get("subgroup") else it["label"]
                lines.append(f"| {escape_md(lbl)} | {format_code_cell(it.get('id'))} |")

    return "\n".join(lines).rstrip() + "\n"


def render_shortcuts_markdown(kys_data: dict[str, list[dict[str, Any]]], source_line: str) -> str:
    """Render parsed KYS shortcuts into Markdown tables with stable English IDs and matched menu paths."""
    menu_entries = kys_data["menus"]
    tool_entries = kys_data["tools"]
    menu_assigned = [e for e in menu_entries if e["assigned"]]
    tool_assigned = [e for e in tool_entries if e["assigned"]]

    lines = [
        "# Adobe Illustrator 默认键盘快捷键",
        "",
        source_line,
        "",
        f"解析自默认快捷键集（`.kys`）：菜单命令共 {len(menu_entries)} 条（已分配默认快捷键 {len(menu_assigned)} 条，100% 含内部 ID），"
        f"工具及交互动作共 {len(tool_entries)} 条（已分配默认快捷键 {len(tool_assigned)} 条，100% 含内部 ID）。"
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
            "| 命令 ID | 菜单路径 | 默认快捷键 | 上下文 |",
            "|---|---|---|---|",
        ]
    )
    for e in menu_assigned:
        mpath = escape_md(e.get("menu_path")) if e.get("menu_path") else "—"
        lines.append(f"| `{escape_md(e['id'])}` | {mpath} | `{escape_md(e['shortcut'])}` | {e['context']} |")

    return "\n".join(lines).rstrip() + "\n"


def build_panels_tools_data(
    menus: list[dict[str, Any]],
    tool_presets: dict[str, Any],
    kys_data: dict[str, list[dict[str, Any]]],
    source_obj: dict[str, Any],
) -> dict[str, Any]:
    """Build structured panels-tools JSON with internal tool IDs and Window panel command IDs."""
    shortcut_by_tool = {e["id"]: e["shortcut"] for e in kys_data["tools"] if e["shortcut"]}

    default_tb = tool_presets.get("Default", {})
    basic_tb = tool_presets.get("Basic", {})
    gs_tb = tool_presets.get("Getting Started", {})

    basic_tool_set = {t for sl in basic_tb.get("slots", []) for t in sl.get("tools", [])}
    default_tool_set = {t for sl in default_tb.get("slots", []) for t in sl.get("tools", [])}

    default_tools: list[dict[str, Any]] = []
    for sl in default_tb.get("slots", []):
        slot_idx = sl["slot"]
        for t_idx, tool_id in enumerate(sl["tools"]):
            slot_str = f"{slot_idx}" if t_idx == 0 else f"{slot_idx}.{t_idx}"
            default_tools.append(
                {
                    "id": tool_id,
                    "slot": slot_str,
                    "shortcut": shortcut_by_tool.get(tool_id),
                    "in_basic": tool_id in basic_tool_set,
                }
            )

    gs_groups: dict[str, list[dict[str, Any]]] = {}
    for sl in gs_tb.get("slots", []):
        g = sl.get("group", "OTHER")
        for tool_id in sl.get("tools", []):
            gs_groups.setdefault(g, []).append(
                {
                    "id": tool_id,
                    "shortcut": shortcut_by_tool.get(tool_id),
                }
            )

    extra_kys_tools = [
        {"id": e["id"], "shortcut": e["shortcut"], "assigned": e["assigned"]}
        for e in kys_data["tools"]
        if e["id"] not in default_tool_set
    ]

    window_menu = None
    for top in menus:
        lbl = top.get("label", "")
        if lbl in ("窗口", "Window") or lbl.startswith("窗口") or lbl.startswith("Window"):
            window_menu = top
            break

    window_items: list[dict[str, Any]] = []
    type_subpanels: list[dict[str, Any]] = []
    panel_leaf_count = 0
    panel_leaf_with_id = 0
    window_submenus = 0

    if window_menu:
        for item in window_menu.get("children", []):
            t = item.get("type")
            if t == "separator":
                continue
            lbl = item.get("label", "")
            if t == "submenu":
                window_submenus += 1
                sub_entry: dict[str, Any] = {
                    "type": "submenu",
                    "id": None,
                    "label": lbl,
                    "shortcut": item.get("shortcut"),
                }
                if item.get("omitted_dynamic"):
                    sub_entry["omitted_dynamic"] = item["omitted_dynamic"]
                    sub_entry["children"] = []
                else:
                    children_list: list[dict[str, Any]] = []
                    for c in item.get("children", []):
                        if c.get("type") == "separator":
                            continue
                        child_obj = {
                            "type": c.get("type"),
                            "id": c.get("id") if c.get("type") == "item" else None,
                            "label": c.get("label", ""),
                            "shortcut": c.get("shortcut"),
                        }
                        children_list.append(child_obj)
                    sub_entry["children"] = children_list
                    if lbl in ("文字", "Type"):
                        type_subpanels = children_list
                window_items.append(sub_entry)
            else:
                panel_leaf_count += 1
                if item.get("id"):
                    panel_leaf_with_id += 1
                window_items.append(
                    {
                        "type": "panel_or_command",
                        "id": item.get("id"),
                        "label": lbl,
                        "shortcut": item.get("shortcut"),
                    }
                )

    type_subpanels_with_id = sum(1 for c in type_subpanels if c.get("id"))
    stats = {
        "default_toolbar_slots": default_tb.get("slot_count", 0),
        "default_toolbar_tools": len(default_tools),
        "default_toolbar_tools_with_id": sum(1 for t in default_tools if t.get("id")),
        "basic_toolbar_tools": basic_tb.get("tool_count", 0),
        "getting_started_tools": gs_tb.get("tool_count", 0),
        "extra_kys_tool_actions": len(extra_kys_tools),
        "extra_kys_tool_actions_with_id": sum(1 for a in extra_kys_tools if a.get("id")),
        "window_menu_items": panel_leaf_count,
        "window_menu_items_with_id": panel_leaf_with_id,
        "window_type_subpanels": len(type_subpanels),
        "window_type_subpanels_with_id": type_subpanels_with_id,
        "window_menu_submenus": window_submenus,
    }
    return {
        "source": source_obj,
        "stats": stats,
        "toolbars": {
            "default": default_tools,
            "basic": sorted(basic_tool_set),
            "getting_started": gs_groups,
        },
        "extra_tool_actions": extra_kys_tools,
        "window_panels": window_items,
    }


def render_panels_tools_markdown(panels_tools_data: dict[str, Any], source_line: str) -> str:
    """Render tool list and Window panel list into Markdown tables with internal ID columns."""
    stats = panels_tools_data["stats"]
    default_tools = panels_tools_data["toolbars"]["default"]
    gs_groups = panels_tools_data["toolbars"]["getting_started"]
    extra_kys_tools = panels_tools_data["extra_tool_actions"]
    window_panels = panels_tools_data["window_panels"]

    lines = [
        "# Adobe Illustrator 工具与面板列表",
        "",
        source_line,
        "",
        "## 工具列表（高级工具栏 Default 预设）",
        "",
        f"高级工具栏共 {stats['default_toolbar_slots']} 个槽位 / {stats['default_toolbar_tools']} 个工具（100% 含内部工具 ID）；"
        f"基本工具栏（Basic）包含 {stats['basic_toolbar_tools']} 个工具；"
        f"快速入门（Getting Started）包含 {stats['getting_started_tools']} 个工具。",
        "",
        "| 槽位 | 内部工具 ID | 默认快捷键 | Basic 包含 |",
        "|---:|---|---|---|",
    ]

    for t in default_tools:
        sc_col = format_code_cell(t.get("shortcut"))
        in_basic = "是" if t.get("in_basic") else "否"
        lines.append(f"| {t['slot']} | `{escape_md(t['id'])}` | {sc_col} | {in_basic} |")

    lines.extend(
        [
            "",
            "### 快速入门工具栏功能分组（Getting Started）",
            "",
            "| 分组 | 工具列表 |",
            "|---|---|",
        ]
    )
    for g, tools in gs_groups.items():
        tools_str = ", ".join(f"`{escape_md(t['id'])}`" for t in tools)
        lines.append(f"| `{escape_md(g)}` | {tools_str} |")

    lines.extend(
        [
            "",
            "### 快捷键集中额外的工具与交互动作",
            "",
            f"除上述工具栏槽位外，`.kys` `/Tools` 节还定义了 {len(extra_kys_tools)} 项交互/模式/画笔/不透明度/混合模式动作（100% 含内部 ID；下表列出已分配快捷键的项及未分配项统计）：",
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
            f"未分配默认快捷键的 `/Tools` 动作（{len(unassigned_extra)} 项，完整见 `shortcuts.json` / `panels-tools.json`）："
            + ", ".join(f"`{escape_md(e['id'])}`" for e in unassigned_extra)
            + "。"
        )

    lines.extend(
        [
            "",
            "## 面板与窗口菜单项（来自「窗口」菜单）",
            "",
            f"顶层面板/命令共 {stats['window_menu_items']} 项（含内部 ID {stats['window_menu_items_with_id']} 项），"
            f"另含 {stats['window_menu_submenus']} 个子菜单（其中「文字」子面板 {stats['window_type_subpanels_with_id']}/{stats['window_type_subpanels']} 项含内部 ID）。",
            "",
            "| 菜单项 | 内部 ID | 类型 | 快捷键 | 子项摘要 |",
            "|---|---|---|---|---|",
        ]
    )
    type_submenu_children: list[dict[str, Any]] = []
    for item in window_panels:
        lbl = escape_md(item.get("label", ""))
        sc = format_code_cell(item.get("shortcut"))
        if item.get("type") == "submenu":
            if item.get("omitted_dynamic"):
                preview = f"*(动态本地项 `{item['omitted_dynamic']}` 已省略)*"
            else:
                children = item.get("children", [])
                sub_labels = [escape_md(c.get("label", "")) for c in children]
                preview = "、".join(sub_labels[:12]) + (f" 等共 {len(sub_labels)} 项" if len(sub_labels) > 12 else "")
                if item.get("label") in ("文字", "Type"):
                    type_submenu_children = children
            lines.append(f"| **{lbl}** | — | 子菜单 | {sc} | {preview} |")
        else:
            id_col = format_code_cell(item.get("id"))
            lines.append(f"| {lbl} | {id_col} | 面板/命令 | {sc} | |")

    if type_submenu_children:
        lines.extend(
            [
                "",
                "### 「窗口 > 文字」子面板",
                "",
                "| 子面板 | 内部 ID | 快捷键 |",
                "|---|---|---|",
            ]
        )
        for c in type_submenu_children:
            lines.append(
                f"| {escape_md(c.get('label', ''))} | {format_code_cell(c.get('id'))} | {format_code_cell(c.get('shortcut'))} |"
            )

    return "\n".join(lines).rstrip() + "\n"


def render_dom_markdown(dom_data: dict[str, Any], source_line: str) -> str:
    """Render live-inspected DOM object schemas into Markdown tables without verbatim Adobe error strings."""
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
            if isinstance(sample, dict) and "unavailable" in sample:
                sample_str = f"*(不可用: `{escape_md(sample['unavailable'])}`)*"
            elif p.get("runtimeType") == "unreadable":
                tag = classify_read_error(str(p.get("readError", "")))
                sample_str = f"*(不可用: `{escape_md(tag)}`)*"
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
    parser.add_argument(
        "--menu-ids",
        default=str(pathlib.Path(__file__).with_name("menu_command_ids.json")),
    )
    parser.add_argument("--menus-raw", help="Path to raw menus JSON from extract_ai_menus*.ps1")
    parser.add_argument("--dom-raw", help="Path to raw DOM/catalogs JSON from extract_ai_dom_and_catalogs.jsx")
    parser.add_argument(
        "--from-existing",
        help="Rebuild from already-generated docs/reference/ai/ files + local .kys / tool presets",
    )
    parser.add_argument("--output-dir", required=True)
    parser.add_argument("--extracted-date", default=date.today().isoformat())
    args = parser.parse_args()

    install_dir = pathlib.Path(args.install_dir)
    out_dir = pathlib.Path(args.output_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    menu_id_map = load_menu_command_ids(pathlib.Path(args.menu_ids))

    if args.from_existing:
        src_dir = pathlib.Path(args.from_existing)
        existing_meta = json.loads((src_dir / "meta.json").read_text(encoding="utf-8"))
        existing_menus = json.loads((src_dir / "menus.json").read_text(encoding="utf-8"))
        existing_dom = json.loads((src_dir / "dom.json").read_text(encoding="utf-8"))
        existing_catalogs = json.loads((src_dir / "catalogs.json").read_text(encoding="utf-8"))

        source_obj = dict(existing_meta["source"])
        if args.extracted_date:
            source_obj["extracted_date"] = args.extracted_date
        ai_info = existing_meta.get("illustrator", {})
        win_info = existing_meta.get("windows", {})
        locale = source_obj.get("locale") or "zh_CN"
        app_name = source_obj.get("application", "Adobe Illustrator")
        version = source_obj.get("version", "unknown")
        build_number = source_obj.get("build_number", "")
        source_line = (
            f"> 来源：{app_name} ({version}"
            + (f", build {build_number}" if build_number else "")
            + f") | 语言：{locale} | 平台：{win_info.get('caption', 'Windows')} Build {win_info.get('build_number', '')} | 提取日期：{source_obj['extracted_date']}"
        )
        raw_menus = existing_menus.get("menus", [])
        menu_method = existing_menus.get("extraction_method", "unknown")
        raw_dom_objects = existing_dom.get("objects", {})
        script_errors = existing_meta.get("extraction", {}).get("script_errors", [])
        available_font_count = existing_catalogs.get("available_font_count", -1)
        app_presets = existing_catalogs.get("app_presets", {})
        save_options = existing_catalogs.get("save_options", {})
        export_options = existing_catalogs.get("export_options", {})
        existing_enums = existing_catalogs.get("enums", {})
    else:
        if not args.menus_raw or not args.dom_raw:
            parser.error("Either --from-existing or both --menus-raw and --dom-raw are required.")
        menus_raw = json.loads(pathlib.Path(args.menus_raw).read_text(encoding="utf-8"))
        dom_raw = json.loads(pathlib.Path(args.dom_raw).read_text(encoding="utf-8"))

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
        ai_info = {
            "product_name": app_name,
            "version": version,
            "build_number": build_number,
            "executable_version": exe_meta.get("fileVersion", ""),
            "scripting_version": ai_meta.get("scriptingVersion", ""),
            "extendscript_version": ai_meta.get("extendScriptVersion", ""),
            "ui_locale": locale,
        }
        win_info = {
            "caption": win_meta.get("caption", ""),
            "version": win_meta.get("version", ""),
            "build_number": win_meta.get("buildNumber", ""),
            "architecture": win_meta.get("architecture", ""),
        }
        source_line = (
            f"> 来源：{app_name} ({version}"
            + (f", build {build_number}" if build_number else "")
            + f") | 语言：{locale} | 平台：{win_info.get('caption', 'Windows')} Build {win_info.get('build_number', '')} | 提取日期：{args.extracted_date}"
        )
        raw_menus = menus_raw.get("menus", [])
        menu_method = menus_raw.get("method", "unknown")
        if not raw_menus and menus_raw.get("uiaMenuBars"):
            best_bar = max(menus_raw["uiaMenuBars"], key=lambda b: len(b.get("items", [])))
            raw_menus = best_bar.get("items", [])
        raw_dom_objects = dom_raw.get("dom", {})
        script_errors = dom_raw.get("errors", [])
        script_catalogs = dom_raw.get("catalogs", {})
        available_font_count = ai_meta.get("availableFontCount", -1)
        app_presets = script_catalogs.get("presets", {})
        save_options = script_catalogs.get("saveOptions", {})
        export_options = script_catalogs.get("exportOptions", {})
        existing_enums = script_catalogs.get("enums", {})

    # 1. Menus
    menus = normalize_and_sanitize_menus(raw_menus)
    menus, id_to_menu_path = attach_menu_ids(menus, menu_id_map)
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
    for entry in kys_data["menus"]:
        entry["menu_path"] = id_to_menu_path.get(entry["id"])

    shortcut_stats = {
        "menu_commands_total": len(kys_data["menus"]),
        "menu_commands_assigned": sum(1 for e in kys_data["menus"] if e["assigned"]),
        "menu_commands_matched_in_menus": sum(1 for e in kys_data["menus"] if e.get("menu_path")),
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

    # 3. Tool presets + Panels/Tools JSON & Markdown
    tool_preset_candidates = list((install_dir / "Presets" / locale).rglob("*工具面板预设*"))
    if not tool_preset_candidates:
        tool_preset_candidates = list((install_dir / "Presets" / locale).rglob("*Tool*Preset*"))
    tool_presets = parse_tool_presets(tool_preset_candidates[0]) if tool_preset_candidates else {}
    panels_tools_data = build_panels_tools_data(menus, tool_presets, kys_data, source_obj)
    write_json(out_dir / "panels-tools.json", panels_tools_data)
    (out_dir / "panels-tools.md").write_text(
        render_panels_tools_markdown(panels_tools_data, source_line), encoding="utf-8"
    )

    # 4. Effects JSON & Markdown
    effects_data = build_effects_data(menus, source_obj)
    write_json(out_dir / "effects.json", effects_data)
    (out_dir / "effects.md").write_text(render_effects_markdown(effects_data, source_line), encoding="utf-8")

    # 5. Catalogs JSON
    fs_catalogs = scan_preset_libraries(install_dir, locale)
    enums_merged = dict(existing_enums)
    omv_enums = parse_omv_enums(pathlib.Path(args.omv_xml))
    for k, v in omv_enums.items():
        if k not in enums_merged or not enums_merged[k]:
            enums_merged[k] = v
    enums_merged = dict(sorted(enums_merged.items()))

    catalogs_payload = {
        "source": source_obj,
        "available_font_count": available_font_count,
        "document_profiles": fs_catalogs["document_profiles"],
        "preset_libraries": fs_catalogs["preset_libraries"],
        "app_presets": app_presets,
        "enums": enums_merged,
        "save_options": save_options,
        "export_options": export_options,
    }
    write_json(out_dir / "catalogs.json", catalogs_payload)

    # 6. DOM JSON & Markdown (sanitized: zero verbatim Adobe error strings)
    dom_objects = sanitize_dom_objects(raw_dom_objects)
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
        "illustrator": ai_info,
        "windows": win_info,
        "extraction": {
            "date": source_obj["extracted_date"],
            "methods": {
                "menus": menu_method,
                "menu_ids": "matched_from_default_kys_and_public_executeMenuCommand_reference",
                "shortcuts": "parsed_default_kys_preset",
                "tools": "parsed_tool_panel_preset_and_kys",
                "panels": "extracted_from_window_menu_with_internal_ids",
                "effects": "extracted_from_effect_menu_with_internal_ids",
                "catalogs": "com_extendscript_and_omv_enum_ids_and_presets_listing",
                "dom": "com_extendscript_temp_document_reflection_with_symbolic_unavailable_tags",
            },
            "script_errors": script_errors,
        },
        "counts": {
            "menus": menu_stats,
            "shortcuts": shortcut_stats,
            "panels_and_tools": panels_tools_data["stats"],
            "effects": effects_data["stats"],
            "catalogs": {
                "available_font_count": available_font_count,
                "document_profiles": len(fs_catalogs["document_profiles"]),
                "preset_libraries": lib_counts,
                "enums": len(enums_merged),
                "save_option_classes": len(save_options),
                "export_option_classes": len(export_options),
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
