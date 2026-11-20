//! Renderer errors.

use std::path::PathBuf;

use bae_model::PrimPath;

/// Errors from evaluation, graph building, GPU execution and output.
#[derive(Debug, thiserror::Error)]
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
}
