"""Writes the 16-bit PNG test charts for the AE comparison, tagged with PNG
cICP chunks (ITU-T H.273 code points) so AE's colour management picks the
media colour space itself (AE scripting cannot set Interpret Footage colour).

    python3 crates/bae-color/tools/ae/make_charts.py /private/tmp/bae-color-ae
"""

import os
import struct
import sys
import zlib

W, H = 32, 8


def chart_value(x, y):
    """Normalized RGB of pixel (x, y); mirrored in examples/ae_compare.rs."""
    t = x / (W - 1)
    if y == 0:
        return (t, 0.2, 0.1)
    if y == 1:
        return (0.1, t, 0.3)
    if y == 2:
        return (0.3, 0.1, t)
    if y == 3:
        return (t, t, t)
    k = (y - 4) * W + x
    return (((k * 37) % 101) / 100, ((k * 61) % 103) / 102, ((k * 89) % 107) / 106)


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)


def write_png(path, cicp):
    rows = []
    for y in range(H):
        row = b"\x00"
        for x in range(W):
            row += b"".join(struct.pack(">H", int(round(v * 65535))) for v in chart_value(x, y))
        rows.append(row)
    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 16, 2, 0, 0, 0))
    if cicp is not None:
        data += chunk(b"cICP", bytes([cicp[0], cicp[1], 0, 1]))
    data += chunk(b"IDAT", zlib.compress(b"".join(rows))) + chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(data)


CHARTS = {
    "untagged": None,
    "srgb": (1, 13),
    "p3": (12, 13),
    "rec709": (1, 1),
    "rec2020": (9, 14),
    "pq2020": (9, 16),
}

if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "/private/tmp/bae-color-ae"
    os.makedirs(out, exist_ok=True)
    for name, cicp in CHARTS.items():
        write_png(os.path.join(out, "chart_%s.png" % name), cicp)
    print("wrote", len(CHARTS), "charts to", out)
