use super::{Kind, Scene};
use crate::gpu::UniformWriter;
use bae_render::effect::ScalarTrack;

#[derive(Clone, Copy, Default)]
pub(super) struct Batch {
    first: u64,
    pub count: u32,
    start: f64,
    area: f64,
    rate: f64,
    slope: f64,
    life: f64,
    life_slope: f64,
}

// GPU layout of BirthInterval in shared.wgsl.
pub(super) const INTERVAL_BYTES: usize = 32;

pub(super) struct Page {
    pub count: u32,
    pub intervals: UniformWriter,
}

/// Pack consecutive birth intervals in draw order. An interval crossing a page
/// boundary starts the next page at the same birth ID and fractional ordinal.
pub(super) fn pages(mut batches: &[Batch], capacity: u32) -> impl Iterator<Item = Page> {
    let mut offset = 0;
    std::iter::from_fn(move || {
        batches.first()?;
        let mut page = Page {
            count: 0,
            intervals: UniformWriter::with_size(0),
        };
        while let Some(batch) = batches.first() {
            let count = (batch.count - offset).min(capacity - page.count);
            let first = batch.first + u64::from(offset);
            let at = page.intervals.bytes.len();
            page.intervals
                .u32(at, page.count)
                .u32(at + 4, first as u32)
                .f32s(
                    at + 8,
                    &[
                        batch.start as f32,
                        (first as f64 + 0.5 - batch.area) as f32,
                        batch.rate as f32,
                        batch.slope as f32,
                        batch.life as f32,
                        batch.life_slope as f32,
                    ],
                );
            page.count += count;
            offset += count;
            if offset == batch.count {
                batches = &batches[1..];
                offset = 0;
            }
            if page.count == capacity {
                break;
            }
        }
        Some(page)
    })
}

struct Control<'a> {
    track: Option<&'a ScalarTrack>,
    value: f64,
}
impl Control<'_> {
    fn at(&self, t: f64) -> (f64, f64) {
        self.track
            .map_or((self.value, self.value * t), |curve| curve.at(t))
    }
}

/// One batch per linear interval intersecting the live window. Cumulative areas
/// keep an absolute birth ID across edits in rate, changes of lifetime and seeks.
pub(super) fn batches(kind: Kind, scene: &Scene<'_>) -> Vec<Batch> {
    let (rate_id, life_id, density) = match kind {
        Kind::World => (4, 5, 1000.),
        Kind::Systems => (1, 2, 1000.),
        Kind::Drizzle => (1, 2, 16.),
        _ => {
            return vec![Batch {
                count: scene.count(kind),
                ..Default::default()
            }];
        }
    };
    let control = |id| Control {
        track: scene
            .p
            .p
            .tracks
            .iter()
            .find(|(n, _)| *n == id)
            .map(|(_, c)| c.as_ref()),
        value: scene.p.f(id).max(0.),
    };
    let rate = control(rate_id);
    let life = control(life_id);
    let end = scene.p.p.time.as_seconds().max(0.);
    let start = (end - life.track.map_or(life.value, |c| c.maximum_through(end))).max(0.);
    let mut times = vec![start, end];
    for track in [rate.track, life.track].into_iter().flatten() {
        times.extend(track.breaks(start, end));
    }
    times.sort_unstable_by(f64::total_cmp);
    times.dedup();
    let births = |area: f64| (area * density + 0.5).floor() as u64;
    let earliest = births(rate.at(end).1).saturating_sub(2_000_000);
    let mut batches = Vec::new();
    for interval in times.windows(2) {
        let (a, b) = (interval[0], interval[1]);
        let (r0, area) = rate.at(a);
        let after = rate.at(b).1;
        let r1 = rate.track.map_or(rate.value, |c| c.incoming(b));
        let first = births(area).max(earliest);
        let count = births(after).saturating_sub(first) as u32;
        let l0 = life.at(a).0;
        let l1 = life.track.map_or(life.value, |c| c.incoming(b));
        if count == 0 || b + l0.max(l1) <= end {
            continue;
        }
        batches.push(Batch {
            first,
            count,
            start: a,
            area: area * density,
            rate: r0 * density,
            slope: (r1 - r0) * density / (b - a),
            life: l0,
            life_slope: (l1 - l0) / (b - a),
        });
    }
    if batches.is_empty() {
        batches.push(Batch::default());
    }
    batches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_intervals_preserve_slots_and_birth_ids_across_draws() {
        const CAPACITY: u32 = 65_536;
        let mut batches: Vec<_> = (0..64u64)
            .map(|i| {
                let first = i * 4096;
                Batch {
                    first,
                    count: 1024,
                    area: first as f64 + 0.25,
                    ..Batch::default()
                }
            })
            .collect();
        assert_eq!(pages(&batches, CAPACITY).count(), 1);
        batches[63].count += 17;
        let mut draws = pages(&batches, CAPACITY);
        let full = draws.next().unwrap();
        assert_eq!(full.count, CAPACITY);
        let records = full.intervals.bytes.as_chunks::<INTERVAL_BYTES>().0;
        assert_eq!(records.len(), 64);
        for (i, record) in records.iter().enumerate() {
            assert_eq!(record[0..4], (i as u32 * 1024).to_le_bytes());
            assert_eq!(record[4..8], (i as u32 * 4096).to_le_bytes());
        }
        let tail = draws.next().unwrap();
        assert_eq!(tail.count, 17);
        let record = tail.intervals.bytes;
        assert_eq!(record.len(), INTERVAL_BYTES);
        assert_eq!(record[0..4], 0u32.to_le_bytes());
        assert_eq!(record[4..8], (63u32 * 4096 + 1024).to_le_bytes());
        assert_eq!(record[12..16], 1024.25f32.to_le_bytes());
        assert!(draws.next().is_none());
    }
}
