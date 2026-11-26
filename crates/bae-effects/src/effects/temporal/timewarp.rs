//! Retimes a layer and, when selected, its foreground and background separately.
use super::*;

fn layer_at(v: Params<'_>, id: u32, time: Time) -> Option<LayerDependency> {
    let mut dep = layer_input::dependency(v.p, id, false)?;
    dep.time = time;
    Some(dep)
}

pub(super) fn dependencies(p: &EffectParams<'_>, v: Params<'_>) -> Vec<LayerDependency> {
    let mut deps = Vec::new();
    for (time, _) in mixed_frames(Kind::Timewarp, p, v) {
        if time != p.comp_time
            && let Some(dep) = before_effect(p, time)
        {
            deps.push(dep);
        }
        deps.extend(layer_at(v, 29, time));
        if v.popup(1) == 3 {
            deps.extend(layer_at(v, 31, time));
        }
    }
    deps
}

struct Plate {
    source: GpuImage,
    guide: Option<GpuImage>,
    matte: Option<GpuImage>,
    time: Time,
}

#[derive(Clone, Copy, PartialEq)]
enum Part {
    All,
    Matte,
    Foreground,
    Background,
}

impl Part {
    fn flag(self) -> u32 {
        match self {
            Self::All => 0,
            Self::Matte => 1,
            Self::Foreground => 2,
            Self::Background => 3,
        }
    }
}

fn prepare(
    t: &mut dyn EffectTarget,
    pass: &Pass,
    src: &GpuImage,
    matte: Option<&GpuImage>,
    part: Part,
    v: Params<'_>,
) -> Result<GpuImage, RenderError> {
    let crop = [34, 35, 36, 37].map(|id| (v.f(id) * 0.01) as f32);
    if part == Part::All && crop == [0., 1., 0., 1.] {
        return Ok(src.clone());
    }
    let flags = [
        5,
        part.flag(),
        v.popup(30) as u32,
        if matte.is_some() {
            1
        } else if layer_at(v, 29, v.p.comp_time).is_some() {
            2
        } else {
            0
        },
    ];
    let mut image = pass.dispatch(t, [src, matte.unwrap_or(src), src], flags, crop)?;
    // Derived inputs retain a content identity so tuned or matted fields also cache.
    if let Some(source) = src.content_key() {
        let mask = matte.map(GpuImage::content_key);
        if mask != Some(None) {
            use std::hash::BuildHasher;
            image = image.with_content_key(rustc_hash::FxBuildHasher.hash_one((
                "timewarp plate",
                source,
                mask,
                flags,
                crop.map(f32::to_bits),
                pass.layer,
                pass.grid.to_bits(),
            )));
        }
    }
    Ok(image)
}

pub(super) fn render(
    target: &mut dyn EffectTarget,
    p: &EffectParams<'_>,
    v: Params<'_>,
    input: &GpuImage,
    pass: &Pass,
) -> Result<GpuImage, RenderError> {
    let mut pairs: Vec<(f64, Vec<f32>)> = Vec::new();
    let samples = timewarp_samples(p, v);
    let weight = 1. / samples.len() as f32;
    for s in samples {
        let f = s.floor();
        let phase = if v.popup(1) == 1 { 0. } else { (s - f) as f32 };
        if let Some((_, phases)) = pairs.last_mut().filter(|(frame, _)| *frame == f) {
            phases.push(phase);
        } else {
            pairs.push((f, vec![phase]));
        }
    }
    let layer = layer_input::own_layer(p).unwrap_or_else(|| p.effect.path.clone());
    let full = Pass {
        roi: pass.layer,
        layer: pass.layer,
        grid: pass.grid,
    };
    let has_matte = layer_at(v, 29, p.comp_time).is_some();
    let parts: &[Part] = match v.popup(32) {
        2 => &[Part::Matte],
        3 => &[Part::Foreground],
        4 => &[Part::Background],
        _ if has_matte => &[Part::Foreground, Part::Background],
        _ => &[Part::All],
    };
    let mut out = None;
    for (frame, phases) in pairs {
        let plate = |frame| {
            let time = source_frame_time(p, frame);
            let sample = |id| {
                layer_at(v, id, time)
                    .and_then(|d| target.effect_layer(&d))
                    .cloned()
            };
            Some(Plate {
                source: frame_at(target, p, input, time)?,
                guide: sample(31),
                matte: sample(29),
                time,
            })
        };
        let Some(a) = plate(frame) else {
            continue;
        };
        let b = phases
            .iter()
            .any(|p| *p > 1e-4)
            .then(|| plate(frame + 1.))
            .flatten();
        for &part in parts {
            let prep = |target: &mut dyn EffectTarget, src: &GpuImage, plate: &Plate| {
                prepare(target, &full, src, plate.matte.as_ref(), part, v)
            };
            let first = prep(target, &a.source, &a)?;
            let Some(b) = &b else {
                out = Some(accumulate(
                    target,
                    pass,
                    p,
                    input,
                    out,
                    &first,
                    phases.len() as f32 * weight,
                )?);
                continue;
            };
            let second = prep(target, &b.source, b)?;
            if v.popup(1) != 3 {
                let mix: f32 = phases.iter().sum();
                out = Some(accumulate(
                    target,
                    pass,
                    p,
                    input,
                    out,
                    &first,
                    (phases.len() as f32 - mix) * weight,
                )?);
                out = Some(accumulate(
                    target,
                    pass,
                    p,
                    input,
                    out,
                    &second,
                    mix * weight,
                )?);
                continue;
            }
            let guide = |target: &mut dyn EffectTarget,
                         plate: &Plate,
                         source: &GpuImage|
             -> Result<GpuImage, RenderError> {
                // A matte display uses the image's motion to retime its coverage.
                let analysis_part = if part == Part::Matte {
                    Part::Foreground
                } else {
                    part
                };
                match (&plate.guide, analysis_part == part) {
                    (None, true) => Ok(source.clone()),
                    _ => prepare(
                        target,
                        &full,
                        plate.guide.as_ref().unwrap_or(&plate.source),
                        plate.matte.as_ref(),
                        analysis_part,
                        v,
                    ),
                }
            };
            let guides = [guide(target, &a, &first)?, guide(target, b, &second)?];
            let pair = flow::Pair {
                images: [&guides[0], &guides[1]],
                times: [a.time, b.time],
                layer: &layer,
                roi: pass.roi,
                grid: pass.grid,
                detail: v.f(6),
                analysis: flow::Analysis::timewarp(v),
            };
            let [ab, ba] = pair.both(target)?;
            for phase in &phases {
                let image = pair.interpolate(
                    target,
                    [&first, &second],
                    [&ab, &ba],
                    *phase,
                    v.b(12),
                    v.popup(14) == 2,
                )?;
                out = Some(accumulate(target, pass, p, input, out, &image, weight)?);
            }
        }
    }
    Ok(out.unwrap_or_else(|| input.clone()))
}
