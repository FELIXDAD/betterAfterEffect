"""Generate probe cases and reduce AE renders to the Lumetri parameter tables.

Offline dependencies: numpy, scipy, Pillow, pypng. Call 'prepare ROOT' after ae_compare
prepare has generated the inputs. Render under aectl lock, then 'reduce ROOT'.
"""
import json
import sys
from itertools import islice
from pathlib import Path

import numpy as np
import png
from PIL import Image
from scipy.spatial import cKDTree

ROOT = Path(sys.argv[2])
OUT = Path(__file__).resolve().parents[3] / "src/effects/color/lumetri"
LEVELS = np.linspace(0., 1., 64)
GRID = np.round(np.arange(32) * 255 / 31) / 255
B, G, R = np.meshgrid(GRID, GRID, GRID, indexing="ij")
RGB = np.stack([R, G, B], axis=-1) ** 2.4
LUMA = RGB @ [.2126, .7152, .0722]


def cases():
    def case(name, image, params, bpc=8, **extra):
        return dict(name=name, input=image, effect="ADBE Lumetri",
                    params=params, bpc=bpc, **extra)

    for control in [12, 13, 14]:
        for value in range(-150, 151, 15):
            yield case(f"t{control}_{value}", "ramp", {control: value}, 16)
    for value in range(-150, 151, 15):
        yield case(f"scontrast{value}", "rgb-cube", {76: 1, 88: value})
    for control, values in [(28, range(0, 151, 10)), (30, range(-100, 101, 25))]:
        for value in values:
            yield case(f"g{control}_{value}_rgb-cube", "rgb-cube", {control: value})
    for value in range(101):
        yield case(f"soften{value}", "lineg", {29: -value}, 16)
    for i in range(-10, 11):
        params = {51: i/2, 52: 0, 54: 0}
        yield case(f"full{i}", "ramp", params, 16)
        yield case(f"vigfull{i}", "strip", params, 16)
    for band in range(2):
        for hue in range(6):
            for amount in range(1, 5):
                wheel = dict(id=32, offset=16*band, values=[60*hue, amount/4])
                yield case(f"split{band}_{hue}_{amount}", "ramp", {}, 16, wheels=[wheel])


def path(name):
    return ROOT / "ae" / (name + ".png")


def cube(name):
    return np.asarray(Image.open(path(name)).convert("RGB"))[:64 if name.startswith("vigfull") else 128].reshape(RGB.shape)/255.


def row(name, index=0):
    _, _, rows, info = png.Reader(filename=str(path(name))).read()
    pixels = next(islice(rows, index, None))
    return np.asarray(pixels).reshape(-1, info["planes"])[:, :3] / (2**info["bitdepth"]-1)


def save(name, values, dtype, scale=1):
    values = np.asarray(values)
    if scale != 1:
        values = np.rint(values*scale)
    (OUT / (name + ".bin")).write_bytes(values.astype(dtype).tobytes())


def tone_curves():
    table = []
    for control in [12, 13, 14]:
        for value in range(-150, 151, 15):
            gray = row(f"t{control}_{value}")[:, 0]
            table.extend(np.interp(LEVELS, np.arange(256)/255, gray))
    save("tone", table, "<u2", 65535)


def secondary_contrast():
    # RGB stays on the line through the corrected luminance. The low endpoint
    # depends on both black and white headroom, especially near clipped primaries.
    level = LUMA**(1/2.4)
    low = RGB.min(-1)**(1/2.4)
    high = RGB.max(-1)**(1/2.4)
    ratio = low / np.maximum(1e-8, level)
    headroom = (high-level) / np.maximum(1e-8, 1-level)
    coordinates = np.stack([level, ratio, headroom], -1).reshape(-1, 3)
    axis = np.linspace(0., 1., 16)
    grid = np.stack(np.meshgrid(axis, axis, axis, indexing="ij"), -1)
    distance, indices = cKDTree(coordinates).query(grid.reshape(-1, 3), k=8)
    weights = 1/np.maximum(1e-7, distance)**3
    table = []
    for value in range(-150, 151, 15):
        gray = row(f"t12_{value}")[:, 0]
        target = np.interp(level, np.arange(256)/255, gray)
        residual = cube(f"scontrast{value}").min(-1) - target*ratio
        fitted = (weights * residual.ravel()[indices]).sum(1) / weights.sum(1)
        table.append(np.clip(np.rint((127.5 if value < 0 else 255)*fitted), -128, 127))
    save("secondary", table, "i1")


def film_response():
    # Fit a slope and a pedestal in linear light, weighting the residuals
    # in output code units. Clipped channels do not constrain the slope.
    table = []
    for value in range(0, 151, 10):
        linear = cube(f"g28_{value}_rgb-cube")**2.4
        for level in LEVELS:
            distance = abs(LUMA**(1/2.4)-level)
            mask = (distance < .025)[..., None] & (linear > 0) & (linear < 1)
            weights = np.exp(-.5*(distance[..., None]/.009)**2)/np.maximum(linear, .001)**1.15
            weights = np.sqrt(weights[mask])
            x = RGB[mask]
            if x.size < 4 or value == 0:
                slope, offset = 1., (value/400)**2.4 if level == 0 else 0.
            else:
                design = np.column_stack([x, np.ones_like(x)])*weights[:, None]
                slope, offset = np.linalg.lstsq(design, linear[mask]*weights, rcond=None)[0]
            table.extend([slope, offset])
    save("film", table, "<f2")


def vibrance_extrema():
    # Middle channels interpolate between the extrema in linear light.
    # Warm hues receive less positive vibrance than the other sectors.
    table = []
    for value in range(-100, 101, 25):
        colors = cube(f"g30_{value}_rgb-cube")
        for skin in (False, True):
            for hi in range(32):
                for lo in range(hi+1):
                    ends = colors[lo, lo, hi, [2, 0]] if skin else colors[hi, lo, lo, [0, 2]]
                    table.extend(ends)
    save("vibrance", table, "u1", 255)


def soften_weights():
    weights = []
    for value in range(101):
        a = row(f"soften{value}", 64)[:, 0]**2.4
        taps = np.maximum(0, (a[128:133]-a[115])/(1-a[115]))
        weights.extend(taps/(taps[0]+2*taps[1:].sum()))
    save("soften", weights, "<u2", 65535)


def vignette_response():
    table = []
    axis = (np.arange(256)/255)**2.4
    for value in range(-10, 11):
        gray = row(f"full{value}")[:, 0]**2.4
        colors = cube(f"vigfull{value}")**2.4
        target = np.interp(LUMA, axis, gray)
        distance = RGB-LUMA[..., None]
        slope = (colors-target[..., None])/(distance+1e-12)
        valid = (colors > .002) & (colors < .97) & (RGB < .999) & (abs(distance) > .02)
        for level in LEVELS:
            mask = (abs(LUMA**(1/2.4)-level) < .02)[..., None] & valid
            if mask.sum() > 8:
                gain = np.quantile(slope[mask], .9 if value < 0 else .5)
            else:
                gain = np.interp(level*255, np.arange(256), np.gradient(gray, axis))
            out = np.interp(level*255, np.arange(256), gray)**(1/2.4)
            table.extend([out, np.clip(gain, 0., 40.)])
    save("vignette", table, "<f2")


def split_curves():
    # The last code is a separate white anchor, even where the shoulder is steep.
    indices = list(range(0, 256, 4)) + [254, 255]
    table = [row(f"split{band}_{hue}_{amount}")[indices]
             for band in range(2) for hue in range(6) for amount in range(1, 5)]
    save("split", table, "u1", 255)


if sys.argv[1] == "prepare":
    image = Image.new("RGBA", (512, 512), (0, 0, 0, 255))
    source = Image.open(ROOT / "inputs/rgb-cube.png").convert("RGBA")
    image.paste(Image.frombytes("RGBA", (512, 64), source.tobytes()), (0, 0))
    image.save(ROOT / "inputs/strip.png")
    (ROOT / "cases.json").write_text(json.dumps(dict(cases=list(cases()))))
else:
    tone_curves()
    secondary_contrast()
    film_response()
    vibrance_extrema()
    soften_weights()
    vignette_response()
    split_curves()
