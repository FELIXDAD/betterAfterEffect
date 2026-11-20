#!/usr/bin/env python3
"""Generate anim-cases.json: keyframe and expression cases for bae-anim reference data.

Usage: python3 bridges/ae/samples/gen_anim_cases.py [--check]

Each case is one AE property set from a bae-model AttrSpec (keyframes keyed by flicks,
comp time). bae-anim-samples.jsx builds the cases in After Effects and records AE's values;
see docs/interop/ae-anim-samples.md. --check fails if the committed file is out of date.
"""
import json
import pathlib
import sys

FLICKS = 705_600_000
OUT = pathlib.Path(__file__).with_name("anim-cases.json")

FPS_30 = {"num": 30, "den": 1}
FPS_2997 = {"num": 30000, "den": 1001}
FPS_23976 = {"num": 24000, "den": 1001}
FPS_25 = {"num": 25, "den": 1}

ROT = ["ADBE Transform Group", "ADBE Rotate Z"]
OPACITY = ["ADBE Transform Group", "ADBE Opacity"]
SCALE = ["ADBE Transform Group", "ADBE Scale"]
POS = ["ADBE Transform Group", "ADBE Position"]
POS_X = ["ADBE Transform Group", "ADBE Position_0"]
POS_Y = ["ADBE Transform Group", "ADBE Position_1"]
COLOR_CTRL = ["ADBE Effect Parade", "ADBE Color Control", "ADBE Color Control-0001"]
POINT_CTRL = ["ADBE Effect Parade", "ADBE Point Control", "ADBE Point Control-0001"]
SLIDER_CTRL = ["ADBE Effect Parade", "ADBE Slider Control", "ADBE Slider Control-0001"]


def sec(s):
    """Seconds -> flicks key (string, as in AttrSpec.keyframes)."""
    return str(round(s * FLICKS))


def frame(n, fps=FPS_30):
    return str(n * FLICKS * fps["den"] // fps["num"])


def flicks(s):
    """Seconds -> flicks as a JSON integer (durations, start times, sample ranges)."""
    return round(s * FLICKS)


def val(v):
    if isinstance(v, (int, float)):
        return {"type": "float", "value": float(v)}
    kind = {2: "vec2", 3: "vec3", 4: "color"}[len(v)]
    return {"type": kind, "value": [float(x) for x in v]}


def ease(speed, influence):
    return {"speed": float(speed), "influence": float(influence)}


def key(v, i="linear", o=None, ein=None, eout=None, sin=None, sout=None, **flags):
    k = {"value": val(v), "interp_in": i, "interp_out": o or i}
    if ein is not None:
        k["ease_in"] = ein if isinstance(ein, list) else [ein]
    if eout is not None:
        k["ease_out"] = eout if isinstance(eout, list) else [eout]
    if sin is not None:
        k["spatial_in"] = [float(x) for x in sin]
    if sout is not None:
        k["spatial_out"] = [float(x) for x in sout]
    unknown = set(flags) - set(FLAG_ORDER)
    assert not unknown, unknown
    on = [name for name in FLAG_ORDER if flags.get(name)]
    if on:
        # bae-model KeyframeFlags field order, so serde output and this file agree.
        k["flags"] = {name: True for name in on}
    return k


def keys(*pairs):
    return {"keyframes": {t: k for t, k in pairs}}


FLAG_ORDER = ["temporal_continuous", "temporal_auto_bezier", "spatial_continuous", "spatial_auto_bezier", "roving"]
EASY = ease(0, 100 / 3)
CASES = []


def case(id, group, description, prop, attr, layer=None, comp=None, sample=None, other=None):
    c = {"id": id, "group": group, "description": description, "property": prop, "attr": attr}
    if layer:
        c["layer"] = layer
    if comp:
        c["comp"] = comp
    if sample:
        c["sample"] = sample
    if other:
        c["other"] = other
    CASES.append(c)


# ---------------------------------------------------------------- 1-D temporal (Rotation, unclamped)
case("linear_two_keys", "temporal_1d", "Two linear keys.", ROT,
     keys((sec(0), key(0)), (sec(2), key(360))))
case("linear_three_keys", "temporal_1d", "Three linear keys with different slopes.", ROT,
     keys((sec(0), key(0)), (sec(1), key(90)), (sec(3), key(-45))))
case("hold_out", "temporal_1d", "Hold keys: value jumps at each key.", ROT,
     keys((sec(0), key(0, "hold")), (sec(1), key(90, "hold")), (sec(2), key(180, "hold"))))
case("hold_in_only", "temporal_1d", "In=Hold on the middle key does not hold the segment before it.", ROT,
     keys((sec(0), key(0)), (sec(1), key(90, "hold", "linear")), (sec(2), key(0))))
case("easy_ease", "temporal_1d", "Easy Ease on both keys (speed 0, influence 33.33%).", ROT,
     keys((sec(0), key(0, "bezier", ein=EASY, eout=EASY)), (sec(2), key(360, "bezier", ein=EASY, eout=EASY))))
case("ease_asymmetric", "temporal_1d", "Out influence 90%, in influence 10%.", ROT,
     keys((sec(0), key(0, "bezier", eout=ease(0, 90), ein=ease(0, 90))),
          (sec(2), key(360, "bezier", ein=ease(0, 10), eout=ease(0, 10)))))
case("ease_speed_overshoot", "temporal_1d", "Non-zero speeds that overshoot and undershoot.", ROT,
     keys((sec(0), key(0, "bezier", eout=ease(300, 50), ein=ease(300, 50))),
          (sec(2), key(180, "bezier", ein=ease(-100, 75), eout=ease(-100, 75)))))
case("ease_influence_max", "temporal_1d", "Influence 100% on both sides.", ROT,
     keys((sec(0), key(0, "bezier", ein=ease(0, 100), eout=ease(0, 100))),
          (sec(2), key(360, "bezier", ein=ease(0, 100), eout=ease(0, 100)))))
case("ease_influence_min", "temporal_1d", "Influence 0.1% (AE minimum) on both sides.", ROT,
     keys((sec(0), key(0, "bezier", ein=ease(0, 0.1), eout=ease(0, 0.1))),
          (sec(2), key(360, "bezier", ein=ease(0, 0.1), eout=ease(0, 0.1)))))
case("ease_bump_equal_values", "temporal_1d", "Equal values with non-zero speeds make a bump.", ROT,
     keys((sec(0), key(90, "bezier", eout=ease(200, 50), ein=ease(200, 50))),
          (sec(2), key(90, "bezier", ein=ease(-200, 50), eout=ease(-200, 50)))))
case("mixed_linear_bezier", "temporal_1d", "Linear out on the first key, Bezier in on the second.", ROT,
     keys((sec(0), key(0, "linear")), (sec(2), key(360, "bezier", "bezier", ein=ease(0, 50), eout=ease(0, 50)))))
case("extrapolation", "temporal_1d", "Constant value before the first and after the last key.", ROT,
     keys((sec(1), key(10, "bezier", ein=EASY, eout=EASY)), (sec(3), key(50, "bezier", ein=EASY, eout=EASY))))
case("temporal_continuous", "temporal_1d", "Continuous Bezier on the middle key.", ROT,
     keys((sec(0), key(0, "bezier", ein=EASY, eout=EASY)),
          (sec(1), key(120, "bezier", ein=ease(120, 40), eout=ease(120, 60), temporal_continuous=True)),
          (sec(3), key(0, "bezier", ein=EASY, eout=EASY))))
case("temporal_auto_bezier", "temporal_1d", "Auto Bezier on four keys (AE computes the eases).", ROT,
     keys(*[(sec(t), key(v, "bezier", temporal_auto_bezier=True, temporal_continuous=True))
            for t, v in [(0, 0), (1, 100), (2, 30), (3, 200)]]))
case("keys_every_frame", "temporal_1d", "Ten linear keys on consecutive frames.", ROT,
     keys(*[(frame(n), key((n * 37) % 50)) for n in range(10)]), sample={"from": 0, "to": int(frame(12)), "per_frame": 8})
case("keys_between_frames", "temporal_1d", "Keys at times that are not on frame boundaries.", ROT,
     keys((sec(0.0123), key(0, "bezier", ein=EASY, eout=EASY)), (sec(1.4567), key(90, "bezier", ein=EASY, eout=EASY))))
case("opacity_overshoot_clamp", "temporal_1d", "Opacity overshoot beyond 100% is clamped.", OPACITY,
     keys((sec(0), key(0, "bezier", eout=ease(500, 80), ein=ease(500, 80))),
          (sec(1), key(100, "bezier", ein=ease(0, 20), eout=ease(0, 20)))))

# ---------------------------------------------------------------- multi-dimensional non-spatial (Scale)
case("scale_linear", "temporal_nd", "Scale, linear.", SCALE,
     keys((sec(0), key([0, 0, 100])), (sec(2), key([200, 50, 100]))))
case("scale_per_dimension_ease", "temporal_nd", "Scale with a different ease per dimension.", SCALE,
     keys((sec(0), key([0, 0, 100], "bezier", ein=[ease(0, 10), ease(0, 50), ease(0, 33.333333)],
                       eout=[ease(0, 90), ease(40, 50), ease(0, 33.333333)])),
          (sec(2), key([200, 50, 100], "bezier", ein=[ease(0, 10), ease(-30, 80), ease(0, 33.333333)],
                       eout=[ease(0, 10), ease(0, 80), ease(0, 33.333333)]))))
case("scale_hold_mixed", "temporal_nd", "Scale: hold then linear.", SCALE,
     keys((sec(0), key([100, 100, 100], "linear", "hold")), (sec(1), key([50, 150, 100])), (sec(2), key([100, 100, 100]))))

# ---------------------------------------------------------------- color (Color Control)
CTRL_COLOR = {"effects": ["ADBE Color Control"]}
case("color_linear", "color", "Color, linear.", COLOR_CTRL,
     keys((sec(0), key([1, 0, 0, 1])), (sec(2), key([0, 0, 1, 1]))), layer=CTRL_COLOR)
case("color_ease", "color", "Color with one shared ease curve.", COLOR_CTRL,
     keys((sec(0), key([1, 0, 0, 1], "bezier", ein=ease(0, 50), eout=ease(0, 50))),
          (sec(2), key([0, 1, 0.5, 1], "bezier", ein=ease(0, 50), eout=ease(0, 50)))), layer=CTRL_COLOR)
case("color_hold", "color", "Color, hold.", COLOR_CTRL,
     keys((sec(0), key([1, 1, 0, 1], "hold")), (sec(1), key([0, 1, 1, 1], "hold"))), layer=CTRL_COLOR)

# ---------------------------------------------------------------- spatial (Position)
case("spatial_linear", "spatial", "Straight motion path, linear timing.", POS,
     keys((sec(0), key([200, 540, 0], sin=[0, 0, 0], sout=[0, 0, 0])),
          (sec(2), key([1720, 540, 0], sin=[0, 0, 0], sout=[0, 0, 0]))))
case("spatial_bezier_tangents", "spatial", "Curved path from explicit tangents, linear timing.", POS,
     keys((sec(0), key([200, 800, 0], sin=[0, 0, 0], sout=[200, -600, 0])),
          (sec(2), key([1720, 800, 0], sin=[-200, -600, 0], sout=[0, 0, 0]))))
case("spatial_bezier_eased", "spatial", "Curved path with Easy Ease (arc-length timing).", POS,
     keys((sec(0), key([200, 800, 0], "bezier", ein=EASY, eout=EASY, sin=[0, 0, 0], sout=[200, -600, 0])),
          (sec(2), key([1720, 800, 0], "bezier", ein=EASY, eout=EASY, sin=[-200, -600, 0], sout=[0, 0, 0]))))
case("spatial_auto_bezier", "spatial", "Four keys with spatial Auto Bezier (AE computes tangents).", POS,
     keys(*[(sec(t), key(p, spatial_auto_bezier=True, spatial_continuous=True))
            for t, p in [(0, [200, 540, 0]), (1, [700, 200, 0]), (2, [1200, 900, 0]), (3, [1700, 540, 0])]]))
case("spatial_continuous", "spatial", "Continuous Bezier with unequal tangent lengths.", POS,
     keys((sec(0), key([200, 540, 0], sin=[0, 0, 0], sout=[100, -100, 0])),
          (sec(1), key([960, 300, 0], sin=[-300, 0, 0], sout=[100, 0, 0], spatial_continuous=True)),
          (sec(2), key([1720, 540, 0], sin=[-100, -100, 0], sout=[0, 0, 0]))))
case("spatial_roving", "spatial", "Roving middle keys: AE retimes them for constant speed.", POS,
     keys(*[(sec(t), key(p, "bezier", ein=EASY, eout=EASY, spatial_auto_bezier=True, spatial_continuous=True,
                         roving=r))
            for t, p, r in [(0, [200, 540, 0], False), (0.5, [500, 200, 0], True), (1, [1000, 300, 0], True),
                            (3, [1700, 800, 0], False)]]))
case("spatial_hold", "spatial", "Position hold keys.", POS,
     keys((sec(0), key([200, 540, 0], "hold")), (sec(1), key([960, 200, 0], "hold")), (sec(2), key([1720, 540, 0], "hold"))))
case("spatial_3d", "spatial", "3D layer: motion in z with tangents.", POS,
     keys((sec(0), key([960, 540, 0], "bezier", ein=EASY, eout=EASY, sin=[0, 0, 0], sout=[0, -200, -500])),
          (sec(2), key([960, 540, -1500], "bezier", ein=EASY, eout=EASY, sin=[300, 0, 0], sout=[0, 0, 0]))),
     layer={"three_d": True})
case("spatial_zero_length", "spatial", "Two keys at the same point with non-zero speeds.", POS,
     keys((sec(0), key([960, 540, 0], "bezier", eout=ease(300, 50), ein=ease(300, 50), sin=[0, 0, 0], sout=[0, 0, 0])),
          (sec(2), key([960, 540, 0], "bezier", ein=ease(300, 50), eout=ease(300, 50), sin=[0, 0, 0], sout=[0, 0, 0]))))
case("spatial_speed_clamp", "spatial", "Speeds too high for the path length (influence is clamped).", POS,
     keys((sec(0), key([900, 540, 0], "bezier", eout=ease(5000, 80), ein=ease(5000, 80), sin=[0, 0, 0], sout=[0, 0, 0])),
          (sec(1), key([1000, 540, 0], "bezier", ein=ease(5000, 80), eout=ease(5000, 80), sin=[0, 0, 0], sout=[0, 0, 0]))))
case("separated_dimensions", "spatial", "Separate Dimensions: X and Y keyed independently.", POS_Y,
     keys((sec(0), key(540, "bezier", ein=EASY, eout=EASY)), (sec(1), key(200, "bezier", ein=ease(0, 80), eout=ease(0, 80))),
          (sec(3), key(900))),
     layer={"separate_position": True},
     other=[{"property": POS_X, "attr": keys((sec(0), key(200)), (sec(3), key(1700)))}])
case("point_control", "spatial", "Point Control (2D spatial effect parameter) with tangents.", POINT_CTRL,
     keys((sec(0), key([0, 0], "bezier", ein=EASY, eout=EASY, sin=[0, 0, 0], sout=[400, 0, 0])),
          (sec(2), key([800, 600], "bezier", ein=EASY, eout=EASY, sin=[0, -400, 0], sout=[0, 0, 0]))),
     layer={"effects": ["ADBE Point Control"]})

# ---------------------------------------------------------------- layer time and frame rates
case("layer_start_offset", "time", "Layer start time 1 s: checks the key time base (keys in comp time).", ROT,
     keys((sec(1.5), key(0)), (sec(3), key(90))), layer={"start_time": flicks(1)})
case("layer_stretch_200", "time", "Layer stretched to 200%.", ROT,
     keys((sec(0), key(0, "bezier", ein=EASY, eout=EASY)), (sec(2), key(90, "bezier", ein=EASY, eout=EASY))),
     layer={"stretch": 200})
case("layer_stretch_reverse", "time", "Layer stretch -100% (time-reversed).", ROT,
     keys((sec(0), key(0)), (sec(2), key(90))), layer={"stretch": -100})
for cid, fps, label in [("comp_29_97", FPS_2997, "29.97"), ("comp_23_976", FPS_23976, "23.976"), ("comp_25", FPS_25, "25")]:
    case(cid, "time", f"{label} fps comp, keys on frames 0, 30, 60.", ROT,
         keys((frame(0, fps), key(0, "bezier", ein=EASY, eout=EASY)),
              (frame(30, fps), key(90, "bezier", ein=ease(30, 50), eout=ease(30, 50))),
              (frame(60, fps), key(0, "bezier", ein=EASY, eout=EASY))),
         comp={"frame_rate": fps})

# ---------------------------------------------------------------- expressions (deterministic only)
def expr(source):
    return {"source": source, "enabled": True}


def expr_case(id, description, source, attr=None, prop=ROT, **kw):
    a = dict(attr or {"value": val(0)})
    a["expression"] = expr(source)
    case(id, "expression", description, prop, a, **kw)


ROT_LIN = keys((sec(0), key(0)), (sec(1), key(90)))
ROT_EASE = keys((sec(0), key(0, "bezier", ein=EASY, eout=EASY)), (sec(1), key(90, "bezier", ein=ease(60, 40), eout=ease(60, 40))))
expr_case("expr_time", "time * 100", "time * 100")
expr_case("expr_value_offset", "value + 10 on linear keys", "value + 10", ROT_LIN)
expr_case("expr_linear", "linear()", "linear(time, 1, 3, 0, 100)")
expr_case("expr_ease", "ease()", "ease(time, 0, 2, 0, 360)")
expr_case("expr_ease_in_out", "easeIn() + easeOut()", "easeIn(time, 0, 2, 0, 100) + easeOut(time, 0, 2, 0, 100)")
expr_case("expr_loop_cycle", "loopOut(\"cycle\")", "loopOut(\"cycle\")", ROT_LIN)
expr_case("expr_loop_pingpong", "loopOut(\"pingpong\")", "loopOut(\"pingpong\")", ROT_EASE)
expr_case("expr_loop_offset", "loopOut(\"offset\")", "loopOut(\"offset\")", ROT_LIN)
expr_case("expr_loop_continue", "loopOut(\"continue\") after an eased key", "loopOut(\"continue\")", ROT_EASE)
expr_case("expr_loop_in_cycle", "loopIn(\"cycle\")", "loopIn(\"cycle\")", keys((sec(2), key(0)), (sec(3), key(90))))
expr_case("expr_value_at_time", "valueAtTime(time - 0.5)", "valueAtTime(time - 0.5)", ROT_EASE)
expr_case("expr_velocity", "velocity of eased keys", "velocity * 0.1", ROT_EASE)
expr_case("expr_math", "Math functions and clamp()", "Math.sin(time * Math.PI) * 50 + clamp(time * 50, 0, 100)")
expr_case("expr_frames", "timeToFrames() and framesToTime()", "timeToFrames(time) + framesToTime(10)")
expr_case("expr_posterize", "posterizeTime(4)", "posterizeTime(4); time * 100")
expr_case("expr_comp_props", "thisComp / thisLayer attributes",
          "thisComp.frameDuration * 1000 + thisLayer.inPoint + thisComp.duration")
expr_case("expr_vector_position", "Array arithmetic on Position (2D layer: value has two components in expressions)",
          "[value[0] + Math.cos(time) * 100, value[1] + Math.sin(time) * 100]",
          keys((sec(0), key([200, 540, 0])), (sec(3), key([1720, 540, 0]))), prop=POS)
expr_case("expr_cross_property", "Reads another keyed property of the same layer", "transform.position[0] / 10",
          other=[{"property": POS, "attr": keys((sec(0), key([200, 540, 0], "bezier", ein=EASY, eout=EASY)),
                                                (sec(2), key([1720, 540, 0], "bezier", ein=EASY, eout=EASY)))}])
expr_case("expr_speed", "speed of a keyed Position", "transform.position.speed / 100",
          other=[{"property": POS, "attr": keys((sec(0), key([200, 540, 0], "bezier", ein=EASY, eout=EASY)),
                                                (sec(2), key([1720, 900, 0], "bezier", ein=EASY, eout=EASY)))}])
expr_case("expr_length", "length() between points", "length(transform.position, [960, 540]) / 10",
          other=[{"property": POS, "attr": keys((sec(0), key([200, 540, 0])), (sec(3), key([1720, 100, 0])))}])
expr_case("expr_slider_control", "Effect parameter by matchName (language independent)",
          "effect(\"ADBE Slider Control\")(\"ADBE Slider Control-0001\") * 2",
          layer={"effects": ["ADBE Slider Control"]},
          other=[{"property": SLIDER_CTRL, "attr": keys((sec(0), key(0)), (sec(2), key(50, "bezier", ein=EASY, eout=EASY)))}])


def document():
    return {
        "format": "bae-anim-cases",
        "format_version": "0.1.0",
        "description": "Generated by bridges/ae/samples/gen_anim_cases.py; see docs/interop/ae-anim-samples.md.",
        "defaults": {
            "comp": {"width": 1920, "height": 1080, "frame_rate": FPS_30, "duration": flicks(4)},
            "layer": {"type": "null"},
            "sample": {"from": 0, "to": flicks(4), "per_frame": 2},
        },
        "cases": CASES,
    }


def main():
    ids = [c["id"] for c in CASES]
    assert len(ids) == len(set(ids)), "duplicate case ids"
    text = json.dumps(document(), indent=1, ensure_ascii=False) + "\n"
    if "--check" in sys.argv:
        if OUT.read_text(encoding="utf-8") != text:
            print(f"{OUT} is out of date: run {sys.argv[0]}", file=sys.stderr)
            return 1
        print(f"{OUT}: up to date ({len(CASES)} cases)")
        return 0
    OUT.write_text(text, encoding="utf-8")
    print(f"wrote {OUT} ({len(CASES)} cases)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
