//! GPU time per graph node through timestamp queries, for the timeline's
//! Render Time column only (`Renderer::set_layer_times`). A timestamp is
//! written before every node and after the last one; the results are read
//! back asynchronously and reported a frame or two later.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use bae_gpu::wgpu;
use bae_model::PrimPath;

/// Most timestamps per frame (wgpu's query set limit).
const MAX_QUERIES: u32 = 4096;
/// Readback buffers in flight.
const SLOTS: usize = 3;

const FREE: u8 = 0;
const PENDING: u8 = 1;
const READY: u8 = 2;

struct Slot {
    buffer: wgpu::Buffer,
    state: Arc<AtomicU8>,
    /// The frame's node owners and the number of timestamps written.
    owners: Vec<Option<PrimPath>>,
    count: u32,
}

/// Timestamp queries and their readback ring.
pub(crate) struct GpuTimer {
    query: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    slots: Vec<Slot>,
    /// The slot of the frame being encoded.
    current: Option<usize>,
}

fn readback(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("bae-render.timestamps.readback"),
        size: u64::from(MAX_QUERIES) * 8,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl GpuTimer {
    /// A timer, when the device can write timestamps between passes.
    pub fn new(device: &wgpu::Device) -> Option<Self> {
        let needed =
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        if !device.features().contains(needed) {
            return None;
        }
        Some(Self {
            query: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("bae-render.timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: MAX_QUERIES,
            }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("bae-render.timestamps.resolve"),
                size: u64::from(MAX_QUERIES) * 8,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            slots: (0..SLOTS)
                .map(|_| Slot {
                    buffer: readback(device),
                    state: Arc::new(AtomicU8::new(FREE)),
                    owners: Vec::new(),
                    count: 0,
                })
                .collect(),
            current: None,
        })
    }

    /// Starts timing a frame of `nodes` nodes; false when it cannot (too many
    /// nodes, or every readback still in flight).
    pub fn begin(&mut self, nodes: usize) -> bool {
        self.current = None;
        if nodes as u64 + 1 > u64::from(MAX_QUERIES) {
            return false;
        }
        self.current = self
            .slots
            .iter()
            .position(|s| s.state.load(Ordering::Acquire) == FREE);
        self.current.is_some()
    }

    /// The timestamp before node `index` (or, with `index` = the node count,
    /// after the last).
    pub fn write(&self, encoder: &mut wgpu::CommandEncoder, index: usize) {
        if self.current.is_some() {
            encoder.write_timestamp(&self.query, index as u32);
        }
    }

    /// Resolves the frame's `count` timestamps into its readback buffer.
    pub fn finish(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        count: usize,
        owners: &[Option<PrimPath>],
    ) {
        let Some(slot) = self.current else { return };
        let count = count as u32;
        encoder.resolve_query_set(&self.query, 0..count, &self.resolve, 0);
        let s = &mut self.slots[slot];
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &s.buffer, 0, u64::from(count) * 8);
        s.owners = owners.to_vec();
        s.count = count;
    }

    /// After the frame's submission: starts reading its timestamps back.
    pub fn submitted(&mut self) {
        let Some(slot) = self.current.take() else {
            return;
        };
        let s = &self.slots[slot];
        s.state.store(PENDING, Ordering::Release);
        let state = s.state.clone();
        s.buffer
            .slice(..u64::from(s.count) * 8)
            .map_async(wgpu::MapMode::Read, move |r| {
                state.store(if r.is_ok() { READY } else { FREE }, Ordering::Release);
            });
    }

    /// The newest frame read back: GPU time per top-level layer, in drawing
    /// order.
    pub fn collect(
        &mut self,
        device: &wgpu::Device,
        period_ns: f32,
    ) -> Option<Vec<(PrimPath, Duration)>> {
        if self
            .slots
            .iter()
            .any(|s| s.state.load(Ordering::Acquire) == PENDING)
        {
            let _ = device.poll(wgpu::PollType::Poll);
        }
        let mut newest = None;
        for s in &mut self.slots {
            if s.state.load(Ordering::Acquire) != READY {
                continue;
            }
            let ticks: Option<Vec<u64>> = s
                .buffer
                .slice(..u64::from(s.count) * 8)
                .get_mapped_range()
                .ok()
                .map(|data| {
                    data.as_chunks::<8>()
                        .0
                        .iter()
                        .map(|b| u64::from_le_bytes(*b))
                        .collect()
                });
            s.buffer.unmap();
            s.state.store(FREE, Ordering::Release);
            let Some(ticks) = ticks else { continue };
            let mut out: Vec<(PrimPath, Duration)> = Vec::new();
            for (owner, pair) in s.owners.iter().zip(ticks.windows(2)) {
                let Some(layer) = owner else { continue };
                let ns = pair[1].saturating_sub(pair[0]) as f64 * f64::from(period_ns);
                let time = Duration::from_nanos(ns as u64);
                match out.iter_mut().find(|(l, _)| l == layer) {
                    Some((_, total)) => *total += time,
                    None => out.push((layer.clone(), time)),
                }
            }
            newest = Some(out);
        }
        newest
    }
}
