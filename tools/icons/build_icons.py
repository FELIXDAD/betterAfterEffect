#!/usr/bin/env python3
"""Convert the UI icon SVGs into the compact fill-path blob that bae-ui draws.

The app never parses SVG at run time. This one-shot script reads the icons listed
in a manifest, flattens every SVG shape into filled outlines (moves, lines,
quadratic and cubic curves) and writes them to a small binary file that
`crates/bae-ui/src/icons/vector.rs` loads, rasterizes at the exact pixel size on
screen and tints with the theme color.

Usage (from the repository root, Python 3.9+, standard library only):

    python3 tools/icons/build_icons.py                  # rebuild assets/icons/icons.bin
    python3 tools/icons/build_icons.py --preview out.png # also write a contact sheet
    python3 tools/icons/build_icons.py --check           # fail if icons.bin is stale

Manifest (`assets/icons/manifest.json`): `{"icons": {"<IconVariant>": "<svg path>"}}`,
SVG paths relative to `assets/icons/`. The key is the `bae_ui::icons::Icon` variant
name.

Supported SVG subset: <svg viewBox>, <g>, <path>, <rect> (rx/ry), <circle>,
<ellipse>, <polygon>, <polyline>, <line>; the `transform`, `fill`, `opacity`,
`fill-opacity`, `stroke`, `stroke-width`, `stroke-linecap` and `stroke-linejoin`
attributes. Fills use the nonzero rule (`fill-rule="evenodd"` is converted by
nesting depth). Strokes are expanded into fills (miter, bevel or round joins;
butt, round or square caps; circles, ellipses and rounded rectangles become exact
rings). `fill="currentColor"` or no fill means "theme color"; any other fill
color is kept as a fixed color for that layer.

The file is zlib-compressed (the app inflates it once with flate2, which the
project-file code already links). Blob format after inflating (little endian),
version 1:
    b"BAEI" u8 version u16 icon_count
    per icon: u8 name_len, name (utf-8), u16 view_box_size (design units),
              u8 layer_count
    per layer: u32 rgba (0 = theme color), u8 alpha, u16 word_count,
               word_count × i16 path words
    path words: opcode (0 move, 1 line, 2 quad, 3 cubic, 4 close) followed by its
                points, coordinates in 1/32 design unit.
"""

import argparse
import json
import math
import re
import struct
import sys
import xml.etree.ElementTree as ET
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ICON_DIR = ROOT / "assets" / "icons"
MANIFEST = ICON_DIR / "manifest.json"
OUTPUT = ICON_DIR / "icons.bin"

FIXED = 32  # sub-units per design unit
VERSION = 1
OP_MOVE, OP_LINE, OP_QUAD, OP_CUBIC, OP_CLOSE = range(5)


# ---------------------------------------------------------------- geometry ----

def mat_mul(a, b):
    """Compose affine matrices (a after b); matrices are (a, b, c, d, e, f)."""
    return (
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    )


IDENTITY = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)


def apply(m, p):
    return (m[0] * p[0] + m[2] * p[1] + m[4], m[1] * p[0] + m[3] * p[1] + m[5])


def parse_transform(text):
    m = IDENTITY
    for name, args in re.findall(r"(\w+)\s*\(([^)]*)\)", text or ""):
        v = [float(x) for x in re.findall(NUMBER, args)]
        if name == "matrix":
            t = tuple(v)
        elif name == "translate":
            t = (1, 0, 0, 1, v[0], v[1] if len(v) > 1 else 0)
        elif name == "scale":
            t = (v[0], 0, 0, v[1] if len(v) > 1 else v[0], 0, 0)
        elif name == "rotate":
            a = math.radians(v[0])
            r = (math.cos(a), math.sin(a), -math.sin(a), math.cos(a), 0, 0)
            if len(v) == 3:
                t = mat_mul((1, 0, 0, 1, v[1], v[2]), mat_mul(r, (1, 0, 0, 1, -v[1], -v[2])))
            else:
                t = r
        elif name == "skewX":
            t = (1, 0, math.tan(math.radians(v[0])), 1, 0, 0)
        elif name == "skewY":
            t = (1, math.tan(math.radians(v[0])), 0, 1, 0, 0)
        else:
            raise ValueError(f"unsupported transform {name}")
        m = mat_mul(m, t)
    return m


NUMBER = r"[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?"


def arc_to_cubics(p0, rx, ry, phi, large, sweep, p1):
    """SVG elliptical arc → list of cubic segments (c1, c2, end)."""
    if p0 == p1:
        return []
    rx, ry = abs(rx), abs(ry)
    if rx == 0 or ry == 0:
        return [(p0, p1, p1)]
    cphi, sphi = math.cos(phi), math.sin(phi)
    dx, dy = (p0[0] - p1[0]) / 2, (p0[1] - p1[1]) / 2
    x1p, y1p = cphi * dx + sphi * dy, -sphi * dx + cphi * dy
    lam = (x1p / rx) ** 2 + (y1p / ry) ** 2
    if lam > 1:
        s = math.sqrt(lam)
        rx, ry = rx * s, ry * s
    num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p
    den = rx * rx * y1p * y1p + ry * ry * x1p * x1p
    coef = math.sqrt(max(0.0, num / den)) if den else 0.0
    if large == sweep:
        coef = -coef
    cxp, cyp = coef * rx * y1p / ry, -coef * ry * x1p / rx
    cx = cphi * cxp - sphi * cyp + (p0[0] + p1[0]) / 2
    cy = sphi * cxp + cphi * cyp + (p0[1] + p1[1]) / 2

    def angle(ux, uy, vx, vy):
        a = math.atan2(ux * vy - uy * vx, ux * vx + uy * vy)
        return a

    t1 = angle(1, 0, (x1p - cxp) / rx, (y1p - cyp) / ry)
    dt = angle((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry)
    if not sweep and dt > 0:
        dt -= 2 * math.pi
    elif sweep and dt < 0:
        dt += 2 * math.pi
    n = max(1, math.ceil(abs(dt) / (math.pi / 2) - 1e-9))
    step = dt / n
    k = 4 / 3 * math.tan(step / 4)
    out = []

    def point(t):
        x, y = rx * math.cos(t), ry * math.sin(t)
        return (cphi * x - sphi * y + cx, sphi * x + cphi * y + cy)

    def deriv(t):
        x, y = -rx * math.sin(t), ry * math.cos(t)
        return (cphi * x - sphi * y, sphi * x + cphi * y)

    t = t1
    for i in range(n):
        a, b = t, t + step
        pa, pb = point(a), point(b)
        da, db = deriv(a), deriv(b)
        c1 = (pa[0] + k * da[0], pa[1] + k * da[1])
        c2 = (pb[0] - k * db[0], pb[1] - k * db[1])
        out.append((c1, c2, p1 if i == n - 1 else pb))
        t = b
    return out


def parse_path(d):
    """Path data → list of subpaths; each is (start, [segment], closed).

    A segment is ("L", p) | ("Q", c, p) | ("C", c1, c2, p) in absolute coordinates.
    """
    tokens = re.findall(r"[MmZzLlHhVvCcSsQqTtAa]|" + NUMBER, d)
    i = 0
    subpaths = []
    cur = (0.0, 0.0)
    start = (0.0, 0.0)
    segs = None
    cmd = None
    last_ctrl = None
    last_cmd = None

    def num():
        nonlocal i
        v = float(tokens[i])
        i += 1
        return v

    def flag():
        # Arc flags may be packed without separators ("a1 1 0 01.5 2").
        nonlocal i
        t = tokens[i]
        if t[0] in "01" and len(t) > 1:
            tokens[i] = t[1:]
            return int(t[0])
        i += 1
        return int(float(t))

    def finish(closed):
        nonlocal segs
        if segs is not None and segs:
            subpaths.append((start, segs, closed))
        segs = None

    while i < len(tokens):
        t = tokens[i]
        if re.match(r"[A-Za-z]", t):
            cmd = t
            i += 1
            if cmd in "Zz":
                finish(True)
                cur = start
                last_cmd = "Z"
                continue
        elif cmd is None:
            raise ValueError("path data must start with a command")
        rel = cmd.islower()
        c = cmd.upper()

        def P(x, y):
            return (cur[0] + x, cur[1] + y) if rel else (x, y)

        if c == "M":
            finish(False)
            cur = P(num(), num())
            start = cur
            segs = []
            cmd = "l" if rel else "L"
            last_ctrl = None
            last_cmd = "M"
            continue
        if segs is None:
            segs = []
            start = cur
        if c == "L":
            cur = P(num(), num())
            segs.append(("L", cur))
            last_ctrl = None
        elif c == "H":
            x = num()
            cur = (cur[0] + x if rel else x, cur[1])
            segs.append(("L", cur))
            last_ctrl = None
        elif c == "V":
            y = num()
            cur = (cur[0], cur[1] + y if rel else y)
            segs.append(("L", cur))
            last_ctrl = None
        elif c == "C":
            c1 = P(num(), num())
            c2 = P(num(), num())
            p = P(num(), num())
            segs.append(("C", c1, c2, p))
            cur, last_ctrl, last_cmd = p, c2, "C"
            continue
        elif c == "S":
            c1 = (2 * cur[0] - last_ctrl[0], 2 * cur[1] - last_ctrl[1]) if last_cmd == "C" else cur
            c2 = P(num(), num())
            p = P(num(), num())
            segs.append(("C", c1, c2, p))
            cur, last_ctrl, last_cmd = p, c2, "C"
            continue
        elif c == "Q":
            q = P(num(), num())
            p = P(num(), num())
            segs.append(("Q", q, p))
            cur, last_ctrl, last_cmd = p, q, "Q"
            continue
        elif c == "T":
            q = (2 * cur[0] - last_ctrl[0], 2 * cur[1] - last_ctrl[1]) if last_cmd == "Q" else cur
            p = P(num(), num())
            segs.append(("Q", q, p))
            cur, last_ctrl, last_cmd = p, q, "Q"
            continue
        elif c == "A":
            rx, ry, rot = num(), num(), num()
            large, sweep = flag(), flag()
            p = P(num(), num())
            for c1, c2, e in arc_to_cubics(cur, rx, ry, math.radians(rot), large, sweep, p):
                segs.append(("C", c1, c2, e))
            cur = p
            last_ctrl = None
        last_cmd = c
    finish(False)
    return subpaths


def ellipse_path(cx, cy, rx, ry):
    k = 0.5522847498
    return [
        (
            (cx + rx, cy),
            [
                ("C", (cx + rx, cy + k * ry), (cx + k * rx, cy + ry), (cx, cy + ry)),
                ("C", (cx - k * rx, cy + ry), (cx - rx, cy + k * ry), (cx - rx, cy)),
                ("C", (cx - rx, cy - k * ry), (cx - k * rx, cy - ry), (cx, cy - ry)),
                ("C", (cx + k * rx, cy - ry), (cx + rx, cy - k * ry), (cx + rx, cy)),
            ],
            True,
        )
    ]


def rect_path(x, y, w, h, rx, ry):
    if rx <= 0 and ry <= 0:
        return [((x, y), [("L", (x + w, y)), ("L", (x + w, y + h)), ("L", (x, y + h))], True)]
    rx = min(rx or ry, w / 2)
    ry = min(ry or rx, h / 2)
    k = 0.5522847498
    kx, ky = k * rx, k * ry
    return [
        (
            (x + rx, y),
            [
                ("L", (x + w - rx, y)),
                ("C", (x + w - rx + kx, y), (x + w, y + ry - ky), (x + w, y + ry)),
                ("L", (x + w, y + h - ry)),
                ("C", (x + w, y + h - ry + ky), (x + w - rx + kx, y + h), (x + w - rx, y + h)),
                ("L", (x + rx, y + h)),
                ("C", (x + rx - kx, y + h), (x, y + h - ry + ky), (x, y + h - ry)),
                ("L", (x, y + ry)),
                ("C", (x, y + ry - ky), (x + rx - kx, y), (x + rx, y)),
            ],
            True,
        )
    ]


def flatten(subpath, tol=0.02):
    """Subpath → polyline points (for strokes and the preview rasterizer)."""
    start, segs, _closed = subpath
    pts = [start]
    cur = start
    for s in segs:
        if s[0] == "L":
            pts.append(s[1])
            cur = s[1]
            continue
        if s[0] == "Q":
            c1 = (cur[0] + 2 / 3 * (s[1][0] - cur[0]), cur[1] + 2 / 3 * (s[1][1] - cur[1]))
            c2 = (s[2][0] + 2 / 3 * (s[1][0] - s[2][0]), s[2][1] + 2 / 3 * (s[1][1] - s[2][1]))
            ctrl = (cur, c1, c2, s[2])
        else:
            ctrl = (cur, s[1], s[2], s[3])
        dd = max(
            math.hypot(ctrl[0][0] - 2 * ctrl[1][0] + ctrl[2][0], ctrl[0][1] - 2 * ctrl[1][1] + ctrl[2][1]),
            math.hypot(ctrl[1][0] - 2 * ctrl[2][0] + ctrl[3][0], ctrl[1][1] - 2 * ctrl[2][1] + ctrl[3][1]),
        )
        n = max(1, math.ceil(math.sqrt(dd * 0.75 / tol)))
        for k in range(1, n + 1):
            t = k / n
            mt = 1 - t
            pts.append(
                (
                    mt ** 3 * ctrl[0][0] + 3 * mt * mt * t * ctrl[1][0] + 3 * mt * t * t * ctrl[2][0] + t ** 3 * ctrl[3][0],
                    mt ** 3 * ctrl[0][1] + 3 * mt * mt * t * ctrl[1][1] + 3 * mt * t * t * ctrl[2][1] + t ** 3 * ctrl[3][1],
                )
            )
        cur = ctrl[3]
    return pts


def signed_area(pts):
    return sum(pts[i][0] * pts[(i + 1) % len(pts)][1] - pts[(i + 1) % len(pts)][0] * pts[i][1] for i in range(len(pts))) / 2


def poly_subpath(pts):
    """Closed polygon with positive orientation."""
    if signed_area(pts) < 0:
        pts = pts[::-1]
    return (pts[0], [("L", p) for p in pts[1:]], True)


def reverse_subpath(sp):
    start, segs, closed = sp
    pts = [start] + [s[-1] for s in segs]
    out = []
    for i in range(len(segs) - 1, -1, -1):
        s = segs[i]
        end = pts[i]
        if s[0] == "L":
            out.append(("L", end))
        elif s[0] == "Q":
            out.append(("Q", s[1], end))
        else:
            out.append(("C", s[2], s[1], end))
    return (pts[-1], out, closed)


def inside(pt, poly):
    x, y = pt
    c = False
    for (x0, y0), (x1, y1) in zip(poly, poly[1:] + poly[:1]):
        if (y0 > y) != (y1 > y) and x < (x1 - x0) * (y - y0) / (y1 - y0) + x0:
            c = not c
    return c


def evenodd_to_nonzero(subpaths):
    """Orient subpaths by nesting depth so the nonzero rule gives the even-odd result.

    Valid for icon outlines whose subpaths do not cross each other (holes and
    islands nest), which is what the even-odd icons in the sources use.
    """
    polys = [flatten(sp, 0.05) for sp in subpaths]
    out = []
    for i, sp in enumerate(subpaths):
        probe = polys[i][0]
        depth = sum(1 for j, poly in enumerate(polys) if j != i and inside(probe, poly))
        positive = signed_area(polys[i]) > 0
        out.append(sp if positive == (depth % 2 == 0) else reverse_subpath(sp))
    return out


def stroke_subpaths(subpaths, width, cap, join):
    """Expand a stroke into filled subpaths that union under the nonzero rule.

    Every piece has positive orientation: a quad per flattened segment, a join
    piece at each corner (miter with limit 4, bevel, or round = disc for turns
    over 15 degrees) and discs at open ends for round caps.
    """
    out = []
    h = width / 2
    for sp in subpaths:
        pts = flatten(sp, 0.08)
        closed = sp[2]
        if closed and pts[0] != pts[-1]:
            pts.append(pts[0])
        dedup = [pts[0]]
        for p in pts[1:]:
            if math.hypot(p[0] - dedup[-1][0], p[1] - dedup[-1][1]) > 1e-6:
                dedup.append(p)
        pts = dedup
        if len(pts) < 2:
            continue
        normals = []
        for a, b in zip(pts, pts[1:]):
            dx, dy = b[0] - a[0], b[1] - a[1]
            ln = math.hypot(dx, dy)
            nx, ny = -dy / ln * h, dx / ln * h
            normals.append((nx, ny))
            out.append(poly_subpath([(a[0] + nx, a[1] + ny), (b[0] + nx, b[1] + ny), (b[0] - nx, b[1] - ny), (a[0] - nx, a[1] - ny)]))
        corners = list(range(1, len(pts) - 1))
        if closed:
            corners.append(0)
        for i in corners:
            n0, n1 = normals[i - 1], normals[i % len(normals)]
            p = pts[i]
            cross = n0[0] * n1[1] - n0[1] * n1[0]
            dot = (n0[0] * n1[0] + n0[1] * n1[1]) / (h * h)
            if abs(cross) < 1e-9 and dot > 0:
                continue
            turn = math.degrees(math.acos(max(-1.0, min(1.0, dot))))
            # The outer side is where the offset edges open a gap.
            sgn = -1 if cross > 0 else 1
            o0 = (p[0] + sgn * n0[0], p[1] + sgn * n0[1])
            o1 = (p[0] + sgn * n1[0], p[1] + sgn * n1[1])
            if join == "round" and turn > 15:
                out.extend(ellipse_path(p[0], p[1], h, h))
                continue
            miter_len = 1 / max(math.cos(math.radians(turn) / 2), 1e-6)
            if join == "miter" and miter_len <= 4:
                bis = (n0[0] + n1[0], n0[1] + n1[1])
                bl = math.hypot(*bis)
                m = (p[0] + sgn * bis[0] / bl * h * miter_len, p[1] + sgn * bis[1] / bl * h * miter_len)
                out.append(poly_subpath([p, o0, m, o1]))
            else:
                out.append(poly_subpath([p, o0, o1]))
        if not closed and cap == "round":
            for p in (pts[0], pts[-1]):
                out.extend(ellipse_path(p[0], p[1], h, h))
        elif not closed and cap == "square":
            for end, nrm, d in ((pts[0], normals[0], -1), (pts[-1], normals[-1], 1)):
                tx, ty = nrm[1] * d, -nrm[0] * d
                out.append(poly_subpath([(end[0] + nrm[0], end[1] + nrm[1]), (end[0] + nrm[0] + tx, end[1] + nrm[1] + ty), (end[0] - nrm[0] + tx, end[1] - nrm[1] + ty), (end[0] - nrm[0], end[1] - nrm[1])]))
    return out


def ring(outer, inner):
    """Outline between two closed shapes: outer positive, inner reversed."""
    return [reverse_subpath(x) if signed_area(flatten(x, 0.05)) < 0 else x for x in outer] + [
        x if signed_area(flatten(x, 0.05)) < 0 else reverse_subpath(x) for x in inner
    ]


# ---------------------------------------------------------------- SVG ----

def strip_ns(tag):
    return tag.rsplit("}", 1)[-1]


def parse_color(value):
    """Return None for the theme color, else an (r, g, b) tuple, or "none"."""
    if value is None or value in ("currentColor", "inherit", ""):
        return None
    if value == "none":
        return "none"
    m = re.fullmatch(r"#([0-9a-fA-F]{6})", value)
    if m:
        v = int(m.group(1), 16)
        return (v >> 16 & 255, v >> 8 & 255, v & 255)
    m = re.fullmatch(r"#([0-9a-fA-F]{3})", value)
    if m:
        return tuple(int(c * 2, 16) for c in m.group(1))
    raise ValueError(f"unsupported color {value!r}")


def style_attrs(el):
    attrs = dict(el.attrib)
    for decl in (attrs.pop("style", "") or "").split(";"):
        if ":" in decl:
            k, v = decl.split(":", 1)
            attrs[k.strip()] = v.strip()
    return attrs


def load_svg(path):
    """SVG file → (view_size, [layer]); layer = (color|None, alpha 0..1, subpaths)."""
    root = ET.parse(path).getroot()
    vb = root.get("viewBox")
    if vb:
        x0, y0, w, h = (float(v) for v in re.findall(NUMBER, vb))
    else:
        x0, y0, w, h = 0.0, 0.0, float(root.get("width")), float(root.get("height"))
    if abs(w - h) > 1e-6:
        raise ValueError(f"{path}: icons must have a square viewBox")
    layers = []

    def add(color, alpha, subpaths, m):
        tsp = []
        for start, segs, closed in subpaths:
            tsp.append(
                (
                    apply(m, start),
                    [(s[0],) + tuple(apply(m, p) for p in s[1:]) for s in segs],
                    closed,
                )
            )
        if layers and layers[-1][0] == color and layers[-1][1] == alpha:
            layers[-1][2].extend(tsp)
        else:
            layers.append((color, alpha, tsp))

    def walk(el, m, inherited):
        attrs = style_attrs(el)
        tag = strip_ns(el.tag)
        if tag in ("defs", "title", "desc", "metadata", "style", "clipPath", "mask"):
            return
        m = mat_mul(m, parse_transform(attrs.get("transform")))
        st = dict(inherited)
        for k in ("fill", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin", "fill-rule"):
            if k in attrs:
                st[k] = attrs[k]
        st["opacity"] = inherited["opacity"] * float(attrs.get("opacity", 1))
        st["fill-opacity"] = float(attrs.get("fill-opacity", inherited["fill-opacity"]))
        f = lambda k, d=0.0: float(attrs.get(k, d))
        if tag in ("svg", "g"):
            for child in el:
                walk(child, m, st)
            return
        if tag == "path":
            sp = parse_path(attrs.get("d", ""))
        elif tag == "rect":
            sp = rect_path(f("x"), f("y"), f("width"), f("height"), f("rx"), f("ry"))
        elif tag == "circle":
            sp = ellipse_path(f("cx"), f("cy"), f("r"), f("r"))
        elif tag == "ellipse":
            sp = ellipse_path(f("cx"), f("cy"), f("rx"), f("ry"))
        elif tag in ("polygon", "polyline"):
            v = [float(x) for x in re.findall(NUMBER, attrs.get("points", ""))]
            pts = list(zip(v[0::2], v[1::2]))
            sp = [(pts[0], [("L", p) for p in pts[1:]], tag == "polygon")]
        elif tag == "line":
            sp = [((f("x1"), f("y1")), [("L", (f("x2"), f("y2")))], False)]
        else:
            raise ValueError(f"{path}: unsupported element <{tag}>")
        if st.get("fill-rule") == "evenodd":
            sp = evenodd_to_nonzero(sp)
        fill = parse_color(st.get("fill")) if tag not in ("line", "polyline") else "none"
        if fill != "none":
            # Give every element positive net winding (holes keep their relative
            # orientation) so it unions with strokes and other elements in a layer.
            if sum(signed_area(flatten(x, 0.05)) for x in sp) * (m[0] * m[3] - m[1] * m[2]) < 0:
                sp = [reverse_subpath(x) for x in sp]
            add(fill, st["opacity"] * st["fill-opacity"], sp, m)
        stroke = parse_color(st.get("stroke", "none"))
        if stroke != "none":
            h = float(st.get("stroke-width", 1)) / 2
            if tag == "circle":
                piece = ring(ellipse_path(f("cx"), f("cy"), f("r") + h, f("r") + h), ellipse_path(f("cx"), f("cy"), f("r") - h, f("r") - h))
            elif tag == "ellipse":
                piece = ring(ellipse_path(f("cx"), f("cy"), f("rx") + h, f("ry") + h), ellipse_path(f("cx"), f("cy"), f("rx") - h, f("ry") - h))
            elif tag == "rect" and (f("rx") or f("ry")):
                rx, ry = f("rx") or f("ry"), f("ry") or f("rx")
                piece = ring(
                    rect_path(f("x") - h, f("y") - h, f("width") + 2 * h, f("height") + 2 * h, rx + h, ry + h),
                    rect_path(f("x") + h, f("y") + h, f("width") - 2 * h, f("height") - 2 * h, max(rx - h, 0), max(ry - h, 0)),
                )
            else:
                cap = st.get("stroke-linecap", "butt")
                join = st.get("stroke-linejoin", "miter")
                piece = stroke_subpaths(sp, 2 * h, cap, join)
            add(stroke, st["opacity"], piece, m)

    walk(root, (1, 0, 0, 1, -x0, -y0), {"opacity": 1.0, "fill-opacity": 1.0})
    return w, layers


# ---------------------------------------------------------------- encode ----

def q(v):
    r = round(v * FIXED)
    if not -32768 <= r <= 32767:
        raise ValueError(f"coordinate {v} out of range")
    return r


def encode_layer(subpaths):
    words = []
    for start, segs, closed in subpaths:
        words += [OP_MOVE, q(start[0]), q(start[1])]
        for s in segs:
            op = {"L": OP_LINE, "Q": OP_QUAD, "C": OP_CUBIC}[s[0]]
            words.append(op)
            for p in s[1:]:
                words += [q(p[0]), q(p[1])]
        words.append(OP_CLOSE)
    return words


def encode(icons):
    out = bytearray(b"BAEI")
    out += struct.pack("<BH", VERSION, len(icons))
    for name, (view, layers) in icons.items():
        nb = name.encode()
        out += struct.pack("<B", len(nb)) + nb
        out += struct.pack("<HB", round(view), len(layers))
        for color, alpha, subpaths in layers:
            rgba = 0 if color is None else (color[0] << 24 | color[1] << 16 | color[2] << 8 | 255)
            words = encode_layer(subpaths)
            if len(words) > 65535:
                raise ValueError(f"{name}: layer too large")
            out += struct.pack("<IBH", rgba, round(alpha * 255), len(words))
            out += struct.pack(f"<{len(words)}h", *words)
    return bytes(out)


# ---------------------------------------------------------------- preview ----

def rasterize(view, layers, px):
    """Same coverage-accumulation algorithm as the Rust rasterizer → RGBA rows."""
    img = [[(0, 0, 0, 0)] * px for _ in range(px)]
    s = px / view
    for color, alpha, subpaths in layers:
        acc = [0.0] * (px * px + 3)
        for sp in subpaths:
            pts = [(x * s, y * s) for x, y in flatten(sp, 0.25 / s)]
            for a, b in zip(pts, pts[1:] + pts[:1]):
                accumulate_line(acc, px, px, a, b)
        rgb = (230, 230, 230) if color is None else color
        total = 0.0
        for i in range(px * px):
            total += acc[i]
            cov = min(1.0, abs(total)) * alpha
            if cov > 0:
                y, x = divmod(i, px)
                r, g, b, a = img[y][x]
                na = cov + a / 255 * (1 - cov)
                mix = lambda c0, c1: round((c1 * cov + c0 * a / 255 * (1 - cov)) / na) if na else 0
                img[y][x] = (mix(r, rgb[0]), mix(g, rgb[1]), mix(b, rgb[2]), round(na * 255))
    return img


def accumulate_line(acc, w, h, p0, p1):
    if abs(p0[1] - p1[1]) < 1e-9:
        return
    if p0[1] < p1[1]:
        d0, p0, p1 = 1.0, p0, p1
    else:
        d0, p0, p1 = -1.0, p1, p0
    dxdy = (p1[0] - p0[0]) / (p1[1] - p0[1])
    x = p0[0]
    if p0[1] < 0:
        x -= p0[1] * dxdy
    for y in range(max(0, int(p0[1])), min(h, math.ceil(p1[1]))):
        dy = min(y + 1, p1[1]) - max(y, p0[1])
        xn = x + dxdy * dy
        d = dy * d0
        x0, x1 = (x, xn) if x < xn else (xn, x)
        x0 = min(max(x0, 0.0), w - 1e-4)
        x1 = min(max(x1, 0.0), w - 1e-4)
        base = y * w
        x0f = math.floor(x0)
        x1c = math.ceil(x1)
        x0i, x1i = int(x0f), int(x1c)
        if x1i <= x0i + 1:
            xm = 0.5 * (x0 + x1) - x0f
            acc[base + x0i] += d - d * xm
            acc[base + x0i + 1] += d * xm
        else:
            sl = 1.0 / (x1 - x0)
            x0r = x0 - x0f
            a0 = 0.5 * sl * (1 - x0r) ** 2
            x1r = x1 - x1c + 1
            am = 0.5 * sl * x1r * x1r
            acc[base + x0i] += d * a0
            if x1i == x0i + 2:
                acc[base + x0i + 1] += d * (1 - a0 - am)
            else:
                a1 = sl * (1.5 - x0r)
                acc[base + x0i + 1] += d * (a1 - a0)
                for xi in range(x0i + 2, x1i - 1):
                    acc[base + xi] += d * sl
                a2 = a1 + (x1i - x0i - 3) * sl
                acc[base + x1i - 1] += d * (1 - a2 - am)
            acc[base + x1i] += d * am
        x = xn


def write_png(path, rows):
    h, w = len(rows), len(rows[0])
    raw = b"".join(b"\0" + bytes(c for px in row for c in px) for row in rows)

    def chunk(t, data):
        return struct.pack(">I", len(data)) + t + data + struct.pack(">I", zlib.crc32(t + data))

    Path(path).write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def contact_sheet(icons, path, sizes=(16, 18, 32)):
    cols = 12
    cell = sum(sizes) + 8 * len(sizes) + 8
    rows_n = math.ceil(len(icons) / cols)
    bg = (0x23, 0x23, 0x23, 255)
    W, H = cols * cell, rows_n * (max(sizes) + 12)
    img = [[bg] * W for _ in range(H)]
    for n, (name, (view, layers)) in enumerate(icons.items()):
        r, c = divmod(n, cols)
        x = c * cell + 4
        for px in sizes:
            ic = rasterize(view, layers, px)
            for yy in range(px):
                for xx in range(px):
                    fr, fg, fb, fa = ic[yy][xx]
                    if fa:
                        a = fa / 255
                        oy, ox = r * (max(sizes) + 12) + 6 + yy, x + xx
                        br, bgc, bb, _ = img[oy][ox]
                        img[oy][ox] = (round(fr * a + br * (1 - a)), round(fg * a + bgc * (1 - a)), round(fb * a + bb * (1 - a)), 255)
            x += px + 8
    write_png(path, img)


# ---------------------------------------------------------------- main ----

def build(manifest_path):
    manifest = json.loads(Path(manifest_path).read_text())
    icons = {}
    for name, rel in manifest["icons"].items():
        icons[name] = load_svg(ICON_DIR / rel)
    return icons


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--manifest", default=MANIFEST)
    ap.add_argument("--out", default=OUTPUT)
    ap.add_argument("--preview", help="write a PNG contact sheet (16, 18 and 32 px)")
    ap.add_argument("--check", action="store_true", help="exit 1 if the output is out of date")
    args = ap.parse_args()
    icons = build(args.manifest)
    blob = encode(icons)
    out = Path(args.out)
    if args.check:
        if not out.exists() or zlib.decompress(out.read_bytes()) != blob:
            print(f"{out} is stale; run python3 tools/icons/build_icons.py", file=sys.stderr)
            return 1
    else:
        packed = zlib.compress(blob, 9)
        out.write_bytes(packed)
        shown = out.relative_to(ROOT) if out.is_relative_to(ROOT) else out
        print(f"{len(icons)} icons, {len(blob)} bytes, {len(packed)} compressed -> {shown}")
    if args.preview:
        contact_sheet(icons, args.preview)
    return 0


if __name__ == "__main__":
    sys.exit(main())
