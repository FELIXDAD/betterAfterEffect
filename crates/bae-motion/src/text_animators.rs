//! Text animator commands for the UI (AE Animation > Animate Text, the Timeline's
//! Animate / Add menus): `text.animate`, `text.add_selector`,
//! `text.remove_animators`. Each is one undo step; they build on bae-model's
//! `text.create_animator` (#260).

use bae_model::commands::{
    CreatePrim, CreatePrimParams, CreateTextAnimator, CreateTextAnimatorParams, PathOutput,
};
use bae_model::{Command, CommandError, PrimPath, PrimType, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query;
use crate::util::{fail, set};

/// The `animators` folder of a text layer.
pub const ANIMATORS: &str = "animators";

/// Animator properties by AE's Animate Text item: (property, attribute, the value AE
/// adds it with).
pub fn animator_property(property: &str) -> Option<(&'static str, Value)> {
    Some(match property {
        "anchor_point" => ("animator:anchor_point", Value::Vec3([0.0; 3])),
        "position" => ("animator:position", Value::Vec3([0.0; 3])),
        "scale" => ("animator:scale", Value::Vec3([100.0; 3])),
        "skew" => ("animator:skew", Value::Float(0.0)),
        "rotation" => ("animator:rotation", Value::Float(0.0)),
        "opacity" => ("animator:opacity", Value::Float(100.0)),
        // AE adds colours as red.
        "fill_rgb" => ("animator:fill_color", Value::Color([1.0, 0.0, 0.0, 1.0])),
        "stroke_rgb" => ("animator:stroke_color", Value::Color([1.0, 0.0, 0.0, 1.0])),
        "stroke_width" => ("animator:stroke_width", Value::Float(0.0)),
        "tracking" => ("animator:tracking", Value::Float(0.0)),
        "line_spacing" => ("animator:line_spacing", Value::Float(0.0)),
        "blur" => ("animator:blur", Value::Float(0.0)),
        _ => return None,
    })
}

/// The animators of a text layer, in order.
pub fn animators_of(stage: &bae_model::Stage, layer: &PrimPath) -> Vec<PrimPath> {
    layer
        .child(ANIMATORS)
        .map(|f| {
            stage
                .children(&f)
                .into_iter()
                .filter(|p| {
                    stage
                        .prim(p)
                        .is_some_and(|v| v.is_a(&PrimType::TEXT_ANIMATOR))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The selectors of an animator, in order.
pub fn selectors_of(stage: &bae_model::Stage, animator: &PrimPath) -> Vec<PrimPath> {
    stage
        .children(animator)
        .into_iter()
        .filter(|p| {
            stage
                .prim(p)
                .is_some_and(|v| v.is_a(&PrimType::TEXT_SELECTOR))
        })
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AnimatorsOutput {
    /// The animators that got the property (new or given).
    pub animators: Vec<PrimPath>,
}

// ---------------------------------------------------------------------------

pub struct AnimateText;

#[derive(Deserialize, JsonSchema)]
pub struct AnimateTextParams {
    /// Text layers: each gets a new animator (with a Range Selector) holding the
    /// property. Ignored when `animator` is given.
    #[serde(default)]
    pub layers: Vec<PrimPath>,
    /// Add the property to this animator instead (the animator's Add menu).
    #[serde(default)]
    pub animator: Option<PrimPath>,
    /// anchor_point, position, scale, skew, rotation, opacity, fill_rgb,
    /// stroke_rgb, stroke_width, tracking, line_spacing or blur.
    pub property: String,
}

impl Command for AnimateText {
    type Params = AnimateTextParams;
    type Output = AnimatorsOutput;
    const ID: &'static str = "text.animate";
    const TITLE: &'static str = "Animate Text";
    const DESCRIPTION: &'static str = "Animate a text property: a new animator with a Range Selector on each text layer (AE Animation > Animate Text), or the property added to an existing animator (its Add menu).";

    fn run(
        &self,
        tx: &mut Transaction<'_>,
        p: Self::Params,
    ) -> Result<AnimatorsOutput, CommandError> {
        let (attr, value) = animator_property(&p.property)
            .ok_or_else(|| fail(format!("unknown text animator property `{}`", p.property)))?;
        let animators = match p.animator {
            Some(a) => vec![a],
            None => {
                let mut out = Vec::new();
                for layer in &p.layers {
                    crate::util::ensure_unlocked(tx.stage(), [layer])?;
                    let n = animators_of(tx.stage(), layer).len() + 1;
                    let folder = crate::util::child(layer, ANIMATORS)?;
                    let name =
                        query::unique_child_name(tx.stage(), &folder, &format!("animator_{n}"));
                    let created = CreateTextAnimator.run(
                        tx,
                        CreateTextAnimatorParams {
                            layer: layer.clone(),
                            name,
                        },
                    )?;
                    out.push(created.path);
                }
                out
            }
        };
        for a in &animators {
            crate::util::ensure_unlocked(tx.stage(), [a])?;
            set(tx, a, attr, value.clone())?;
        }
        Ok(AnimatorsOutput { animators })
    }
}

// ---------------------------------------------------------------------------

pub struct AddTextSelector;

#[derive(Deserialize, JsonSchema)]
pub struct AddTextSelectorParams {
    pub animator: PrimPath,
    /// range (wiggly and expression selectors are not evaluated yet).
    #[serde(default = "range")]
    pub selector: String,
}

fn range() -> String {
    "range".into()
}

impl Command for AddTextSelector {
    type Params = AddTextSelectorParams;
    type Output = PathOutput;
    const ID: &'static str = "text.add_selector";
    const TITLE: &'static str = "Add Text Selector";
    const DESCRIPTION: &'static str = "Add a selector to a text animator (AE Animation > Add Text Selector, the animator's Add > Selector menu).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<Self::Output, CommandError> {
        if p.selector != "range" {
            return Err(fail("only range selectors are supported"));
        }
        crate::util::ensure_unlocked(tx.stage(), [&p.animator])?;
        let n = selectors_of(tx.stage(), &p.animator).len() + 1;
        let name = query::unique_child_name(tx.stage(), &p.animator, &format!("range_{n}"));
        let path = crate::util::child(&p.animator, &name)?;
        CreatePrim.run(
            tx,
            CreatePrimParams {
                path: path.clone(),
                prim_type: PrimType::TEXT_SELECTOR,
                index: None,
            },
        )?;
        Ok(PathOutput { path })
    }
}

// ---------------------------------------------------------------------------

pub struct RemoveTextAnimators;

#[derive(Deserialize, JsonSchema)]
pub struct RemoveTextAnimatorsParams {
    pub layers: Vec<PrimPath>,
}

impl Command for RemoveTextAnimators {
    type Params = RemoveTextAnimatorsParams;
    type Output = ();
    const ID: &'static str = "text.remove_animators";
    const TITLE: &'static str = "Remove All Text Animators";
    const DESCRIPTION: &'static str =
        "Remove every animator of text layers (AE Animation > Remove All Text Animators).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        crate::util::ensure_unlocked(tx.stage(), p.layers.iter())?;
        let all: Vec<PrimPath> = p
            .layers
            .iter()
            .flat_map(|l| animators_of(tx.stage(), l))
            .collect();
        crate::layer::delete_prims(tx, &all, bae_model::Time::ZERO)
    }
}
