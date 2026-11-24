//! Time effects ask the graph for the layer before the effect at other times.
use std::sync::Arc;

use bae_gpu::wgpu;
use bae_model::{EffectDef, Time};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, effect::LayerDependency,
};

use crate::{
    Builtin, EffectTarget, TargetEffect,
    catalog::DefBuilder,
    gpu::{self, Resource, UniformWriter},
    params::Params,
    rust_builtin,
};

use super::{image_grid, layer_input};

const LABEL: &str = "bae.effects.temporal";
const SHADER: &str = include_str!("time.wgsl");
const BYTES: u32 = 160;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Echo,
    Posterize,
    Displacement,
}

struct Temporal {
    def: EffectDef,
    kind: Kind,
}

pub(crate) fn builtins() -> [Builtin; 3] {
    let echo = DefBuilder::new("ADBE Echo", "Echo", "Time")
        .slider(
            1,
            "Echo Time (seconds)",
            "残影时间（秒）",
            -0.033333,
            [-30000., 30000.],
            [-2., 2.],
            3,
            "seconds",
        )
        .int(2, "Number Of Echoes", "残影数量", 1, [0, 30000])
        .slider(
            3,
            "Starting Intensity",
            "起始强度",
            1.,
            [0., 1.],
            [0., 1.],
            2,
            "none",
        )
        .slider(4, "Decay", "衰减", 1., [0., 100.], [0., 1.], 2, "none")
        .popup(
            5,
            "Echo Operator",
            "残影运算符",
            1,
            &[
                "Add",
                "Maximum",
                "Minimum",
                "Screen",
                "Composite In Back",
                "Composite In Front",
                "Blend",
            ],
        );
    let posterize = DefBuilder::new("ADBE Posterize Time", "Posterize Time", "Time").slider(
        1,
        "Frame Rate",
        "帧速率",
        24.,
        [0.009995, 999.],
        [1., 99.],
        2,
        "none",
    );
    let displacement = DefBuilder::new("ADBE Time Displacement", "Time Displacement", "Time")
        .layer(1, "Time Displacement Layer", "时间置换图层")
        .layer_stage(1)
        .slider(
            2,
            "Max Displacement Time [sec]",
            "最大移位时间 [秒]",
            1.,
            [-3600., 3600.],
            [-10., 10.],
            2,
            "seconds",
        )
        .slider(
            3,
            "Time Resolution [fps]",
            "时间分辨率 [fps]",
            60.,
            [0.009995, 3600.],
            [1., 120.],
            2,
            "none",
        )
        .checkbox(4, "If Layer Sizes Differ", "如果图层大小不同", true);
    [
        (Kind::Echo, echo, "echo", "残影"),
        (Kind::Posterize, posterize, "posterize-time", "抽帧"),
        (
            Kind::Displacement,
            displacement,
            "time-displacement",
            "时间置换",
        ),
    ]
    .map(|(kind, b, id, zh)| {
        let (def, names) = b.build();
        rust_builtin((Arc::new(Temporal { def, kind }), names), id, zh, &[id, zh])
    })
}

/// Other frames of the same layer up to the effect's position in the stack.
pub(super) fn before_effect(p: &EffectParams<'_>, time: Time) -> Option<LayerDependency> {
    Some(LayerDependency::before_effect(
        layer_input::own_layer(p)?,
        time,
        p.effect.path.clone(),
    ))
}

fn local_offset(p: &EffectParams<'_>, seconds: f64) -> Time {
    p.comp_time + Time::from_seconds(seconds * p.layer_time_scale)
}

fn held_time(p: &EffectParams<'_>, fps: f64) -> Time {
    let seconds = p.time.as_seconds();
    let held = (seconds * fps + 1e-9).floor() / fps;
    local_offset(p, held - seconds)
}

fn echo_time(p: &EffectParams<'_>, v: Params<'_>, index: i32) -> Time {
    local_offset(p, f64::from(index) * v.f(1))
}

fn displaced_steps(p: &EffectParams<'_>, v: Params<'_>) -> Vec<(Time, [u32; 2])> {
    let interval = 1. / v.f(3).max(0.009995);
    let max = v.f(2);
    let mut steps: Vec<(Time, [u32; 2])> = Vec::new();
    // All pixels in one time bin can share the same upstream frame.
    let seconds = p.time.as_seconds();
    for level in 0..256 {
        let shifted = seconds + (f64::from(level) - 127.) * max / 128.;
        let bucket = shifted - shifted.rem_euclid(interval);
        // The sampled time has 16 fractional bits in seconds. Rounding can
        // put it just before an input frame boundary.
        let sample = (bucket * 65536.).round() / 65536.;
        let time = local_offset(p, sample - seconds);
        match steps.last_mut() {
            Some((last, range)) if *last == time => range[1] = level,
            _ => steps.push((time, [level, level])),
        }
    }
    steps
}

impl Effect for Temporal {
    fn def(&self) -> &EffectDef {
        &self.def
    }

    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        let v = Params::new(&self.def, p);
        match self.kind {
            Kind::Echo if v.f(1) != 0. => {
                let mut times: Vec<_> = (1..=v.f(2) as i32)
                    .map(|index| echo_time(p, v, index))
                    .filter(|&time| time != p.comp_time)
                    .collect();
                times.dedup();
                times
                    .into_iter()
                    .filter_map(|time| before_effect(p, time))
                    .collect()
            }
            Kind::Echo => Vec::new(),
            Kind::Posterize => {
                let time = held_time(p, v.f(1).max(0.009995));
                (time != p.comp_time)
                    .then(|| before_effect(p, time))
                    .flatten()
                    .into_iter()
                    .collect()
            }
            Kind::Displacement => {
                let Some(map) = layer_input::dependency(p, 1, true) else {
                    return Vec::new();
                };
                std::iter::once(map)
                    .chain(displaced_steps(p, v).into_iter().filter_map(|(time, _)| {
                        if time == p.comp_time {
                            None
                        } else {
                            before_effect(p, time)
                        }
                    }))
                    .collect()
            }
        }
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

impl TargetEffect for Temporal {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        if roi.is_empty() {
            return Ok(input.clone());
        }
        let v = Params::new(&self.def, p);
        let grid = image_grid::grid_scale(p, input);
        let layer = if p.layer_bounds.is_empty() {
            image_grid::pixel_rect(input.rect, 1. / grid)
        } else {
            image_grid::layer_rect(p)
        };
        let pass = Pass { roi, layer, grid };
        match self.kind {
            Kind::Echo => echo(target, p, v, input, &pass),
            Kind::Posterize => {
                let time = held_time(p, v.f(1).max(0.009995));
                Ok(before_effect(p, time)
                    .and_then(|d| target.effect_layer(&d))
                    .unwrap_or(input)
                    .clone())
            }
            Kind::Displacement => {
                let map = layer_input::dependency(p, 1, true)
                    .and_then(|d| target.effect_layer(&d))
                    .cloned();
                let Some(map) = map else {
                    return Ok(input.clone());
                };
                let steps = displaced_steps(p, v);
                if let [(time, _)] = steps.as_slice() {
                    return Ok(before_effect(p, *time)
                        .and_then(|dep| target.effect_layer(&dep))
                        .unwrap_or(input)
                        .clone());
                }
                let rect = image_grid::pixel_rect(roi, grid);
                let levels = target.allocate(rect, grid)?;
                pass.write(
                    target,
                    [input, &map, input],
                    [3, 0, 0, u32::from(v.b(4))],
                    [0.; 4],
                    &levels,
                )?;
                let out = target.allocate(rect, grid)?;
                pass.write(target, [input; 3], [2, 0, 0, 0], [0.; 4], &out)?;
                for (time, range) in steps {
                    if time == p.comp_time {
                        continue;
                    }
                    let sample = before_effect(p, time)
                        .and_then(|d| target.effect_layer(&d))
                        .cloned();
                    if let Some(sample) = sample {
                        pass.write(
                            target,
                            [&sample, &levels, input],
                            [1, range[0], range[1], 0],
                            [0.; 4],
                            &out,
                        )?;
                    }
                }
                Ok(out)
            }
        }
    }
}

fn echo(
    target: &mut dyn EffectTarget,
    p: &EffectParams<'_>,
    v: Params<'_>,
    input: &GpuImage,
    pass: &Pass,
) -> Result<GpuImage, RenderError> {
    let count = v.f(2) as i32;
    let operation = v.popup(5) as u32;
    let mut out = input.clone();
    let base = v.f(3);
    for index in 0..=count {
        let time = echo_time(p, v, index);
        let sampled = if time == p.comp_time {
            input
        } else {
            let Some(frame) = before_effect(p, time).and_then(|d| target.effect_layer(&d)) else {
                continue;
            };
            frame
        }
        .clone();
        let gain = base * v.f(4).powi(index);
        let levels = if p.float {
            0.
        } else if p.sixteen_bpc {
            32768.
        } else {
            255.
        };
        out = pass.dispatch(
            target,
            [&sampled, input, &out],
            [0, operation, u32::from(index == 0), u32::from(!p.float)],
            [gain as f32, levels, 0., 0.],
        )?;
    }
    Ok(out)
}

struct Pass {
    roi: IRect,
    layer: IRect,
    grid: f64,
}

impl Pass {
    fn dispatch(
        &self,
        target: &mut dyn EffectTarget,
        images: [&GpuImage; 3],
        flags: [u32; 4],
        data: [f32; 4],
    ) -> Result<GpuImage, RenderError> {
        let out = target.allocate(image_grid::pixel_rect(self.roi, self.grid), self.grid)?;
        self.write(target, images, flags, data, &out)?;
        Ok(out)
    }

    fn write(
        &self,
        target: &mut dyn EffectTarget,
        images: [&GpuImage; 3],
        flags: [u32; 4],
        data: [f32; 4],
        out: &GpuImage,
    ) -> Result<(), RenderError> {
        let rt = gpu::runtime(target.gpu());
        let kernel = image_grid::kernel_with_inputs(&rt, target.format(), LABEL, SHADER, BYTES, 3)?;
        let mut u = UniformWriter::with_size(BYTES as usize);
        for (i, image) in images.into_iter().enumerate() {
            image_grid::image_coords(&mut u, i * 32, image, self.grid);
        }
        u.i32s(96, &image_grid::rect4(self.roi))
            .i32s(112, &image_grid::rect4(self.layer))
            .f32s(144, &data);
        for (i, flag) in flags.into_iter().enumerate() {
            u.u32(128 + 4 * i, flag);
        }
        let group = rt.bind_group(
            &kernel,
            &[
                Resource::Uniform,
                Resource::View(&images[0].view),
                Resource::View(&images[1].view),
                Resource::View(&images[2].view),
                Resource::View(&out.view),
            ],
        );
        let offset = gpu::push_uniform(&rt, &u);
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(LABEL),
                timestamp_writes: None,
            });
        gpu::dispatch(&mut pass, &kernel, &group, &[offset], self.roi.size());
        Ok(())
    }
}

pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    image_grid::kernel_with_inputs(rt, format, LABEL, SHADER, BYTES, 3)?;
    Ok(())
}
