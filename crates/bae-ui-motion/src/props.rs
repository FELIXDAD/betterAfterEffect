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
        out.push(Node::Group {
            id: format!("{layer}#text"),
            label: tr("Text").to_owned(),
            children: vec![prop(stage, layer, "text:source", three_d)],
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
    let effects = effect_nodes(stage, layer);
    if !effects.is_empty() {
        out.push(Node::Group {
            id: format!("{layer}#effects"),
            label: tr("Effects").to_owned(),
            children: effects,
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
    out
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
            let mut params: Vec<&str> = v
                .attr_names()
                .into_iter()
                .filter(|n| n.starts_with("param:"))
                .collect();
            params.sort_unstable();
            Some(Node::Group {
                id: format!("{layer}#{e}"),
                label: group_label(stage, &e),
                children: params
                    .into_iter()
                    .map(|a| Node::Prop {
                        path: e.clone(),
                        attr: a.to_owned(),
                        label: format!("{} {}", tr("Param"), a.trim_start_matches("param:")),
                    })
                    .collect(),
            })
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
pub(crate) fn revealed(stage: &Stage, layer: &PrimPath, reveal: &Reveal) -> Vec<Node> {
    let mut all = Vec::new();
    flatten_props(&layer_tree(stage, layer), &mut all);
    all.into_iter()
        .filter(|n| {
            let Node::Prop { path, attr, .. } = n else {
                return false;
            };
            let spec = query::attr_spec(stage, path, attr);
            match reveal {
                Reveal::Animated => spec.is_some_and(|s| s.is_animated() || s.expression.is_some()),
                Reveal::Modified => spec.is_some_and(|s| !s.is_empty()),
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
