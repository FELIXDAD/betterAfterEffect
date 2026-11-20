//! In-process vs out-of-process hosting.
//!
//! `docs/ARCHITECTURE.md` §15 prefers running AE plug-ins out of process so a
//! crashing plug-in cannot take BAE down. Out-of-process hosting is also the
//! only way to run a foreign-architecture binary (x86_64 `.plugin` on Apple
//! Silicon through Rosetta 2) and it lets a notarized, library-validated BAE
//! app keep its hardened-runtime entitlements while a separate helper loads
//! third-party code (spec §3.1).
//!
//! Protocol shape (spec §2.4):
//!
//! * one sandbox process per plug-in *module* (all instances of the module's
//!   effects share it, matching AE's per-module globals and serialisation);
//! * frames travel through shared memory slots ([`FrameSlot`]) laid out as AE
//!   worlds, so the sandbox hands slot memory to the plug-in directly;
//! * control messages ([`HostToSandbox`], [`SandboxToHost`]) are small and go
//!   over a pipe/socket; plug-in callbacks that need host state (param
//!   checkout at other times, layer checkout bounds, suite calls that read
//!   the Stage) become synchronous RPCs back to the host;
//! * per-pixel suites (iterate, sampling, transform...) are implemented
//!   *inside* the sandbox — never as RPCs.

use crate::params::ParamIndex;
use crate::pixel::{PixelDepth, WorldDesc};
use crate::selector::{InstanceId, Selector};
use crate::smart::{CheckoutResult, RenderRequest};
use crate::time::Flicks;

/// Where a plug-in module runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostingMode {
    /// Loaded into the BAE process (fastest; a crash is fatal). Only for
    /// modules the user marked trusted.
    InProcess,
    /// Native-architecture helper process (default).
    Sandbox,
    /// Foreign-architecture helper under OS translation (Rosetta 2 / x64
    /// emulation); slower CPU code, same protocol.
    TranslatedSandbox,
}

/// A shared-memory region holding one world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameSlot {
    /// Index of the shared-memory segment.
    pub segment: u32,
    /// Byte offset of the world's first pixel within the segment
    /// (page-aligned so GPU staging can map it directly where supported).
    pub offset: u64,
    /// World layout (rowbytes, depth, origin, extent).
    pub world: WorldDesc,
}

/// Messages from host to sandbox.
#[derive(Clone, Debug, PartialEq)]
pub enum HostToSandbox {
    /// Call the entry point with a selector for an instance; `in_data` is a
    /// serialized subset (time, scale, downsample, quality, ...).
    Call {
        /// Correlation id.
        call: u64,
        /// Selector.
        selector: Selector,
        /// Instance (if any).
        instance: Option<InstanceId>,
        /// Serialized in-data fields.
        in_data: Vec<u8>,
        /// Input worlds already written into slots.
        inputs: Vec<(ParamIndex, FrameSlot)>,
        /// Output world slot.
        output: Option<FrameSlot>,
    },
    /// Answer to a callback RPC.
    CallbackReply {
        /// Correlation id of the callback.
        callback: u64,
        /// Serialized reply.
        payload: Vec<u8>,
    },
    /// Abort the in-flight call (user cancel / superseded render).
    Abort {
        /// Call to abort.
        call: u64,
    },
}

/// Messages from sandbox to host.
#[derive(Clone, Debug, PartialEq)]
pub enum SandboxToHost {
    /// The call returned.
    Return {
        /// Correlation id.
        call: u64,
        /// Plug-in error code (0 = success).
        err: i32,
        /// Serialized out-data (flags, sizes, return message, flat sequence
        /// data when the selector produced it).
        out_data: Vec<u8>,
    },
    /// The plug-in invoked a host callback that needs host state.
    Callback {
        /// Correlation id.
        callback: u64,
        /// Which callback.
        kind: CallbackKind,
    },
    /// Progress report (also polled for abort).
    Progress {
        /// Call.
        call: u64,
        /// Done.
        current: u32,
        /// Total.
        total: u32,
    },
}

/// Host callbacks that cross the process boundary.
#[derive(Clone, Debug, PartialEq)]
pub enum CallbackKind {
    /// Evaluate a param of the instance at another time.
    CheckoutParam {
        /// Param.
        param: ParamIndex,
        /// Time.
        time: Flicks,
    },
    /// SmartFX checkout during pre-render (bounds only).
    CheckoutLayer {
        /// Param.
        param: ParamIndex,
        /// Time.
        time: Flicks,
        /// Request.
        request: RenderRequest,
    },
    /// SmartFX pixel checkout during render: the host renders/copies the
    /// planned input into a slot and replies with it.
    CheckoutLayerPixels {
        /// Checkout id from pre-render.
        checkout_id: i32,
        /// Requested depth.
        depth: PixelDepth,
    },
    /// A suite function that reads or writes host state (serialized args).
    Suite {
        /// Suite key (BAE naming) as a stable string for logs.
        suite: &'static str,
        /// Function index within the suite version.
        function: u16,
        /// Arguments.
        args: Vec<u8>,
    },
}

/// Bounds answer sent back for [`CallbackKind::CheckoutLayer`].
pub type CheckoutReply = CheckoutResult;

/// Crash policy after a sandbox dies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrashPolicy {
    /// Restart the sandbox and RESETUP every instance from its last flat
    /// state; the failed frame renders as the placeholder (checkerboard +
    /// badge) and the error is reported.
    RestartAndResetup,
    /// After repeated crashes, disable the module for the session.
    Disable,
}

/// Simple restart budget: allow `max` crashes per `window` renders.
pub fn crash_policy(crashes_in_window: u32, max: u32) -> CrashPolicy {
    if crashes_in_window < max {
        CrashPolicy::RestartAndResetup
    } else {
        CrashPolicy::Disable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy() {
        assert_eq!(crash_policy(0, 3), CrashPolicy::RestartAndResetup);
        assert_eq!(crash_policy(3, 3), CrashPolicy::Disable);
    }
}
