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

impl Batch {
    pub(super) fn write(&self, frame: &mut UniformWriter, offset: u32, count: u32) {
        let id = self.first + u64::from(offset);
        frame
            .u32(224, id as u32)
            .u32(228, count)
            .f32s(
                240,
                &[
                    self.start as f32,
                    (id as f64 + 0.5 - self.area) as f32,
                    self.rate as f32,
                    self.slope as f32,
                ],
            )
            .f32s(256, &[self.life as f32, self.life_slope as f32]);
    }
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
