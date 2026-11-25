//! Layer > Time commands (AE-ANI-087 … 092): Time Remap, Time Stretch, Freeze
//! Frame, Freeze On Last Frame and Time-Reverse Layer. Evaluation (layer time,
//! remapped source time, frame selection) is `bae-anim` / `bae-media`; these
//! commands only write the layer's timing attributes and Time Remap keyframes.
//!
//! Keyframes are stored in layer time, so changing the stretch or start time moves
//! them with the layer, as in AE.

use bae_model::{AttrSpec, Command, CommandError, Keyframe, PrimPath, Time, Transaction, Value};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::query;
use crate::util::{ensure_unlocked, fail, set, write_attr};

const ENABLED: &str = "time_remap_enabled";
const REMAP: &str = "time_remap";

/// The duration of `layer`'s time-based source, or an error naming it.
fn source_duration(tx: &Transaction<'_>, layer: &PrimPath) -> Result<Time, CommandError> {
    query::source_duration(tx.stage(), layer).ok_or_else(|| {
        fail(format!(
            "{layer}: time remapping needs footage with a duration or a pre-comp"
        ))
    })
}

/// One frame of the comp `layer` is in (AE's Freeze On Last Frame holds one
/// comp frame before the source end).
fn comp_frame(tx: &Transaction<'_>, layer: &PrimPath) -> Time {
    query::owning_comp(tx.stage(), layer)
        .and_then(|c| query::comp_info(tx.stage(), &c))
        .map_or(bae_model::FrameRate::FPS_30, |c| c.frame_rate)
        .frame_duration()
}

fn remap_keys(keys: impl IntoIterator<Item = (Time, Keyframe)>) -> AttrSpec {
    AttrSpec {
        keyframes: keys.into_iter().collect(),
        ..AttrSpec::default()
    }
}

// ---------------------------------------------------------------------------

pub struct SetTimeRemap;

#[derive(Deserialize, JsonSchema)]
pub struct SetTimeRemapParams {
    /// Layers with footage that has a duration, or pre-comp layers.
    pub layers: Vec<PrimPath>,
    /// On: identity keys at the source start and end (AE). Off: the Time Remap
    /// keys are deleted.
    pub enabled: bool,
}

impl Command for SetTimeRemap {
    type Params = SetTimeRemapParams;
    type Output = ();
    const ID: &'static str = "layer.time_remap";
    const TITLE: &'static str = "Enable Time Remapping";
    const DESCRIPTION: &'static str = "Turn Time Remapping on or off (AE: Layer > Time > Enable Time Remapping, Cmd/Ctrl+Alt+T). On creates two linear keys mapping the source start and end to themselves; off deletes the keys.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        ensure_unlocked(tx.stage(), &p.layers)?;
        for l in &p.layers {
            if p.enabled {
                let d = source_duration(tx, l)?;
                set(tx, l, ENABLED, Value::Bool(true))?;
                write_attr(
                    tx,
                    l,
                    REMAP,
                    remap_keys([
                        (Time::ZERO, Keyframe::linear(Value::Time(Time::ZERO))),
                        (d, Keyframe::linear(Value::Time(d))),
                    ]),
                )?;
            } else {
                set(tx, l, ENABLED, Value::Bool(false))?;
                write_attr(tx, l, REMAP, AttrSpec::default())?;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

/// The point a time stretch keeps in place (AE Time Stretch > Hold in Place).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StretchHold {
    /// 图层入点
    #[default]
    InPoint,
    /// 当前帧 (`time`)
    CurrentFrame,
    /// 图层出点
    OutPoint,
}

pub struct TimeStretch;

#[derive(Deserialize, JsonSchema)]
pub struct TimeStretchParams {
    pub layers: Vec<PrimPath>,
    /// New stretch in percent (−9900 … 9900; 0 < |v| < 1 becomes ±1; negative
    /// plays backwards). Give this or `new_duration`.
    #[serde(default)]
    pub stretch: Option<f64>,
    /// New layer duration (comp flicks): stretch = duration / source span.
    #[serde(default)]
    pub new_duration: Option<Time>,
    /// What stays in place.
    #[serde(default)]
    pub hold: StretchHold,
    /// The current time (comp flicks), for `hold = current_frame`.
    #[serde(default)]
    pub time: Option<Time>,
}

/// AE's stretch range and coercion (AE-ANI-087).
fn clamp_stretch(s: f64) -> f64 {
    let s = s.clamp(-9900.0, 9900.0);
    if s != 0.0 && s.abs() < 1.0 {
        s.signum()
    } else {
        s
    }
}

impl Command for TimeStretch {
    type Params = TimeStretchParams;
    type Output = ();
    const ID: &'static str = "layer.time_stretch";
    const TITLE: &'static str = "Time Stretch";
    const DESCRIPTION: &'static str = "Set layers' stretch factor or new duration, holding the in point, the current frame or the out point in place (AE: Layer > Time > Time Stretch). Keyframes move with the layer.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        ensure_unlocked(tx.stage(), &p.layers)?;
        if p.stretch.is_none() == p.new_duration.is_none() {
            return Err(fail("give either `stretch` or `new_duration`"));
        }
        for l in &p.layers {
            let t = query::layer_timing(tx.stage(), l);
            let old = if t.stretch.abs() < 1.0 {
                100.0
            } else {
                t.stretch
            };
            let new = match (p.stretch, p.new_duration) {
                (Some(s), _) => clamp_stretch(s),
                (None, Some(d)) => {
                    // The source span the layer shows now, at 100 %.
                    let span = (t.out_point - t.in_point).0 as f64 * 100.0 / old.abs();
                    if span <= 0.0 || d <= Time::ZERO {
                        return Err(fail(format!("{l}: the new duration must be positive")));
                    }
                    clamp_stretch(old.signum() * d.0 as f64 / span * 100.0)
                }
                (None, None) => unreachable!("checked above"),
            };
            if new == 0.0 {
                return Err(fail("the stretch factor cannot be 0"));
            }
            let h = match p.hold {
                StretchHold::InPoint => t.in_point,
                StretchHold::OutPoint => t.out_point,
                StretchHold::CurrentFrame => p
                    .time
                    .ok_or_else(|| fail("`time` is needed to hold the current frame"))?,
            };
            // The layer-time instant shown at h stays at h; in and out points scale
            // about h.
            let lh = t.layer_time(h);
            let start = h - Time((lh.0 as f64 * new / 100.0).round() as i64);
            let ratio = new / old;
            let scale = |c: Time| h + Time(((c - h).0 as f64 * ratio).round() as i64);
            let (a, b) = (scale(t.in_point), scale(t.out_point));
            set(tx, l, "stretch", Value::Float(new))?;
            set(tx, l, "start_time", Value::Time(start))?;
            set(tx, l, "in_point", Value::Time(a.min(b)))?;
            set(tx, l, "out_point", Value::Time(a.max(b)))?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct TimeReverse;

#[derive(Deserialize, JsonSchema)]
pub struct LayersParams {
    pub layers: Vec<PrimPath>,
}

impl Command for TimeReverse {
    type Params = LayersParams;
    type Output = ();
    const ID: &'static str = "layer.time_reverse";
    const TITLE: &'static str = "Time-Reverse Layer";
    const DESCRIPTION: &'static str = "Play layers backwards keeping their in and out points (AE: Layer > Time > Time-Reverse Layer, Cmd/Ctrl+Alt+R).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        ensure_unlocked(tx.stage(), &p.layers)?;
        for l in &p.layers {
            let t = query::layer_timing(tx.stage(), l);
            set(tx, l, "stretch", Value::Float(-t.stretch))?;
            set(
                tx,
                l,
                "start_time",
                Value::Time(t.in_point + t.out_point - t.start_time),
            )?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct FreezeFrame;

#[derive(Deserialize, JsonSchema)]
pub struct FreezeFrameParams {
    pub layers: Vec<PrimPath>,
    /// The current time (comp flicks): the frame shown there is held.
    pub time: Time,
}

impl Command for FreezeFrame {
    type Params = FreezeFrameParams;
    type Output = ();
    const ID: &'static str = "layer.freeze_frame";
    const TITLE: &'static str = "Freeze Frame";
    const DESCRIPTION: &'static str = "Hold the frame shown at the current time for the whole layer (AE: Layer > Time > Freeze Frame): Time Remapping on with one hold key.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        ensure_unlocked(tx.stage(), &p.layers)?;
        for l in &p.layers {
            source_duration(tx, l)?;
            let t = query::layer_timing(tx.stage(), l);
            let at = t.layer_time(p.time);
            // The source time shown now: the current remapping, or the layer time.
            let enabled = tx
                .stage()
                .prim(l)
                .and_then(|v| v.value(ENABLED))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let shown = if enabled {
                match bae_anim::evaluate_attribute(tx.stage(), l, REMAP, p.time) {
                    Ok(Value::Time(s)) => s,
                    Ok(other) => Time::from_seconds(other.as_f64().unwrap_or(0.0)),
                    Err(_) => at,
                }
            } else {
                at
            };
            set(tx, l, ENABLED, Value::Bool(true))?;
            // AE makes it hold on both sides.
            let key = Keyframe {
                interp_in: bae_model::Interpolation::Hold,
                ..Keyframe::hold(Value::Time(shown))
            };
            write_attr(tx, l, REMAP, remap_keys([(at, key)]))?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------

pub struct FreezeOnLastFrame;

impl Command for FreezeOnLastFrame {
    type Params = LayersParams;
    type Output = ();
    const ID: &'static str = "layer.freeze_on_last_frame";
    const TITLE: &'static str = "Freeze On Last Frame";
    const DESCRIPTION: &'static str = "Play layers normally, then hold their last frame to the end of the comp (AE: Layer > Time > Freeze On Last Frame).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        ensure_unlocked(tx.stage(), &p.layers)?;
        for l in &p.layers {
            let d = source_duration(tx, l)?;
            let last = (d - comp_frame(tx, l)).max(Time::ZERO);
            set(tx, l, ENABLED, Value::Bool(true))?;
            write_attr(
                tx,
                l,
                REMAP,
                remap_keys([
                    (Time::ZERO, Keyframe::linear(Value::Time(Time::ZERO))),
                    (last, Keyframe::hold(Value::Time(last))),
                ]),
            )?;
            let comp_end = query::owning_comp(tx.stage(), l)
                .and_then(|c| query::comp_info(tx.stage(), &c))
                .map(|c| c.duration);
            if let Some(end) = comp_end {
                let t = query::layer_timing(tx.stage(), l);
                if end > t.out_point {
                    set(tx, l, "out_point", Value::Time(end))?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stretch_coercion() {
        assert_eq!(clamp_stretch(0.5), 1.0);
        assert_eq!(clamp_stretch(-0.2), -1.0);
        assert_eq!(clamp_stretch(20000.0), 9900.0);
        assert_eq!(clamp_stretch(250.0), 250.0);
    }
}
