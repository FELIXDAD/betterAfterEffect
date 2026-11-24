//! Prim schemas (first version): the attributes of every built-in prim type, with
//! value types, defaults and whether they can be animated.
//!
//! Conventions (see `docs/spec/data-model.md` §10):
//! - Units follow AE: percent for scale/opacity, degrees for angles, pixels for
//!   positions, flicks for times. Values import 1:1 from AE.
//! - Layer attributes (and those of its masks, effects, shape contents, markers) that
//!   carry keyframes use **layer time**; comp attributes use comp time.
//! - The first child under a comp's `layers` folder is the topmost layer (AE index 1).
//! - A layer's content comes from `source` (a comp or an `Asset`); `references` are for
//!   instancing prims from other projects.
//! - Attributes BAE does not know are kept as-is; AE-only ones use the `ae:` prefix.
//!   Effect parameters are `param:NNNN` (see [`crate::effect`]).

use serde::Serialize;

use crate::error::{ModelError, Result};
use crate::path::PrimPath;
use crate::prim::PrimType;
use crate::time::{FrameRate, Time};
use crate::value::{BezierPath, Value, ValueType};

/// A constant default value (schemas are static tables).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DefaultValue {
    None,
    Float(f64),
    Int(i64),
    Bool(bool),
    Str(&'static str),
    Enum(&'static str),
    Vec2([f64; 2]),
    Vec3([f64; 3]),
    Color([f64; 4]),
    Time(i64),
    FrameRate(u32, u32),
    EmptyPath,
}

impl DefaultValue {
    pub fn to_value(self) -> Option<Value> {
        Some(match self {
            DefaultValue::None => return None,
            DefaultValue::Float(v) => Value::Float(v),
            DefaultValue::Int(v) => Value::Int(v),
            DefaultValue::Bool(v) => Value::Bool(v),
            DefaultValue::Str(v) => Value::String(v.to_owned()),
            DefaultValue::Enum(v) => Value::Enum(v.to_owned()),
            DefaultValue::Vec2(v) => Value::Vec2(v),
            DefaultValue::Vec3(v) => Value::Vec3(v),
            DefaultValue::Color(v) => Value::Color(v),
            DefaultValue::Time(v) => Value::Time(Time(v)),
            DefaultValue::FrameRate(n, d) => Value::FrameRate(FrameRate::new(n, d)),
            DefaultValue::EmptyPath => Value::Bezier(BezierPath::default()),
        })
    }
}

/// One attribute of a prim type.
#[derive(Clone, Copy, Debug)]
pub struct AttrDef {
    pub name: &'static str,
    pub value_type: ValueType,
    pub default: DefaultValue,
    /// Can carry keyframes / expressions.
    pub animatable: bool,
    /// For `enum` attributes: the allowed tokens.
    pub options: &'static [&'static str],
    /// For `Layer` attributes: only these layer kinds have it (empty = all kinds).
    pub kinds: &'static [&'static str],
    pub doc: &'static str,
}

impl AttrDef {
    const fn new(
        name: &'static str,
        value_type: ValueType,
        default: DefaultValue,
        animatable: bool,
    ) -> Self {
        AttrDef {
            name,
            value_type,
            default,
            animatable,
            options: &[],
            kinds: &[],
            doc: "",
        }
    }
    const fn doc(mut self, doc: &'static str) -> Self {
        self.doc = doc;
        self
    }
    const fn kinds(mut self, kinds: &'static [&'static str]) -> Self {
        self.kinds = kinds;
        self
    }
    const fn fixed(mut self) -> Self {
        self.animatable = false;
        self
    }
}

const fn float(n: &'static str, v: f64) -> AttrDef {
    AttrDef::new(n, ValueType::Float, DefaultValue::Float(v), true)
}
const fn int(n: &'static str, v: i64) -> AttrDef {
    AttrDef::new(n, ValueType::Int, DefaultValue::Int(v), false)
}
const fn boolean(n: &'static str, v: bool) -> AttrDef {
    AttrDef::new(n, ValueType::Bool, DefaultValue::Bool(v), false)
}
const fn string(n: &'static str) -> AttrDef {
    AttrDef::new(n, ValueType::String, DefaultValue::None, false)
}
const fn vec2(n: &'static str, v: [f64; 2]) -> AttrDef {
    AttrDef::new(n, ValueType::Vec2, DefaultValue::Vec2(v), true)
}
const fn vec3(n: &'static str, v: [f64; 3]) -> AttrDef {
    AttrDef::new(n, ValueType::Vec3, DefaultValue::Vec3(v), true)
}
const fn color(n: &'static str, v: [f64; 4]) -> AttrDef {
    AttrDef::new(n, ValueType::Color, DefaultValue::Color(v), true)
}
const fn time(n: &'static str, flicks: i64) -> AttrDef {
    AttrDef::new(n, ValueType::Time, DefaultValue::Time(flicks), false)
}
const fn time_none(n: &'static str) -> AttrDef {
    AttrDef::new(n, ValueType::Time, DefaultValue::None, false)
}
const fn rate(n: &'static str, num: u32, den: u32) -> AttrDef {
    AttrDef::new(
        n,
        ValueType::FrameRate,
        DefaultValue::FrameRate(num, den),
        false,
    )
}
const fn prim_ref(n: &'static str) -> AttrDef {
    AttrDef::new(n, ValueType::PrimRef, DefaultValue::None, false)
}
const fn bezier(n: &'static str) -> AttrDef {
    AttrDef::new(n, ValueType::Bezier, DefaultValue::EmptyPath, true)
}
const fn choice(
    n: &'static str,
    default: &'static str,
    options: &'static [&'static str],
) -> AttrDef {
    let mut a = AttrDef::new(n, ValueType::Enum, DefaultValue::Enum(default), false);
    a.options = options;
    a
}

const SECOND: i64 = Time::FLICKS_PER_SECOND;
const WHITE: [f64; 4] = [1.0, 1.0, 1.0, 1.0];
const BLACK: [f64; 4] = [0.0, 0.0, 0.0, 1.0];

/// AE blend modes, in AE menu order.
pub const BLEND_MODES: &[&str] = &[
    "normal",
    "dissolve",
    "dancing_dissolve",
    "darken",
    "multiply",
    "color_burn",
    "classic_color_burn",
    "linear_burn",
    "darker_color",
    "add",
    "lighten",
    "screen",
    "color_dodge",
    "classic_color_dodge",
    "linear_dodge",
    "lighter_color",
    "overlay",
    "soft_light",
    "hard_light",
    "linear_light",
    "vivid_light",
    "pin_light",
    "hard_mix",
    "difference",
    "classic_difference",
    "exclusion",
    "subtract",
    "divide",
    "hue",
    "saturation",
    "color",
    "luminosity",
    "stencil_alpha",
    "stencil_luma",
    "silhouette_alpha",
    "silhouette_luma",
    "alpha_add",
    "luminescent_premul",
];
pub const LAYER_KINDS: &[&str] = &[
    "shape",
    "text",
    "solid",
    "footage",
    "null",
    "camera",
    "light",
    "adjustment",
    "pre_comp",
    "audio",
];
const VISUAL: &[&str] = &[
    "shape",
    "text",
    "solid",
    "footage",
    "adjustment",
    "pre_comp",
];
const DIRECTION: &[&str] = &["normal", "reversed"];
const SHAPE_BLEND: &[&str] = BLEND_MODES;
const COMPOSITE: &[&str] = &["above", "below"];

/// Attributes of a built-in prim type.
#[derive(Debug)]
pub struct PrimSchema {
    pub prim_type: PrimType,
    pub doc: &'static str,
    pub attrs: &'static [AttrDef],
}

const LAYER_TRANSFORM: &[AttrDef] = &[
    vec3("transform:anchor_point", [0.0, 0.0, 0.0]),
    vec3("transform:position", [0.0, 0.0, 0.0])
        .doc("Pixels in comp space; importers set the comp center like AE."),
    boolean("transform:separate_dimensions", false),
    float("transform:position_x", 0.0).doc("Used when separate_dimensions is on."),
    float("transform:position_y", 0.0),
    float("transform:position_z", 0.0),
    vec3("transform:scale", [100.0, 100.0, 100.0]).doc("Percent."),
    vec3("transform:orientation", [0.0, 0.0, 0.0]).doc("Degrees, 3D layers."),
    float("transform:rotation_x", 0.0),
    float("transform:rotation_y", 0.0),
    float("transform:rotation", 0.0).doc("Degrees (Z rotation for 3D layers)."),
    float("transform:opacity", 100.0).doc("Percent."),
    vec3("transform:point_of_interest", [0.0, 0.0, 0.0]).kinds(&["camera", "light"]),
];

const LAYER: &[AttrDef] = &[
    string("name").doc("Display name (may repeat or contain any character)."),
    choice("kind", "solid", LAYER_KINDS).doc("Layer kind; required."),
    prim_ref("source").doc("Comp (pre-comp) or Asset (footage, solid) providing the pixels/audio."),
    int("label", 1),
    time("start_time", 0).doc("Comp time of layer time 0."),
    time("in_point", 0).doc("Comp time."),
    time_none("out_point").doc("Comp time; none = end of the comp."),
    float("stretch", 100.0)
        .fixed()
        .doc("Time stretch in percent."),
    boolean("enabled", true).doc("Video switch."),
    boolean("audio_enabled", true),
    boolean("solo", false),
    boolean("locked", false),
    boolean("shy", false),
    prim_ref("parent"),
    choice("blend_mode", "normal", BLEND_MODES),
    choice(
        "track_matte",
        "none",
        &["none", "alpha", "alpha_inverted", "luma", "luma_inverted"],
    ),
    prim_ref("track_matte_layer").doc("AE 2023+: the matte layer; none = the layer above."),
    boolean("three_d", false),
    boolean("adjustment_layer", false),
    boolean("preserve_transparency", false)
        .kinds(VISUAL)
        .doc("AE 'Preserve Underlying Transparency' (T switch): draw only where layers below are opaque."),
    boolean("collapse_transformation", false).doc("Continuous rasterization for vectors."),
    boolean("motion_blur", false),
    choice("quality", "best", &["best", "draft", "wireframe"]),
    choice("sampling", "bilinear", &["bilinear", "bicubic"]),
    boolean("guide", false),
    choice(
        "auto_orient",
        "off",
        &["off", "along_path", "towards_point_of_interest"],
    ),
    boolean("time_remap_enabled", false),
    AttrDef::new("time_remap", ValueType::Time, DefaultValue::None, true)
        .doc("Source time; animatable."),
    vec2("audio:levels", [0.0, 0.0])
        .kinds(&["audio", "footage", "pre_comp"])
        .doc("dB, left/right."),
    AttrDef::new(
        "text:source",
        ValueType::TextDocument,
        DefaultValue::None,
        true,
    )
    .kinds(&["text"]),
    prim_ref("text:path")
        .kinds(&["text"])
        .doc("Mask used as the text path."),
    choice("camera:type", "two_node", &["one_node", "two_node"]).kinds(&["camera"]),
    float("camera:zoom", 1777.8)
        .kinds(&["camera"])
        .doc("Pixels."),
    boolean("camera:depth_of_field", false).kinds(&["camera"]),
    float("camera:focus_distance", 1777.8).kinds(&["camera"]),
    float("camera:aperture", 25.3).kinds(&["camera"]),
    float("camera:blur_level", 100.0).kinds(&["camera"]),
    choice(
        "light:type",
        "spot",
        &["parallel", "spot", "point", "ambient", "environment"],
    )
    .kinds(&["light"]),
    float("light:intensity", 100.0).kinds(&["light"]),
    color("light:color", WHITE).kinds(&["light"]),
    float("light:cone_angle", 90.0).kinds(&["light"]),
    float("light:cone_feather", 50.0).kinds(&["light"]),
    choice(
        "light:falloff",
        "none",
        &["none", "smooth", "inverse_square_clamped"],
    )
    .kinds(&["light"]),
    float("light:radius", 500.0).kinds(&["light"]),
    float("light:falloff_distance", 500.0).kinds(&["light"]),
    boolean("light:casts_shadows", false).kinds(&["light"]),
    float("light:shadow_darkness", 100.0).kinds(&["light"]),
    float("light:shadow_diffusion", 0.0).kinds(&["light"]),
    boolean("material:casts_shadows", false).kinds(VISUAL),
    boolean("material:accepts_shadows", true).kinds(VISUAL),
    boolean("material:accepts_lights", true).kinds(VISUAL),
];

const SHAPE_TRANSFORM: &[AttrDef] = &[
    vec2("transform:anchor_point", [0.0, 0.0]),
    vec2("transform:position", [0.0, 0.0]),
    vec2("transform:scale", [100.0, 100.0]),
    float("transform:skew", 0.0),
    float("transform:skew_axis", 0.0),
    float("transform:rotation", 0.0),
    float("transform:opacity", 100.0),
];

/// Every built-in schema.
pub static SCHEMAS: &[PrimSchema] = &[
    PrimSchema {
        prim_type: PrimType::PROJECT,
        doc: "Project settings (one per project, /project).",
        attrs: &[
            choice("bit_depth", "16", &["8", "16", "32"]),
            string("working_space").doc("Working color space name; empty = none (AE default)."),
            boolean("linearize", false),
            boolean("blend_colors_using_gamma_1", false),
            choice("time_display", "timecode", &["timecode", "frames"]),
            choice(
                "expression_engine",
                "javascript",
                &["javascript", "extendscript"],
            ),
        ],
    },
    PrimSchema {
        prim_type: PrimType::FOLDER,
        doc: "Project panel folder; also groups a comp's layers (`layers`).",
        attrs: &[string("name"), int("label", 2)],
    },
    PrimSchema {
        prim_type: PrimType::COMP,
        doc: "Composition. Layers live under its `layers` child, topmost first.",
        attrs: &[
            string("name"),
            int("width", 1920),
            int("height", 1080),
            AttrDef::new(
                "pixel_aspect",
                ValueType::Float,
                DefaultValue::Float(1.0),
                false,
            ),
            rate("frame_rate", 30, 1),
            time("duration", 10 * SECOND),
            color("bg_color", BLACK).fixed(),
            time("work_area_start", 0),
            time_none("work_area_duration").doc("None = to the end."),
            time("display_start_time", 0),
            boolean("motion_blur", false),
            AttrDef::new(
                "shutter_angle",
                ValueType::Float,
                DefaultValue::Float(180.0),
                false,
            ),
            AttrDef::new(
                "shutter_phase",
                ValueType::Float,
                DefaultValue::Float(-90.0),
                false,
            ),
            int("motion_blur_samples", 16),
            choice(
                "renderer",
                "classic_3d",
                &["classic_3d", "cinema_4d", "advanced_3d"],
            ),
            boolean("preserve_frame_rate", false),
            boolean("preserve_resolution", false),
            int("label", 15),
        ],
    },
    PrimSchema {
        prim_type: PrimType::LAYER,
        doc: "A layer of a comp. Kind-specific attributes list their kinds.",
        attrs: LAYER,
    },
    PrimSchema {
        prim_type: PrimType::SHAPE_GROUP,
        doc: "Shape group (contents of a shape layer or of another group).",
        attrs: SHAPE_TRANSFORM,
    },
    PrimSchema {
        prim_type: PrimType::PATH,
        doc: "Free-form shape path.",
        attrs: &[bezier("path"), choice("direction", "normal", DIRECTION)],
    },
    PrimSchema {
        prim_type: PrimType::RECT,
        doc: "Rectangle path.",
        attrs: &[
            vec2("size", [100.0, 100.0]),
            vec2("position", [0.0, 0.0]),
            float("roundness", 0.0),
            choice("direction", "normal", DIRECTION),
        ],
    },
    PrimSchema {
        prim_type: PrimType::ELLIPSE,
        doc: "Ellipse path.",
        attrs: &[
            vec2("size", [100.0, 100.0]),
            vec2("position", [0.0, 0.0]),
            choice("direction", "normal", DIRECTION),
        ],
    },
    PrimSchema {
        prim_type: PrimType::STAR,
        doc: "Polystar path.",
        attrs: &[
            choice("star_type", "star", &["star", "polygon"]),
            float("points", 5.0),
            vec2("position", [0.0, 0.0]),
            float("rotation", 0.0),
            float("inner_radius", 50.0),
            float("outer_radius", 100.0),
            float("inner_roundness", 0.0),
            float("outer_roundness", 0.0),
            choice("direction", "normal", DIRECTION),
        ],
    },
    PrimSchema {
        prim_type: PrimType::FILL,
        doc: "Solid fill of the paths above it in the group.",
        attrs: &[
            color("color", [1.0, 0.0, 0.0, 1.0]),
            float("opacity", 100.0),
            choice("fill_rule", "non_zero", &["non_zero", "even_odd"]),
            choice("blend_mode", "normal", SHAPE_BLEND),
            choice("composite", "below", COMPOSITE),
        ],
    },
    PrimSchema {
        prim_type: PrimType::STROKE,
        doc: "Stroke of the paths above it in the group.",
        attrs: &[
            color("color", WHITE),
            float("opacity", 100.0),
            float("width", 2.0),
            choice("line_cap", "butt", &["butt", "round", "projecting"]),
            choice("line_join", "miter", &["miter", "round", "bevel"]),
            float("miter_limit", 4.0),
            AttrDef::new("dashes", ValueType::FloatList, DefaultValue::None, true)
                .doc("dash, gap, dash, gap …"),
            float("dash_offset", 0.0),
            choice("blend_mode", "normal", SHAPE_BLEND),
            choice("composite", "below", COMPOSITE),
        ],
    },
    PrimSchema {
        prim_type: PrimType::GRADIENT_FILL,
        doc: "Gradient fill.",
        attrs: &[
            choice("gradient_type", "linear", &["linear", "radial"]),
            vec2("start_point", [0.0, 0.0]),
            vec2("end_point", [100.0, 0.0]),
            float("highlight_length", 0.0),
            float("highlight_angle", 0.0),
            AttrDef::new("colors", ValueType::FloatList, DefaultValue::None, true).doc(
                "AE layout: [pos, r, g, b] per color stop, then [pos, alpha] per opacity stop.",
            ),
            int("color_stops", 2),
            float("opacity", 100.0),
            choice("fill_rule", "non_zero", &["non_zero", "even_odd"]),
            choice("blend_mode", "normal", SHAPE_BLEND),
        ],
    },
    PrimSchema {
        prim_type: PrimType::GRADIENT_STROKE,
        doc: "Gradient stroke.",
        attrs: &[
            choice("gradient_type", "linear", &["linear", "radial"]),
            vec2("start_point", [0.0, 0.0]),
            vec2("end_point", [100.0, 0.0]),
            float("highlight_length", 0.0),
            float("highlight_angle", 0.0),
            AttrDef::new("colors", ValueType::FloatList, DefaultValue::None, true),
            int("color_stops", 2),
            float("opacity", 100.0),
            float("width", 2.0),
            choice("line_cap", "butt", &["butt", "round", "projecting"]),
            choice("line_join", "miter", &["miter", "round", "bevel"]),
            float("miter_limit", 4.0),
        ],
    },
    PrimSchema {
        prim_type: PrimType::SHAPE_OPERATOR,
        doc: "Path operator applied to the paths above it in the group.",
        attrs: &[
            choice(
                "operator",
                "trim",
                &[
                    "merge",
                    "trim",
                    "repeater",
                    "offset",
                    "round_corners",
                    "twist",
                    "pucker_bloat",
                    "wiggle_paths",
                    "zig_zag",
                    "wiggle_transform",
                ],
            ),
            choice(
                "merge:mode",
                "merge",
                &["merge", "add", "subtract", "intersect", "exclude"],
            ),
            float("trim:start", 0.0),
            float("trim:end", 100.0),
            float("trim:offset", 0.0),
            choice(
                "trim:mode",
                "simultaneously",
                &["simultaneously", "individually"],
            ),
            float("repeater:copies", 3.0),
            float("repeater:offset", 0.0),
            choice("repeater:composite", "below", COMPOSITE),
            vec2("repeater:position", [100.0, 0.0]),
            vec2("repeater:scale", [100.0, 100.0]),
            float("repeater:rotation", 0.0),
            float("repeater:start_opacity", 100.0),
            float("repeater:end_opacity", 100.0),
            float("offset:amount", 0.0),
            float("round_corners:radius", 10.0),
            float("twist:angle", 0.0),
            float("pucker_bloat:amount", 0.0),
        ],
    },
    PrimSchema {
        prim_type: PrimType::MASK,
        doc: "Layer mask, in layer space.",
        attrs: &[
            string("name"),
            bezier("path"),
            choice(
                "mode",
                "add",
                &[
                    "none",
                    "add",
                    "subtract",
                    "intersect",
                    "lighten",
                    "darken",
                    "difference",
                ],
            ),
            boolean("inverted", false),
            vec2("feather", [0.0, 0.0]),
            float("opacity", 100.0),
            float("expansion", 0.0),
            color("color", [1.0, 1.0, 0.0, 1.0]).fixed(),
            boolean("locked", false),
        ],
    },
    PrimSchema {
        prim_type: PrimType::EFFECT,
        doc: "Effect instance. Parameters are `param:NNNN` (see bae_model::effect).",
        attrs: &[
            string("match_name").doc("Effect identity, e.g. ADBE Gaussian Blur 2; required."),
            string("name"),
            boolean("enabled", true),
            string("plugin:id").doc("Plugin that provides the effect (none = built-in)."),
            string("plugin:version"),
            AttrDef::new("state", ValueType::Bytes, DefaultValue::None, false)
                .doc("Opaque per-instance plugin state (AE sequence data)."),
        ],
    },
    PrimSchema {
        prim_type: PrimType::MARKER,
        doc: "Comp or layer marker (layer markers use layer time).",
        attrs: &[
            time("time", 0),
            time("duration", 0),
            string("comment"),
            string("chapter"),
            string("url"),
            string("frame_target"),
            string("cue_point_name"),
            int("label", 0),
            boolean("protected_region", false),
        ],
    },
    PrimSchema {
        prim_type: PrimType::TEXT_ANIMATOR,
        doc: "Text animator: properties applied through its selectors.",
        attrs: &[
            string("name"),
            vec3("animator:anchor_point", [0.0, 0.0, 0.0]),
            vec3("animator:position", [0.0, 0.0, 0.0]),
            vec3("animator:scale", [100.0, 100.0, 100.0]),
            float("animator:skew", 0.0),
            float("animator:rotation", 0.0),
            float("animator:opacity", 100.0),
            color("animator:fill_color", WHITE),
            color("animator:stroke_color", WHITE),
            float("animator:stroke_width", 0.0),
            float("animator:tracking", 0.0),
            float("animator:line_spacing", 0.0),
            float("animator:blur", 0.0),
        ],
    },
    PrimSchema {
        prim_type: PrimType::TEXT_SELECTOR,
        doc: "Text selector of an animator.",
        attrs: &[
            choice("selector_type", "range", &["range", "wiggly", "expression"]),
            float("start", 0.0),
            float("end", 100.0),
            float("offset", 0.0),
            choice("units", "percentage", &["percentage", "index"]),
            choice(
                "based_on",
                "characters",
                &[
                    "characters",
                    "characters_excluding_spaces",
                    "words",
                    "lines",
                ],
            ),
            choice(
                "mode",
                "add",
                &["add", "subtract", "intersect", "min", "max", "difference"],
            ),
            float("amount", 100.0),
            choice(
                "shape",
                "square",
                &[
                    "square",
                    "ramp_up",
                    "ramp_down",
                    "triangle",
                    "round",
                    "smooth",
                ],
            ),
            float("smoothness", 100.0),
            float("ease_high", 0.0),
            float("ease_low", 0.0),
            boolean("randomize_order", false),
            int("random_seed", 0),
        ],
    },
    PrimSchema {
        prim_type: PrimType::ASSET,
        doc: "Footage, solid or placeholder item of the project panel.",
        attrs: &[
            string("name"),
            choice(
                "kind",
                "footage",
                &["footage", "solid", "placeholder", "sequence"],
            ),
            string("file:relative"),
            string("file:original"),
            int("file:size", 0),
            int("file:mtime", 0),
            string("file:hash"),
            int("width", 0),
            int("height", 0),
            rate("frame_rate", 30, 1),
            time_none("duration"),
            AttrDef::new(
                "pixel_aspect",
                ValueType::Float,
                DefaultValue::Float(1.0),
                false,
            ),
            choice(
                "alpha_mode",
                "straight",
                &["straight", "premultiplied", "ignore"],
            ),
            color("premultiply_color", BLACK).fixed(),
            string("color_space"),
            boolean("has_video", true),
            boolean("has_audio", false),
            color("solid_color", [0.5, 0.5, 0.5, 1.0]).fixed(),
            int("loop", 1),
            int("sequence:first_frame", 0),
            string("sequence:pattern").doc("e.g. name_####.png"),
            int("label", 4),
        ],
    },
    PrimSchema {
        prim_type: PrimType::ARTBOARD,
        doc: "Ai-style artboard (framework only).",
        attrs: &[
            string("name"),
            vec2("size", [1920.0, 1080.0]),
            vec2("origin", [0.0, 0.0]),
        ],
    },
    PrimSchema {
        prim_type: PrimType::BOARD,
        doc: "Storyboard (framework only).",
        attrs: &[string("name")],
    },
    PrimSchema {
        prim_type: PrimType::PANEL,
        doc: "Storyboard panel (framework only).",
        attrs: &[
            string("name"),
            prim_ref("image"),
            time("duration", 2 * SECOND),
            string("dialogue"),
            string("action"),
            string("camera_notes"),
            prim_ref("shot"),
        ],
    },
    PrimSchema {
        prim_type: PrimType::SHOT,
        doc: "Shot ID linking panels, motion comps and edit clips (framework only).",
        attrs: &[string("name"), string("code"), prim_ref("comp")],
    },
    PrimSchema {
        prim_type: PrimType::SEQUENCE,
        doc: "Edit sequence (framework only).",
        attrs: &[
            string("name"),
            rate("frame_rate", 24, 1),
            int("width", 1920),
            int("height", 1080),
        ],
    },
    PrimSchema {
        prim_type: PrimType::TRACK,
        doc: "Edit track (framework only).",
        attrs: &[
            string("name"),
            choice("track_kind", "video", &["video", "audio"]),
            boolean("muted", false),
        ],
    },
    PrimSchema {
        prim_type: PrimType::CLIP,
        doc: "Edit clip (framework only).",
        attrs: &[
            prim_ref("source"),
            time("start", 0),
            time("source_in", 0),
            time("duration", SECOND),
            float("speed", 100.0).fixed(),
            prim_ref("shot"),
        ],
    },
];

/// Schema of a built-in prim type.
pub fn prim_schema(prim_type: &PrimType) -> Option<&'static PrimSchema> {
    SCHEMAS.iter().find(|s| &s.prim_type == prim_type)
}

/// Definition of one attribute of a prim type (layer transforms included).
pub fn attr_def(prim_type: &PrimType, attr: &str) -> Option<&'static AttrDef> {
    if prim_type == &PrimType::LAYER
        && let Some(d) = LAYER_TRANSFORM.iter().find(|a| a.name == attr)
    {
        return Some(d);
    }
    prim_schema(prim_type)?
        .attrs
        .iter()
        .find(|a| a.name == attr)
}

/// All attributes of a prim type (for layers: transform first, then the rest).
pub fn attr_defs(prim_type: &PrimType) -> Vec<&'static AttrDef> {
    let mut out: Vec<&'static AttrDef> = Vec::new();
    if prim_type == &PrimType::LAYER {
        out.extend(LAYER_TRANSFORM.iter());
    }
    if let Some(s) = prim_schema(prim_type) {
        out.extend(s.attrs.iter());
    }
    out
}

/// Check a value against the schema. Unknown types and attributes (plugin types,
/// `ae:` / `param:` / custom attributes) are accepted. An `int` is accepted where a
/// `float` is expected; enum tokens must be one of the options.
pub fn check_value(
    path: &PrimPath,
    prim_type: Option<&PrimType>,
    attr: &str,
    value: &Value,
) -> Result<()> {
    let Some(def) = prim_type.and_then(|t| attr_def(t, attr)) else {
        return Ok(());
    };
    let got = value.value_type();
    let ok = got == def.value_type
        || (def.value_type == ValueType::Float && got == ValueType::Int)
        || (def.value_type == ValueType::PrimRef && got == ValueType::AssetRef);
    if !ok {
        return Err(ModelError::TypeMismatch {
            path: path.clone(),
            attr: attr.to_owned(),
            expected: format!("{:?}", def.value_type).to_lowercase(),
            got: format!("{got:?}").to_lowercase(),
        });
    }
    if let (Value::Enum(token), false) = (value, def.options.is_empty())
        && !def.options.contains(&token.as_str())
    {
        return Err(ModelError::TypeMismatch {
            path: path.clone(),
            attr: attr.to_owned(),
            expected: format!("one of {}", def.options.join(", ")),
            got: token.clone(),
        });
    }
    Ok(())
}

/// Whether keyframes/expressions are allowed (unknown attributes: yes).
pub fn is_animatable(prim_type: Option<&PrimType>, attr: &str) -> bool {
    prim_type
        .and_then(|t| attr_def(t, attr))
        .is_none_or(|d| d.animatable)
}

#[derive(Serialize)]
struct AttrJson {
    name: &'static str,
    #[serde(rename = "type")]
    value_type: ValueType,
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<Value>,
    animatable: bool,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    options: &'static [&'static str],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    kinds: &'static [&'static str],
    #[serde(skip_serializing_if = "str::is_empty")]
    doc: &'static str,
}

/// All schemas as JSON (for agents, the UI property panel and docs).
pub fn to_json() -> serde_json::Value {
    let types: Vec<serde_json::Value> = SCHEMAS
        .iter()
        .map(|s| {
            let attrs: Vec<AttrJson> = attr_defs(&s.prim_type)
                .into_iter()
                .map(|a| AttrJson {
                    name: a.name,
                    value_type: a.value_type,
                    default: a.default.to_value(),
                    animatable: a.animatable,
                    options: a.options,
                    kinds: a.kinds,
                    doc: a.doc,
                })
                .collect();
            serde_json::json!({ "prim_type": s.prim_type, "doc": s.doc, "attrs": attrs })
        })
        .collect();
    serde_json::json!({ "version": 1, "prim_types": types })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_type_has_a_schema_and_defaults_match_types() {
        for t in PrimType::BUILTIN {
            let s = prim_schema(t).unwrap_or_else(|| panic!("no schema for {t}"));
            let mut seen = std::collections::HashSet::new();
            for a in attr_defs(&s.prim_type) {
                crate::error::validate_attr_name(a.name).unwrap();
                assert!(seen.insert(a.name), "{t}: duplicate {}", a.name);
                if let Some(v) = a.default.to_value() {
                    assert_eq!(v.value_type(), a.value_type, "{t}.{}", a.name);
                }
                if let DefaultValue::Enum(d) = a.default {
                    assert!(a.options.contains(&d), "{t}.{}", a.name);
                }
                for k in a.kinds {
                    assert!(LAYER_KINDS.contains(k));
                }
            }
        }
        let json = to_json();
        assert!(json["prim_types"].as_array().unwrap().len() >= PrimType::BUILTIN.len());
    }

    #[test]
    fn checks() {
        let p = PrimPath::new("/c/l").unwrap();
        let layer = Some(&PrimType::LAYER);
        check_value(&p, layer, "transform:opacity", &Value::Float(50.0)).unwrap();
        check_value(&p, layer, "transform:opacity", &Value::Int(50)).unwrap();
        assert!(check_value(&p, layer, "transform:opacity", &Value::String("50".into())).is_err());
        check_value(&p, layer, "blend_mode", &Value::Enum("multiply".into())).unwrap();
        assert!(check_value(&p, layer, "blend_mode", &Value::Enum("mutliply".into())).is_err());
        check_value(&p, layer, "ae:ADBE_Foo", &Value::Int(1)).unwrap();
        check_value(&p, layer, "custom_thing", &Value::Bool(true)).unwrap();
        check_value(&p, None, "transform:opacity", &Value::Bool(true)).unwrap();
        assert!(!is_animatable(layer, "blend_mode"));
        assert!(is_animatable(layer, "transform:position"));
    }
}
