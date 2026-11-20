#!/usr/bin/env python3
"""Lint and index the BAE Premiere Pro feature inventory (docs/spec/pr/*.md).

Derived from ../ae/speclint.py (same idea: one item per feature, fixed fields,
generated README tables). Differences: the fields are Chinese, priorities are BAE
milestones (M1..M4, 以后) instead of P0..P2, and the owners include the edit crates.

Usage:
  python3 speclint.py [FILE...]          Lint (default: every inventory file). Exit 1 on errors.
  python3 speclint.py --readme [FILE...] Print the generated README tables.
  python3 speclint.py --json [FILE...]   Print the parsed items as JSON.

Item format (one item per feature):

  ### PR-TL-001 · 中文名（English name）

  - **行为要点：** what Premiere Pro does, the details that make it feel right
  - **优先级：** M1 | M2 | M3 | M4 | 以后 — optional explanation
  - **BAE 模块：** <primary>[ (+ <secondary>, <secondary>)]
  - **数据模型：** effect on docs/spec/data-model.md / edit-model.md, or 无
  - **备注：** optional, last

`待核` anywhere in an item marks a fact or a Chinese term not yet checked against
Premiere Pro 2026 (26.2); it is counted in the report.
"""

import json
import os
import re
import sys

OWNERS = [
    "bae-edit",
    "bae-ui-edit",
    "bae-ui-motion",
    "bae-model",
    "bae-anim",
    "bae-render",
    "bae-media",
    "bae-color",
    "bae-effects",
    "bae-text",
    "bae-vector",
    "bae-motion",
    "bae-ui",
    "bae-package",
    "bae-agent",
    "bae-plugin",
    "bae-sync",
    "bae-interop-edit",
    "apps/bae",
]
REQUIRED = ["行为要点", "优先级", "BAE 模块", "数据模型"]
OPTIONAL = ["备注"]
PRIORITIES = ["M1", "M2", "M3", "M4", "以后"]
INVENTORY_FILES = [
    "project-sequence.md",
    "monitors.md",
    "timeline-editing.md",
    "effects-transitions.md",
    "keyframes.md",
    "audio.md",
    "color.md",
    "graphics-captions.md",
    "markers.md",
    "proxy-render-export.md",
    "interchange.md",
    "shortcuts.md",
]
HEAD_RE = re.compile(r"^### (PR-([A-Z]+)-(\d{3})) · (.+?)\s*$")
FIELD_RE = re.compile(r"^- \*\*([^*]+?)[：:]\*\*(.*)$")
CJK_RE = re.compile(r"[㐀-鿿]")
FENCE_RE = re.compile(r"^\s*(```|~~~)")


def parse(path):
    items, errors, warnings = [], [], []
    with open(path, encoding="utf-8") as f:
        lines = f.read().split("\n")
    cur = None
    in_fence = False
    for n, line in enumerate(lines, 1):
        if FENCE_RE.match(line):
            in_fence = not in_fence
            if cur is not None:
                cur["body"].append(line)
            continue
        if in_fence:
            if cur is not None:
                cur["body"].append(line)
            continue
        if line.startswith("### "):
            m = HEAD_RE.match(line)
            if not m:
                errors.append(f"{path}:{n}: bad item heading (want '### PR-XXX-NNN · 中文名（English）'): {line[:90]}")
                cur = None
                continue
            cur = {
                "id": m.group(1),
                "area": m.group(2),
                "num": int(m.group(3)),
                "name": m.group(4),
                "file": os.path.basename(path),
                "line": n,
                "fields": [],
                "values": {},
                "body": [],
            }
            items.append(cur)
            continue
        if line.startswith("## ") or line.startswith("# "):
            cur = None
            continue
        if line.startswith("#### "):
            if cur is not None:
                warnings.append(f"{path}:{n}: '####' heading inside item {cur['id']} (use bold text instead)")
            continue
        if cur is None:
            continue
        cur["body"].append(line)
        m = FIELD_RE.match(line)
        if m and m.group(1).strip() in REQUIRED + OPTIONAL:
            name = m.group(1).strip()
            cur["fields"].append(name)
            cur["values"][name] = m.group(2).strip()
    for it in items:
        it["pending"] = sum(l.count("待核") for l in it["body"]) + it["name"].count("待核")
        where = f"{it['file']}:{it['line']} {it['id']}"
        want = REQUIRED + [f for f in OPTIONAL if f in it["fields"]]
        if it["fields"] != want:
            errors.append(f"{where}: fields {it['fields']} != {REQUIRED} (+ optional {OPTIONAL} last), each once")
        pr = it["values"].get("优先级", "")
        m = re.match(r"^(M[1-4]|以后)", pr)
        if not m:
            errors.append(f"{where}: bad 优先级 '{pr}' (want one of {PRIORITIES})")
            it["priority"] = "?"
        else:
            it["priority"] = m.group(1)
        ow = it["values"].get("BAE 模块", "")
        m = re.match(r"^([a-z/\-]+)\s*(?:\(\+\s*([^)]*)\))?\s*$", ow)
        if not m:
            errors.append(f"{where}: bad BAE 模块 '{ow}' (want 'primary (+ a, b)')")
            it["owner"], it["secondary"] = "?", []
        else:
            it["owner"] = m.group(1)
            it["secondary"] = [s.strip() for s in (m.group(2) or "").split(",") if s.strip()]
            for o in [it["owner"]] + it["secondary"]:
                if o not in OWNERS:
                    errors.append(f"{where}: unknown owner '{o}'")
        if not CJK_RE.search(it["name"]):
            warnings.append(f"{where}: no Chinese term in heading")
        for f in REQUIRED:
            if not it["values"].get(f):
                errors.append(f"{where}: empty '{f}'")
        del it["body"]
    return items, errors, warnings


def lint(paths):
    all_items, errors, warnings = [], [], []
    for p in paths:
        items, e, w = parse(p)
        all_items += items
        errors += e
        warnings += w
    seen = {}
    for it in all_items:
        if it["id"] in seen:
            errors.append(f"duplicate ID {it['id']} ({seen[it['id']]} and {it['file']}:{it['line']})")
        seen[it["id"]] = f"{it['file']}:{it['line']}"
    prev = {}
    for it in all_items:
        key = (it["file"], it["area"])
        if key in prev and it["num"] < prev[key]:
            warnings.append(f"{it['id']}: out of document order (allowed for later additions)")
        prev[key] = max(prev.get(key, 0), it["num"])
    return all_items, errors, warnings


def readme_tables(items):
    out = []
    out.append("### 按文件统计\n")
    out.append("| 文件 | 区域 | " + " | ".join(PRIORITIES) + " | 合计 | 含“待核” |")
    out.append("|---|---|" + "---:|" * (len(PRIORITIES) + 2))
    files = []
    for i in items:
        if i["file"] not in files:
            files.append(i["file"])
    tot = {p: 0 for p in PRIORITIES}
    for f in files:
        fi = [i for i in items if i["file"] == f]
        areas = []
        for i in fi:
            if i["area"] not in areas:
                areas.append(i["area"])
        c = {p: sum(1 for i in fi if i["priority"] == p) for p in PRIORITIES}
        for p in PRIORITIES:
            tot[p] += c[p]
        u = sum(1 for i in fi if i["pending"])
        out.append(f"| [{f}]({f}) | {'、'.join(areas)} | " + " | ".join(str(c[p]) for p in PRIORITIES) + f" | {len(fi)} | {u} |")
    out.append("| **合计** | | " + " | ".join(f"**{tot[p]}**" for p in PRIORITIES) + f" | **{len(items)}** | **{sum(1 for i in items if i['pending'])}** |")
    out.append("")
    out.append("### 按主负责模块统计\n")
    out.append("| 模块 | " + " | ".join(PRIORITIES) + " | 合计 |")
    out.append("|---|" + "---:|" * (len(PRIORITIES) + 1))
    for o in OWNERS:
        c = {p: sum(1 for i in items if i["owner"] == o and i["priority"] == p) for p in PRIORITIES}
        if sum(c.values()) == 0:
            continue
        out.append(f"| {o} | " + " | ".join(str(c[p]) for p in PRIORITIES) + f" | {sum(c.values())} |")
    out.append("")
    out.append("### M1 清单（0.0.1 框架必须做或预留）\n")
    out.append("落地的 PR 里把“已做”改成 `[x]` 并写上 PR 号。\n")
    out.append("| ID | 名称 | 模块 | 已做 |")
    out.append("|---|---|---|---|")
    for i in items:
        if i["priority"] == "M1":
            sec = f" (+ {', '.join(i['secondary'])})" if i["secondary"] else ""
            name = i["name"].replace("|", "\\|")
            out.append(f"| [{i['id']}]({i['file']}) | {name} | {i['owner']}{sec} | [ ] |")
    out.append("")
    return "\n".join(out) + "\n"


def main(argv):
    mode = "lint"
    paths = []
    for a in argv:
        if a == "--readme":
            mode = "readme"
        elif a == "--json":
            mode = "json"
        else:
            paths.append(a)
    if not paths:
        here = os.path.dirname(os.path.abspath(__file__))
        paths = [os.path.join(here, f) for f in INVENTORY_FILES if os.path.exists(os.path.join(here, f))]
    items, errors, warnings = lint(paths)
    if mode == "readme":
        sys.stdout.write(readme_tables(items))
    elif mode == "json":
        json.dump(items, sys.stdout, ensure_ascii=False, indent=1)
    else:
        for w in warnings:
            print("warning:", w)
        for e in errors:
            print("error:", e)
        pc = {p: sum(1 for i in items if i["priority"] == p) for p in PRIORITIES}
        n_pending = sum(1 for i in items if i["pending"])
        print(
            f"{len(items)} items ("
            + ", ".join(f"{pc[p]} {p}" for p in PRIORITIES)
            + f"), {n_pending} with 待核, {len(errors)} errors, {len(warnings)} warnings"
        )
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
