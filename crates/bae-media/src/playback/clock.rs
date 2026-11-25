//! DAC timestamps published by the output callback. Several buffers may be
//! queued ahead of the speaker, so retain recent timestamps across underruns.
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
struct Stamp {
    sequence: AtomicU64,
    epoch: AtomicU64,
    nanos: AtomicU64,
    first: AtomicU64,
    frames: AtomicU64,
}

#[derive(Default)]
pub(super) struct Clock {
    stamps: [Stamp; 32],
    written: AtomicU64,
}

impl Clock {
    // One writer. An odd sequence marks an entry being replaced. The audio
    // callback never waits for readers.
    pub(super) fn publish(&self, epoch: u64, nanos: u64, first: u64, frames: u64) {
        let n = self.written.load(Ordering::Relaxed);
        let slot = &self.stamps[n as usize % self.stamps.len()];
        slot.sequence.store(2 * n + 1, Ordering::SeqCst);
        slot.epoch.store(epoch, Ordering::SeqCst);
        slot.nanos.store(nanos, Ordering::SeqCst);
        slot.first.store(first, Ordering::SeqCst);
        slot.frames.store(frames, Ordering::SeqCst);
        slot.sequence.store(2 * n + 2, Ordering::SeqCst);
        self.written.store(n + 1, Ordering::Release);
    }

    pub(super) fn heard(&self, epoch: u64, now: u64, rate: u32) -> u64 {
        let end = self.written.load(Ordering::Acquire);
        for n in (end.saturating_sub(self.stamps.len() as u64)..end).rev() {
            let slot = &self.stamps[n as usize % self.stamps.len()];
            let sequence = 2 * n + 2;
            if slot.sequence.load(Ordering::SeqCst) != sequence
                || slot.epoch.load(Ordering::SeqCst) != epoch
            {
                continue;
            }
            let dac = slot.nanos.load(Ordering::SeqCst);
            let first = slot.first.load(Ordering::SeqCst);
            let frames = slot.frames.load(Ordering::SeqCst);
            if dac <= now && slot.sequence.load(Ordering::SeqCst) == sequence {
                let elapsed = u128::from(now - dac) * u128::from(rate) / 1_000_000_000;
                return first + frames.min(elapsed as u64);
            }
        }
        0
    }
}
