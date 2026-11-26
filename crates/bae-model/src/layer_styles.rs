//! Photoshop layer style properties in AE timeline order. Each style owns a
//! fixed block of parameter IDs in the layer's single animatable style prim.
use crate::{PrimPath, Stage, Value};

pub const PRIM: &str = "layer_styles";
pub const RENDER_ID: &str = "bae.effects/layer-styles";

pub struct Group {
    pub token: &'static str,
    pub name: &'static str,
    pub zh: &'static str,
    pub enabled: u32,
    pub fields: &'static [Field],
}

pub struct Field {
    pub id: u32,
    pub match_name: &'static str,
    pub name: &'static str,
    pub zh: &'static str,
    pub kind: Control,
}

#[derive(Clone, Copy)]
pub enum Control {
    Number(f64, f64, f64),
    Angle(f64),
    Switch(bool),
    Menu(i64, &'static [&'static str]),
    Color([f64; 4]),
    Point,
    Gradient,
}

impl Field {
    pub fn initial(&self) -> Value {
        match self.kind {
            Number(n, ..) | Angle(n) => Value::Float(n),
            Switch(b) => Value::Bool(b),
            Menu(n, _) => Value::Int(n),
            Color(c) => Value::Color(c),
            Point => Value::Vec2([0.; 2]),
            Gradient => Value::FloatList(vec![0., 0., 0., 0., 1., 1., 1., 1., 1., 1.]),
        }
    }
}

use Control::*;

const fn field(
    id: u32,
    match_name: &'static str,
    name: &'static str,
    zh: &'static str,
    kind: Control,
) -> Field {
    Field {
        id,
        match_name,
        name,
        zh,
        kind,
    }
}

const MODES: &[&str] = &[
    "Normal",
    "Dissolve",
    "Dancing Dissolve",
    "Darken",
    "Multiply",
    "Color Burn",
    "Classic Color Burn",
    "Linear Burn",
    "Darker Color",
    "Add",
    "Screen",
    "Color Dodge",
    "Classic Color Dodge",
    "Linear Dodge",
    "Lighter Color",
    "Overlay",
    "Soft Light",
    "Hard Light",
    "Linear Light",
    "Vivid Light",
    "Pin Light",
    "Hard Mix",
    "Difference",
    "Classic Difference",
    "Exclusion",
    "Subtract",
    "Divide",
    "Hue",
    "Saturation",
    "Color",
    "Luminosity",
    "Stencil Alpha",
    "Silhouette Alpha",
];
const COLOR_TYPE: &[&str] = &["Single Color", "Gradient"];
const GLOW_TECHNIQUE: &[&str] = &["Softer", "Precise"];
const SOURCE: &[&str] = &["Edge", "Center"];
const BEVEL_STYLE: &[&str] = &[
    "Outer Bevel",
    "Inner Bevel",
    "Emboss",
    "Pillow Emboss",
    "Stroke Emboss",
];
const BEVEL_TECHNIQUE: &[&str] = &["Smooth", "Chisel Hard", "Chisel Soft"];
const DIRECTION: &[&str] = &["Up", "Down"];
const GRADIENT_STYLE: &[&str] = &["Linear", "Radial", "Angle", "Reflected", "Diamond"];
const POSITION: &[&str] = &["Outside", "Inside", "Center"];

pub const GROUPS: &[Group] = &[
    Group {
        token: "blendingOptions",
        name: "Blending Options",
        zh: "混合选项",
        enabled: 0,
        fields: &[
            field(
                1,
                "ADBE Global Angle2",
                "Global Light Angle",
                "全局光角度",
                Angle(120.),
            ),
            field(
                2,
                "ADBE Global Altitude2",
                "Global Light Altitude",
                "全局光高度",
                Angle(30.),
            ),
            field(
                3,
                "ADBE Layer Fill Opacity2",
                "Fill Opacity",
                "填充不透明度",
                Number(100., 0., 100.),
            ),
            field(4, "ADBE R Channel Blend", "Red", "红色", Switch(true)),
            field(5, "ADBE G Channel Blend", "Green", "绿色", Switch(true)),
            field(6, "ADBE B Channel Blend", "Blue", "蓝色", Switch(true)),
            field(
                7,
                "ADBE Blend Interior",
                "Blend Interior Styles as Group",
                "混合内部样式为组",
                Switch(false),
            ),
            field(
                8,
                "ADBE Blend Ranges",
                "Use Source's Blend Ranges",
                "使用源的混合范围",
                Switch(true),
            ),
        ],
    },
    Group {
        token: "dropShadow",
        name: "Drop Shadow",
        zh: "投影",
        enabled: 100,
        fields: &[
            field(
                101,
                "dropShadow/mode2",
                "Blend Mode",
                "混合模式",
                Menu(5, MODES),
            ),
            field(
                102,
                "dropShadow/color",
                "Color",
                "颜色",
                Color([0., 0., 0., 1.]),
            ),
            field(
                103,
                "dropShadow/opacity",
                "Opacity",
                "不透明度",
                Number(75., 0., 100.),
            ),
            field(
                104,
                "dropShadow/useGlobalAngle",
                "Use Global Light",
                "使用全局光",
                Switch(false),
            ),
            field(
                105,
                "dropShadow/localLightingAngle",
                "Angle",
                "角度",
                Angle(120.),
            ),
            field(
                106,
                "dropShadow/distance",
                "Distance",
                "距离",
                Number(5., 0., 30000.),
            ),
            field(
                107,
                "dropShadow/chokeMatte",
                "Spread",
                "扩展",
                Number(0., 0., 100.),
            ),
            field(108, "dropShadow/blur", "Size", "大小", Number(5., 0., 250.)),
            field(
                109,
                "dropShadow/noise",
                "Noise",
                "杂色",
                Number(0., 0., 100.),
            ),
            field(
                110,
                "dropShadow/layerConceals",
                "Layer Knocks Out Drop Shadow",
                "图层镂空投影",
                Switch(true),
            ),
        ],
    },
    Group {
        token: "innerShadow",
        name: "Inner Shadow",
        zh: "内阴影",
        enabled: 200,
        fields: &[
            field(
                201,
                "innerShadow/mode2",
                "Blend Mode",
                "混合模式",
                Menu(5, MODES),
            ),
            field(
                202,
                "innerShadow/color",
                "Color",
                "颜色",
                Color([0., 0., 0., 1.]),
            ),
            field(
                203,
                "innerShadow/opacity",
                "Opacity",
                "不透明度",
                Number(75., 0., 100.),
            ),
            field(
                204,
                "innerShadow/useGlobalAngle",
                "Use Global Light",
                "使用全局光",
                Switch(false),
            ),
            field(
                205,
                "innerShadow/localLightingAngle",
                "Angle",
                "角度",
                Angle(120.),
            ),
            field(
                206,
                "innerShadow/distance",
                "Distance",
                "距离",
                Number(5., 0., 30000.),
            ),
            field(
                207,
                "innerShadow/chokeMatte",
                "Choke",
                "阻塞",
                Number(0., 0., 100.),
            ),
            field(
                208,
                "innerShadow/blur",
                "Size",
                "大小",
                Number(5., 0., 250.),
            ),
            field(
                209,
                "innerShadow/noise",
                "Noise",
                "杂色",
                Number(0., 0., 100.),
            ),
        ],
    },
    Group {
        token: "outerGlow",
        name: "Outer Glow",
        zh: "外发光",
        enabled: 300,
        fields: &[
            field(
                301,
                "outerGlow/mode2",
                "Blend Mode",
                "混合模式",
                Menu(11, MODES),
            ),
            field(
                302,
                "outerGlow/opacity",
                "Opacity",
                "不透明度",
                Number(75., 0., 100.),
            ),
            field(
                303,
                "outerGlow/noise",
                "Noise",
                "杂色",
                Number(0., 0., 100.),
            ),
            field(
                304,
                "outerGlow/AEColorChoice",
                "Color Type",
                "颜色类型",
                Menu(1, COLOR_TYPE),
            ),
            field(
                305,
                "outerGlow/color",
                "Color",
                "颜色",
                Color([1., 1., 0.745098, 1.]),
            ),
            field(306, "outerGlow/gradient", "Colors", "颜色", Gradient),
            field(
                307,
                "outerGlow/gradientSmoothness",
                "Gradient Smoothness",
                "渐变平滑度",
                Number(100., 0., 100.),
            ),
            field(
                308,
                "outerGlow/glowTechnique",
                "Technique",
                "技术",
                Menu(1, GLOW_TECHNIQUE),
            ),
            field(
                309,
                "outerGlow/chokeMatte",
                "Spread",
                "扩展",
                Number(0., 0., 100.),
            ),
            field(310, "outerGlow/blur", "Size", "大小", Number(5., 0., 250.)),
            field(
                311,
                "outerGlow/inputRange",
                "Range",
                "范围",
                Number(50., 1., 100.),
            ),
            field(
                312,
                "outerGlow/shadingNoise",
                "Jitter",
                "抖动",
                Number(0., 0., 100.),
            ),
        ],
    },
    Group {
        token: "innerGlow",
        name: "Inner Glow",
        zh: "内发光",
        enabled: 400,
        fields: &[
            field(
                401,
                "innerGlow/mode2",
                "Blend Mode",
                "混合模式",
                Menu(11, MODES),
            ),
            field(
                402,
                "innerGlow/opacity",
                "Opacity",
                "不透明度",
                Number(75., 0., 100.),
            ),
            field(
                403,
                "innerGlow/noise",
                "Noise",
                "杂色",
                Number(0., 0., 100.),
            ),
            field(
                404,
                "innerGlow/AEColorChoice",
                "Color Type",
                "颜色类型",
                Menu(1, COLOR_TYPE),
            ),
            field(
                405,
                "innerGlow/color",
                "Color",
                "颜色",
                Color([1., 1., 0.745098, 1.]),
            ),
            field(406, "innerGlow/gradient", "Colors", "颜色", Gradient),
            field(
                407,
                "innerGlow/gradientSmoothness",
                "Gradient Smoothness",
                "渐变平滑度",
                Number(100., 0., 100.),
            ),
            field(
                408,
                "innerGlow/glowTechnique",
                "Technique",
                "技术",
                Menu(1, GLOW_TECHNIQUE),
            ),
            field(
                409,
                "innerGlow/innerGlowSource",
                "Source",
                "源",
                Menu(1, SOURCE),
            ),
            field(
                410,
                "innerGlow/chokeMatte",
                "Choke",
                "阻塞",
                Number(0., 0., 100.),
            ),
            field(411, "innerGlow/blur", "Size", "大小", Number(5., 0., 250.)),
            field(
                412,
                "innerGlow/inputRange",
                "Range",
                "范围",
                Number(50., 1., 100.),
            ),
            field(
                413,
                "innerGlow/shadingNoise",
                "Jitter",
                "抖动",
                Number(0., 0., 100.),
            ),
        ],
    },
    Group {
        token: "bevelEmboss",
        name: "Bevel and Emboss",
        zh: "斜面和浮雕",
        enabled: 500,
        fields: &[
            field(
                501,
                "bevelEmboss/bevelStyle",
                "Style",
                "样式",
                Menu(2, BEVEL_STYLE),
            ),
            field(
                502,
                "bevelEmboss/bevelTechnique",
                "Technique",
                "技术",
                Menu(1, BEVEL_TECHNIQUE),
            ),
            field(
                503,
                "bevelEmboss/strengthRatio",
                "Depth",
                "深度",
                Number(100., 1., 1000.),
            ),
            field(
                504,
                "bevelEmboss/bevelDirection",
                "Direction",
                "方向",
                Menu(1, DIRECTION),
            ),
            field(
                505,
                "bevelEmboss/blur",
                "Size",
                "大小",
                Number(5., 0., 250.),
            ),
            field(
                506,
                "bevelEmboss/softness",
                "Soften",
                "柔化",
                Number(0., 0., 16.),
            ),
            field(
                507,
                "bevelEmboss/useGlobalAngle",
                "Use Global Light",
                "使用全局光",
                Switch(false),
            ),
            field(
                508,
                "bevelEmboss/localLightingAngle",
                "Angle",
                "角度",
                Angle(120.),
            ),
            field(
                509,
                "bevelEmboss/localLightingAltitude",
                "Altitude",
                "高度",
                Angle(30.),
            ),
            field(
                510,
                "bevelEmboss/highlightMode",
                "Highlight Mode",
                "高亮模式",
                Menu(11, MODES),
            ),
            field(
                511,
                "bevelEmboss/highlightColor",
                "Highlight Color",
                "加亮颜色",
                Color([1., 1., 1., 1.]),
            ),
            field(
                512,
                "bevelEmboss/highlightOpacity",
                "Highlight Opacity",
                "高光不透明度",
                Number(75., 0., 100.),
            ),
            field(
                513,
                "bevelEmboss/shadowMode",
                "Shadow Mode",
                "阴影模式",
                Menu(5, MODES),
            ),
            field(
                514,
                "bevelEmboss/shadowColor",
                "Shadow Color",
                "阴影颜色",
                Color([0., 0., 0., 1.]),
            ),
            field(
                515,
                "bevelEmboss/shadowOpacity",
                "Shadow Opacity",
                "阴影不透明度",
                Number(75., 0., 100.),
            ),
        ],
    },
    Group {
        token: "chromeFX",
        name: "Satin",
        zh: "光泽",
        enabled: 600,
        fields: &[
            field(
                601,
                "chromeFX/mode2",
                "Blend Mode",
                "混合模式",
                Menu(5, MODES),
            ),
            field(
                602,
                "chromeFX/color",
                "Color",
                "颜色",
                Color([0., 0., 0., 1.]),
            ),
            field(
                603,
                "chromeFX/opacity",
                "Opacity",
                "不透明度",
                Number(50., 0., 100.),
            ),
            field(
                604,
                "chromeFX/localLightingAngle",
                "Angle",
                "角度",
                Angle(19.),
            ),
            field(
                605,
                "chromeFX/distance",
                "Distance",
                "距离",
                Number(11., 1., 250.),
            ),
            field(606, "chromeFX/blur", "Size", "大小", Number(14., 0., 250.)),
            field(607, "chromeFX/invert", "Invert", "反转", Switch(true)),
        ],
    },
    Group {
        token: "solidFill",
        name: "Color Overlay",
        zh: "颜色叠加",
        enabled: 700,
        fields: &[
            field(
                701,
                "solidFill/mode2",
                "Blend Mode",
                "混合模式",
                Menu(1, MODES),
            ),
            field(
                702,
                "solidFill/color",
                "Color",
                "颜色",
                Color([1., 0., 0., 1.]),
            ),
            field(
                703,
                "solidFill/opacity",
                "Opacity",
                "不透明度",
                Number(100., 0., 100.),
            ),
        ],
    },
    Group {
        token: "gradientFill",
        name: "Gradient Overlay",
        zh: "渐变叠加",
        enabled: 800,
        fields: &[
            field(
                801,
                "gradientFill/mode2",
                "Blend Mode",
                "混合模式",
                Menu(1, MODES),
            ),
            field(
                802,
                "gradientFill/opacity",
                "Opacity",
                "不透明度",
                Number(100., 0., 100.),
            ),
            field(803, "gradientFill/gradient", "Colors", "颜色", Gradient),
            field(
                804,
                "gradientFill/gradientSmoothness",
                "Gradient Smoothness",
                "渐变平滑度",
                Number(100., 0., 100.),
            ),
            field(805, "gradientFill/angle", "Angle", "角度", Angle(90.)),
            field(
                806,
                "gradientFill/type",
                "Style",
                "样式",
                Menu(1, GRADIENT_STYLE),
            ),
            field(
                807,
                "gradientFill/reverse",
                "Reverse",
                "反向",
                Switch(false),
            ),
            field(
                808,
                "gradientFill/align",
                "Align with Layer",
                "与图层对齐",
                Switch(true),
            ),
            field(
                809,
                "gradientFill/scale",
                "Scale",
                "缩放",
                Number(100., 10., 150.),
            ),
            field(810, "gradientFill/offset", "Offset", "偏移", Point),
        ],
    },
    Group {
        token: "frameFX",
        name: "Stroke",
        zh: "描边",
        enabled: 900,
        fields: &[
            field(
                901,
                "frameFX/mode2",
                "Blend Mode",
                "混合模式",
                Menu(1, MODES),
            ),
            field(
                902,
                "frameFX/color",
                "Color",
                "颜色",
                Color([1., 0., 0., 1.]),
            ),
            field(903, "frameFX/size", "Size", "大小", Number(3., 1., 250.)),
            field(
                904,
                "frameFX/opacity",
                "Opacity",
                "不透明度",
                Number(100., 0., 100.),
            ),
            field(905, "frameFX/style", "Position", "位置", Menu(1, POSITION)),
        ],
    },
];

pub fn group(token: &str) -> Option<&'static Group> {
    GROUPS.iter().find(|g| g.token == token)
}

pub fn field_by_id(id: u32) -> Option<&'static Field> {
    GROUPS
        .get((id / 100) as usize)?
        .fields
        .iter()
        .find(|f| f.id == id)
}

/// False on an instance after removing inherited styles. An untouched instance
/// reads the source asset until its styles are made editable.
pub const USE_SOURCE: &str = "layer_styles:use_source";

pub fn source_styles(stage: &Stage, layer: &PrimPath) -> Option<PrimPath> {
    let source = stage
        .prim(layer)?
        .value("source")?
        .as_prim_ref()?
        .child(PRIM)
        .ok()?;
    stage.has_prim(&source).then_some(source)
}

pub fn effective_styles(stage: &Stage, layer: &PrimPath) -> Option<PrimPath> {
    let local = layer.child(PRIM).ok()?;
    if stage.has_prim(&local) {
        return Some(local);
    }
    if stage
        .prim(layer)?
        .value(USE_SOURCE)
        .and_then(Value::as_bool)
        == Some(false)
    {
        return None;
    }
    source_styles(stage, layer)
}
