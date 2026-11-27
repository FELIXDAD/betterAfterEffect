//! Sparse reconstruction in AE coordinates (+X right, +Y down, +Z forward).
mod geometry;
mod math;
use crate::{
    query,
    util::{ensure_unlocked, fail, set},
};
use bae_anim::glam::{DQuat, DVec2, DVec3};
use bae_model::{Command, CommandError, PrimPath, Stage, Time, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn attr(stage: &Stage, path: &PrimPath, key: &str) -> Option<Value> {
    stage.prim(path).and_then(|p| query::static_value(&p, key))
}
pub const MATCH_NAME: &str = "ADBE 3D Tracker";
pub const ERROR: &str = "tracker:average_error";
pub const SOLUTION: &str = "tracker:solution";
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Settings {
    pub shot: i64,
    pub angle: f64,
    pub method: i64,
    pub detailed: bool,
}
impl Settings {
    pub fn read(stage: &Stage, fx: &PrimPath) -> Self {
        let get = |id| attr(stage, fx, &bae_model::effect::param_attr(id));
        Self {
            shot: get(403)
                .and_then(|v| v.as_f64().map(|n| n as i64))
                .unwrap_or(1),
            angle: get(414).and_then(|v| v.as_f64()).unwrap_or(0.),
            method: get(410)
                .and_then(|v| v.as_f64().map(|n| n as i64))
                .unwrap_or(1),
            detailed: get(36) == Some(Value::Bool(true)),
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Sample {
    pub id: u32,
    pub pixel: [f32; 2],
}
impl Sample {
    pub fn xy(self) -> DVec2 {
        DVec2::new(self.pixel[0] as f64, self.pixel[1] as f64)
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Pose {
    pub center: [f64; 3],
    pub rotation: [f64; 4],
    pub zoom: f64,
}
impl Pose {
    fn new(zoom: f64) -> Self {
        Self {
            center: [0.; 3],
            rotation: DQuat::IDENTITY.to_array(),
            zoom,
        }
    }
    pub fn quat(self) -> DQuat {
        DQuat::from_array(self.rotation)
    }
    pub fn position(self) -> DVec3 {
        DVec3::from_array(self.center)
    }
    /// Pixel relative to the optical centre.
    pub fn pixel(self, world: DVec3) -> Option<DVec2> {
        let p = self.quat().conjugate() * (world - self.position());
        (p.z > 1e-6).then(|| p.truncate() * (self.zoom / p.z))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Landmark {
    pub world: [f64; 3],
    pub error: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Frame {
    pub time: Time,
    pub camera: Pose,
    pub samples: Vec<Sample>,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Solution {
    pub settings: Settings,
    pub size: [u32; 2],
    pub frames: Vec<Frame>,
    pub points: BTreeMap<u32, Landmark>,
    pub error: f64,
}

const INSUFFICIENT: &str = "Insufficient matching track points";
pub fn solve(
    size: [u32; 2],
    frames: Vec<(Time, Vec<Sample>)>,
    settings: Settings,
    cancelled: impl Fn() -> bool,
) -> Result<Solution, String> {
    if frames.len() < 2 || size.contains(&0) {
        return Err(INSUFFICIENT.into());
    }
    let check = || {
        if cancelled() {
            Err("Tracking stopped".to_string())
        } else {
            Ok(())
        }
    };
    let width = f64::from(size[0]);
    let variable = settings.shot == 2;
    let specified = if settings.shot == 4 {
        if !(0. < settings.angle && settings.angle < 180.) {
            return Err("Specify a horizontal angle of view between 0 and 180 degrees".into());
        }
        Some(crate::camera::lens::zoom_from_angle(settings.angle, width))
    } else {
        None
    };
    let first: BTreeMap<_, _> = frames[0].1.iter().map(|s| (s.id, s.xy())).collect();
    let mut baseline = 0.;
    let mut pairs = Vec::new();
    let mut seed = 1;
    for (i, (_, samples)) in frames.iter().enumerate().skip(1).take(45) {
        check()?;
        let candidate: Vec<_> = samples
            .iter()
            .filter_map(|s| Some((s.id, *first.get(&s.id)?, s.xy())))
            .collect();
        if candidate.len() < 8 {
            continue;
        }
        let mut movement: Vec<_> = candidate.iter().map(|(_, a, b)| a.distance(*b)).collect();
        movement.sort_by(f64::total_cmp);
        let score =
            movement[movement.len() / 2] * ((candidate.len() as f64) / (first.len() as f64)).sqrt();
        if pairs.is_empty() || score > baseline {
            baseline = score;
            seed = i;
            pairs = candidate;
        }
    }
    if pairs.len() < 8 {
        return Err(INSUFFICIENT.into());
    }
    let mut zoom = specified.unwrap_or(width);
    let mut pan = settings.method == 5;
    let mut flat = settings.method == 4;
    let endpoints: Vec<_> = pairs.iter().map(|&(_, a, b)| (a, b)).collect();
    let mut scene = BTreeMap::new();
    if !pan
        && !flat
        && let Some((pose, f)) = geometry::bootstrap(&endpoints, width, specified)
    {
        zoom = f;
        for &(id, a, b) in &pairs {
            if let Some(p) = geometry::triangulate([(Pose::new(f), a), (pose, b)].into_iter())
                && pose.pixel(p).is_some_and(|v| v.distance(b) < 3.)
                && Pose::new(f).pixel(p).is_some_and(|v| v.distance(a) < 3.)
            {
                scene.insert(id, p);
            }
        }
    }
    if scene.len() < 8 {
        if settings.method == 3 {
            return Err("Could not recover camera depth".into());
        }
        // A panorama has no measurable depth. Test the fixed-centre model before allowing translation.
        scene = first
            .iter()
            .map(|(&id, &xy)| (id, xy.extend(zoom)))
            .collect();
        if settings.method == 1 {
            let guess = geometry::refine(Pose::new(zoom), &frames[seed].1, &scene, true, variable);
            let inliers = guess.map_or(0, |c| {
                pairs
                    .iter()
                    .filter(|(id, _, b)| c.pixel(scene[id]).is_some_and(|p| p.distance(*b) < 1.5))
                    .count()
            });
            pan = inliers * 5 >= pairs.len() * 4;
            flat = !pan;
        }
    }
    let mut cameras = vec![Pose::new(zoom); frames.len()];
    let mut by_point: BTreeMap<u32, Vec<(usize, DVec2)>> = BTreeMap::new();
    for (i, (_, samples)) in frames.iter().enumerate() {
        for &s in samples {
            by_point.entry(s.id).or_default().push((i, s.xy()));
        }
    }
    // Incremental resection; triangulate new tracks once an observed baseline exists.
    for (i, (_, samples)) in frames.iter().enumerate().skip(1) {
        check()?;
        cameras[i] =
            geometry::refine(cameras[i - 1], samples, &scene, pan, variable).ok_or(INSUFFICIENT)?;
        for s in samples {
            if scene.contains_key(&s.id) {
                continue;
            }
            let &(start, xy) = by_point[&s.id].first().unwrap();
            let c = cameras[start];
            let point = if pan || flat {
                let ray = c.quat() * (xy / c.zoom).extend(1.);
                let depth = (zoom - c.position().z) / ray.z;
                (depth > 0.).then(|| c.position() + ray * depth)
            } else if i > start && c.position().distance(cameras[i].position()) > 0.015 {
                geometry::triangulate([(c, xy), (cameras[i], s.xy())].into_iter())
            } else {
                None
            };
            if let Some(p) = point
                && cameras[i]
                    .pixel(p)
                    .is_some_and(|v| v.distance(s.xy()) < 2.5)
            {
                scene.insert(s.id, p);
            }
        }
    }
    // Block coordinate bundle adjustment: solve 3D points against all their rays, then
    // re-estimate each camera against the same world. Frame zero fixes the gauge.
    for _ in 0..if settings.detailed { 10 } else { 5 } {
        check()?;
        if !pan {
            for (&id, obs) in &by_point {
                if let Some(p) = geometry::triangulate(obs.iter().map(|&(i, xy)| (cameras[i], xy)))
                    && p.is_finite()
                    && cameras[obs[0].0].pixel(p).is_some()
                {
                    scene.insert(id, p);
                }
            }
            if flat {
                geometry::flatten(&mut scene);
            }
        }
        for (i, (_, samples)) in frames.iter().enumerate().skip(1) {
            check()?;
            cameras[i] =
                geometry::refine(cameras[i], samples, &scene, pan, variable).ok_or(INSUFFICIENT)?;
        }
        if specified.is_none() && !variable && !pan {
            let (mut num, mut den) = (0., 0.);
            for (i, (_, samples)) in frames.iter().enumerate() {
                for s in samples {
                    if let Some(x) = scene.get(&s.id).and_then(|&p| cameras[i].pixel(p)) {
                        let u = x / zoom;
                        let w = 1. / (1. + x.distance(s.xy()));
                        num += w * u.dot(s.xy());
                        den += w * u.length_squared();
                    }
                }
            }
            if den > 0. {
                zoom = (num / den).clamp(width * 0.1, width * 10.);
                for c in &mut cameras {
                    c.zoom = zoom;
                }
            }
        }
    }
    let mut points = BTreeMap::new();
    let (mut sum, mut count) = (0., 0);
    for (id, p) in scene {
        let obs = &by_point[&id];
        let err: Option<f64> = obs
            .iter()
            .map(|&(i, xy)| cameras[i].pixel(p).map(|v| v.distance(xy)))
            .sum();
        if let Some(e) = err
            && obs.len() >= 2
            && e / (obs.len() as f64) < 4.
        {
            sum += e;
            count += obs.len();
            points.insert(
                id,
                Landmark {
                    world: p.to_array(),
                    error: e / (obs.len() as f64),
                },
            );
        }
    }
    if points.len() < 6
        || frames
            .iter()
            .any(|(_, s)| s.iter().filter(|p| points.contains_key(&p.id)).count() < 6)
    {
        return Err(INSUFFICIENT.into());
    }
    Ok(Solution {
        settings,
        size,
        points,
        error: sum / count as f64,
        frames: frames
            .into_iter()
            .zip(cameras)
            .map(|((time, samples), camera)| Frame {
                time,
                samples,
                camera,
            })
            .collect(),
    })
}
pub struct Record;
#[derive(Deserialize, JsonSchema)]
pub struct Recording {
    pub effect: PrimPath,
    pub solution: Solution,
}
impl Command for Record {
    type Params = Recording;
    type Output = ();
    const ID: &'static str = "camera_tracker.record";
    const TITLE: &'static str = "Solve Camera";
    const DESCRIPTION: &'static str =
        "Store the reconstructed camera and its measured track points.";
    fn run(&self, tx: &mut Transaction<'_>, p: Recording) -> Result<(), CommandError> {
        let layer = crate::effect::layer_of_effect(&p.effect)
            .ok_or_else(|| fail("Missing camera tracker"))?;
        ensure_unlocked(tx.stage(), &[layer])?;
        if attr(tx.stage(), &p.effect, "match_name")
            .as_ref()
            .and_then(Value::as_str)
            != Some(MATCH_NAME)
            || Settings::read(tx.stage(), &p.effect) != p.solution.settings
        {
            return Err(fail("Camera tracker changed"));
        }
        let data = serde_json::to_vec(&p.solution).map_err(|e| fail(e.to_string()))?;
        set(tx, &p.effect, ERROR, Value::Float(p.solution.error))?;
        set(tx, &p.effect, SOLUTION, Value::Bytes(data))
    }
}
