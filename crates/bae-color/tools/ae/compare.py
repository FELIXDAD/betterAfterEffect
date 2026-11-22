"""Compares AE renders (render_cases.jsx) with bae-color (examples/ae_compare.rs).

    python3 crates/bae-color/tools/ae/compare.py /private/tmp/bae-color-ae

For every case prints the max / mean absolute error against the expected bae
model and the closest of all models (to discover what AE actually does).
Errors are in normalized code values; AE wrote 16-bit PNG (1 LSB = 1.5e-5).
"""

import json
import os
import struct
import sys
import zlib

EXPECTED = {
    "a_srgb_identity": "srgb->srgb",
    "a_untagged_in_srgb": "srgb->srgb",
    "a_p3_to_srgb": "display-p3->srgb",
    # AE reads the BT.709 / BT.2020 transfer tags as BT.1886 gamma 2.4.
    "a_rec709_to_srgb_comp": "rec709-gamma24->srgb",
    "a_rec709_to_srgb_nocomp": "rec709-gamma24->srgb",
    # Linearized working space; the output module converts back to the
    # (non-linear) working space, so the chain is the identity.
    "a_srgb_linearized": "srgb->srgb",
    "a_srgb_to_g24": "srgb->rec709-gamma24",
    "a_srgb_to_acescg": "srgb->acescg",
    "a_srgb_to_p3": "srgb->display-p3",
    "a_srgb_to_pq_w100": "srgb->rec2100-pq",
    # AE 26.2's "Rec.2100 PQ W203" encodes SDR footage exactly like W100.
    "a_srgb_to_pq_w203": "srgb->rec2100-pq",
    "a_pq_to_srgb": "rec2100-pq->srgb",
    "a_rec2020_to_srgb": "rec2020-gamma24->srgb",
}


def read_png16(path):
    data = open(path, "rb").read()
    pos, idat, w, h, ctype = 8, b"", 0, 0, 0
    while pos < len(data):
        n = struct.unpack(">I", data[pos:pos + 4])[0]
        kind, body = data[pos + 4:pos + 8], data[pos + 8:pos + 8 + n]
        if kind == b"IHDR":
            w, h, depth, ctype = struct.unpack(">IIBB", body[:10])
            assert depth == 16, depth
        elif kind == b"IDAT":
            idat += body
        pos += 12 + n
    ch = {2: 3, 6: 4}[ctype]
    bpp = ch * 2
    raw = zlib.decompress(idat)
    stride = w * bpp
    prev = bytearray(stride)
    px = []
    i = 0
    for _ in range(h):
        f = raw[i]
        line = bytearray(raw[i + 1:i + 1 + stride])
        i += 1 + stride
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0
            b = prev[x]
            c = prev[x - bpp] if x >= bpp else 0
            if f == 1:
                line[x] = (line[x] + a) & 255
            elif f == 2:
                line[x] = (line[x] + b) & 255
            elif f == 3:
                line[x] = (line[x] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pr = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                line[x] = (line[x] + pr) & 255
        prev = line
        for x in range(w):
            v = struct.unpack(">%dH" % ch, bytes(line[x * bpp:(x + 1) * bpp]))
            px.append([v[0] / 65535, v[1] / 65535, v[2] / 65535])
    return px


def errors(ae, model):
    diffs = []
    for a, m in zip(ae, model):
        for c in range(3):
            diffs.append(abs(a[c] - min(max(m[c], 0.0), 1.0)))
    return max(diffs), sum(diffs) / len(diffs)


def main():
    d = sys.argv[1] if len(sys.argv) > 1 else "/private/tmp/bae-color-ae"
    bae = json.load(open(os.path.join(d, "bae.json")))
    models = {k: v for k, v in bae.items() if k != "input"}
    rows = []
    for case, model in EXPECTED.items():
        path = os.path.join(d, "out", case + "_00000.png")
        if not os.path.exists(path):
            rows.append((case, "missing", "", "", ""))
            continue
        ae = read_png16(path)
        mx, mean = errors(ae, models[model]) if model in models else (float("nan"), float("nan"))
        best = min(models, key=lambda k: errors(ae, models[k])[0])
        bmx = errors(ae, models[best])[0]
        rows.append((case, model, "%.2e" % mx, "%.2e" % mean, "%s (%.2e)" % (best, bmx)))
    w = max(len(r[0]) for r in rows)
    print("%-*s  %-34s %-9s %-9s %s" % (w, "case", "bae model", "max", "mean", "closest model"))
    for r in rows:
        print("%-*s  %-34s %-9s %-9s %s" % (w, r[0], r[1], r[2], r[3], r[4]))


if __name__ == "__main__":
    main()
