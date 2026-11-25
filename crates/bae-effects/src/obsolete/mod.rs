//! Obsolete AE identities keep their disk parameter schemas and render through the modern effect.
mod color;
mod noise_utility;

use super::{Builtin, EffectInfo, Registry};
use crate::params::Params;
use bae_model::{EffectDef, EffectParamDef, ParamKind, Value};
use bae_render::effect::{EffectValues, LayerDependency, MaskSelect};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};
use smallvec::SmallVec;
use std::sync::Arc;

struct Translation<'a> {
    source: &'a [(u32, Value)],
    changes: SmallVec<[(u32, Option<Value>); 8]>,
}

impl<'a> Translation<'a> {
    fn replace<const N: usize>(values: [(u32, Value); N]) -> Self {
        Self {
            source: &[],
            changes: values.into_iter().map(|(id, v)| (id, Some(v))).collect(),
        }
    }

    fn patch<const N: usize>(p: Params<'a>, values: [(u32, Option<Value>); N]) -> Self {
        Self {
            source: &p.p.effect.params,
            changes: values.into_iter().collect(),
        }
    }
}

type Map = for<'a> fn(Params<'a>) -> Translation<'a>;

#[derive(Debug)]
pub(super) struct Alias {
    pub def: EffectDef,
    target: Arc<dyn Effect>,
    map: Option<Map>,
}

impl Alias {
    pub(super) fn with_params<R>(
        &self,
        p: &EffectParams<'_>,
        f: impl FnOnce(&EffectParams<'_>) -> R,
    ) -> R {
        let Some(map) = self.map else { return f(p) };
        let mapped = map(Params::new(&self.def, p));
        let values = EffectValues::new(mapped.source, &mapped.changes);
        f(&EffectParams {
            translated_values: Some(&values),
            ..*p
        })
    }
}

impl Effect for Alias {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn renders_pixels(&self) -> bool {
        self.target.renders_pixels()
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        self.with_params(p, |p| self.target.input_roi(p, roi))
    }
    fn output_bounds(&self, p: &EffectParams<'_>, input: IRect) -> IRect {
        self.with_params(p, |p| self.target.output_bounds(p, input))
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        self.with_params(p, |p| self.target.layer_dependencies(p))
    }
    fn mask_dependencies(&self, p: &EffectParams<'_>) -> Vec<MaskSelect> {
        self.with_params(p, |p| self.target.mask_dependencies(p))
    }
    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.with_params(p, |p| self.target.render(ctx, p, input, roi))
    }
}

struct Definition {
    info: EffectInfo,
    def: EffectDef,
}

impl Definition {
    fn inherit(target: &Builtin, old: &str, id: &str, name: &str, zh: &str) -> Self {
        let mut def = target.def().clone();
        def.match_name = old.into();
        def.category = "Obsolete".into();
        def.name = name.into();
        for param in &mut def.params {
            if param.match_name.is_some() {
                param.match_name = Some(format!("{old}-{:04}", param.id));
            }
        }
        let mut info = target.info.clone();
        info.full_id = format!("{}/{id}", crate::PLUGIN_ID);
        info.match_name = old.into();
        info.category = def.category.clone();
        info.name = name.into();
        info.name_zh = zh.into();
        info.obsolete = true;
        Self { def, info }
    }
    fn param(&mut self, id: u32) -> &mut EffectParamDef {
        self.def
            .params
            .iter_mut()
            .find(|p| p.id == id)
            .expect("source effect parameter")
    }
    fn finish(mut self, target: &Builtin, map: Option<Map>) -> Builtin {
        let has = |id| self.def.param(id).is_some();
        self.info.param_names_zh.retain(|(id, _)| has(*id));
        self.info.param_options_zh.retain(|(id, _)| {
            matches!(
                self.def.param(*id).map(|p| &p.kind),
                Some(ParamKind::Popup { .. })
            )
        });
        Builtin {
            info: self.info,
            implementation: target.implementation.clone(),
            alias: Some(Arc::new(Alias {
                def: self.def,
                target: target.effect(),
                map,
            })),
        }
    }
}

pub(super) fn register(r: &mut Registry) {
    let mut aliases = Vec::new();
    for target in &r.effects {
        noise_utility::register(target, &mut aliases);
        color::register(target, &mut aliases);
    }
    r.effects.extend(aliases);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_model::PrimPath;
    use bae_render::RenderEffect;

    #[test]
    fn legacy_levels_borrows_the_instance_and_unchanged_values() {
        let fx = crate::get("ADBE Easy Levels").unwrap();
        let instance = RenderEffect {
            path: PrimPath::new("/comps/main/layers/a/effects/levels").unwrap(),
            effect: fx.info.match_name.clone(),
            enabled: true,
            params: vec![
                (2, Value::Bytes(vec![0; 1024])),
                (3, Value::Float(0.2)),
                (8, Value::Bool(false)),
            ],
        };
        let params = EffectParams::new(&instance);
        fx.alias.as_ref().unwrap().with_params(&params, |p| {
            assert!(std::ptr::eq(p.effect, &instance));
            assert!(std::ptr::eq(
                p.value(3).unwrap(),
                instance.param(3).unwrap()
            ));
            assert!(p.value(2).is_none());
            assert_eq!((p.popup(8), p.popup(9)), (Some(2), Some(1)));
        });
    }
}
