//! Evaluation errors.

use bae_model::PrimPath;

/// Why a comp or attribute could not be evaluated at all. Problems with single
/// attributes (expression errors) do not fail evaluation; they are reported in
/// [`crate::EvaluatedComp::errors`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EvalError {
    #[error("no prim at {0}")]
    NoSuchPrim(PrimPath),
    #[error("{0} is not a Comp")]
    NotAComp(PrimPath),
    #[error("{prim} has no value for {attr} (no opinion and no schema default)")]
    NoValue { prim: PrimPath, attr: String },
}
