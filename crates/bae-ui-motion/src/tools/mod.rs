//! Tools of the Motion workspace (AE-UI-300): the current tool, tool groups with
//! flyouts, tool shortcuts (tap to switch, repeat to cycle a group, hold to switch
//! only while the key is down), and the options of the shape, pen, type and other
//! tools shown right of the tool buttons (AE-UI-100).
//!
//! The state is an app resource, [`SharedTools`], that the toolbar
//! ([`toolbar::AeToolbar`], registered with the shell as the `ae` keymap's
//! [`bae_ui::Toolbar`]) changes and the viewer and other panels read. Like AE's
//! (`app.project.toolType`), it is application state: saved with the workbench
//! state, not in the project.

pub(crate) mod cursor;
pub mod toolbar;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use bae_ui::icons::Icon;
use bae_ui::{ActionHandler, Binding, Invocation, KeymapBindings, PanelContext};
use serde::{Deserialize, Serialize};

/// Every tool of the AE 26 toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Selection,
    Hand,
    Zoom,
    OrbitCursor,
    OrbitScene,
    OrbitPoi,
    PanCursor,
    PanPoi,
    DollyCursor,
    DollyTo,
    DollyPoi,
    Rotation,
    PanBehind,
    Rectangle,
    RoundedRectangle,
    Ellipse,
    Polygon,
    Star,
    Cube,
    Sphere,
    Plane,
    Torus,
    Cylinder,
    Cone,
    Pen,
    AddVertex,
    DeleteVertex,
    ConvertVertex,
    MaskFeather,
    TypeHorizontal,
    TypeVertical,
    Brush,
    CloneStamp,
    Eraser,
    RotoBrush,
    RefineEdge,
    PuppetPosition,
    PuppetStarch,
    PuppetBend,
    PuppetAdvanced,
    PuppetOverlap,
}

/// A toolbar button: one tool or a group of tools sharing a button with a flyout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    Selection,
    Hand,
    Zoom,
    Orbit,
    Pan,
    Dolly,
    Rotation,
    PanBehind,
    Shape,
    Mesh,
    Pen,
    Type,
    Brush,
    CloneStamp,
    Eraser,
    Roto,
    Puppet,
}

impl Group {
    /// Toolbar order, left to right (after the Home button).
    pub const ALL: [Group; 17] = [
        Group::Selection,
        Group::Hand,
        Group::Zoom,
        Group::Orbit,
        Group::Pan,
        Group::Dolly,
        Group::Rotation,
        Group::PanBehind,
        Group::Shape,
        Group::Mesh,
        Group::Pen,
        Group::Type,
        Group::Brush,
        Group::CloneStamp,
        Group::Eraser,
        Group::Roto,
        Group::Puppet,
    ];

    /// Members in flyout order; the first is the default.
    pub fn members(self) -> &'static [Tool] {
        use Tool::*;
        match self {
            Group::Selection => &[Selection],
            Group::Hand => &[Hand],
            Group::Zoom => &[Zoom],
            Group::Orbit => &[OrbitCursor, OrbitScene, OrbitPoi],
            Group::Pan => &[PanCursor, PanPoi],
            Group::Dolly => &[DollyCursor, DollyTo, DollyPoi],
            Group::Rotation => &[Rotation],
            Group::PanBehind => &[PanBehind],
            Group::Shape => &[Rectangle, RoundedRectangle, Ellipse, Polygon, Star],
            Group::Mesh => &[Cube, Sphere, Plane, Torus, Cylinder, Cone],
            Group::Pen => &[Pen, AddVertex, DeleteVertex, ConvertVertex, MaskFeather],
            Group::Type => &[TypeHorizontal, TypeVertical],
            Group::Brush => &[Brush],
            Group::CloneStamp => &[CloneStamp],
            Group::Eraser => &[Eraser],
            Group::Roto => &[RotoBrush, RefineEdge],
            Group::Puppet => &[
                PuppetPosition,
                PuppetStarch,
                PuppetBend,
                PuppetAdvanced,
                PuppetOverlap,
            ],
        }
    }

    /// Members a repeated shortcut cycles through (AE: G cycles Pen and Mask
    /// Feather only; the vertex tools come from the flyout or modifiers).
    fn cycle(self) -> &'static [Tool] {
        match self {
            Group::Pen => &[Tool::Pen, Tool::MaskFeather],
            g => g.members(),
        }
    }
}

impl Tool {
    pub fn group(self) -> Group {
        Group::ALL
            .into_iter()
            .find(|g| g.members().contains(&self))
            .unwrap_or(Group::Selection)
    }

    /// English name (translated with `tr`), as in AE's tooltips.
    pub fn name(self) -> &'static str {
        use Tool::*;
        match self {
            Selection => "Selection Tool",
            Hand => "Hand Tool",
            Zoom => "Zoom Tool",
            OrbitCursor => "Orbit Around Cursor Tool",
            OrbitScene => "Orbit Around Scene Tool",
            OrbitPoi => "Orbit Around Camera POI Tool",
            PanCursor => "Pan Under Cursor Tool",
            PanPoi => "Pan Camera POI Tool",
            DollyCursor => "Dolly Towards Cursor Tool",
            DollyTo => "Dolly to Cursor Tool",
            DollyPoi => "Dolly to Camera POI Tool",
            Rotation => "Rotation Tool",
            PanBehind => "Pan Behind (Anchor Point) Tool",
            Rectangle => "Rectangle Tool",
            RoundedRectangle => "Rounded Rectangle Tool",
            Ellipse => "Ellipse Tool",
            Polygon => "Polygon Tool",
            Star => "Star Tool",
            Cube => "Cube Tool",
            Sphere => "Sphere Tool",
            Plane => "Plane Tool",
            Torus => "Torus Tool",
            Cylinder => "Cylinder Tool",
            Cone => "Cone Tool",
            Pen => "Pen Tool",
            AddVertex => "Add Vertex Tool",
            DeleteVertex => "Delete Vertex Tool",
            ConvertVertex => "Convert Vertex Tool",
            MaskFeather => "Mask Feather Tool",
            TypeHorizontal => "Horizontal Type Tool",
            TypeVertical => "Vertical Type Tool",
            Brush => "Brush Tool",
            CloneStamp => "Clone Stamp Tool",
            Eraser => "Eraser Tool",
            RotoBrush => "Roto Brush Tool",
            RefineEdge => "Refine Edge Tool",
            PuppetPosition => "Puppet Position Pin Tool",
            PuppetStarch => "Puppet Starch Pin Tool",
            PuppetBend => "Puppet Bend Pin Tool",
            PuppetAdvanced => "Puppet Advanced Pin Tool",
            PuppetOverlap => "Puppet Overlap Pin Tool",
        }
    }

    pub fn icon(self) -> Icon {
        use Tool::*;
        match self {
            Selection => Icon::Selection,
            Hand => Icon::Hand,
            Zoom => Icon::Zoom,
            OrbitCursor | OrbitScene | OrbitPoi => Icon::Orbit,
            PanCursor | PanPoi => Icon::PanCamera,
            DollyCursor | DollyTo | DollyPoi => Icon::Dolly,
            Rotation => Icon::Rotate,
            PanBehind => Icon::PanBehind,
            Rectangle => Icon::Rectangle,
            RoundedRectangle => Icon::RoundedRectangle,
            Ellipse => Icon::Ellipse,
            Polygon => Icon::Polygon,
            Star => Icon::StarTool,
            Cube => Icon::Extrude,
            Sphere => Icon::Sphere,
            Plane => Icon::Plane,
            Torus => Icon::Torus,
            Cylinder => Icon::Cylinder,
            Cone => Icon::Cone,
            Pen => Icon::Pen,
            AddVertex => Icon::PenAdd,
            DeleteVertex => Icon::PenDelete,
            ConvertVertex => Icon::PenConvert,
            MaskFeather => Icon::MaskFeather,
            TypeHorizontal => Icon::Type,
            TypeVertical => Icon::TypeVertical,
            Brush => Icon::Brush,
            CloneStamp => Icon::CloneStamp,
            Eraser => Icon::Eraser,
            RotoBrush => Icon::RotoBrush,
            RefineEdge => Icon::RefineEdge,
            PuppetPosition => Icon::Puppet,
            PuppetStarch => Icon::PuppetStarch,
            PuppetBend => Icon::PuppetBend,
            PuppetAdvanced => Icon::PuppetAdvanced,
            PuppetOverlap => Icon::PuppetOverlap,
        }
    }

    /// Rectangle, Rounded Rectangle, Ellipse, Polygon, Star.
    pub fn is_shape(self) -> bool {
        self.group() == Group::Shape
    }

    /// Pen, Add / Delete / Convert Vertex, Mask Feather.
    pub fn is_pen(self) -> bool {
        self.group() == Group::Pen
    }

    pub fn is_type(self) -> bool {
        self.group() == Group::Type
    }

    pub fn is_camera(self) -> bool {
        matches!(self.group(), Group::Orbit | Group::Pan | Group::Dolly)
    }

    /// Tools that do something in the Composition viewer in this version. The
    /// others can be chosen (AE does too) but clicks in the viewer do nothing.
    pub fn acts_in_viewer(self) -> bool {
        matches!(
            self.group(),
            Group::Selection | Group::Hand | Group::Zoom | Group::Rotation | Group::PanBehind
        ) || self.is_shape()
            || self.is_pen()
            || self.is_camera()
            // Vertical type needs a text direction in the model (not yet).
            || self == Tool::TypeHorizontal
    }
}

/// What a tool shortcut does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shortcut {
    /// Activate this tool.
    Tool(Tool),
    /// Activate the group's last used tool; pressed again while a member is
    /// active, go to the next member (`cycle`) or stay (`!cycle`).
    Group { group: Group, cycle: bool },
    /// Ctrl/Cmd+B: Brush → Clone Stamp → Eraser.
    Paint,
    /// C: the Orbit → Pan → Dolly camera groups (each with its last used tool).
    Camera,
}

/// Tool action ids (`motion.tool.*`, the AE `CEggAppTool` context).
pub mod ids {
    pub const SELECTION: &str = "motion.tool.selection";
    pub const HAND: &str = "motion.tool.hand";
    pub const ZOOM: &str = "motion.tool.zoom";
    pub const ROTATION: &str = "motion.tool.rotation";
    pub const PAN_BEHIND: &str = "motion.tool.pan_behind";
    pub const SHAPE: &str = "motion.tool.shape";
    pub const SHAPE_CYCLE: &str = "motion.tool.shape_cycle";
    pub const PEN: &str = "motion.tool.pen";
    pub const PEN_CYCLE: &str = "motion.tool.pen_cycle";
    pub const TYPE: &str = "motion.tool.type";
    pub const PAINT: &str = "motion.tool.paint";
    pub const PUPPET: &str = "motion.tool.puppet";
    pub const ROTO_BRUSH: &str = "motion.tool.roto_brush";
    pub const CAMERA: &str = "motion.tool.camera";
    pub const CAMERA_CYCLE: &str = "motion.tool.camera_cycle";
    pub const CAMERA_ORBIT: &str = "motion.tool.camera_orbit";
    pub const CAMERA_ORBIT_CYCLE: &str = "motion.tool.camera_orbit_cycle";
    pub const CAMERA_PAN: &str = "motion.tool.camera_pan";
    pub const CAMERA_PAN_CYCLE: &str = "motion.tool.camera_pan_cycle";
    pub const CAMERA_DOLLY: &str = "motion.tool.camera_dolly";
    pub const CAMERA_DOLLY_CYCLE: &str = "motion.tool.camera_dolly_cycle";
    pub const PARAMETRIC_MESH: &str = "motion.tool.parametric_mesh";
    /// Arrow keys: nudge the selected layers (or mask vertices) by `{dx, dy}`
    /// screen pixels at the viewer's magnification (AE `NudgeUp` …, id from the
    /// command catalog).
    pub const NUDGE: &str = "motion.layer.nudge_position";
    /// View > Resolution ▸ Full / Half / Third / Quarter (`{factor}`): the active
    /// comp's resolution (Cmd/Ctrl+J, Cmd/Ctrl+Shift+J, Cmd/Ctrl+Alt+Shift+J).
    pub const RESOLUTION: &str = "motion.view.resolution";
    /// Layer > Mask and Shape Path > Free Transform Points (catalog id).
    pub const FREE_TRANSFORM: &str = "motion.layer.free_transform_points";
}

/// `(action id, English title, default chord, what it does)`; chords from AE 26's
/// default shortcuts (`docs/reference/ae/shortcuts.json`, `CEggAppTool`).
const SHORTCUTS: &[(&str, &str, &str, Shortcut)] = &[
    (
        ids::SELECTION,
        "Selection Tool",
        "V",
        Shortcut::Tool(Tool::Selection),
    ),
    (ids::HAND, "Hand Tool", "H", Shortcut::Tool(Tool::Hand)),
    (ids::ZOOM, "Zoom Tool", "Z", Shortcut::Tool(Tool::Zoom)),
    (
        ids::ROTATION,
        "Rotation Tool",
        "W",
        Shortcut::Tool(Tool::Rotation),
    ),
    (
        ids::PAN_BEHIND,
        "Pan Behind (Anchor Point) Tool",
        "Y",
        Shortcut::Tool(Tool::PanBehind),
    ),
    (
        ids::SHAPE,
        "Shape Tools",
        "Q",
        Shortcut::Group {
            group: Group::Shape,
            cycle: true,
        },
    ),
    (
        ids::SHAPE_CYCLE,
        "Cycle Shape Tools",
        "Shift+Q",
        Shortcut::Group {
            group: Group::Shape,
            cycle: true,
        },
    ),
    (
        ids::PEN,
        "Pen Tool",
        "G",
        Shortcut::Group {
            group: Group::Pen,
            cycle: true,
        },
    ),
    (
        ids::PEN_CYCLE,
        "Cycle Pen Tools",
        "Shift+G",
        Shortcut::Group {
            group: Group::Pen,
            cycle: true,
        },
    ),
    (
        ids::TYPE,
        "Type Tools",
        "Cmd+T",
        Shortcut::Group {
            group: Group::Type,
            cycle: true,
        },
    ),
    (
        ids::PAINT,
        "Brush, Clone Stamp and Eraser Tools",
        "Cmd+B",
        Shortcut::Paint,
    ),
    (
        ids::PUPPET,
        "Puppet Tools",
        "Cmd+P",
        Shortcut::Group {
            group: Group::Puppet,
            cycle: true,
        },
    ),
    (
        ids::ROTO_BRUSH,
        "Roto Brush and Refine Edge Tools",
        "Alt+W",
        Shortcut::Group {
            group: Group::Roto,
            cycle: true,
        },
    ),
    (ids::CAMERA, "Camera Tools", "C", Shortcut::Camera),
    (
        ids::CAMERA_CYCLE,
        "Cycle Camera Tools",
        "Shift+C",
        Shortcut::Camera,
    ),
    (
        ids::CAMERA_ORBIT,
        "Orbit Camera Tools",
        "1",
        Shortcut::Group {
            group: Group::Orbit,
            cycle: false,
        },
    ),
    (
        ids::CAMERA_ORBIT_CYCLE,
        "Cycle Orbit Camera Tools",
        "Shift+1",
        Shortcut::Group {
            group: Group::Orbit,
            cycle: true,
        },
    ),
    (
        ids::CAMERA_PAN,
        "Pan Camera Tools",
        "2",
        Shortcut::Group {
            group: Group::Pan,
            cycle: false,
        },
    ),
    (
        ids::CAMERA_PAN_CYCLE,
        "Cycle Pan Camera Tools",
        "Shift+2",
        Shortcut::Group {
            group: Group::Pan,
            cycle: true,
        },
    ),
    (
        ids::CAMERA_DOLLY,
        "Dolly Camera Tools",
        "3",
        Shortcut::Group {
            group: Group::Dolly,
            cycle: false,
        },
    ),
    (
        ids::CAMERA_DOLLY_CYCLE,
        "Cycle Dolly Camera Tools",
        "Shift+3",
        Shortcut::Group {
            group: Group::Dolly,
            cycle: true,
        },
    ),
    (
        ids::PARAMETRIC_MESH,
        "3D Parametric Mesh Tools",
        "Shift+D",
        Shortcut::Group {
            group: Group::Mesh,
            cycle: true,
        },
    ),
];

/// Arrow-key nudges: `(chord, dx, dy, English title)` (AE `CCompCompCmd`
/// `NudgeUp` … `NudgeRightMore`: 1 pixel at the current magnification, Shift 10).
const NUDGES: &[(&str, i32, i32, &str)] = &[
    ("ArrowUp", 0, -1, "Nudge Up"),
    ("ArrowDown", 0, 1, "Nudge Down"),
    ("ArrowLeft", -1, 0, "Nudge Left"),
    ("ArrowRight", 1, 0, "Nudge Right"),
    ("Shift+ArrowUp", 0, -10, "Nudge Up 10 Pixels"),
    ("Shift+ArrowDown", 0, 10, "Nudge Down 10 Pixels"),
    ("Shift+ArrowLeft", -10, 0, "Nudge Left 10 Pixels"),
    ("Shift+ArrowRight", 10, 0, "Nudge Right 10 Pixels"),
];

fn nudge_params(dx: i32, dy: i32) -> serde_json::Value {
    serde_json::json!({ "dx": dx, "dy": dy })
}

/// `(id, English title)` of the tool actions, for the shortcut editor.
pub fn actions() -> impl Iterator<Item = (&'static str, &'static str)> {
    SHORTCUTS
        .iter()
        .map(|(id, title, ..)| (*id, *title))
        .chain([
            (ids::NUDGE, "Nudge Layer Position"),
            (ids::RESOLUTION, "Resolution"),
            (ids::FREE_TRANSFORM, "Free Transform Points"),
        ])
        .chain(crate::viewer::view3d::ACTIONS.iter().copied())
        .chain(crate::viewer::guides::ACTIONS.iter().copied())
        .chain(GIZMOS.iter().map(|(id, title, ..)| (*id, *title)))
}

/// AE's resolution shortcuts: `(params, English title)` (AE's names).
const RESOLUTIONS: &[(&str, &str)] = &[
    (r#"{"factor":1}"#, "Set Composition Resolution to High"),
    (r#"{"factor":2}"#, "Set Composition Resolution to Half"),
    (r#"{"factor":4}"#, "Set Composition Resolution to Low"),
    (
        r#"{"factor":1,"nested":true}"#,
        "Set Composition Resolution to High for the Focused and Nested Comps",
    ),
    (
        r#"{"factor":2,"nested":true}"#,
        "Set Composition Resolution to Half for the Focused and Nested Comps",
    ),
    (
        r#"{"factor":4,"nested":true}"#,
        "Set Composition Resolution to Low for the Focused and Nested Comps",
    ),
];

/// The variants of the nudge and resolution actions (one row per shortcut in the
/// shortcut editor).
pub fn variants() -> Vec<(String, bae_ui::CommandVariant)> {
    let nudges = NUDGES
        .iter()
        .map(|(_, dx, dy, title)| (ids::NUDGE, nudge_params(*dx, *dy), *title));
    let resolutions = RESOLUTIONS.iter().map(|(p, title)| {
        let params: serde_json::Value = serde_json::from_str(p).expect("JSON");
        (ids::RESOLUTION, params, *title)
    });
    let views = crate::viewer::view3d::SHORTCUT_NAMES
        .iter()
        .map(|(slot, title)| {
            (
                crate::viewer::view3d::ids::SHORTCUT,
                serde_json::json!({ "slot": slot }),
                *title,
            )
        });
    nudges
        .chain(resolutions)
        .chain(views)
        .map(|(id, params, title)| {
            (
                format!("{id}/{params}"),
                bae_ui::CommandVariant::new(id, params, title.to_owned()),
            )
        })
        .collect()
}

/// The tool shortcuts and arrow-key nudges, added to the `ae` keymap.
pub fn bindings() -> Vec<Binding> {
    SHORTCUTS
        .iter()
        .map(|(id, _, chord, _)| Binding::new(chord, id))
        .chain(NUDGES.iter().map(|(chord, dx, dy, _)| {
            Binding::new(chord, ids::NUDGE).params(nudge_params(*dx, *dy))
        }))
        // Esc is Switch to Last 3D View whichever panel has focus (AE). Text
        // fields and open menus keep their Esc (the workbench); the viewer
        // passes it on to a drag or text it is editing.
        .chain([Binding::new("Esc", crate::viewer::view3d::ids::LAST)])
        .chain(GIZMOS.iter().map(|(id, _, chord, _)| Binding::new(chord, id)))
        .collect()
}

/// The keymap extension holding [`bindings`].
pub fn keymap_bindings() -> KeymapBindings {
    KeymapBindings::new(bae_ui::presets::AE_KEYMAP, bindings())
        .for_workspace(crate::menus::WORKSPACE)
}

/// A tool switched on while its key is held (AE: hold a tool's key to use it,
/// release to get the previous tool back).
#[derive(Clone, Debug)]
struct Momentary {
    /// The action whose key is held.
    action: String,
    /// Tool to return to.
    previous: Tool,
    since: Instant,
    /// The key was seen down after the switch (not just tapped).
    held: bool,
    /// The pointer was pressed while the key was down.
    used: bool,
}

/// Holding a tool key longer than this returns to the previous tool on release
/// even without a click (spec AE-UI-300, BAE decision; AE's exact rule 待核).
const SPRING_HOLD: Duration = Duration::from_millis(250);

/// Fill or stroke kind (Fill Options / Stroke Options dialog).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaintKind {
    None,
    #[default]
    Solid,
    LinearGradient,
    RadialGradient,
}

/// Fill or stroke settings for new shapes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Paint {
    pub kind: PaintKind,
    /// Straight RGBA, 0–1.
    pub color: [f64; 4],
    /// Percent.
    pub opacity: f64,
    /// Blend mode id (`normal`, `multiply` …).
    pub blend_mode: String,
}

impl Default for Paint {
    fn default() -> Self {
        Paint {
            kind: PaintKind::Solid,
            color: [1.0, 1.0, 1.0, 1.0],
            opacity: 100.0,
            blend_mode: "normal".into(),
        }
    }
}

/// Axis mode for 3D layers (Local / World / View).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisMode {
    #[default]
    Local,
    World,
    View,
}

/// Paint and Brushes panel settings for the next stroke (AE-UI-104); each
/// stroke keeps its own copy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaintOptions {
    pub foreground: [f64; 3],
    pub background: [f64; 3],
    /// Percent.
    pub opacity: f64,
    pub flow: f64,
    /// A layer blend mode token.
    pub mode: String,
    /// `rgba`, `rgb` or `alpha`.
    pub channels: String,
    /// `constant`, `write_on`, `single_frame` or `custom`.
    pub duration: String,
    /// Custom duration in frames.
    pub frames: u32,
    /// Eraser: `layer_and_paint`, `paint` or `last_stroke`.
    pub erase: String,
    /// Clone Stamp: the source layer (none: the layer painted on).
    pub clone_source: Option<bae_model::PrimPath>,
    pub aligned: bool,
    pub lock_source_time: bool,
    /// Clone Stamp: source point minus the stroke's start (pixels).
    pub clone_offset: [f64; 2],
    /// Clone Stamp: source time shift in seconds.
    pub clone_time_shift: f64,
    /// Brush tip (Brushes panel): pixels, degrees, percents.
    pub diameter: f64,
    pub angle: f64,
    pub roundness: f64,
    pub hardness: f64,
    pub spacing: f64,
    /// Brush Dynamics of Size, Angle, Roundness, Opacity, Flow: 0 Off, 1 Pen
    /// Pressure, 2 Pen Tilt, 3 Stylus Wheel.
    pub dynamics: [u8; 5],
    pub minimum_size: f64,
}

impl Default for PaintOptions {
    fn default() -> Self {
        PaintOptions {
            foreground: [0.0, 0.0, 0.0],
            background: [1.0, 1.0, 1.0],
            opacity: 100.0,
            flow: 100.0,
            mode: "normal".to_owned(),
            channels: "rgba".to_owned(),
            duration: "constant".to_owned(),
            frames: 1,
            erase: "layer_and_paint".to_owned(),
            clone_source: None,
            aligned: true,
            lock_source_time: false,
            clone_offset: [0.0, 0.0],
            clone_time_shift: 0.0,
            diameter: 21.0,
            angle: 0.0,
            roundness: 100.0,
            hardness: 100.0,
            spacing: 25.0,
            dynamics: [1, 0, 0, 0, 0],
            minimum_size: 1.0,
        }
    }
}

/// What the Selection tool's 3D gizmo does (AE's 3D Gizmo modes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GizmoMode {
    #[default]
    Position,
    Scale,
    Rotation,
}

/// AE's 3D Gizmo shortcuts: `(action id, English title, chord, mode)`; `None`
/// goes to the next mode.
const GIZMOS: &[(&str, &str, &str, Option<GizmoMode>)] = &[
    (
        "motion.tool.gizmo_position",
        "3D Gizmo: Position",
        "4",
        Some(GizmoMode::Position),
    ),
    (
        "motion.tool.gizmo_scale",
        "3D Gizmo: Scale",
        "5",
        Some(GizmoMode::Scale),
    ),
    (
        "motion.tool.gizmo_rotate",
        "3D Gizmo: Rotation",
        "6",
        Some(GizmoMode::Rotation),
    ),
    (
        "motion.tool.gizmo_toggle",
        "Toggle 3D Gizmo Mode",
        "Shift+V",
        None,
    ),
];

/// Options of the tools, shown right of the tool buttons. First-run values follow
/// the spec's best understanding of AE's (AE-SHP-057, 待核).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolOptions {
    pub fill: Paint,
    pub stroke: Paint,
    /// Stroke width in pixels.
    pub stroke_width: f64,
    /// Shape tools create Bezier paths instead of parametric shapes.
    pub bezier_path: bool,
    /// Pen: RotoBezier.
    pub roto_bezier: bool,
    /// Tool Creates Mask (on a selected shape layer) instead of Tool Creates Shape.
    pub creates_mask: bool,
    /// Snapping (Selection, Rotation, Pan Behind, shape and pen tools).
    pub snapping: bool,
    /// Type and paint tools: open the Character / Paragraph (Paint / Brushes) panels.
    pub auto_open_panels: bool,
    pub axis_mode: AxisMode,
    pub gizmo: GizmoMode,
    /// Rotation tool on 3D layers: set Orientation (true) or Rotation.
    pub rotation_sets_orientation: bool,
    /// Rounded Rectangle tool: last roundness in pixels.
    pub roundness: f64,
    /// Polygon and Star tools: last number of points.
    pub points: f64,
    /// Paint and Brushes panels.
    pub paint: PaintOptions,
    /// Puppet tools: show the mesh, expansion and density.
    pub puppet_show_mesh: bool,
    pub puppet_expansion: f64,
    pub puppet_density: f64,
}

impl Default for ToolOptions {
    fn default() -> Self {
        ToolOptions {
            fill: Paint {
                color: [1.0, 0.0, 0.0, 1.0],
                ..Paint::default()
            },
            stroke: Paint::default(),
            stroke_width: 2.0,
            bezier_path: false,
            roto_bezier: false,
            creates_mask: false,
            snapping: false,
            auto_open_panels: true,
            axis_mode: AxisMode::Local,
            gizmo: GizmoMode::Position,
            rotation_sets_orientation: true,
            roundness: 10.0,
            points: 5.0,
            paint: PaintOptions::default(),
            puppet_show_mesh: false,
            puppet_expansion: 3.0,
            puppet_density: 10.0,
        }
    }
}

/// A viewer change requested from the toolbar (double-clicking the Hand or Zoom
/// tool), applied by the Composition viewer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewRequest {
    /// Double-click Hand: center the composition in the viewer.
    Center,
    /// Double-click Zoom: magnification 100%.
    Actual,
}

/// A double-click on a shape tool button (AE-SHP-054), applied by the viewer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FillRequest {
    pub tool: Tool,
    /// Shift was held: the largest square / circle that fits.
    pub shift: bool,
}

/// The tool state (an app resource through [`SharedTools`]).
#[derive(Debug)]
pub struct ToolState {
    current: Tool,
    /// Last used member of each group (the button shows it; shortcuts return to it).
    last: HashMap<Group, Tool>,
    momentary: Option<Momentary>,
    pub options: ToolOptions,
    /// For the viewer (see [`ViewRequest`]).
    pub view_request: Option<ViewRequest>,
    /// A shape tool button was double-clicked: the viewer adds a layer- or
    /// comp-sized shape or mask with this tool.
    pub fill_request: Option<FillRequest>,
    /// Cmd/Ctrl double-click on the Pan Behind tool: center the anchor points.
    pub center_anchor_request: bool,
    /// Edit this text layer in the viewer now (Layer > New > Text): the viewer
    /// switches to the Horizontal Type tool and puts the caret in it.
    pub edit_text_request: Option<bae_model::PrimPath>,
    /// Free Transform Points was chosen: the viewer puts a box on the selected
    /// mask path (its selected vertices, or all).
    pub free_transform_request: bool,
    /// Magnification of the Composition viewer (1 = 100%), for the arrow keys.
    pub magnification: f32,
}

impl Default for ToolState {
    fn default() -> Self {
        ToolState {
            current: Tool::Selection,
            last: HashMap::new(),
            momentary: None,
            options: ToolOptions::default(),
            view_request: None,
            fill_request: None,
            center_anchor_request: false,
            edit_text_request: None,
            free_transform_request: false,
            magnification: 1.0,
        }
    }
}

/// What the toolbar saves between sessions.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    tool: Option<Tool>,
    last: Vec<Tool>,
    options: ToolOptions,
}

impl ToolState {
    /// The active tool.
    pub fn current(&self) -> Tool {
        self.current
    }

    /// The tool a group's button shows.
    pub fn shown(&self, group: Group) -> Tool {
        if self.current.group() == group {
            return self.current;
        }
        self.last.get(&group).copied().unwrap_or(group.members()[0])
    }

    /// Make `tool` the active tool (toolbar click, flyout, shortcut).
    pub fn set(&mut self, tool: Tool) {
        self.current = tool;
        self.last.insert(tool.group(), tool);
        self.momentary = None;
    }

    /// The member after the one `group`'s button shows: every member for an
    /// Alt/Option-click on the button (`all`), the shortcut's cycle otherwise.
    pub fn next_in_group(&self, group: Group, all: bool) -> Tool {
        let list = if all { group.members() } else { group.cycle() };
        let shown = self.shown(group);
        match list.iter().position(|t| *t == shown) {
            Some(i) => list[(i + 1) % list.len()],
            None => list[0],
        }
    }

    fn apply(&self, shortcut: Shortcut) -> Tool {
        match shortcut {
            Shortcut::Tool(t) => t,
            Shortcut::Group { group, cycle } => {
                if cycle && self.current.group() == group {
                    self.next_in_group(group, false)
                } else {
                    self.shown(group)
                }
            }
            Shortcut::Paint => match self.current {
                Tool::Brush => Tool::CloneStamp,
                Tool::CloneStamp => Tool::Eraser,
                _ => Tool::Brush,
            },
            Shortcut::Camera => {
                let next = match self.current.group() {
                    Group::Orbit => Group::Pan,
                    Group::Pan => Group::Dolly,
                    Group::Dolly => Group::Orbit,
                    _ => self
                        .last
                        .keys()
                        .copied()
                        .find(|g| matches!(g, Group::Orbit | Group::Pan | Group::Dolly))
                        .unwrap_or(Group::Orbit),
                };
                self.shown(next)
            }
        }
    }

    /// Run a tool shortcut (`motion.tool.*`). Returns false for other actions.
    pub fn shortcut(&mut self, action: &str) -> bool {
        let Some((_, _, _, what)) = SHORTCUTS.iter().find(|(id, ..)| *id == action) else {
            return false;
        };
        // Key repeat while the key is held: keep the tool.
        if self
            .momentary
            .as_ref()
            .is_some_and(|m| m.action == action && m.held)
        {
            return true;
        }
        let previous = self.momentary.as_ref().map_or(self.current, |m| m.previous);
        let tool = self.apply(*what);
        self.set(tool);
        self.momentary = Some(Momentary {
            action: action.to_owned(),
            previous,
            since: Instant::now(),
            held: false,
            used: false,
        });
        true
    }

    /// Called every frame with whether a key of the momentary tool's action is
    /// down and whether the pointer was pressed; returns to the previous tool when
    /// the key is released after being held or used. Returns true while a
    /// momentary tool is pending (the caller keeps repainting until the release).
    pub fn spring(&mut self, key_down: impl Fn(&str) -> bool, pointer_pressed: bool) -> bool {
        let Some(m) = &mut self.momentary else {
            return false;
        };
        if key_down(&m.action) {
            m.held = true;
            m.used |= pointer_pressed;
            return true;
        }
        let m = self.momentary.take().expect("checked above");
        if m.held && (m.used || m.since.elapsed() >= SPRING_HOLD) && m.previous != self.current {
            let previous = m.previous;
            self.current = previous;
            self.last.insert(previous.group(), previous);
        }
        false
    }

    /// The action whose key is held for a momentary tool.
    pub fn momentary_action(&self) -> Option<&str> {
        self.momentary.as_ref().map(|m| m.action.as_str())
    }

    fn save(&self) -> serde_json::Value {
        let saved = Saved {
            tool: Some(self.current),
            last: self.last.values().copied().collect(),
            options: self.options.clone(),
        };
        serde_json::to_value(saved).unwrap_or_default()
    }

    fn load(&mut self, value: &serde_json::Value) {
        let Ok(saved) = serde_json::from_value::<Saved>(value.clone()) else {
            return;
        };
        for t in saved.last {
            self.last.insert(t.group(), t);
        }
        if let Some(t) = saved.tool {
            self.set(t);
        }
        self.options = saved.options;
    }
}

/// The tool state as an app resource (`Resources::get::<SharedTools>()`).
#[derive(Clone, Debug, Default)]
pub struct SharedTools(Arc<Mutex<ToolState>>);

impl SharedTools {
    pub fn lock(&self) -> MutexGuard<'_, ToolState> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The active tool.
    pub fn current(&self) -> Tool {
        self.lock().current
    }
}

/// The tools of the app (a default state when the module is not loaded, e.g. in
/// panel tests).
pub(crate) fn shared(resources: &bae_app::Resources) -> SharedTools {
    resources.get::<SharedTools>().cloned().unwrap_or_default()
}

/// Handles the `motion.tool.*` shortcuts whichever panel has focus.
pub struct ToolActions;

impl ActionHandler for ToolActions {
    fn actions(&self) -> Vec<String> {
        actions().map(|(id, _)| id.to_owned()).collect()
    }

    fn workspace(&self) -> Option<&str> {
        Some("motion")
    }

    fn handle(&self, inv: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        if inv.id == ids::RESOLUTION {
            let factor = inv
                .params
                .get("factor")
                .and_then(|v| v.as_u64())
                .unwrap_or(1);
            let session = crate::common::session(cx.resources);
            let nested = inv.params.get("nested").and_then(|v| v.as_bool()) == Some(true);
            crate::viewer::set_resolution(cx, &session, factor.max(1) as u32, nested);
            return true;
        }
        if inv.id == ids::NUDGE {
            // Another panel has focus (the viewer handles it itself).
            let p = |k: &str| inv.params.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let mag = f64::from(shared(cx.resources).lock().magnification);
            let session = crate::common::session(cx.resources);
            return crate::viewer::nudge_layers(cx, &session, p("dx"), p("dy"), mag);
        }
        if crate::viewer::view3d::handle(inv, cx) || crate::viewer::guides::handle(inv, cx) {
            return true;
        }
        if let Some((.., mode)) = GIZMOS.iter().find(|(id, ..)| *id == inv.id) {
            let tools = shared(cx.resources);
            let mut t = tools.lock();
            let o = &mut t.options;
            o.gizmo = mode.unwrap_or(match o.gizmo {
                GizmoMode::Position => GizmoMode::Scale,
                GizmoMode::Scale => GizmoMode::Rotation,
                GizmoMode::Rotation => GizmoMode::Position,
            });
            t.set(Tool::Selection);
            return true;
        }
        if inv.id == ids::FREE_TRANSFORM {
            let tools = shared(cx.resources);
            let mut t = tools.lock();
            t.set(Tool::Selection);
            t.free_transform_request = true;
            return true;
        }
        shared(cx.resources).lock().shortcut(&inv.id)
    }
}

/// Chinese names of the tools and tool options (`docs/glossary.md`; names marked
/// 待核 in the PR are best effort until R1 checks AE 26's Chinese UI).
pub(crate) const ZH: &[(&str, &str)] = &[
    ("3D Gizmo: Position", "3D 位置 Gizmo"),
    ("3D Gizmo: Scale", "三维缩放 Gizmo"),
    ("3D Gizmo: Rotation", "三维旋转 Gizmo"),
    ("Toggle 3D Gizmo Mode", "向前循环 3D Gizmo 模式"),
    ("Orbit Around Scene Tool", "绕场景旋转工具"),
    ("Orbit Around Camera POI Tool", "绕摄像机目标点旋转工具"),
    ("Pan Camera POI Tool", "平移摄像机目标点工具"),
    ("Dolly to Cursor Tool", "推拉至光标工具"),
    ("Dolly to Camera POI Tool", "推拉至摄像机目标点工具"),
    ("Rounded Rectangle Tool", "圆角矩形工具"),
    ("Ellipse Tool", "椭圆工具"),
    ("Polygon Tool", "多边形工具"),
    ("Star Tool", "星形工具"),
    ("Sphere Tool", "球体工具"),
    ("Plane Tool", "平面工具"),
    ("Torus Tool", "圆环工具"),
    ("Cylinder Tool", "圆柱体工具"),
    ("Cone Tool", "圆锥体工具"),
    ("Add Vertex Tool", "添加“顶点”工具"),
    ("Delete Vertex Tool", "删除“顶点”工具"),
    ("Convert Vertex Tool", "转换“顶点”工具"),
    ("Mask Feather Tool", "蒙版羽化工具"),
    ("Vertical Type Tool", "直排文字工具"),
    ("Refine Edge Tool", "调整边缘工具"),
    ("Puppet Starch Pin Tool", "人偶固化控点工具"),
    ("Puppet Bend Pin Tool", "人偶弯曲控点工具"),
    ("Puppet Advanced Pin Tool", "人偶高级控点工具"),
    ("Puppet Overlap Pin Tool", "人偶重叠控点工具"),
    // Shortcut editor titles of the tool shortcuts.
    ("Shape Tools", "形状工具"),
    ("Cycle Shape Tools", "循环切换形状工具"),
    ("Cycle Pen Tools", "循环切换钢笔工具"),
    ("Type Tools", "文字工具"),
    (
        "Brush, Clone Stamp and Eraser Tools",
        "画笔、仿制图章和橡皮擦工具",
    ),
    ("Puppet Tools", "人偶工具"),
    (
        "Roto Brush and Refine Edge Tools",
        "Roto 笔刷和调整边缘工具",
    ),
    ("Camera Tools", "摄像机工具"),
    ("Cycle Camera Tools", "循环切换摄像机工具"),
    ("Orbit Camera Tools", "环绕摄像机工具"),
    ("Cycle Orbit Camera Tools", "循环切换环绕摄像机工具"),
    ("Pan Camera Tools", "平移摄像机工具"),
    ("Cycle Pan Camera Tools", "循环切换平移摄像机工具"),
    ("Dolly Camera Tools", "推拉摄像机工具"),
    ("Cycle Dolly Camera Tools", "循环切换推拉摄像机工具"),
    ("3D Parametric Mesh Tools", "3D 参数化网格工具"),
    // Tool options (AE-UI-100).
    ("Tool Creates Shape", "工具创建形状"),
    ("Tool Creates Mask", "工具创建蒙版"),
    ("Fill:", "填充："),
    ("Stroke:", "描边："),
    ("Fill Options", "填充选项"),
    ("Stroke Options", "描边选项"),
    ("Fill Color", "填充颜色"),
    ("Stroke Color", "描边颜色"),
    ("Stroke Width", "描边宽度"),
    ("px", "像素"),
    ("Bezier Path", "贝塞尔曲线路径"),
    ("RotoBezier", "RotoBezier"),
    ("Snapping", "对齐"),
    ("Auto-Open Panels", "自动打开面板"),
    ("Local Axis Mode", "本地轴模式"),
    ("World Axis Mode", "世界轴模式"),
    ("View Axis Mode", "视图轴模式"),
    ("Set:", "设置："),
    ("Orientation", "方向"),
    ("Rotation", "旋转"),
    ("Mesh:", "网格："),
    ("Show", "显示"),
    ("Expansion", "扩展"),
    ("Density", "密度"),
    ("Record Options...", "记录选项..."),
    ("Add", "添加"),
    ("None", "无"),
    ("Solid Color", "纯色"),
    ("Linear Gradient", "线性渐变"),
    ("Radial Gradient", "径向渐变"),
    ("Blend Mode", "混合模式"),
    ("Opacity", "不透明度"),
    ("Normal", "正常"),
    ("Mixed", "混合"),
    // Arrow-key nudges.
    ("Nudge Layer Position", "微移图层位置"),
    // AE's names of the resolution shortcuts.
    ("Set Composition Resolution to High", "将合成分辨率设为高"),
    ("Set Composition Resolution to Half", "将合成分辨率设为一半"),
    ("Set Composition Resolution to Low", "将合成分辨率设为低"),
    (
        "Set Composition Resolution to High for the Focused and Nested Comps",
        "将“合成分辨率”设置为“高”，以便对嵌套在其中的焦点合成和预合成进行处理",
    ),
    (
        "Set Composition Resolution to Half for the Focused and Nested Comps",
        "将“合成分辨率”设置为“一半”，以便对嵌套在其中的焦点合成和预合成进行处理",
    ),
    (
        "Set Composition Resolution to Low for the Focused and Nested Comps",
        "将“合成分辨率”设置为“低”，以便对嵌套在其中的焦点合成和预合成进行处理",
    ),
    ("Free Transform Points", "自由变换点"),
    ("Nudge Up", "向上微移"),
    ("Nudge Down", "向下微移"),
    ("Nudge Left", "向左微移"),
    ("Nudge Right", "向右微移"),
    ("Nudge Up 10 Pixels", "向上微移 10 像素"),
    ("Nudge Down 10 Pixels", "向下微移 10 像素"),
    ("Nudge Left 10 Pixels", "向左微移 10 像素"),
    ("Nudge Right 10 Pixels", "向右微移 10 像素"),
];

#[cfg(test)]
mod tests;
