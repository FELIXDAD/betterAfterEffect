//! Layer pixels read back for `sampleImage` expressions without waiting for the
//! GPU ([`bae_anim::ImageSamples`] explains the round trip;
//! [`crate::Renderer::read_samples`] starts it).

use bae_anim::{ImageSamples, SampleKey};
use bae_gpu::transfer::{ImageData, PendingReadback};
use bae_gpu::{GpuContext, wgpu};
use bae_model::{PrimPath, Time};

use crate::error::RenderError;
use crate::geom::IRect;
use crate::input::{LayerContent, RenderComp};

/// One sampled layer image being read back.
#[derive(Debug)]
struct Job {
    generation: u64,
    readback: PendingReadback,
    /// Layer pixels the read-back texels cover (full resolution).
    rect: IRect,
    keys: Vec<SampleKey>,
}

/// Read-backs in flight.
#[derive(Debug, Default)]
pub(crate) struct SampleJobs {
    jobs: Vec<Job>,
}

impl SampleJobs {
    pub(crate) fn push(
        &mut self,
        generation: u64,
        readback: PendingReadback,
        rect: IRect,
        keys: Vec<SampleKey>,
    ) {
        self.jobs.push(Job {
            generation,
            readback,
            rect,
            keys,
        });
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// Stores the values of finished read-backs; true when any arrived.
    pub(crate) fn finish(&mut self, samples: &ImageSamples) -> bool {
        let before = self.jobs.len();
        self.jobs.retain(|job| match job.readback.try_finish() {
            None => true,
            Some(result) => {
                fill(
                    samples,
                    job.generation,
                    job.rect,
                    &job.keys,
                    result.ok().as_ref(),
                );
                false
            }
        });
        self.jobs.len() != before
    }

    /// Waits for every read-back and stores the values.
    pub(crate) fn wait(
        &mut self,
        gpu: &GpuContext,
        samples: &ImageSamples,
    ) -> Result<(), RenderError> {
        for job in std::mem::take(&mut self.jobs) {
            let Job {
                generation,
                readback,
                rect,
                keys,
            } = job;
            let data = readback.wait(gpu)?;
            fill(samples, generation, rect, &keys, Some(&data));
        }
        Ok(())
    }
}

/// Values of `keys` read from `data` covering `rect` (transparent black when the
/// read-back failed).
fn fill(
    samples: &ImageSamples,
    generation: u64,
    rect: IRect,
    keys: &[SampleKey],
    data: Option<&ImageData>,
) {
    for key in keys {
        let rgba = data.map_or([0.0; 4], |d| average(d, rect, key.area()));
        samples.fulfil(generation, key.clone(), rgba);
    }
}

/// Every pixel any of `keys` touches, in layer pixels (a zero-size area takes
/// the pixel under its centre).
pub(crate) fn region(keys: &[SampleKey]) -> IRect {
    let mut out: Option<IRect> = None;
    for key in keys {
        let [x0, y0, x1, y1] = pixels(key.area());
        let r = IRect { x0, y0, x1, y1 };
        out = Some(match out {
            None => r,
            Some(o) => IRect {
                x0: o.x0.min(r.x0),
                y0: o.y0.min(r.y0),
                x1: o.x1.max(r.x1),
                y1: o.y1.max(r.y1),
            },
        });
    }
    out.unwrap_or(IRect {
        x0: 0,
        y0: 0,
        x1: 1,
        y1: 1,
    })
}

/// The area actually averaged: `area`, or the pixel under its centre along an
/// axis where it has no extent.
fn effective(area: [f64; 4]) -> [f64; 4] {
    let [mut l, mut t, mut r, mut b] = area;
    if r <= l {
        l = ((l + r) / 2.0).floor();
        r = l + 1.0;
    }
    if b <= t {
        t = ((t + b) / 2.0).floor();
        b = t + 1.0;
    }
    [l, t, r, b]
}

fn pixels(area: [f64; 4]) -> [i32; 4] {
    let [l, t, r, b] = effective(area);
    let clamp = |v: f64| v.clamp(-1e9, 1e9) as i32;
    [
        clamp(l.floor()),
        clamp(t.floor()),
        clamp(r.ceil()),
        clamp(b.ceil()),
    ]
}

/// AE's sampleImage value: the alpha-weighted average colour of `area` (layer
/// pixels) and its average alpha, from premultiplied texels `data` covering
/// `rect`; pixels outside `rect` are transparent.
pub(crate) fn average(data: &ImageData, rect: IRect, area: [f64; 4]) -> [f32; 4] {
    let [l, t, r, b] = effective(area);
    let [x0, y0, x1, y1] = pixels(area);
    let total = (r - l) * (b - t);
    let mut sum = [0.0f64; 4];
    for y in y0.max(rect.y0)..y1.min(rect.y1) {
        let wy = (b.min(f64::from(y) + 1.0) - t.max(f64::from(y))).max(0.0);
        for x in x0.max(rect.x0)..x1.min(rect.x1) {
            let wx = (r.min(f64::from(x) + 1.0) - l.max(f64::from(x))).max(0.0);
            let (tx, ty) = ((x - rect.x0) as u32, (y - rect.y0) as u32);
            if tx >= data.width || ty >= data.height {
                continue;
            }
            let p = texel(data, tx, ty);
            for c in 0..4 {
                sum[c] += f64::from(p[c]) * wx * wy;
            }
        }
    }
    let alpha = sum[3] / total;
    let straight = |c: f64| if sum[3] > 0.0 { c / sum[3] } else { 0.0 };
    [
        straight(sum[0]) as f32,
        straight(sum[1]) as f32,
        straight(sum[2]) as f32,
        alpha as f32,
    ]
}

fn texel(data: &ImageData, x: u32, y: u32) -> [f32; 4] {
    let p = data.pixel(x, y);
    match data.format {
        wgpu::TextureFormat::Rgba16Float => std::array::from_fn(|c| {
            half::f16::from_bits(u16::from_le_bytes([p[2 * c], p[2 * c + 1]])).to_f32()
        }),
        wgpu::TextureFormat::Rgba32Float => std::array::from_fn(|c| {
            f32::from_le_bytes([p[4 * c], p[4 * c + 1], p[4 * c + 2], p[4 * c + 3]])
        }),
        wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => {
            std::array::from_fn(|c| f32::from(p[c]) / 255.0)
        }
        _ => [0.0; 4],
    }
}

/// The comp `path` at `time` holding `layer`, inside `comp` (itself or a
/// nested comp; a preview of fewer layers does not count).
pub(crate) fn find_comp<'a>(
    comp: &'a RenderComp,
    path: &PrimPath,
    time: Time,
    layer: &PrimPath,
) -> Option<&'a RenderComp> {
    if comp.path == *path && comp.time == time && comp.layers.iter().any(|l| l.path == *layer) {
        return Some(comp);
    }
    comp.layers.iter().find_map(|l| match &l.content {
        LayerContent::Comp(nested) => find_comp(nested, path, time, layer),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2×1 image: opaque red, then transparent.
    fn image() -> ImageData {
        let mut data = Vec::new();
        for px in [[1.0f32, 0.0, 0.0, 1.0], [0.0; 4]] {
            for c in px {
                data.extend_from_slice(&c.to_le_bytes());
            }
        }
        ImageData {
            width: 2,
            height: 1,
            format: wgpu::TextureFormat::Rgba32Float,
            bytes_per_row: 32,
            data,
        }
    }

    #[test]
    fn alpha_weighted_area_average() {
        let rect = IRect {
            x0: 10,
            y0: 0,
            x1: 12,
            y1: 1,
        };
        // Exactly the red pixel.
        assert_eq!(
            average(&image(), rect, [10.0, 0.0, 11.0, 1.0]),
            [1.0, 0.0, 0.0, 1.0]
        );
        // Half red, half transparent: the colour stays red, alpha halves.
        assert_eq!(
            average(&image(), rect, [10.5, 0.0, 11.5, 1.0]),
            [1.0, 0.0, 0.0, 0.5]
        );
        // Outside the read-back: transparent.
        assert_eq!(average(&image(), rect, [20.0, 0.0, 21.0, 1.0]), [0.0; 4]);
        // No extent: the pixel under the centre.
        assert_eq!(
            average(&image(), rect, [10.5, 0.5, 10.5, 0.5]),
            [1.0, 0.0, 0.0, 1.0]
        );
    }
}
