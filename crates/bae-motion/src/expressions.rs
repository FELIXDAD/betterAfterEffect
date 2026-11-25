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

/// `attr.bake_expression`: Animation > Keyframe Assistant > Convert Expression to
/// Keyframes. The expression's values become keyframes (they replace the
/// property's keyframes) and the expression is turned off, in one step.
pub struct BakeExpression;

#[derive(Deserialize, JsonSchema)]
pub struct BakedKey {
    /// Keyframe time in flicks (layer time for layer attributes).
    pub time: bae_model::Time,
    pub value: bae_model::Value,
}

#[derive(Deserialize, JsonSchema)]
pub struct BakeExpressionParams {
    pub path: PrimPath,
    pub attr: String,
    /// One per frame, as the evaluator gave them.
    pub keys: Vec<BakedKey>,
}

impl Command for BakeExpression {
    type Params = BakeExpressionParams;
    type Output = ();
    const ID: &'static str = "attr.bake_expression";
    const TITLE: &'static str = "Convert Expression to Keyframes";
    const DESCRIPTION: &'static str = "Replace a property's keyframes with given per-frame values and turn its expression off (Convert Expression to Keyframes).";

    fn run(&self, tx: &mut Transaction<'_>, p: Self::Params) -> Result<(), CommandError> {
        ensure_unlocked(tx.stage(), [&p.path])?;
        crate::util::check_animatable(tx.stage(), &p.path, &p.attr)?;
        let mut spec = crate::query::attr_spec(tx.stage(), &p.path, &p.attr)
            .cloned()
            .unwrap_or_default();
        let source = spec.expression.as_ref().map(|e| e.source.clone());
        spec.keyframes = p
            .keys
            .into_iter()
            .map(|k| (k.time, bae_model::Keyframe::linear(k.value)))
            .collect();
        crate::util::write_attr(tx, &p.path, &p.attr, spec)?;
        if let Some(source) = source {
            SetExpression.run(
                tx,
                SetExpressionParams {
                    path: p.path,
                    attr: p.attr,
                    source: Some(source),
                    enabled: false,
                },
            )?;
        }
        Ok(())
    }
}
