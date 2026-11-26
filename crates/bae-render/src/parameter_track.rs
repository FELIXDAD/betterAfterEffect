use crate::{effect::ScalarTrack, graph::EffectFrame};
use bae_model::{AttrSpec, Interpolation, PrimPath, Stage, Time};
use rustc_hash::FxHashMap;
use std::sync::Arc;

#[derive(Debug, Default)]
pub(crate) struct TrackCache {
    revision: Option<u64>,
    entries: FxHashMap<(PrimPath, String), CachedTrack>,
}

#[derive(Debug)]
struct CachedTrack {
    // Arbitrary JavaScript can read another composition or a different time.
    // Only a property-local expression can reuse samples after a document edit.
    local_input: Option<AttrSpec>,
    grid: Option<(Time, Time, f64)>,
    samples: Arc<ScalarTrack>,
}

impl TrackCache {
    pub(crate) fn get(
        &mut self,
        stage: &Stage,
        path: &PrimPath,
        name: &str,
        attr: &AttrSpec,
        frame: &EffectFrame,
    ) -> &mut Arc<ScalarTrack> {
        if self.revision != Some(stage.revision()) {
            self.entries.retain(|(path, name), entry| {
                let Some(current) = stage.prim(path).and_then(|prim| prim.attr(name)) else {
                    return false;
                };
                let Some(previous) = &mut entry.local_input else {
                    return false;
                };
                if current != previous {
                    if !has_expression(current) || previous.expression != current.expression {
                        return false;
                    }
                    // A local expression sees only its value at the sample time.
                    // The segment preceding the first edited key can change too.
                    let common = previous
                        .keyframes
                        .iter()
                        .zip(&current.keyframes)
                        .take_while(|(a, b)| a == b)
                        .last();
                    let cutoff = common.map_or(Time::ZERO, |((time, _), _)| *time);
                    Arc::make_mut(&mut entry.samples).truncate_from(cutoff);
                    *previous = current.clone();
                }
                true
            });
            self.revision = Some(stage.revision());
        }
        let entry = self
            .entries
            .entry((path.clone(), name.to_owned()))
            .or_insert_with(|| {
                let local = !has_expression(attr)
                    || attr
                        .expression
                        .as_ref()
                        .is_some_and(|e| property_local(&e.source));
                CachedTrack {
                    local_input: local.then(|| attr.clone()),
                    grid: None,
                    samples: Arc::default(),
                }
            });
        if has_expression(attr) {
            let grid = (
                frame.layer_start_time,
                frame.time_step,
                frame.layer_time_scale,
            );
            if entry.grid != Some(grid) {
                entry.samples = Arc::default();
                entry.grid = Some(grid);
            }
        }
        &mut entry.samples
    }
}

fn has_expression(attr: &AttrSpec) -> bool {
    attr.expression
        .as_ref()
        .is_some_and(|e| e.enabled && !e.source.trim().is_empty())
}

// Recognize only arithmetic on the current time/value and pure scalar functions.
// QuickJS still evaluates the original source. Other syntax may access the stage.
fn property_local(source: &str) -> bool {
    source
        .split(|c: char| c.is_ascii_whitespace() || "+-*/%(),;?:<>".contains(c))
        .all(|part| {
            part.is_empty()
                || part.parse::<f64>().is_ok()
                || matches!(
                    part,
                    "time"
                        | "value"
                        | "linear"
                        | "ease"
                        | "easeIn"
                        | "easeOut"
                        | "clamp"
                        | "Math.PI"
                        | "Math.abs"
                        | "Math.min"
                        | "Math.max"
                        | "Math.floor"
                        | "Math.ceil"
                        | "Math.round"
                        | "Math.sin"
                        | "Math.cos"
                        | "Math.exp"
                        | "Math.pow"
                        | "Math.sqrt"
                )
        })
}

/// Linear and held intervals keep their exact slopes. Ease segments are flattened
/// on an authored-key grid, independent of frame order. Expressions extend a
/// frame-aligned prefix once and reuse it on backward seeks.
pub(crate) fn prepare(
    track: &mut Arc<ScalarTrack>,
    attr: &AttrSpec,
    frame: &EffectFrame,
    mut expression: impl FnMut(Time) -> Option<f64>,
) -> Option<Arc<ScalarTrack>> {
    if has_expression(attr) {
        let step = frame.time_step.as_seconds() / frame.layer_time_scale.abs().max(0.000001);
        let last = (frame.time.as_seconds().max(0.) / step).ceil() as usize;
        for i in track.len()..=last {
            let time = Time::from_seconds(i as f64 * step);
            let value = expression(frame.comp_at(time))?;
            Arc::make_mut(track).push(time, value, value);
        }
        return Some(track.clone());
    }
    if track.len() != 0 {
        return Some(track.clone());
    }
    let curve = bae_anim::Curve::new(&attr.keyframes, bae_anim::CurveKind::PerDimension)?;
    let mut times = vec![(Time::ZERO, false)];
    let mut previous = None;
    for (&end, key) in &attr.keyframes {
        if end > Time::ZERO {
            if let Some((start, out)) = previous {
                match (out, key.interp_in) {
                    (Interpolation::Hold, _) => {}
                    (Interpolation::Bezier, _) | (_, Interpolation::Bezier) => {
                        for i in 1..64 {
                            let t = start
                                + Time::from_seconds(
                                    (end - start).as_seconds() * f64::from(i) / 64.,
                                );
                            if t > Time::ZERO {
                                times.push((t, false))
                            }
                        }
                    }
                    _ => {}
                }
            }
            times.push((
                end,
                previous.is_some_and(|(_, out)| out == Interpolation::Hold),
            ));
        }
        previous = Some((end, key.interp_out));
    }
    times.dedup();
    let mut compiled = ScalarTrack::default();
    for (t, hold) in times {
        let value = curve.sample(t).as_f64()?;
        let incoming = if hold {
            curve.sample(t - Time::from_flicks(1)).as_f64()?
        } else {
            value
        };
        compiled.push(t, value, incoming);
    }
    *track = Arc::new(compiled);
    Some(track.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_model::{AuthorId, CommandRegistry, FrameRate, Keyframe, Value};
    use serde_json::json;

    #[test]
    fn editing_a_rate_preserves_only_the_unaffected_expression_samples() {
        let fps = FrameRate::FPS_30;
        let t = Time::from_seconds;
        let frame = EffectFrame {
            time: t(10.),
            comp_time: t(10.),
            layer_start_time: Time::ZERO,
            layer_in: Time::ZERO,
            layer_time_scale: 1.,
            scale: 1.,
            layer_bounds: crate::IRect::from_size(64, 64),
            frame_rate: fps,
            time_step: fps.frame_duration(),
            linear: true,
            purpose: crate::RenderPurpose::Preview,
            float: false,
            shutter: [0.; 2],
        };
        let mut scene = crate::fixture::SceneBuilder::new();
        let comp = scene.comp("Main", 64, 64, fps, t(12.)).unwrap();
        let layer = scene.layer(&comp, "Emitter", "solid", None).unwrap();
        let name = "transform:rotation";
        for sec in [0., 8., 10.] {
            scene.key(&layer, name, t(sec), Value::Float(1.)).unwrap();
        }
        let mut stage = scene.into_stage();
        let commands = CommandRegistry::with_builtins();
        let author = AuthorId::new("test").unwrap();
        let set = |stage: &mut Stage, source: &str| {
            commands
                .execute(
                    stage,
                    &author,
                    "attr.set_expression",
                    json!({"path":layer,"attr":name,"source":source}),
                )
                .unwrap();
        };
        let mut cache = TrackCache::default();
        let mut eval = bae_anim::Evaluator::new();
        let mut run = |stage: &Stage, frame: &EffectFrame| {
            eval.clear();
            let attr = stage.prim(&layer).unwrap().attr(name).unwrap();
            let slot = cache.get(stage, &layer, name, attr, frame);
            let mut calls = 0;
            let track = prepare(slot, attr, frame, |time| {
                calls += 1;
                eval.evaluate_attribute(stage, &layer, name, time)
                    .ok()?
                    .as_f64()
            })
            .unwrap();
            (track.at(frame.time.as_seconds()), calls)
        };
        set(&mut stage, "time + value");
        let original = run(&stage, &frame);
        assert_eq!(original.1, 301);
        commands
            .execute(
                &mut stage,
                &author,
                "attr.set",
                json!({"path":layer,"attr":"name","value": Value::String("Particle".into())}),
            )
            .unwrap();
        assert_eq!(run(&stage, &frame), (original.0, 0));
        commands.execute(&mut stage, &author, "key.set",
            json!({"path":layer,"attr":name,"time":t(10.).flicks(),"keyframe":Keyframe::linear(3.)})).unwrap();
        let (edited, calls) = run(&stage, &frame);
        assert_eq!(calls, 61);
        assert!((edited.1 - original.0.1 - 2.).abs() < 1e-7);
        assert_eq!(
            run(
                &stage,
                &EffectFrame {
                    time: t(2.),
                    comp_time: t(2.),
                    ..frame
                }
            )
            .1,
            0
        );
        stage.undo(&author).unwrap();
        assert_eq!(run(&stage, &frame), (original.0, 61));
        set(&mut stage, "thisComp.width / 2");
        assert_eq!(run(&stage, &frame), ((32., 320.), 301));
        commands
            .execute(
                &mut stage,
                &author,
                "attr.set",
                json!({"path":comp,"attr":"width","value":Value::Int(128)}),
            )
            .unwrap();
        assert_eq!(run(&stage, &frame), ((64., 640.), 301));
        set(&mut stage, "time");
        let ((value, area), calls) = run(&stage, &frame);
        assert_eq!((value, calls), (10., 301));
        assert!((area - 50.).abs() < 1e-7);
        let reversed = EffectFrame {
            layer_start_time: t(12.),
            layer_time_scale: -0.5,
            comp_time: t(7.),
            ..frame
        };
        let ((value, area), calls) = run(&stage, &reversed);
        assert_eq!((value, calls), (7., 151));
        assert!((area - 95.).abs() < 1e-7);
    }
}
