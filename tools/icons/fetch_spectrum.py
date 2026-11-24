#!/usr/bin/env python3
"""Copy the Spectrum 1 icons BAE uses from Adobe's open-source npm packages.

Downloads the two pinned tarballs from the npm registry, checks their sha512
integrity, and writes only the icons listed below into `assets/icons/spectrum/`
together with the packages' license and copyright files. Nothing is extracted to
disk; files are read from the tarball in memory.

    python3 tools/icons/fetch_spectrum.py   # then: python3 tools/icons/build_icons.py

Workflow icons are copied byte for byte. UI icons ship as React components, so
their path data for the medium scale is written into a small SVG wrapper (an
18×18 viewBox with the glyph centered); that wrapper is a modification and is
listed in `assets/icons/spectrum/README.md`, as are the tool-group variants in
`modified/` (a Spectrum glyph scaled down plus a small badge).
"""

import base64
import hashlib
import io
import re
import sys
import tarfile
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_icons  # noqa: E402  (same directory; geometry helpers)

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "assets" / "icons" / "spectrum"

WORKFLOW_PKG = (
    "@adobe/spectrum-css-workflow-icons",
    "1.5.4",
    "https://registry.npmjs.org/@adobe/spectrum-css-workflow-icons/-/spectrum-css-workflow-icons-1.5.4.tgz",
    "sha512-sZ19YOLGw5xTZzCEkVXPjf53lXVzo063KmDTJjpSjy/XLVsF+RaX0b436SfSM4hsIUZ7n27+UsbOvzFaFjcYXw==",
)
UI_PKG = (
    "@adobe/react-spectrum-ui",
    "1.2.1",
    "https://registry.npmjs.org/@adobe/react-spectrum-ui/-/react-spectrum-ui-1.2.1.tgz",
    "sha512-wcrbEE2O/9WnEn6avBnaVRRx88S5PLFsPLr4wffzlbMfXeQsy+RMQwaJd3cbzrn18/j04Isit7f7Emfn0dhrJA==",
)

# Spectrum 1 workflow icons used by bae-ui (file names in dist/18/).
WORKFLOW = [
    "BookmarkSingle", "Brush", "Camera", "ChevronDoubleRight", "CloneStamp", "Crosshairs", "Delete", "Dolly",
    "Effects", "Ellipse", "Erase", "Exposure", "Filmroll", "FlashOn", "Folder", "Hand",
    "Home", "Label", "Link", "LockClosed", "LockOpen", "Orbit", "Pan", "PinOn",
    "PlayCircle", "Polygon", "Rectangle", "RegionSelect", "RotateCW", "Sampler",
    "SaveFloppy", "Search", "Select", "Send", "ShowMenu", "Star", "StarOutline",
    "Stopwatch", "Text", "Transparency", "VectorDraw", "Visibility", "VolumeThree",
    "Workflow",
]
# Spectrum 1 UI icons (React components in src/), medium scale.
UI = ["ChevronDownMedium", "ChevronLeftMedium", "ChevronRightMedium", "CrossMedium"]
UI_BOX = 18


def fetch(pkg):
    name, version, url, integrity = pkg
    with urllib.request.urlopen(url) as r:
        data = r.read()
    algo, want = integrity.split("-", 1)
    got = base64.b64encode(hashlib.new(algo, data).digest()).decode()
    if got != want:
        sys.exit(f"{name}@{version}: integrity mismatch")
    return tarfile.open(fileobj=io.BytesIO(data), mode="r:gz")


def member(tar, path):
    f = tar.extractfile(f"package/{path}")
    if f is None:
        sys.exit(f"missing {path}")
    return f.read()


def ui_svg(source, name):
    m = re.search(r"scale === 'M' && \(\s*<path d=\"([^\"]+)\"", source)
    if not m:
        sys.exit(f"{name}: no medium path")
    d = m.group(1)
    pts = [p for sp in build_icons.parse_path(d) for p in build_icons.flatten(sp)]
    xs, ys = [p[0] for p in pts], [p[1] for p in pts]
    dx = round((UI_BOX - (max(xs) - min(xs))) / 2 - min(xs), 3)
    dy = round((UI_BOX - (max(ys) - min(ys))) / 2 - min(ys), 3)
    return (
        f"<!-- {name} (medium) from {UI_PKG[0]}@{UI_PKG[1]}, Apache-2.0. Modified by BAE: "
        f"path data wrapped in a {UI_BOX}x{UI_BOX} SVG, glyph centered. -->\n"
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {UI_BOX} {UI_BOX}">'
        f'<path transform="translate({dx} {dy})" d="{d}"/></svg>\n'
    )


# Tool-group variants AE has but Spectrum does not: a Spectrum glyph scaled into the
# top-left corner plus a small badge (BAE's own drawing) in the bottom-right corner.
STROKE = 'fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"'
BADGES = {
    "add": f'<path d="M24.5 29.5h9M29 25v9" {STROKE}/>',
    "delete": f'<path d="M24.5 29.5h9" {STROKE}/>',
    "convert": f'<path d="M24.5 33 29 25.5l4.5 7.5" {STROKE}/>',
    "feather": f'<path d="M23.5 34c1.5-5 5-8.5 10.5-9.5" {STROKE}/><path d="M27 34c1-2.5 3-4.5 7-5.5" {STROKE}/>',
    "down": f'<path d="M30 6v24M26 26l4 5 4-5" {STROKE}/>',
    "starch": f'<rect x="24.5" y="24.5" width="9" height="9" rx="1" {STROKE}/>',
    "bend": f'<path d="M25 33a5 5 0 1 1 8.5-3.5" {STROKE}/><path d="M34.5 26.5v3.5H31" {STROKE}/>',
    "advanced": f'<circle cx="29" cy="29" r="4.5" {STROKE}/>',
    "overlap": f'<path d="M24.5 27h9M24.5 32h9" {STROKE}/>',
}
COMPOSED = {
    "PenAdd": ("VectorDraw", "add"),
    "PenDelete": ("VectorDraw", "delete"),
    "PenConvert": ("VectorDraw", "convert"),
    "MaskFeather": ("VectorDraw", "feather"),
    "TypeVertical": ("Text", "down"),
    "PuppetStarch": ("PinOn", "starch"),
    "PuppetBend": ("PinOn", "bend"),
    "PuppetAdvanced": ("PinOn", "advanced"),
    "PuppetOverlap": ("PinOn", "overlap"),
}


def composed_svg(name, base, badge):
    src = (OUT / "workflow" / f"{base}.svg").read_text()
    inner = re.search(r"<svg[^>]*>(.*)</svg>", src, re.S).group(1)
    return (
        f"<!-- {name}: Spectrum workflow icon {base} from {WORKFLOW_PKG[0]}@{WORKFLOW_PKG[1]}, "
        f"Apache-2.0. Modified by BAE: scaled to 75 % in the top-left corner, '{badge}' badge "
        f"(BAE's own drawing) added. Generated by tools/icons/fetch_spectrum.py. -->\n"
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 36 36">'
        f'<g transform="translate(0.5 0.5) scale(0.75)">{inner}</g>{BADGES[badge]}</svg>\n'
    )


def main():
    (OUT / "workflow").mkdir(parents=True, exist_ok=True)
    (OUT / "ui").mkdir(parents=True, exist_ok=True)
    wf = fetch(WORKFLOW_PKG)
    for name in WORKFLOW:
        (OUT / "workflow" / f"{name}.svg").write_bytes(member(wf, f"dist/18/{name}.svg"))
    (OUT / "LICENSE").write_bytes(member(wf, "LICENSE"))
    (OUT / "COPYRIGHT").write_bytes(member(wf, "COPYRIGHT"))
    ui = fetch(UI_PKG)
    # Both packages are under the Apache License 2.0; the copies differ only in the
    # appendix's sample copyright line, so one LICENSE file covers both.
    for name in UI:
        (OUT / "ui" / f"{name}.svg").write_text(ui_svg(member(ui, f"src/{name}.js").decode(), name))
    (OUT / "modified").mkdir(exist_ok=True)
    for name, (base, badge) in COMPOSED.items():
        (OUT / "modified" / f"{name}.svg").write_text(composed_svg(name, base, badge))
    print(f"{len(WORKFLOW)} workflow + {len(UI)} UI icons -> {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
