use super::Processor;

/// Four decorrelated delay lines with an orthogonal feedback mix and high-frequency damping.
#[derive(Default)]
pub(super) struct Main {
    lines: [Vec<f32>; 4],
    cursor: [usize; 4],
    damped: [f32; 4],
}
const LENGTHS: [f64; 4] = [0.149, 0.211, 0.263, 0.293];

impl Processor for Main {
    /// The four lines for five seconds of reverb time, allocated once per rate.
    fn prepare(&mut self, rate: u32, _: &[f64], _: u32) {
        for (line, length) in self.lines.iter_mut().zip(LENGTHS) {
            *line = vec![0.0; (f64::from(rate) * 5.0 * length).ceil() as usize + 2];
        }
        self.cursor = [0; 4];
        self.damped = [0.0; 4];
    }
    fn clear_history(&mut self) -> bool {
        for line in &mut self.lines {
            line.fill(0.0);
        }
        self.cursor = [0; 4];
        self.damped = [0.0; 4];
        true
    }
    fn apply(&mut self, frame: [f32; 2], p: &[f64], rate: u32) -> [f32; 2] {
        let diffusion = (p[1] * 0.01) as f32;
        let brightness = 0.02 + (p[3] * 0.0098) as f32;
        let feedback = (0.15 + p[2] * 0.0084) as f32;
        let lengths = LENGTHS;
        let mut taps = [0.0; 4];
        for (i, line) in self.lines.iter_mut().enumerate() {
            let lag = (p[0] * 0.001 * f64::from(rate) * lengths[i])
                .round()
                .clamp(1.0, (line.len() - 1) as f64) as usize;
            let tap = line[(self.cursor[i] + line.len() - lag) % line.len()];
            self.damped[i] += brightness * (tap - self.damped[i]);
            taps[i] = self.damped[i];
        }
        let sum = taps.iter().sum::<f32>() * 0.5;
        for (i, line) in self.lines.iter_mut().enumerate() {
            let mixed = taps[i] * (1.0 - diffusion) + (sum - taps[i]) * diffusion;
            line[self.cursor[i]] = frame[i % 2] + feedback * mixed;
            self.cursor[i] = (self.cursor[i] + 1) % line.len();
        }
        std::array::from_fn(|c| {
            frame[c] * (p[4] * 0.01) as f32 + (taps[c] + taps[c + 2]) * (p[5] * 0.005) as f32
        })
    }
}
#[test]
fn stereo_impulse_has_a_decaying_diffuse_tail() {
    let mut fx = Main::default();
    let p = [100.0, 80.0, 50.0, 70.0, 0.0, 100.0];
    fx.prepare(8000, &p, 0);
    let energy: Vec<f32> = (0..4000)
        .map(|i| {
            fx.apply(if i == 0 { [1.0, 0.0] } else { [0.0; 2] }, &p, 8000)
                .into_iter()
                .map(|v| v * v)
                .sum()
        })
        .collect();
    assert!(energy[..110].iter().all(|s| *s == 0.0));
    assert!(energy[110..1500].iter().sum::<f32>() > 0.1);
    assert!(energy[3500..].iter().sum::<f32>() < 0.001);
}
