//! Layer-space motion tracking. Each editing gesture or analysis run is a transaction.
mod apply;
mod stabilize;
use crate::query;
use crate::util::{child, create, ensure_folder, ensure_unlocked, fail, set, write_attr};
pub use apply::ApplyTrack;
use bae_model::{
    AttrSpec, Command, CommandError, Keyframe, Op, PrimPath, PrimType, Stage, Time, Transaction,
    Value,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub use stabilize::RecordStabilization;
use std::collections::BTreeMap;

pub const FOLDER: &str = "motion_trackers";
pub const PROPERTIES: [(&str, &str); 7] = [
    ("feature_center", "Feature Center"),
    ("feature_size", "Feature Size"),
    ("search_offset", "Search Offset"),
    ("search_size", "Search Size"),
    ("confidence", "Confidence"),
    ("attach_point", "Attach Point"),
    ("attach_offset", "Attach Point Offset"),
];

pub fn trackers(stage: &Stage, layer: &PrimPath) -> Vec<PrimPath> {
    layer
        .child(FOLDER)
        .map(|f| {
            stage
                .children(&f)
                .into_iter()
                .filter(|p| {
                    stage
                        .prim(p)
                        .is_some_and(|v| v.is_a(&PrimType::MOTION_TRACKER))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn setting(stage: &Stage, tracker: &PrimPath, key: &str) -> Option<Value> {
    query::static_value(&stage.prim(tracker)?, key)
}

pub fn mode(stage: &Stage, tracker: &PrimPath) -> String {
    setting(stage, tracker, "track_type")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn count(stage: &Stage, tracker: &PrimPath) -> usize {
    match mode(stage, tracker).as_str() {
        "parallel" => 3,
        "perspective" => 4,
        _ if ["rotation", "scale"]
            .iter()
            .any(|k| setting(stage, tracker, k) == Some(Value::Bool(true))) =>
        {
            2
        }
        _ => 1,
    }
}

pub fn points(stage: &Stage, tracker: &PrimPath) -> Vec<PrimPath> {
    let built_in = (1..=count(stage, tracker))
        .filter_map(|n| tracker.child(&format!("point_{n}")).ok())
        .filter(|p| stage.has_prim(p));
    let mut extra: Vec<_> = stage
        .children(tracker)
        .into_iter()
        .filter_map(|p| {
            let number = p
                .as_str()
                .rsplit('/')
                .next()?
                .strip_prefix("custom_point_")?
                .parse::<usize>()
                .ok()?;
            Some((number, p))
        })
        .collect();
    extra.sort_unstable_by_key(|(n, _)| *n);
    built_in.chain(extra.into_iter().map(|(_, p)| p)).collect()
}

pub fn vector_at(stage: &Stage, point: &PrimPath, key: &str, comp_time: Time) -> Option<[f64; 2]> {
    match bae_anim::evaluate_attribute(stage, point, key, comp_time)
        .ok()
        .or_else(|| setting(stage, point, key))?
    {
        Value::Vec2(v) => Some(v),
        _ => None,
    }
}

pub fn trackable_property(stage: &Stage, path: &PrimPath, attr: &str) -> bool {
    if query::is_layer(stage, path) {
        return matches!(
            attr,
            "transform:position" | "transform:rotation" | "transform:scale"
        );
    }
    attr.starts_with("param:")
        && stage.prim(path).is_some_and(|p| p.is_a(&PrimType::EFFECT))
        && query::attr_spec(stage, path, attr).is_some_and(|a| {
            matches!(a.value, Some(Value::Vec2(_)))
                || a.keyframes
                    .values()
                    .any(|k| matches!(k.value, Value::Vec2(_)))
        })
}

fn require_tracker(stage: &Stage, tracker: &PrimPath) -> Result<(), CommandError> {
    if !stage
        .prim(tracker)
        .is_some_and(|v| v.is_a(&PrimType::MOTION_TRACKER))
    {
        return Err(fail("expected a motion tracker"));
    }
    ensure_unlocked(stage, [tracker])
}

/// Half the width and height of the rectangle multi-point trackers start on.
const POINT_SPREAD: [f64; 2] = [32.0, 24.0];

fn sync_points(
    tx: &mut Transaction<'_>,
    tracker: &PrimPath,
    reset: bool,
) -> Result<(), CommandError> {
    let Some(Value::Vec2(origin)) = setting(tx.stage(), tracker, "origin") else {
        return Err(fail("missing tracker origin"));
    };
    let n = count(tx.stage(), tracker);
    for i in 0..if reset { 4 } else { n } {
        let p = child(tracker, &format!("point_{}", i + 1))?;
        if !tx.stage().has_prim(&p) {
            if i >= n {
                continue;
            }
            create(tx, &p, PrimType::TRACK_POINT, None)?;
            let name = if crate::names::chinese() {
                "跟踪点"
            } else {
                "Track Point"
            };
            set(tx, &p, "name", format!("{name} {}", i + 1))?;
        } else if !reset {
            continue;
        }
        // One point sits on the origin; more points take the corners of a
        // fixed rectangle around it (1 top left, 2 top right, 3 bottom left,
        // 4 bottom right), wherever the origin is.
        let [hx, hy] = POINT_SPREAD;
        let center = match i {
            0 if n <= 1 => origin,
            0 => [origin[0] - hx, origin[1] - hy],
            1 => [origin[0] + hx, origin[1] - hy],
            2 => [origin[0] - hx, origin[1] + hy],
            _ => [origin[0] + hx, origin[1] + hy],
        };
        initialize_point(tx, &p, center)?;
    }
    if reset {
        for point in points(tx.stage(), tracker) {
            if point
                .as_str()
                .rsplit('/')
                .next()
                .is_some_and(|name| name.starts_with("custom_point_"))
            {
                initialize_point(tx, &point, origin)?;
            }
        }
    }
    Ok(())
}

fn initialize_point(
    tx: &mut Transaction<'_>,
    point: &PrimPath,
    center: [f64; 2],
) -> Result<(), CommandError> {
    for (key, _) in PROPERTIES {
        let value = match key {
            "feature_center" | "attach_point" => Value::Vec2(center),
            _ => bae_model::schema::attr_def(&PrimType::TRACK_POINT, key)
                .and_then(|d| d.default.to_value())
                .expect("track point schema"),
        };
        write_attr(tx, point, key, AttrSpec::with_value(value))?;
    }
    Ok(())
}

#[derive(Deserialize, JsonSchema)]
pub struct AddPointParams {
    pub tracker: PrimPath,
}

pub struct AddPoint;
impl Command for AddPoint {
    type Params = AddPointParams;
    type Output = PrimPath;
    const ID: &'static str = "tracker.add_point";
    const TITLE: &'static str = "New Track Point";
    const DESCRIPTION: &'static str = "Add an independently analyzed point to the current tracker.";
    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<PrimPath, CommandError> {
        require_tracker(tx.stage(), &p.tracker)?;
        let Some(Value::Vec2(center)) = setting(tx.stage(), &p.tracker, "origin") else {
            return Err(fail("missing tracker origin"));
        };
        let number = (1..)
            .find(|n| {
                !tx.stage().has_prim(
                    &p.tracker
                        .child(&format!("custom_point_{n}"))
                        .expect("point name"),
                )
            })
            .expect("unbounded");
        let point = child(&p.tracker, &format!("custom_point_{number}"))?;
        let name = if crate::names::chinese() {
            "跟踪点"
        } else {
            "Track Point"
        };
        let index = points(tx.stage(), &p.tracker).len() + 1;
        create(tx, &point, PrimType::TRACK_POINT, None)?;
        set(tx, &point, "name", format!("{name} {index}"))?;
        initialize_point(tx, &point, center)?;
        Ok(point)
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct CreateParams {
    pub layer: PrimPath,
    pub center: [f64; 2],
    #[serde(default)]
    pub stabilize: bool,
    pub property_target: Option<(PrimPath, String)>,
}
pub struct CreateTracker;
impl Command for CreateTracker {
    type Params = CreateParams;
    type Output = PrimPath;
    const ID: &'static str = "tracker.create";
    const TITLE: &'static str = "Track Motion";
    const DESCRIPTION: &'static str = "Create a motion tracker on a source layer.";
    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<PrimPath, CommandError> {
        if !query::is_layer(tx.stage(), &p.layer) {
            return Err(fail("expected a source layer"));
        }
        ensure_unlocked(tx.stage(), [&p.layer])?;
        let folder = child(&p.layer, FOLDER)?;
        ensure_folder(tx, &folder)?;
        let (i, path) = (1..)
            .find_map(|i| {
                let path = folder.child(&format!("tracker_{i}")).ok()?;
                (!tx.stage().has_prim(&path)).then_some((i, path))
            })
            .expect("unbounded");
        create(tx, &path, PrimType::MOTION_TRACKER, None)?;
        let label = if crate::names::chinese() {
            "跟踪器"
        } else {
            "Tracker"
        };
        set(tx, &path, "name", format!("{label} {i}"))?;
        set(tx, &path, "origin", Value::Vec2(p.center))?;
        let target = query::owning_comp(tx.stage(), &p.layer)
            .and_then(|comp| {
                query::comp_layers(tx.stage(), &comp)
                    .into_iter()
                    .find(|l| l != &p.layer && !query::is_locked(tx.stage(), l))
            })
            .unwrap_or_else(|| p.layer.clone());
        set(tx, &path, "target", Value::PrimRef(target))?;
        if let Some((dest, attr)) = p.property_target {
            let layer = query::owning_layer(tx.stage(), &dest)
                .ok_or_else(|| fail("expected a target layer"))?;
            ensure_unlocked(tx.stage(), [&layer])?;
            if query::owning_comp(tx.stage(), &layer) != query::owning_comp(tx.stage(), &p.layer) {
                return Err(fail("target is in another composition"));
            }
            let transform = dest == layer;
            if !trackable_property(tx.stage(), &dest, &attr) {
                return Err(fail("expected a trackable property"));
            }
            set(tx, &path, "target", Value::PrimRef(dest))?;
            for key in ["position", "rotation", "scale"] {
                set(
                    tx,
                    &path,
                    key,
                    Value::Bool(if transform {
                        attr.strip_prefix("transform:") == Some(key)
                    } else {
                        key == "position"
                    }),
                )?;
            }
            if !transform {
                set(tx, &path, "target_property", Value::String(attr))?;
            }
        }
        if p.stabilize {
            set(tx, &path, "track_type", Value::Enum("stabilize".into()))?;
        }
        sync_points(tx, &path, false)?;
        Ok(path)
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct ConfigureParams {
    pub tracker: PrimPath,
    pub values: BTreeMap<String, Value>,
}
pub struct ConfigureTracker;
impl Command for ConfigureTracker {
    type Params = ConfigureParams;
    type Output = ();
    const ID: &'static str = "tracker.configure";
    const TITLE: &'static str = "Edit Motion Tracker";
    const DESCRIPTION: &'static str = "Change tracking mode, options or motion target.";
    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        require_tracker(tx.stage(), &p.tracker)?;
        for (key, val) in p.values {
            if key == "origin"
                || bae_model::schema::attr_def(&PrimType::MOTION_TRACKER, &key).is_none()
            {
                return Err(fail("unknown tracker option"));
            }
            if key == "threshold" && !val.as_f64().is_some_and(|n| (0.0..=100.0).contains(&n)) {
                return Err(fail("threshold must be a percentage"));
            }
            set(tx, &p.tracker, &key, val)?;
        }
        if matches!(
            mode(tx.stage(), &p.tracker).as_str(),
            "parallel" | "perspective"
        ) && let Some(Value::PrimRef(target)) = setting(tx.stage(), &p.tracker, "target")
            && let Some(layer) = crate::effect::layer_of_effect(&target)
        {
            set(tx, &p.tracker, "target", Value::PrimRef(layer))?;
            set(tx, &p.tracker, "target_property", String::new())?;
        }
        sync_points(tx, &p.tracker, false)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Sample {
    pub center: [f64; 2],
    pub confidence: f64,
    pub attach: Option<[f64; 2]>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Frame {
    pub time: Time,
    pub points: Vec<Sample>,
}
#[derive(Deserialize, JsonSchema)]
pub struct RecordParams {
    pub tracker: PrimPath,
    pub frames: Vec<Frame>,
}
pub(crate) fn key(
    tx: &mut Transaction<'_>,
    path: &PrimPath,
    attr: &str,
    time: Time,
    value: Value,
) -> Result<(), CommandError> {
    tx.ensure_spec(path)?;
    tx.apply(Op::SetKeyframe {
        path: path.clone(),
        attr: attr.into(),
        time,
        keyframe: Keyframe::linear(value),
    })?;
    Ok(())
}

pub struct RecordTrack;
impl Command for RecordTrack {
    type Params = RecordParams;
    type Output = ();
    const ID: &'static str = "tracker.record";
    const TITLE: &'static str = "Analyze Motion";
    const DESCRIPTION: &'static str = "Record all frames of a motion analysis in one undo step.";
    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        require_tracker(tx.stage(), &p.tracker)?;
        let pts = points(tx.stage(), &p.tracker);
        for f in p.frames {
            if pts.len() != f.points.len() {
                return Err(fail("track point count changed"));
            }
            let t = query::to_key_time(tx.stage(), &p.tracker, f.time);
            for (pt, s) in pts.iter().zip(f.points) {
                key(tx, pt, "feature_center", t, Value::Vec2(s.center))?;
                key(tx, pt, "confidence", t, Value::Float(s.confidence))?;
                if let Some(a) = s.attach {
                    key(tx, pt, "attach_point", t, Value::Vec2(a))?;
                } else if query::attr_spec(tx.stage(), pt, "attach_point")
                    .is_some_and(|a| a.keyframes.contains_key(&t))
                {
                    tx.apply(Op::RemoveKeyframe {
                        path: pt.clone(),
                        attr: "attach_point".into(),
                        time: t,
                    })?;
                }
            }
        }
        Ok(())
    }
}
#[derive(Deserialize, JsonSchema)]
pub struct TrackerParams {
    pub tracker: PrimPath,
}
pub struct ResetTracker;
impl Command for ResetTracker {
    type Params = TrackerParams;
    type Output = ();
    const ID: &'static str = "tracker.reset";
    const TITLE: &'static str = "Reset Tracker";
    const DESCRIPTION: &'static str = "Restore the regions and clear all tracking samples.";
    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        require_tracker(tx.stage(), &p.tracker)?;
        sync_points(tx, &p.tracker, true)
    }
}
