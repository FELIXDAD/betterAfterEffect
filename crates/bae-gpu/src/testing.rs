//! Test support: one GPU context per test process.
//!
//! Tests that each create their own [`GpuContext`] run several devices at once,
//! because the test harness runs tests in parallel. The app never does that, and on
//! Windows CI (WARP with the D3D12 debug layer) it crashed the test process
//! (STATUS_ACCESS_VIOLATION, #154). GPU tests share [`shared_context`] instead,
//! which is also closer to the app: one device used from several threads.
//!
//! Tests that need a context of their own (a custom descriptor, destroying the
//! device) still call [`GpuContext::new_headless`].

use std::sync::{Arc, OnceLock};

use crate::{GpuContext, GpuContextDescriptor, GpuError};

/// Environment variable that turns "no adapter" from a skip into a test failure
/// (`1`), for machines that are known to have a GPU.
pub const REQUIRE_GPU_ENV: &str = "BAE_REQUIRE_GPU";

enum Shared {
    Ready(Arc<GpuContext>),
    NoAdapter(String),
    Failed(String),
}

/// The process-wide headless context for tests, created on first use from
/// `GpuContextDescriptor::default().with_env()` (so `BAE_GPU_ADAPTER`,
/// `BAE_GPU_VALIDATION` … apply).
///
/// Returns `None` when there is no adapter (the caller skips the test, after a
/// "skipping" line on stderr), or panics when [`REQUIRE_GPU_ENV`] is `1`. Panics
/// when creation fails for another reason.
pub fn shared_context() -> Option<Arc<GpuContext>> {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    let shared = SHARED.get_or_init(|| {
        match GpuContext::new_headless(GpuContextDescriptor::default().with_env()) {
            Ok(ctx) => Shared::Ready(ctx),
            Err(error @ GpuError::NoAdapter { .. }) => Shared::NoAdapter(error.to_string()),
            Err(error) => Shared::Failed(error.to_string()),
        }
    });
    match shared {
        Shared::Ready(ctx) => Some(ctx.clone()),
        Shared::NoAdapter(error) => {
            if std::env::var_os(REQUIRE_GPU_ENV).is_some_and(|v| v == "1") {
                panic!("{REQUIRE_GPU_ENV}=1 but {error}");
            }
            eprintln!("skipping GPU test: {error}");
            None
        }
        Shared::Failed(error) => panic!("GPU context creation failed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_context_is_one_device() {
        let Some(a) = shared_context() else { return };
        let b = shared_context().unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        assert!(GpuContext::live_count() >= 1);
    }
}
