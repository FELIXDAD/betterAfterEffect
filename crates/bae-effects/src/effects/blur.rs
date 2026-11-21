//! Gaussian Blur (AE-FX-300/301) and Fast Box Blur (AE-FX-302/303).

use bae_model::EffectDef;
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};

use crate::blur::{self, Edge};
use crate::catalog::DefBuilder;
use crate::params::Params;
use crate::target::{EffectTarget, TargetEffect};

const DIMENSIONS: [&str; 3] = ["Horizontal and Vertical", "Horizontal", "Vertical"];

/// AE Blurriness → Gaussian standard deviation (lottie-web and Skottie use
/// 0.3; AE's exact factor is UNVERIFIED until the AE reference renders).
pub const BLURRINESS_TO_SIGMA: f64 = 0.3;

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
}

impl GaussianBlur {
    /// The current effect (`ADBE Gaussian Blur 2`, AE 26 defaults).
    pub fn new() -> Self {
        Self::build("ADBE Gaussian Blur 2", "Blur & Sharpen", 25.0, 3000.0, true)
    }

    /// Gaussian Blur (Legacy) (`ADBE Gaussian Blur`), same kernel.
    pub fn legacy() -> Self {
        let mut fx = Self::build("ADBE Gaussian Blur", "Obsolete", 0.0, 1000.0, false);
        fx.def.name = "Gaussian Blur (Legacy)".into();
        fx
    }

    fn build(match_name: &str, category: &str, blur: f64, max: f64, repeat: bool) -> Self {
        let (def, names_zh) = DefBuilder::new(match_name, "Gaussian Blur", category)
            .slider(
                1,
                "Blurriness",
                "模糊度",
                blur,
                [0.0, max],
                [0.0, 50.0],
                1,
                "pixels",
            )
            .popup(2, "Blur Dimensions", "模糊方向", 1, &DIMENSIONS)
            .checkbox(3, "Repeat Edge Pixels", "重复边缘像素", repeat)
            .build();
        Self { def, names_zh }
    }

    fn setup(&self, params: &EffectParams<'_>) -> ([f64; 2], Edge) {
        let p = Params::new(&self.def, params);
        let s = BLURRINESS_TO_SIGMA * p.f(1).max(0.0) * params.scale;
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
        let bounds = grown(
            input.rect,
            [blur::radius(sigma[0]), blur::radius(sigma[1])],
            edge,
        );
        let out = output_roi.intersect(&bounds);
        if sigma == [0.0, 0.0] || out.is_empty() {
            return Ok(input.clone());
        }
        let input = crate::common::premultiplied(target, input)?;
        blur::plan_gaussian(input.rect, sigma, edge, out).encode(target, &input)
    }
}

/// Fast Box Blur (`ADBE Box Blur2`) and the legacy Box Blur matchName.
#[derive(Debug)]
pub struct BoxBlur {
    def: EffectDef,
    pub(crate) names_zh: Vec<(u32, String)>,
}

impl BoxBlur {
    /// Fast Box Blur (`ADBE Box Blur2`).
    pub fn new() -> Self {
        Self::build("ADBE Box Blur2", "Fast Box Blur", "Blur & Sharpen", 3)
    }

    /// Box Blur (`ADBE Box Blur`, obsolete matchName, default 1 iteration).
    pub fn legacy() -> Self {
        Self::build("ADBE Box Blur", "Box Blur", "Obsolete", 1)
    }

    fn build(match_name: &str, name: &str, category: &str, iterations: i64) -> Self {
        let (def, names_zh) = DefBuilder::new(match_name, name, category)
            .slider(
                1,
                "Blur Radius",
                "模糊半径",
                0.0,
                [0.0, 32767.0],
                [0.0, 50.0],
                1,
                "pixels",
            )
            .int(2, "Iterations", "迭代", iterations, [1, 50])
            .popup(3, "Blur Dimensions", "模糊方向", 1, &DIMENSIONS)
            .checkbox(4, "Repeat Edge Pixels", "重复边缘像素", false)
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
