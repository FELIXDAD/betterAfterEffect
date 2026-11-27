//! The property tree of a layer as AE's timeline shows it: Text, Contents (shape
//! groups and their items), Masks, Effects, Transform; and the U / UU / P S R T A
//! reveal filters.

use bae_model::schema;
use bae_model::{PrimPath, PrimType, Stage};
use bae_motion::query;
use bae_ui::tr;

use crate::common::{self, attr_label};
use crate::session::Reveal;

#[derive(Clone, Debug)]
pub(crate) enum Node {
    Group {
        /// Expansion key (unique per layer).
        id: String,
        label: String,
        children: Vec<Node>,
    },
    Prop {
        path: PrimPath,
        attr: String,
        label: String,
    },
}

fn prop(stage: &Stage, path: &PrimPath, attr: &str, three_d: bool) -> Node {
    let ty = stage.prim(path).and_then(|v| v.prim_type().cloned());
    Node::Prop {
        path: path.clone(),
        attr: attr.to_owned(),
        label: attr_label(ty.as_ref(), attr, three_d),
    }
}

/// Text > Path Options: Path, and once a mask is chosen the options of text on
/// a path (AE shows them only then).
fn path_options(stage: &Stage, layer: &PrimPath, three_d: bool) -> Node {
    let mut children = vec![prop(stage, layer, "text:path", three_d)];
    if common::text_path_mask(stage, layer).is_some() {
        for attr in [
            "text:reverse_path",
            "text:perpendicular_to_path",
            "text:force_alignment",
            "text:first_margin",
            "text:last_margin",
        ] {
            children.push(prop(stage, layer, attr, three_d));
        }
    }
    Node::Group {
        id: format!("{layer}#path_options"),
        label: tr("Path Options").to_owned(),
        children,
    }
}

/// Transform properties of a layer (AE order).
pub(crate) fn transform_attrs(stage: &Stage, layer: &PrimPath) -> Vec<&'static str> {
    let mut attrs = transform_attrs_combined(stage, layer);
    // Separate Dimensions: X / Y (/ Z) Position in place of Position.
    if bae_motion::dimensions::is_separated(stage, layer)
        && let Some(i) = attrs.iter().position(|a| *a == "transform:position")
    {
        let n = if common::is_3d(stage, layer) { 3 } else { 2 };
        attrs.splice(
            i..=i,
            bae_motion::dimensions::COMPONENTS[..n].iter().copied(),
        );
    }
    attrs
}

fn transform_attrs_combined(stage: &Stage, layer: &PrimPath) -> Vec<&'static str> {
    let kind = query::layer_kind(stage, layer).unwrap_or_default();
    let three_d = common::is_3d(stage, layer);
    match kind.as_str() {
        "camera" | "light" => vec![
            "transform:point_of_interest",
            "transform:position",
            "transform:orientation",
            "transform:rotation_x",
            "transform:rotation_y",
            "transform:rotation",
        ],
        _ if three_d => vec![
            "transform:anchor_point",
            "transform:position",
            "transform:scale",
            "transform:orientation",
            "transform:rotation_x",
            "transform:rotation_y",
            "transform:rotation",
            "transform:opacity",
        ],
        _ => vec![
            "transform:anchor_point",
            "transform:position",
            "transform:scale",
            "transform:rotation",
            "transform:opacity",
        ],
    }
}

fn group_label(stage: &Stage, path: &PrimPath) -> String {
    query::display_name_of(stage, path)
}

/// Attributes shown for a shape item.
fn shape_item_attrs(ty: &PrimType) -> &'static [&'static str] {
    if *ty == PrimType::RECT {
        &["size", "position", "roundness"]
    } else if *ty == PrimType::ELLIPSE {
        &["size", "position"]
    } else if *ty == PrimType::STAR {
        &[
            "points",
            "position",
            "rotation",
            "inner_radius",
            "outer_radius",
            "inner_roundness",
            "outer_roundness",
        ]
    } else if *ty == PrimType::PATH {
        &["path"]
    } else if *ty == PrimType::FILL {
        &["color", "opacity"]
    } else if *ty == PrimType::STROKE {
        &["color", "opacity", "width"]
    } else if *ty == PrimType::GRADIENT_FILL || *ty == PrimType::GRADIENT_STROKE {
        &["start_point", "end_point", "opacity"]
    } else {
        &[]
    }
}

const SHAPE_TRANSFORM: &[&str] = &[
    "transform:anchor_point",
    "transform:position",
    "transform:scale",
    "transform:skew",
    "transform:skew_axis",
    "transform:rotation",
    "transform:opacity",
];

fn shape_nodes(stage: &Stage, parent: &PrimPath, layer: &PrimPath, out: &mut Vec<Node>) {
    for c in stage.children(parent) {
        let Some(view) = stage.prim(&c) else {
            continue;
        };
        let Some(ty) = view.prim_type().cloned() else {
            continue;
        };
        if ty == PrimType::FOLDER {
            shape_nodes(stage, &c, layer, out);
            continue;
        }
        let id = format!("{}#{}", layer, c);
        let mut children = Vec::new();
        if ty == PrimType::SHAPE_GROUP {
            let mut inner = Vec::new();
            shape_nodes(stage, &c, layer, &mut inner);
            children.push(Node::Group {
                id: format!("{id}#contents"),
                label: tr("Contents").to_owned(),
                children: inner,
            });
            children.push(Node::Group {
                id: format!("{id}#transform"),
                label: format!("{}: {}", tr("Transform"), group_label(stage, &c)),
                children: SHAPE_TRANSFORM
                    .iter()
                    .map(|a| prop(stage, &c, a, false))
                    .collect(),
            });
        } else if ty == PrimType::SHAPE_OPERATOR {
            let op = query::static_value(&view, "operator")
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            for d in schema::attr_defs(&ty) {
                if d.animatable && d.name.starts_with(&format!("{op}:")) {
                    children.push(prop(stage, &c, d.name, false));
                }
            }
        } else {
            for a in shape_item_attrs(&ty) {
                children.push(prop(stage, &c, a, false));
            }
        }
        out.push(Node::Group {
            id,
            label: group_label(stage, &c),
            children,
        });
    }
}

/// The full property tree under a layer.
pub(crate) fn layer_tree(stage: &Stage, layer: &PrimPath) -> Vec<Node> {
    let kind = query::layer_kind(stage, layer).unwrap_or_default();
    let three_d = common::is_3d(stage, layer);
    let mut out = Vec::new();
    if kind == "text" {
        let mut children = vec![
            prop(stage, layer, "text:source", three_d),
            path_options(stage, layer, three_d),
        ];
        for (i, a) in bae_motion::text_animators::animators_of(stage, layer)
            .iter()
            .enumerate()
        {
            children.push(animator_node(stage, a, i + 1));
        }
        out.push(Node::Group {
            id: format!("{layer}#text"),
            label: tr("Text").to_owned(),
            children,
        });
    }
    if let Ok(contents) = layer.child(query::CONTENTS)
        && stage.has_prim(&contents)
    {
        let mut children = Vec::new();
        shape_nodes(stage, &contents, layer, &mut children);
        out.push(Node::Group {
            id: format!("{layer}#contents"),
            label: tr("Contents").to_owned(),
            children,
        });
    }
    // Time Remap, once enabled: a property of its own above the trackers and
    // masks (AE's property order).
    if stage
        .prim(layer)
        .and_then(|v| query::static_value(&v, "time_remap_enabled"))
        .and_then(|v| v.as_bool())
        == Some(true)
    {
        out.push(prop(stage, layer, "time_remap", three_d));
    }
    if let Ok(masks) = layer.child("masks") {
        let children: Vec<Node> = stage
            .children(&masks)
            .into_iter()
            .filter(|m| stage.prim(m).is_some_and(|v| v.is_a(&PrimType::MASK)))
            .map(|m| Node::Group {
                id: format!("{layer}#{m}"),
                label: group_label(stage, &m),
                children: ["path", "feather", "opacity", "expansion"]
                    .iter()
                    .map(|a| prop(stage, &m, a, false))
                    .collect(),
            })
            .collect();
        if !children.is_empty() {
            out.push(Node::Group {
                id: format!("{layer}#masks"),
                label: tr("Masks").to_owned(),
                children,
            });
        }
    }
    let trackers = bae_motion::tracker::trackers(stage, layer);
    if !trackers.is_empty() {
        let children = trackers
            .into_iter()
            .map(|tracker| {
                let children = bae_motion::tracker::points(stage, &tracker)
                    .into_iter()
                    .map(|point| Node::Group {
                        id: point.to_string(),
                        label: group_label(stage, &point),
                        children: bae_motion::tracker::PROPERTIES
                            .iter()
                            .map(|(key, name)| Node::Prop {
                                path: point.clone(),
                                attr: (*key).into(),
                                label: tr(name).into(),
                            })
                            .collect(),
                    })
                    .collect();
                Node::Group {
                    id: tracker.to_string(),
                    label: group_label(stage, &tracker),
                    children,
                }
            })
            .collect();
        out.push(Node::Group {
            id: format!("{layer}#trackers"),
            label: tr("Motion Trackers").into(),
            children,
        });
    }
    let effects = effect_nodes(stage, layer);
    if !effects.is_empty() {
        out.push(Node::Group {
            id: format!("{layer}#effects"),
            label: tr("Effects").to_owned(),
            children: effects,
        });
    }
    if bae_media::audio::layer_file(stage, layer).is_some() {
        out.push(Node::Group {
            id: format!("{layer}#audio"),
            label: tr("Audio").to_owned(),
            children: vec![prop(stage, layer, "audio:levels", false)],
        });
    }
    out.push(Node::Group {
        id: format!("{layer}#transform"),
        label: tr("Transform").to_owned(),
        children: transform_attrs(stage, layer)
            .into_iter()
            .map(|a| prop(stage, layer, a, three_d))
            .collect(),
    });
    out.extend(crate::layer_styles::node(stage, layer));
    // Camera Options / Light Options / a 3D layer's Material Options (AE
    // shows them after Transform).
    let options = match kind.as_str() {
        "camera" => Some(("camera_options", "Camera Options", CAMERA_OPTIONS.to_vec())),
        "light" => Some((
            "light_options",
            "Light Options",
            light_options(stage, layer),
        )),
        _ if three_d => Some((
            "material_options",
            "Material Options",
            material_options(stage, layer),
        )),
        _ => None,
    };
    // Advanced 3D: text and shape layers' Geometry Options, before their
    // Material Options.
    if three_d && matches!(kind.as_str(), "text" | "shape") && advanced_3d(stage, layer) {
        out.push(Node::Group {
            id: format!("{layer}#geometry_options"),
            label: tr("Geometry Options").to_owned(),
            children: GEOMETRY_OPTIONS
                .into_iter()
                .map(|a| prop(stage, layer, a, three_d))
                .collect(),
        });
    }
    if let Some((id, label, attrs)) = options {
        out.push(Node::Group {
            id: format!("{layer}#{id}"),
            label: tr(label).to_owned(),
            children: attrs
                .into_iter()
                .map(|a| prop(stage, layer, a, three_d))
                .collect(),
        });
    }
    out
}

/// A camera's Camera Options, in AE's order.
const CAMERA_OPTIONS: [&str; 13] = [
    "camera:zoom",
    "camera:depth_of_field",
    "camera:focus_distance",
    "camera:aperture",
    "camera:blur_level",
    "camera:iris_shape",
    "camera:iris_rotation",
    "camera:iris_roundness",
    "camera:iris_aspect_ratio",
    "camera:iris_diffraction_fringe",
    "camera:highlight_gain",
    "camera:highlight_threshold",
    "camera:highlight_saturation",
];

/// A 3D layer's Material Options (Classic 3D), in AE's order.
const MATERIAL_OPTIONS: [&str; 9] = [
    "material:casts_shadows",
    "material:light_transmission",
    "material:accepts_shadows",
    "material:accepts_lights",
    "material:ambient",
    "material:diffuse",
    "material:specular_intensity",
    "material:specular_shininess",
    "material:metal",
];

/// The Advanced 3D renderer's Material Options, in AE's order (its comp
/// renderer decides which a layer shows).
const ADVANCED_3D_MATERIAL_OPTIONS: [&str; 17] = [
    "material:casts_shadows",
    "material:light_transmission",
    "material:accepts_shadows",
    "material:accepts_lights",
    "material:shadow_color",
    "material:appears_in_reflections",
    "material:ambient",
    "material:diffuse",
    "material:specular_intensity",
    "material:specular_shininess",
    "material:metal",
    "material:reflection_intensity",
    "material:reflection_sharpness",
    "material:reflection_rolloff",
    "material:transparency",
    "material:transparency_rolloff",
    "material:index_of_refraction",
];

/// Advanced 3D Geometry Options of text and shape layers, in AE's order.
const GEOMETRY_OPTIONS: [&str; 5] = [
    "geometry:bevel_style",
    "geometry:bevel_direction",
    "geometry:bevel_depth",
    "geometry:hole_bevel_depth",
    "geometry:extrusion_depth",
];

/// Whether `layer`'s comp renders 3D with Advanced 3D.
fn advanced_3d(stage: &Stage, layer: &PrimPath) -> bool {
    query::owning_comp(stage, layer)
        .and_then(|c| stage.prim(&c))
        .and_then(|v| query::static_value(&v, "renderer"))
        .is_some_and(|v| v.as_str() == Some("advanced_3d"))
}

fn material_options(stage: &Stage, layer: &PrimPath) -> Vec<&'static str> {
    if advanced_3d(stage, layer) {
        ADVANCED_3D_MATERIAL_OPTIONS.to_vec()
    } else {
        MATERIAL_OPTIONS.to_vec()
    }
}

/// A light's Light Options for its type, in AE's order: ambient lights have
/// only intensity and colour, parallel lights no falloff, spots a cone.
fn light_options(stage: &Stage, layer: &PrimPath) -> Vec<&'static str> {
    let ty = stage
        .prim(layer)
        .and_then(|v| query::static_value(&v, "light:type"))
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "spot".into());
    if ty == "environment" {
        // AE's order: the picture and how it shows, then its light.
        return vec![
            "light:source",
            "light:background_visible",
            "light:background_opacity",
            "light:background_blur",
            "light:intensity",
            "light:color",
        ];
    }
    let mut attrs = vec!["light:intensity", "light:color"];
    if ty == "ambient" {
        return attrs;
    }
    if ty == "spot" {
        attrs.extend(["light:cone_angle", "light:cone_feather"]);
    }
    if ty != "parallel" {
        attrs.extend(["light:falloff", "light:radius", "light:falloff_distance"]);
    }
    attrs.extend([
        "light:casts_shadows",
        "light:shadow_darkness",
        "light:shadow_diffusion",
    ]);
    attrs
}

/// Animator properties in AE's order.
const ANIMATOR_ATTRS: [&str; 12] = [
    "animator:anchor_point",
    "animator:position",
    "animator:scale",
    "animator:skew",
    "animator:rotation",
    "animator:opacity",
    "animator:fill_color",
    "animator:stroke_color",
    "animator:stroke_width",
    "animator:tracking",
    "animator:line_spacing",
    "animator:blur",
];

/// "Animator N" (AE: 动画制作工具 N): its Range Selectors, then the properties it
/// was given. The group id ends in `#animator` (the Timeline's Add menu reads it).
fn animator_node(stage: &Stage, animator: &PrimPath, n: usize) -> Node {
    use bae_motion::text_animators::selectors_of;
    let mut children: Vec<Node> = selectors_of(stage, animator)
        .iter()
        .enumerate()
        .map(|(j, sel)| Node::Group {
            id: format!("{sel}#selector"),
            label: format!("{} {}", tr("Range Selector"), j + 1),
            children: vec![
                prop(stage, sel, "start", false),
                prop(stage, sel, "end", false),
                prop(stage, sel, "offset", false),
                Node::Group {
                    id: format!("{sel}#advanced"),
                    label: tr("Advanced").to_owned(),
                    children: [
                        "units",
                        "based_on",
                        "mode",
                        "amount",
                        "shape",
                        "smoothness",
                        "ease_high",
                        "ease_low",
                        "randomize_order",
                        "random_seed",
                    ]
                    .iter()
                    .map(|a| prop(stage, sel, a, false))
                    .collect(),
                },
            ],
        })
        .collect();
    children.extend(
        ANIMATOR_ATTRS
            .iter()
            .filter(|a| query::attr_spec(stage, animator, a).is_some())
            .map(|a| prop(stage, animator, a, false)),
    );
    let name = stage
        .prim(animator)
        .and_then(|v| query::static_value(&v, "name"))
        .and_then(|v| v.as_str().map(str::to_owned))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("{} {n}", tr("Animator")));
    Node::Group {
        id: format!("{animator}#animator"),
        label: name,
        children,
    }
}

/// One group per effect with its parameters (`param:NNNN`).
pub(crate) fn effect_nodes(stage: &Stage, layer: &PrimPath) -> Vec<Node> {
    let Ok(folder) = layer.child("effects") else {
        return Vec::new();
    };
    stage
        .children(&folder)
        .into_iter()
        .filter_map(|e| {
            let v = stage.prim(&e)?;
            if !v.is_a(&PrimType::EFFECT) {
                return None;
            }
            let match_name_of = query::static_value(&v, "match_name")
                .and_then(|m| m.as_str().map(str::to_owned))
                .unwrap_or_default();
            let mut params: Vec<&str> = v
                .attr_names()
                .into_iter()
                .filter(|n| n.starts_with("param:"))
                // The Puppet effect's traced outline is data, shown as its
                // Mesh group (AE's Auto-traced Shapes has no value).
                .filter(|n| !(match_name_of == bae_motion::puppet::PUPPET && *n == "param:0006"))
                .collect();
            params.sort_unstable();
            let match_name = query::static_value(&v, "match_name")
                .and_then(|m| m.as_str().map(str::to_owned))
                .unwrap_or_default();
            let mut children: Vec<Node> = params
                .into_iter()
                .map(|a| Node::Prop {
                    path: e.clone(),
                    attr: a.to_owned(),
                    label: bae_model::effect::param_id(a)
                        .and_then(|id| crate::effects::param_label_of(&match_name, id))
                        .unwrap_or_else(|| {
                            format!("{} {}", tr("Param"), a.trim_start_matches("param:"))
                        }),
                })
                .collect();
            if match_name == bae_motion::puppet::PUPPET {
                children.extend(puppet_mesh(stage, layer, &e));
            }
            if match_name == bae_motion::paint::PAINT {
                children.extend(paint_strokes(stage, layer, &e));
            }
            Some(Node::Group {
                id: format!("{layer}#{e}"),
                label: crate::effects::instance_name(stage, &e),
                children,
            })
        })
        .collect()
}

/// A pin's name as AE shows it in the UI language ("Puppet Pin 1" → "操控点 1").
fn pin_label(stage: &Stage, pin: &PrimPath) -> String {
    let name = query::display_name_of(stage, pin);
    ["Puppet Pin", "Overlap", "Starch"]
        .into_iter()
        .find_map(|base| {
            let n = name.strip_prefix(base)?.strip_prefix(' ')?;
            n.parse::<u32>().ok().map(|n| format!("{} {n}", tr(base)))
        })
        .unwrap_or(name)
}

/// A stroke's name as AE shows it in the UI language ("Brush 1" → "画笔 1").
fn stroke_label(stage: &Stage, stroke: &PrimPath) -> String {
    let name = query::display_name_of(stage, stroke);
    ["Brush", "Clone", "Eraser"]
        .into_iter()
        .find_map(|base| {
            let n = name.strip_prefix(base)?.strip_prefix(' ')?;
            n.parse::<u32>().ok().map(|n| format!("{} {n}", tr(base)))
        })
        .unwrap_or(name)
}

/// The Puppet effect's Mesh 1 group (AE): Deform with the Position, Bend and
/// Advanced pins, then Overlap and Starch with theirs.
fn puppet_mesh(stage: &Stage, layer: &PrimPath, effect: &PrimPath) -> Option<Node> {
    let pins = bae_motion::puppet::pins_of(stage, effect);
    if pins.is_empty() {
        return None;
    }
    let kind = |p: &PrimPath| {
        stage
            .prim(p)
            .and_then(|v| query::static_value(&v, "pin_type"))
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| "position".to_owned())
    };
    let pin_node = |p: &PrimPath, attrs: &[(&str, &str)]| Node::Group {
        id: format!("{layer}#{p}"),
        label: pin_label(stage, p),
        children: attrs
            .iter()
            .map(|(attr, label)| Node::Prop {
                path: p.clone(),
                attr: (*attr).to_owned(),
                label: tr(label).to_owned(),
            })
            .collect(),
    };
    // Bend and Advanced pins also turn and scale.
    const TURNED: &[(&str, &str)] = &[
        ("position", "Position"),
        ("scale", "Scale"),
        ("rotation", "Rotation"),
    ];
    let group = |id: &str, label: &str, kinds: &[&str], attrs: &[(&str, &str)]| {
        let children: Vec<Node> = pins
            .iter()
            .filter(|p| kinds.contains(&kind(p).as_str()))
            .map(|p| match kind(p).as_str() {
                "bend" | "advanced" => pin_node(p, TURNED),
                _ => pin_node(p, attrs),
            })
            .collect();
        (!children.is_empty()).then(|| Node::Group {
            id: format!("{layer}#{effect}#{id}"),
            label: tr(label).to_owned(),
            children,
        })
    };
    let children: Vec<Node> = [
        group(
            "deform",
            "Deform",
            &["position", "bend", "advanced"],
            &[("position", "Position")],
        ),
        group(
            "overlap",
            "Overlap",
            &["overlap"],
            &[
                ("position", "Position"),
                ("in_front", "In Front"),
                ("extent", "Extent"),
            ],
        ),
        group(
            "starch",
            "Starch",
            &["starch"],
            &[
                ("position", "Position"),
                ("amount", "Amount"),
                ("extent", "Extent"),
            ],
        ),
    ]
    .into_iter()
    .flatten()
    .collect();
    Some(Node::Group {
        id: format!("{layer}#{effect}#mesh"),
        label: format!("{} 1", tr("Mesh")),
        children,
    })
}

/// The Paint effect's strokes, newest first (AE): each with its Path, Stroke
/// Options and Transform: <stroke> (all keyframable, previewed live).
fn paint_strokes(stage: &Stage, layer: &PrimPath, effect: &PrimPath) -> Vec<Node> {
    const OPTIONS: &[(&str, &str)] = &[
        ("start", "Start"),
        ("end", "End"),
        ("color", "Color"),
        ("diameter", "Diameter"),
        ("angle", "Angle"),
        ("hardness", "Hardness"),
        ("roundness", "Roundness"),
        ("spacing", "Spacing"),
        ("channels", "Channels"),
        ("opacity", "Opacity"),
        ("flow", "Flow"),
    ];
    const TRANSFORM: &[(&str, &str)] = &[
        ("transform:anchor_point", "Anchor Point"),
        ("transform:position", "Position"),
        ("transform:scale", "Scale"),
        ("transform:rotation", "Rotation"),
    ];
    let props = |s: &PrimPath, attrs: &[(&str, &str)]| -> Vec<Node> {
        attrs
            .iter()
            .map(|(attr, label)| Node::Prop {
                path: s.clone(),
                attr: (*attr).to_owned(),
                label: tr(label).to_owned(),
            })
            .collect()
    };
    bae_motion::paint::strokes_of(stage, effect)
        .into_iter()
        .rev()
        .map(|s| {
            let label = stroke_label(stage, &s);
            Node::Group {
                id: format!("{layer}#{s}"),
                label: label.clone(),
                children: vec![
                    Node::Prop {
                        path: s.clone(),
                        attr: "path".to_owned(),
                        label: tr("Path").to_owned(),
                    },
                    Node::Group {
                        id: format!("{layer}#{s}#options"),
                        label: tr("Stroke Options").to_owned(),
                        children: props(&s, OPTIONS),
                    },
                    Node::Group {
                        id: format!("{layer}#{s}#transform"),
                        label: format!("{} {label}", tr("Transform:")),
                        children: props(&s, TRANSFORM),
                    },
                ],
            }
        })
        .collect()
}

fn flatten_props(nodes: &[Node], out: &mut Vec<Node>) {
    for n in nodes {
        match n {
            Node::Group { children, .. } => flatten_props(children, out),
            p @ Node::Prop { .. } => out.push(p.clone()),
        }
    }
}

/// The properties a reveal filter shows directly under the layer.
/// The masks of `layer`, in order.
pub(crate) fn masks_of(stage: &Stage, layer: &PrimPath) -> Vec<PrimPath> {
    layer.child("masks").map_or_else(
        |_| Vec::new(),
        |masks| {
            stage
                .children(&masks)
                .into_iter()
                .filter(|m| stage.prim(m).is_some_and(|v| v.is_a(&PrimType::MASK)))
                .collect()
        },
    )
}

/// The mask attributes a reveal list asks for (M: Mask Path, F: Mask Feather,
/// MM: all), as the masks' own attribute names (`*` = all).
pub(crate) fn revealed_mask_attrs(list: &[String]) -> Vec<&'static str> {
    list.iter()
        .filter_map(|a| match a.as_str() {
            "mask_path" => Some("path"),
            "mask_feather" => Some("feather"),
            "masks" => Some("*"),
            _ => None,
        })
        .collect()
}

pub(crate) fn revealed(stage: &Stage, layer: &PrimPath, reveal: &Reveal) -> Vec<Node> {
    // Groups the timeline opens itself (E, M, MM, F, LL): no layer property.
    if let Reveal::Attrs(list) = reveal
        && list.iter().all(|a| {
            matches!(
                a.as_str(),
                "effects" | "masks" | "mask_path" | "mask_feather" | "audio:waveform"
            )
        })
    {
        return Vec::new();
    }
    let mut all = Vec::new();
    flatten_props(&layer_tree(stage, layer), &mut all);
    all.into_iter()
        .filter(|n| {
            let Node::Prop { path, attr, .. } = n else {
                return false;
            };
            let spec = || query::attr_spec(stage, path, attr);
            match reveal {
                Reveal::Animated => {
                    spec().is_some_and(|s| s.is_animated() || s.expression.is_some())
                }
                Reveal::Modified => spec().is_some_and(|s| !s.is_empty()),
                Reveal::Props(list) => list.iter().any(|(p, a)| p == path && a == attr),
                Reveal::Attrs(list) => {
                    path == layer
                        && list.iter().any(|a| {
                            a == attr
                                || (a == "transform:position"
                                    && bae_motion::dimensions::COMPONENTS.contains(&attr.as_str()))
                        })
                }
            }
        })
        .collect()
}
