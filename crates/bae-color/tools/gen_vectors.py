"""Generates tests/data/colour_vectors.json with colour-science (and, for the
OCIO comparison, PyOpenColorIO's built-in ACES studio config).

    uv venv -p 3.12 .venv && uv pip install -p .venv colour-science opencolorio
    .venv/bin/python crates/bae-color/tools/gen_vectors.py

Conventions mirror bae-color: normalized primary matrices are derived from the
chromaticities (not colour's rounded IEC sRGB matrix), PQ linear 1.0 = 100 nits,
Rec.709 uses the continuous BT.2020 (12-bit) constants, display curves mirror
negatives, camera spaces use their vendors' CAT (CAT02 for Sony/ARRI/Canon).
"""

import json
import os
import warnings

warnings.filterwarnings("ignore")

import numpy as np  # noqa: E402
import colour  # noqa: E402
from colour import models as M  # noqa: E402


def mirror(f):
    return lambda x: np.sign(x) * f(np.abs(x))


CURVES = {
    # id: (decode, encode, encoded sample range)
    "srgb": (mirror(M.eotf_sRGB), mirror(M.eotf_inverse_sRGB), (-0.2, 1.2)),
    "rec709": (
        mirror(lambda v: M.oetf_inverse_BT2020(v, is_12_bits_system=True)),
        mirror(lambda l: M.oetf_BT2020(l, is_12_bits_system=True)),
        (-0.2, 1.2),
    ),
    "gamma2.2": (mirror(lambda v: v**2.2), mirror(lambda l: l ** (1 / 2.2)), (-0.2, 1.2)),
    "gamma2.4": (mirror(lambda v: v**2.4), mirror(lambda l: l ** (1 / 2.4)), (-0.2, 1.2)),
    "pq": (
        mirror(lambda v: M.eotf_ST2084(v) / 100.0),
        mirror(lambda l: M.eotf_inverse_ST2084(l * 100.0)),
        (0.0, 1.0),
    ),
    "hlg": (mirror(M.oetf_inverse_BT2100_HLG), mirror(M.oetf_BT2100_HLG), (0.0, 1.0)),
    "acescc": (M.log_decoding_ACEScc, M.log_encoding_ACEScc, (-0.3, 1.3)),
    "acescct": (M.log_decoding_ACEScct, M.log_encoding_ACEScct, (0.0, 1.3)),
    "slog3": (M.log_decoding_SLog3, M.log_encoding_SLog3, (0.0, 1.0)),
    "logc3": (M.log_decoding_ARRILogC3, M.log_encoding_ARRILogC3, (0.0, 1.0)),
    "logc4": (M.log_decoding_ARRILogC4, M.log_encoding_ARRILogC4, (0.0, 1.0)),
    "vlog": (M.log_decoding_VLog, M.log_encoding_VLog, (0.0, 1.0)),
    "clog3": (M.log_decoding_CanonLog3, M.log_encoding_CanonLog3, (0.0, 1.0)),
    "linear": (lambda v: v, lambda l: l, (-0.2, 1.2)),
}

D65 = [0.3127, 0.3290]
ACES_W = [0.32168, 0.33767]
GAMUTS = {
    "rec709": ([[0.64, 0.33], [0.30, 0.60], [0.15, 0.06]], D65),
    "p3d65": ([[0.680, 0.320], [0.265, 0.690], [0.150, 0.060]], D65),
    "dcip3": ([[0.680, 0.320], [0.265, 0.690], [0.150, 0.060]], [0.314, 0.351]),
    "rec2020": ([[0.708, 0.292], [0.170, 0.797], [0.131, 0.046]], D65),
    "ap0": ([[0.7347, 0.2653], [0.0, 1.0], [0.0001, -0.0770]], ACES_W),
    "ap1": ([[0.713, 0.293], [0.165, 0.830], [0.128, 0.044]], ACES_W),
    "adobergb": ([[0.64, 0.33], [0.21, 0.71], [0.15, 0.06]], D65),
    "sgamut3": ([[0.730, 0.280], [0.140, 0.855], [0.100, -0.050]], D65),
    "sgamut3cine": ([[0.766, 0.275], [0.225, 0.800], [0.089, -0.087]], D65),
    "awg3": ([[0.6840, 0.3130], [0.2210, 0.8480], [0.0861, -0.1020]], D65),
    "awg4": ([[0.7347, 0.2653], [0.1424, 0.8576], [0.0991, -0.0308]], D65),
    "vgamut": ([[0.730, 0.280], [0.165, 0.840], [0.100, -0.030]], D65),
    "cinemagamut": ([[0.740, 0.270], [0.170, 1.140], [0.080, -0.100]], D65),
}

# bae-color built-in id -> (gamut, curve, CAT to the ACES white)
SPACES = {
    "srgb": ("rec709", "srgb", "Bradford"),
    "srgb-linear": ("rec709", "linear", "Bradford"),
    "rec709": ("rec709", "rec709", "Bradford"),
    "rec709-gamma24": ("rec709", "gamma2.4", "Bradford"),
    "gamma22-rec709": ("rec709", "gamma2.2", "Bradford"),
    "display-p3": ("p3d65", "srgb", "Bradford"),
    "p3-d65-linear": ("p3d65", "linear", "Bradford"),
    "rec2020": ("rec2020", "rec709", "Bradford"),
    "rec2020-linear": ("rec2020", "linear", "Bradford"),
    "rec2100-pq": ("rec2020", "pq", "Bradford"),
    "rec2100-hlg": ("rec2020", "hlg", "Bradford"),
    "acescg": ("ap1", "linear", "Bradford"),
    "aces2065-1": ("ap0", "linear", "Bradford"),
    "acescct": ("ap1", "acescct", "Bradford"),
    "slog3-sgamut3": ("sgamut3", "slog3", "CAT02"),
    "slog3-sgamut3cine": ("sgamut3cine", "slog3", "CAT02"),
    "logc3-awg3": ("awg3", "logc3", "CAT02"),
    "logc4-awg4": ("awg4", "logc4", "CAT02"),
    "vlog-vgamut": ("vgamut", "vlog", "Bradford"),
    "clog3-cinemagamut": ("cinemagamut", "clog3", "CAT02"),
}


def to_ap0(gamut, cat):
    prim, white = GAMUTS[gamut]
    npm = colour.normalised_primary_matrix(np.array(prim), np.array(white))
    ap0_prim, ap0_white = GAMUTS["ap0"]
    npm0 = colour.normalised_primary_matrix(np.array(ap0_prim), np.array(ap0_white))
    if white == ap0_white:
        cat_m = np.identity(3)
    else:
        cat_m = colour.adaptation.matrix_chromatic_adaptation_VonKries(
            colour.xy_to_XYZ(white), colour.xy_to_XYZ(ap0_white), cat
        )
    return np.linalg.inv(npm0) @ cat_m @ npm


def convert(src, dst, rgb):
    g0, c0, a0 = SPACES[src]
    g1, c1, a1 = SPACES[dst]
    lin = CURVES[c0][0](np.array(rgb, dtype=np.float64))
    ap0 = to_ap0(g0, a0) @ lin
    out = np.linalg.inv(to_ap0(g1, a1)) @ ap0
    return CURVES[c1][1](out)


def main():
    out = {"curves": {}, "conversions": [], "ocio": []}
    for cid, (dec, enc, (lo, hi)) in CURVES.items():
        v = np.linspace(lo, hi, 23)
        lin = dec(v)
        out["curves"][cid] = {"encoded": v.tolist(), "linear": np.asarray(lin, dtype=float).tolist()}

    rgb_samples = [
        [0.18, 0.18, 0.18],
        [0.5, 0.4, 0.3],
        [0.9, 0.1, 0.2],
        [0.05, 0.6, 0.9],
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0],
        [0.3, 0.7, 0.45],
    ]
    pairs = [
        ("srgb", "acescg"),
        ("srgb", "display-p3"),
        ("display-p3", "rec2020"),
        ("rec709", "srgb"),
        ("srgb", "rec709-gamma24"),
        ("acescg", "rec2100-pq"),
        ("rec2100-hlg", "rec2100-pq"),
        ("acescct", "srgb"),
        ("slog3-sgamut3", "acescg"),
        ("slog3-sgamut3cine", "srgb"),
        ("logc3-awg3", "aces2065-1"),
        ("logc4-awg4", "acescg"),
        ("vlog-vgamut", "rec709"),
        ("clog3-cinemagamut", "aces2065-1"),
        ("aces2065-1", "srgb-linear"),
        ("gamma22-rec709", "p3-d65-linear"),
        ("rec2020-linear", "srgb"),
    ]
    for src, dst in pairs:
        for rgb in rgb_samples:
            res = convert(src, dst, rgb)
            out["conversions"].append({"src": src, "dst": dst, "rgb": rgb, "out": np.asarray(res, float).tolist()})

    # OCIO ACES studio config: the same conversions as AE's OCIO engine performs.
    try:
        import PyOpenColorIO as OCIO

        cfg = OCIO.Config.CreateFromBuiltinConfig("studio-config-latest")
        ocio_names = {
            "srgb-linear": "Linear Rec.709 (sRGB)",
            "acescg": "ACEScg",
            "aces2065-1": "ACES2065-1",
            "acescct": "ACEScct",
            "p3-d65-linear": "Linear P3-D65",
            "rec2020-linear": "Linear Rec.2020",
            "gamma22-rec709": "Gamma 2.2 Encoded Rec.709",
            "rec709-gamma24": "Gamma 2.4 Encoded Rec.709",
            "slog3-sgamut3": "S-Log3 S-Gamut3",
            "slog3-sgamut3cine": "S-Log3 S-Gamut3.Cine",
            "logc3-awg3": "ARRI LogC3 (EI800)",
            "logc4-awg4": "ARRI LogC4",
            "vlog-vgamut": "V-Log V-Gamut",
            "clog3-cinemagamut": "CanonLog3 CinemaGamut D55",
        }
        out["ocio_config"] = cfg.getName()
        for src, sname in ocio_names.items():
            for dst in ["acescg", "srgb-linear"]:
                if src == dst:
                    continue
                proc = cfg.getProcessor(sname, ocio_names[dst]).getDefaultCPUProcessor()
                for rgb in rgb_samples:
                    res = proc.applyRGB([float(x) for x in rgb])
                    out["ocio"].append({"src": src, "dst": dst, "rgb": rgb, "out": [float(x) for x in res]})
    except ImportError:
        pass

    path = os.path.join(os.path.dirname(__file__), "..", "tests", "data", "colour_vectors.json")
    with open(path, "w") as f:
        json.dump(out, f, indent=0, sort_keys=True)
        f.write("\n")
    print("wrote", os.path.normpath(path), "colour", colour.__version__)


if __name__ == "__main__":
    main()
