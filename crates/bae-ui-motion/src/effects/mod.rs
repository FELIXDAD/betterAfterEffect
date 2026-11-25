//! Effects in the motion workspace: the AE effect directory behind the Effect
//! menu and the Effects & Presets panel, applying effects (menu, double-click,
//! drag and drop onto layers), and the Effect Controls panel content.
//!
//! Every AE effect is listed in AE's categories; effects BAE does not
//! implement (no definition in the app's `Registry<EffectDef>`) are greyed,
//! like AE shows unavailable items. Applying runs `effect.apply` (bae-motion).

pub(crate) mod browser;
pub(crate) mod controls;
mod warp_grid;

use std::sync::{Arc, Mutex, RwLock};

use bae_app::{Registry, Resources};
use bae_effects::EffectInfo;
use bae_model::{EffectDef, EffectParamDef, PrimPath, PrimType, Stage};
use bae_render::effect::registry_id;
use bae_ui::i18n::{self, Lang};
use bae_ui::menu::{MenuItemSpec, MenuPos};
use bae_ui::{ActionHandler, Invocation, PanelContext};
use serde_json::json;

use crate::common;

/// Action: apply the effect `{"match_name": …}` to the selected layers.
pub const APPLY: &str = "motion.effect.apply";
/// Action: Effect ▸ Last Effect (apply the last applied effect again).
pub const LAST_EFFECT: &str = "motion.effect.last_effect";
/// Action: Effect ▸ Remove All (every effect of the selected layers).
pub const REMOVE_ALL: &str = "motion.effect.remove_all";

pub use crate::catalog::AeEffect;

/// Every AE 26 effect of the Effect menu (299 in 22 categories; from the
/// command catalog).
pub fn directory() -> &'static [AeEffect] {
    crate::catalog::ae_effects()
}

/// Whether AE's Chinese UI shows `en` as is: effects whose Chinese name is
/// the English one (CC effects, Keylight, Mocha AE …).
pub fn same_in_chinese(en: &str) -> bool {
    directory().iter().any(|e| e.name == en && e.name_zh == en)
}

/// The Chinese names of AE's effects, for the menu and the panels
/// (`bae_ui::i18n`; categories come with the menu table). Existing UI terms
/// are not overridden.
pub(crate) fn register_i18n() {
    let pairs: Vec<(&'static str, &'static str)> = directory()
        .iter()
        .filter(|e| !i18n::has_translation(e.name))
        .map(|e| (e.name, e.name_zh))
        .collect();
    i18n::register(&pairs);
}

/// The app's effect registries (definitions and catalog entries), as an app
/// resource for the panels and menus.
#[derive(Clone)]
pub struct EffectRegistries {
    /// Parameter definitions (built-in and plugin effects).
    pub defs: Arc<Registry<EffectDef>>,
    /// Catalog entries of built-in effects (Chinese names, keywords).
    pub infos: Arc<Registry<EffectInfo>>,
}

impl std::fmt::Debug for EffectRegistries {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EffectRegistries").finish_non_exhaustive()
    }
}

impl EffectRegistries {
    /// The definition of `match_name`.
    pub fn def(&self, match_name: &str) -> Option<Arc<EffectDef>> {
        self.defs.get(&registry_id(match_name))
    }

    /// The catalog entry of `match_name` (built-in effects).
    pub fn info(&self, match_name: &str) -> Option<Arc<EffectInfo>> {
        self.infos.get(&registry_id(match_name))
    }
}

/// The registries every panel sees; also kept process-wide for code without
/// resources at hand (property labels in the timeline).
static GLOBAL: RwLock<Option<EffectRegistries>> = RwLock::new(None);

pub(crate) fn set_global(registries: EffectRegistries) {
    *GLOBAL.write().unwrap_or_else(|e| e.into_inner()) = Some(registries);
}

/// The effect registries of `resources`, or the process-wide ones.
pub(crate) fn registries(resources: &Resources) -> Option<EffectRegistries> {
    resources.get::<EffectRegistries>().cloned().or_else(global)
}

fn global() -> Option<EffectRegistries> {
    GLOBAL.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Whether the UI shows Chinese.
fn chinese() -> bool {
    i18n::lang() == Lang::Zh
}

/// Localized effect name of `match_name` (AE directory, then the definition).
pub(crate) fn effect_name(match_name: &str, def: Option<&EffectDef>) -> String {
    if let Some(e) = directory().iter().find(|e| e.match_name == match_name) {
        return if chinese() { e.name_zh } else { e.name }.to_owned();
    }
    match def {
        Some(d) => i18n::tr(&d.name).to_owned(),
        None => match_name.to_owned(),
    }
}

/// Localized label of `param` of effect `match_name`.
pub(crate) fn param_label(match_name: &str, param: &EffectParamDef) -> String {
    if chinese()
        && let Some(info) = global().and_then(|r| r.info(match_name))
        && let Some((_, zh)) = info.param_names_zh.iter().find(|(id, _)| *id == param.id)
    {
        return zh.clone();
    }
    i18n::tr(&param.name).to_owned()
}

/// Localized label of parameter `id` of the effect at `effect` (the
/// timeline's property names); `None` when the effect is unknown.
pub(crate) fn param_label_of(match_name: &str, id: u32) -> Option<String> {
    let def = global()?.def(match_name)?;
    let param = def.param(id)?;
    Some(param_label(match_name, param))
}

/// Localized popup options of `param` of effect `match_name`.
pub(crate) fn option_labels(match_name: &str, param: &EffectParamDef) -> Vec<String> {
    let bae_model::effect::ParamKind::Popup { options } = &param.kind else {
        return Vec::new();
    };
    if chinese()
        && let Some(info) = global().and_then(|r| r.info(match_name))
        && let Some((_, zh)) = info.param_options_zh.iter().find(|(id, _)| *id == param.id)
        && zh.len() == options.len()
    {
        return zh.clone();
    }
    options.iter().map(|o| i18n::tr(o).to_owned()).collect()
}

/// The layer an effect instance belongs to.
pub(crate) fn layer_of(effect: &PrimPath) -> Option<PrimPath> {
    bae_motion::effect::layer_of_effect(effect)
}

/// Masks of the layer `effect` is on, Mask 1 first (AE's mask popups).
pub(crate) fn layer_masks(stage: &Stage, effect: &PrimPath) -> Vec<PrimPath> {
    let Some(masks) = layer_of(effect).and_then(|l| l.child("masks").ok()) else {
        return Vec::new();
    };
    stage
        .children(&masks)
        .into_iter()
        .filter(|m| stage.prim(m).is_some_and(|v| v.is_a(&PrimType::MASK)))
        .collect()
}

/// A dragged effect (Effects & Presets → a layer in the timeline or viewer).
#[derive(Clone, Debug)]
pub(crate) struct EffectDrag(pub String);

/// The match name of the last applied effect (Effect ▸ Last Effect).
static LAST: Mutex<Option<String>> = Mutex::new(None);

/// Applies `match_name` to `layers` (one undo step) and remembers it for
/// Effect ▸ Last Effect.
pub(crate) fn apply(cx: &mut PanelContext<'_>, layers: &[PrimPath], match_name: &str) {
    if layers.is_empty() {
        return;
    }
    cx.invoke_with(
        "effect.apply",
        json!({"layers": layers, "match_name": match_name}),
    );
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some(match_name.to_owned());
}

fn selected_layers(cx: &PanelContext<'_>) -> Vec<PrimPath> {
    let session = common::session(cx.resources);
    if common::active_comp(cx.stage, &session).is_none() {
        return Vec::new();
    }
    let s = session.lock();
    s.selected_layers
        .iter()
        .filter(|l| cx.stage.has_prim(l))
        .cloned()
        .collect()
}

/// Titles of the actions (Keyboard Shortcuts, command palette).
pub(crate) const ACTIONS: &[(&str, &str)] = &[
    (APPLY, "Apply Effect"),
    (LAST_EFFECT, "Last Effect"),
    (REMOVE_ALL, "Remove All"),
];

/// Handles the Effect menu's actions.
pub(crate) struct EffectActions;

impl ActionHandler for EffectActions {
    fn actions(&self) -> Vec<String> {
        vec![APPLY.into(), LAST_EFFECT.into(), REMOVE_ALL.into()]
    }

    fn workspace(&self) -> Option<&str> {
        Some(crate::menus::WORKSPACE)
    }

    fn handle(&self, inv: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        let layers = selected_layers(cx);
        match inv.id.as_str() {
            APPLY => {
                if let Some(m) = inv.params.get("match_name").and_then(|v| v.as_str()) {
                    apply(cx, &layers, m);
                }
                true
            }
            LAST_EFFECT => {
                let last = LAST.lock().unwrap_or_else(|e| e.into_inner()).clone();
                if let Some(m) = last {
                    apply(cx, &layers, &m);
                }
                true
            }
            REMOVE_ALL => {
                if !layers.is_empty() {
                    cx.invoke_with("effect.remove_all", json!({"layers": layers}));
                }
                true
            }
            _ => false,
        }
    }
}

/// Whether the selection can take an effect (an active comp with selected
/// layers).
fn has_layers(cx: &bae_ui::menu::MenuContext<'_>) -> bool {
    let session = common::session(cx.resources);
    common::active_comp(cx.stage, &session).is_some() && !session.lock().selected_layers.is_empty()
}

/// Enabled check of Effect ▸ Last Effect.
pub(crate) fn last_effect_enabled(cx: &bae_ui::menu::MenuContext<'_>) -> bool {
    LAST.lock().unwrap_or_else(|e| e.into_inner()).is_some() && has_layers(cx)
}

/// Enabled check of Effect ▸ Remove All: selected layers with effects.
pub(crate) fn remove_all_enabled(cx: &bae_ui::menu::MenuContext<'_>) -> bool {
    has_layers(cx)
        && common::session(cx.resources)
            .lock()
            .selected_layers
            .iter()
            .any(|l| !bae_motion::effect::effects_of(cx.stage, l).is_empty())
}

/// The effect entries of one category submenu of the Effect menu (`path` =
/// `["Effect", <category>]`, `levels` = the submenu's position): every AE
/// effect of the category, sorted by its localized name like AE, greyed
/// unless BAE implements it and layers are selected.
pub(crate) fn menu_items(
    path: &[&'static str],
    levels: &[MenuPos],
    category: &str,
    workspace: &str,
) -> Vec<(&'static str, MenuItemSpec)> {
    directory()
        .iter()
        .filter(|e| e.category == category)
        .enumerate()
        .map(|(i, e)| {
            let mut at = levels.to_vec();
            at.push(MenuPos::new(0, i as i32 + 1).sorted());
            let match_name = e.match_name;
            let spec = MenuItemSpec::new(path, e.name, APPLY)
                .workspace(workspace)
                .params(json!({"match_name": match_name}))
                .at(at)
                .enabled_when(move |cx| {
                    registries(cx.resources).is_some_and(|r| r.def(match_name).is_some())
                        && has_layers(cx)
                });
            (e.name, spec)
        })
        .collect()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod directory_tests {
    use super::*;

    #[test]
    fn directory_matches_ae_menu() {
        let list = directory();
        assert_eq!(list.len(), 299);
        let categories: std::collections::BTreeSet<&str> =
            list.iter().map(|e| e.category).collect();
        let mut want = crate::catalog::effect_categories().to_vec();
        want.sort_unstable();
        assert_eq!(categories.into_iter().collect::<Vec<_>>(), want);
        for m in ["ADBE Gaussian Blur 2", "ADBE Tint", "ADBE Exposure2"] {
            assert!(list.iter().any(|e| e.match_name == m), "{m}");
        }
        assert!(list.iter().all(|e| !e.name_zh.is_empty()));
    }
}
