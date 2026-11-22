use crate::ids::{InvalidId, SublayerId};
use crate::path::{PathError, PrimPath};
use crate::time::Time;

/// Errors from stage and sublayer operations.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error(transparent)]
    Path(#[from] PathError),
    #[error(transparent)]
    Id(#[from] InvalidId),
    #[error("no prim spec at {path} in sublayer {sublayer}")]
    PrimNotFound {
        sublayer: SublayerId,
        path: PrimPath,
    },
    #[error("prim {0} does not exist on the stage")]
    NoSuchPrim(PrimPath),
    #[error(
        "prim {0} comes from a reference; override its attributes instead of deleting, moving or renaming it"
    )]
    ProvidedByReference(PrimPath),
    #[error("a prim spec already exists at {path} in sublayer {sublayer}")]
    PrimExists {
        sublayer: SublayerId,
        path: PrimPath,
    },
    #[error("the pseudo-root / cannot be edited")]
    RootNotEditable,
    #[error("cannot move {from} into its own subtree ({to})")]
    CyclicMove { from: PrimPath, to: PrimPath },
    #[error("no keyframe at {time} on {path}.{attr}")]
    KeyframeNotFound {
        path: PrimPath,
        attr: String,
        time: Time,
    },
    #[error("unknown sublayer {0}")]
    UnknownSublayer(SublayerId),
    #[error("sublayer {0} already exists")]
    SublayerExists(SublayerId),
    #[error("sublayer {0} is not in the layer stack")]
    NotInStack(SublayerId),
    /// A guarded transaction ([`crate::Stage::transact_guarded`]) tried to write a
    /// sublayer its guard does not allow. Nothing was written to that sublayer.
    #[error("writing sublayer {0} is not allowed")]
    WriteDenied(SublayerId),
    /// The layer's Lock switch is on: it, its properties and keyframes cannot
    /// be changed or deleted until it is unlocked (AE). Only `locked` itself
    /// can be set.
    #[error("layer \"{name}\" is locked; unlock it first ({layer})")]
    LayerLocked { layer: PrimPath, name: String },
    #[error("{path}.{attr}: expected {expected}, got {got}")]
    TypeMismatch {
        path: PrimPath,
        attr: String,
        expected: String,
        got: String,
    },
    #[error("{path}.{attr} cannot be animated")]
    NotAnimatable { path: PrimPath, attr: String },
    #[error("invalid attribute name {0:?}")]
    InvalidAttrName(String),
    #[error("storage error: {0}")]
    Storage(String),
}

impl From<loro::LoroError> for ModelError {
    fn from(e: loro::LoroError) -> Self {
        ModelError::Storage(e.to_string())
    }
}

pub type Result<T, E = ModelError> = std::result::Result<T, E>;

/// Attribute names: non-empty, letters/digits/`_`/`-`, `:` for namespaces
/// (e.g. `transform:position`).
pub fn validate_attr_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 256
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ':'))
        && !name.starts_with(':')
        && !name.ends_with(':');
    if ok {
        Ok(())
    } else {
        Err(ModelError::InvalidAttrName(name.to_owned()))
    }
}
