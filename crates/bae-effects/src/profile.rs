//! Per-step GPU timing of the Rust effects (benchmarks and investigations).
//!
//! While a timer is installed, every dispatch of a Rust effect is recorded in
//! its own compute pass with timestamps (this changes scheduling slightly; do
//! not leave it on for frame-time measurements).

use bae_gpu::timer::GpuTimer;
use parking_lot::Mutex;

pub(crate) static PROFILER: Mutex<Option<GpuTimer>> = Mutex::new(None);

/// Installs `timer`; returns the previous one.
pub fn install(timer: GpuTimer) -> Option<GpuTimer> {
    PROFILER.lock().replace(timer)
}

/// Removes the installed timer.
pub fn take() -> Option<GpuTimer> {
    PROFILER.lock().take()
}
