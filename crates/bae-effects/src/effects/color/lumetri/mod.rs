mod definition;

use crate::blur::{self, Edge};
use crate::effects::{image_grid, lut::LutCache};
use crate::gpu::{self, Resource, UniformWriter};
use crate::{Builtin, EffectTarget, TargetEffect, params::Params, rust_builtin};
use bae_model::{EffectDef, Value};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};
use std::sync::Arc;

const HEADER: usize = 256;
const TONE: &[u8] = include_bytes!("tone.bin");
const KNOTS: usize = 64;

struct Lumetri {
    def: EffectDef,
    lut: LutCache,
}

pub(crate) fn builtin() -> Builtin {
    let (def, names) = definition::definition();
    rust_builtin(
        (
            Arc::new(Lumetri {
                def,
                lut: LutCache::default(),
            }),
            names,
        ),
        "lumetri-color",
        "Lumetri 颜色",
        &["grading", "调色"],
    )
}

impl Effect for Lumetri {
    fn def(&self) -> &EffectDef {
        &self.def
    }

    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        let v = Params::new(&self.def, p);
        secondary_roi(v, roi).inflate(sharpen_radius(v.f(29)) * i32::from(v.b(23)))
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

fn sharpen_radius(amount: f64) -> i32 {
    if amount == 0. {
        0
    } else if amount > 0. {
        1
    } else {
        4
    }
}

fn secondary_roi(v: Params<'_>, roi: IRect) -> IRect {
    if !v.b(68) {
        return roi;
    }
    let sigma = v.f(82) * v.p.scale;
    let mask = if sigma > 0. {
        blur::plan_gaussian(IRect::EMPTY, [sigma; 2], Edge::Repeat, roi).read
    } else {
        roi
    };
    mask.inflate(i32::from(v.f(81) > 0.))
        .union(&roi.inflate(sharpen_radius(v.f(89))))
}

fn soften(u: &mut UniformWriter, amount: f64) {
    if amount < 0. {
        let pos = (-amount).clamp(0., 100.);
        let row = (pos as usize).min(99);
        let mut n = 0;
        for i in 0..5 {
            let sample = |r: usize| {
                let at = 2 * (5 * r + i);
                let raw = include_bytes!("soften.bin");
                f64::from(u16::from_le_bytes([raw[at], raw[at + 1]])) / 65535.
            };
            let w = sample(row) + (pos - row as f64) * (sample(row + 1) - sample(row));
            if w > 0. {
                n = i;
            }
            u.f32(if i == 4 { 124 } else { 240 + i * 4 }, w as f32);
        }
        u.u32(220, n as u32);
    }
}

fn white_balance(temp: f64, tint: f64) -> [f32; 3] {
    let gain = [
        (temp * 0.01 + tint * 0.015).exp2(),
        1.,
        (-temp * 0.01 + tint * 0.015).exp2(),
    ];
    let y = gain[0] * 0.2126 + gain[1] * 0.7152 + gain[2] * 0.0722;
    gain.map(|v| (v / y).powf(1. / 2.4) as f32)
}

// Interpolate the measured grayscale response between neighboring slider settings.
fn tone_row(u: &mut UniformWriter, offset: usize, control: usize, value: f64) {
    let row = ((value + 150.) / 15.).clamp(0., 20.);
    let base = (row.floor() as usize).min(19);
    for k in 0..KNOTS {
        let sample = |r| {
            let at = 2 * ((control * 21 + r) * KNOTS + k);
            f64::from(u16::from_le_bytes([TONE[at], TONE[at + 1]])) / 65535.
        };
        let a = sample(base);
        u.f32(
            offset + k * 4,
            (a + (sample(base + 1) - a) * (row - base as f64)) as f32,
        );
    }
}

fn color(v: Params<'_>, id: u32) -> [f32; 3] {
    match v.value(id) {
        Some(Value::Color([r, g, b, _])) => [*r as f32, *g as f32, *b as f32],
        _ => [0.; 3],
    }
}

const SPLIT_OFFSET: usize = HEADER + 5 * 256 * 4 + 512 + 4224 + 512;
const SPLIT_KNOTS: usize = 66;
const SECONDARY_OFFSET: usize = SPLIT_OFFSET + SPLIT_KNOTS * 16;
const CORRECTION_OFFSET: usize = SECONDARY_OFFSET + KNOTS * 4;
const CHROMA_OFFSET: usize = CORRECTION_OFFSET + 16;
const CHROMA_GRID: usize = 16 * 16 * 16;

fn secondary_response(u: &mut UniformWriter, value: f64) {
    tone_row(u, SECONDARY_OFFSET, 0, value);
    let position = ((value + 150.) / 15.).clamp(0., 20.);
    let row = (position as usize).min(19);
    let mix = position - row as f64;
    let data = include_bytes!("secondary.bin");
    let a = &data[row * CHROMA_GRID..][..CHROMA_GRID];
    let b = &data[(row + 1) * CHROMA_GRID..][..CHROMA_GRID];
    let out = &mut u.bytes[CHROMA_OFFSET..];
    for ((dst, a), b) in out.iter_mut().zip(a).zip(b) {
        let a = f64::from(*a as i8);
        *dst = (a + (f64::from(*b as i8) - a) * mix).round() as i8 as u8;
    }
}

// The two tint wheels produce independent per-channel transfers. Interpolate
// those transfers on the six hue sectors and the measured strength rows.
fn split_tone(v: Params<'_>, u: &mut UniformWriter) {
    let mut offsets = [[0.0_f32; 3]; SPLIT_KNOTS];
    for (band, id) in [1001, 1002].into_iter().enumerate() {
        let [r, g, b] = color(v, id);
        let hi = r.max(g).max(b);
        let chroma = hi - r.min(g).min(b);
        if chroma <= 0. {
            continue;
        }
        let hue = if hi == r {
            ((g - b) / chroma).rem_euclid(6.)
        } else if hi == g {
            2. + (b - r) / chroma
        } else {
            4. + (r - g) / chroma
        };
        let pos = (chroma * 4.).clamp(0., 4.);
        let level = (pos as usize).min(3);
        for (k, knot) in offsets.iter_mut().enumerate() {
            let input = ((k * 4).min(254) + usize::from(k == 65)) as f32;
            for (channel, offset) in knot.iter_mut().enumerate() {
                let sample = |h: usize, a: usize| {
                    if a == 0 {
                        return input;
                    }
                    let i = (((band * 6 + h) * 4 + a - 1) * SPLIT_KNOTS + k) * 3 + channel;
                    f32::from(include_bytes!("split.bin")[i])
                };
                let sample_hue = |h| {
                    let a = sample(h, level);
                    a + (pos - level as f32) * (sample(h, level + 1) - a)
                };
                let h = hue as usize % 6;
                let a = sample_hue(h);
                let out = a + hue.fract() * (sample_hue((h + 1) % 6) - a);
                *offset += (out - input) / 255.;
            }
        }
    }
    for (i, knot) in offsets.iter().enumerate() {
        u.f32s(SPLIT_OFFSET + i * 16, knot);
    }
}

fn curves(v: Params<'_>, u: &mut UniformWriter) -> u32 {
    let mut mask = 0;
    for (id, channels) in [(39, 4), (42, 1)] {
        let Some(Value::FloatList(values)) = v.value(id) else {
            continue;
        };
        for ch in 0..channels {
            let mut points = values
                .as_chunks::<3>()
                .0
                .iter()
                .filter(|p| p[0] == ch as f64)
                .map(|p| [p[1], p[2]])
                .collect::<Vec<_>>();
            if points.is_empty() {
                continue;
            }
            let table = if id == 42 { 4 } else { ch };
            mask |= 1 << table;
            if table == 4 {
                hue_samples(u, &mut points);
                continue;
            }
            let samples = super::curves::samples(&mut points);
            for k in 0..256 {
                u.f32(
                    HEADER + (table * 256 + k) * 4,
                    samples[(k * 1023 + 127) / 255] as f32,
                );
            }
        }
    }
    mask
}

// The hue axis wraps at red. A local extremum has a horizontal tangent;
// adjacent monotone intervals share their harmonic-mean secant slope.
fn hue_samples(u: &mut UniformWriter, points: &mut Vec<[f64; 2]>) {
    points.iter_mut().for_each(|p| p[0] = p[0].rem_euclid(1.));
    points.sort_by(|a, b| a[0].total_cmp(&b[0]));
    points.dedup_by(|a, b| a[0] == b[0]);
    let n = points.len();
    let secant = |i: usize| {
        let [x, y] = points[i];
        let [xx, yy] = points[(i + 1) % n];
        (yy - y) / (xx - x + f64::from(i + 1 == n))
    };
    let slope = |i: usize| {
        let a = secant((i + n - 1) % n);
        let b = secant(i);
        if a * b > 0. { 2. * a * b / (a + b) } else { 0. }
    };
    for k in 0..256 {
        let x = k as f64 / 255.;
        let i = (points.partition_point(|p| p[0] <= x) + n - 1) % n;
        let j = (i + 1) % n;
        let dx = points[j][0] - points[i][0] + f64::from(j == 0);
        let t = (x - points[i][0] + f64::from(x < points[i][0])) / dx;
        let h = t * t * (3. - 2. * t);
        let y = (1. - h) * points[i][1]
            + h * points[j][1]
            + dx * t * (1. - t) * ((1. - t) * slope(i) - t * slope(j));
        u.f32(HEADER + 4 * (4 * 256 + k), y as f32);
    }
}

fn render_pass(
    target: &mut dyn EffectTarget,
    source: &GpuImage,
    roi: IRect,
    grid: f64,
    basic: bool,
    mut u: UniformWriter,
    mask: Option<&GpuImage>,
) -> Result<GpuImage, RenderError> {
    image_grid::image_coords(&mut u, 0, source, grid);
    u.i32s(32, &[roi.x0, roi.y0]);
    let rt = gpu::runtime(target.gpu());
    let (label, shader) = if basic {
        (
            "Lumetri Basic",
            concat!(include_str!("shared.wgsl"), include_str!("basic.wgsl")),
        )
    } else {
        (
            "Lumetri Grading",
            concat!(include_str!("shared.wgsl"), include_str!("grading.wgsl")),
        )
    };
    let kernel = image_grid::kernel_with_inputs(
        &rt,
        target.format(),
        label,
        shader,
        u.bytes.len() as u32,
        if basic { 1 } else { 2 },
    )?;
    let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
    let mut resources = vec![Resource::Uniform, Resource::View(&source.view)];
    if !basic {
        resources.push(Resource::View(&mask.unwrap_or(source).view));
    }
    resources.push(Resource::View(&out.view));
    let bg = rt.bind_group(&kernel, &resources);
    let offset = gpu::push_uniform(&rt, &u);
    let mut pass = target.encoder().begin_compute_pass(&Default::default());
    gpu::dispatch(&mut pass, &kernel, &bg, &[offset], roi.size());
    drop(pass);
    Ok(out)
}

impl TargetEffect for Lumetri {
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
        let bounds = if p.layer_bounds.is_empty() {
            image_grid::pixel_rect(input.rect, 1. / grid)
        } else {
            image_grid::layer_rect(p)
        };
        let input_grid = image_grid::pixel_rect(input.rect, 1. / grid);
        if self.def.match_name == "CS Vignette" {
            let amount = (-v.f(1) / 100.).clamp(-5., 5.);
            if amount == 0. {
                return Ok(input.clone());
            }
            let mut u = UniformWriter::with_size(CHROMA_OFFSET + CHROMA_GRID);
            vignette_curve(&mut u, amount);
            let center = match p.value(3) {
                Some(Value::Vec2(xy)) => xy.map(|a| (a * p.scale) as f32),
                _ => [
                    (bounds.x0 + bounds.x1) as f32 * 0.5,
                    (bounds.y0 + bounds.y1) as f32 * 0.5,
                ],
            };
            let dx = (center[0] - 0.5 * (bounds.x0 + bounds.x1) as f32).round() as i32;
            let dy = (center[1] - 0.5 * (bounds.y0 + bounds.y1) as f32).round() as i32;
            u.u32(40, u32::from(p.float))
                .u32(44, 8 | 256)
                .i32s(
                    48,
                    &[
                        bounds.x0 + dx,
                        bounds.y0 + dy,
                        bounds.x1 + dx,
                        bounds.y1 + dy,
                    ],
                )
                .f32(64, (v.f(5) * 0.01) as f32)
                .f32s(224, &[amount as f32, (1. - v.f(2) / 120.) as f32, 0., 1.]);
            return render_pass(target, input, roi, grid, false, u, None);
        }
        let secondary_read = secondary_roi(v, roi).intersect(&input_grid);
        let read = secondary_read
            .inflate(sharpen_radius(v.f(29)) * i32::from(v.b(23)))
            .intersect(&input_grid);
        let mut image = input.clone();
        if v.b(3) {
            if let Some(Value::Bytes(data)) = p.value(125) {
                image = self.lut.apply(target, p, &image, read, data, 1.)?;
            }
            if [7, 8, 11, 12, 13, 14, 15, 16]
                .iter()
                .any(|&id| v.f(id) != 0.)
                || v.f(20) != 100.
            {
                let mut u = UniformWriter::with_size(HEADER + 3 * KNOTS * 4);
                u.u32(40, u32::from(p.float))
                    .f32s(64, &white_balance(v.f(7), v.f(8)))
                    .f32(76, (v.f(20) * 0.01) as f32)
                    .f32s(
                        80,
                        &[
                            v.f(11).exp2() as f32,
                            (v.f(15) * 0.01).exp2() as f32,
                            (v.f(16) * 0.0002) as f32,
                        ],
                    );
                for (i, id) in [12, 13, 14].into_iter().enumerate() {
                    tone_row(&mut u, HEADER + i * KNOTS * 4, i, v.f(id));
                }
                image = render_pass(target, &image, read, grid, true, u, None)?;
            }
        }
        if v.b(23)
            && let Some(Value::Bytes(data)) = p.value(126)
        {
            image = self
                .lut
                .apply(target, p, &image, read, data, (v.f(26) * 0.01) as f32)?;
        }
        let creative = v.b(23)
            && ([28, 29, 30].iter().any(|&id| v.f(id) != 0.)
                || v.f(31) != 100.
                || [1001, 1002].iter().any(|&id| color(v, id) != [0.5; 3]));
        let wheels = v.b(46)
            && [1003, 1004, 1005]
                .iter()
                .any(|&id| color(v, id) != [0.5; 3] || v.f(id + 10) != 0.);
        let key = v.b(68)
            && (v.b(74)
                || v.f(86) != 0.
                || v.f(87) != 0.
                || v.f(88) != 0.
                || v.f(90) != 100.
                || v.f(89) != 0.
                || color(v, 1016) != [0.5; 3]);
        let vignette = v.b(50) && v.f(51) != 0.;
        let has_curves = v.b(37)
            && [39, 42]
                .iter()
                .any(|&id| matches!(v.value(id),Some(Value::FloatList(p)) if !p.is_empty()));
        if !creative && !wheels && !key && !vignette && !has_curves {
            return Ok(image);
        }
        let mut u = UniformWriter::with_size(CHROMA_OFFSET + CHROMA_GRID);
        let active = if has_curves { curves(v, &mut u) } else { 0 };
        if vignette {
            vignette_curve(&mut u, v.f(51));
        }
        if creative {
            let at = HEADER + 5 * 256 * 4;
            let row = (v.f(28) * 0.1).clamp(0., 14.) as usize;
            u.bytes[at..at + 512]
                .copy_from_slice(&include_bytes!("film.bin")[row * 256..row * 256 + 512]);
            let row = ((v.f(30) + 100.) / 25.).clamp(0., 7.) as usize;
            u.bytes[at + 512..at + 512 + 4224]
                .copy_from_slice(&include_bytes!("vibrance.bin")[row * 2112..row * 2112 + 4224]);
        }
        let tint = creative && [1001, 1002].iter().any(|&id| color(v, id) != [0.5; 3]);
        if tint {
            split_tone(v, &mut u);
        }
        if key && v.f(88) != 0. {
            secondary_response(&mut u, v.f(88));
        }
        let flags = u32::from(tint) << 4
            | u32::from(creative)
            | u32::from(wheels) << 1
            | u32::from(key) << 2
            | u32::from(vignette) << 3;
        u.u32(40, u32::from(p.float))
            .u32(44, flags)
            .i32s(48, &image_grid::rect4(bounds));
        u.f32s(64, &[28, 29, 30, 31].map(|id| (v.f(id) * 0.01) as f32));
        for (i, id) in [1001, 1002, 1003, 1004, 1005].into_iter().enumerate() {
            let rgb = color(v, id);
            let y = rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
            let light = if i >= 2 {
                v.f(id + 10) as f32 / 200.
            } else {
                0.
            };
            u.f32s(80 + 16 * i, &rgb.map(|x| x - y + light));
        }
        soften(&mut u, v.f(29));
        u.f32(92, (v.f(33) * 0.01) as f32).u32(108, active);
        u.f32s(160, &color(v, 70))
            .u32(172, u32::from(p.value(70).is_some()));
        u.f32s(
            176,
            &[
                (v.f(1006) / 360.) as f32,
                (v.f(1007) * 0.01) as f32,
                (v.f(1008) * 0.01) as f32,
                (v.f(1009) * 0.01) as f32,
            ],
        );
        u.f32s(192, &white_balance(v.f(86), v.f(87)))
            .f32(204, (v.f(90) * 0.01) as f32);
        u.f32(208, v.f(88) as f32)
            .u32(212, u32::from(v.b(76)))
            .u32(216, if v.b(74) { v.popup(75) as u32 } else { 0 });
        u.f32s(
            224,
            &[
                v.f(51) as f32,
                (v.f(52) * 0.01) as f32,
                (v.f(53) * 0.01) as f32,
                (v.f(54) * 0.01) as f32,
            ],
        );
        let refine = key && (v.f(81) > 0. || v.f(82) > 0.);
        let sharpen = key && v.f(89) != 0. && !v.b(74);
        let mut flags = flags;
        if (refine || sharpen) && (creative || wheels || active != 0) {
            let mut upstream = UniformWriter {
                bytes: u.bytes.clone(),
            };
            upstream.u32(44, flags & !(4 | 8)).u32(40, 1);
            image = render_pass(target, &image, secondary_read, grid, false, upstream, None)?;
            flags &= 4 | 8;
            u.u32(108, 0);
        }
        let mut mask = None;
        if refine {
            let sigma = v.f(82) * p.scale;
            let plan = blur::plan_gaussian(input_grid, [sigma; 2], Edge::Repeat, roi);
            let mask_rect = plan.read.intersect(&input_grid);
            let mut settings = UniformWriter {
                bytes: u.bytes.clone(),
            };
            settings.u32(44, 64).f32(220, (v.f(81) * 0.01) as f32);
            let mut result = render_pass(target, &image, mask_rect, grid, false, settings, None)?;
            // Blur plans operate in render texels, including for preview scales.
            result.rect = mask_rect;
            result.pixel_scale = 1.;
            mask = Some(if sigma > 0. {
                plan.encode(target, &result)?
            } else {
                result
            });
            flags |= 128;
        }
        if sharpen {
            u.f32s(64, &[0., (v.f(89) * 0.01) as f32, 0., 1.]);
            soften(&mut u, v.f(89));
            flags |= 32;
        }
        let correction = color(v, 1016);
        let y = correction[0] * 0.2126 + correction[1] * 0.7152 + correction[2] * 0.0722;
        u.f32s(CORRECTION_OFFSET, &correction.map(|c| c - y));
        u.u32(44, flags);
        render_pass(target, &image, roi, grid, false, u, mask.as_ref())
    }
}

fn vignette_curve(u: &mut UniformWriter, amount: f64) {
    let row = (amount * 2. + 10.).clamp(0., 19.) as usize;
    let at = SPLIT_OFFSET - 512;
    u.bytes[at..at + 512]
        .copy_from_slice(&include_bytes!("vignette.bin")[row * 256..row * 256 + 512]);
}
pub(crate) fn cc_vignette() -> Builtin {
    let mut b = crate::catalog::DefBuilder::new("CS Vignette", "CC Vignette", "Stylize");
    b = b.slider(
        1,
        "Amount",
        "Amount",
        100.0,
        [-1000.0, 1000.0],
        [-1000.0, 1000.0],
        2,
        "none",
    );
    b = b.slider(
        2,
        "Angle of View",
        "Angle of View",
        45.0,
        [0.0, 120.0],
        [0.0, 120.0],
        2,
        "none",
    );
    b.push(
        3,
        "Center",
        "Center",
        bae_model::ParamKind::Point,
        Some(Value::Vec2([320.0, 180.0])),
    );
    b = b.slider(
        5,
        "Pin Highlights",
        "Pin Highlights",
        0.0,
        [0.0, 100.0],
        [0.0, 100.0],
        2,
        "none",
    );
    let (def, labels) = b.build();
    rust_builtin(
        (
            Arc::new(Lumetri {
                def,
                lut: LutCache::default(),
            }),
            labels,
        ),
        "cc-vignette",
        "CC Vignette",
        &["晕影", "vignette"],
    )
}
