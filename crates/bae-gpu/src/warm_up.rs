//! Work done before its first use — shader compilation, renderers, font and
//! glyph caches, decoders, audio devices, plugins — run in the background as
//! soon as the GPU context exists, so no first action stalls the UI.
//!
//! Modules register tasks in the app registry
//! (`register_arc::<dyn WarmUp>(id, task)`); the app runs them in
//! [`WarmUp::priority`] order on one low-priority thread ([`spawn`]). Code that
//! needs something a task is still preparing must not wait for it on the UI
//! thread (see `bae_render::is_interactive_thread`): it skips the frame and
//! the app repaints when the warm-up is done.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::GpuContext;

static RUNNING: AtomicBool = AtomicBool::new(false);

/// Whether the warm-up is still running: first-use work it covers should not
/// be done on the UI thread meanwhile (skip the frame instead).
pub fn is_running() -> bool {
    RUNNING.load(Ordering::Acquire)
}

/// One piece of warm-up work.
pub trait WarmUp: Send + Sync {
    /// Shown in the log ("vector renderer", "fonts", "effects").
    fn name(&self) -> &str;

    /// Lower runs first: what the user is likely to touch first.
    fn priority(&self) -> u8 {
        50
    }

    /// Does the work (on the warm-up thread).
    fn run(&self, gpu: &Arc<GpuContext>) -> Result<(), String>;
}

/// Runs `tasks` in priority order on a new low-priority thread, logs the time
/// of each and the total, then calls `done`.
pub fn spawn(
    gpu: Arc<GpuContext>,
    mut tasks: Vec<Arc<dyn WarmUp>>,
    done: impl FnOnce() + Send + 'static,
) {
    tasks.sort_by_key(|t| t.priority());
    RUNNING.store(true, Ordering::Release);
    let spawned = std::thread::Builder::new()
        .name("bae-warm-up".into())
        .spawn(move || {
            lower_priority();
            let start = Instant::now();
            let mut times = Vec::with_capacity(tasks.len());
            for task in &tasks {
                let t = Instant::now();
                if let Err(e) = task.run(&gpu) {
                    tracing::warn!("warm-up {}: {e}", task.name());
                }
                times.push(format!("{} {:.0?}", task.name(), t.elapsed()));
            }
            RUNNING.store(false, Ordering::Release);
            tracing::info!(
                "warm-up done in {:.0?} ({})",
                start.elapsed(),
                times.join(", ")
            );
            done();
        });
    if let Err(e) = spawned {
        RUNNING.store(false, Ordering::Release);
        tracing::warn!("warm-up thread: {e}");
    }
}

/// Lowers the calling thread's priority (utility QoS on Apple platforms,
/// below normal on Windows) so warm-up does not compete with the UI.
pub fn lower_priority() {
    #[cfg(target_vendor = "apple")]
    // SAFETY: changes only the calling thread's QoS class.
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0);
    }
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
        };
        // SAFETY: GetCurrentThread is the calling thread's pseudo handle.
        let _ = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) };
    }
}
