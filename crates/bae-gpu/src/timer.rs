//! GPU timing with timestamp queries.
//!
//! ```no_run
//! # fn demo(ctx: &bae_gpu::GpuContext, texture_view: &bae_gpu::wgpu::TextureView) {
//! use bae_gpu::timer::GpuTimer;
//! let Some(mut timer) = GpuTimer::new(ctx, 16) else { return }; // unsupported
//! let mut encoder = ctx.device().create_command_encoder(&Default::default());
//! let scope = timer.scope("composite").unwrap();
//! {
//!     let _pass = encoder.begin_render_pass(&bae_gpu::wgpu::RenderPassDescriptor {
//!         timestamp_writes: Some(timer.render_pass_writes(scope)),
//!         ..Default::default()
//!     });
//! }
//! timer.resolve(&mut encoder);
//! ctx.queue().submit([encoder.finish()]);
//! for sample in timer.read(ctx).unwrap() {
//!     println!("{}: {:.3} ms", sample.label, sample.millis());
//! }
//! # }
//! ```

use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use crate::context::{GpuContext, GpuError};

/// A begin/end timestamp pair reserved with [`GpuTimer::scope`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimerScope {
    index: u32,
}

impl TimerScope {
    fn begin(self) -> u32 {
        self.index * 2
    }

    fn end(self) -> u32 {
        self.index * 2 + 1
    }
}

/// A measured scope.
#[derive(Debug, Clone, PartialEq)]
pub struct TimerSample {
    /// Label given to [`GpuTimer::scope`].
    pub label: Arc<str>,
    /// GPU time between the begin and end timestamps.
    pub duration: Duration,
}

impl TimerSample {
    /// Duration in milliseconds.
    pub fn millis(&self) -> f64 {
        self.duration.as_secs_f64() * 1000.0
    }
}

/// Measures GPU time of passes (always, when `TIMESTAMP_QUERY` is enabled) or of
/// arbitrary encoder ranges (when `TIMESTAMP_QUERY_INSIDE_ENCODERS` is enabled).
/// One timer serves one frame at a time: reserve scopes, record, [`Self::resolve`],
/// submit, [`Self::read`], then [`Self::reset`] for the next frame.
pub struct GpuTimer {
    query_set: wgpu::QuerySet,
    resolve_buffer: wgpu::Buffer,
    readback_buffer: wgpu::Buffer,
    capacity: u32,
    labels: Vec<Arc<str>>,
    period_ns: f64,
    inside_encoders: bool,
}

impl std::fmt::Debug for GpuTimer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuTimer")
            .field("capacity", &self.capacity)
            .field("used", &self.labels.len())
            .field("inside_encoders", &self.inside_encoders)
            .finish_non_exhaustive()
    }
}

impl GpuTimer {
    /// Creates a timer for up to `max_scopes` scopes per frame, or `None` when the
    /// device has no timestamp support.
    pub fn new(ctx: &GpuContext, max_scopes: u32) -> Option<Self> {
        if !ctx.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let capacity = max_scopes.clamp(1, wgpu::QUERY_SET_MAX_QUERIES / 2);
        let count = capacity * 2;
        let device = ctx.device();
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("bae.timer.queries"),
            ty: wgpu::QueryType::Timestamp,
            count,
        });
        let size = u64::from(count) * wgpu::QUERY_SIZE as u64;
        let resolve_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bae.timer.resolve"),
            size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bae.timer.readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Some(Self {
            query_set,
            resolve_buffer,
            readback_buffer,
            capacity,
            labels: Vec::new(),
            period_ns: f64::from(ctx.queue().get_timestamp_period()),
            inside_encoders: ctx
                .features()
                .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS),
        })
    }

    /// `true` if [`Self::write_begin`] / [`Self::write_end`] work (timestamps
    /// anywhere in an encoder, not only on pass boundaries).
    pub fn supports_encoder_timestamps(&self) -> bool {
        self.inside_encoders
    }

    /// Reserves a scope, or `None` when the timer is full.
    pub fn scope(&mut self, label: impl Into<Arc<str>>) -> Option<TimerScope> {
        let index = u32::try_from(self.labels.len()).ok()?;
        if index >= self.capacity {
            return None;
        }
        self.labels.push(label.into());
        Some(TimerScope { index })
    }

    /// Timestamp writes measuring a whole render pass.
    pub fn render_pass_writes(&self, scope: TimerScope) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(scope.begin()),
            end_of_pass_write_index: Some(scope.end()),
        }
    }

    /// Timestamp writes measuring a whole compute pass.
    pub fn compute_pass_writes(&self, scope: TimerScope) -> wgpu::ComputePassTimestampWrites<'_> {
        wgpu::ComputePassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(scope.begin()),
            end_of_pass_write_index: Some(scope.end()),
        }
    }

    /// Writes the begin timestamp of `scope` into `encoder` (outside passes).
    /// Returns `false` (and writes nothing) without encoder timestamp support.
    pub fn write_begin(&self, encoder: &mut wgpu::CommandEncoder, scope: TimerScope) -> bool {
        if self.inside_encoders {
            encoder.write_timestamp(&self.query_set, scope.begin());
        }
        self.inside_encoders
    }

    /// Writes the end timestamp of `scope`; see [`Self::write_begin`].
    pub fn write_end(&self, encoder: &mut wgpu::CommandEncoder, scope: TimerScope) -> bool {
        if self.inside_encoders {
            encoder.write_timestamp(&self.query_set, scope.end());
        }
        self.inside_encoders
    }

    /// Resolves the used queries into the readback buffer. Record after all
    /// measured work, in the last encoder submitted for the frame.
    pub fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        let used = self.used_queries();
        if used == 0 {
            return;
        }
        encoder.resolve_query_set(&self.query_set, 0..used, &self.resolve_buffer, 0);
        encoder.copy_buffer_to_buffer(
            &self.resolve_buffer,
            0,
            &self.readback_buffer,
            0,
            Some(u64::from(used) * wgpu::QUERY_SIZE as u64),
        );
    }

    /// Reads the results after the encoder with [`Self::resolve`] was submitted.
    /// Blocks until the GPU finished that work. Scopes whose timestamps were never
    /// written report zero.
    pub fn read(&self, ctx: &GpuContext) -> Result<Vec<TimerSample>, GpuError> {
        let used = self.used_queries();
        if used == 0 {
            return Ok(Vec::new());
        }
        let byte_len = u64::from(used) * wgpu::QUERY_SIZE as u64;
        let (tx, rx) = mpsc::channel();
        self.readback_buffer
            .map_async(wgpu::MapMode::Read, ..byte_len, move |result| {
                let _ = tx.send(result);
            });
        ctx.device().poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()
            .map_err(|_| GpuError::Map("map callback dropped".into()))?
            .map_err(|e| GpuError::Map(e.to_string()))?;
        let ticks: Vec<u64> = {
            let view = self
                .readback_buffer
                .get_mapped_range(..byte_len)
                .map_err(|e| GpuError::Map(e.to_string()))?;
            view.as_chunks::<8>()
                .0
                .iter()
                .map(|chunk| u64::from_le_bytes(*chunk))
                .collect()
        };
        self.readback_buffer.unmap();
        Ok(self
            .labels
            .iter()
            .enumerate()
            .map(|(i, label)| {
                let begin = ticks[i * 2];
                let end = ticks[i * 2 + 1];
                let nanos = end.saturating_sub(begin) as f64 * self.period_ns;
                TimerSample {
                    label: label.clone(),
                    duration: Duration::from_nanos(nanos as u64),
                }
            })
            .collect())
    }

    /// Forgets all scopes so the timer can be reused for the next frame.
    pub fn reset(&mut self) {
        self.labels.clear();
    }

    fn used_queries(&self) -> u32 {
        // `labels.len() <= capacity <= QUERY_SET_MAX_QUERIES / 2`, so this fits.
        self.labels.len() as u32 * 2
    }
}
