//! The device's one producer owns the live mix and decoders across seeks.
use super::{BLOCK_FRAMES, Block, Monitor};
use crate::audio::time_to_sample;
use crate::mix::AudioMix;
use bae_model::{Time, TimeRange};
use rtrb::Producer;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

pub(super) enum Command {
    Prepare(AudioMix),
    Seek {
        epoch: u64,
        at: Time,
        range: TimeRange,
    },
    Stop,
}

struct Cursor {
    epoch: u64,
    next: i64,
    first: i64,
    end: i64,
}

pub(super) fn run(
    messages: &Receiver<Command>,
    mut ring: Producer<Block>,
    monitor: &Monitor,
    rate: u32,
) {
    let mut mix = AudioMix::default();
    let mut cursor: Option<Cursor> = None;
    loop {
        let message = if cursor.is_none() {
            match messages.recv() {
                Ok(msg) => Some(msg),
                Err(_) => return,
            }
        } else if ring.slots() == 0 {
            match messages.recv_timeout(Duration::from_millis(3)) {
                Ok(msg) => Some(msg),
                Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => continue,
            }
        } else {
            match messages.try_recv() {
                Ok(msg) => Some(msg),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            }
        };
        if let Some(msg) = message {
            match msg {
                Command::Prepare(new_mix) => {
                    mix = new_mix;
                    mix.prepare_sources(rate);
                    cursor = None;
                }
                Command::Seek { epoch, at, range } => {
                    let first = time_to_sample(range.start, rate);
                    let end = time_to_sample(range.end, rate).max(first);
                    cursor = Some(Cursor {
                        epoch,
                        first,
                        end,
                        next: time_to_sample(at, rate).clamp(first, end),
                    });
                }
                Command::Stop => cursor = None,
            }
            continue;
        }
        let Some(play) = &mut cursor else { continue };
        let mut chunk = ring.write_chunk(1).expect("one producer and a free slot");
        let (a, b) = chunk.as_mut_slices();
        let block = a.first_mut().or_else(|| b.first_mut()).unwrap();
        block.epoch = play.epoch;
        if play.next >= play.end && monitor.repeat.load(Ordering::Relaxed) {
            play.next = play.first;
        }
        let count = (play.end - play.next).min(BLOCK_FRAMES as i64) as usize;
        block.len = count;
        if let Err(error) = mix.render(play.next, rate, &mut block.samples[..count]) {
            tracing::warn!(%error, "Audio preview decode failed");
            block.len = 0;
        }
        play.next += block.len as i64;
        // A zero-length block marks the precise end for the consumer.
        if block.len == 0 {
            cursor = None;
        }
        chunk.commit_all();
    }
}
