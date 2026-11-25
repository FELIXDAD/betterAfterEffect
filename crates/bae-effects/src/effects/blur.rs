//! Gaussian Blur (AE-FX-300/301) and Fast Box Blur (AE-FX-302/303).

use bae_model::EffectDef;
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};

use crate::blur::{self, Edge};
use crate::catalog::DefBuilder;
use crate::params::Params;
use crate::target::{EffectTarget, TargetEffect};

const DIMENSIONS: [&str; 3] = ["Horizontal and Vertical", "Horizontal", "Vertical"];

/// Gaussian Blur: Blurriness → standard deviation. Fitted to AE 26 impulse
/// responses (Blurriness 2–70, linear-light model): 0.2877 ± 0.002.
pub const BLURRINESS_TO_SIGMA: f64 = 0.288;

/// Gaussian Blur blurs in linear light in non-linear projects: AE 26's impulse
/// responses only sum to 1 after raising them to 2.4 (pure power, like
/// Exposure), and its step responses are skewed towards the dark side.
pub const LINEAR_LIGHT_GAMMA: f64 = 2.4;

/// Gaussian Blur (Legacy): (Blurriness, σ) measured from AE 26 impulse
/// responses (8 bpc, horizontal); the kernel is a plain Gaussian with no
/// linearization (max error ≤ 1.3 of 255 for Blurriness ≥ 2). σ is not
/// proportional to Blurriness; beyond the table it grows at the last slope.
const LEGACY_SIGMA: &[(f64, f64)] = &[
    (0.0, 0.0),
    (0.5, 0.623),
    (1.0, 0.803),
    (1.5, 1.019),
    (2.0, 1.198),
    (2.5, 1.419),
    (3.0, 1.699),
    (4.0, 2.155),
    (5.0, 2.477),
    (6.0, 2.891),
    (7.0, 3.319),
    (8.0, 3.572),
    (10.0, 4.467),
    (12.0, 5.205),
    (15.0, 6.385),
    (20.0, 8.366),
    (25.0, 10.082),
    (30.0, 12.116),
    (40.0, 16.225),
    (50.0, 19.402),
    (70.0, 25.945),
];

/// σ of Gaussian Blur (Legacy) for `blurriness` (full-resolution pixels).
pub fn legacy_sigma(blurriness: f64) -> f64 {
    let b = blurriness.max(0.0);
    let pairs = LEGACY_SIGMA.windows(2);
    for w in pairs {
        let ((b0, s0), (b1, s1)) = (w[0], w[1]);
        if b <= b1 {
            return s0 + (s1 - s0) * (b - b0) / (b1 - b0);
        }
    }
    let (bl, sl) = LEGACY_SIGMA[LEGACY_SIGMA.len() - 1];
    sl * b / bl
}

/// Per-axis factor of Blur Dimensions (1 both, 2 horizontal, 3 vertical).
pub(crate) fn dimensions(popup: i64) -> [f64; 2] {
    match popup {
        2 => [1.0, 0.0],
        3 => [0.0, 1.0],
        _ => [1.0, 1.0],
    }
}

/// Gaussian Blur (`ADBE Gaussian Blur 2`) and its legacy version.
#[derive(Debug)]
pub struct GaussianBlur {
    def: EffectDef,
    pub(crate) names_zh: Vec<(u32, String)>,
    legacy: bool,
}

impl GaussianBlur {
    /// The current effect (`ADBE Gaussian Blur 2`, AE 26 defaults).
    pub fn new() -> Self {
        Self::build("ADBE Gaussian Blur 2", "Blur & Sharpen", 25.0, true)
    }

    /// Gaussian Blur (Legacy) (`ADBE Gaussian Blur`), same kernel.
    pub fn legacy() -> Self {
        let mut fx = Self::build("ADBE Gaussian Blur", "Obsolete", 0.0, false);
        fx.def.name = "Gaussian Blur (Legacy)".into();
        fx.legacy = true;
        fx
    }

    fn build(match_name: &str, category: &str, blur: f64, repeat: bool) -> Self {
        let (def, names_zh) = DefBuilder::new(match_name, "Gaussian Blur", category)
            .slider(
                1,
                "Blurriness",
                "模糊度",
                blur,
                [0.0, 30000.0],
                [0.0, 50.0],
                1,
                "pixels",
            )
            .popup(2, "Blur Dimensions", "模糊方向", 1, &DIMENSIONS)
            .checkbox(3, "Repeat Edge Pixels", "重复边缘像素", repeat)
            .build();
        Self {
            def,
            names_zh,
            legacy: false,
        }
    }

    fn setup(&self, params: &EffectParams<'_>) -> ([f64; 2], Edge) {
        let p = Params::new(&self.def, params);
        let blurriness = p.f(1).max(0.0);
        let s = if self.legacy {
            legacy_sigma(blurriness)
        } else {
            BLURRINESS_TO_SIGMA * blurriness
        } * params.scale;
        let [dx, dy] = dimensions(p.popup(2));
        let edge = if p.b(3) {
            Edge::Repeat
        } else {
            Edge::Transparent
        };
        ([s * dx, s * dy], edge)
    }
}

impl Default for GaussianBlur {
    fn default() -> Self {
        Self::new()
    }
}

/// Output bounds of a blur reaching `reach` render pixels per axis.
fn grown(input: IRect, reach: [i32; 2], edge: Edge) -> IRect {
    match edge {
        Edge::Repeat => input,
        Edge::Transparent => IRect::new(
            input.x0 - reach[0],
            input.y0 - reach[1],
            input.x1 + reach[0],
            input.y1 + reach[1],
        ),
    }
}

impl Effect for GaussianBlur {
    fn def(&self) -> &EffectDef {
        &self.def
    }

    fn output_bounds(&self, params: &EffectParams<'_>, input: IRect) -> IRect {
        let (sigma, edge) = self.setup(params);
        grown(
            input,
            [blur::radius(sigma[0]), blur::radius(sigma[1])],
            edge,
        )
    }

    fn input_roi(&self, params: &EffectParams<'_>, output_roi: IRect) -> IRect {
        let (sigma, edge) = self.setup(params);
        if sigma == [0.0, 0.0] || output_roi.is_empty() {
            return output_roi;
        }
        blur::plan_gaussian(IRect::EMPTY, sigma, edge, output_roi).read
    }

    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        params: &EffectParams<'_>,
        input: &GpuImage,
        output_roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(ctx, params, input, output_roi)
    }
}

impl TargetEffect for GaussianBlur {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        params: &EffectParams<'_>,
        input: &GpuImage,
        output_roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let (sigma, edge) = self.setup(params);
        let grid = super::image_grid::grid_scale(params, input);
        let texel = input.pixel_scale;
        let sigma = sigma.map(|s| s * grid / texel);
        let taps = sigma.map(blur::radius);
        if taps == [0, 0] || output_roi.is_empty() {
            return Ok(input.clone());
        }
        // The convolution indexes source texels. Keep the sampling phase of a
        // cropped input when its layer origin falls between grid coordinates.
        let origin = [input.rect.x0, input.rect.y0];
        let [x, y] = origin.map(|v| (f64::from(v) / texel).floor() as i32);
        let [w, h] = input.texel_size();
        let mut source = input.clone();
        source.rect = IRect::new(x, y, x + w as i32, y + h as i32);
        source.pixel_scale = 1.;
        let tx = |v: i32| f64::from(x) + (f64::from(v) * grid - f64::from(origin[0])) / texel;
        let ty = |v: i32| f64::from(y) + (f64::from(v) * grid - f64::from(origin[1])) / texel;
        let roi = IRect::new(
            tx(output_roi.x0).floor() as i32,
            ty(output_roi.y0).floor() as i32,
            tx(output_roi.x1).ceil() as i32,
            ty(output_roi.y1).ceil() as i32,
        );
        let out = roi.intersect(&grown(source.rect, taps, edge));
        if out.is_empty() {
            return Ok(input.clone());
        }
        let source = crate::common::premultiplied(target, &source)?;
        let mut plan = blur::plan_gaussian(source.rect, sigma, edge, out);
        if !self.legacy && !params.linear {
            plan = plan.in_linear_light(LINEAR_LIGHT_GAMMA);
        }
        let mut image = plan.encode(target, &source)?;
        let px = |v: i32| origin[0] + (f64::from(v - x) * texel).round() as i32;
        let py = |v: i32| origin[1] + (f64::from(v - y) * texel).round() as i32;
        image.rect = IRect::new(px(out.x0), py(out.y0), px(out.x1), py(out.y1));
        if edge == Edge::Repeat {
            image.rect = image.rect.intersect(&input.rect);
        }
        image.pixel_scale = texel;
        Ok(image)
    }
}

/// Fast Box Blur (`ADBE Box Blur2`) and the legacy Box Blur matchName.
#[derive(Debug)]
pub struct BoxBlur {
    def: EffectDef,
    pub(crate) names_zh: Vec<(u32, String)>,
}

impl BoxBlur {
    /// Fast Box Blur (`ADBE Box Blur2`; AE 26 defaults: 3 iterations, Repeat
    /// Edge Pixels on).
    pub fn new() -> Self {
        Self::build("ADBE Box Blur2", "Fast Box Blur", "Blur & Sharpen")
    }

    /// Box Blur (`ADBE Box Blur`, obsolete matchName).
    pub fn legacy() -> Self {
        Self::build("ADBE Box Blur", "Box Blur", "Obsolete")
    }

    fn build(match_name: &str, name: &str, category: &str) -> Self {
        let (def, names_zh) = DefBuilder::new(match_name, name, category)
            .slider(
                1,
                "Blur Radius",
                "模糊半径",
                0.0,
                [0.0, 30000.0],
                [0.0, 50.0],
                1,
                "pixels",
            )
            .int(2, "Iterations", "迭代", 3, [1, 50])
            .popup(3, "Blur Dimensions", "模糊方向", 1, &DIMENSIONS)
            .checkbox(4, "Repeat Edge Pixels", "重复边缘像素", true)
            .build();
        Self { def, names_zh }
    }

    fn setup(&self, params: &EffectParams<'_>) -> ([f64; 2], u32, Edge) {
        let p = Params::new(&self.def, params);
        let r = p.f(1).max(0.0) * params.scale;
        let [dx, dy] = dimensions(p.popup(3));
        let iterations = p.f(2).round().clamp(1.0, 50.0) as u32;
        let edge = if p.b(4) {
            Edge::Repeat
        } else {
            Edge::Transparent
        };
        ([r * dx, r * dy], iterations, edge)
    }

    fn reach(r: [f64; 2], iterations: u32) -> [i32; 2] {
        let n = iterations as i32;
        r.map(|x| n * x.ceil() as i32)
    }
}

impl Default for BoxBlur {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for BoxBlur {
    fn def(&self) -> &EffectDef {
        &self.def
    }

    fn output_bounds(&self, params: &EffectParams<'_>, input: IRect) -> IRect {
        let (r, n, edge) = self.setup(params);
        grown(input, Self::reach(r, n), edge)
    }

    fn input_roi(&self, params: &EffectParams<'_>, output_roi: IRect) -> IRect {
        let (r, n, edge) = self.setup(params);
        if r == [0.0, 0.0] || output_roi.is_empty() {
            return output_roi;
        }
        let [x, y] = Self::reach(r, n);
        let _ = edge;
        IRect::new(
            output_roi.x0 - x,
            output_roi.y0 - y,
            output_roi.x1 + x,
            output_roi.y1 + y,
        )
    }

    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        params: &EffectParams<'_>,
        input: &GpuImage,
        output_roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(ctx, params, input, output_roi)
    }
}

impl TargetEffect for BoxBlur {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        params: &EffectParams<'_>,
        input: &GpuImage,
        output_roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let (r, n, edge) = self.setup(params);
        let out = output_roi.intersect(&grown(input.rect, Self::reach(r, n), edge));
        if r == [0.0, 0.0] || out.is_empty() {
            return Ok(input.clone());
        }
        let input = crate::common::premultiplied(target, input)?;
        blur::plan_box(input.rect, r, n, edge, out).encode(target, &input)
    }
}
