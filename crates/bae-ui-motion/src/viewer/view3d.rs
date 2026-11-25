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
    /// `{"views": 1 | 2 | 4}`: View ▸ Switch View Layout.
    pub const LAYOUT: &str = "motion.view.view_layout";
}

/// `(action id, English title)` for the action registry.
pub(crate) const ACTIONS: &[(&str, &str)] = &[
    (ids::SWITCH, "Switch 3D View"),
    (ids::SHORTCUT, "Switch to 3D View"),
    (ids::ASSIGN, "Assign 3D View Shortcut"),
    (ids::LAST, "Switch to Last 3D View"),
    (ids::RESET, "Reset 3D View"),
    (ids::LAYOUT, "Switch View Layout"),
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
    /// The active view's 3D view.
    pub current: View3d,
    /// Where Esc goes back to.
    pub last: View3d,
    /// Cameras the camera tools moved (the others are [`ViewCamera::initial`]).
    cameras: HashMap<View3d, ViewCamera>,
    /// The other views of the layout (View ▸ Switch View Layout), in order;
    /// the active view is at `active`.
    others: Vec<View3d>,
    active: usize,
}

impl Views {
    /// Views in the layout: 1, 2 or 4.
    pub fn layout(&self) -> usize {
        self.others.len() + 1
    }

    /// 1, 2 or 4 views. The active view keeps its 3D view and comes last;
    /// the others start as AE's (Top; Top, Front, Right, 待核).
    pub fn set_layout(&mut self, n: usize) {
        use OrthoView::*;
        let others: &[View3d] = match n {
            2 => &[View3d::Ortho(Top)],
            4 => &[
                View3d::Ortho(Top),
                View3d::Ortho(Front),
                View3d::Ortho(Right),
            ],
            _ => &[],
        };
        self.others = others.to_vec();
        self.active = others.len();
    }

    /// The 3D view of each view of the layout, in order.
    pub fn panes(&self) -> Vec<View3d> {
        let mut v = self.others.clone();
        v.insert(self.active.min(v.len()), self.current);
        v
    }

    /// Index of the active view.
    pub fn active(&self) -> usize {
        self.active
    }

    /// Make view `i` of the layout the active one (a click in it).
    pub fn activate(&mut self, i: usize) {
        let mut panes = self.panes();
        if i >= panes.len() || i == self.active {
            return;
        }
        self.current = panes.remove(i);
        self.others = panes;
        self.active = i;
    }

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
        self.camera_of(self.current, size)
    }

    /// The camera `view` shows the comp with; `None` for Active Camera.
    pub fn camera_of(&self, view: View3d, size: DVec2) -> Option<Camera> {
        (view != View3d::Active).then(|| self.view_camera(view, size).camera(view))
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
pub(crate) fn handle(inv: &Invocation, cx: &PanelContext<'_>) -> bool {
    if !ACTIONS.iter().any(|(id, _)| *id == inv.id) {
        return false;
    }
    let session = common::session(cx.resources);
    let mut s = session.lock();
    let Some(comp) = s.active_comp.clone() else {
        return true;
    };
    let shortcuts = s.view_shortcuts;
    let views = s.views3d.entry(comp).or_default();
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
        ids::RESET => {
            let current = views.current;
            views.cameras.remove(&current);
        }
        ids::ASSIGN => {
            let current = views.current;
            s.view_shortcuts.0[slot(inv)] = current;
        }
        ids::LAYOUT => {
            let n = inv
                .params
                .get("views")
                .and_then(|v| v.as_u64())
                .unwrap_or(1);
            views.set_layout(n as usize);
        }
        _ => {}
    }
    true
}

/// The active comp's view layout: 1, 2 or 4 views.
pub(crate) fn layout(resources: &bae_app::Resources) -> usize {
    let session = common::session(resources);
    let s = session.lock();
    s.active_comp
        .as_ref()
        .and_then(|c| s.views3d.get(c))
        .map_or(1, Views::layout)
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
