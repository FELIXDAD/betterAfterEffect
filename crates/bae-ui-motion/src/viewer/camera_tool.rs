//! The camera tools: Orbit (around the cursor, the scene or the camera's point
//! of interest), Pan (under the cursor, the point of interest) and Dolly
//! (towards the cursor, to the cursor, to the point of interest). With any of
//! them the middle button pans and the right button dollies.
//!
//! In Active Camera with a camera layer they move the camera layer (Position
//! and Point of Interest, one undo step per drag). Otherwise they move the 3D
//! view's own camera (Active Camera without a camera layer switches to Default,
//! like AE's default camera navigation). Orthographic views pan and dolly
//! (zoom) but do not orbit.

use bae_anim::EvaluatedComp;
use bae_anim::glam::{DMat3, DMat4, DVec2, DVec3};
use bae_model::{PrimPath, Stage, Time, Value};
use bae_motion::query;
use bae_render::Camera;
use egui::{PointerButton, Pos2, Response, Ui, Vec2};
use serde_json::json;

use super::view3d::{View3d, ViewCamera, Views};
use crate::render::{self, View};
use crate::tools::Tool;

/// Orbit: degrees per point dragged.
const ORBIT_SPEED: f64 = 0.4;
/// Dolly: the distance shrinks by `exp(-DOLLY_SPEED)` per point dragged.
const DOLLY_SPEED: f64 = 0.005;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Orbit,
    Pan,
    /// Along the line to `pivot`, the point of interest following.
    Dolly,
    /// Towards the point of interest, which stays.
    DollyPoi,
}

enum Target {
    View(View3d),
    /// A camera layer; `two_node`: it has a point of interest.
    Layer {
        path: PrimPath,
        parent: DMat4,
        two_node: bool,
    },
}

pub(super) struct CameraDrag {
    target: Target,
    mode: Mode,
    start: ViewCamera,
    /// Orbit centre, the point that pans with the pointer, the dolly goal.
    pivot: DVec3,
    origin: Pos2,
    pub now: ViewCamera,
    /// A wheel dolly on a camera layer: notches add up into one gesture.
    wheel: Option<Wheel>,
}

/// Where a wheel dolly started, how far it has gone and its last notch.
#[derive(Clone, Copy, Debug)]
struct Wheel {
    at: Pos2,
    distance: f32,
    last: std::time::Instant,
}

/// A wheel dolly on a camera layer commits (one undo step) once the wheel
/// has rested this long; high-resolution wheels split a notch into events.
const WHEEL_REST: std::time::Duration = std::time::Duration::from_millis(300);

/// Position, point of interest and zoom of a perspective camera.
fn rig(c: ViewCamera) -> Option<(DVec3, DVec3, f64)> {
    match c {
        ViewCamera::Perspective {
            position,
            target,
            zoom,
        } => Some((position, target, zoom)),
        ViewCamera::Ortho { .. } => None,
    }
}

fn rotation(axis: DVec3, degrees: f64) -> DMat3 {
    DMat3::from_axis_angle(axis.normalize_or(DVec3::Y), degrees.to_radians())
}

impl CameraDrag {
    #[allow(clippy::too_many_arguments)]
    fn new(
        tool: Tool,
        button: PointerButton,
        views: &mut Views,
        comp: &EvaluatedComp,
        size: DVec2,
        view: &View,
        pos: Pos2,
    ) -> Option<Self> {
        let (target, start) = match comp
            .camera
            .and_then(|i| comp.layers.get(i))
            .filter(|_| views.current == View3d::Active)
        {
            Some(l) => {
                let c = l.camera.as_ref()?;
                let parent = l
                    .parent
                    .map_or(DMat4::IDENTITY, |p| comp.layers[p].world_matrix);
                let position = c.world_matrix.w_axis.truncate();
                let target = if c.two_node {
                    parent.transform_point3(c.point_of_interest)
                } else {
                    position + c.world_matrix.z_axis.truncate().normalize() * c.zoom
                };
                let start = ViewCamera::Perspective {
                    position,
                    target,
                    zoom: c.zoom,
                };
                let target = Target::Layer {
                    path: l.path.clone(),
                    parent,
                    two_node: c.two_node,
                };
                (target, start)
            }
            None => {
                if views.current == View3d::Active {
                    views.switch(View3d::Default);
                }
                let v = views.current;
                (Target::View(v), views.view_camera(v, size))
            }
        };
        let mode = match (button, tool) {
            (PointerButton::Middle, _) => Mode::Pan,
            (PointerButton::Secondary, _) => Mode::Dolly,
            (_, Tool::OrbitCursor | Tool::OrbitScene | Tool::OrbitPoi) => Mode::Orbit,
            (_, Tool::PanCursor | Tool::PanPoi) => Mode::Pan,
            (_, Tool::DollyPoi) => Mode::DollyPoi,
            _ => Mode::Dolly,
        };
        if mode == Mode::Orbit && rig(start).is_none() {
            return None;
        }
        let camera = start.camera(match &target {
            Target::View(v) => *v,
            Target::Layer { .. } => View3d::Active,
        });
        let q = view.to_comp(pos);
        let poi = match start {
            ViewCamera::Perspective { target, .. } => target,
            ViewCamera::Ortho { centre, .. } => centre,
        };
        let depth = camera.view().transform_point3(poi).z;
        let under_cursor = camera.unproject(q, depth, size);
        let pivot = match tool {
            _ if button != PointerButton::Primary => under_cursor,
            Tool::OrbitScene => scene_centre(comp, size),
            Tool::OrbitPoi | Tool::PanPoi | Tool::DollyPoi => poi,
            _ => under_cursor,
        };
        Some(CameraDrag {
            target,
            mode,
            start,
            pivot,
            origin: pos,
            now: start,
            wheel: None,
        })
    }

    /// Follow the pointer at `pos`.
    fn update(&mut self, pos: Pos2, view: &View, size: DVec2, reverse_dolly: bool) {
        let d: Vec2 = pos - self.origin;
        let (dx, dy) = (f64::from(d.x), f64::from(d.y));
        let dolly = if reverse_dolly { -(dx + dy) } else { dx + dy };
        let view_of = |t: &Target| match t {
            Target::View(v) => *v,
            Target::Layer { .. } => View3d::Active,
        };
        let camera = self.start.camera(view_of(&self.target));
        let pivot = self.pivot;
        self.now = match (self.mode, self.start) {
            (
                Mode::Orbit,
                ViewCamera::Perspective {
                    position,
                    target,
                    zoom,
                },
            ) => {
                let right = camera.world.x_axis.truncate();
                let r = rotation(DVec3::Y, -dx * ORBIT_SPEED) * rotation(right, dy * ORBIT_SPEED);
                ViewCamera::Perspective {
                    position: pivot + r * (position - pivot),
                    target: pivot + r * (target - pivot),
                    zoom,
                }
            }
            (Mode::Orbit, ortho) => ortho,
            (Mode::Pan, start) => {
                let q0 = view.to_comp(self.origin);
                let q1 = view.to_comp(pos);
                let depth = camera.view().transform_point3(pivot).z;
                let w = camera.unproject(q1, depth, size) - camera.unproject(q0, depth, size);
                match start {
                    ViewCamera::Perspective {
                        position,
                        target,
                        zoom,
                    } => ViewCamera::Perspective {
                        position: position - w,
                        target: target - w,
                        zoom,
                    },
                    ViewCamera::Ortho { centre, scale } => ViewCamera::Ortho {
                        centre: centre - w,
                        scale,
                    },
                }
            }
            (Mode::Dolly | Mode::DollyPoi, ViewCamera::Ortho { centre, scale }) => {
                let s = scale * (DOLLY_SPEED * dolly).exp();
                ViewCamera::Ortho {
                    centre: pivot - (pivot - centre) * scale / s,
                    scale: s,
                }
            }
            (
                mode,
                ViewCamera::Perspective {
                    position,
                    target,
                    zoom,
                },
            ) => {
                let to = pivot - position;
                let distance = to.length();
                let mut step = distance * (1.0 - (-DOLLY_SPEED * dolly).exp());
                if mode == Mode::DollyPoi {
                    // Not past the point of interest.
                    step = step.min(distance - 1.0);
                }
                let w = to.normalize_or_zero() * step;
                ViewCamera::Perspective {
                    position: position + w,
                    target: if mode == Mode::DollyPoi {
                        target
                    } else {
                        target + w
                    },
                    zoom,
                }
            }
        };
    }

    /// The camera layer's camera while it is dragged (the viewer shows it).
    pub fn layer_camera(&self) -> Option<Camera> {
        matches!(self.target, Target::Layer { .. }).then(|| self.now.camera(View3d::Active))
    }

    pub(super) fn paint_point(
        &self,
        painter: &egui::Painter,
        view: &View,
        mode: super::prefs::CameraPoint,
        pointer: Option<Pos2>,
    ) {
        use super::prefs::CameraPoint;
        if mode == CameraPoint::Hidden {
            return;
        }
        let Some(pivot) = view.project(self.pivot, true) else {
            return;
        };
        let ink = egui::Stroke::new(1.5, bae_ui::theme::color::ACCENT);
        painter.circle_stroke(pivot, 5.0, egui::Stroke::new(3.0, egui::Color32::BLACK));
        painter.circle_stroke(pivot, 5.0, ink);
        if mode == CameraPoint::Directional
            && let Some(pos) = pointer
        {
            let direction = pos - self.origin;
            if direction.length_sq() > 1.0 {
                painter.arrow(pivot, direction.normalized() * 17.0, ink);
            }
        }
    }

    /// The camera layer's `attr.set_many` edits.
    fn commit(&self, stage: &Stage, time: Time) -> Vec<serde_json::Value> {
        let Target::Layer {
            path,
            parent,
            two_node,
        } = &self.target
        else {
            return Vec::new();
        };
        let (Some((p0, t0, _)), Some((p1, t1, _))) = (rig(self.start), rig(self.now)) else {
            return Vec::new();
        };
        let local = |p: DVec3| {
            let v = parent.inverse().transform_point3(p);
            Value::Vec3([v.x, v.y, v.z])
        };
        let t = query::to_key_time(stage, path, time);
        let mut out = Vec::new();
        if p1 != p0 {
            out.push(
                json!({"path": path, "attr": "transform:position", "time": t, "value": local(p1)}),
            );
        }
        if *two_node && t1 != t0 {
            out.push(json!({"path": path, "attr": "transform:point_of_interest", "time": t, "value": local(t1)}));
        }
        out
    }
}

/// The middle of the 3D layers' anchor points (the comp centre without any).
fn scene_centre(comp: &EvaluatedComp, size: DVec2) -> DVec3 {
    let points: Vec<DVec3> = comp
        .layers
        .iter()
        .filter(|l| render::is_3d(l))
        .map(|l| l.world_matrix.transform_point3(l.transform.anchor_point))
        .collect();
    if points.is_empty() {
        (size / 2.0).extend(0.0)
    } else {
        points.iter().sum::<DVec3>() / points.len() as f64
    }
}

/// One frame of a camera tool in the viewer: start, follow and finish drags.
#[allow(clippy::too_many_arguments)]
pub(super) fn frame(
    drag: &mut Option<CameraDrag>,
    resp: &Response,
    ui: &Ui,
    cx: &mut bae_ui::PanelContext<'_>,
    session: &crate::session::SharedSession,
    comp_path: &PrimPath,
    comp: &EvaluatedComp,
    size: DVec2,
    view: &View,
    tool: Tool,
    time: Time,
    wheel_notches: f32,
    reverse_drag: bool,
) {
    if wheel_notches != 0.0
        && let Some(pos) = resp.hover_pos()
    {
        // One eighth-octave step, also continuous on high-resolution wheels.
        let step = (std::f64::consts::LN_2 / (8.0 * DOLLY_SPEED)) as f32 * wheel_notches;
        let now = std::time::Instant::now();
        match drag {
            // The wheel keeps turning: the same gesture goes on.
            Some(d) if d.wheel.is_some() => {
                let mut w = d.wheel.expect("wheel gesture");
                w.distance += step;
                w.last = now;
                d.wheel = Some(w);
                d.update(w.at + egui::vec2(0.0, w.distance), view, size, false);
            }
            Some(_) => {}
            None => {
                let mut s = session.lock();
                let views = s.views3d.entry(comp_path.clone()).or_default();
                if let Some(mut wheel) = CameraDrag::new(
                    Tool::DollyCursor,
                    PointerButton::Secondary,
                    views,
                    comp,
                    size,
                    view,
                    pos,
                ) {
                    wheel.update(pos + egui::vec2(0.0, step), view, size, false);
                    match wheel.target {
                        Target::View(v) => views.set_camera(v, wheel.now),
                        // A camera layer previews through the gesture and
                        // commits once the wheel rests.
                        Target::Layer { .. } => {
                            wheel.wheel = Some(Wheel {
                                at: pos,
                                distance: step,
                                last: now,
                            });
                            *drag = Some(wheel);
                        }
                    }
                }
            }
        }
    }
    if drag.is_none() && tool.is_camera() {
        for button in [
            PointerButton::Primary,
            PointerButton::Middle,
            PointerButton::Secondary,
        ] {
            if resp.drag_started_by(button)
                && let Some(pos) = ui.input(|i| i.pointer.press_origin())
            {
                let mut s = session.lock();
                let views = s.views3d.entry(comp_path.clone()).or_default();
                *drag = CameraDrag::new(tool, button, views, comp, size, view, pos);
            }
        }
    }
    let Some(d) = drag.as_mut() else {
        return;
    };
    if let Some(w) = d.wheel {
        if w.last.elapsed() >= WHEEL_REST {
            let edits = d.commit(cx.stage, time);
            if !edits.is_empty() {
                cx.invoke_with("attr.set_many", json!({ "edits": edits }));
            }
            *drag = None;
        } else {
            ui.ctx().request_repaint_after(WHEEL_REST);
        }
        return;
    }
    if let Some(pos) = resp.interact_pointer_pos().or(resp.hover_pos()) {
        d.update(pos, view, size, reverse_drag);
    }
    if let Target::View(v) = d.target {
        session
            .lock()
            .views3d
            .entry(comp_path.clone())
            .or_default()
            .set_camera(v, d.now);
    }
    if !ui.input(|i| i.pointer.any_down()) {
        let edits = d.commit(cx.stage, time);
        if !edits.is_empty() {
            cx.invoke_with("attr.set_many", json!({ "edits": edits }));
        }
        *drag = None;
    }
}
