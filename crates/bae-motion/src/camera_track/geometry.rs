use super::{Pose, Sample, math};
use bae_anim::glam::{DMat3, DQuat, DVec2, DVec3};
use std::collections::BTreeMap;

type Pair = (DVec3, DVec3);

fn fundamental(pairs: &[Pair], scale: f64) -> Option<DMat3> {
    if pairs.len() < 8 {
        return None;
    }
    let fit = |points: &[Pair]| {
        let m = math::null_vector(points.iter().map(|&(a, b)| {
            let (x, y, u, v) = (a.x, a.y, b.x, b.y);
            [u * x, u * y, u, v * x, v * y, v, x, y, 1.]
        }));
        let (u, s, v) = math::svd(DMat3::from_cols_array(&m).transpose())?;
        Some(u * DMat3::from_diagonal(DVec3::new(s.x, s.y, 0.)) * v.transpose())
    };
    let inliers = |m: DMat3| -> Vec<Pair> {
        pairs
            .iter()
            .copied()
            .filter(|&(a, b)| {
                let (l, r) = (m * a, m.transpose() * b);
                b.dot(l).powi(2) * scale * scale
                    < 2.25 * (l.truncate().length_squared() + r.truncate().length_squared())
            })
            .collect()
    };
    let mut best = vec![];
    let mut state = 17u32;
    for _ in 0..64 {
        let mut indices = Vec::with_capacity(8);
        while indices.len() < 8 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let index = ((state as u64 * pairs.len() as u64) >> 32) as usize;
            if !indices.contains(&index) {
                indices.push(index);
            }
        }
        if let Some(f) = fit(&indices.iter().map(|&i| pairs[i]).collect::<Vec<_>>()) {
            let keep = inliers(f);
            if keep.len() > best.len() {
                best = keep;
            }
            if best.len() == pairs.len() {
                break;
            }
        }
    }
    (best.len() >= 8).then(|| fit(&best)).flatten()
}

/// Relative pose and focal length from the best supported essential decomposition.
pub(super) fn bootstrap(
    pairs: &[(DVec2, DVec2)],
    width: f64,
    focal: Option<f64>,
) -> Option<(Pose, f64)> {
    let rays: Vec<_> = pairs
        .iter()
        .map(|&(a, b)| ((a / width).extend(1.), (b / width).extend(1.)))
        .collect();
    let f = fundamental(&rays, width)?;
    let essential = |zoom: f64| {
        let k = DMat3::from_diagonal(DVec3::new(zoom / width, zoom / width, 1.));
        math::svd(k * f * k)
    };
    let zoom = focal.unwrap_or_else(|| {
        (0..65)
            .map(|i| width * (0.25_f64.ln() + i as f64 / 64. * 16_f64.ln()).exp())
            .min_by(|&a, &b| {
                let cost = |z| {
                    essential(z).map_or(f64::MAX, |(_, s, _)| {
                        (s.x - s.y) / s.x.max(1e-12) + 1e-8 * (z / width).ln().abs()
                    })
                };
                cost(a).total_cmp(&cost(b))
            })
            .unwrap_or(width)
    });
    let (u, _, v) = essential(zoom)?;
    let w = DMat3::from_cols(DVec3::Y, -DVec3::X, DVec3::Z);
    let origin = Pose::new(zoom);
    let mut best = None;
    let mut votes = 0;
    for twist in [w, w.transpose()] {
        let world = (u * twist * v.transpose()).transpose();
        for sign in [-1., 1.] {
            let pose = Pose {
                center: (world * u.z_axis * sign).to_array(),
                rotation: DQuat::from_mat3(&world).to_array(),
                zoom,
            };
            let n = pairs
                .iter()
                .filter(|&&(a, b)| {
                    triangulate([(origin, a), (pose, b)].into_iter()).is_some_and(|p| {
                        origin.pixel(p).is_some_and(|xy| xy.distance(a) < 4.)
                            && pose.pixel(p).is_some_and(|xy| xy.distance(b) < 4.)
                    })
                })
                .count();
            if n > votes {
                votes = n;
                best = Some((pose, zoom));
            }
        }
    }
    best.filter(|_| votes >= 8)
}

pub(super) fn triangulate(rays: impl Iterator<Item = (Pose, DVec2)>) -> Option<DVec3> {
    let mut lhs = DMat3::ZERO;
    let mut rhs = DVec3::ZERO;
    for (cam, xy) in rays {
        let ray = (cam.quat() * (xy / cam.zoom).extend(1.)).normalize();
        let normal = DMat3::IDENTITY - DMat3::from_cols(ray * ray.x, ray * ray.y, ray * ray.z);
        lhs += normal;
        rhs += normal * cam.position();
    }
    (lhs.determinant().abs() > 1e-10).then(|| lhs.inverse() * rhs)
}

/// Robust damped Gauss-Newton pose update in local camera coordinates.
pub(super) fn refine(
    mut pose: Pose,
    samples: &[Sample],
    scene: &BTreeMap<u32, DVec3>,
    pan: bool,
    zoom: bool,
) -> Option<Pose> {
    let mut damping = 1e-4;
    for _ in 0..18 {
        let mut h = [[0.; 7]; 7];
        let mut g = [0.; 7];
        let mut cost = 0.;
        let mut count = 0;
        for obs in samples {
            let Some(&world) = scene.get(&obs.id) else {
                continue;
            };
            let p = pose.quat().conjugate() * (world - pose.position());
            if p.z < 1e-6 {
                continue;
            }
            let xy = p.truncate() / p.z;
            let pixel = xy * pose.zoom;
            let error = obs.xy() - pixel;
            let weight = (2. / error.length().max(2.)).min(1.);
            let deriv = |v: DVec3| (v.truncate() - xy * v.z) * (pose.zoom / p.z);
            let axes = [DVec3::X, DVec3::Y, DVec3::Z];
            let mut j = [DVec2::ZERO; 7];
            for i in 0..3 {
                j[i] = deriv(p.cross(axes[i]));
                if !pan {
                    j[i + 3] = deriv(-axes[i]);
                }
            }
            if zoom {
                j[6] = pixel;
            }
            for i in 0..7 {
                g[i] += weight * j[i].dot(error);
                for k in 0..7 {
                    h[i][k] += weight * j[i].dot(j[k]);
                }
            }
            cost += huber(error.length());
            count += 1;
        }
        if count < 6 {
            return None;
        }
        for (i, row) in h.iter_mut().enumerate() {
            row[i] += damping * (1. + row[i]);
        }
        let d = math::linear(h, g)?;
        let q = pose.quat();
        let trial = Pose {
            rotation: (q * DQuat::from_scaled_axis(DVec3::new(d[0], d[1], d[2])))
                .normalize()
                .to_array(),
            center: (pose.position() + q * DVec3::new(d[3], d[4], d[5])).to_array(),
            zoom: pose.zoom * d[6].clamp(-0.25, 0.25).exp(),
        };
        let next: f64 = samples
            .iter()
            .filter_map(|s| {
                let &p = scene.get(&s.id)?;
                pose.pixel(p)?;
                Some(trial.pixel(p).map_or(1e10, |xy| huber(xy.distance(s.xy()))))
            })
            .sum();
        if next <= cost {
            pose = trial;
            damping = (damping * 0.3).max(1e-8);
            if (cost - next).abs() < 1e-7 {
                break;
            }
        } else {
            damping *= 10.;
        }
    }
    Some(pose)
}
fn huber(r: f64) -> f64 {
    if r < 2. { r * r * 0.5 } else { 2. * r - 2. }
}

pub(super) fn flatten(scene: &mut BTreeMap<u32, DVec3>) {
    let center = scene.values().sum::<DVec3>() / scene.len() as f64;
    let normal = DVec3::from_array(math::null_vector(
        scene.values().map(|p| (*p - center).to_array()),
    ));
    for p in scene.values_mut() {
        *p -= normal * normal.dot(*p - center);
    }
}
