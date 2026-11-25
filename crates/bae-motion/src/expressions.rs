//! `attr.set_expressions`: add, change, enable / disable or remove the expressions
//! of several properties in one step (Animation > Add Expression on a selection,
//! Alt+Shift+=).

use bae_model::commands::{SetExpression, SetExpressionParams};
use bae_model::{Command, CommandError, PrimPath, Transaction};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::util::ensure_unlocked;

pub struct SetExpressions;

#[derive(Deserialize, JsonSchema)]
pub struct ExpressionEdit {
    pub path: PrimPath,
    pub attr: String,
    /// Expression source; null removes the expression.
    pub source: Option<String>,
    /// Whether the expression is enabled (default true).
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, JsonSchema)]
pub struct SetExpressionsParams {
    pub items: Vec<ExpressionEdit>,
}

impl Command for SetExpressions {
    type Params = SetExpressionsParams;
    type Output = ();
    const ID: &'static str = "attr.set_expressions";
    const TITLE: &'static str = "Set Expressions";
    const DESCRIPTION: &'static str = "Set, disable or remove the expressions of several properties in one step (`items`: path, attr, source or null, enabled).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        ensure_unlocked(tx.stage(), p.items.iter().map(|e| &e.path))?;
        for e in p.items {
            SetExpression.run(
                tx,
                SetExpressionParams {
                    path: e.path,
                    attr: e.attr,
                    source: e.source,
                    enabled: e.enabled,
                },
            )?;
        }
        Ok(())
    }
}
