use bae_model::{PrimPath, Value};
use bae_render::{
    EffectParams,
    effect::{LayerDependency, LayerStage},
};

use crate::catalog::layer_stage_id;

pub(crate) fn own_layer(params: &EffectParams<'_>) -> Option<PrimPath> {
    params.effect.path.parent()?.parent()
}

pub(crate) fn dependency(
    params: &EffectParams<'_>,
    id: u32,
    default_self: bool,
) -> Option<LayerDependency> {
    let layer = match params.value(id) {
        Some(Value::PrimRef(layer)) => layer.clone(),
        None if default_self => own_layer(params)?,
        _ => return None,
    };
    let stage = match params.popup(layer_stage_id(id)) {
        Some(2) => LayerStage::Masks,
        Some(3) => LayerStage::EffectsAndMasks,
        _ => LayerStage::Source,
    };
    Some(LayerDependency {
        layer,
        time: params.comp_time,
        stage,
    })
}
