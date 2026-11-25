//! Renderer errors.

use std::fmt;
use std::path::PathBuf;

use bae_model::PrimPath;

/// Errors from evaluation, graph building, GPU execution and output.
///
/// `Debug` prints the message (see the impl below), not a field dump.
#[derive(thiserror::Error)]
pub enum RenderError {
    /// The prim is not a comp or does not exist.
    #[error("{0} is not a composition")]
    NotAComp(PrimPath),
    /// A comp has an invalid size.
    #[error("{path} has an invalid size {width}×{height}")]
    InvalidSize {
        /// The comp.
        path: PrimPath,
        /// Width.
        width: u32,
        /// Height.
        height: u32,
    },
    /// Nested comps reference each other in a cycle (or nest too deep).
    #[error("composition cycle or nesting deeper than {max} at {path}")]
    Cycle {
        /// Where the cycle was detected.
        path: PrimPath,
        /// Maximum nesting depth.
        max: usize,
    },
    /// No source provider is registered for custom content.
    #[error("no layer source provider `{0}`")]
    MissingProvider(String),
    /// Footage could not be opened or decoded.
    #[error("footage {path}: {message}")]
    Footage {
        /// The file.
        path: PathBuf,
        /// What went wrong.
        message: String,
    },
    /// The texture would exceed the device limits.
    #[error("image of {width}×{height} exceeds the GPU texture limit {limit}")]
    TooLarge {
        /// Width.
        width: u32,
        /// Height.
        height: u32,
        /// `max_texture_dimension_2d`.
        limit: u32,
    },
    /// A GPU-layer error (device lost, readback …).
    #[error(transparent)]
    Gpu(#[from] bae_gpu::GpuError),
    /// Model access failed.
    #[error(transparent)]
    Model(#[from] bae_model::ModelError),
    /// Writing output failed.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// Something else (effect or source specific).
    #[error("{0}")]
    Other(String),
    /// Something the app's warm-up is still preparing (a renderer, fonts, a
    /// pipeline). The frame is drawn without it, marked incomplete, and drawn
    /// again when it is ready.
    #[error("{0} is still warming up")]
    WarmingUp(String),
    /// An export was cancelled.
    #[error("export canceled")]
    Canceled,
}

impl RenderError {
    /// Whether this is [`RenderError::WarmingUp`].
    pub fn is_warming_up(&self) -> bool {
        matches!(self, RenderError::WarmingUp(_))
    }
}

impl fmt::Debug for RenderError {
    // The message, not the derived field dump: a derived impl inlines the Debug
    // formatting of every nested wgpu / naga error type into this crate (about
    // 50 KB of release code, a second copy of what bae-gpu already has).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RenderError({self})")
    }
}
