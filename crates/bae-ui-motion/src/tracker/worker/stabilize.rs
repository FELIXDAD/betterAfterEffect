use super::*;
use bae_model::effect::stabilizer::CELLS;

/// Estimate frame-to-frame camera motion on a regular layer-space mesh. The
/// grid is reseeded each frame so features that leave the frame cannot drag
/// the whole solution out of view.
impl Analysis<'_> {
    pub(super) fn stabilize(
        &self,
        quality: i64,
        picture: &mut impl FnMut(Time) -> Result<Image, String>,
        mut emit: impl FnMut(Frame) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut time = self.lo;
        if time >= self.hi {
            return Err("No visible frames to analyze".into());
        }
        let mut previous = picture(time)?;
        let mut path = [DVec2::ZERO; CELLS];
        let frame = |time, path: &[DVec2; CELLS]| Frame {
            time,
            points: path
                .iter()
                .map(|p| Sample {
                    center: p.to_array(),
                    confidence: 100.,
                    attach: None,
                })
                .collect(),
        };
        emit(frame(time, &path))?;
        let detailed = quality & 1 != 0;
        let fast = quality & 2 != 0;
        while time + self.stride < self.hi {
            if self.stop.load(Ordering::Relaxed) {
                return Err("Analysis cancelled".into());
            }
            time += self.stride;
            let image = picture(time)?;
            let size = DVec2::new(
                previous.texture.width() as f64,
                previous.texture.height() as f64,
            );
            let mut observations: [Vec<DVec2>; CELLS] = Default::default();
            let offsets: &[f64] = if detailed {
                &[-0.24, 0.0, 0.24]
            } else if fast {
                &[0.0]
            } else {
                &[-0.16, 0.16]
            };
            for &dy in offsets {
                for &dx in offsets {
                    let regions: Vec<_> = (0..CELLS)
                        .map(|i| {
                            let pos =
                                (DVec2::new((i % 3) as f64 + 0.5 + dx, (i / 3) as f64 + 0.5 + dy)
                                    * size
                                    / 3.)
                                    .round();
                            let center = pos.as_ivec2().to_array();
                            Region {
                                reference: &previous.texture,
                                field: 0,
                                center,
                                search_center: (pos + previous.origin - image.origin)
                                    .round()
                                    .as_ivec2()
                                    .to_array(),
                                feature: [if fast { 12 } else { 18 }; 2],
                                search: [if fast { 44 } else { 68 }; 2],
                            }
                        })
                        .collect();
                    for (batch, chunk) in regions.chunks(8).enumerate() {
                        let hits = self.gpu.matcher.analyze(
                            &self.gpu.gpu,
                            &image.texture,
                            chunk,
                            super::super::gpu::Options {
                                channel: 1,
                                subpixel: true,
                                enhance: 0,
                                field: 0,
                                radius: 1,
                            },
                        )?;
                        for (j, (r, hit)) in chunk.iter().zip(hits).enumerate() {
                            if hit[2] >= 0.65 {
                                let delta = image.origin - previous.origin
                                    + DVec2::new(
                                        hit[0] as f64 - r.center[0] as f64,
                                        hit[1] as f64 - r.center[1] as f64,
                                    );
                                observations[batch * 8 + j].push(delta);
                            }
                        }
                    }
                }
            }
            fn median(v: &[DVec2]) -> DVec2 {
                if v.is_empty() {
                    return DVec2::ZERO;
                }
                let axis = |f: fn(&DVec2) -> f64| {
                    let mut a: Vec<_> = v.iter().map(f).collect();
                    a.sort_by(f64::total_cmp);
                    (a[(a.len() - 1) / 2] + a[a.len() / 2]) * 0.5
                };
                DVec2::new(axis(|p| p.x), axis(|p| p.y))
            }
            let all: Vec<_> = observations.iter().flatten().copied().collect();
            let global = median(&all);
            for (p, cell) in path.iter_mut().zip(&observations) {
                let local = median(cell);
                *p += if cell.is_empty() || local.distance(global) > 16. {
                    global
                } else {
                    local
                };
            }
            emit(frame(time, &path))?;
            previous = image;
        }
        Ok(())
    }
}
