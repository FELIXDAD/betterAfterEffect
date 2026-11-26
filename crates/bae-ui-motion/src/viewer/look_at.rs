//! View ▸ Look at Selected Layers (Cmd/Ctrl+Alt+Shift+\) and Look at All
//! Layers: move the current 3D view's camera, keeping its direction and zoom,
//! so the layers fill the frame. In Active Camera the comp's camera layer
//! moves (position and point of interest, one undo step); in Default and the
//! custom views the view camera moves; an orthographic view is centred on the
//! layers and zoomed to fit them.

use bae_anim::EvaluatedComp;
use bae_anim::glam::{DMat4, DVec2, DVec3};
use bae_model::{LayerKind, PrimPath};
use bae_ui::{Invocation, PanelContext};
use serde_json::json;

use super::view3d::{View3d, ViewCamera};
use crate::common;

/// Catalog action ids.
pub(crate) mod ids {
    pub const SELECTED: &str = "motion.view.look_at_selected_layers";
    pub const ALL: &str = "motion.view.look_at_all_layers";
}

/// `(action id, English title)` for the action registry.
pub(crate) const ACTIONS: &[(&str, &str)] = &[
    (ids::SELECTED, "Look at Selected Layers"),
    (ids::ALL, "Look at All Layers"),
];

/// Comp-space points to frame: the corners of the 3D layers and the places
/// of the lights and other cameras (`only`: just these layers; `skip`: the
/// camera that moves). 2D layers are not seen through the camera.
fn points(comp: &EvaluatedComp, only: Option<&[PrimPath]>, skip: Option<usize>) -> Vec<DVec3> {
    let mut out = Vec::new();
    for (i, l) in comp.layers.iter().enumerate() {
        if Some(i) == skip || !l.visible || only.is_some_and(|o| !o.contains(&l.path)) {
            continue;
        }
        if matches!(l.kind, LayerKind::Camera | LayerKind::Light) {
            out.push(l.world_matrix.w_axis.truncate());
        } else if crate::render::is_3d(l)
            && let Some((min, max)) = crate::render::layer_bounds(l)
        {
            for p in [min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)] {
                out.push(l.world_matrix.transform_point3(p.extend(0.0)));
            }
        }
    }
    out
}

/// A camera's axes in comp space: right, down, forward (unit length).
fn axes(world: &DMat4) -> [DVec3; 3] {
    [world.x_axis, world.y_axis, world.z_axis].map(|a| a.truncate().normalize_or(DVec3::Z))
}

fn centre(points: &[DVec3]) -> DVec3 {
    let (lo, hi) = points.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    (lo + hi) / 2.0
}

/// Where a perspective camera looking along `world`'s axes with `zoom` must
/// stand to show all `points` in a `size` frame: `(eye, target)`, the target
/// being the middle of the points. `fallback`: the distance for points that
/// span nothing (one light).
pub(crate) fn fit_perspective(
    points: &[DVec3],
    world: &DMat4,
    zoom: f64,
    size: DVec2,
    fallback: f64,
) -> Option<(DVec3, DVec3)> {
    if points.is_empty() {
        return None;
    }
    let [right, down, forward] = axes(world);
    let c = centre(points);
    let (hw, hh) = (size.x / 2.0, size.y / 2.0);
    // A point at offset q appears |q·right| · zoom / depth from the middle,
    // with depth = distance + q·forward.
    let distance = points.iter().fold(0.0f64, |d, p| {
        let q = *p - c;
        let f = q.dot(forward);
        d.max(q.dot(right).abs() * zoom / hw - f)
            .max(q.dot(down).abs() * zoom / hh - f)
            .max(1.0 - f)
    });
    let distance = if distance <= 1.0 + 1e-9 && points.iter().all(|p| p.distance(c) < 1e-6) {
        fallback
    } else {
        distance
    };
    Some((c - forward * distance, c))
}

/// An orthographic view showing all `points`: `(centre, scale)`; the scale
/// stays `scale` when the points span nothing.
pub(crate) fn fit_ortho(
    points: &[DVec3],
    world: &DMat4,
    size: DVec2,
    scale: f64,
) -> Option<(DVec3, f64)> {
    if points.is_empty() {
        return None;
    }
    let [right, down, _] = axes(world);
    let c = centre(points);
    let (hw, hh) = (size.x / 2.0, size.y / 2.0);
    let fit = points.iter().fold(f64::INFINITY, |s, p| {
        let q = *p - c;
        let (x, y) = (q.dot(right).abs(), q.dot(down).abs());
        let s = if x > 1e-9 { s.min(hw / x) } else { s };
        if y > 1e-9 { s.min(hh / y) } else { s }
    });
    Some((c, if fit.is_finite() { fit } else { scale }))
}

/// Runs Look at Selected / All Layers on the active comp; `false` for other
/// actions.
pub(crate) fn handle(inv: &Invocation, cx: &mut PanelContext<'_>) -> bool {
    let all = match inv.id.as_str() {
        ids::SELECTED => false,
        ids::ALL => true,
        _ => return false,
    };
    let session = common::session(cx.resources);
    let (comp, time, selected, view) = {
        let s = session.lock();
        let Some(comp) = s.active_comp.clone() else {
            return true;
        };
        let view = s.views3d.get(&comp).map_or(View3d::Active, |v| v.current);
        (comp.clone(), s.time(&comp), s.selected_layers.clone(), view)
    };
    let Ok(ev) = bae_anim::evaluate_comp(cx.stage, &comp, time) else {
        return true;
    };
    let size = DVec2::new(f64::from(ev.width), f64::from(ev.height));
    let only = (!all).then_some(selected.as_slice());
    match view {
        View3d::Active => {
            let Some((i, layer)) = ev.camera.and_then(|i| Some((i, ev.layers.get(i)?))) else {
                return true;
            };
            let Some(cam) = layer.camera else {
                return true;
            };
            let pts = points(&ev, only, Some(i));
            let world = layer.world_matrix;
            let here = world.w_axis.truncate();
            let Some((eye, target)) = fit_perspective(&pts, &world, cam.zoom, size, cam.zoom)
            else {
                return true;
            };
            // Position and point of interest live in the camera's parent space.
            let parent = world * layer.local_matrix.inverse();
            let to_local = parent.inverse();
            let (eye, target) = (
                to_local.transform_point3(eye),
                to_local.transform_point3(target),
            );
            if eye.distance(to_local.transform_point3(here)) < 1e-6 {
                return true;
            }
            let t = bae_motion::query::to_key_time(cx.stage, &layer.path, time);
            let v3 = |v: DVec3| json!({"type": "vec3", "value": v.to_array()});
            let mut edits = vec![
                json!({"path": layer.path, "attr": "transform:position", "time": t, "value": v3(eye)}),
            ];
            if cam.two_node {
                edits.push(json!({"path": layer.path, "attr": "transform:point_of_interest", "time": t, "value": v3(target)}));
            }
            cx.invoke_with("attr.set_many", json!({ "edits": edits }));
        }
        current => {
            let pts = points(&ev, only, None);
            let mut s = session.lock();
            let views = s.views3d.entry(comp).or_default();
            let fitted = match views.view_camera(current, size) {
                ViewCamera::Perspective {
                    position,
                    target,
                    zoom,
                } => {
                    let world = ViewCamera::Perspective {
                        position,
                        target,
                        zoom,
                    }
                    .camera(current)
                    .world;
                    fit_perspective(&pts, &world, zoom, size, position.distance(target)).map(
                        |(position, target)| ViewCamera::Perspective {
                            position,
                            target,
                            zoom,
                        },
                    )
                }
                ortho @ ViewCamera::Ortho { scale, .. } => {
                    let world = ortho.camera(current).world;
                    fit_ortho(&pts, &world, size, scale)
                        .map(|(centre, scale)| ViewCamera::Ortho { centre, scale })
                }
            };
            if let Some(camera) = fitted {
                views.set_camera(current, camera);
            }
        }
    }
    true
}

/// Whether Look at Selected / All Layers applies: a comp is active and, for
/// Look at Selected, layers are selected (Active Camera without a camera
/// layer does nothing: the default camera cannot move).
pub(crate) fn enabled(resources: &bae_app::Resources, all: bool) -> bool {
    let session = common::session(resources);
    let s = session.lock();
    s.active_comp.is_some() && (all || !s.selected_layers.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: DVec2 = DVec2::new(640.0, 360.0);

    fn corners(x0: f64, y0: f64, x1: f64, y1: f64, z: f64) -> Vec<DVec3> {
        vec![
            DVec3::new(x0, y0, z),
            DVec3::new(x1, y0, z),
            DVec3::new(x1, y1, z),
            DVec3::new(x0, y1, z),
        ]
    }

    #[test]
    fn a_comp_sized_layer_puts_the_default_camera_back() {
        let zoom = bae_render::camera::default_zoom(640);
        let pts = corners(0.0, 0.0, 640.0, 360.0, 0.0);
        let (eye, target) = fit_perspective(&pts, &DMat4::IDENTITY, zoom, SIZE, zoom).expect("fit");
        assert!(target.distance(DVec3::new(320.0, 180.0, 0.0)) < 1e-9);
        assert!(
            eye.distance(DVec3::new(320.0, 180.0, -zoom)) < 1e-6,
            "{eye}"
        );
    }

    #[test]
    fn a_small_layer_off_centre_fills_the_frame() {
        let zoom = bae_render::camera::default_zoom(640);
        // 64 × 36 at (100, 50): a tenth of the comp, so a tenth of the distance.
        let pts = corners(100.0, 50.0, 164.0, 86.0, 0.0);
        let (eye, target) = fit_perspective(&pts, &DMat4::IDENTITY, zoom, SIZE, zoom).expect("fit");
        assert!(target.distance(DVec3::new(132.0, 68.0, 0.0)) < 1e-9);
        assert!((eye.z + zoom / 10.0).abs() < 1e-6, "{eye}");
        // One point keeps the given distance.
        let one = [DVec3::new(5.0, 6.0, 7.0)];
        let (eye, _) = fit_perspective(&one, &DMat4::IDENTITY, zoom, SIZE, 300.0).expect("fit");
        assert!(eye.distance(DVec3::new(5.0, 6.0, -293.0)) < 1e-9);
        // Orthographic: centred, scaled to fit the wider side.
        let (c, s) = fit_ortho(&pts, &DMat4::IDENTITY, SIZE, 1.0).expect("fit");
        assert!(c.distance(DVec3::new(132.0, 68.0, 0.0)) < 1e-9);
        assert!((s - 10.0).abs() < 1e-9);
    }
}
