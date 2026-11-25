//! The blur engine shared by the blur, glow, shadow and feather effects.
//!
//! **Gaussian** ([`plan_gaussian`], [`encode_gaussian`]): separable, with the
//! kernel cut at 3σ like AE (taps with |x| ≤ 3σ). Small σ (< 6 render px) runs two direct passes
//! whose source lines are cached in workgroup memory. Larger σ blurs a
//! block-averaged copy at 1/2ⁿ resolution (σ at the level stays in 3..6
//! texels) and upsamples it bilinearly: the cost no longer depends on the
//! radius (4K, Blurriness 100 ≈ one full-resolution read and write). The level
//! σ is reduced by the variance the box downsample ((f²−1)/12) and the
//! bilinear upsample (≈ f²/6) add, so the effective kernel keeps the requested
//! σ. The level grid is anchored to multiples of 2ⁿ in render pixels, so
//! tiles rendered with different ROIs agree.
//!
//! **Box** ([`plan_box`], [`encode_box`]): AE Fast Box Blur, iterated
//! horizontal/vertical box filters of width 2r+1 (fractional r weights the two
//! outer taps), each a sliding-window pass (O(1) per pixel).
//!
//! Addressing follows AE: transparent outside the input, or clamped to the
//! input rect with Repeat Edge Pixels (the output then stays inside it).

use std::sync::Arc;

use bae_gpu::wgpu;
use bae_render::{GpuImage, IRect, RenderError};

use crate::gpu::{
    self, Binding, Kernel, KernelKey, KernelSource, Resource, Runtime, UniformWriter,
};
use crate::target::EffectTarget;

const SOURCE: &str = include_str!("shaders/blur.wgsl");
const PARAMS_SIZE: u32 = 96 + 15 * 16;
/// Direct (full-resolution) passes up to this σ, pyramid above.
pub const DIRECT_MAX_SIGMA: f64 = 6.0;
/// `conv_line` tile: outputs along the axis × lines across it.
const TILE_ALONG: u32 = 128;
const TILE_ACROSS: u32 = 4;
/// Most taps per side `conv_line` handles (box blurs above use `box_line`).
const MAX_TAPS: i32 = 24;
/// Index of offset 0 in the symmetric weight table (`WPAD` in blur.wgsl).
const WPAD: usize = 28;

/// Kernel radius (taps per side) for σ, as AE: taps with |x| ≤ 3σ (AE 26
/// impulse responses end there; σ < 1/3 leaves the image unchanged).
pub fn radius(sigma: f64) -> i32 {
    if sigma <= 0.0 {
        0
    } else {
        (3.0 * sigma + 1e-9).floor() as i32
    }
}

/// Edge addressing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// Transparent outside the input (output grows by the blur radius).
    Transparent,
    /// Repeat Edge Pixels: clamp to the input rect (output stays inside it).
    Repeat,
}

fn kernel(
    rt: &Runtime,
    format: wgpu::TextureFormat,
    entry: &'static str,
) -> Result<Arc<Kernel>, RenderError> {
    let fmt = format_name(format)?;
    let key = KernelKey {
        owner: Arc::from("bae.effects/blur"),
        entry: Arc::from(entry),
        variant: crate::gpu::format_key(format),
    };
    let workgroup = match entry {
        "conv_line" => [32, TILE_ACROSS, 1],
        "box_line" => [64, 1, 1],
        _ => [8, 8, 1],
    };
    rt.kernel(&key, || {
        Ok(KernelSource {
            label: "bae.effects.blur",
            wgsl: SOURCE.replace("WORKING_FORMAT", fmt),
            entry,
            bindings: vec![
                (0, Binding::Uniform(PARAMS_SIZE)),
                (1, Binding::Texture),
                (2, Binding::Storage(format)),
            ],
            workgroup,
        })
    })
}

/// WGSL storage format name of a working format.
pub(crate) fn format_name(format: wgpu::TextureFormat) -> Result<&'static str, RenderError> {
    match format {
        wgpu::TextureFormat::Rgba16Float => Ok("rgba16float"),
        wgpu::TextureFormat::Rgba32Float => Ok("rgba32float"),
        other => Err(RenderError::Other(format!(
            "bae-effects: unsupported working format {other:?}"
        ))),
    }
}

/// One dispatch of a blur kernel.
#[derive(Clone, Debug)]
struct Step {
    entry: &'static str,
    /// Index of the source image (0 = effect input, k = intermediate k-1).
    src: usize,
    /// Index of the destination intermediate (`None` = the output image).
    dst: Option<usize>,
    src_rect: IRect,
    dst_rect: IRect,
    base: [i32; 2],
    factor: [i32; 2],
    axis: u32,
    clamp: bool,
    radius: i32,
    sigma: f32,
    frac: f32,
    seg: i32,
    /// Power applied to the straight colour of what the step reads / writes
    /// (1 = none; linear-light blurs).
    load_gamma: f32,
    store_gamma: f32,
    /// `conv_line` weights by |offset|, normalised (index 0..=radius).
    weights: Vec<f32>,
}

impl Step {
    fn new(
        entry: &'static str,
        src: usize,
        dst: Option<usize>,
        src_rect: IRect,
        dst_rect: IRect,
    ) -> Self {
        Self {
            entry,
            src,
            dst,
            src_rect,
            dst_rect,
            base: [0, 0],
            factor: [1, 1],
            axis: 0,
            clamp: false,
            radius: 0,
            sigma: 1.0,
            frac: 0.0,
            seg: 1,
            load_gamma: 1.0,
            store_gamma: 1.0,
            weights: Vec::new(),
        }
    }

    /// A Gaussian `conv_line` step.
    fn gauss(
        src: usize,
        dst: Option<usize>,
        src_rect: IRect,
        dst_rect: IRect,
        axis: u32,
        sigma: f64,
    ) -> Self {
        let r = radius(sigma);
        let mut s = Self::new("conv_line", src, dst, src_rect, dst_rect);
        s.axis = axis;
        s.radius = r;
        s.sigma = sigma as f32;
        s.weights = gaussian_weights(sigma, r);
        s
    }
}

/// Normalised Gaussian weights for offsets 0..=r (the kernel spans -r..=r).
pub fn gaussian_weights(sigma: f64, r: i32) -> Vec<f32> {
    let w: Vec<f64> = (0..=r)
        .map(|k| (-f64::from(k * k) / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum = w[0] + 2.0 * w[1..].iter().sum::<f64>();
    w.into_iter().map(|x| (x / sum) as f32).collect()
}

/// Normalised box weights for radius `r` (fractional outer taps).
fn box_weights(r: f64) -> (i32, Vec<f32>) {
    let ri = r.floor() as i32;
    let frac = r - f64::from(ri);
    let taps = ri + i32::from(frac > 0.0);
    let norm = 1.0 / (f64::from(2 * ri + 1) + 2.0 * frac);
    let w = (0..=taps)
        .map(|k| (if k <= ri { norm } else { frac * norm }) as f32)
        .collect();
    (taps, w)
}

/// A planned blur: intermediates to allocate (rect, pixel-scale factor) and
/// the dispatches. `read` is the region of the input it reads.
#[derive(Clone, Debug)]
pub struct Plan {
    /// Intermediate images: rect and pixel-scale multiplier.
    images: Vec<(IRect, f64)>,
    steps: Vec<Step>,
    /// Output rect.
    pub output: IRect,
    /// Input region read (before clamping to the input rect).
    pub read: IRect,
}

fn inflate_xy(r: IRect, x: i32, y: i32) -> IRect {
    IRect::new(r.x0 - x, r.y0 - y, r.x1 + x, r.y1 + y)
}

fn floor_div(a: i32, b: i32) -> i32 {
    a.div_euclid(b)
}

/// Pyramid level (power-of-two factor) for σ on one axis.
pub fn level_factor(sigma: f64) -> i32 {
    if sigma < DIRECT_MAX_SIGMA {
        1
    } else {
        // σ at the level stays in [3, 6).
        let n = (sigma / 3.0).log2().floor().max(1.0) as u32;
        1 << n.min(12)
    }
}

/// σ at the pyramid level (in level texels) that gives σ overall.
pub fn level_sigma(sigma: f64, factor: i32) -> f64 {
    if factor <= 1 {
        return sigma;
    }
    let f = f64::from(factor);
    let var = sigma * sigma - (f * f - 1.0) / 12.0 - f * f / 6.0;
    var.max(0.25 * f * f).sqrt() / f
}

/// Plans a Gaussian blur of an input covering `input` (render px) with
/// standard deviations `sigma` (render px, 0 = no blur on that axis) for the
/// output region `output`.
pub fn plan_gaussian(input: IRect, sigma: [f64; 2], edge: Edge, output: IRect) -> Plan {
    let clamp = edge == Edge::Repeat;
    let factor = sigma.map(level_factor);
    if factor == [1, 1] {
        return plan_direct(input, sigma, clamp, output);
    }
    let lsigma = [
        level_sigma(sigma[0], factor[0]),
        level_sigma(sigma[1], factor[1]),
    ];
    let r = [radius(lsigma[0]), radius(lsigma[1])];
    let [fx, fy] = factor;
    // Grid anchored to multiples of the factor (ROI-independent results).
    let base = [floor_div(output.x0, fx) * fx, floor_div(output.y0, fy) * fy];
    // Level texels the upsample reads (bilinear footprint of every output pixel).
    let lo =
        |o: i32, b: i32, f: i32| ((f64::from(o - b) + 0.5) / f64::from(f) - 0.5).floor() as i32;
    let level_out = IRect::new(
        lo(output.x0, base[0], fx),
        lo(output.y0, base[1], fy),
        lo(output.x1 - 1, base[0], fx) + 2,
        lo(output.y1 - 1, base[1], fy) + 2,
    );
    let level_tmp = inflate_xy(level_out, 0, r[1]);
    let level_down = inflate_xy(level_out, r[0], r[1]);
    let read = IRect::new(
        base[0] + level_down.x0 * fx,
        base[1] + level_down.y0 * fy,
        base[0] + level_down.x1 * fx,
        base[1] + level_down.y1 * fy,
    );
    let fscale = f64::from(fx.max(fy));
    let mut images = Vec::new();
    let mut steps = Vec::new();

    // Downsample in stages of at most 8× per axis (≤ 64 loads per texel).
    let mut stages = Vec::new();
    let (mut ax, mut ay) = (fx, fy);
    while ax > 1 || ay > 1 {
        let sx = ax.min(8);
        let sy = ay.min(8);
        stages.push([sx, sy]);
        ax /= sx;
        ay /= sy;
    }
    // Region of each stage's level, from the last (= level_down) backwards.
    let mut regions = vec![level_down; stages.len()];
    for k in (0..stages.len().saturating_sub(1)).rev() {
        let [sx, sy] = stages[k + 1];
        let n = regions[k + 1];
        regions[k] = IRect::new(n.x0 * sx, n.y0 * sy, n.x1 * sx, n.y1 * sy);
    }
    let mut cum = [1, 1];
    for (k, &[sx, sy]) in stages.iter().enumerate() {
        cum = [cum[0] * sx, cum[1] * sy];
        images.push((regions[k], f64::from(cum[0].max(cum[1]))));
        let dst = images.len() - 1;
        let mut s = if k == 0 {
            let mut s = Step::new("downsample", 0, Some(dst), input, regions[0]);
            s.base = base;
            s.clamp = clamp;
            s
        } else {
            Step::new("downsample", dst, Some(dst), regions[k - 1], regions[k])
        };
        s.factor = [sx, sy];
        steps.push(s);
    }
    let down = images.len(); // 1-based source index of the last level
    // Level blur: horizontal into tmp, vertical into the blurred level.
    let mut cur_src = down;
    let mut cur_rect = level_down;
    if r[0] > 0 {
        images.push((level_tmp, fscale));
        let dst = Some(images.len() - 1);
        steps.push(Step::gauss(cur_src, dst, cur_rect, level_tmp, 0, lsigma[0]));
        cur_src = images.len();
        cur_rect = level_tmp;
    }
    if r[1] > 0 {
        images.push((level_out, fscale));
        let dst = Some(images.len() - 1);
        steps.push(Step::gauss(cur_src, dst, cur_rect, level_out, 1, lsigma[1]));
        cur_src = images.len();
        cur_rect = level_out;
    }
    let mut s = Step::new("upsample", cur_src, None, cur_rect, output);
    s.base = base;
    s.factor = factor;
    steps.push(s);
    Plan {
        images,
        steps,
        output,
        read,
    }
}

/// Box-average aligned source texels into the next Gaussian pyramid level.
pub(crate) fn reduce_level(
    target: &mut dyn EffectTarget,
    input: &GpuImage,
    factor: [i32; 2],
    repeat: bool,
) -> Result<GpuImage, RenderError> {
    let r = input.rect;
    let [x, y] = factor;
    let output = IRect::new(
        r.x0.div_euclid(x),
        r.y0.div_euclid(y),
        (r.x1 + x - 1).div_euclid(x),
        (r.y1 + y - 1).div_euclid(y),
    );
    // Retain a fully extended edge texel beyond each partial box, so subsequent
    // pyramid levels and convolutions repeat the true source edge.
    let output = if repeat { output.inflate(1) } else { output };
    let mut step = Step::new("downsample", 0, None, r, output);
    step.factor = factor;
    step.clamp = repeat;
    Plan {
        images: Vec::new(),
        steps: vec![step],
        output,
        read: r,
    }
    .encode_steps(target, input, true)
}

fn plan_direct(input: IRect, sigma: [f64; 2], clamp: bool, output: IRect) -> Plan {
    let r = [radius(sigma[0]), radius(sigma[1])];
    let mut images = Vec::new();
    let mut steps = Vec::new();
    let read = inflate_xy(output, r[0], r[1]);
    match (r[0] > 0, r[1] > 0) {
        (true, true) => {
            let mut tmp = inflate_xy(output, 0, r[1]);
            if clamp {
                tmp = IRect::new(tmp.x0, tmp.y0.max(input.y0), tmp.x1, tmp.y1.min(input.y1));
            }
            images.push((tmp, 1.0));
            let mut h = Step::gauss(0, Some(0), input, tmp, 0, sigma[0]);
            h.clamp = clamp;
            let mut v = Step::gauss(1, None, tmp, output, 1, sigma[1]);
            v.clamp = clamp;
            steps.push(h);
            steps.push(v);
        }
        (true, false) | (false, true) => {
            let axis = u32::from(r[1] > 0);
            let mut s = Step::gauss(0, None, input, output, axis, sigma[axis as usize]);
            s.clamp = clamp;
            steps.push(s);
        }
        (false, false) => {}
    }
    Plan {
        images,
        steps,
        output,
        read,
    }
}

/// Plans `iterations` rounds of horizontal then vertical box filters of
/// radius `r` (render px; per-axis, 0 = off).
pub fn plan_box(input: IRect, r: [f64; 2], iterations: u32, edge: Edge, output: IRect) -> Plan {
    let clamp = edge == Edge::Repeat;
    let mut axes = Vec::new();
    for _ in 0..iterations.max(1) {
        for (a, &ra) in r.iter().enumerate() {
            if ra > 0.0 {
                axes.push(a as u32);
            }
        }
    }
    let margin = |ra: f64| {
        let ri = ra.floor();
        (ri as i32) + i32::from(ra > ri)
    };
    // Regions backwards from the output.
    let mut regions = vec![output; axes.len()];
    for k in (0..axes.len().saturating_sub(1)).rev() {
        let a = axes[k + 1] as usize;
        let m = margin(r[a]);
        let n = regions[k + 1];
        let mut g = if a == 0 {
            inflate_xy(n, m, 0)
        } else {
            inflate_xy(n, 0, m)
        };
        if clamp {
            g = g.intersect(&input);
        }
        regions[k] = g;
    }
    let read = match axes.first() {
        Some(&a) => {
            let m = margin(r[a as usize]);
            if a == 0 {
                inflate_xy(regions[0], m, 0)
            } else {
                inflate_xy(regions[0], 0, m)
            }
        }
        None => output,
    };
    let mut images = Vec::new();
    let mut steps = Vec::new();
    let last = axes.len().saturating_sub(1);
    let mut src = 0;
    let mut src_rect = input;
    for (k, &a) in axes.iter().enumerate() {
        let ra = r[a as usize];
        let ri = ra.floor() as i32;
        let dst = if k == last {
            None
        } else {
            images.push((regions[k], 1.0));
            Some(images.len() - 1)
        };
        let (taps, weights) = box_weights(ra);
        let mut s = if taps <= MAX_TAPS {
            let mut s = Step::new("conv_line", src, dst, src_rect, regions[k]);
            s.radius = taps;
            s.weights = weights;
            s
        } else {
            let mut s = Step::new("box_line", src, dst, src_rect, regions[k]);
            s.radius = ri;
            s.frac = (ra - f64::from(ri)) as f32;
            s.seg = (2 * ri + 1).clamp(32, 512);
            s
        };
        s.axis = a;
        s.clamp = clamp;
        steps.push(s);
        src = images.len();
        src_rect = regions[k];
    }
    Plan {
        images,
        steps,
        output,
        read,
    }
}

impl Plan {
    /// Uses a tent in place of the Gaussian weights on the already-planned
    /// direct passes. The Gaussian footprint must contain the tent.
    pub(crate) fn with_tent(mut self, half_width: f64) -> Self {
        for step in &mut self.steps {
            if step.entry == "conv_line" {
                for (offset, weight) in step.weights.iter_mut().enumerate() {
                    *weight = (1. - offset as f64 / half_width).max(0.) as f32;
                }
                let sum = step.weights[0] + 2. * step.weights[1..].iter().sum::<f32>();
                for weight in &mut step.weights {
                    *weight /= sum;
                }
            }
        }
        self
    }

    /// Filters in linear light: what the plan reads from the input is raised
    /// to `gamma` (straight colour), its output to `1 / gamma`. No extra passes.
    #[must_use]
    pub fn in_linear_light(mut self, gamma: f64) -> Self {
        for s in &mut self.steps {
            if s.src == 0 {
                s.load_gamma = gamma as f32;
            }
            if s.dst.is_none() {
                s.store_gamma = (1.0 / gamma) as f32;
            }
        }
        self
    }

    /// Records the plan: reads `input`, allocates the intermediates and the
    /// output (a working image covering `self.output`).
    pub fn encode(
        &self,
        target: &mut dyn EffectTarget,
        input: &GpuImage,
    ) -> Result<GpuImage, RenderError> {
        self.encode_steps(target, input, false)
    }

    /// Encodes in the input texel grid and leaves the final bilinear upsample
    /// to the caller. The returned rect and pixel scale describe that level in
    /// the input grid, so a following pass can sample it directly.
    pub(crate) fn encode_level(
        mut self,
        target: &mut dyn EffectTarget,
        input: &GpuImage,
    ) -> Result<GpuImage, RenderError> {
        let last = self.steps.last().unwrap();
        if last.entry != "upsample" || last.factor[0] != last.factor[1] {
            return self.encode_steps(target, input, true);
        }
        let step = self.steps.pop().unwrap();
        self.images.pop();
        self.output = step.src_rect;
        self.steps.last_mut().unwrap().dst = None;
        let mut image = self.encode_steps(target, input, true)?;
        let [x, y] = step.base;
        let factor = step.factor[0];
        image.rect = IRect::new(
            x + image.rect.x0 * factor,
            y + image.rect.y0 * factor,
            x + image.rect.x1 * factor,
            y + image.rect.y1 * factor,
        );
        image.pixel_scale *= f64::from(factor);
        Ok(image)
    }

    fn encode_steps(
        &self,
        target: &mut dyn EffectTarget,
        input: &GpuImage,
        texel_grid: bool,
    ) -> Result<GpuImage, RenderError> {
        let rt = gpu::runtime(target.gpu());
        let format = target.format();
        let scale = input.pixel_scale;
        let mut images = Vec::with_capacity(self.images.len());
        for (rect, k) in &self.images {
            images.push(target.allocate(*rect, if texel_grid { 1. } else { scale * k })?);
        }
        let output = target.allocate(self.output, scale)?;
        let mut prepared = Vec::with_capacity(self.steps.len());
        for s in &self.steps {
            let k = kernel(&rt, format, s.entry)?;
            let src = if s.src == 0 {
                input
            } else {
                &images[s.src - 1]
            };
            let dst = match s.dst {
                Some(d) => &images[d],
                None => &output,
            };
            let mut w = UniformWriter::with_size(PARAMS_SIZE as usize);
            w.i32s(0, &rect4(s.src_rect))
                .i32s(16, &rect4(s.dst_rect))
                .i32s(32, &[src.rect.x0, src.rect.y0])
                .i32s(40, &[dst.rect.x0, dst.rect.y0])
                .i32s(48, &s.base)
                .i32s(56, &s.factor)
                .u32(64, s.axis)
                .u32(68, u32::from(s.clamp))
                .i32(72, s.radius)
                .f32(76, s.sigma.max(1e-6))
                .f32(80, s.frac)
                .i32(84, s.seg)
                .f32(88, s.load_gamma)
                .f32(92, s.store_gamma);
            // Symmetric, zero-padded layout (see `wsym` in blur.wgsl).
            for (d, x) in s.weights.iter().enumerate().take(WPAD + 1) {
                w.f32(96 + 4 * (WPAD + d), *x);
                w.f32(96 + 4 * (WPAD - d), *x);
            }
            let offset = gpu::push_uniform(&rt, &w);
            let group = rt.bind_group(
                &k,
                &[
                    Resource::Uniform,
                    Resource::View(&src.view),
                    Resource::View(&dst.view),
                ],
            );
            let [w, h] = s.dst_rect.size();
            let groups = match s.entry {
                "conv_line" => {
                    let (along, across) = if s.axis == 0 { (w, h) } else { (h, w) };
                    [along.div_ceil(TILE_ALONG), across.div_ceil(TILE_ACROSS), 1]
                }
                "box_line" => {
                    let (along, across) = if s.axis == 0 { (w, h) } else { (h, w) };
                    [across.div_ceil(64), along.div_ceil(s.seg.max(1) as u32), 1]
                }
                _ => k.groups([w, h]),
            };
            prepared.push((k, group, offset, groups));
        }
        let encoder = target.encoder();
        let mut profiler = crate::profile::PROFILER.lock();
        if let Some(timer) = profiler.as_mut() {
            // Profiling: one pass per step, each with timestamps.
            for ((k, group, offset, groups), s) in prepared.iter().zip(&self.steps) {
                let [w, h] = s.dst_rect.size();
                let label = format!("{} r{} {}x{}", s.entry, s.radius, w, h);
                let scope = timer.scope(label);
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("bae.effects.blur"),
                    timestamp_writes: scope.map(|sc| timer.compute_pass_writes(sc)),
                });
                pass.set_pipeline(&k.pipeline);
                pass.set_bind_group(0, group, &[*offset]);
                pass.dispatch_workgroups(groups[0], groups[1], groups[2]);
            }
            return Ok(output);
        }
        drop(profiler);
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("bae.effects.blur"),
            timestamp_writes: None,
        });
        for (k, group, offset, groups) in &prepared {
            if groups.contains(&0) {
                continue;
            }
            pass.set_pipeline(&k.pipeline);
            pass.set_bind_group(0, group, &[*offset]);
            pass.dispatch_workgroups(groups[0], groups[1], groups[2]);
        }
        drop(pass);
        Ok(output)
    }
}

fn rect4(r: IRect) -> [i32; 4] {
    [r.x0, r.y0, r.x1, r.y1]
}

/// Prewarms the blur kernels for `format`.
pub(crate) fn prewarm(rt: &Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    for entry in ["conv_line", "downsample", "upsample", "box_line"] {
        kernel(rt, format, entry)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels() {
        assert_eq!(level_factor(5.9), 1);
        assert_eq!(level_factor(6.0), 2);
        assert_eq!(level_factor(11.9), 2);
        assert_eq!(level_factor(12.0), 4);
        assert_eq!(level_factor(30.0), 8);
        for s in [6.0, 8.0, 12.0, 30.0, 100.0, 900.0] {
            let f = level_factor(s);
            let ls = level_sigma(s, f);
            assert!((2.5..6.5).contains(&ls), "sigma {s}: level sigma {ls}");
        }
    }

    #[test]
    fn plan_regions_cover_reads() {
        let input = IRect::new(0, 0, 400, 300);
        let out = IRect::new(-90, -90, 490, 390);
        let p = plan_gaussian(input, [30.0, 30.0], Edge::Transparent, out);
        assert!(p.read.contains(&IRect::new(-180, -180, 580, 480)));
        // Every step's destination lies inside its image.
        for s in &p.steps {
            let rect = match s.dst {
                Some(d) => p.images[d].0,
                None => p.output,
            };
            assert!(rect.contains(&s.dst_rect), "{s:?}");
        }
    }
}
