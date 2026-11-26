//! Animation presets (AE: Animation ▸ Save / Apply Animation Preset, the
//! Effects & Presets panel's * Animation Presets): a preset holds effects with
//! their parameter values, saved as a `.baepreset` file (JSON) in the user
//! presets folder. Applying one adds its effects to the selected layers in one
//! undo step (`effect.apply_preset`).

use std::path::{Path, PathBuf};

use bae_model::{PrimPath, Stage, Time};
use bae_motion::effect::PresetEffect;
use bae_motion::query;
use bae_ui::files::{OpenRequest, SaveRequest, SharedFilePicker};
use bae_ui::{PanelContext, tr};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::session::SharedSession;

/// Extension of BAE's preset files.
pub const EXTENSION: &str = "baepreset";

/// A preset file.
#[derive(Debug, Serialize, Deserialize)]
pub struct Preset {
    /// `bae-animation-preset`.
    pub format: String,
    pub version: u32,
    pub effects: Vec<PresetEffect>,
}

/// The user presets folder (AE: User Presets), created on first use.
pub(crate) fn folder(resources: &bae_app::Resources) -> Option<PathBuf> {
    let dirs = resources.get::<bae_platform::AppDirs>()?;
    Some(dirs.data.join("Presets"))
}

/// The presets in `folder`, by name (file name without the extension).
pub(crate) fn list(folder: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(folder)
        .map(|dir| {
            dir.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == EXTENSION))
                .filter_map(|p| Some((p.file_stem()?.to_string_lossy().into_owned(), p)))
                .collect()
        })
        .unwrap_or_default();
    out.sort_by_key(|a| a.0.to_lowercase());
    out
}

/// The effects to save: the selected ones, else every effect of the one
/// selected layer (AE saves the selected properties).
pub(crate) fn effects_to_save(stage: &Stage, session: &SharedSession) -> Vec<PrimPath> {
    let s = session.lock();
    if !s.selected_effects.is_empty() {
        return s.selected_effects.clone();
    }
    match s.selected_layers.as_slice() {
        [layer] => bae_motion::effect::effects_of(stage, layer),
        _ => Vec::new(),
    }
}

/// The preset of `effects` with their values at comp time `t`.
pub(crate) fn preset_of(stage: &Stage, effects: &[PrimPath], t: Time) -> Preset {
    let effects = effects
        .iter()
        .filter_map(|e| {
            let view = stage.prim(e)?;
            let match_name = query::static_value(&view, "match_name")
                .and_then(|v| v.as_str().map(str::to_owned))?;
            let values = view
                .attr_names()
                .into_iter()
                .filter(|a| a.starts_with("param:"))
                .filter_map(|a| {
                    let key_time = query::to_key_time(stage, e, t);
                    let v = query::sample_attr(stage, e, a, key_time)
                        .or_else(|| query::static_value(&view, a))?;
                    Some((a.to_owned(), v))
                })
                .collect();
            Some(PresetEffect { match_name, values })
        })
        .collect();
    Preset {
        format: "bae-animation-preset".into(),
        version: 1,
        effects,
    }
}

/// Animation ▸ Save Animation Preset...: the save panel in the presets folder,
/// then the file.
pub(crate) fn save(cx: &PanelContext<'_>, session: &SharedSession) {
    let effects = effects_to_save(cx.stage, session);
    if effects.is_empty() {
        return;
    }
    let t = session.lock().active_time();
    let preset = preset_of(cx.stage, &effects, t);
    let dir = folder(cx.resources);
    if let Some(d) = &dir {
        let _ = std::fs::create_dir_all(d);
    }
    let name = effects
        .first()
        .map(|e| query::display_name_of(cx.stage, e))
        .unwrap_or_default();
    let request = SaveRequest {
        title: tr("Save Animation Preset As:").to_owned(),
        filters: vec![(
            tr("Animation Preset").to_owned(),
            vec![EXTENSION.to_owned()],
        )],
        directory: dir,
        file_name: format!("{name}.{EXTENSION}"),
    };
    let Some(path) = SharedFilePicker::save(cx.resources, &request) else {
        return;
    };
    if let Ok(text) = serde_json::to_string_pretty(&preset)
        && std::fs::write(&path, text).is_err()
    {
        crate::dialogs::alert(
            cx,
            tr("Save Animation Preset"),
            tr("The preset could not be saved.").to_owned(),
        );
    }
}

/// Reads a preset file.
pub(crate) fn read(path: &Path) -> Option<Preset> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Applies the preset at `path` to the selected layers.
pub(crate) fn apply_file(cx: &mut PanelContext<'_>, session: &SharedSession, path: &Path) {
    let layers = session.lock().selected_layers.clone();
    if layers.is_empty() {
        return;
    }
    match read(path) {
        Some(p) => {
            cx.invoke_with(
                "effect.apply_preset",
                json!({"layers": layers, "effects": p.effects}),
            );
            cx.invoke_with(
                bae_ui::action::ids::OPEN_PANEL,
                json!({"id": "effect_controls"}),
            );
        }
        None => crate::dialogs::alert(
            cx,
            tr("Apply Animation Preset"),
            tr("The file is not an animation preset.").to_owned(),
        ),
    }
}

/// Animation ▸ Apply Animation Preset...: the open panel, then the preset.
pub(crate) fn apply(cx: &mut PanelContext<'_>, session: &SharedSession) {
    let request = OpenRequest {
        title: tr("Apply Animation Preset").to_owned(),
        filters: vec![(
            tr("Animation Preset").to_owned(),
            vec![EXTENSION.to_owned()],
        )],
        multiple: false,
        folders: false,
    };
    if let Some(path) = SharedFilePicker::open(cx.resources, &request).first() {
        apply_file(cx, session, path);
    }
}

/// Registers the Chinese strings (`MotionUiModule::build`).
pub(crate) fn register(_app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
}

/// Chinese for presets (AE 26).
pub(crate) const ZH: &[(&str, &str)] = &[
    ("Save Animation Preset As:", "动画预设另存为:"),
    ("Animation Preset", "动画预设"),
    ("Save Animation Preset", "保存动画预设"),
    ("Apply Animation Preset", "应用动画预设"),
    ("The preset could not be saved.", "无法保存预设。"),
    (
        "The file is not an animation preset.",
        "该文件不是动画预设。",
    ),
    ("* Animation Presets", "* 动画预设"),
    ("Show Effects", "显示效果"),
    ("Show Animation Presets", "显示动画预设"),
    ("Refresh List", "刷新列表"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_round_trip_through_files() {
        let preset = Preset {
            format: "bae-animation-preset".into(),
            version: 1,
            effects: vec![PresetEffect {
                match_name: "ADBE Gaussian Blur 2".into(),
                values: [("param:1".to_owned(), bae_model::Value::Float(25.0))]
                    .into_iter()
                    .collect(),
            }],
        };
        let dir = std::env::temp_dir().join(format!("bae-presets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("Blur.{EXTENSION}"));
        std::fs::write(&path, serde_json::to_string(&preset).unwrap()).unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        let listed = list(&dir);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, "Blur");
        let back = read(&path).unwrap();
        assert_eq!(back.effects[0].match_name, "ADBE Gaussian Blur 2");
        let _ = std::fs::remove_dir_all(dir);
    }
}
