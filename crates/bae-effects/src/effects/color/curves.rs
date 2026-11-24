use crate::catalog::DefBuilder;
use crate::gpu::UniformWriter;
use crate::target::{EffectTarget, TargetEffect};
use bae_model::{EffectDef, ParamKind, Value};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};

pub(super) const UNIFORM_BYTES: usize = 16 + 5 * 2048;

#[derive(Debug)]
pub(crate) struct Curves(pub EffectDef);

impl Curves {
    pub fn new() -> (Self, Vec<(u32, String)>) {
        let mut b = DefBuilder::new("ADBE CurvesCustom", "Curves", "Color Correction").popup(
            2,
            "Channel:",
            "通道:",
            1,
            &["RGB", "Red", "Green", "Blue", "Alpha"],
        );
        b.push(
            1,
            "Curves",
            "曲线",
            ParamKind::CurvePoints,
            Some(Value::FloatList(vec![])),
        );
        let (def, zh) = b.build();
        (Self(def), zh)
    }
}

// Natural cubic through ordered knots. The tridiagonal system has zero
// second derivatives at both endpoints. Outside the knots the curve is flat.
fn samples(points: &mut Vec<[f64; 2]>) -> [f64; 1024] {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]));
    points.dedup_by(|a, b| a[0] == b[0]);
    let n = points.len();
    let mut second = vec![0.; n];
    let mut rhs = vec![0.; n];
    for i in 1..n - 1 {
        let [x, y] = points[i];
        let [x0, y0] = points[i - 1];
        let [x1, y1] = points[i + 1];
        let t = (x - x0) / (x1 - x0);
        let den = t * second[i - 1] + 2.;
        second[i] = (t - 1.) / den;
        rhs[i] =
            (6. * ((y1 - y) / (x1 - x) - (y - y0) / (x - x0)) / (x1 - x0) - t * rhs[i - 1]) / den;
    }
    for i in (0..n - 1).rev() {
        second[i] = second[i] * second[i + 1] + rhs[i];
    }
    let mut i = 0;
    std::array::from_fn(|k| {
        let x = k as f64 / 1023.;
        while i + 1 < n && x >= points[i + 1][0] {
            i += 1;
        }
        let [x0, y0] = points[i];
        if i + 1 == n || x <= x0 {
            return y0;
        }
        let [x1, y1] = points[i + 1];
        let dx = x1 - x0;
        let b = (x - x0) / dx;
        let a = 1. - b;
        a * y0
            + b * y1
            + ((a * a * a - a) * second[i] + (b * b * b - b) * second[i + 1]) * dx * dx / 6.
    })
}

impl Effect for Curves {
    fn def(&self) -> &EffectDef {
        &self.0
    }
    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(ctx, p, input, roi)
    }
}

impl TargetEffect for Curves {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let roi = roi.intersect(&input.rect);
        let Some(Value::FloatList(points)) = p.value(1) else {
            return Ok(input.clone());
        };
        if points.is_empty() || roi.is_empty() {
            return Ok(input.clone());
        }
        let input = crate::common::premultiplied(target, input)?;
        let out = target.allocate(roi, input.pixel_scale)?;
        let mut channels: [Vec<[f64; 2]>; 5] = Default::default();
        for &[ch, x, y] in points.as_chunks::<3>().0 {
            if (0.0..=4.0).contains(&ch) && ch.fract() == 0.0 {
                channels[ch as usize].push([x, y]);
            }
        }
        // Two 16-bit samples per word keep all five 1024-entry curves inside
        // the portable 16 KiB uniform limit. No image pixels leave the GPU.
        let mut u = UniformWriter::with_size(UNIFORM_BYTES);
        u.i32s(0, &[roi.x0 - input.rect.x0, roi.y0 - input.rect.y0])
            .u32(8, u32::from(p.float));
        let mut active = 0;
        for (ch, points) in channels.iter_mut().enumerate() {
            if points.is_empty() {
                continue;
            }
            active |= 1 << ch;
            for (i, pair) in samples(points).as_chunks::<2>().0.iter().enumerate() {
                let pack = |x: f64| (x.clamp(0., 1.) * 65535.).round() as u32;
                u.u32(16 + ch * 2048 + i * 4, pack(pair[0]) | pack(pair[1]) << 16);
            }
        }
        u.u32(12, active);
        super::dispatch(target, "apply_curves", &u, &[&input], &out, roi.size())?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spline_interpolates_knots_and_preserves_endpoints() {
        let y = samples(&mut vec![[1., 1.], [1. / 3., 0.75], [0., 0.]]);
        assert_eq!([y[0], y[341], y[1023]], [0., 0.75, 1.]);
        assert!(y[170] > 0.375);
    }
}
