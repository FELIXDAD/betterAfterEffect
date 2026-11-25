//! [`EffectsModule`]: the built-in effects as an app module.
//!
//! Publishes every built-in effect in the app's runtime registries, next to
//! the plugin effects `bae-plugin` puts there (ARCHITECTURE §3, §14: built-ins
//! go through the same registration path as third-party effects):
//!
//! * `Registry<bae_model::EffectDef>`: parameter definitions (UI, agents, AE
//!   import), id = [`registry_id`] of the AE matchName;
//! * `Registry<dyn bae_render::Effect>`: implementations (renderers), same id;
//! * `Registry<EffectInfo>`: catalog entries (Chinese names, category,
//!   keywords) for the Effect menu and the Effects & Presets panel.
//!
//! Renderers sync their [`bae_render::EffectLibrary`] from the registry
//! ([`bae_render::EffectLibrary::sync`]), so effects loaded later (plugins)
//! reach open viewers without a restart.

use std::sync::Arc;

use bae_app::{AppBuilder, AppModule};
use bae_model::EffectDef;
use bae_render::Effect;
use bae_render::effect::{FRAME_INTERPOLATOR_ID, registry_id};

use crate::catalog::EffectInfo;

/// App module of the built-in effects.
#[derive(Debug, Default)]
pub struct EffectsModule;

impl AppModule for EffectsModule {
    fn name(&self) -> &str {
        "bae-effects"
    }

    fn build(&self, app: &mut AppBuilder) {
        let owner = app.current_owner();
        let defs = app.registry::<EffectDef>();
        let effects = app.registry::<dyn Effect>();
        let infos = app.registry::<EffectInfo>();
        defs.batch(|tx| {
            for b in crate::builtins() {
                if let Err(e) = tx.register(
                    owner.clone(),
                    registry_id(&b.info.match_name),
                    b.def().clone(),
                ) {
                    tracing::warn!("effect definition not registered: {e}");
                }
            }
        });
        effects.batch(|tx| {
            if let Err(e) = tx.register_arc(
                owner.clone(),
                FRAME_INTERPOLATOR_ID,
                crate::effects::temporal::interpolator(),
            ) {
                tracing::warn!("frame interpolator not registered: {e}");
            }
            for b in crate::builtins() {
                if let Err(e) =
                    tx.register_arc(owner.clone(), registry_id(&b.info.match_name), b.effect())
                {
                    tracing::warn!("effect not registered: {e}");
                }
            }
        });
        infos.batch(|tx| {
            for b in crate::builtins() {
                let info: Arc<EffectInfo> = Arc::new(b.info.clone());
                if let Err(e) =
                    tx.register_arc(owner.clone(), registry_id(&b.info.match_name), info)
                {
                    tracing::warn!("effect catalog entry not registered: {e}");
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_every_builtin() {
        let mut app = bae_app::AppBuilder::new();
        app.add_module(EffectsModule);
        let app = app.build();
        let n = crate::builtins().len();
        assert!(n >= 11);
        let defs = app.get_registry::<EffectDef>().expect("defs");
        let effects = app.get_registry::<dyn Effect>().expect("effects");
        let infos = app.get_registry::<EffectInfo>().expect("infos");
        assert_eq!(defs.len(), n);
        assert_eq!(effects.len(), n + 1);
        assert_eq!(infos.len(), n);
        let mut library = bae_render::EffectLibrary::new();
        assert!(library.sync(&effects));
        assert!(!library.sync(&effects));
        assert!(library.frame_interpolator().is_some());
        assert_eq!(library.len(), n);
        assert!(library.get("ADBE Gaussian Blur 2").is_some());
        for b in crate::builtins() {
            let name = b.info.match_name.as_str();
            let id = registry_id(name);
            assert_eq!(defs.get(&id).expect("def").match_name, name);
            assert_eq!(effects.get(&id).expect("effect").def().match_name, name);
        }
    }
}
