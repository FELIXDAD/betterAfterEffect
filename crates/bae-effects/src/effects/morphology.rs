//! Separable minimum/maximum filters for Minimax and Simple Choker.

use std::sync::Arc;

use bae_gpu::wgpu;
use bae_model::EffectDef;
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};

use crate::catalog::DefBuilder;
use crate::gpu::{self, Resource, UniformWriter};
use crate::params::Params;
use crate::target::{EffectTarget, TargetEffect};
use crate::{Builtin, rust_builtin};

use super::image_grid;

const LABEL: &str = "bae.effects.morphology";
const SOURCE: &str = include_str!("../shaders/morphology.wgsl");
const UNIFORM_SIZE: u32 = 160;

struct Morphology {
    def: EffectDef,
    choker: bool,
}

pub(crate) fn minimax() -> Builtin {
    let (def, zh) = DefBuilder::new("ADBE Minimax", "Minimax", "Channel")
        .popup(
            1,
            "Operation",
            "操作",
            2,
            &[
                "Minimum",
                "Maximum",
                "Minimum Then Maximum",
                "Maximum Then Minimum",
            ],
        )
        .slider(
            2,
            "Radius",
            "半径",
            0.0,
            [0.0, 32_000.0],
            [0.0, 100.0],
            1,
            "pixels",
        )
        .popup(
            3,
            "Channel",
            "通道",
            1,
            &["Color", "Alpha and Color", "Red", "Green", "Blue", "Alpha"],
        )
        .popup(
            4,
            "Direction",
            "方向",
            1,
            &["Horizontal & Vertical", "Just Horizontal", "Just Vertical"],
        )
        .checkbox(5, "Don't Shrink Edges", "不要收缩边缘", false)
        .build();
    rust_builtin(
        (Arc::new(Morphology { def, choker: false }), zh),
        "minimax",
        "最小/最大",
        &["minimum", "maximum", "erode", "dilate", "最小", "最大"],
    )
}

pub(crate) fn simple_choker() -> Builtin {
    let (def, zh) = DefBuilder::new("ADBE Simple Choker", "Simple Choker", "Matte")
        .popup(1, "View", "视图", 1, &["Final Output", "Matte"])
        .slider(
            2,
            "Choke Matte",
            "阻塞遮罩",
            0.0,
            [-100.0, 100.0],
            [-10.0, 10.0],
            2,
            "pixels",
        )
        .build();
    rust_builtin(
        (Arc::new(Morphology { def, choker: true }), zh),
        "simple-choker",
        "简单阻塞工具",
        &["choke", "spread", "matte", "遮罩", "阻塞"],
    )
}

pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    image_grid::kernel(rt, format, LABEL, SOURCE, UNIFORM_SIZE)?;
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct Step {
    x: i32,
    y: i32,
    minimum: bool,
}

impl Step {
    fn read(self, roi: IRect) -> IRect {
        IRect::new(
            roi.x0.saturating_sub(self.x),
            roi.y0.saturating_sub(self.y),
            roi.x1.saturating_add(self.x),
            roi.y1.saturating_add(self.y),
        )
    }
}

// Taking extrema at -step, 0 and +step extends a radius-r contiguous
// window to radius r + step whenever step <= 2r + 1. Each pass can triple
// the covered width, with no loop over the user-controlled radius on GPU.
fn axis(steps: &mut Vec<Step>, want: i32, horizontal: bool, minimum: bool) {
    let mut reach = 0;
    while reach < want {
        let step = (reach * 2 + 1).min(want - reach);
        reach += step;
        steps.push(Step {
            x: if horizontal { step } else { 0 },
            y: if horizontal { 0 } else { step },
            minimum,
        });
    }
}

struct Setup {
    steps: Vec<Step>,
    mask: u32,
    repeat: bool,
    view_matte: bool,
    sigma: f32,
    gain: f32,
    bias: f32,
}

impl Morphology {
    fn radius(&self, params: &EffectParams<'_>) -> i32 {
        let value = Params::new(&self.def, params).f(2);
        if self.choker {
            (3.0 * Self::choker_sigma(value, params.scale)).ceil() as i32
        } else {
            (value.clamp(0.0, 32_000.0) * params.scale)
                .round()
                .clamp(0.0, 32_000.0) as i32
        }
    }

    fn choker_sigma(value: f64, scale: f64) -> f64 {
        0.836 * value.abs().min(100.0).sqrt() * scale
    }

    fn setup(&self, params: &EffectParams<'_>) -> Setup {
        let p = Params::new(&self.def, params);
        let r = self.radius(params);
        let mut steps = Vec::new();
        if self.choker {
            // AE 26's soft-disc results fit a Gaussian on alpha followed by a
            // clipped linear remap. Gain and offset are fitted at 1 and 4.5.
            let choke = p.f(2).clamp(-100.0, 100.0);
            let amount = choke.abs();
            let excess = (amount - 1.0).max(0.0);
            let (gain, bias) = if choke >= 0.0 {
                let gain = 1.0 + amount + 0.125 * excess;
                (gain, 1.0 - gain + 0.075 * excess)
            } else {
                (1.0 + amount + 0.11 * excess, -0.063 * excess)
            };
            if r > 0 {
                steps.push(Step {
                    x: r,
                    y: 0,
                    minimum: false,
                });
                steps.push(Step {
                    x: 0,
                    y: r,
                    minimum: false,
                });
            }
            return Setup {
                steps,
                mask: 8,
                repeat: false,
                view_matte: p.popup(1) == 2,
                sigma: Self::choker_sigma(choke, params.scale) as f32,
                gain: gain as f32,
                bias: bias as f32,
            };
        }
        let operations: &[bool] = match p.popup(1) {
            1 => &[true],
            3 => &[true, false],
            4 => &[false, true],
            _ => &[false],
        };
        let direction = p.popup(4);
        for &minimum in operations {
            if direction != 3 {
                axis(&mut steps, r, true, minimum);
            }
            if direction != 2 {
                axis(&mut steps, r, false, minimum);
            }
        }
        let mask = match p.popup(3) {
            2 => 15,
            3 => 1,
            4 => 2,
            5 => 4,
            6 => 8,
            _ => 7,
        };
        Setup {
            steps,
            mask,
            repeat: p.b(5),
            view_matte: false,
            sigma: 0.0,
            gain: 1.0,
            bias: 0.0,
        }
    }
}

impl Effect for Morphology {
    fn def(&self) -> &EffectDef {
        &self.def
    }

    fn output_bounds(&self, params: &EffectParams<'_>, input: IRect) -> IRect {
        let p = Params::new(&self.def, params);
        let grows = if self.choker {
            p.f(2) < 0.0 || p.popup(1) == 2
        } else {
            matches!(p.popup(3), 2 | 6) && matches!(p.popup(1), 2..=4)
        };
        let domain = image_grid::layer_rect(params);
        if grows && !domain.is_empty() {
            if self.choker && p.popup(1) == 2 {
                return input.union(&domain);
            }
            return input
                .inflate(self.radius(params))
                .intersect(&domain)
                .union(&input);
        }
        input
    }

    fn input_roi(&self, params: &EffectParams<'_>, roi: IRect) -> IRect {
        self.setup(params)
            .steps
            .iter()
            .rev()
            .fold(roi, |r, &s| s.read(r))
    }

    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        params: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(ctx, params, input, roi)
    }
}

impl TargetEffect for Morphology {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        params: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let Setup {
            mut steps,
            mask,
            repeat,
            view_matte,
            sigma,
            gain,
            bias,
        } = self.setup(params);
        if roi.is_empty() || (steps.is_empty() && !view_matte) {
            return Ok(input.clone());
        }
        if steps.is_empty() {
            steps.push(Step {
                x: 0,
                y: 0,
                minimum: false,
            });
        }
        let grid = image_grid::grid_scale(params, input);
        let domain =
            image_grid::layer_rect(params).union(&image_grid::pixel_rect(input.rect, 1.0 / grid));
        let mut rects = vec![roi; steps.len() + 1];
        for (i, &step) in steps.iter().enumerate().rev() {
            rects[i] = step.read(rects[i + 1]).intersect(&domain);
        }
        let work_roi = rects[1].union(&roi);
        let work_rect = image_grid::pixel_rect(work_roi, grid);
        let work: Vec<_> = (0..(steps.len() - 1).min(2))
            .map(|_| target.allocate(work_rect, grid))
            .collect::<Result<_, _>>()?;
        let output = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        let rt = gpu::runtime(target.gpu());
        let kernel = image_grid::kernel(&rt, target.format(), LABEL, SOURCE, UNIFORM_SIZE)?;
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(LABEL),
                timestamp_writes: None,
            });
        for (i, step) in steps.iter().enumerate() {
            let first = i == 0;
            let last = i == steps.len() - 1;
            let src = if first { input } else { &work[(i - 1) % 2] };
            let dst = if last { &output } else { &work[i % 2] };
            let mut w = UniformWriter::with_size(UNIFORM_SIZE as usize);
            image_grid::image_coords(&mut w, 0, src, grid);
            image_grid::image_coords(&mut w, 32, input, grid);
            let origin = if last { roi } else { work_roi };
            w.i32s(64, &image_grid::rect4(rects[i]))
                .i32s(80, &image_grid::rect4(rects[i + 1]))
                .i32s(96, &image_grid::rect4(domain))
                .i32s(112, &[step.x, step.y, origin.x0, origin.y0])
                .f32(128, sigma)
                .u32(132, mask)
                .u32(
                    136,
                    u32::from(step.minimum)
                        | (u32::from(first) << 1)
                        | (u32::from(last) << 2)
                        | (u32::from(repeat) << 3)
                        | (u32::from(self.choker) << 4)
                        | (u32::from(view_matte) << 5)
                        | (u32::from(params.float) << 6),
                )
                .f32(144, gain)
                .f32(148, bias);
            let offset = gpu::push_uniform(&rt, &w);
            let group = rt.bind_group(
                &kernel,
                &[
                    Resource::Uniform,
                    Resource::View(&src.view),
                    Resource::View(&input.view),
                    Resource::View(&dst.view),
                ],
            );
            gpu::dispatch(&mut pass, &kernel, &group, &[offset], rects[i + 1].size());
        }
        drop(pass);
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_cover_every_integer_in_the_radius() {
        for r in [0, 1, 2, 3, 7, 20, 100, 32_000] {
            let mut plan = Vec::new();
            axis(&mut plan, r, true, false);
            let mut reach = 0;
            for step in plan {
                assert!((1..=2 * reach + 1).contains(&step.x));
                reach += step.x;
            }
            assert_eq!(reach, r);
        }
    }
}
