use super::Processor;
use std::f64::consts::TAU;

#[derive(Default)]
pub(super) struct Tone {
    phase: [f64; 5],
    sample: f64,
    speed: f64,
}
impl Processor for Tone {
    fn seek(&mut self, seconds: f64, rate: u32, speed: f64, p: &[f64]) {
        self.phase = std::array::from_fn(|i| (p[i + 1] * seconds).rem_euclid(1.));
        self.sample = seconds * f64::from(rate);
        self.speed = speed;
    }
    fn apply(&mut self, _frame: [f32; 2], p: &[f64], rate: u32) -> [f32; 2] {
        let mut signal = 0.;
        if p[0] as u32 == 5 {
            let mut n = (self.sample.round() as i64 as u64)
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            n = (n ^ (n >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            signal = ((n ^ (n >> 27)) >> 40) as f64 / 8388608. - 1.;
        } else {
            for (i, phase) in self.phase.iter_mut().enumerate() {
                let frequency = p[i + 1];
                if frequency <= 0. || (frequency * self.speed).abs() >= f64::from(rate) * 0.5 {
                    continue;
                }
                signal += match p[0] as u32 {
                    2 => 1. - 4. * (*phase - 0.5).abs(),
                    3 => 2. * *phase - 1.,
                    4 => {
                        if *phase < 0.5 {
                            1.
                        } else {
                            -1.
                        }
                    }
                    _ => (TAU * *phase).sin(),
                };
                *phase = (*phase + frequency * self.speed / f64::from(rate)).rem_euclid(1.);
            }
        }
        self.sample += self.speed;
        [(signal * p[6] * 0.01) as f32; 2]
    }
}
