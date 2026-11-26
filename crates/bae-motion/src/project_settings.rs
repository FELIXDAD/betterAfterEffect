//! File ▸ Project Settings (AE: Cmd/Ctrl+Alt+Shift+K): the project-wide
//! settings on `/project` — color depth, working space, linearize, blending
//! gamma, time display style, expression engine. `project.set_settings`
//! changes any of them in one undo step (creating `/project` the first time).

use bae_model::{Command, CommandError, PrimPath, PrimType, Transaction, Value};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::util::{create, fail, set};

/// The project settings prim.
pub const PROJECT: &str = "/project";

pub struct SetProjectSettings;

#[derive(Default, Deserialize, JsonSchema)]
pub struct ProjectSettingsParams {
    /// Bits per channel: `8`, `16` or `32`.
    #[serde(default)]
    pub bit_depth: Option<String>,
    /// Working color space name; empty = none.
    #[serde(default)]
    pub working_space: Option<String>,
    #[serde(default)]
    pub linearize: Option<bool>,
    #[serde(default)]
    pub blend_colors_using_gamma_1: Option<bool>,
    /// `timecode` or `frames`.
    #[serde(default)]
    pub time_display: Option<String>,
    /// `javascript` or `extendscript`.
    #[serde(default)]
    pub expression_engine: Option<String>,
}

impl Command for SetProjectSettings {
    type Params = ProjectSettingsParams;
    type Output = ();
    const ID: &'static str = "project.set_settings";
    const TITLE: &'static str = "Project Settings";
    const DESCRIPTION: &'static str = "Change the project settings (AE File > Project Settings): color depth (8/16/32), working space, linearize, blend colors using 1.0 gamma, time display style (timecode/frames), expression engine.";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        let path = PrimPath::new(PROJECT).map_err(|e| fail(e.to_string()))?;
        let choice = |v: &Option<String>, options: &[&str], name: &str| match v {
            Some(s) if !options.contains(&s.as_str()) => Err(fail(format!(
                "{name} must be one of {}",
                options.join(", ")
            ))),
            _ => Ok(()),
        };
        choice(&p.bit_depth, &["8", "16", "32"], "bit_depth")?;
        choice(&p.time_display, &["timecode", "frames"], "time_display")?;
        choice(
            &p.expression_engine,
            &["javascript", "extendscript"],
            "expression_engine",
        )?;
        if !tx.stage().has_prim(&path) {
            create(tx, &path, PrimType::PROJECT, None)?;
        }
        let edits: [(&str, Option<Value>); 6] = [
            ("bit_depth", p.bit_depth.map(Value::Enum)),
            ("working_space", p.working_space.map(Value::String)),
            ("linearize", p.linearize.map(Value::Bool)),
            (
                "blend_colors_using_gamma_1",
                p.blend_colors_using_gamma_1.map(Value::Bool),
            ),
            ("time_display", p.time_display.map(Value::Enum)),
            ("expression_engine", p.expression_engine.map(Value::Enum)),
        ];
        for (attr, value) in edits {
            if let Some(v) = value {
                set(tx, &path, attr, v)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bae_model::{AuthorId, CommandRegistry, Stage};
    use serde_json::json;

    #[test]
    fn settings_go_to_the_project_prim_in_one_step() {
        let mut stage = Stage::new();
        let mut reg = CommandRegistry::with_builtins();
        crate::register_commands(&mut reg);
        let author = AuthorId::new("user:test").unwrap();
        reg.execute(
            &mut stage,
            &author,
            "project.set_settings",
            json!({"bit_depth": "32", "time_display": "frames"}),
        )
        .unwrap();
        let view = stage.prim(&PrimPath::new(PROJECT).unwrap()).unwrap();
        assert_eq!(view.value("bit_depth"), Some(&Value::Enum("32".into())));
        assert_eq!(
            view.value("time_display"),
            Some(&Value::Enum("frames".into()))
        );
        assert!(
            reg.execute(
                &mut stage,
                &author,
                "project.set_settings",
                json!({"bit_depth": "10"})
            )
            .is_err()
        );
        assert!(stage.can_undo(&author));
    }
}
