/// Blackman-windowed polyphase low-pass. Derive the cutoff from both the
/// sampling rates and the layer's stretch, including reverse playback.
pub(super) struct Sinc {
    pub(super) cutoff: f64,
    pub(super) radius: usize,
    taps: Vec<f32>,
}

impl Sinc {
    const PHASES: usize = 256;

    pub(super) fn new(cutoff: f64) -> Self {
        use std::f64::consts::PI;
        let radius = (16.0 / cutoff).ceil().min(512.0) as usize;
        let width = radius * 2;
        let mut taps = vec![0.0; Self::PHASES * width];
        for (phase, row) in taps.chunks_exact_mut(width).enumerate() {
            let fraction = phase as f64 / Self::PHASES as f64;
            for (i, tap) in row.iter_mut().enumerate() {
                let x = i as f64 + 1.0 - radius as f64 - fraction;
                let angle = PI * x / radius as f64;
                let window = 0.42 + 0.5 * angle.cos() + 0.08 * (2.0 * angle).cos();
                let y = PI * x * cutoff;
                *tap = (window * cutoff * if y == 0.0 { 1.0 } else { y.sin() / y }) as f32;
            }
            let sum: f32 = row.iter().sum();
            for tap in row {
                *tap /= sum;
            }
        }
        Self {
            cutoff,
            radius,
            taps,
        }
    }

    pub(super) fn sample(&self, data: &[f32], channels: usize, pos: f64) -> [f32; 2] {
        let phase = (pos.fract() * Self::PHASES as f64) as usize;
        let width = self.radius * 2;
        let mut stereo = [0.0; 2];
        let first = pos.floor() as isize + 1 - self.radius as isize;
        // The source is zero-padded at its boundaries.
        let lo = (-first).clamp(0, width as isize) as usize;
        let hi =
            (data.len() as isize / channels as isize - first).clamp(0, width as isize) as usize;
        let right = 1.min(channels - 1);
        for i in lo..hi {
            let base = (first + i as isize) as usize * channels;
            let tap = self.taps[phase * width + i];
            stereo[0] += tap * data[base];
            stereo[1] += tap * data[base + right];
        }
        stereo
    }
}

#[cfg(test)]
mod tests {
    use super::Sinc;

    #[test]
    fn downsampling_preserves_passband_and_rejects_aliases_in_both_directions() {
        let kernel = Sinc::new(1.0 / 3.0);
        let sample =
            |freq: f64, frame: f64| (std::f64::consts::TAU * freq * frame / 48_000.0).sin() as f32;
        let data: Vec<_> = (0..1024)
            .flat_map(|i| [sample(1000.0, f64::from(i)), sample(12_000.0, f64::from(i))])
            .collect();
        for (start, step) in [(64.375, 3.0), (960.375, -3.0)] {
            for i in 0..250 {
                let pos = start + f64::from(i) * step;
                let [tone, alias] = kernel.sample(&data, 2, pos);
                assert!((tone - sample(1000.0, pos)).abs() < 0.001);
                assert!(
                    alias.abs() < 0.001,
                    "out-of-band tone leaked at {pos}: {alias}"
                );
            }
        }
    }
}
