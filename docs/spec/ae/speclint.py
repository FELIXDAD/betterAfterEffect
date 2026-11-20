#!/usr/bin/env python3
"""Lint and index the BAE After Effects feature inventory (docs/spec/ae/*.md).

Usage:
  python3 speclint.py FILE [FILE...] [--range 001-299,300-599] [--area PRJ]
      Lint the given files. Exit code 1 on errors.
  python3 speclint.py --readme [FILE...]
      Print the generated README tables (counts, P0 checklist, UNVERIFIED list).
      Without FILE arguments, all inventory files next to this script are used.
  python3 speclint.py --json [FILE...]
      Print the parsed items as JSON.

Item format (one item per feature):

  ### AE-PRJ-001 · English name (中文名)

  - **What it does:** ...
  - **Exact semantics:** ...
  - **Priority:** P0 | P1 | P2
  - **Owner:** <primary>[ (+ <secondary>, <secondary>)]
  - **Notes:** ...

The heading text after the ID is the item's Name field.
"""

import json
import os
import re
import sys

OWNERS = [
    "bae-model",
    "bae-anim",
    "bae-render",
    "bae-vector",
    "bae-text",
    "bae-color",
    "bae-effects",
    "bae-media",
    "bae-motion",
    "bae-ui",
    "bae-ui-motion",
    "bae-plugin",
    "bae-package",
    "apps/bae",
]
FIELDS = ["What it does", "Exact semantics", "Priority", "Owner", "Notes"]
INVENTORY_FILES = [
    "project-comp-layers.md",
    "transform-keyframes-expressions.md",
    "compositing-3d-color.md",
    "shapes.md",
    "text.md",
    "effects.md",
    "ui-preview-render-io.md",
]
HEAD_RE = re.compile(r"^### (AE-([A-Z]+)-(\d{3})) · (.+?)\s*$")
FIELD_RE = re.compile(r"^- \*\*([^*:]+):\*\*(.*)$")
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
                errors.append(f"{path}:{n}: bad item heading (want '### AE-XXX-NNN · Name (中文名)'): {line[:90]}")
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
        if m and m.group(1).strip() in FIELDS:
            name = m.group(1).strip()
            cur["fields"].append(name)
            cur["values"][name] = m.group(2).strip()
    for it in items:
        it["unverified"] = sum(l.count("UNVERIFIED") for l in it["body"]) + it["name"].count("UNVERIFIED")
        it["zh_pending"] = sum(l.count("译名待核") for l in it["body"]) + it["name"].count("译名待核")
        where = f"{it['file']}:{it['line']} {it['id']}"
        if it["fields"] != FIELDS:
            errors.append(f"{where}: fields {it['fields']} != required {FIELDS} (order and exactly once each)")
        pr = it["values"].get("Priority", "")
        m = re.match(r"^(P[012])\b", pr)
        if not m:
            errors.append(f"{where}: bad Priority '{pr}'")
            it["priority"] = "?"
        else:
            it["priority"] = m.group(1)
        ow = it["values"].get("Owner", "")
        m = re.match(r"^([a-z/\-]+)\s*(?:\(\+\s*([^)]*)\))?\s*$", ow)
        if not m:
            errors.append(f"{where}: bad Owner '{ow}' (want 'primary (+ a, b)')")
            it["owner"], it["secondary"] = "?", []
        else:
            it["owner"] = m.group(1)
            it["secondary"] = [s.strip() for s in (m.group(2) or "").split(",") if s.strip()]
            for o in [it["owner"]] + it["secondary"]:
                if o not in OWNERS:
                    errors.append(f"{where}: unknown owner '{o}'")
        if not CJK_RE.search(it["name"]):
            warnings.append(f"{where}: no Chinese term in heading")
        if not it["values"].get("What it does"):
            errors.append(f"{where}: empty 'What it does'")
        del it["body"]
    return items, errors, warnings


def parse_ranges(spec):
    out = []
    for part in spec.split(","):
        a, b = part.split("-")
        out.append((int(a), int(b)))
    return out


def lint(paths, ranges=None, area=None):
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
        if area and it["area"] != area:
            errors.append(f"{it['id']}: area should be {area}")
        if ranges and not any(a <= it["num"] <= b for a, b in ranges):
            errors.append(f"{it['id']}: outside allotted range(s) {ranges}")
    prev = {}
    for it in all_items:
        key = (it["file"], it["area"])
        if key in prev and it["num"] < prev[key]:
            warnings.append(f"{it['id']}: out of document order (allowed for later additions)")
        prev[key] = max(prev.get(key, 0), it["num"])
    return all_items, errors, warnings


def readme_tables(items):
    out = []
    pri = ["P0", "P1", "P2"]
    out.append("### Counts per owner (primary owner)\n")
    out.append("| Owner | P0 | P1 | P2 | Total |")
    out.append("|---|---:|---:|---:|---:|")
    tot = {p: 0 for p in pri}
    for o in OWNERS:
        c = {p: sum(1 for i in items if i["owner"] == o and i["priority"] == p) for p in pri}
        if sum(c.values()) == 0:
            continue
        for p in pri:
            tot[p] += c[p]
        out.append(f"| {o} | {c['P0']} | {c['P1']} | {c['P2']} | {sum(c.values())} |")
    out.append(f"| **Total** | **{tot['P0']}** | **{tot['P1']}** | **{tot['P2']}** | **{sum(tot.values())}** |")
    out.append("")
    out.append("### Counts per file\n")
    out.append("| File | Area | P0 | P1 | P2 | Total | Items with UNVERIFIED |")
    out.append("|---|---|---:|---:|---:|---:|---:|")
    files = []
    for i in items:
        if i["file"] not in files:
            files.append(i["file"])
    for f in files:
        fi = [i for i in items if i["file"] == f]
        c = {p: sum(1 for i in fi if i["priority"] == p) for p in pri}
        u = sum(1 for i in fi if i["unverified"])
        out.append(f"| [{f}]({f}) | {fi[0]['area']} | {c['P0']} | {c['P1']} | {c['P2']} | {len(fi)} | {u} |")
    out.append("")
    out.append("### P0 checklist\n")
    out.append("Tick `implemented?` (`[x]`) in the PR that lands the feature, and name the PR.\n")
    out.append("| ID | Name | Owner | Implemented? |")
    out.append("|---|---|---|---|")
    for i in items:
        if i["priority"] == "P0":
            sec = f" (+ {', '.join(i['secondary'])})" if i["secondary"] else ""
            name = i["name"].replace("|", "\\|")
            out.append(f"| [{i['id']}]({i['file']}) | {name} | {i['owner']}{sec} | [ ] |")
    out.append("")
    out.append("### Items still containing UNVERIFIED\n")
    unv = [i for i in items if i["unverified"]]
    if not unv:
        out.append("None.")
    else:
        out.append("| ID | Name | Priority | Owner | UNVERIFIED notes |")
        out.append("|---|---|---|---|---:|")
        for i in unv:
            name = i["name"].replace("|", "\\|")
            out.append(f"| [{i['id']}]({i['file']}) | {name} | {i['priority']} | {i['owner']} | {i['unverified']} |")
    return "\n".join(out) + "\n"


def main(argv):
    mode = "lint"
    ranges = area = None
    paths = []
    it = iter(argv)
    for a in it:
        if a == "--readme":
            mode = "readme"
        elif a == "--json":
            mode = "json"
        elif a == "--range":
            ranges = parse_ranges(next(it))
        elif a == "--area":
            area = next(it)
        else:
            paths.append(a)
    if not paths and mode != "lint":
        here = os.path.dirname(os.path.abspath(__file__))
        paths = [os.path.join(here, f) for f in INVENTORY_FILES if os.path.exists(os.path.join(here, f))]
    items, errors, warnings = lint(paths, ranges, area)
    if mode == "readme":
        sys.stdout.write(readme_tables(items))
    elif mode == "json":
        json.dump(items, sys.stdout, ensure_ascii=False, indent=1)
    else:
        for w in warnings:
            print("warning:", w)
        for e in errors:
            print("error:", e)
        n_unv = sum(1 for i in items if i["unverified"])
        n_zh = sum(1 for i in items if i["zh_pending"])
        pc = {p: sum(1 for i in items if i["priority"] == p) for p in ["P0", "P1", "P2"]}
        print(
            f"{len(items)} items ({pc['P0']} P0, {pc['P1']} P1, {pc['P2']} P2), "
            f"{n_unv} with UNVERIFIED, {n_zh} with 译名待核, {len(errors)} errors, {len(warnings)} warnings"
        )
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
