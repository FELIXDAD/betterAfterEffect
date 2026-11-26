//! GPU particle simulation and layer-driven surface effects.
mod catalog;
mod emission;
mod engine;
pub(crate) mod projection;
pub(crate) use engine::{Engine, prewarm};
mod scatter;
mod surface;
mod water;

use crate::effects::layer_input;
use crate::{Builtin, EffectTarget, TargetEffect, common, gpu::UniformWriter, params::Params};
use bae_model::{EffectDef, Value};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, effect::LayerDependency,
};

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    World,
    Systems,
    Rain,
    Snow,
    Bubbles,
    Drizzle,
    Stars,
    Scatter,
    Polly,
    Shatter,
    Cards,
    Playground,
    Ball,
    Mercury,
    Hair,
    Foam = 15,
    Wave = 16,
    Caustics = 17,
}

pub(crate) fn builtin(kind: Kind) -> Builtin {
    catalog::builtin(kind)
}

struct Simulation {
    def: EffectDef,
    kind: Kind,
}

// Every birth stays in one fixed layer-time bucket throughout the particle's
// life. Four adjacent buckets cover the live population at the requested time.
fn texture_times(now: f64, life: f64, at_birth: bool) -> [f64; 4] {
    let step = life.max(0.001) / 3.;
    let first = if at_birth {
        (now / step).floor() - 3.
    } else {
        0.
    };
    [0., 1., 2., 3.].map(|i| ((first + i) * step).max(0.))
}

fn max_lifetime(p: Params<'_>, id: u32) -> f64 {
    p.p.tracks.iter().find(|(n, _)| *n == id).map_or_else(
        || p.f(id),
        |(_, curve)| curve.maximum_through(p.p.time.as_seconds()),
    )
}

impl Simulation {
    fn sources(&self) -> &[u32] {
        match self.kind {
            Kind::Hair => &[8],
            Kind::Playground => &[201, 281, 501, 551, 253, 303, 353, 403, 423],
            Kind::Foam => &[54, 58, 64],
            Kind::Wave => &[36],
            Kind::Caustics => &[2, 10, 70],
            Kind::Cards => &[10, 12, 8],
            Kind::Shatter => &[8, 20, 62, 70, 66],
            _ => &[],
        }
    }
    fn inputs(&self, p: &EffectParams<'_>) -> Vec<Option<LayerDependency>> {
        if self.kind == Kind::Caustics {
            return self
                .sources()
                .iter()
                .map(|&id| water::dependency(p, id, id == 2))
                .collect();
        }
        if self.kind == Kind::World {
            let Some(texture) = layer_input::dependency(p, 45, false) else {
                return vec![];
            };
            let v = Params::new(&self.def, p);
            if v.popup(47) == 2 {
                return vec![Some(texture)];
            }
            let t = p.time.as_seconds();
            return texture_times(t, max_lifetime(v, 5), v.popup(47) == 1)
                .into_iter()
                .map(|local| {
                    let mut dep = texture.clone();
                    dep.time = p.comp_time
                        + bae_model::Time::from_seconds((local - t) * p.layer_time_scale);
                    Some(dep)
                })
                .collect();
        }
        if self.kind == Kind::Playground {
            let v = Params::new(&self.def, p);
            return self
                .sources()
                .iter()
                .map(|&id| {
                    let mut dep = layer_input::dependency(p, id, false)?;
                    if id == 281 {
                        let seconds = v.f(282);
                        let offset = if matches!(v.popup(284), 2 | 4) {
                            seconds - p.time.as_seconds()
                        } else {
                            seconds
                        };
                        dep.time = p.comp_time
                            + bae_model::Time::from_seconds(offset * p.layer_time_scale);
                    }
                    Some(dep)
                })
                .collect();
        }
        self.sources()
            .iter()
            .map(|id| layer_input::dependency(p, *id, matches!((self.kind, *id), (Kind::Hair, 8))))
            .collect()
    }
}

impl Effect for Simulation {
    fn def(&self) -> &EffectDef {
        &self.def
    }

    fn input_roi(&self, _: &EffectParams<'_>, out: IRect) -> IRect {
        if out.is_empty() {
            out
        } else {
            IRect::new(-1 << 26, -1 << 26, 1 << 26, 1 << 26)
        }
    }
    fn uses_scene(&self, p: &EffectParams<'_>) -> bool {
        let p = Params::new(&self.def, p);
        match self.kind {
            Kind::World => true,
            Kind::Mercury => p.popup(31) == 2,
            Kind::Caustics => p.popup(38) == 3,
            Kind::Cards => p.popup(66) == 3 || p.popup(102) == 3,
            Kind::Shatter => p.popup(72) == 3 || p.popup(108) == 3,
            Kind::Drizzle => p.popup(22) == 2,
            _ => false,
        }
    }
    fn full_surface(&self) -> bool {
        self.kind != Kind::Scatter
    }

    fn integrated_parameters(&self) -> &'static [u32] {
        match self.kind {
            Kind::World => &[4, 5],
            Kind::Mercury => &[6, 7],
            Kind::Foam => &[14, 20],
            Kind::Systems | Kind::Drizzle => &[1, 2],
            Kind::Playground => &[103],
            _ => &[],
        }
    }

    fn parameter_ids(&self) -> &'static [u32] {
        match self.kind {
            Kind::World => &[7, 8, 9],
            Kind::Systems => &[4],
            Kind::Mercury => &[3],
            Kind::Playground => &[101],
            _ => &[],
        }
    }

    fn parameter_times(&self, p: &EffectParams<'_>) -> Vec<bae_model::Time> {
        let t = p.time.as_seconds();
        let v = Params::new(&self.def, p);
        let life = match self.kind {
            Kind::World => max_lifetime(v, 5),
            Kind::Systems => max_lifetime(v, 2),
            Kind::Mercury => max_lifetime(v, 7),
            // The cannon keeps every particle: the whole emission so far.
            Kind::Playground => t.max(0.),
            _ => return Vec::new(),
        };
        let fps = p.frame_rate.as_f64();
        let stride = ((life * fps / 61.).ceil().max(1.)) / fps;
        let first = ((t - life).max(0.) / stride).floor() as i64;
        let last = (t.max(0.) / stride).ceil() as i64;
        (first..=last)
            .map(|i| bae_model::Time::from_seconds(i as f64 * stride))
            .collect()
    }

    fn mask_dependencies(&self, p: &EffectParams<'_>) -> Vec<bae_render::effect::MaskSelect> {
        if self.kind == Kind::Playground
            && let Some(Value::PrimRef(path)) = p.value(453)
        {
            vec![bae_render::effect::MaskSelect::One(path.clone())]
        } else {
            vec![]
        }
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        self.inputs(p).into_iter().flatten().collect()
    }
    fn render(
        &self,
        target: &mut EncodeContext<'_>,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(target, p, input, roi)
    }
}

const CONTROL_COUNT: usize = 145;

// Points are passed to the GPU in normalized layer coordinates, including AE's
// size-relative defaults (the reference composition measures 640 x 360).
struct Scene<'a> {
    p: Params<'a>,
    size: [f32; 2],
    origin: [f32; 2],
}

impl Scene<'_> {
    fn controls(&self, kind: Kind, seed: u32) -> UniformWriter {
        let history_offset = 32 + CONTROL_COUNT * 16;
        let mut u = UniformWriter::with_size(history_offset + 16 + 64 * 16);
        u.u32(0, kind as u32).u32(8, seed).f32s(16, &self.size);
        for (index, c) in self
            .p
            .def
            .params
            .iter()
            .filter(|c| c.match_name.is_some())
            .enumerate()
        {
            let slot = if kind == Kind::Playground {
                index
            } else {
                c.id as usize
            };
            let offset = 32 + 16 * slot;
            match self.p.value(c.id) {
                Some(Value::Vec2(pos)) => {
                    let pos = if self.p.p.value(c.id).is_some() {
                        [
                            (pos[0] as f32 - self.origin[0]) / self.size[0],
                            (pos[1] as f32 - self.origin[1]) / self.size[1],
                        ]
                    } else {
                        [pos[0] as f32 / 640., pos[1] as f32 / 360.]
                    };
                    u.f32s(offset, &pos);
                }
                Some(Value::Color(v)) => {
                    u.f32s(offset, &v.map(|v| v as f32));
                }
                Some(Value::PrimRef(_)) => {
                    u.f32(offset, 1.);
                }
                None => {}
                Some(_) => {
                    u.f32(offset, self.p.f(c.id) as f32);
                }
            }
        }
        let samples = self.p.p.parameter_samples;
        u.u32(history_offset, samples.len().min(64) as u32);
        if kind == Kind::Playground {
            for (offset, ids) in [(4, [502, 506, 510]), (8, [552, 556, 560])] {
                let count = ids
                    .iter()
                    .rposition(|&id| self.p.popup(id) > 1)
                    .map_or(0, |n| n + 1);
                u.u32(history_offset + offset, count as u32);
            }
        }
        for (index, sample) in samples.iter().take(64).enumerate() {
            let value = |id| {
                sample
                    .values
                    .iter()
                    .find(|(k, _)| *k == id)
                    .map(|(_, v)| v)
                    .or_else(|| self.p.p.value(id))
            };
            let position = if kind == Kind::World {
                let component =
                    |id| value(id).and_then(Value::as_f64).unwrap_or(0.) as f32 * self.size[0];
                [
                    self.size[0] * 0.5 + component(7),
                    self.size[1] * 0.5 + component(8),
                    component(9),
                ]
            } else if let Some(Value::Vec2(xy)) = value(match kind {
                Kind::Mercury => 3,
                Kind::Playground => 101,
                _ => 4,
            }) {
                [
                    xy[0] as f32 - self.origin[0],
                    xy[1] as f32 - self.origin[1],
                    0.,
                ]
            } else {
                [self.size[0] * 0.5, self.size[1] * 0.5, 0.]
            };
            u.f32s(
                history_offset + 16 + 16 * index,
                &[
                    position[0],
                    position[1],
                    position[2],
                    sample.time.as_seconds() as f32,
                ],
            );
        }
        u
    }

    /// Slots per Particle Playground particle: Explode Particles splits each into five.
    fn playground_children(&self) -> f64 {
        if self.p.f(231) != 0. { 5. } else { 1. }
    }

    fn count(&self, k: Kind) -> u32 {
        let f = |id| self.p.f(id);
        let tiles = |spacing: f64| {
            (self.size[0] as f64 / spacing.max(1.)).ceil()
                * (self.size[1] as f64 / spacing.max(1.)).ceil()
        };
        let n = match k {
            Kind::World | Kind::Systems | Kind::Drizzle | Kind::Scatter => 0.,

            Kind::Mercury | Kind::Wave | Kind::Foam | Kind::Caustics => 0.,
            Kind::Ball => tiles(f(6) + 1.),
            Kind::Hair => 4. * (f(5) * self.size[0] as f64 * self.size[1] as f64 / 10000.).ceil(),
            // The grid and the exploded layer; the cannon's births are intervals.
            Kind::Playground => {
                let grid = f(134).round() * f(135).round();
                let explosion = if self.p.p.value(201).is_some() {
                    tiles((2. * f(202)).max(2.))
                } else {
                    0.
                };
                ((grid + explosion) * self.playground_children()).min(2_000_000.)
            }
            Kind::Rain | Kind::Snow => f(1),
            Kind::Bubbles => f(1),
            Kind::Stars => tiles(f(4)),
            Kind::Polly => tiles(f(7)) * if f(8) <= 2. { 2. } else { 1. },
            Kind::Shatter => {
                let spacing = self.size[0] as f64 / f(10).max(1.);
                let columns = f(10).ceil() + if f(6) == 3. { 1. } else { 0. };
                columns * (self.size[1] as f64 / spacing).ceil() * if f(6) >= 19. { 2. } else { 1. }
            }
            Kind::Cards => {
                f(4).round()
                    * if self.p.popup(2) == 1 {
                        f(6).round()
                    } else {
                        (f(4) * self.size[0] as f64 / self.size[1] as f64)
                            .round()
                            .max(1.)
                    }
            }
        };
        n.ceil() as u32
    }
}

impl TargetEffect for Simulation {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        params: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        if roi.is_empty() {
            return Ok(input.clone());
        }
        let p = Params::new(&self.def, params);
        if matches!(self.kind, Kind::Wave | Kind::Caustics) {
            return water::render(self.kind, target, p, input, roi);
        }
        if self.kind == Kind::Scatter {
            return scatter::render(target, &p, input, roi);
        }
        // Rasterize a guard band around a requested tile. The two-pixel grid
        // keeps fragment quads aligned with the full layer at tile edges.
        let align = |x: i32, base: i32| base + (x - base).div_euclid(2) * 2;
        let out = IRect::new(
            align(roi.x0, input.rect.x0).saturating_sub(2),
            align(roi.y0, input.rect.y0).saturating_sub(2),
            roi.x1.saturating_add(2),
            roi.y1.saturating_add(2),
        )
        .intersect(&input.rect);
        if out.is_empty() {
            return Ok(input.clone());
        }
        if self.kind == Kind::Polly && params.time.as_seconds() <= p.f(10) {
            return Ok(input.clone());
        }
        let (bounds, s) = if params.layer_bounds.is_empty() {
            (input.rect, params.scale as f32)
        } else {
            (params.layer_bounds, 1.)
        };
        let scene = Scene {
            p,
            size: [bounds.width() as f32 / s, bounds.height() as f32 / s],
            origin: [bounds.x0 as f32 / s, bounds.y0 as f32 / s],
        };
        let input = common::premultiplied(target, input)?;
        let mut maps: Vec<_> = self
            .inputs(params)
            .into_iter()
            .map(|dep| dep.and_then(|d| target.effect_layer(&d).cloned()))
            .collect();
        if self.kind == Kind::Playground {
            maps.insert(
                4,
                self.mask_dependencies(params)
                    .first()
                    .and_then(|s| target.effect_mask(s))
                    .cloned(),
            );
        }
        engine::render(self.kind, target, &scene, &input, out, &maps)
    }
}
