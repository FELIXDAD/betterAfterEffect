"""Generates the OCIO comparison data in tests/data/ocio/ with PyOpenColorIO:

* the ACES configs that ship inside OCIO 2.6 (serialized as .ocio text),
* a hand-written config that uses every transform bae-color supports, plus
  small LUT files (.cube 1D / 3D / shaper+3D, .spi1d, .spi3d),
* vectors.json: OCIO's results for colour space and display/view conversions.

    uv venv -p 3.12 .venv && uv pip install -p .venv opencolorio
    .venv/bin/python crates/bae-color/tools/gen_ocio_vectors.py
"""

import json
import math
import os

import PyOpenColorIO as OCIO

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "tests", "data", "ocio")

SAMPLES = [
    [0.18, 0.18, 0.18],
    [0.0, 0.0, 0.0],
    [1.0, 1.0, 1.0],
    [0.9, 0.2, 0.05],
    [0.05, 0.5, 0.8],
    [0.35, 0.6, 0.25],
    [2.5, 1.2, 0.6],
    [0.002, 0.004, 0.01],
]

ACES_CONFIGS = [
    "studio-config-v5.0.0_aces-v2.1_ocio-v2.6",
    "cg-config-v2.2.0_aces-v1.3_ocio-v2.4",
]


def write(name, text):
    with open(os.path.join(OUT, name), "w") as f:
        f.write(text)


def luts():
    # 1D: a gentle S curve on a [-0.125, 1.125] domain.
    n = 33
    lines = ["TITLE \"bae 1d\"", "LUT_1D_SIZE %d" % n, "DOMAIN_MIN -0.125 -0.125 -0.125", "DOMAIN_MAX 1.125 1.125 1.125"]
    for i in range(n):
        x = -0.125 + 1.25 * i / (n - 1)
        y = 0.5 + 0.5 * math.tanh(2.0 * (x - 0.5)) / math.tanh(1.0)
        lines.append("%.7f %.7f %.7f" % (y, y * 0.95, y ** 1.1 if y > 0 else y))
    write("curve1d.cube", "\n".join(lines) + "\n")

    def f3(r, g, b):
        return (0.8 * r + 0.15 * g + 0.05 * b * b, 0.1 * r + 0.85 * g * g + 0.05 * b, 0.05 * r + 0.1 * g + 0.85 * b ** 1.2)

    n = 9
    lines = ["LUT_3D_SIZE %d" % n]
    for b in range(n):
        for g in range(n):
            for r in range(n):
                o = f3(r / (n - 1), g / (n - 1), b / (n - 1))
                lines.append("%.7f %.7f %.7f" % o)
    write("look3d.cube", "\n".join(lines) + "\n")

    # Shaper (log2 range) + 3D in one .cube.
    lines = ["LUT_1D_SIZE 17", "LUT_3D_SIZE 5", "LUT_1D_INPUT_RANGE 0 2"]
    for i in range(17):
        x = 2.0 * i / 16
        y = math.sqrt(x / 2.0)
        lines.append("%.7f %.7f %.7f" % (y, y, y))
    for b in range(5):
        for g in range(5):
            for r in range(5):
                o = f3(r / 4, g / 4, b / 4)
                lines.append("%.7f %.7f %.7f" % o)
    write("shaper3d.cube", "\n".join(lines) + "\n")

    n = 64
    vals = ["%.7f" % ((i / (n - 1)) ** 2.0) for i in range(n)]
    write("gamma.spi1d", "Version 1\nFrom 0.0 1.0\nLength %d\nComponents 1\n{\n%s\n}\n" % (n, "\n".join(vals)))
    n = 7
    lines = ["SPILUT 1.0", "3 3", "%d %d %d" % (n, n, n)]
    for i in range(n):
        for j in range(n):
            for k in range(n):
                o = f3(i / (n - 1), j / (n - 1), k / (n - 1))
                lines.append("%d %d %d %.7f %.7f %.7f" % ((i, j, k) + o))
    write("look.spi3d", "\n".join(lines) + "\n")


HANDMADE = """ocio_profile_version: 2
name: bae-handmade
search_path: .
roles:
  scene_linear: lin
  aces_interchange: lin
  cie_xyz_d65_interchange: xyz
  data: raw
  default: lin
file_rules:
  - !<Rule> {name: Default, colorspace: lin}
default_view_transform: identity
view_transforms:
  - !<ViewTransform>
    name: identity
    from_scene_reference: !<MatrixTransform> {}
displays:
  Display:
    - !<View> {name: Raw, colorspace: raw}
    - !<View> {name: Standard, view_transform: identity, display_colorspace: <USE_DISPLAY_NAME>}
colorspaces:
  - !<ColorSpace>
    name: lin
  - !<ColorSpace>
    name: raw
    isdata: true
  - !<ColorSpace>
    name: range_clamp
    from_scene_reference: !<RangeTransform> {min_in_value: 0.1, max_in_value: 0.9, min_out_value: 0, max_out_value: 1}
  - !<ColorSpace>
    name: range_noclamp
    from_scene_reference: !<RangeTransform> {min_in_value: -0.5, max_in_value: 2, min_out_value: 0, max_out_value: 1, style: noClamp}
  - !<ColorSpace>
    name: range_min
    from_scene_reference: !<RangeTransform> {min_in_value: 0.05, min_out_value: 0.05}
  - !<ColorSpace>
    name: cdl_asc
    from_scene_reference: !<CDLTransform> {slope: [1.1, 0.95, 1.05], offset: [0.01, -0.02, 0.0], power: [1.2, 0.9, 1.0], sat: 0.85, style: asc}
  - !<ColorSpace>
    name: cdl_noclamp
    from_scene_reference: !<CDLTransform> {slope: [1.1, 0.95, 1.05], offset: [0.01, -0.02, 0.0], power: [1.2, 0.9, 1.0], sat: 1.2, style: noClamp}
  - !<ColorSpace>
    name: exp_clamp
    from_scene_reference: !<ExponentTransform> {value: [2.2, 2.4, 1.8, 1], direction: inverse}
  - !<ColorSpace>
    name: exp_mirror
    from_scene_reference: !<ExponentTransform> {value: [2.2, 2.4, 1.8, 1], style: mirror, direction: inverse}
  - !<ColorSpace>
    name: exp_pass
    from_scene_reference: !<ExponentTransform> {value: [2.2, 2.4, 1.8, 1], style: pass_thru, direction: inverse}
  - !<ColorSpace>
    name: moncurve
    from_scene_reference: !<ExponentWithLinearTransform> {gamma: [2.4, 2.2, 3.0, 1], offset: [0.055, 0.099, 0.16, 0], direction: inverse}
  - !<ColorSpace>
    name: moncurve_mirror
    from_scene_reference: !<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055, style: mirror, direction: inverse}
  - !<ColorSpace>
    name: log10
    from_scene_reference: !<LogTransform> {base: 10}
  - !<ColorSpace>
    name: logaffine
    from_scene_reference: !<LogAffineTransform> {base: 2, log_side_slope: [0.09, 0.1, 0.11], log_side_offset: 0.5, lin_side_slope: 1.2, lin_side_offset: 0.01}
  - !<ColorSpace>
    name: logcamera
    from_scene_reference: !<LogCameraTransform> {base: 10, log_side_slope: 0.24719, log_side_offset: 0.385537, lin_side_slope: 5.555556, lin_side_offset: 0.0522723, lin_side_break: 0.010591}
  - !<ColorSpace>
    name: matrix_offset
    from_scene_reference: !<MatrixTransform> {matrix: [1.2, -0.1, -0.1, 0, 0.05, 0.9, 0.05, 0, 0, 0.2, 0.8, 0, 0, 0, 0, 1], offset: [0.01, -0.02, 0.03, 0]}
  - !<ColorSpace>
    name: lut1d_cube
    from_scene_reference: !<FileTransform> {src: curve1d.cube}
  - !<ColorSpace>
    name: lut3d_linear
    from_scene_reference: !<FileTransform> {src: look3d.cube, interpolation: linear}
  - !<ColorSpace>
    name: lut3d_tetra
    from_scene_reference: !<FileTransform> {src: look3d.cube, interpolation: tetrahedral}
  - !<ColorSpace>
    name: shaper3d
    from_scene_reference: !<FileTransform> {src: shaper3d.cube, interpolation: linear}
  - !<ColorSpace>
    name: spi1d
    from_scene_reference: !<FileTransform> {src: gamma.spi1d}
  - !<ColorSpace>
    name: spi1d_inverse
    from_scene_reference: !<FileTransform> {src: gamma.spi1d, direction: inverse}
  - !<ColorSpace>
    name: spi3d
    from_scene_reference: !<FileTransform> {src: look.spi3d, interpolation: linear}
  - !<ColorSpace>
    name: group
    to_scene_reference: !<GroupTransform>
      children:
        - !<ColorSpaceTransform> {src: moncurve, dst: lin}
        - !<MatrixTransform> {matrix: [0.9, 0.1, 0, 0, 0, 1, 0, 0, 0, 0.1, 0.9, 0, 0, 0, 0, 1]}
  - !<ColorSpace>
    name: builtin_acescct
    to_scene_reference: !<BuiltinTransform> {style: ACEScct_to_ACES2065-1}
display_colorspaces:
  - !<ColorSpace>
    name: xyz
  - !<ColorSpace>
    name: Display
    from_display_reference: !<BuiltinTransform> {style: DISPLAY - CIE-XYZ-D65_to_sRGB}
"""


def cpu(proc):
    # LOSSLESS: no fast log/exp/pow approximations (the default CPU processor
    # uses them, with errors around 1e-4).
    return proc.getOptimizedCPUProcessor(OCIO.BIT_DEPTH_F32, OCIO.BIT_DEPTH_F32, OCIO.OPTIMIZATION_LOSSLESS)


def apply(proc, rgb):
    return [float(v) for v in proc.applyRGB([float(x) for x in rgb])]


def main():
    os.makedirs(OUT, exist_ok=True)
    luts()
    write("handmade.ocio", HANDMADE)
    data = {"ocio_version": OCIO.__version__, "configs": {}}

    configs = {"handmade.ocio": OCIO.Config.CreateFromFile(os.path.join(OUT, "handmade.ocio"))}
    for name in ACES_CONFIGS:
        cfg = OCIO.Config.CreateFromBuiltinConfig(name)
        fname = name + ".ocio"
        write(fname, cfg.serialize())
        configs[fname] = cfg

    for fname, cfg in configs.items():
        cases = []
        names = [cs.getName() for cs in cfg.getColorSpaces()]
        aces = cfg.getRoleColorSpace("aces_interchange") or names[0]
        targets = [aces]
        scene_lin = cfg.getRoleColorSpace("scene_linear")
        if scene_lin and scene_lin not in targets:
            targets.append(scene_lin)
        for src in names:
            for dst in targets + ([d for d in names if "sRGB - Display" in d][:1]):
                if src == dst:
                    continue
                try:
                    proc = cpu(cfg.getProcessor(src, dst))
                except Exception:  # noqa: BLE001
                    continue
                cases.append({"kind": "cs", "src": src, "dst": dst, "in": SAMPLES, "out": [apply(proc, s) for s in SAMPLES]})
        if fname == "handmade.ocio":
            # Forward direction of every from_scene_reference.
            for dst in names:
                if dst == aces:
                    continue
                proc = cpu(cfg.getProcessor(aces, dst))
                cases.append({"kind": "cs", "src": aces, "dst": dst, "in": SAMPLES, "out": [apply(proc, s) for s in SAMPLES]})
        for display in cfg.getDisplays():
            for view in cfg.getViews(display):
                src = scene_lin or aces
                try:
                    t = OCIO.DisplayViewTransform(src=src, display=display, view=view)
                    proc = cpu(cfg.getProcessor(t))
                except Exception:  # noqa: BLE001
                    continue
                cases.append({"kind": "dv", "src": src, "display": display, "view": view, "in": SAMPLES, "out": [apply(proc, s) for s in SAMPLES]})
        data["configs"][fname] = cases

    with open(os.path.join(OUT, "vectors.json"), "w") as f:
        json.dump(data, f, separators=(",", ":"))
        f.write("\n")
    print("wrote", os.path.normpath(OUT), "OCIO", OCIO.__version__, {k: len(v) for k, v in data["configs"].items()})


if __name__ == "__main__":
    main()
