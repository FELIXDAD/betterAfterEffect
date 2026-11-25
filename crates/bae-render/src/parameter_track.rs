use crate::{effect::ScalarTrack, graph::EffectFrame};
use bae_model::{AttrSpec, Interpolation, Time};
use std::sync::Arc;

/// Linear and held intervals keep their exact slopes. Ease segments are flattened
/// on an authored-key grid, independent of frame order. Expressions extend a
/// frame-aligned prefix once and reuse it on backward seeks.
pub(crate) fn prepare(
    track: &mut Arc<ScalarTrack>,
    attr: &AttrSpec,
    frame: &EffectFrame,
    mut expression: impl FnMut(Time) -> Option<f64>,
) -> Option<Arc<ScalarTrack>> {
    if attr.expression.is_some() {
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
