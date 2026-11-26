"""Generate the tiny layered PSD/PSB used by decoding tests and startup warm-up."""
from pathlib import Path
from struct import pack
from zlib import compress

def document(version, depth):
    length = ">I" if version == 1 else ">Q"
    def block(data):
        return pack(length, len(data)) + data
    def channel(v):
        if depth == 32:
            return pack(">f", v)
        return pack(">H", round(v * 65535))
    values = (1.5 if depth == 32 else 0.12345, 0.25, 0.5, 0.75)
    planes = [pack(">H", 2) + compress(channel(v)) for v in values]
    tags = b"8BIMlyid" + pack(">II", 4, 37)
    extra = bytes(8) + b"\x03Ink" + tags
    record = pack(">iiiiH", 0, 1, 1, 2, 4)
    record += b"".join(pack(">h", i) + pack(length, len(p)) for i, p in zip((0, 1, 2, -1), planes))
    record += b"8BIMnorm" + bytes([192, 0, 0, 0]) + pack(">I", len(extra)) + extra
    layers = pack(">h", -1) + record + b"".join(planes)
    layers += bytes(-len(layers) % 4)
    section = block(b"") + bytes(4) + b"8BIM" + (b"Lr16" if depth == 16 else b"Lr32") + block(layers)
    header = b"8BPS" + pack(">H6xHIIHH", version, 4, 1, 2, depth, 3)
    merged = pack(">H", 0) + b"".join(channel(0) + channel(v) for v in values)
    return header + bytes(8) + block(section) + merged

for version, depth, ext in [(1, 16, "psd"), (2, 32, "psb")]:
    Path(__file__).with_name(f"ink{depth}.{ext}").write_bytes(document(version, depth))
