//! The viewer's 3D views (AE-CMP-410): View ▸ Switch 3D View, the 3D View
//! popup in the viewer's bar, the three 3D view shortcuts (F10, F11, F12) and
//! Esc (the previous view).
//!
//! Only Active Camera is the comp's camera. The other views look at the scene
//! without changing the render output: Default (a free copy of the default
//! camera), the orthographic Front … Bottom and Custom View 1–3. Each comp keeps
//! its views and their cameras in the session, like AE keeps them per viewer.

use std::collections::HashMap;

use bae_anim::EvaluatedComp;
use bae_anim::glam::{DMat4, DVec2, DVec3};
use bae_render::camera::{Camera, OrthoView};
use bae_ui::{Invocation, PanelContext};

use crate::common;

/// Catalog action ids.
pub(crate) mod ids {
    /// `{"view": "front"}` …
    pub const SWITCH: &str = "motion.view.switch_3d_view";
    /// `{"slot": "a" | "b" | "c"}`: F10, F11, F12.
    pub const SHORTCUT: &str = "motion.view.switch_3d_view_shortcut";
    /// View ▸ Assign 3D View Shortcut ▸ A / B / C.
    pub const ASSIGN: &str = "motion.view.assign_3d_view_shortcut";
    /// Esc.
    pub const LAST: &str = "motion.view.switch_to_last_3d_view";
    pub const RESET: &str = "motion.view.reset_3d_view";
    /// View ▸ Create Camera from 3D View.
    pub const CREATE_CAMERA: &str = "motion.view.create_camera_from_3d_view";
}

/// `(action id, English title)` for the action registry.
pub(crate) const ACTIONS: &[(&str, &str)] = &[
    (ids::SWITCH, "Switch 3D View"),
    (ids::SHORTCUT, "Switch to 3D View"),
    (ids::ASSIGN, "Assign 3D View Shortcut"),
    (ids::LAST, "Switch to Last 3D View"),
    (ids::RESET, "Reset 3D View"),
    (ids::CREATE_CAMERA, "Create Camera from 3D View"),
];

/// The 3D view shortcut commands, named as in AE's Keyboard Shortcuts.
pub(crate) const SHORTCUT_NAMES: &[(&str, &str)] = &[
    ("a", "Switch to 3D View A"),
    ("b", "Switch to 3D View B"),
    ("c", "Switch to 3D View C"),
];

/// One of AE's 3D views.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum View3d {
    /// The comp's active camera (the default camera without a camera layer).
    #[default]
    Active,
    /// The default camera, free to move without changing the output.
    Default,
    Ortho(OrthoView),
    /// Custom View 1–3.
    Custom(u8),
}

impl View3d {
    /// AE's order (the 3D View popup and View ▸ Switch 3D View).
    pub const ALL: [View3d; 11] = [
        View3d::Active,
        View3d::Default,
        View3d::Ortho(OrthoView::Front),
        View3d::Ortho(OrthoView::Left),
        View3d::Ortho(OrthoView::Top),
        View3d::Ortho(OrthoView::Back),
        View3d::Ortho(OrthoView::Right),
        View3d::Ortho(OrthoView::Bottom),
        View3d::Custom(1),
        View3d::Custom(2),
        View3d::Custom(3),
    ];

    /// The catalog's `view` parameter.
    pub fn key(self) -> &'static str {
        match self {
            View3d::Active => "active_camera",
            View3d::Default => "default",
            View3d::Ortho(OrthoView::Front) => "front",
            View3d::Ortho(OrthoView::Left) => "left",
            View3d::Ortho(OrthoView::Top) => "top",
            View3d::Ortho(OrthoView::Back) => "back",
            View3d::Ortho(OrthoView::Right) => "right",
            View3d::Ortho(OrthoView::Bottom) => "bottom",
            View3d::Custom(1) => "custom_1",
            View3d::Custom(2) => "custom_2",
            View3d::Custom(_) => "custom_3",
        }
    }

    pub fn parse(key: &str) -> Option<View3d> {
        View3d::ALL.into_iter().find(|v| v.key() == key)
    }

    /// English name (AE's).
    pub fn title(self) -> &'static str {
        match self {
            View3d::Active => "Active Camera",
            View3d::Default => "Default",
            View3d::Ortho(OrthoView::Front) => "Front",
            View3d::Ortho(OrthoView::Left) => "Left",
            View3d::Ortho(OrthoView::Top) => "Top",
            View3d::Ortho(OrthoView::Back) => "Back",
            View3d::Ortho(OrthoView::Right) => "Right",
            View3d::Ortho(OrthoView::Bottom) => "Bottom",
            View3d::Custom(1) => "Custom View 1",
            View3d::Custom(2) => "Custom View 2",
            View3d::Custom(_) => "Custom View 3",
        }
    }
}

/// The camera of a view other than Active Camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ViewCamera {
    /// Default and Custom View 1–3: a perspective camera aimed at `target`.
    Perspective {
        position: DVec3,
        target: DVec3,
        zoom: f64,
    },
    /// Front … Bottom: the comp-space point in the middle of the frame and the
    /// magnification (1 = 100 %).
    Ortho { centre: DVec3, scale: f64 },
}

impl ViewCamera {
    /// AE's camera for `view` in a `size` comp before any camera tool moved it.
    pub fn initial(view: View3d, size: DVec2) -> ViewCamera {
        let centre = (size / 2.0).extend(0.0);
        let zoom = bae_render::camera::default_zoom(size.x as u32);
        match view {
            View3d::Ortho(_) => ViewCamera::Ortho { centre, scale: 1.0 },
            View3d::Custom(n) => {
                let (dir, distance) = CUSTOM[usize::from(n.clamp(1, 3)) - 1];
                ViewCamera::Perspective {
                    position: centre + DVec3::from_array(dir).normalize() * distance * zoom,
                    target: centre,
                    zoom,
                }
            }
            View3d::Active | View3d::Default => ViewCamera::Perspective {
                position: centre - DVec3::Z * zoom,
                target: centre,
                zoom,
            },
        }
    }

    /// A perspective camera that sees what this view sees: position, point of
    /// interest and zoom. An orthographic view becomes a camera along its
    /// viewing direction that shows the plane through its centre at the same
    /// magnification.
    pub fn lens(self, view: View3d, size: DVec2) -> (DVec3, DVec3, f64) {
        match self {
            ViewCamera::Perspective {
                position,
                target,
                zoom,
            } => (position, target, zoom),
            ViewCamera::Ortho { centre, scale } => {
                let zoom = bae_render::camera::default_zoom(size.x as u32);
                let dir = self.camera(view).direction();
                (centre - dir * zoom / scale.max(1e-6), centre, zoom)
            }
        }
    }

    pub fn camera(self, view: View3d) -> Camera {
        match (self, view) {
            (ViewCamera::Ortho { centre, scale }, View3d::Ortho(o)) => {
                Camera::orthographic(o, centre, scale)
            }
            (ViewCamera::Ortho { centre, scale }, _) => {
                Camera::orthographic(OrthoView::Front, centre, scale)
            }
            (
                ViewCamera::Perspective {
                    position,
                    target,
                    zoom,
                },
                _,
            ) => Camera::from_transform(
                DMat4::IDENTITY,
                position,
                Some(target),
                DVec3::ZERO,
                DVec3::ZERO,
                zoom,
            ),
        }
    }
}

/// Custom View 1–3 as they start: the direction from the comp centre to the
/// camera and its distance in default zooms (from above left, above, above
/// right; AE's exact angles 待核).
const CUSTOM: [([f64; 3], f64); 3] = [
    ([-1.0, -1.0, -1.0], 1.0),
    ([0.0, -1.0, -1.0], 1.0),
    ([1.0, -1.0, -1.0], 1.0),
];

/// One comp's 3D views.
#[derive(Clone, Debug, Default)]
pub struct Views {
    pub current: View3d,
    /// Where Esc goes back to.
    pub last: View3d,
    /// Cameras the camera tools moved (the others are [`ViewCamera::initial`]).
    cameras: HashMap<View3d, ViewCamera>,
}

impl Views {
    pub fn switch(&mut self, to: View3d) {
        if to != self.current {
            self.last = self.current;
            self.current = to;
        }
    }

    pub fn view_camera(&self, view: View3d, size: DVec2) -> ViewCamera {
        self.cameras
            .get(&view)
            .copied()
            .unwrap_or_else(|| ViewCamera::initial(view, size))
    }

    /// The camera the viewer shows the current view with; `None` for Active
    /// Camera (the comp's own).
    pub fn camera(&self, size: DVec2) -> Option<Camera> {
        (self.current != View3d::Active)
            .then(|| self.view_camera(self.current, size).camera(self.current))
    }
}

/// The three 3D view shortcuts (A = F10, B = F11, C = F12), AE's defaults:
/// Front, Custom View 1, Active Camera.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcuts(pub [View3d; 3]);

impl Default for Shortcuts {
    fn default() -> Self {
        Shortcuts([
            View3d::Ortho(OrthoView::Front),
            View3d::Custom(1),
            View3d::Active,
        ])
    }
}

/// The comp's active camera: its camera layer, or the default camera.
pub(crate) fn active_camera(comp: &EvaluatedComp, size: DVec2) -> Camera {
    comp.camera
        .and_then(|i| comp.layers.get(i)?.camera.as_ref())
        .map_or_else(
            || Camera::default_for(size.x as u32, size.y as u32),
            |c| Camera::new(c.world_matrix, c.zoom),
        )
}

fn slot(inv: &Invocation) -> usize {
    match inv.params.get("slot").and_then(|v| v.as_str()) {
        Some("b") => 1,
        Some("c") => 2,
        _ => 0,
    }
}

/// Runs the 3D view commands on the active comp.
pub(crate) fn handle(inv: &Invocation, cx: &mut PanelContext<'_>) -> bool {
    if !ACTIONS.iter().any(|(id, _)| *id == inv.id) {
        return false;
    }
    let session = common::session(cx.resources);
    let mut s = session.lock();
    let Some(comp) = s.active_comp.clone() else {
        return true;
    };
    let shortcuts = s.view_shortcuts;
    let views = s.views3d.entry(comp.clone()).or_default();
    match inv.id.as_str() {
        ids::SWITCH => {
            if let Some(v) = inv
                .params
                .get("view")
                .and_then(|v| v.as_str())
                .and_then(View3d::parse)
            {
                views.switch(v);
            }
        }
        ids::SHORTCUT => views.switch(shortcuts.0[slot(inv)]),
        ids::LAST => views.switch(views.last),
        ids::RESET if views.current == View3d::Active => {
            // AE's "Reset Camera 1": the active camera layer back to the
            // default camera's place, keeping its zoom.
            let time = s.time(&comp);
            drop(s);
            reset_camera(cx, &comp, time);
            return true;
        }
        ids::RESET => {
            let current = views.current;
            views.cameras.remove(&current);
        }
        ids::ASSIGN => {
            let current = views.current;
            s.view_shortcuts.0[slot(inv)] = current;
        }
        ids::CREATE_CAMERA if views.current != View3d::Active => {
            let Some(info) = bae_motion::query::comp_info(cx.stage, &comp) else {
                return true;
            };
            let size = DVec2::new(f64::from(info.width), f64::from(info.height));
            let (eye, target, zoom) = views
                .view_camera(views.current, size)
                .lens(views.current, size);
            drop(s);
            common::new_layer(
                cx,
                &session,
                bae_model::LayerKind::Camera,
                serde_json::json!({
                    "camera": { "zoom": zoom },
                    "position_3d": eye.to_array(),
                    "point_of_interest": target.to_array(),
                }),
            );
        }
        _ => {}
    }
    true
}

/// Whether View ▸ Create Camera from 3D View applies: the active comp shows a
/// view other than Active Camera.
pub(crate) fn can_create_camera(resources: &bae_app::Resources) -> bool {
    common::session(resources).lock().active_comp.is_some() && current(resources) != View3d::Active
}

/// Active Camera ▸ Reset: the comp's camera layer at `(W/2, H/2, −zoom)` aimed
/// at the comp centre, no orientation or rotation (one undo step).
fn reset_camera(cx: &mut PanelContext<'_>, comp: &bae_model::PrimPath, time: bae_model::Time) {
    let Ok(ev) = bae_anim::evaluate_comp(cx.stage, comp, time) else {
        return;
    };
    let Some((layer, zoom)) = ev
        .camera
        .and_then(|i| ev.layers.get(i))
        .and_then(|l| Some((l.path.clone(), l.camera.as_ref()?.zoom)))
    else {
        return;
    };
    let (w, h) = (f64::from(ev.width) / 2.0, f64::from(ev.height) / 2.0);
    let t = bae_motion::query::to_key_time(cx.stage, &layer, time);
    let v3 = |v: [f64; 3]| serde_json::json!({"type": "vec3", "value": v});
    let f = |v: f64| serde_json::json!({"type": "float", "value": v});
    let edit = |attr: &str, value: serde_json::Value| serde_json::json!({"path": layer, "attr": attr, "time": t, "value": value});
    let edits = vec![
        edit("transform:position", v3([w, h, -zoom])),
        edit("transform:point_of_interest", v3([w, h, 0.0])),
        edit("transform:orientation", v3([0.0; 3])),
        edit("transform:rotation_x", f(0.0)),
        edit("transform:rotation_y", f(0.0)),
        edit("transform:rotation", f(0.0)),
    ];
    cx.invoke_with("attr.set_many", serde_json::json!({ "edits": edits }));
}

/// The active comp's current 3D view (menus, the viewer's popup).
pub(crate) fn current(resources: &bae_app::Resources) -> View3d {
    let session = common::session(resources);
    let s = session.lock();
    s.active_comp
        .as_ref()
        .and_then(|c| s.views3d.get(c))
        .map_or(View3d::Active, |v| v.current)
}
