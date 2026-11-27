//! Create all tracked layers and their animation in a single document transaction.
use super::{Frame, SOLUTION, Solution};
use crate::{
    CreateLayer, LayersOutput, query,
    util::{ensure_unlocked, fail, set, write_attr},
};
use bae_anim::glam::{DMat3, DQuat, DVec3, EulerRot};
use bae_model::{
    AttrSpec, Command, CommandError, Keyframe, PrimPath, Stage, Time, Transaction, Value,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeSet;

pub(super) const GROUND: &str = "tracker:ground";
pub(super) const CAMERA: &str = "tracker:camera";
pub fn solution(stage: &Stage, fx: &PrimPath) -> Result<Solution, CommandError> {
    let Some(Value::Bytes(data)) =
        query::attr_spec(stage, fx, SOLUTION).and_then(|s| s.value.as_ref())
    else {
        return Err(fail("Camera has not been solved"));
    };
    serde_json::from_slice(data).map_err(|e| fail(e.to_string()))
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Ground,
    Text,
    Solid,
    Null,
    Shadow,
    Delete,
}
#[derive(Deserialize, JsonSchema)]
pub struct Params {
    pub effect: PrimPath,
    pub points: BTreeSet<u32>,
    pub time: Time,
    pub action: Action,
}

#[derive(Clone, Copy)]
struct Plane {
    origin: DVec3,
    basis: DQuat,
}
fn plane(solution: &Solution, ids: &BTreeSet<u32>, frame: &Frame) -> Result<Plane, CommandError> {
    let pts: Vec<_> = ids
        .iter()
        .filter_map(|id| solution.points.get(id).map(|p| DVec3::from_array(p.world)))
        .collect();
    if pts.is_empty() {
        return Err(fail("Select solved track points"));
    }
    let origin = pts.iter().sum::<DVec3>() / pts.len() as f64;
    let camera = frame.camera.quat();
    let mut normal = camera * DVec3::Z;
    if pts.len() >= 3 {
        let mut covariance = DMat3::ZERO;
        for p in pts {
            let d = p - origin;
            covariance += DMat3::from_cols(d * d.x, d * d.y, d * d.z);
        }
        let (v, q) = super::math::eigen(covariance.to_cols_array_2d());
        let mut order = [0, 1, 2];
        order.sort_by(|&a, &b| v[a].total_cmp(&v[b]));
        if v[order[1]] > 1e-9 * v[order[2]].max(1e-10) {
            let k = order[0];
            normal = DVec3::new(q[0][k], q[1][k], q[2][k]);
            if normal.dot(origin - frame.camera.position()) < 0. {
                normal = -normal;
            }
        }
    }
    let right = camera * DVec3::X;
    let x = (right - normal * normal.dot(right))
        .try_normalize()
        .unwrap_or_else(|| normal.any_orthonormal_vector());
    Ok(Plane {
        origin,
        basis: DQuat::from_mat3(&DMat3::from_cols(x, normal.cross(x), normal)),
    })
}
fn euler(q: DQuat) -> [f64; 3] {
    let (x, y, z) = q.to_euler(EulerRot::XYZ);
    [x, y, z].map(f64::to_degrees)
}
pub struct Attach;
impl Command for Attach {
    type Params = Params;
    type Output = LayersOutput;
    const ID: &'static str = "camera_tracker.attach";
    const TITLE: &'static str = "3D Camera Tracker";
    const DESCRIPTION: &'static str =
        "Set the ground plane or attach content to selected reconstructed points.";
    fn run(&self, tx: &mut Transaction<'_>, p: Params) -> Result<LayersOutput, CommandError> {
        let source = crate::effect::layer_of_effect(&p.effect)
            .ok_or_else(|| fail("Missing tracked layer"))?;
        ensure_unlocked(tx.stage(), [&source])?;
        let comp =
            query::owning_comp(tx.stage(), &source).ok_or_else(|| fail("Missing composition"))?;
        let info =
            query::comp_info(tx.stage(), &comp).ok_or_else(|| fail("Missing composition"))?;
        let mut solve = solution(tx.stage(), &p.effect)?;
        let index = solve
            .frames
            .partition_point(|f| f.time <= p.time)
            .saturating_sub(1);
        let frame = solve
            .frames
            .get(index)
            .ok_or_else(|| fail("Missing solved frame"))?;
        if p.points.is_empty()
            || p.points
                .iter()
                .any(|id| !frame.samples.iter().any(|s| s.id == *id))
        {
            return Err(fail("Select visible track points"));
        }
        if p.action == Action::Delete {
            let across = super::attr(tx.stage(), &p.effect, &bae_model::effect::param_attr(306))
                != Some(Value::Bool(false));
            let time = frame.time;
            for f in &mut solve.frames {
                if across || f.time == time {
                    f.samples.retain(|s| !p.points.contains(&s.id));
                }
            }
            if across {
                solve.points.retain(|id, _| !p.points.contains(id));
            }
            let record = super::Recording::prepare(tx.stage(), &p.effect, &solve)?;
            super::Record.run(tx, record)?;
            set(tx, &p.effect, "tracker:needs_solve", Value::Bool(true))?;
            return Ok(LayersOutput { paths: vec![] });
        }
        let target = plane(&solve, &p.points, frame)?;
        if p.action == Action::Ground {
            let mut data = target.origin.to_array().to_vec();
            data.extend(target.basis.to_array());
            set(tx, &p.effect, GROUND, Value::FloatList(data))?;
            // Objects created after a new ground definition share a new camera.
            set(tx, &p.effect, CAMERA, Value::PrimRef(PrimPath::root()))?;
            return Ok(LayersOutput { paths: vec![] });
        }
        let first = solve.frames[0].camera;
        let mut depths: Vec<_> = solve
            .points
            .values()
            .map(|p| DVec3::from_array(p.world).z)
            .filter(|z| *z > 1e-6)
            .collect();
        depths.sort_by(f64::total_cmp);
        let unit = first.zoom / depths.get(depths.len() / 2).copied().unwrap_or(first.zoom);
        let center = DVec3::new(info.width as f64 * 0.5, info.height as f64 * 0.5, 0.);
        let (rotation, offset) = match super::attr(tx.stage(), &p.effect, GROUND) {
            Some(Value::FloatList(g)) if g.len() == 7 => {
                // The normal points from the camera into the ground: AE's +Y (down).
                let q = DQuat::from_rotation_x(-std::f64::consts::FRAC_PI_2)
                    * DQuat::from_array([g[3], g[4], g[5], g[6]]).conjugate();
                (q, -(q * DVec3::new(g[0], g[1], g[2])) * unit)
            }
            _ => (DQuat::IDENTITY, center - DVec3::Z * first.zoom),
        };
        let pos = |v: DVec3| (rotation * v * unit + offset).to_array();
        let mut paths = vec![];
        let existing = super::attr(tx.stage(), &p.effect, CAMERA)
            .and_then(|v| v.as_prim_ref().cloned())
            .filter(|c| {
                query::layer_kind(tx.stage(), c).as_deref() == Some("camera")
                    && query::owning_comp(tx.stage(), c).as_ref() == Some(&comp)
            });
        if existing.is_none() {
            let cam = create(
                tx,
                json!({"comp":comp,"kind":"camera","name":"3D Tracker Camera","camera":{"camera_type":"one_node","zoom":first.zoom}}),
            )?;
            let mut position = AttrSpec::default();
            let mut orientation = AttrSpec::default();
            let mut zoom = AttrSpec::default();
            let mut previous = euler(rotation * first.quat());
            for f in &solve.frames {
                let t = query::to_key_time(tx.stage(), &cam, f.time);
                position
                    .keyframes
                    .insert(t, Keyframe::linear(Value::Vec3(pos(f.camera.position()))));
                let mut angles = euler(rotation * f.camera.quat());
                for (a, b) in angles.iter_mut().zip(previous) {
                    *a = b + (*a - b + 180.).rem_euclid(360.) - 180.;
                }
                previous = angles;
                orientation
                    .keyframes
                    .insert(t, Keyframe::linear(Value::Vec3(angles)));
                zoom.keyframes
                    .insert(t, Keyframe::linear(Value::Float(f.camera.zoom)));
            }
            write_attr(tx, &cam, "transform:position", position)?;
            write_attr(tx, &cam, "transform:orientation", orientation)?;
            if solve.settings.shot == 2 {
                write_attr(tx, &cam, "camera:zoom", zoom)?;
            }
            set(tx, &p.effect, CAMERA, Value::PrimRef(cam.clone()))?;
            paths.push(cam);
        }
        let target_size = bae_anim::Evaluator::new()
            .evaluate_attribute(
                tx.stage(),
                &p.effect,
                &bae_model::effect::param_attr(409),
                p.time,
            )
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(100.)
            / 100.;
        let size = (f64::from(solve.size[0]) * 0.12 * target_size)
            .round()
            .clamp(1., 32768.) as u32;
        let (kind, name) = match p.action {
            Action::Text => ("text", "Track Text"),
            Action::Solid => ("solid", "Track Solid"),
            Action::Null => ("null", "Track Null"),
            Action::Shadow => ("solid", "Shadow Catcher"),
            _ => unreachable!(),
        };
        let object = create(
            tx,
            json!({"comp":comp,"kind":kind,"name":name,"text":"Text","font_size":size as f64,"size":if p.action==Action::Shadow{solve.size}else{[size,size]},"color":[1.,1.,1.,1.]}),
        )?;
        set(tx, &object, "three_d", Value::Bool(true))?;
        set(
            tx,
            &object,
            "transform:position",
            Value::Vec3(pos(target.origin)),
        )?;
        set(
            tx,
            &object,
            "transform:orientation",
            Value::Vec3(euler(rotation * target.basis)),
        )?;
        if p.action == Action::Shadow {
            set(tx, &object, "material:accepts_shadows", Value::Int(2))?;
            if !query::comp_layers(tx.stage(), &comp)
                .iter()
                .any(|l| query::layer_kind(tx.stage(), l).as_deref() == Some("light"))
            {
                let light = create(
                    tx,
                    json!({"comp":comp,"kind":"light","name":"Shadow Catcher Light","position_3d":pos(frame.camera.position()),"point_of_interest":pos(target.origin),"light":{"light_type":"point","casts_shadows":true}}),
                )?;
                paths.push(light);
            }
        }
        paths.push(object);
        Ok(LayersOutput { paths })
    }
}
fn create(tx: &mut Transaction<'_>, args: serde_json::Value) -> Result<PrimPath, CommandError> {
    let params = serde_json::from_value(args).map_err(|e| fail(e.to_string()))?;
    Ok(CreateLayer.run(tx, params)?.path)
}
