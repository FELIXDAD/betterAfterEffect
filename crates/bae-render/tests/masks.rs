//! Layer masks on the GPU against CPU references of AE's mask formulas
//! (docs/spec/ae/compositing-3d-color.md AE-CMP-038..045).

use std::sync::Arc;

use bae_gpu::{GpuContext, wgpu};
use bae_model::{BezierPath, EffectDef, FrameRate, PrimPath, Time, Value};
use bae_render::fixture::{SceneBuilder, ellipse_path, rect_path};
use bae_render::mask::FEATHER_TO_SIGMA;
use bae_render::{
    AnimEvaluator, CompEvaluator, Effect, EffectParams, EncodeContext, GpuImage, IRect,
    LayerContent, MaskMode, OutputAlpha, RenderComp, RenderEffect, RenderError, RenderLayer,
    RenderMask, RenderRequest, Renderer, Resolution, TrackMatte, TrackMatteMode,
};
use glam::{DAffine2, DVec2};

fn gpu() -> Option<Arc<GpuContext>> {
    bae_gpu::testing::shared_context()
}

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

/// A white solid of `size` with its top-left corner at `at`.
fn solid(name: &str, size: [u32; 2], at: [f64; 2]) -> RenderLayer {
    let mut l = RenderLayer::new(
        p(&format!("/comps/main/layers/{name}")),
        LayerContent::Solid {
            color: [1.0, 1.0, 1.0, 1.0],
            width: size[0],
            height: size[1],
        },
    );
    l.transform = DAffine2::from_translation(DVec2::new(at[0], at[1]));
    l
}

fn mask(mode: MaskMode, shape: BezierPath) -> RenderMask {
    RenderMask {
        path: p("/comps/main/layers/l/masks/m"),
        shape,
        mode,
        inverted: false,
        opacity: 1.0,
        feather: [0.0, 0.0],
        expansion: 0.0,
    }
}

struct Image {
    w: u32,
    data: Vec<u8>,
}

impl Image {
    fn alpha(&self, x: u32, y: u32) -> u8 {
        self.data[((y * self.w + x) * 4 + 3) as usize]
    }

    fn alpha_f(&self, x: u32, y: u32) -> f64 {
        f64::from(self.alpha(x, y)) / 255.0
    }
}

fn render(r: &mut Renderer, comp: &RenderComp, req: &RenderRequest) -> Image {
    let frame = r.render_frame(comp, req).unwrap();
    assert!(
        frame.warnings.iter().all(|w| !w.contains("mask")),
        "{:?}",
        frame.warnings
    );
    let img = r.read_rgba8(&frame, OutputAlpha::Straight).unwrap();
    Image {
        w: img.width,
        data: img.data,
    }
}

/// One layer (a white solid filling a `w`×`h` comp) with `masks`.
fn masked_comp(w: u32, h: u32, masks: Vec<RenderMask>) -> RenderComp {
    let mut comp = RenderComp::new(p("/comps/main"), w, h);
    let mut l = solid("l", [w, h], [0.0, 0.0]);
    l.masks = masks;
    comp.layers.push(l);
    comp
}

fn to8(v: f64) -> i32 {
    (v.clamp(0.0, 1.0) * 255.0).round() as i32
}

fn assert_alpha(img: &Image, x: u32, y: u32, expected: f64, tol: i32, what: &str) {
    let got = i32::from(img.alpha(x, y));
    assert!(
        (got - to8(expected)).abs() <= tol,
        "{what}: alpha at ({x}, {y}) = {got}, expected {} ({expected:.4})",
        to8(expected)
    );
}

/// AE-CMP-038 accumulation for per-mask values `m` (top first).
fn combine(modes: &[MaskMode], m: &[f64]) -> f64 {
    let mut acc = match modes[0] {
        MaskMode::Subtract | MaskMode::Intersect | MaskMode::Darken => 1.0,
        _ => 0.0,
    };
    for (mode, m) in modes.iter().zip(m) {
        acc = match mode {
            MaskMode::Add => acc + m - acc * m,
            MaskMode::Subtract => acc * (1.0 - m),
            MaskMode::Intersect => acc * m,
            MaskMode::Lighten => f64::max(acc, *m),
            MaskMode::Darken => f64::min(acc, *m),
            MaskMode::Difference => acc + m - 2.0 * acc * m,
            MaskMode::None => acc,
        };
    }
    acc
}

const MODES: [MaskMode; 6] = [
    MaskMode::Add,
    MaskMode::Subtract,
    MaskMode::Intersect,
    MaskMode::Lighten,
    MaskMode::Darken,
    MaskMode::Difference,
];

#[test]
fn rect_mask_is_pixel_exact_and_half_pixel_edges_are_half_covered() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let req = RenderRequest::default();
    // Integer edges: pixels 20..60 × 10..30 inside.
    let comp = masked_comp(
        80,
        40,
        vec![mask(MaskMode::Add, rect_path(20.0, 10.0, 60.0, 30.0))],
    );
    let img = render(&mut r, &comp, &req);
    for (x, y, a) in [
        (19, 20, 0),
        (20, 20, 255),
        (59, 20, 255),
        (60, 20, 0),
        (40, 9, 0),
        (40, 10, 255),
        (40, 29, 255),
        (40, 30, 0),
        (0, 0, 0),
    ] {
        assert_eq!(img.alpha(x, y), a, "({x}, {y})");
    }
    // Half-pixel edges: the boundary pixels are half covered (a quarter at the
    // corner).
    let comp = masked_comp(
        80,
        40,
        vec![mask(MaskMode::Add, rect_path(20.5, 10.5, 59.5, 29.5))],
    );
    let img = render(&mut r, &comp, &req);
    assert_alpha(&img, 20, 20, 0.5, 1, "left edge");
    assert_alpha(&img, 59, 20, 0.5, 1, "right edge");
    assert_alpha(&img, 40, 10, 0.5, 1, "top edge");
    assert_alpha(&img, 20, 10, 0.25, 1, "corner");
    assert_alpha(&img, 21, 11, 1.0, 0, "inside");
}

#[test]
fn ellipse_mask_coverage_matches_its_area() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let comp = masked_comp(
        100,
        100,
        vec![mask(MaskMode::Add, ellipse_path(10.0, 20.0, 90.0, 80.0))],
    );
    let img = render(&mut r, &comp, &RenderRequest::default());
    let sum: f64 = (0..100)
        .flat_map(|y| (0..100).map(move |x| (x, y)))
        .map(|(x, y)| img.alpha_f(x, y))
        .sum();
    let area = std::f64::consts::PI * 40.0 * 30.0;
    // The cubic approximation of a circle is within 0.03 % of its radius.
    assert!((sum - area).abs() / area < 0.003, "{sum} vs {area}");
    assert_alpha(&img, 50, 50, 1.0, 0, "centre");
    assert_alpha(&img, 10, 20, 0.0, 0, "bounding box corner");
}

#[test]
fn every_mode_alone_and_after_a_partial_add_mask() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let req = RenderRequest::default();
    let a = rect_path(10.0, 10.0, 50.0, 50.0);
    let b = rect_path(30.0, 30.0, 70.0, 70.0);
    // Probe points: A only, B only, both, neither.
    let probes = [(20, 20), (60, 60), (40, 40), (75, 5)];
    let inside = |x: u32, y: u32, r: [f64; 4]| {
        let (fx, fy) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
        if fx > r[0] && fx < r[2] && fy > r[1] && fy < r[3] {
            1.0
        } else {
            0.0
        }
    };
    let (ra, rb) = ([10.0, 10.0, 50.0, 50.0], [30.0, 30.0, 70.0, 70.0]);
    for mode in MODES {
        // Alone (exercises the start value): mask B at 50 % opacity.
        let mut mb = mask(mode, b.clone());
        mb.opacity = 0.5;
        let img = render(&mut r, &masked_comp(80, 80, vec![mb.clone()]), &req);
        for (x, y) in probes {
            let want = combine(&[mode], &[0.5 * inside(x, y, rb)]);
            assert_alpha(&img, x, y, want, 1, &format!("{mode:?} alone"));
        }
        // Below a 60 % Add mask A (exercises the accumulator and blend states).
        let mut ma = mask(MaskMode::Add, a.clone());
        ma.opacity = 0.6;
        let img = render(&mut r, &masked_comp(80, 80, vec![ma, mb]), &req);
        for (x, y) in probes {
            let want = combine(
                &[MaskMode::Add, mode],
                &[0.6 * inside(x, y, ra), 0.5 * inside(x, y, rb)],
            );
            assert_alpha(&img, x, y, want, 1, &format!("Add + {mode:?}"));
        }
    }
}

#[test]
fn three_masks_and_none_or_open_masks() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let req = RenderRequest::default();
    let mut open = rect_path(0.0, 0.0, 80.0, 80.0);
    open.closed = false;
    let masks = vec![
        mask(MaskMode::None, rect_path(0.0, 0.0, 80.0, 80.0)),
        mask(MaskMode::Add, open),
        mask(MaskMode::Add, rect_path(0.0, 0.0, 40.0, 80.0)),
        mask(MaskMode::Add, rect_path(40.0, 0.0, 80.0, 80.0)),
        mask(MaskMode::Subtract, rect_path(30.0, 30.0, 50.0, 50.0)),
    ];
    let img = render(&mut r, &masked_comp(80, 80, masks), &req);
    assert_alpha(&img, 10, 10, 1.0, 0, "first add");
    assert_alpha(&img, 70, 10, 1.0, 0, "second add");
    assert_alpha(&img, 40, 40, 0.0, 0, "subtracted");
    // Only None / open masks: the layer is untouched.
    let mut open = rect_path(10.0, 10.0, 20.0, 20.0);
    open.closed = false;
    let img = render(
        &mut r,
        &masked_comp(
            80,
            80,
            vec![
                mask(MaskMode::None, rect_path(10.0, 10.0, 20.0, 20.0)),
                mask(MaskMode::Add, open),
            ],
        ),
        &req,
    );
    assert_alpha(&img, 70, 70, 1.0, 0, "no active mask");
}

#[test]
fn inverted_masks_and_mask_opacity() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let req = RenderRequest::default();
    let mut m = mask(MaskMode::Add, rect_path(20.0, 20.0, 60.0, 60.0));
    m.inverted = true;
    let img = render(&mut r, &masked_comp(80, 80, vec![m.clone()]), &req);
    assert_alpha(&img, 40, 40, 0.0, 0, "inside inverted");
    assert_alpha(&img, 5, 5, 1.0, 0, "outside inverted");
    m.opacity = 0.4;
    let img = render(&mut r, &masked_comp(80, 80, vec![m.clone()]), &req);
    assert_alpha(&img, 40, 40, 0.0, 0, "inside inverted 40 %");
    assert_alpha(&img, 5, 5, 0.4, 1, "outside inverted 40 %");
    m.inverted = false;
    let img = render(&mut r, &masked_comp(80, 80, vec![m.clone()]), &req);
    assert_alpha(&img, 40, 40, 0.4, 1, "inside 40 %");
    assert_alpha(&img, 5, 5, 0.0, 0, "outside 40 %");
    // Subtract + inverted = Add.
    m.opacity = 1.0;
    m.inverted = true;
    m.mode = MaskMode::Subtract;
    let img = render(&mut r, &masked_comp(80, 80, vec![m]), &req);
    assert_alpha(&img, 40, 40, 1.0, 0, "subtract inverted inside");
    assert_alpha(&img, 5, 5, 0.0, 0, "subtract inverted outside");
    // Opacity 0 Add mask hides the layer (AE-CMP-040).
    let mut zero = mask(MaskMode::Add, rect_path(20.0, 20.0, 60.0, 60.0));
    zero.opacity = 0.0;
    let comp = masked_comp(80, 80, vec![zero]);
    let frame = r.render_frame(&comp, &req).unwrap();
    assert_eq!(frame.stats.draws, 0, "nothing to draw");
}

#[test]
fn expansion_grows_with_round_corners_and_shrinks_with_sharp_ones() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let req = RenderRequest::default();
    let mut m = mask(MaskMode::Add, rect_path(30.0, 30.0, 70.0, 70.0));
    m.expansion = 5.0;
    let img = render(&mut r, &masked_comp(100, 100, vec![m.clone()]), &req);
    assert_alpha(&img, 25, 50, 1.0, 1, "grown left edge");
    assert_alpha(&img, 24, 50, 0.0, 1, "outside grown edge");
    assert_alpha(&img, 74, 50, 1.0, 1, "grown right edge");
    assert_alpha(&img, 75, 50, 0.0, 1, "outside grown right edge");
    // Rounded corner: (25.5, 25.5) is 6.4 px from the corner (30, 30).
    assert_alpha(&img, 25, 25, 0.0, 1, "rounded corner");
    assert_alpha(&img, 27, 27, 1.0, 1, "inside the rounded corner");
    m.expansion = -5.0;
    let img = render(&mut r, &masked_comp(100, 100, vec![m.clone()]), &req);
    assert_alpha(&img, 35, 50, 1.0, 1, "shrunk left edge");
    assert_alpha(&img, 34, 50, 0.0, 1, "outside shrunk edge");
    assert_alpha(&img, 35, 35, 1.0, 1, "sharp inner corner");
    m.expansion = -25.0;
    let img = render(&mut r, &masked_comp(100, 100, vec![m]), &req);
    assert_alpha(&img, 50, 50, 0.0, 0, "shrunk away");
}

/// Coverage of a vertical edge at `edge` blurred by a Gaussian of `sigma`, at
/// pixel centre `x + 0.5` (inside to the left).
fn feathered_step(x: u32, edge: f64, sigma: f64) -> f64 {
    0.5 * erfc((f64::from(x) + 0.5 - edge) / (sigma * std::f64::consts::SQRT_2))
}

fn erfc(x: f64) -> f64 {
    // Abramowitz–Stegun 7.1.26 (|error| < 1.5e-7).
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let y = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))))
        * (-x * x).exp();
    if x >= 0.0 { y } else { 2.0 - y }
}

#[test]
fn feather_is_a_gaussian_centred_on_the_edge_per_axis() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let req = RenderRequest::default();
    for (feather, tol) in [(20.0, 2), (100.0, 3)] {
        let w = 1000;
        let mut m = mask(MaskMode::Add, rect_path(-100.0, -100.0, 500.0, 300.0));
        m.feather = [feather as f32, 0.0];
        let img = render(&mut r, &masked_comp(w, 100, vec![m]), &req);
        let sigma = FEATHER_TO_SIGMA * feather;
        for x in (500 - (3.0 * sigma) as u32..500 + (3.0 * sigma) as u32).step_by(3) {
            let want = feathered_step(x, 500.0, sigma);
            assert_alpha(&img, x, 50, want, tol, &format!("feather {feather} x"));
        }
        // The edge (x = 500) lies between pixels 499 and 500.
        let mid = (img.alpha_f(499, 50) + img.alpha_f(500, 50)) / 2.0;
        assert!((mid - 0.5).abs() < 0.01, "half at the edge: {mid}");
    }
    // Different x / y feather: a corner region far from the layer frame.
    let mut m = mask(MaskMode::Add, rect_path(50.0, 50.0, 150.0, 150.0));
    m.feather = [30.0, 6.0];
    let img = render(&mut r, &masked_comp(200, 200, vec![m]), &req);
    let (sx, sy) = (30.0 * FEATHER_TO_SIGMA, 6.0 * FEATHER_TO_SIGMA);
    for x in [40u32, 45, 50, 55, 60] {
        // Left edge at x = 50, inside to the right.
        let want = 1.0 - feathered_step(x, 50.0, sx);
        assert_alpha(&img, x, 100, want, 2, "x feather");
    }
    for y in [46u32, 48, 50, 52, 54] {
        let want = 1.0 - feathered_step(y, 50.0, sy);
        assert_alpha(&img, 100, y, want, 2, "y feather");
    }
}

#[test]
fn feather_stops_at_the_layer_frame() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    // A mask covering the whole solid and more: the feather reads transparent
    // pixels outside the layer, so the layer's own edge softens (AE-CMP-041).
    let mut m = mask(MaskMode::Add, rect_path(-50.0, -50.0, 150.0, 150.0));
    m.feather = [20.0, 20.0];
    let mut comp = RenderComp::new(p("/comps/main"), 200, 200);
    let mut l = solid("l", [100, 100], [50.0, 50.0]);
    l.masks = vec![m];
    comp.layers.push(l);
    let img = render(&mut r, &comp, &RenderRequest::default());
    let sigma = 20.0 * FEATHER_TO_SIGMA;
    // Layer x = 0 (comp x = 50): a Gaussian of the frame edge, ≈ half.
    let edge = 1.0 - feathered_step(0, 0.0, sigma);
    assert_alpha(&img, 50, 100, edge, 2, "layer edge");
    assert_alpha(&img, 100, 100, 1.0, 1, "layer centre");
    assert_alpha(&img, 40, 100, 0.0, 0, "outside the layer");
}

#[test]
fn masks_animate_with_keyframes() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut b = SceneBuilder::new();
    let comp = b
        .comp("main", 100, 100, FrameRate::FPS_30, Time::from_seconds(2.0))
        .unwrap();
    let layer = b
        .solid_layer(&comp, "l", [1.0, 1.0, 1.0, 1.0], [100, 100], [0.0, 0.0])
        .unwrap();
    let m = b
        .mask(&layer, "m", rect_path(0.0, 0.0, 20.0, 100.0), "add")
        .unwrap();
    // The path moves 0 → 60 px right over one second; feather 0 → 10.
    b.key(
        &m,
        "path",
        Time::ZERO,
        Value::Bezier(rect_path(0.0, 0.0, 20.0, 100.0)),
    )
    .unwrap();
    b.key(
        &m,
        "path",
        Time::from_seconds(1.0),
        Value::Bezier(rect_path(60.0, 0.0, 80.0, 100.0)),
    )
    .unwrap();
    b.key(&m, "opacity", Time::ZERO, Value::Float(100.0))
        .unwrap();
    b.key(&m, "opacity", Time::from_seconds(1.0), Value::Float(50.0))
        .unwrap();
    let stage = b.into_stage();
    let mut eval = AnimEvaluator::new(None);
    let req = RenderRequest::default();
    let at = |eval: &mut AnimEvaluator, r: &mut Renderer, s: f64| {
        let rc = eval.evaluate(&stage, &comp, Time::from_seconds(s)).unwrap();
        assert_eq!(rc.layers[0].masks.len(), 1);
        render(r, &rc, &req)
    };
    let img = at(&mut eval, &mut r, 0.0);
    assert_alpha(&img, 10, 50, 1.0, 0, "t = 0 inside");
    assert_alpha(&img, 40, 50, 0.0, 0, "t = 0 outside");
    let img = at(&mut eval, &mut r, 0.5);
    // Halfway: the mask spans 30..50 at 75 % opacity.
    assert_alpha(&img, 10, 50, 0.0, 0, "t = 0.5 left");
    assert_alpha(&img, 40, 50, 0.75, 1, "t = 0.5 inside");
    let img = at(&mut eval, &mut r, 1.0);
    assert_alpha(&img, 70, 50, 0.5, 1, "t = 1 inside");
    assert_alpha(&img, 40, 50, 0.0, 0, "t = 1 left");
}

#[test]
fn masks_live_in_layer_space_and_follow_resolution() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 200, 200);
    let mut l = solid("l", [50, 50], [0.0, 0.0]);
    l.masks = vec![mask(MaskMode::Add, rect_path(10.0, 10.0, 30.0, 30.0))];
    // Scale 300 % then move: the mask's layer rect 10..30 lands at 30+20..90+20.
    l.transform = DAffine2::from_translation(DVec2::new(20.0, 20.0))
        * DAffine2::from_scale(DVec2::splat(3.0));
    comp.layers.push(l);
    let img = render(&mut r, &comp, &RenderRequest::default());
    assert_alpha(&img, 51, 80, 1.0, 0, "left edge inside (sharp at 300 %)");
    assert_alpha(&img, 49, 80, 0.0, 0, "left edge outside");
    assert_alpha(&img, 109, 80, 1.0, 0, "right edge inside");
    assert_alpha(&img, 110, 80, 0.0, 0, "right edge outside");
    // Rotated 30° and scaled 150 %: the masked area is the mask's area × 2.25.
    let mut rotated = comp.clone();
    rotated.layers[0].transform = DAffine2::from_translation(DVec2::new(100.0, 100.0))
        * DAffine2::from_angle(30f64.to_radians())
        * DAffine2::from_scale(DVec2::splat(1.5))
        * DAffine2::from_translation(DVec2::new(-20.0, -20.0));
    let img = render(&mut r, &rotated, &RenderRequest::default());
    let sum: f64 = (0..200)
        .flat_map(|y| (0..200).map(move |x| (x, y)))
        .map(|(x, y)| img.alpha_f(x, y))
        .sum();
    assert!((sum - 400.0 * 2.25).abs() < 4.0, "rotated area {sum}");
    // Half resolution: the mask edges land on half-resolution pixels.
    let req = RenderRequest {
        resolution: Resolution::Half,
        ..RenderRequest::default()
    };
    let img = render(&mut r, &comp, &req);
    assert_eq!(img.w, 100);
    assert_alpha(&img, 25, 40, 1.0, 0, "half res inside");
    assert_alpha(&img, 24, 40, 0.0, 0, "half res outside");
    assert_alpha(&img, 54, 40, 1.0, 0, "half res right inside");
    assert_alpha(&img, 55, 40, 0.0, 0, "half res right outside");
}

#[test]
fn masked_layers_as_track_mattes() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 100, 100);
    // Matte layer (index 0): a solid masked to 20..60, video off.
    let mut matte = solid("matte", [100, 100], [0.0, 0.0]);
    matte.visible = false;
    matte.masks = vec![mask(MaskMode::Add, rect_path(20.0, 0.0, 60.0, 100.0))];
    let mut fill = solid("fill", [100, 100], [0.0, 0.0]);
    fill.track_matte = Some(TrackMatte {
        layer: 0,
        mode: TrackMatteMode::Alpha,
    });
    comp.layers.push(matte);
    comp.layers.push(fill);
    let img = render(&mut r, &comp, &RenderRequest::default());
    assert_alpha(&img, 10, 50, 0.0, 0, "outside the matte's mask");
    assert_alpha(&img, 40, 50, 1.0, 0, "inside the matte's mask");
}

/// A test effect that ignores its input and fills its whole output opaque white:
/// if masks run before effects (AE), the result is not masked.
struct OpaqueFill(EffectDef);

impl Effect for OpaqueFill {
    fn def(&self) -> &EffectDef {
        &self.0
    }

    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        params: &EffectParams<'_>,
        input: &GpuImage,
        output_roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let s = input.pixel_scale;
        let px = |v: i32| (f64::from(v) * s).round() as i32;
        let rect = IRect::new(
            px(output_roi.x0),
            px(output_roi.y0),
            px(output_roi.x1),
            px(output_roi.y1),
        );
        let _ = params;
        let out = ctx.allocate(rect, s)?;
        ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("test.opaque"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &out.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        Ok(out)
    }
}

#[test]
fn masks_apply_before_effects() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    r.effects_mut().register(Arc::new(OpaqueFill(EffectDef {
        match_name: "test.opaque".into(),
        name: "Opaque".into(),
        category: String::new(),
        plugin: None,
        params: Vec::new(),
        thread_safe: true,
        gpu: true,
    })));
    let mut m = mask(MaskMode::Add, rect_path(20.0, 20.0, 60.0, 60.0));
    let mut comp = masked_comp(80, 80, vec![m.clone()]);
    comp.layers[0].effects.push(RenderEffect {
        path: p("/comps/main/layers/l/effects/fx"),
        effect: "test.opaque".into(),
        enabled: true,
        params: Vec::new(),
    });
    // The effect sees the masked layer: bounds shrink to the mask, then the
    // effect fills them; outside the mask's bounds nothing is drawn.
    let img = render(&mut r, &comp, &RenderRequest::default());
    assert_alpha(&img, 40, 40, 1.0, 0, "inside");
    assert_alpha(&img, 5, 5, 0.0, 0, "outside the mask bounds");
    // An inverted mask leaves the whole layer; the effect paints over the hole.
    m.inverted = true;
    comp.layers[0].masks = vec![m];
    let img = render(&mut r, &comp, &RenderRequest::default());
    assert_alpha(&img, 40, 40, 1.0, 0, "hole filled by the effect");
}

#[test]
fn masked_solid_with_effect_reads_the_masked_pixels() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    // A pass-through effect: the masked pixels come out unchanged.
    struct Copy(EffectDef);
    impl Effect for Copy {
        fn def(&self) -> &EffectDef {
            &self.0
        }
        fn render(
            &self,
            _ctx: &mut EncodeContext<'_>,
            _params: &EffectParams<'_>,
            input: &GpuImage,
            _roi: IRect,
        ) -> Result<GpuImage, RenderError> {
            Ok(input.clone())
        }
    }
    r.effects_mut().register(Arc::new(Copy(EffectDef {
        match_name: "test.copy".into(),
        name: "Copy".into(),
        category: String::new(),
        plugin: None,
        params: Vec::new(),
        thread_safe: true,
        gpu: true,
    })));
    let mut m = mask(MaskMode::Add, rect_path(20.5, 20.0, 60.0, 60.0));
    m.opacity = 0.5;
    let mut comp = masked_comp(80, 80, vec![m]);
    comp.layers[0].effects.push(RenderEffect {
        path: p("/comps/main/layers/l/effects/fx"),
        effect: "test.copy".into(),
        enabled: true,
        params: Vec::new(),
    });
    let img = render(&mut r, &comp, &RenderRequest::default());
    assert_alpha(&img, 40, 40, 0.5, 1, "inside");
    assert_alpha(&img, 20, 40, 0.25, 1, "half-covered edge");
    assert_alpha(&img, 5, 5, 0.0, 0, "outside");
}

#[test]
fn many_masked_layers_share_atlases_without_crosstalk() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let req = RenderRequest::default();
    // Twelve layers side by side, each with its own masks (feathered, inverted,
    // expanded, several per layer) – all packed into the same atlas pages.
    let layers: Vec<RenderLayer> = (0..12)
        .map(|i| {
            let x = f64::from(i % 4) * 60.0;
            let y = f64::from(i / 4) * 60.0;
            let mut l = solid(&format!("l{i}"), [60, 60], [x, y]);
            let mut a = mask(MaskMode::Add, ellipse_path(5.0, 8.0, 55.0, 52.0));
            a.feather = [f64::from(i) as f32 * 3.0, f64::from(11 - i) as f32];
            a.inverted = i % 4 == 1;
            a.expansion = if i % 3 == 2 { -4.0 } else { 0.0 };
            l.masks.push(a);
            if i % 2 == 0 {
                let mut b = mask(MODES[i as usize % 6], rect_path(20.0, 20.0, 40.0, 40.0));
                b.opacity = 0.7;
                l.masks.push(b);
            }
            l
        })
        .collect();
    let mut all = RenderComp::new(p("/comps/main"), 240, 180);
    all.layers = layers.clone();
    let together = render(&mut r, &all, &req);
    for (i, layer) in layers.into_iter().enumerate() {
        let mut alone = RenderComp::new(p("/comps/main"), 240, 180);
        alone.layers.push(layer);
        let img = render(&mut r, &alone, &req);
        let (x0, y0) = ((i % 4) as u32 * 60, (i / 4) as u32 * 60);
        for y in y0..y0 + 60 {
            for x in x0..x0 + 60 {
                let (a, b) = (img.alpha(x, y), together.alpha(x, y));
                assert!(
                    a.abs_diff(b) <= 1,
                    "layer {i} at ({x}, {y}): alone {a}, together {b}"
                );
            }
        }
    }
}
