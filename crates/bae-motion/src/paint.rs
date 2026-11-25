//! Paint strokes (AE's Brush, Clone Stamp and Eraser in the Layer panel). Each
//! stroke is a `PaintStroke` prim under the layer's Paint effect (`ADBE Paint`),
//! which the first stroke adds at the bottom of the effect stack. A stroke
//! keeps its own copy of the brush settings, all keyframable like AE's stroke
//! properties. One stroke is one undo step.

use bae_model::{
    BezierPath, Command, CommandError, Keyframe, Op, PrimPath, PrimType, Time, Transaction, Value,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::effect::{EFFECTS_FOLDER, effects_of};
use crate::query;
use crate::util::{child, create, ensure_unlocked, fail, set};

/// The Paint effect's matchName.
pub const PAINT: &str = "ADBE Paint";

/// How long a stroke stays (AE's Paint panel Duration).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StrokeDuration {
    /// From where it starts to the layer's end.
    #[default]
    Constant,
    /// Like Constant, and End animates from 0 to 100 % over the time drawing took.
    WriteOn,
    /// One frame.
    SingleFrame,
    /// `frames` frames.
    Custom,
}

fn brush() -> String {
    "brush".to_owned()
}
fn white() -> [f64; 4] {
    [1.0, 1.0, 1.0, 1.0]
}
fn hundred() -> f64 {
    100.0
}

#[derive(Deserialize, JsonSchema)]
pub struct AddStrokeParams {
    pub layer: PrimPath,
    /// `brush`, `clone` or `eraser`.
    #[serde(default = "brush")]
    pub tool: String,
    /// The pointer's path in layer pixels, in drawing order.
    pub points: Vec<[f64; 2]>,
    /// Layer time the stroke starts at.
    pub time: Time,
    #[serde(default)]
    pub duration: StrokeDuration,
    /// Custom duration in frames.
    #[serde(default)]
    pub frames: u32,
    /// How long drawing took (Write On).
    #[serde(default)]
    pub draw_time: Time,
    #[serde(default = "white")]
    pub color: [f64; 4],
    #[serde(default)]
    pub diameter: Option<f64>,
    #[serde(default)]
    pub angle: f64,
    #[serde(default = "hundred")]
    pub hardness: f64,
    #[serde(default = "hundred")]
    pub roundness: f64,
    #[serde(default)]
    pub spacing: Option<f64>,
    #[serde(default = "hundred")]
    pub opacity: f64,
    #[serde(default = "hundred")]
    pub flow: f64,
    /// `rgba`, `rgb` or `alpha`.
    #[serde(default)]
    pub channels: Option<String>,
    /// Blend mode (a layer blend mode token).
    #[serde(default)]
    pub mode: Option<String>,
    /// Eraser: `layer_and_paint`, `paint` or `last_stroke`.
    #[serde(default)]
    pub erase: Option<String>,
    /// Clone Stamp: the source layer (none: this layer).
    #[serde(default)]
    pub clone_source: Option<PrimPath>,
    /// Clone Stamp: source point minus the stroke's first point.
    #[serde(default)]
    pub clone_offset: [f64; 2],
    /// Clone Stamp: source time offset.
    #[serde(default)]
    pub clone_time_shift: Time,
}

#[derive(Serialize, JsonSchema)]
pub struct AddStrokeOutput {
    pub effect: PrimPath,
    pub stroke: PrimPath,
}

pub struct AddStroke;

impl Command for AddStroke {
    type Params = AddStrokeParams;
    type Output = AddStrokeOutput;
    const ID: &'static str = "paint.add_stroke";
    const TITLE: &'static str = "Paint Stroke";
    const DESCRIPTION: &'static str = "Add a Brush, Clone Stamp or Eraser stroke to a layer \
        (AE: paint tools in the Layer panel). The first stroke adds the Paint effect.";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: AddStrokeParams,
    ) -> Result<AddStrokeOutput, CommandError> {
        let layer = &p.layer;
        if !query::is_layer(tx.stage(), layer) {
            return Err(fail(format!("{layer} is not a layer")));
        }
        ensure_unlocked(tx.stage(), std::slice::from_ref(layer))?;
        if !matches!(p.tool.as_str(), "brush" | "clone" | "eraser") {
            return Err(fail(format!("unknown paint tool `{}`", p.tool)));
        }
        let Some(first) = p.points.first() else {
            return Err(fail("a stroke needs at least one point"));
        };
        let effect = paint_effect(tx, layer)?;
        let base = match p.tool.as_str() {
            "clone" => "Clone",
            "eraser" => "Eraser",
            _ => "Brush",
        };
        let n = (1..)
            .find(|i| {
                child(&effect, &format!("{}_{i}", base.to_lowercase()))
                    .is_ok_and(|c| !tx.stage().has_prim(&c))
            })
            .unwrap_or(1);
        let stroke = child(&effect, &format!("{}_{n}", base.to_lowercase()))?;
        create(tx, &stroke, PrimType::PAINT_STROKE, None)?;
        let frame = query::owning_comp(tx.stage(), layer)
            .and_then(|c| query::comp_info(tx.stage(), &c))
            .map_or(Time::from_seconds(1.0 / 30.0), |i| i.frame_duration());
        let timing = query::layer_timing(tx.stage(), layer);
        let layer_end = timing.out_point.max(timing.in_point) - timing.start_time;
        let out_point = match p.duration {
            StrokeDuration::Constant | StrokeDuration::WriteOn => layer_end.max(p.time + frame),
            StrokeDuration::SingleFrame => p.time + frame,
            StrokeDuration::Custom => p.time + Time(frame.0 * i64::from(p.frames.max(1))),
        };
        let n_points = p.points.len();
        let path = BezierPath {
            vertices: p.points.clone(),
            in_tangents: vec![[0.0, 0.0]; n_points],
            out_tangents: vec![[0.0, 0.0]; n_points],
            closed: false,
            ..BezierPath::default()
        };
        let diameter = p.diameter.unwrap_or(10.0);
        let values = [
            ("name", Value::String(format!("{base} {n}"))),
            ("tool", Value::Enum(p.tool.clone())),
            ("path", Value::Bezier(path)),
            ("color", Value::Color(p.color)),
            ("diameter", Value::Float(diameter)),
            ("angle", Value::Float(p.angle)),
            ("hardness", Value::Float(p.hardness)),
            ("roundness", Value::Float(p.roundness)),
            ("spacing", Value::Float(p.spacing.unwrap_or(25.0))),
            ("opacity", Value::Float(p.opacity)),
            ("flow", Value::Float(p.flow)),
            ("in_point", Value::Time(p.time)),
            ("out_point", Value::Time(out_point)),
        ];
        for (attr, v) in values {
            set(tx, &stroke, attr, v)?;
        }
        for (attr, v) in [
            ("channels", &p.channels),
            ("mode", &p.mode),
            ("erase", &p.erase),
        ] {
            if let Some(v) = v {
                set(tx, &stroke, attr, Value::Enum(v.clone()))?;
            }
        }
        if p.tool == "clone" {
            let src = [first[0] + p.clone_offset[0], first[1] + p.clone_offset[1]];
            set(tx, &stroke, "clone:position", Value::Vec2(src))?;
            set(
                tx,
                &stroke,
                "clone:time_shift",
                Value::Time(p.clone_time_shift),
            )?;
            if let Some(source) = &p.clone_source {
                set(tx, &stroke, "clone:source", Value::PrimRef(source.clone()))?;
            }
        }
        if p.duration == StrokeDuration::WriteOn {
            tx.ensure_spec(&stroke)?;
            let draw = p.draw_time.max(frame);
            for (t, v) in [(p.time, 0.0), (p.time + draw, 100.0)] {
                tx.apply(Op::SetKeyframe {
                    path: stroke.clone(),
                    attr: "end".to_owned(),
                    time: t,
                    keyframe: Keyframe::linear(Value::Float(v)),
                })?;
            }
        }
        Ok(AddStrokeOutput { effect, stroke })
    }
}

/// The layer's Paint effect, added at the bottom of its stack if missing.
fn paint_effect(tx: &mut Transaction<'_>, layer: &PrimPath) -> Result<PrimPath, CommandError> {
    let existing = effects_of(tx.stage(), layer).into_iter().find(|e| {
        tx.stage()
            .prim(e)
            .and_then(|v| {
                v.value("match_name")
                    .and_then(|m| m.as_str().map(str::to_owned))
            })
            .as_deref()
            == Some(PAINT)
    });
    if let Some(e) = existing {
        return Ok(e);
    }
    let folder = child(layer, EFFECTS_FOLDER)?;
    if !tx.stage().has_prim(&folder) {
        create(tx, &folder, PrimType::FOLDER, None)?;
    }
    let name = query::unique_child_name(tx.stage(), &folder, "Paint");
    let path = child(&folder, &name)?;
    create(tx, &path, PrimType::EFFECT, None)?;
    set(tx, &path, "match_name", Value::String(PAINT.to_owned()))?;
    set(tx, &path, "name", Value::String("Paint".to_owned()))?;
    // Paint on Transparent: off.
    set(
        tx,
        &path,
        &bae_model::effect::param_attr(1),
        Value::Bool(false),
    )?;
    Ok(path)
}

/// The strokes under a Paint effect, oldest first.
pub fn strokes_of(stage: &bae_model::Stage, effect: &PrimPath) -> Vec<PrimPath> {
    stage
        .children(effect)
        .into_iter()
        .filter(|c| {
            stage
                .prim(c)
                .is_some_and(|v| v.is_a(&PrimType::PAINT_STROKE))
        })
        .collect()
}
