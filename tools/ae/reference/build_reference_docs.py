#!/usr/bin/env python3
"""Transform raw After Effects extraction outputs into clean, compact JSON and
Markdown reference files under docs/reference/ae/.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
from collections import defaultdict
from typing import Any


CATEGORY_SLUG_MAP = {
    "3D 通道": "3d-channel",
    "3D Channel": "3d-channel",
    "音频": "audio",
    "Audio": "audio",
    "模糊和锐化": "blur-and-sharpen",
    "Blur & Sharpen": "blur-and-sharpen",
    "通道": "channel",
    "Channel": "channel",
    "Cinema 4D": "cinema-4d",
    "Boris FX Mocha": "mocha-ae",
    "颜色校正": "color-correction",
    "Color Correction": "color-correction",
    "扭曲": "distort",
    "Distort": "distort",
    "表达式控制": "expression-controls",
    "Expression Controls": "expression-controls",
    "生成": "generate",
    "Generate": "generate",
    "沉浸式视频": "immersive-video",
    "Immersive Video": "immersive-video",
    "抠像": "keying",
    "键控": "keying",
    "Keying": "keying",
    "遮罩": "matte",
    "Matte": "matte",
    "杂色和颗粒": "noise-and-grain",
    "Noise & Grain": "noise-and-grain",
    "过时": "obsolete",
    "Obsolete": "obsolete",
    "_Obsolete": "obsolete-hidden",
    "透视": "perspective",
    "Perspective": "perspective",
    "模拟": "simulation",
    "Simulation": "simulation",
    "风格化": "stylize",
    "Stylize": "stylize",
    "文本": "text",
    "Text": "text",
    "时间": "time",
    "Time": "time",
    "过渡": "transition",
    "Transition": "transition",
    "实用工具": "utility",
    "Utility": "utility",
    "": "hidden-presets",
}

THIRD_PARTY_CATEGORIES = {
    "FMT",
    "BGRA",
    "BSKL",
    "FilmConvert",
    "Pixel Sorter Studio",
    "Plugin Everything",
    "Swarm",
    "TiM",
    "Zaebects",
}

BUILTIN_SCRIPTUI_PANELS = {
    "Create Nulls From Paths.jsx",
    "VR Comp Editor.jsx",
}

MRU_SUBMENU_LABELS = {
    "打开最近的文件",
    "Open Recent",
    "导入最近的素材",
    "Import Recent Footage",
    "最近的脚本文件",
    "Recent Script Files",
}

TOOL_DESCRIPTIONS = {
    "ToolArrow": ("Selection Tool", "选择工具"),
    "ToolHand": ("Hand Tool", "抓手工具"),
    "ToolMagnify": ("Zoom Tool", "缩放工具"),
    "ToolRotate": ("Rotation Tool", "旋转工具"),
    "ToolCamera": ("Unified Camera Tool", "统一摄像机工具"),
    "ToolCameraToggle": ("Cycle Camera Tools", "切换摄像机工具"),
    "ToolCameraOrbit": ("Orbit Around Cursor / Scene Tool", "环绕摄像机工具"),
    "ToolCameraOrbitToggle": ("Cycle Orbit Camera Mode", "切换环绕模式"),
    "ToolCameraPan": ("Pan Under Cursor Tool", "平移摄像机工具"),
    "ToolCameraPanToggle": ("Cycle Pan Camera Mode", "切换平移模式"),
    "ToolCameraDolly": ("Dolly Towards/Under Cursor Tool", "推拉摄像机工具"),
    "ToolCameraDollyToggle": ("Cycle Dolly Camera Mode", "切换推拉模式"),
    "ToolPan": ("Pan Behind (Anchor Point) Tool", "向后平移（锚点）工具"),
    "ToolMask": ("Shape / Mask Tool", "形状 / 遮罩绘制工具"),
    "ToolMaskToggle": ("Cycle Shape / Mask Tools", "切换形状 / 遮罩工具"),
    "ToolPen": ("Pen Tool", "钢笔工具"),
    "ToolPenToggle": ("Cycle Pen Tools", "切换钢笔工具"),
    "ToolText": ("Type Tool", "文字工具"),
    "ToolPaint": ("Brush / Clone Stamp / Eraser Tool", "画笔 / 图章 / 橡皮擦工具"),
    "ToolRotoBrush": ("Roto Brush / Refine Edge Tool", "Roto 笔刷 / 调整边缘工具"),
    "ToolPin": ("Puppet Pin Tool", "操控点工具"),
    "ToolParametricMesh": ("3D Parametric Mesh Tool", "3D 参数化网格工具"),
    "GizmoModeToggle": ("Toggle 3D Gizmo Mode", "切换 3D 变换控件模式"),
    "GizmoModeLegacy": ("3D Gizmo: Universal / Legacy", "3D 变换控件：通用模式"),
    "GizmoModePosition": ("3D Gizmo: Position", "3D 变换控件：位置"),
    "GizmoModeScale": ("3D Gizmo: Scale", "3D 变换控件：缩放"),
    "GizmoModeRotate": ("3D Gizmo: Rotation", "3D 变换控件：旋转"),
}

DRIVE_PATH_RE = re.compile(r"[A-Za-z]:[/\\]|\\\\[A-Za-z0-9_.-]+\\|/Users/|/home/")


def slugify_category(cat: str) -> str:
    if cat in CATEGORY_SLUG_MAP:
        return CATEGORY_SLUG_MAP[cat]
    ascii_slug = re.sub(r"[^a-z0-9]+", "-", cat.lower()).strip("-")
    if ascii_slug:
        return ascii_slug
    digest = hashlib.sha1(cat.encode("utf-8")).hexdigest()[:8]
    return f"cat-{digest}"


def classify_effect_origin(match_name: str, category: str) -> str:
    if category.startswith("Sapphire") or category in THIRD_PARTY_CATEGORIES or match_name == "mochaProAE":
        return "third-party"
    if match_name.startswith(("ADBE ", "APC ", "PSL ", "VISINF ", "Mettle ", "Pseudo/ADBE ")):
        return "adobe-builtin"
    return "adobe-bundled"


def count_leaf_properties(props: list[dict[str, Any]]) -> int:
    total = 0
    for p in props:
        if p.get("propertyType") == "PROPERTY":
            total += 1
        elif "properties" in p:
            total += count_leaf_properties(p["properties"])
    return total


def count_dropdown_properties(props: list[dict[str, Any]]) -> int:
    total = 0
    for p in props:
        if p.get("propertyType") == "PROPERTY":
            if p.get("dropdownOptions"):
                total += 1
        elif "properties" in p:
            total += count_dropdown_properties(p["properties"])
    return total


def md_escape(val: Any) -> str:
    if val is None:
        return ""
    return str(val).replace("|", "\\|").replace("\n", " ").replace("\r", "")


def sanitize_render_dict(d: Any) -> Any:
    if isinstance(d, dict):
        out = {}
        for k, v in d.items():
            if k == "Output File Info":
                if isinstance(v, dict):
                    sanitized_info = {}
                    for ik, iv in v.items():
                        if isinstance(iv, str) and (DRIVE_PATH_RE.search(iv) or ik in ("Full Flat Path", "Base Path")):
                            sanitized_info[ik] = "<redacted:path>"
                        else:
                            sanitized_info[ik] = iv
                    out[k] = sanitized_info
                else:
                    out[k] = "<redacted:path>"
            elif isinstance(v, str) and DRIVE_PATH_RE.search(v):
                out[k] = "<redacted:path>"
            else:
                out[k] = sanitize_render_dict(v)
        return out
    if isinstance(d, list):
        return [sanitize_render_dict(x) for x in d]
    return d


def sanitize_menu_nodes(nodes: list[dict[str, Any]], parent_label: str = "") -> list[dict[str, Any]]:
    cleaned: list[dict[str, Any]] = []
    in_effect_menu = ("效果" in parent_label or "Effect" in parent_label)
    in_window_menu = ("窗口" in parent_label or "Window" in parent_label)

    for n in nodes:
        t = n.get("type")
        if t == "separator":
            cleaned.append({"type": "separator"})
            continue

        label = n.get("label", "")

        # Redact MRU submenus (recent files/footage/scripts)
        if label in MRU_SUBMENU_LABELS and t == "submenu":
            cleaned.append({
                "type": "submenu",
                "label": label,
                "items": [{"type": "item", "label": "<redacted:MRU list>"}],
            })
            continue

        # Omit host-specific third-party CEP extensions
        if in_window_menu and label in ("扩展", "Extensions") and t == "submenu":
            cleaned.append({
                "type": "submenu",
                "label": label,
                "items": [{"type": "item", "label": "<omitted:host third-party CEP extensions>"}],
            })
            continue

        # Omit user-installed third-party ScriptUI panels under Window menu
        if in_window_menu and (label.endswith(".jsx") or label.endswith(".jsxbin")):
            if label not in BUILTIN_SCRIPTUI_PANELS:
                continue

        # Omit host third-party effect category submenus under Effect menu
        if in_effect_menu and t == "submenu":
            if label.startswith("Sapphire") or label in THIRD_PARTY_CATEGORIES:
                continue

        # Redact any stray absolute path
        if DRIVE_PATH_RE.search(label):
            label = "<redacted:path>"

        new_node: dict[str, Any] = {
            "type": t,
            "label": label,
        }
        if n.get("shortcut"):
            new_node["shortcut"] = n["shortcut"]
        if n.get("commandId") is not None:
            new_node["commandId"] = n["commandId"]
        if t == "submenu":
            new_node["items"] = sanitize_menu_nodes(n.get("items", []), parent_label=label)
        cleaned.append(new_node)

    return cleaned


def write_json(path: pathlib.Path, data: Any, compact: bool = True) -> int:
    path.parent.mkdir(parents=True, exist_ok=True)
    if compact:
        text = json.dumps(data, ensure_ascii=False, separators=(",", ":"))
    else:
        text = json.dumps(data, ensure_ascii=False, indent=2)
    path.write_text(text + "\n", encoding="utf-8")
    return path.stat().st_size


def write_md(path: pathlib.Path, lines: list[str]) -> int:
    path.parent.mkdir(parents=True, exist_ok=True)
    text = "\n".join(lines).rstrip() + "\n"
    path.write_text(text, encoding="utf-8")
    return path.stat().st_size


def count_menu_nodes(nodes: list[dict[str, Any]]) -> tuple[int, int]:
    items = 0
    submenus = 0
    for n in nodes:
        t = n.get("type")
        if t == "separator":
            continue
        if t == "submenu":
            submenus += 1
            ci, cs = count_menu_nodes(n.get("items", []))
            items += ci
            submenus += cs
        else:
            items += 1
    return items, submenus


def render_menu_md_lines(nodes: list[dict[str, Any]], depth: int = 0) -> list[str]:
    lines: list[str] = []
    indent = "  " * depth
    for n in nodes:
        t = n.get("type")
        if t == "separator":
            continue
        label = n.get("label", "")
        shortcut = n.get("shortcut")
        cmd_id = n.get("commandId")
        if t == "submenu":
            lines.append(f"{indent}- **{label}**")
            lines.extend(render_menu_md_lines(n.get("items", []), depth + 1))
        else:
            sc = f" `{shortcut}`" if shortcut else ""
            cid = f" *(id:{cmd_id})*" if cmd_id is not None else ""
            lines.append(f"{indent}- {label}{sc}{cid}")
    return lines


def extract_panels_from_menus(menus: list[dict[str, Any]]) -> dict[str, Any]:
    window_menu = None
    for top in menus:
        lbl = top.get("label", "")
        if "窗口" in lbl or "Window" in lbl:
            window_menu = top
            break
    if not window_menu:
        return {"found": False, "workspaces": [], "auxPanels": [], "corePanels": [], "scriptUiPanels": []}

    groups: list[list[dict[str, Any]]] = [[]]
    for item in window_menu.get("items", []):
        if item.get("type") == "separator":
            if groups[-1]:
                groups.append([])
        else:
            groups[-1].append(item)

    workspaces: list[str] = []
    aux_panels: list[dict[str, Any]] = []
    core_panels: list[dict[str, Any]] = []
    script_ui: list[dict[str, Any]] = []

    for grp in groups:
        for it in grp:
            lbl = it.get("label", "")
            if it.get("type") == "submenu" and (lbl.startswith("工作区") or lbl.startswith("Workspace")):
                for ws in it.get("items", []):
                    if ws.get("type") == "separator":
                        break
                    workspaces.append(ws.get("label", ""))
            elif lbl.endswith(".jsx"):
                script_ui.append(it)

    # Identify built-in auxiliary panels (largest flat item group) and core document panels
    flat_groups = [
        g for g in groups
        if len(g) >= 5 and all(it.get("type") == "item" and not it.get("label", "").endswith(".jsx") for it in g)
    ]
    if len(flat_groups) >= 2:
        aux_panels = flat_groups[0]
        core_panels = flat_groups[1]
    elif len(flat_groups) == 1:
        aux_panels = flat_groups[0]

    return {
        "found": True,
        "workspaces": workspaces,
        "auxPanels": aux_panels,
        "corePanels": core_panels,
        "scriptUiPanels": script_ui,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Build docs/reference/ae from raw extraction JSON.")
    parser.add_argument("--raw-dir", default=r"E:\bae-test\ref-ae\raw")
    parser.add_argument("--out-dir", default="docs/reference/ae")
    parser.add_argument("--windows-version", default="Windows 11 Pro (10.0.26200, x64)")
    parser.add_argument("--extracted-at", default="2026-11-21")
    args = parser.parse_args()

    raw_dir = pathlib.Path(args.raw_dir)
    out_dir = pathlib.Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    meta_raw = json.loads((raw_dir / "meta_raw.json").read_text(encoding="utf-8"))
    effects_raw = json.loads((raw_dir / "effects_raw.json").read_text(encoding="utf-8"))
    layers_raw = json.loads((raw_dir / "layers_raw.json").read_text(encoding="utf-8"))
    render_raw = json.loads((raw_dir / "render_output_raw.json").read_text(encoding="utf-8"))
    shortcuts_raw = json.loads((raw_dir / "shortcuts_raw.json").read_text(encoding="utf-8"))
    prefs_raw = json.loads((raw_dir / "preferences_raw.json").read_text(encoding="utf-8"))
    menus_raw_path = raw_dir / "menus_raw.json"
    menus_raw = json.loads(menus_raw_path.read_text(encoding="utf-8")) if menus_raw_path.exists() else None

    source_header = {
        "app": "Adobe After Effects",
        "version": meta_raw.get("appVersion"),
        "buildName": meta_raw.get("buildName"),
        "buildNumber": meta_raw.get("buildNumber"),
        "language": meta_raw.get("isoLanguage"),
        "platform": args.windows_version,
        "extractedAt": args.extracted_at,
    }
    md_source_line = (
        f"> 来源：{source_header['app']} {source_header['version']} "
        f"({source_header['language']}) | 平台：{source_header['platform']} | "
        f"提取日期：{source_header['extractedAt']}"
    )

    # 1. Effects catalog & per-category parameter files
    catalog = effects_raw.get("catalog", [])
    detailed = effects_raw.get("detailed", [])
    skipped = effects_raw.get("skipped", [])

    detailed_by_match = {e["matchName"]: e for e in detailed}
    skipped_by_match = {e["matchName"]: e for e in skipped}
    by_category_slug: dict[str, list[dict[str, Any]]] = defaultdict(list)
    category_display_names: dict[str, str] = {}
    third_party_list: list[dict[str, Any]] = []
    adobe_builtin_count = 0
    adobe_bundled_count = 0
    total_effect_params = 0
    total_dropdown_params = 0

    for entry in catalog:
        mn = entry["matchName"]
        cat = entry.get("category", "")
        origin = classify_effect_origin(mn, cat)
        if origin == "third-party":
            third_party_list.append({
                "displayName": entry["displayName"],
                "matchName": mn,
                "category": cat,
                "origin": origin,
            })
            continue

        if origin == "adobe-builtin":
            adobe_builtin_count += 1
        else:
            adobe_bundled_count += 1

        slug = slugify_category(cat)
        category_display_names.setdefault(slug, cat if cat else "(Hidden / Animation Preset Pseudo-Effects)")
        det = detailed_by_match.get(mn)
        item_obj: dict[str, Any] = {
            "displayName": entry["displayName"],
            "matchName": mn,
            "category": cat,
            "origin": origin,
        }
        if det is not None:
            props = det.get("properties", [])
            pcnt = count_leaf_properties(props)
            ddcnt = count_dropdown_properties(props)
            total_effect_params += pcnt
            total_dropdown_params += ddcnt
            item_obj["paramCount"] = pcnt
            item_obj["properties"] = props
        else:
            item_obj["paramCount"] = 0
            item_obj["skipped"] = True
            if mn in skipped_by_match:
                item_obj["skipReason"] = skipped_by_match[mn].get("reason")
        by_category_slug[slug].append(item_obj)

    effects_dir = out_dir / "effects"
    effects_dir.mkdir(parents=True, exist_ok=True)
    for old in effects_dir.glob("*.json"):
        old.unlink()

    effect_files_meta = []
    for slug in sorted(by_category_slug.keys()):
        items = sorted(by_category_slug[slug], key=lambda x: (x["matchName"], x["displayName"]))
        payload = {
            "source": source_header,
            "category": category_display_names[slug],
            "categorySlug": slug,
            "effectCount": len(items),
            "effects": items,
        }
        fpath = effects_dir / f"{slug}.json"
        sz = write_json(fpath, payload, compact=True)
        effect_files_meta.append((slug, category_display_names[slug], len(items), sz))

    tp_payload = {
        "source": source_header,
        "note": "Non-Adobe third-party plugins detected on the extraction host; catalog metadata only (parameters not expanded).",
        "effectCount": len(third_party_list),
        "effects": sorted(third_party_list, key=lambda x: (x["category"], x["matchName"])),
    }
    write_json(effects_dir / "third-party.json", tp_payload, compact=True)

    ef_md: list[str] = [
        "# After Effects 效果目录总表",
        "",
        md_source_line,
        "",
        "## 统计摘要",
        "",
        f"- `app.effects` 总条目：{len(catalog)}",
        f"- Adobe 内置效果（`ADBE` / `APC` / `PSL` / `VISINF` / `Mettle` / `Pseudo/ADBE`）：{adobe_builtin_count}",
        f"- Adobe 随附效果（CycoreFX HD `CC`/`CS`、Keylight、ProEXR、Cineware、Mocha AE）：{adobe_bundled_count}",
        f"- 成功展开参数的效果数：{len(detailed)}（共 {total_effect_params} 个叶子参数，其中 {total_dropdown_params} 个含下拉选项列表）",
        f"- 因模态框或图层类型限制跳过的自带效果数：{len(skipped)}",
        f"- 本机检测到的第三方插件（仅录目录，见 `effects/third-party.json`）：{len(third_party_list)}",
        "",
        "## 分类文件索引",
        "",
        "| 分类（AE 界面名） | 文件 | 效果数 |",
        "|---|---|---:|",
    ]
    for slug, cat_name, cnt, _ in effect_files_meta:
        ef_md.append(f"| {md_escape(cat_name)} | `effects/{slug}.json` | {cnt} |")
    ef_md.append(f"| *(第三方插件目录)* | `effects/third-party.json` | {len(third_party_list)} |")

    ef_md.extend([
        "",
        "## Adobe 自带及随附效果总表",
        "",
        "| 分类 | 名称 | matchName | 来源 | 参数数 |",
        "|---|---|---|---|---:|",
    ])
    for slug in sorted(by_category_slug.keys()):
        for it in sorted(by_category_slug[slug], key=lambda x: (x["matchName"], x["displayName"])):
            p_str = str(it["paramCount"]) if not it.get("skipped") else "*(跳过)*"
            cat_disp = it["category"] if it["category"] else "*(隐藏/预设)*"
            ef_md.append(
                f"| {md_escape(cat_disp)} | {md_escape(it['displayName'])} | "
                f"`{md_escape(it['matchName'])}` | {it['origin']} | {p_str} |"
            )

    if skipped:
        ef_md.extend([
            "",
            "## 跳过的自带效果",
            "",
            "| 分类 | 名称 | matchName | 原因 |",
            "|---|---|---|---|",
        ])
        for sk in skipped:
            ef_md.append(
                f"| {md_escape(sk.get('category'))} | {md_escape(sk.get('displayName'))} | "
                f"`{md_escape(sk.get('matchName'))}` | {md_escape(sk.get('reason'))} |"
            )

    if third_party_list:
        ef_md.extend([
            "",
            "## 本机第三方插件（非 Adobe，仅作区分标注）",
            "",
            "| 分类 | 名称 | matchName |",
            "|---|---|---|",
        ])
        for tp in sorted(third_party_list, key=lambda x: (x["category"], x["matchName"])):
            ef_md.append(
                f"| {md_escape(tp['category'])} | {md_escape(tp['displayName'])} | `{md_escape(tp['matchName'])}` |"
            )

    write_md(out_dir / "effects.md", ef_md)

    # 2. Layer property trees
    layers_dir = out_dir / "layers"
    layers_dir.mkdir(parents=True, exist_ok=True)
    for old in layers_dir.glob("*.json"):
        old.unlink()

    raw_layers = layers_raw.get("layers", {})
    renderer_geom = layers_raw.get("rendererGeometry", {})
    layer_notes = layers_raw.get("notes", {})

    layer_bundles: list[tuple[str, str, dict[str, Any]]] = [
        ("solid", "纯色图层（2D、3D+遮罩及 3 种 3D 渲染器几何/材质选项）", {
            "solid2D": raw_layers.get("solid"),
            "solid3DWithMask": raw_layers.get("solid_3d_and_mask"),
            "rendererGeometryOptions": renderer_geom,
        }),
        ("text", "文字图层（含文本动画器、3 种选择器及全部动画属性）", {
            "text": raw_layers.get("text"),
            "constructionNotes": layer_notes.get("text"),
        }),
        ("shape", "形状图层（含组、4 种路径、4 种描边/填充及 10 种操作器）", {
            "shape": raw_layers.get("shape"),
            "constructionNotes": layer_notes.get("shape"),
        }),
        ("camera", "摄像机图层（双节点与单节点）", {
            "twoNode": raw_layers.get("camera_two_node"),
            "oneNode": raw_layers.get("camera_one_node"),
        }),
        ("light", "灯光图层（平行光、聚光、点光、环境光、Environment 光）", {
            k: v for k, v in raw_layers.items() if k.startswith("light_")
        }),
        ("null", "空对象图层", {
            "null": raw_layers.get("null_layer"),
        }),
        ("adjustment", "调整图层", {
            "adjustment": raw_layers.get("adjustment"),
        }),
        ("footage", "素材图层（静态图像素材与音频素材）", {
            "image": raw_layers.get("footage_image"),
            "audio": raw_layers.get("footage_audio"),
            "constructionNotes": layer_notes.get("footage"),
        }),
        ("precomp", "预合成图层（含时间重映射与音频）", {
            "precomp": raw_layers.get("precomp"),
        }),
    ]

    layers_md: list[str] = [
        "# After Effects 图层属性树总览",
        "",
        md_source_line,
        "",
        "## 图层类型索引",
        "",
        "| 图层类型 | 说明 | 文件 | 顶层属性组数 | 叶子属性总数 |",
        "|---|---|---|---:|---:|",
    ]

    layer_count_summary: dict[str, int] = {}
    for slug, desc, content in layer_bundles:
        payload = {
            "source": source_header,
            "layerType": slug,
            "data": content,
        }
        write_json(layers_dir / f"{slug}.json", payload, compact=True)

        top_groups = 0
        leaf_total = 0
        for vk, vv in content.items():
            if isinstance(vv, dict) and "properties" in vv and isinstance(vv["properties"], list):
                top_groups += len(vv["properties"])
                leaf_total += count_leaf_properties(vv["properties"])
        layer_count_summary[slug] = leaf_total
        layers_md.append(f"| `{slug}` | {desc} | `layers/{slug}.json` | {top_groups} | {leaf_total} |")

    layers_md.extend(["", "## 各图层顶层属性组明细", ""])
    for slug, desc, content in layer_bundles:
        layers_md.append(f"### `{slug}` — {desc}")
        layers_md.append("")
        for vk, vv in content.items():
            if isinstance(vv, dict) and "properties" in vv and isinstance(vv["properties"], list):
                layers_md.append(
                    f"**变体 `{vk}`**（`matchName`: `{vv.get('matchName')}`, "
                    f"3D: `{vv.get('threeDLayer')}`, 音频: `{vv.get('hasAudio')}`）"
                )
                layers_md.append("")
                layers_md.append("| 顶层属性组名称 | matchName | 类型 | 子属性数（叶子） |")
                layers_md.append("|---|---|---|---:|")
                for p in vv["properties"]:
                    leaves = count_leaf_properties(p.get("properties", [])) if "properties" in p else 1
                    layers_md.append(
                        f"| {md_escape(p.get('name'))} | `{md_escape(p.get('matchName'))}` | "
                        f"`{p.get('propertyType')}` | {leaves} |"
                    )
                layers_md.append("")

    shape_added = (layer_notes.get("shape") or {}).get("added", [])
    if shape_added:
        layers_md.extend([
            "## 形状图层已展开内容（`ADBE Root Vectors Group`）",
            "",
            ", ".join(f"`{m}`" for m in shape_added),
            "",
        ])
    text_added = (layer_notes.get("text") or {}).get("added", [])
    if text_added:
        layers_md.extend([
            "## 文字动画器已展开属性与选择器",
            "",
            ", ".join(f"`{m}`" for m in text_added),
            "",
        ])

    write_md(out_dir / "layers.md", layers_md)

    # 3. Render Settings & Output Modules
    render_clean = sanitize_render_dict(render_raw)
    render_payload = {
        "source": source_header,
        "renderSettings": render_clean.get("renderSettings", {}),
        "outputModule": render_clean.get("outputModule", {}),
    }
    write_json(out_dir / "render-output.json", render_payload, compact=False)

    # 4. Shortcuts
    shortcuts_payload = {
        "source": source_header,
        "contextCount": shortcuts_raw["contextCount"],
        "commandCount": shortcuts_raw["commandCount"],
        "contexts": shortcuts_raw["contexts"],
    }
    write_json(out_dir / "shortcuts.json", shortcuts_payload, compact=True)

    sc_md: list[str] = [
        "# After Effects 默认键盘快捷键",
        "",
        md_source_line,
        "",
        f"共提取 **{shortcuts_raw['contextCount']}** 个上下文、**{shortcuts_raw['commandCount']}** 条命令（来自 AE 默认快捷键配置文件，不含用户自定义覆盖）。",
        "",
    ]
    for ctx_name, cmds in shortcuts_raw["contexts"].items():
        sc_md.append(f"## `{ctx_name}`（{len(cmds)}）")
        sc_md.append("")
        sc_md.append("| 命令 | 默认快捷键 |")
        sc_md.append("|---|---|")
        for c in cmds:
            keys = " / ".join(f"`{k}`" for k in c["shortcuts"]) if c["shortcuts"] else "*(无默认绑定)*"
            sc_md.append(f"| `{md_escape(c['command'])}` | {keys} |")
        sc_md.append("")
    write_md(out_dir / "shortcuts.md", sc_md)

    # 5. Preferences
    prefs_payload = {
        "source": source_header,
        "sanitizationNote": "Personal paths, user tokens, GUIDs, device names, third-party/CEP/MRU sections, and binary preset blobs have been redacted or omitted.",
        "fileGroups": prefs_raw["fileGroups"],
        "sectionCount": prefs_raw["sectionCount"],
        "keyCount": prefs_raw["keyCount"],
        "excludedSectionCount": prefs_raw["excludedSectionCount"],
        "sections": prefs_raw["sections"],
    }
    write_json(out_dir / "preferences.json", prefs_payload, compact=True)

    # 6. Menus & Panels/Tools
    menu_item_count = 0
    menu_submenu_count = 0
    top_menu_count = 0
    menu_method = "unavailable"
    panels_info: dict[str, Any] = {"found": False}

    if menus_raw is not None:
        menu_method = menus_raw.get("method", "unknown")
        menus_tree = sanitize_menu_nodes(menus_raw.get("menus", []))
        top_menu_count = len(menus_tree)
        menu_item_count, menu_submenu_count = count_menu_nodes(menus_tree)
        panels_info = extract_panels_from_menus(menus_tree)

        menus_payload = {
            "source": source_header,
            "extractionMethod": menu_method,
            "sanitizationNote": "MRU recent-file/footage/script lists, host third-party CEP extensions, user ScriptUI scripts, and third-party effect category submenus have been redacted or omitted.",
            "topLevelCount": top_menu_count,
            "submenuCount": menu_submenu_count,
            "itemCount": menu_item_count,
            "menus": menus_tree,
        }
        write_json(out_dir / "menus.json", menus_payload, compact=True)

        menus_md: list[str] = [
            "# After Effects 主菜单树",
            "",
            md_source_line,
            "",
            f"- 提取方式：`{menu_method}`（只读 Win32 菜单句柄遍历，无界面点击或前台抢占）",
            f"- 顶层菜单：{top_menu_count} | 子菜单：{menu_submenu_count} | 叶子菜单项：{menu_item_count}",
            "- 脱敏说明：已移除本机最近打开/导入记录、第三方插件效果子菜单、第三方 CEP 扩展及用户安装的第三方 ScriptUI 脚本。",
            "",
        ]
        for top in menus_tree:
            menus_md.append(f"## {top.get('label', '')}")
            menus_md.append("")
            menus_md.extend(render_menu_md_lines(top.get("items", []), depth=0))
            menus_md.append("")
        write_md(out_dir / "menus.md", menus_md)

    # 7. Panels & Tools (panels-tools.md)
    tools_list = shortcuts_raw.get("tools", [])
    aux_panels = panels_info.get("auxPanels", [])
    core_panels = panels_info.get("corePanels", [])
    script_ui_panels = panels_info.get("scriptUiPanels", [])
    workspaces = panels_info.get("workspaces", [])

    pt_md: list[str] = [
        "# After Effects 面板、工作区与工具列表",
        "",
        md_source_line,
        "",
        "## 工具列表（来自默认快捷键 `CEggAppTool`，共 " + str(len(tools_list)) + " 项）",
        "",
        "| 命令标识 | 默认快捷键 | 简述（英文 / 中文） |",
        "|---|---|---|",
    ]
    for t in tools_list:
        cmd = t["command"]
        keys = " / ".join(f"`{k}`" for k in t["shortcuts"]) if t["shortcuts"] else "*(无)*"
        en, zh = TOOL_DESCRIPTIONS.get(cmd, (cmd, ""))
        desc = f"{en} / {zh}" if zh else en
        pt_md.append(f"| `{cmd}` | {keys} | {desc} |")

    pt_md.extend([
        "",
        f"## 核心文档面板（来自「窗口」菜单，共 {len(core_panels)} 项）",
        "",
        "| 面板名称 | 快捷键 | Win32 Command ID |",
        "|---|---|---:|",
    ])
    for it in core_panels:
        lbl = re.sub(r"[:：]\s*[（(]无[）)]$", "", it.get("label", ""))
        sc = f"`{it['shortcut']}`" if it.get("shortcut") else ""
        cid = str(it["commandId"]) if it.get("commandId") is not None else ""
        pt_md.append(f"| {md_escape(lbl)} | {sc} | {cid} |")

    pt_md.extend([
        "",
        f"## 内置功能面板（来自「窗口」菜单，共 {len(aux_panels)} 项）",
        "",
        "| 面板名称 | 快捷键 | Win32 Command ID |",
        "|---|---|---:|",
    ])
    for it in aux_panels:
        sc = f"`{it['shortcut']}`" if it.get("shortcut") else ""
        cid = str(it["commandId"]) if it.get("commandId") is not None else ""
        pt_md.append(f"| {md_escape(it.get('label'))} | {sc} | {cid} |")

    pt_md.extend([
        "",
        f"## 内置 ScriptUI 面板（共 {len(script_ui_panels)} 项）",
        "",
        "| 脚本面板 | Win32 Command ID |",
        "|---|---:|",
    ])
    for it in script_ui_panels:
        cid = str(it["commandId"]) if it.get("commandId") is not None else ""
        pt_md.append(f"| `{md_escape(it.get('label'))}` | {cid} |")

    pt_md.extend([
        "",
        f"## 内置工作区（来自「窗口 → 工作区」，共 {len(workspaces)} 项）",
        "",
        ", ".join(workspaces) if workspaces else "*(无)*",
        "",
    ])
    write_md(out_dir / "panels-tools.md", pt_md)

    # 8. meta.json
    rq_settings = render_payload.get("renderSettings", {})
    om_settings = render_payload.get("outputModule", {})
    meta_payload = {
        "source": source_header,
        "extractionMethods": {
            "effectsAndLayersAndRender": "AfterFX.exe -r tools/ae/reference/extract_ae_data.jsx (ExtendScript DOM)",
            "menus": f"tools/ae/reference/extract_ae_menus.ps1 ({menu_method})",
            "shortcutsAndPreferences": "tools/ae/reference/parse_ae_configs.py (parsed from %APPDATA%/Adobe/After Effects/26.0)",
        },
        "compRenderers": meta_raw.get("compRenderers", []),
        "defaultCompRenderer": meta_raw.get("defaultCompRenderer"),
        "extendScriptCapabilities": {
            "dropdownOptionLabelsExposed": True,
            "dropdownPropertyName": "Property.propertyParameters",
            "dropdownParamsExtracted": total_dropdown_params,
            "propertyReflectProbe": meta_raw.get("propertyReflectProbe"),
        },
        "counts": {
            "effectsCatalogTotal": len(catalog),
            "effectsAdobeBuiltin": adobe_builtin_count,
            "effectsAdobeBundled": adobe_bundled_count,
            "effectsDetailedExtracted": len(detailed),
            "effectsTotalLeafParams": total_effect_params,
            "effectsDropdownParams": total_dropdown_params,
            "effectsSkipped": len(skipped),
            "effectsThirdParty": len(third_party_list),
            "effectCategoryFiles": len(effect_files_meta),
            "layerTypeFiles": len(layer_bundles),
            "layerLeafPropertiesByType": layer_count_summary,
            "renderSettingsTemplates": len(rq_settings.get("templates", [])),
            "renderSettingsKeys": len(rq_settings.get("stringSettable", {})),
            "outputModuleTemplates": len(om_settings.get("templates", [])),
            "outputModuleKeys": len(om_settings.get("stringSettable", {})),
            "shortcutContexts": shortcuts_raw["contextCount"],
            "shortcutCommands": shortcuts_raw["commandCount"],
            "tools": len(tools_list),
            "panelsCore": len(core_panels),
            "panelsAuxiliary": len(aux_panels),
            "panelsScriptUI": len(script_ui_panels),
            "workspaces": len(workspaces),
            "menuTopLevel": top_menu_count,
            "menuSubmenus": menu_submenu_count,
            "menuItems": menu_item_count,
            "preferenceSections": prefs_raw["sectionCount"],
            "preferenceKeys": prefs_raw["keyCount"],
        },
    }
    write_json(out_dir / "meta.json", meta_payload, compact=False)

    # Size budget check
    all_files = sorted(p for p in out_dir.rglob("*") if p.is_file())
    total_bytes = sum(p.stat().st_size for p in all_files)
    oversized = [(p, p.stat().st_size) for p in all_files if p.stat().st_size >= 1_000_000]
    print(f"Generated {len(all_files)} files under {out_dir}, total = {total_bytes:,} bytes ({total_bytes / 1024 / 1024:.2f} MB)")
    for p in all_files:
        sz = p.stat().st_size
        if sz > 100_000:
            print(f"  LARGE ({sz:,} B): {p.relative_to(out_dir)}")
    if oversized:
        for p, s in oversized:
            print(f"WARNING: File exceeds 1 MB: {p} ({s:,} bytes)")
    if total_bytes >= 5_000_000:
        print(f"WARNING: Total size exceeds 5 MB ({total_bytes:,} bytes)")


if __name__ == "__main__":
    main()
