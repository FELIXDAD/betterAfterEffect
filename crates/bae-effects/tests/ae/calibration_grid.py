"""Generate the seven-effect AE calibration grid in an ae_compare probe directory."""
import argparse
import json
from pathlib import Path


def cases(qualities):
    frames = []

    def add(name, effect, params, inputs, quality=None, time=0):
        for image in inputs.split():
            for q in qualities:
                if quality is None or q == quality:
                    frames.append(dict(name=f"{name}_{image}_{q}", input=image,
                                       effect="ADBE " + effect, params=params, quality=q, time=time))

    for angle, radius in [(20, 30), (100, 15), (100, 30), (-100, 30), (180, 30),
                          (100, 50), (100, 80), (100, 100)]:
        add(f"tw{angle}_{radius}", "Twirl", {1: angle, 2: radius}, "coords fine shift comb")

    ellipses = [([18, 21], 85, 51, 2), ([128, 64], 85, 51, 2),
                ([190, 95], 90, 70, -1), ([128, 64], 33, 30, 1)]
    for i, (center, rx, ry, height) in enumerate(ellipses):
        for pin in (0, 1):
            for aa in (1, 2):
                add(f"bu{i}_{pin}_{aa}", "Bulge", {1: rx, 2: ry, 3: center, 4: height, 6: aa, 7: pin},
                    "coords fine shift comb")
    for rx, ry in [(33, 30), (30, 33), (85, 51), (51, 85), (60, 30), (45, 30),
                   (43, 30), (35, 30), (34, 30), (31, 30), (32, 30), (33, 31), (33, 32),
                   (33, 33), (33, 34), (20, 20), (35, 35), (50, 45), (57, 40), (90, 35),
                   (50.25, 30), (50, 30.25), (37, 24), (41, 51)]:
        add(f"boundary{rx}_{ry}", "Bulge", {1: rx, 2: ry, 3: [128, 64], 4: 2, 6: 2, 7: 0},
            "comb shift fine", "best")

    for kind, amount, seed in [(1, 5, 0), (1, 18, 0), (1, 18, 1), (2, 15, 0), (2, 30, 0)]:
        for aa in (1, 2):
            add(f"rb{kind}_{amount}_{aa}_{seed}", "Radial Blur",
                {1: amount, 2: [100, 70], 3: kind, 4: aa, 6: seed}, "fine comb line chart")
    add("rb_repeat", "Radial Blur", {1: 18, 2: [100, 70], 3: 1, 4: 1, 6: 0}, "comb")

    for nx, ny in [(8, 4), (10, 10), (13, 7), (7, 13), (17, 9), (31, 15),
                   (5, 3), (2, 1), (128, 64), (19, 1), (1, 19)]:
        for sharp in (0, 1):
            add(f"mo{nx}_{ny}_{sharp}", "Mosaic", {1: nx, 2: ny, 3: sharp}, "white fine edge")

    for radius in (1, 3, 8):
        for mode in (0, 1):
            for threshold in (0, 0.1):
                add(f"us{radius}_{mode}_{threshold:g}", "Unsharp Mask2",
                    {1: 100, 2: radius, 3: threshold, 4: mode}, "ush alpha soft lineg")

    for radius, phase, width in [(30, 0, 20), (80, 0, 20), (60, 90, 20),
                                 (60, 40, 30), (45, 180, 8)]:
        for kind in (1, 2):
            add(f"ri{radius}_{phase}_{kind}", "Ripple",
                {1: radius, 3: kind, 5: width, 6: 20, 7: phase}, "coords fine shift comb")

    for axis, center, angle in [(1, [128, 64], 35), (1, [128, 64], 80), (2, [128, 64], 70),
                                (2, [150, 75], 70), (3, [128, 64], 80), (1, [90, 54], 50)]:
        for reverse in (0, 1):
            for size in (0, 1):
                add(f"op{axis}_{center[0]}_{angle}_{reverse}_{size}", "Optics Compensation",
                    {1: angle, 2: reverse, 3: axis, 4: center, 5: size}, "coords fine shift comb")
    # Independent cases cover fractional centres, source clipping and animated phase.
    for i, (cx, cy, radius, width, height, phase, time) in enumerate([
        (83.25, 47.75, 35, 20, 20, 0, 0), (83, 47, 35, 5, 40, 47, 0.4),
        (128, 64, 3.25, 2, 20, 0, 0), (199, 93, 98, 39.5, 32, 138, 0.1),
        (128, 64, 80, 8, 200, 18, 0), (100, 70, 45, 4, 400, 90, 0),
    ]):
        for kind in (1, 2):
            add(f"ripple_hold{i}_{kind}", "Ripple", {1: radius, 2: [cx, cy], 3: kind,
                5: width, 6: height, 7: phase}, "comb coords fine shift", "best", time)
    for i, (cx, cy, axis, angle) in enumerate([
        (30, 16, 2, 85), (128, 64, 1, 110), (83.25, 47.75, 1, 95),
        (128, 64, 2, 150), (20, 29, 2, 135), (128.5, 64.5, 3, 165),
    ]):
        for reverse in (0, 1):
            for optimal in (0, 1):
                add(f"optics_hold{i}_{reverse}_{optimal}", "Optics Compensation",
                    {1: angle, 2: reverse, 3: axis, 4: [cx, cy], 5: optimal},
                    "comb fine coords", "best")
    return frames


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path, help="probe directory from ae_compare prepare")
    parser.add_argument("--quality", choices=["both", "best", "draft"], default="both")
    args = parser.parse_args()
    root = args.directory.resolve()
    if not (root / "inputs" / "comb.png").exists():
        parser.error("generate the probe inputs with ae_compare prepare first")
    frames = cases(["best", "draft"] if args.quality == "both" else [args.quality])
    (root / "cases.json").write_text(json.dumps({"cases": frames}))
    renderer = Path(__file__).with_name("render_cases.jsx").read_text()
    (root / "run.jsx").write_text("var BAE_07_ROOT = " + json.dumps(str(root)) + ";\n" + renderer)
    print(f"{len(frames)} frames in {root}")
