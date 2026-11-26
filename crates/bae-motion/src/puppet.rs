//! Puppet pins (AE's Puppet Position, Starch and Overlap Pin tools). Each pin
//! is a `PuppetPin` prim under the layer's Puppet effect (`ADBE FreePin3`),
//! which the first pin adds at the bottom of the effect stack. A new pin rests
//! where it is placed; a Position pin's Position gets a keyframe at the
//! current time, as in AE (puppet pins animate from the start). One pin is
//! one undo step.

use bae_model::{Command, CommandError, Op, PrimPath, PrimType, Time, Transaction, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::effect::{EFFECTS_FOLDER, effects_of};
use crate::query;
use crate::util::{child, create, ensure_unlocked, fail, set};

/// The Puppet effect's matchName.
pub const PUPPET: &str = "ADBE FreePin3";

fn position() -> String {
    "position".to_owned()
}

#[derive(Deserialize, JsonSchema)]
pub struct AddPinParams {
    pub layer: PrimPath,
    /// `position` (Puppet Position Pin tool), `starch`, `overlap`, `bend` or
    /// `advanced`.
    #[serde(default = "position")]
    pub pin_type: String,
    /// Where on the layer (layer pixels).
    pub at: [f64; 2],
    /// Layer time of the Position keyframe.
    #[serde(default)]
    pub time: Time,
    /// A new Puppet effect's mesh Expansion (pixels) and Density (the
    /// Puppet tools' options).
    #[serde(default)]
    pub expansion: Option<f64>,
    #[serde(default)]
    pub density: Option<f64>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AddPinOutput {
    pub effect: PrimPath,
    pub pin: PrimPath,
}

pub struct AddPin;

impl Command for AddPin {
    type Params = AddPinParams;
    type Output = AddPinOutput;
    const ID: &'static str = "puppet.add_pin";
    const TITLE: &'static str = "Add Puppet Pin";
    const DESCRIPTION: &'static str = "Add a puppet pin to a layer (AE: Puppet Position, \
        Starch and Overlap Pin tools). The first pin adds the Puppet effect.";

    fn run(&self, tx: &mut Transaction<'_>, p: AddPinParams) -> Result<AddPinOutput, CommandError> {
        let layer = &p.layer;
        if !query::is_layer(tx.stage(), layer) {
            return Err(fail(format!("{layer} is not a layer")));
        }
        ensure_unlocked(tx.stage(), std::slice::from_ref(layer))?;
        if !matches!(
            p.pin_type.as_str(),
            "position" | "starch" | "overlap" | "bend" | "advanced"
        ) {
            return Err(fail(format!("unknown pin type `{}`", p.pin_type)));
        }
        let effect = puppet_effect(tx, layer, p.expansion, p.density)?;
        // AE: "Puppet Pin 1", "Overlap 1" …, numbered per kind.
        let (base, title) = match p.pin_type.as_str() {
            "overlap" => ("overlap", "Overlap"),
            "starch" => ("starch", "Starch"),
            _ => ("pin", "Puppet Pin"),
        };
        let n = (1..)
            .find(|i| {
                child(&effect, &format!("{base}_{i}")).is_ok_and(|c| !tx.stage().has_prim(&c))
            })
            .unwrap_or(1);
        let pin = child(&effect, &format!("{base}_{n}"))?;
        create(tx, &pin, PrimType::PUPPET_PIN, None)?;
        set(tx, &pin, "name", Value::String(format!("{title} {n}")))?;
        set(tx, &pin, "pin_type", Value::Enum(p.pin_type.clone()))?;
        set(tx, &pin, "rest", Value::Vec2(p.at))?;
        set(tx, &pin, "position", Value::Vec2(p.at))?;
        if !matches!(p.pin_type.as_str(), "starch" | "overlap") {
            tx.apply(Op::SetKeyframe {
                path: pin.clone(),
                attr: "position".to_owned(),
                time: p.time,
                keyframe: crate::keys::new_key(
                    "position",
                    &bae_model::AttrSpec::default(),
                    Value::Vec2(p.at),
                ),
            })?;
        }
        Ok(AddPinOutput { effect, pin })
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct DeletePinsParams {
    pub pins: Vec<PrimPath>,
}

pub struct DeletePins;

impl Command for DeletePins {
    type Params = DeletePinsParams;
    type Output = ();
    const ID: &'static str = "puppet.delete_pins";
    const TITLE: &'static str = "Remove Selected Puppet Parameters";
    const DESCRIPTION: &'static str = "Remove puppet pins (AE: select pins with a Puppet \
        tool and press Delete). The Puppet effect stays.";

    fn run(&self, tx: &mut Transaction<'_>, p: DeletePinsParams) -> Result<(), CommandError> {
        for pin in &p.pins {
            let is_pin = tx
                .stage()
                .prim(pin)
                .is_some_and(|v| v.is_a(&PrimType::PUPPET_PIN));
            if !is_pin {
                return Err(fail(format!("{pin} is not a puppet pin")));
            }
            if let Some(layer) = query::owning_layer(tx.stage(), pin) {
                ensure_unlocked(tx.stage(), std::slice::from_ref(&layer))?;
            }
        }
        crate::layer::delete_prims(tx, &p.pins, Time::ZERO)
    }
}

/// The layer's Puppet effect, added at the bottom of its stack if missing.
fn puppet_effect(
    tx: &mut Transaction<'_>,
    layer: &PrimPath,
    expansion: Option<f64>,
    density: Option<f64>,
) -> Result<PrimPath, CommandError> {
    let existing = effects_of(tx.stage(), layer).into_iter().find(|e| {
        tx.stage()
            .prim(e)
            .and_then(|v| {
                v.value("match_name")
                    .and_then(|m| m.as_str().map(str::to_owned))
            })
            .as_deref()
            == Some(PUPPET)
    });
    if let Some(e) = existing {
        return Ok(e);
    }
    let folder = child(layer, EFFECTS_FOLDER)?;
    if !tx.stage().has_prim(&folder) {
        create(tx, &folder, PrimType::FOLDER, None)?;
    }
    let name = query::unique_child_name(tx.stage(), &folder, "Puppet");
    let path = child(&folder, &name)?;
    create(tx, &path, PrimType::EFFECT, None)?;
    set(tx, &path, "match_name", Value::String(PUPPET.to_owned()))?;
    set(tx, &path, "name", Value::String("Puppet".to_owned()))?;
    // Expansion and Density (parameters 4 and 5).
    for (id, v) in [(4, expansion), (5, density)] {
        if let Some(v) = v {
            set(
                tx,
                &path,
                &bae_model::effect::param_attr(id),
                Value::Float(v),
            )?;
        }
    }
    Ok(path)
}

/// The pins under a Puppet effect, in order.
pub fn pins_of(stage: &bae_model::Stage, effect: &PrimPath) -> Vec<PrimPath> {
    stage
        .children(effect)
        .into_iter()
        .filter(|c| stage.prim(c).is_some_and(|v| v.is_a(&PrimType::PUPPET_PIN)))
        .collect()
}
