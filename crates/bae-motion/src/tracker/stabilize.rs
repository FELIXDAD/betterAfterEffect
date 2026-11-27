use super::*;
use bae_model::effect::{
    param_attr,
    stabilizer::{CELLS, MATCH_NAME, MOTION, QUALITY},
};

#[derive(Deserialize, JsonSchema)]
pub struct RecordParams {
    effect: PrimPath,
    quality: i64,
    frames: Vec<Frame>,
}
pub struct RecordStabilization;
impl Command for RecordStabilization {
    type Params = RecordParams;
    type Output = ();
    const ID: &'static str = "tracker.record_stabilization";
    const TITLE: &'static str = "Analyze Stabilization";
    const DESCRIPTION: &'static str =
        "Replace the effect's measured camera path in one transaction.";
    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        let layer = crate::effect::layer_of_effect(&p.effect)
            .ok_or_else(|| fail("Missing effect layer"))?;
        ensure_unlocked(tx.stage(), &[layer])?;
        if tx
            .stage()
            .prim(&p.effect)
            .and_then(|e| e.value("match_name"))
            .and_then(Value::as_str)
            != Some(MATCH_NAME)
            || p.frames.is_empty()
        {
            return Err(fail("Missing stabilizer analysis"));
        }
        let mut track = AttrSpec::default();
        for frame in p.frames {
            if frame.points.len() != CELLS
                || frame
                    .points
                    .iter()
                    .flat_map(|s| s.center)
                    .any(|x| !x.is_finite())
            {
                return Err(fail("Invalid stabilization grid"));
            }
            let time = query::to_key_time(tx.stage(), &p.effect, frame.time);
            let data = frame.points.into_iter().flat_map(|s| s.center).collect();
            track
                .keyframes
                .insert(time, Keyframe::linear(Value::FloatList(data)));
        }
        write_attr(tx, &p.effect, &param_attr(MOTION), track)?;
        set(tx, &p.effect, &param_attr(QUALITY), Value::Int(p.quality))
    }
}
