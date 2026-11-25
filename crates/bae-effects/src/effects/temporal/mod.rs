//! Time effects ask the graph for the layer before the effect at other times.
use std::sync::Arc;

use bae_gpu::wgpu;
use bae_model::{EffectDef, ParamKind, Time};
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
    Difference,
    WideTime,
    ForceBlur,
    Timewarp,
}

struct Temporal {
    def: EffectDef,
    kind: Kind,
}

pub(crate) fn builtins() -> [Builtin; 7] {
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
    let difference = DefBuilder::new("ADBE Difference", "Time Difference", "Time")
        .layer(1, "Target", "目标")
        .slider(
            2,
            "Time Offset (sec)",
            "时间偏移量（秒）",
            0.,
            [-2000., 2000.],
            [-2., 2.],
            2,
            "seconds",
        )
        .slider(
            3,
            "Contrast",
            "对比度",
            50.,
            [-10000., 10000.],
            [0., 100.],
            1,
            "none",
        )
        .checkbox(4, "Absolute Difference", "绝对差值", false)
        .popup(
            5,
            "Alpha Channel",
            "Alpha 通道",
            1,
            &[
                "Original",
                "Target",
                "Blend",
                "Max",
                "Full On",
                "(-",
                "Lightness of Result",
                "Max of Result",
                "(-",
                "Alpha Difference",
                "Alpha Difference Only",
            ],
        );
    let wide_time = DefBuilder::new("CC Wide Time", "CC Wide Time", "Time")
        .int(1, "Forward Steps", "Forward Steps", 1, [0, 126])
        .int(2, "Backward Steps", "Backward Steps", 1, [0, 126])
        .popup(
            3,
            "Native Motion Blur",
            "Native Motion Blur",
            2,
            &["On", "Off"],
        );
    let force_blur = DefBuilder::new("CC Force Motion Blur", "CC Force Motion Blur", "Time")
        .int(1, "Motion Blur Samples", "Motion Blur Samples", 8, [1, 255])
        .checkbox(
            2,
            "Override Shutter Settings",
            "Override Shutter Settings",
            true,
        )
        .slider(
            3,
            "Shutter Angle",
            "Shutter Angle",
            180.,
            [0., 3600.],
            [0., 360.],
            1,
            "degrees",
        )
        .slider(
            5,
            "Shutter Phase",
            "Shutter Phase",
            0.,
            [-360., 360.],
            [-360., 360.],
            1,
            "degrees",
        )
        .popup(
            4,
            "Native Motion Blur",
            "Native Motion Blur",
            2,
            &["On", "Off"],
        );
    [
        (Kind::Echo, echo, "echo", "残影", "残影"),
        (Kind::Posterize, posterize, "posterize-time", "抽帧", "抽帧"),
        (
            Kind::Displacement,
            displacement,
            "time-displacement",
            "时间置换",
            "时间置换",
        ),
        (
            Kind::Difference,
            difference,
            "time-difference",
            "时差",
            "时差",
        ),
        (
            Kind::WideTime,
            wide_time,
            "cc-wide-time",
            "CC Wide Time",
            "时间混合",
        ),
        (
            Kind::ForceBlur,
            force_blur,
            "cc-force-motion-blur",
            "CC Force Motion Blur",
            "强制运动模糊",
        ),
        (
            Kind::Timewarp,
            timewarp(),
            "timewarp",
            "时间扭曲",
            "时间扭曲",
        ),
    ]
    .map(|(kind, b, id, zh, keyword)| {
        let (def, names) = b.build();
        rust_builtin(
            (Arc::new(Temporal { def, kind }), names),
            id,
            zh,
            &[id, zh, keyword],
        )
    })
}

/// AE Timewarp's parameters. Only Method, Adjust Time By, Speed, Source Frame
/// and Motion Blur act; the flow tuning, matte, warp layer and crops wait for
/// the optical-flow pass (Pixel Motion renders as Frame Mix until then).
fn timewarp() -> DefBuilder {
    let mut b = DefBuilder::new("ADBE Timewarp", "Timewarp", "Time")
        .popup(
            1,
            "Method",
            "方法",
            3,
            &["Whole Frames", "Frame Mix", "Pixel Motion"],
        )
        .popup(
            2,
            "Adjust Time By",
            "调整时间方式",
            1,
            &["Speed", "Source Frame"],
        )
        .slider(
            3,
            "Speed",
            "速度",
            50.,
            [-5000., 5000.],
            [0., 200.],
            1,
            "none",
        )
        .slider(
            4,
            "Source Frame",
            "源帧",
            0.,
            [-5000., 5000.],
            [0., 300.],
            2,
            "none",
        );
    let group = |b: &mut DefBuilder, id, name: &str, zh: &str, start: bool| {
        let kind = if start {
            ParamKind::GroupStart
        } else {
            ParamKind::GroupEnd
        };
        b.push(id, name, zh, kind, None);
    };
    group(&mut b, 5, "Tuning", "调节", true);
    b = b.slider(
        6,
        "Vector Detail",
        "矢量详细信息",
        20.,
        [5., 100.],
        [5., 100.],
        1,
        "percent",
    );
    group(&mut b, 7, "Smoothing", "平滑", true);
    b = b
        .slider(
            8,
            "Global Smoothness",
            "全局平滑度",
            500.,
            [-5000., 5000.],
            [0., 1000.],
            1,
            "none",
        )
        .slider(
            9,
            "Local Smoothness",
            "局部平滑度",
            500.,
            [-5000., 5000.],
            [0., 1000.],
            1,
            "none",
        )
        .int(10, "Smoothing Iterations", "平滑迭代", 5, [1, 300]);
    group(&mut b, 11, "Smoothing", "平滑", false);
    b = b
        .checkbox(12, "Build From One Image", "从一个图像开始构建", false)
        .checkbox(13, "Correct Luminance Changes", "适当明亮度更改", false)
        .popup(14, "Filtering", "过滤", 1, &["Normal", "Extreme"])
        .slider(
            15,
            "Error Threshold",
            "错误阈值",
            1.,
            [0., 100.],
            [0., 100.],
            2,
            "none",
        )
        .int(16, "Block Size", "块大小", 6, [4, 1000]);
    group(&mut b, 17, "Weighting", "权重", true);
    for (id, name, zh, default) in [
        (18, "Red Weight", "红色权重", 29.899994),
        (19, "Green Weight", "绿色权重", 58.699997),
        (20, "Blue Weight", "蓝色权重", 11.440002),
    ] {
        b = b.slider(id, name, zh, default, [0., 100.], [0., 100.], 2, "none");
    }
    group(&mut b, 277, "Weighting", "权重", false);
    group(&mut b, 278, "Tuning", "调节", false);
    group(&mut b, 23, "Motion Blur", "运动模糊", true);
    b = b
        .checkbox(24, "Enable Motion Blur", "启用运动模糊", false)
        .popup(
            25,
            "Shutter Control",
            "快门控制",
            2,
            &["Manual", "Automatic"],
        )
        .slider(
            26,
            "Shutter Angle",
            "快门角度",
            180.,
            [0., 5000.],
            [0., 360.],
            1,
            "degrees",
        )
        .int(27, "Shutter Samples", "快门采样", 5, [1, 1000]);
    group(&mut b, 28, "Motion Blur", "运动模糊", false);
    b = b
        .layer(29, "Matte Layer", "遮罩图层")
        .popup(
            30,
            "Matte Channel",
            "遮罩通道",
            3,
            &["Luminance", "Inverted Luminance", "Alpha", "Inverted Alpha"],
        )
        .layer(31, "Warp Layer", "变形图层")
        .popup(
            32,
            "Show",
            "显示",
            1,
            &["Normal", "Matte", "Foreground", "Background"],
        );
    group(&mut b, 33, "Source Crops", "源裁剪", true);
    for (id, name, zh, default) in [
        (34, "Left", "左侧", 0.),
        (35, "Right", "右侧", 100.),
        (36, "Bottom", "底部", 0.),
        (37, "Top", "顶部", 100.),
    ] {
        b = b.slider(id, name, zh, default, [0., 100.], [0., 100.], 2, "none");
    }
    group(&mut b, 38, "Source Crops", "源裁剪", false);
    b
}

/// Other frames of the same layer up to the effect's position in the stack.
pub(crate) fn before_effect(p: &EffectParams<'_>, time: Time) -> Option<LayerDependency> {
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

/// The frames (comp times) and weights that Wide Time, Force Motion Blur and
/// Timewarp average; times equal to the current one use the effect's input.
fn mixed_frames(kind: Kind, p: &EffectParams<'_>, v: Params<'_>) -> Vec<(Time, f32)> {
    let step = p.time_step.as_seconds();
    let mut out: Vec<(Time, f32)> = Vec::new();
    let mut add = |time: Time, w: f64| match out.iter_mut().find(|(t, _)| *t == time) {
        Some((_, acc)) => *acc += w as f32,
        None => out.push((time, w as f32)),
    };
    match kind {
        Kind::WideTime => {
            let (forward, backward) = (v.f(1).max(0.) as i64, v.f(2).max(0.) as i64);
            let w = 1. / (forward + backward + 1) as f64;
            for k in -backward..=forward {
                add(p.comp_time + Time::from_seconds(k as f64 * step), w);
            }
        }
        Kind::ForceBlur => {
            let n = v.f(1).max(1.) as i64;
            let (angle, phase) = if v.b(2) {
                (v.f(3), v.f(5))
            } else {
                (p.shutter_angle, p.shutter_phase)
            };
            for i in 0..n {
                let at = phase / 360. + angle / 360. * (i as f64 + 0.5) / n as f64;
                add(p.comp_time + Time::from_seconds(at * step), 1. / n as f64);
            }
        }
        Kind::Timewarp => {
            let fps = p.frame_rate.as_f64();
            let speed = v.f(3) / 100.;
            // The source time (layer seconds) shown now.
            let source = if v.popup(2) == 2 {
                v.f(4) / fps
            } else {
                p.time.as_seconds() * speed
            };
            // Motion blur spreads samples over the source the shutter sees.
            let (n, span) = if v.b(24) {
                let angle = if v.popup(25) == 2 { 180. } else { v.f(26) };
                (
                    v.f(27).max(1.) as i64,
                    speed.abs() * angle / 360. / fps * f64::from(v.popup(2) == 1),
                )
            } else {
                (1, 0.)
            };
            for i in 0..n {
                let s = source + span * ((i as f64 + 0.5) / n as f64 - 0.5);
                let frame = s * fps + 1e-9;
                let f0 = frame.floor();
                let w = if v.popup(1) == 1 { 0. } else { frame - f0 };
                for (f, weight) in [(f0, 1. - w), (f0 + 1., w)] {
                    if weight > 1e-4 {
                        let at = local_offset(p, f / fps - p.time.as_seconds());
                        add(at, weight / n as f64);
                    }
                }
            }
        }
        _ => {}
    }
    out
}

/// The layer before this effect at `time` (the input itself now).
fn frame_at(
    target: &dyn EffectTarget,
    p: &EffectParams<'_>,
    input: &GpuImage,
    time: Time,
) -> Option<GpuImage> {
    if time == p.comp_time {
        return Some(input.clone());
    }
    before_effect(p, time)
        .and_then(|d| target.effect_layer(&d))
        .cloned()
}

fn difference_target(p: &EffectParams<'_>, v: Params<'_>) -> Option<LayerDependency> {
    let mut dep = layer_input::dependency(p, 1, true)?;
    dep.time = local_offset(p, v.f(2));
    Some(dep)
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
            Kind::Difference => difference_target(p, v).into_iter().collect(),
            Kind::WideTime | Kind::ForceBlur | Kind::Timewarp => mixed_frames(self.kind, p, v)
                .into_iter()
                .filter(|&(time, _)| time != p.comp_time)
                .filter_map(|(time, _)| before_effect(p, time))
                .collect(),
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
            Kind::Difference => {
                let Some(other) = difference_target(p, v)
                    .and_then(|d| target.effect_layer(&d))
                    .cloned()
                else {
                    return Ok(input.clone());
                };
                pass.dispatch(
                    target,
                    [input, &other, input],
                    [4, v.popup(5) as u32, u32::from(v.b(4)), u32::from(!p.float)],
                    [(v.f(3) / 50.) as f32, 0., 0., 0.],
                )
            }
            Kind::WideTime | Kind::ForceBlur | Kind::Timewarp => {
                let frames = mixed_frames(self.kind, p, v);
                let mut out: Option<GpuImage> = None;
                for (time, weight) in frames {
                    let Some(sample) = frame_at(&*target, p, input, time) else {
                        continue;
                    };
                    // Echo's Blend operator accumulates `sample · weight`.
                    let first = out.is_none();
                    let previous = out.unwrap_or_else(|| input.clone());
                    out = Some(pass.dispatch(
                        target,
                        [&sample, input, &previous],
                        [0, 7, u32::from(first), u32::from(!p.float)],
                        [weight, 0., 0., 0.],
                    )?);
                }
                Ok(out.unwrap_or_else(|| input.clone()))
            }
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
