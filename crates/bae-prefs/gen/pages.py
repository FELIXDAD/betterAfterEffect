"""Generates crates/bae-prefs/src/pages.rs from R1's AE 26 Preferences data.

    python3 -I crates/bae-prefs/gen/pages.py [docs/reference/ae/ui/preferences-dialog.json]

Every control becomes an item of `PAGES` (keys are `<page>.<control id>`), and
every English label with its Chinese text goes into `ZH`. Hand-made adaptations
(Choose Folder buttons bound to a path value, the macOS "Reveal in Finder" label,
dynamic defaults) are listed below.
"""

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SRC = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "docs/reference/ae/ui/preferences-dialog.json"
OUT = ROOT / "crates/bae-prefs/src/pages.rs"

# Read-only path displays become path values; their Choose button asks the host.
PATHS = {
    "project_template_path_display": ("project.project_template", "choose_project_template"),
    "auto_save_folder_display": ("project.auto_save_folder", "choose_auto_save_folder"),
    "disk_cache_folder_display": ("disk.disk_cache_folder", "choose_disk_cache_folder"),
}
CHOOSE = {button: key for key, button in PATHS.values()}
# Defaults AE computes from the machine.
DYNAMIC_DEFAULT = {"max_disk_cache_size_gb": 100, "ram_reserved_for_other_apps_gb": 6}
DYNAMIC_MAX = 1024
# English labels R1 recorded without Chinese text.
EXTRA_ZH = {"Setup...": "设置..."}
# BAE's own items (not in AE), first on their page: the UI language, switched
# without a restart. The label names the setting in both languages so it can be
# found whatever the current one is; languages are named in themselves.
BAE_ITEMS = {
    "appearance": (
        'Item::P(choice("appearance.language", "Language / 语言", '
        '&[Opt::new("system", "Follow System"), Opt::new("zh-CN", "简体中文"), Opt::new("en", "English")], '
        '"system", Style::Dropdown))',
        {"Language / 语言": "语言 / Language", "Follow System": "跟随系统", "简体中文": "简体中文", "English": "English"},
    ),
}

zh = {}
same = set()
conflicts = []


def tr(en, cn):
    cn = cn or EXTRA_ZH.get(en, "")
    if cn == en and any(ch.isalpha() for ch in en):
        same.add(en)
    if not en or not cn or cn == en:
        # Nothing to translate (numbers, "3D", or text R1 saw in English).
        return
    if en in zh and zh[en] != cn:
        conflicts.append((en, zh[en], cn))
        return
    zh.setdefault(en, cn)


def s(text):
    return json.dumps(text, ensure_ascii=False)


def num(v):
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def when(cond, page):
    if "all" in cond:
        return "When::All(&[" + ", ".join(when(c, page) for c in cond["all"]) + "])"
    key = f"{page}.{cond['control']}"
    v = cond["equals"]
    if v is True:
        return f"When::On({s(key)})"
    if v is False:
        return f"When::Off({s(key)})"
    return f"When::Is({s(key)}, {s(str(v))})"


def opts(options):
    out = []
    for o in options:
        tr(o.get("labelEn", ""), o.get("labelZh", ""))
        out.append(f"Opt::new({s(str(o['value']))}, {s(o.get('labelEn') or str(o['value']))})")
    return "&[" + ", ".join(out) + "]"


# Settings BAE shows but does not do yet: the dialog draws them greyed.
UNSUPPORTED = {"project.dynamic_link_highest_number", "export.preview_output_during_render_queue"}
UNSUPPORTED_GROUPS = {"Mercury Transmit"}
# Labels naming AE's apps, in BAE's words (with their Chinese).
LABELS = {"RAM Available for [AE/Pr/AME/Au]:": ("RAM Available for BAE:", "BAE 可用的 RAM:")}
GROUP = None


def pref(page, c):
    """Rust expression for a stored control, or None."""
    t = c["type"]
    key = f"{page}.{c['id']}"
    label = c.get("labelEn", "")
    tr(label, c.get("labelZh", ""))
    if t == "checkbox":
        e = f"b({s(key)}, {s(label)}, {str(bool(c.get('default'))).lower()})"
    elif t in ("number_input",):
        r = c.get("range", {})
        lo, hi, dv = r.get("min", 0), r.get("max", DYNAMIC_MAX), c.get("default")
        if not num(dv):
            dv = DYNAMIC_DEFAULT.get(c["id"], lo if num(lo) else 0)
        if not num(hi):
            hi = DYNAMIC_MAX
        if not num(lo):
            lo = 0
        if any(isinstance(x, float) for x in (lo, hi, dv)) and any(float(x) != int(x) for x in (lo, hi, dv)):
            dec = max(len(repr(float(x)).split(".")[1].rstrip("0") or "") for x in (lo, hi, dv))
            e = f"num({s(key)}, {s(label)}, {float(dv)!r}, {float(lo)!r}, {float(hi)!r}, {max(dec, 1)})"
        else:
            e = f"int({s(key)}, {s(label)}, {int(dv)}, {int(lo)}, {int(hi)})"
    elif t == "slider":
        r = c["range"]
        tr(c.get("leftLabelEn", ""), c.get("leftLabelZh", ""))
        tr(c.get("rightLabelEn", ""), c.get("rightLabelZh", ""))
        e = (f"slider({s(key)}, {s(label)}, {float(c['default'])!r}, {float(r['min'])!r}, {float(r['max'])!r}, "
             f"{s(c.get('leftLabelEn', ''))}, {s(c.get('rightLabelEn', ''))})")
    elif t in ("dropdown", "radio_group", "theme_picker"):
        if c.get("optionsRef") == "labelPalette17":
            e = f"label({s(key)}, {s(label)}, {int(c.get('default', 0))})"
        elif key == "audio.audio_default_output":
            e = f"audio_output({s(key)}, {s(label)}, {s(c['default'])})"
        else:
            options = c.get("options") or [{"value": c.get("default"), "labelEn": "System Default", "labelZh": "系统默认值"}]
            style = {"dropdown": "Style::Dropdown", "radio_group": "Style::Radio", "theme_picker": "Style::Theme"}[t]
            e = f"choice({s(key)}, {s(label)}, {opts(options)}, {s(str(c.get('default')))}, {style})"
    elif t == "timecode_input":
        e = f"timecode({s(key)}, {s(label)}, {s(c.get('default', '0:00:00:00'))})"
    elif t in ("color_swatch", "color_picker"):
        e = f"color({s(key)}, {s(label)}, {s(c['defaultColorHex'].lower())})"
    elif t == "text_input":
        tr(c.get("defaultEn", ""), c.get("defaultZh", ""))
        e = f"text({s(key)}, {s(label)}, {s(c.get('defaultEn', ''))})"
    else:
        return None
    unit = c.get("unitEn", "")
    if unit:
        tr(unit, c.get("unitZh", ""))
        e += f".unit({s(unit)})"
    if "enabledWhen" in c:
        e += f".when({when(c['enabledWhen'], page)})"
    elif ":" in c.get("inlineWith", ""):
        parent, value = c["inlineWith"].split(":", 1)
        e += f".when(When::Is({s(page + '.' + parent)}, {s(value)}))"
    if c.get("indented"):
        e += ".indent()"
    if c.get("inlineWith") or t == "radio_group":
        e += ".quiet()"
    if key in UNSUPPORTED or GROUP in UNSUPPORTED_GROUPS:
        e += ".unsupported()"
    pk = c.get("preferencesKey")
    if pk:
        e += f".ae({s(pk['section'] + '/' + pk['key'])})"
    return e


def item(page, c):
    t = c["type"]
    cid = c["id"]
    label = c.get("labelEn", "")
    tr(label, c.get("labelZh", ""))
    if cid in {"media_cache_database_choose", "media_cache_choose"}:
        key = "disk." + cid.removesuffix("_choose") + "_folder"
        return f'Item::Row(&[Item::Button({s("choose:" + key)}, {s(label)}), Item::P(path({s(key)}, {s(label)}).quiet())])'
    if cid in PATHS:
        key = PATHS[cid][0]
        e = f"path({s(key)}, {s(label)}).quiet()"
        if "enabledWhen" in c:
            e += f".when({when(c['enabledWhen'], page)})"
        return f"Item::P({e})"
    if t == "label_color_entry":
        i = c["index"]
        color = f"color({s(f'{page}.label_color_{i}')}, \"\", {s(c['defaultColorHex'].lower())}).ae({s(c['preferencesKey']['section'] + '/' + c['preferencesKey']['key'])})"
        name = f"text({s(f'{page}.label_name_{i}')}, \"\", {s(label)})"
        return f"Item::Row(&[Item::P({color}), Item::P({name})])"
    if t in ("button", "icon_button", "link"):
        bid = f"choose:{CHOOSE[cid]}" if cid in CHOOSE else f"{page}.{cid}"
        b = f"Item::Button({s(bid)}, {s(label)})"
        if "enabledWhen" in c:
            b = f"Item::When({when(c['enabledWhen'], page)}, &{b})"
        if cid == "reveal_preferences_in_explorer":
            tr("Reveal Preferences in Finder", "在访达中显示首选项")
            mac = f"Item::Button({s(bid)}, {s('Reveal Preferences in Finder')})"
            return f"Item::Only(Os::Windows, &{b}), Item::Only(Os::Mac, &{mac})"
        return b
    if t == "readonly_text":
        value = str(c.get("default", ""))
        if label in LABELS:
            label, cn = LABELS[label]
            tr(label, cn)
        return f"Item::Info({s(label)}, {s(value)})"
    if t == "table":
        cols = []
        for col in c.get("columns", []):
            tr(col.get("labelEn", ""), col.get("labelZh", ""))
            cols.append(s(col.get("labelEn", "")))
        t_item = f"Item::Table({s(label)}, &[{', '.join(cols)}])"
        if "enabledWhen" in c:
            t_item = f"Item::When({when(c['enabledWhen'], page)}, &{t_item})"
        return t_item
    if t == "code_preview":
        return f"Item::Preview({s(page)})"
    e = pref(page, c)
    if e is None:
        raise SystemExit(f"unknown control type {t} ({page}.{cid})")
    if c.get("visibleOnWindows") is False:
        return f"Item::Only(Os::Mac, &Item::P({e}))"
    return f"Item::P({e})"


def group_items(page, g):
    global GROUP
    GROUP = g["titleEn"]
    controls = g["controls"]
    rendered = {c["id"]: item(page, c) for c in controls}
    # Inline controls go on the line of the control they follow.
    inline = {}
    for c in controls:
        target = c.get("inlineWith")
        if target:
            inline.setdefault(target.split(":")[0], []).append(c["id"])
    skip = {i for ids in inline.values() for i in ids}
    layout = g.get("layout")
    has_col = any("column" in c or "layoutColumn" in c for c in controls)
    out = []
    cols = {}
    for c in controls:
        cid = c["id"]
        if cid in skip:
            continue
        e = rendered[cid]
        if cid in inline:
            e = "Item::Row(&[" + ", ".join([e] + [rendered[i] for i in inline[cid]]) + "])"
        col = c.get("column") or c.get("layoutColumn")
        if has_col and col is not None:
            if not cols:
                out.append("__COLS__")
            cols.setdefault(str(col), []).append(e)
        else:
            out.append(e)
    if cols:
        order = sorted(cols, key=lambda k: {"left": "0", "right": "1"}.get(k, k))
        cols_item = "Item::Cols(&[" + ", ".join("&[" + ", ".join(cols[k]) + "]" for k in order) + "])"
        out = [cols_item if e == "__COLS__" else e for e in out]
    if layout == "horizontal_row" and not cols:
        out = ["Item::Row(&[" + ", ".join(out) + "])"]
    elif layout == "two_column_grid" and not cols:
        out = ["Item::Row(&[" + ", ".join(out[i:i + 2]) + "])" for i in range(0, len(out), 2)]
    return out


def main():
    data = json.loads(SRC.read_text(encoding="utf-8"))
    tr(data["dialog"]["titleEn"], data["dialog"]["titleZh"])
    for b in data["dialog"]["buttons"]:
        tr(b["labelEn"], b["labelZh"])
    pages = []
    for p in data["pages"]:
        pid = p["id"]
        tr(p["labelEn"], p["labelZh"])
        for pal in (p.get("sharedDropdownOptions") or {}).values():
            for o in pal:
                tr(o["labelEn"], o["labelZh"])
        items = []
        if pid in BAE_ITEMS:
            extra, strings = BAE_ITEMS[pid]
            items.append(extra)
            for en, cn in strings.items():
                tr(en, cn)
        for g in p["groups"]:
            tr(g["titleEn"], g["titleZh"])
            inner = group_items(pid, g)
            if pid == "memory_cpu" and g["titleEn"] == "Memory":
                label = "GPU Preview Cache (MB)"
                tr(label, "显存预览缓存（MB）")
                inner.append(f'Item::P(int("memory_cpu.preview_cache_mib", {s(label)}, 1024, 0, 32768))')
            sub = [c for c in g["controls"] if c.get("subgroupEn")]
            for c in sub:
                tr(c["subgroupEn"], c.get("subgroupZh", ""))
            items.append(f"Item::Group({s(g['titleEn'])}, &[{', '.join(inner)}])")
        for c in p.get("footerControls") or []:
            tr(c.get("labelEn", ""), c.get("labelZh", ""))
            items.append(f"Item::Info({s(c['labelEn'])}, \"\")")
        pages.append(f"Page {{ id: {s(pid)}, title: {s(p['labelEn'])}, items: &[{', '.join(items)}] }}")
    rs = [
        "// @generated by crates/bae-prefs/gen/pages.py from",
        "// docs/reference/ae/ui/preferences-dialog.json (R1, AE 26.0 on Windows). Do not edit;",
        "// change the generator or the data and run it again.",
        "",
        "#![cfg_attr(rustfmt, rustfmt_skip)]",
        "",
        "use crate::schema::*;",
        "",
        "/// The 16 pages of AE 26's Preferences dialog, in order.",
        "pub static PAGES: &[Page] = &[",
        *[f"    {p}," for p in pages],
        "];",
        "",
        "/// Chinese text of every English label in [`PAGES`] (AE's zh_CN UI).",
        "pub const ZH: &[(&str, &str)] = &[",
        *[f"    ({s(en)}, {s(cn)})," for en, cn in sorted(zh.items())],
        "];",
        "",
        "/// Labels AE's Chinese UI shows in English (theme names, units, a few titles).",
        "pub const SAME_IN_ZH: &[&str] = &[",
        *[f"    {s(en)}," for en in sorted(same - set(zh))],
        "];",
        "",
    ]
    OUT.write_text("\n".join(rs), encoding="utf-8", newline="\n")
    for en, a, b in conflicts:
        print(f"note: {en!r} is {a!r} and also {b!r}; kept the first", file=sys.stderr)
    print(f"wrote {OUT.relative_to(ROOT)}: {len(pages)} pages, {len(zh)} strings")


main()
