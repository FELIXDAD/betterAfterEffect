//! BAE colour management (workstream 06).
//!
//! Professional colour management without OpenColorIO: colour spaces as data,
//! exact CPU reference maths, and merged transforms that compile to WGSL
//! snippets for the GPU render graph.
//!
//! * [`primaries`] — chromaticities, RGB ↔ XYZ, chromatic adaptation.
//! * [`transfer`] — tone curves: sRGB, Rec.709, gamma, PQ, HLG, ACEScc/cct,
//!   S-Log3, LogC3, LogC4, V-Log, Canon Log 3.
//! * [`space`] — [`ColorSpace`] and the [`builtin`] library; every space reaches
//!   the ACES2065-1 reference.
//! * [`transform`] — [`ColorTransform`]: an op list that merges adjacent ops
//!   (matrices multiply, inverse curves cancel), with a CPU reference.
//! * [`wgsl`] — [`GpuSnippet`]: WGSL + uniform bytes + LUT textures for the
//!   render graph; see the module docs for the splicing contract.
//! * [`context`] — [`WorkingSpace`] (AE's non-linear / linearized blending),
//!   [`DisplayTarget`], [`ColorContext`].
//! * [`interpret`] — footage colour space detection and Interpret Footage
//!   overrides.
//! * [`lut`] — 1D / 3D LUT data.
//!
//! The crate has no GPU, window or platform dependency (ARCHITECTURE §12);
//! display profiles and HDR state come from `bae-gpu` / `bae-platform`.

pub mod context;
pub mod interpret;
pub mod lut;
pub mod math;
pub mod primaries;
pub mod space;
pub mod transfer;
pub mod transform;
pub mod wgsl;

pub use context::{
    BlendLinearization, BlendSpace, ColorContext, DisplayTarget, SurfaceEncoding, UnmanagedGamma,
    ViewTransform, WorkingSpace,
};
pub use interpret::{Interpretation, LinearLight, SourceTags};
pub use lut::{Interpolation, Lut1d, Lut3d};
pub use math::{Affine, Mat3};
pub use primaries::{Adaptation, Chromaticities};
pub use space::{ColorSpace, ColorSpaceKind, Encoding, builtin, convert};
pub use transfer::TransferFunction;
pub use transform::{ColorOp, ColorTransform};
pub use wgsl::{GpuSnippet, LutDimension, LutTexture};

/// Errors of colour management.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ColorError {
    /// A colour space id that no resolver knows.
    #[error("unknown color space `{0}`")]
    UnknownSpace(String),
    /// The space can only be converted from, not to (no inverse transform).
    #[error("color space `{0}` has no transform from the reference space")]
    OneWay(String),
    /// An op has no inverse.
    #[error("not invertible: {0}")]
    NotInvertible(&'static str),
}
