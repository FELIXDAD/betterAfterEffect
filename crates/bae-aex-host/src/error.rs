//! Host-side error model.
//!
//! The AE protocol reports failures with integer `PF_Err` codes (SDK guide,
//! "Effect Basics › Errors"). The P3 FFI shim will translate those codes into
//! [`PluginError`]; this module only fixes the *semantic* categories BAE must
//! handle, so that the render graph and the UI can react uniformly whether the
//! failing effect is a native BAE plug-in or an AE plug-in behind this host.

use core::fmt;

/// Semantic category of an error returned *by a plug-in* to the host
/// (mirrors the documented `PF_Err_*` categories; no numeric values here).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PluginErrorKind {
    /// The plug-in ran out of memory (`PF_Err_OUT_OF_MEMORY`). The host should
    /// purge caches and may retry once at lower quality / smaller ROI.
    OutOfMemory,
    /// Internal structure damaged (`PF_Err_INTERNAL_STRUCT_DAMAGED`): treat the
    /// instance as poisoned; re-run sequence resetup before the next call.
    InternalStructDamaged,
    /// An index argument was out of range (`PF_Err_INVALID_INDEX`).
    InvalidIndex,
    /// The plug-in does not recognise a parameter type
    /// (`PF_Err_UNRECOGNIZED_PARAM_TYPE`): a host bug or version mismatch.
    UnrecognizedParamType,
    /// An invalid callback was invoked (`PF_Err_INVALID_CALLBACK`).
    InvalidCallback,
    /// A callback received a bad parameter (`PF_Err_BAD_CALLBACK_PARAM`).
    BadCallbackParam,
    /// The user (or host) interrupted the call (`PF_Interrupt_CANCEL`). Not a
    /// failure: discard the partial result silently.
    Interrupted,
    /// Keyframe text could not be parsed (`PF_Err_CANNOT_PARSE_KEYFRAME_TEXT`).
    CannotParseKeyframeText,
    /// Any other non-zero code, kept verbatim for diagnostics.
    Other(i32),
}

/// An error reported by a hosted plug-in, together with the optional message
/// the plug-in placed in its out-data return message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginError {
    /// Category.
    pub kind: PluginErrorKind,
    /// Message from the plug-in (already converted to UTF-8), if any.
    pub message: Option<String>,
    /// Whether the plug-in asked for the message to be shown as an error
    /// dialog rather than an informational one.
    pub display_as_error: bool,
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AE plug-in error {:?}", self.kind)?;
        if let Some(m) = &self.message {
            write!(f, ": {m}")?;
        }
        Ok(())
    }
}

impl std::error::Error for PluginError {}

/// Errors raised by the host side of the translation layer itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostError {
    /// A selector was sent in a lifecycle state where the protocol forbids it.
    ProtocolViolation {
        /// Human-readable description of the violated rule.
        rule: &'static str,
    },
    /// A value could not be represented exactly in the AE representation and
    /// the caller asked for exactness (e.g. time, fixed point).
    Inexact {
        /// What was being converted.
        what: &'static str,
    },
    /// A value is outside the representable range of the AE type.
    OutOfRange {
        /// What was being converted.
        what: &'static str,
    },
    /// A buffer description is inconsistent (rowbytes too small, length too
    /// short, misaligned...).
    BadBuffer {
        /// Description of the problem.
        reason: &'static str,
    },
    /// A SmartFX checkout rule was violated by the plug-in.
    Checkout {
        /// Description of the problem.
        reason: &'static str,
    },
    /// The plug-in returned an error.
    Plugin(PluginError),
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::ProtocolViolation { rule } => write!(f, "protocol violation: {rule}"),
            HostError::Inexact { what } => write!(f, "inexact conversion of {what}"),
            HostError::OutOfRange { what } => write!(f, "{what} out of range"),
            HostError::BadBuffer { reason } => write!(f, "bad buffer: {reason}"),
            HostError::Checkout { reason } => write!(f, "checkout rule violated: {reason}"),
            HostError::Plugin(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for HostError {}

impl From<PluginError> for HostError {
    fn from(e: PluginError) -> Self {
        HostError::Plugin(e)
    }
}

/// Convenience alias.
pub type Result<T, E = HostError> = core::result::Result<T, E>;
