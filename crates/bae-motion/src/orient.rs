//! Layer ▸ Transform ▸ Auto-Orient (AE: Cmd/Ctrl+Alt+O): `layer.auto_orient`
//! sets how layers turn by themselves — off, along their motion path, or (a
//! camera or light) towards its point of interest — in one undo step.

use bae_model::{Command, CommandError, PrimPath, Transaction, Value};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::query;
use crate::util::{ensure_unlocked, fail, set};

pub struct AutoOrient;

#[derive(Deserialize, JsonSchema)]
pub struct AutoOrientParams {
    pub layers: Vec<PrimPath>,
    /// `off`, `along_path`, or `towards_point_of_interest` (cameras and lights).
    pub mode: String,
}

impl Command for AutoOrient {
    type Params = AutoOrientParams;
    type Output = ();
    const ID: &'static str = "layer.auto_orient";
    const TITLE: &'static str = "Auto-Orient";
    const DESCRIPTION: &'static str = "Set layers' auto-orientation (AE: Layer > Transform > Auto-Orient): off, along the motion path, or towards the point of interest (cameras and lights).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        if !matches!(
            p.mode.as_str(),
            "off" | "along_path" | "towards_point_of_interest"
        ) {
            return Err(fail(
                "mode must be off, along_path or towards_point_of_interest",
            ));
        }
        ensure_unlocked(tx.stage(), &p.layers)?;
        for l in &p.layers {
            let kind = query::layer_kind(tx.stage(), l)
                .ok_or_else(|| fail(format!("{l} is not a layer")))?;
            if p.mode == "towards_point_of_interest" && !matches!(kind.as_str(), "camera" | "light")
            {
                return Err(fail(format!(
                    "{l}: only cameras and lights orient towards a point of interest"
                )));
            }
            set(tx, l, "auto_orient", Value::Enum(p.mode.clone()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bae_model::{AuthorId, CommandRegistry, PrimPath, Stage, Value};
    use serde_json::json;

    #[test]
    fn auto_orient_sets_the_mode_and_keeps_poi_for_cameras() {
        let mut stage = Stage::new();
        let mut reg = CommandRegistry::with_builtins();
        crate::register_commands(&mut reg);
        let author = AuthorId::new("user:test").unwrap();
        let run = |stage: &mut Stage, id: &str, p: serde_json::Value| {
            reg.execute(stage, &author, id, p).map(|o| o.output)
        };
        let comp = run(
            &mut stage,
            "comp.create",
            json!({"width": 64, "height": 64}),
        )
        .unwrap();
        let comp = PrimPath::new(comp["path"].as_str().unwrap()).unwrap();
        let solid = run(
            &mut stage,
            "layer.create",
            json!({"comp": comp, "kind": "solid"}),
        )
        .unwrap();
        let solid = PrimPath::new(solid["path"].as_str().unwrap()).unwrap();
        run(
            &mut stage,
            "layer.auto_orient",
            json!({"layers": [solid], "mode": "along_path"}),
        )
        .unwrap();
        assert_eq!(
            stage.prim(&solid).unwrap().value("auto_orient"),
            Some(&Value::Enum("along_path".into()))
        );
        // A solid has no point of interest.
        assert!(
            run(
                &mut stage,
                "layer.auto_orient",
                json!({"layers": [solid], "mode": "towards_point_of_interest"})
            )
            .is_err()
        );
    }
}
