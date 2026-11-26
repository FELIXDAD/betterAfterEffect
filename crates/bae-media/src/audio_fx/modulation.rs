use super::Processor;
use std::f64::consts::TAU;

#[derive(Default)]
struct DelayLine {
    samples: Vec<[f32; 2]>,
    next: usize,
}
impl DelayLine {
    fn prepare(&mut self, rate: u32, seconds: f64) {
        self.samples
            .resize((f64::from(rate) * seconds).ceil() as usize + 2, [0.; 2]);
        self.samples.fill([0.; 2]);
        self.next = 0;
    }
    fn clear(&mut self) {
        self.samples.fill([0.; 2]);
        self.next = 0;
    }
    fn read(&self, ch: usize, delay: f64) -> f32 {
        let length = self.samples.len();
        let delay = delay.clamp(0., (length - 2) as f64);
        let near = (self.next + length - delay.floor() as usize) % length;
        let far = (near + length - 1) % length;
        let a = self.samples[near][ch];
        a + (self.samples[far][ch] - a) * delay.fract() as f32
    }
    fn advance(&mut self) {
        self.next = (self.next + 1) % self.samples.len();
    }
}

#[derive(Default)]
pub(super) struct Modulator {
    line: DelayLine,
    phase: f64,
}
impl Processor for Modulator {
    fn prepare(&mut self, rate: u32, _: &[f64], _: u32) {
        self.line.prepare(rate, 0.03);
    }
    fn clear_history(&mut self) -> bool {
        self.line.clear();
        true
    }
    fn seek(&mut self, t: f64, _: u32, _: f64, p: &[f64]) {
        self.phase = (t * p[1]).rem_euclid(1.);
    }
    fn apply(&mut self, frame: [f32; 2], p: &[f64], rate: u32) -> [f32; 2] {
        self.line.samples[self.line.next] = frame;
        let osc = if p[0] as u32 == 2 {
            1. - 4. * (self.phase - 0.5).abs()
        } else {
            (TAU * self.phase).sin()
        };
        let delay = (1. + osc) * p[2] * 0.00015 * f64::from(rate);
        let amplitude = 1. - p[3] * 0.005 * (1. - osc);
        let out = std::array::from_fn(|ch| self.line.read(ch, delay) * amplitude as f32);
        self.phase = (self.phase + p[1] / f64::from(rate)).fract();
        self.line.advance();
        out
    }
}

#[derive(Default)]
pub(super) struct Chorus {
    line: DelayLine,
    phase: f64,
}
impl Processor for Chorus {
    fn prepare(&mut self, rate: u32, p: &[f64], animated: u32) {
        let separation = if animated & 1 != 0 { 500. } else { p[0] }.clamp(0., 500.);
        let voices = if animated & 2 != 0 { 32. } else { p[1] }
            .round()
            .clamp(0., 32.);
        self.line.prepare(rate, separation * (voices + 0.5) * 0.001);
    }
    fn clear_history(&mut self) -> bool {
        self.line.clear();
        true
    }
    fn seek(&mut self, t: f64, _: u32, _: f64, p: &[f64]) {
        self.phase = (t * p[2]).rem_euclid(1.);
    }
    fn apply(&mut self, frame: [f32; 2], p: &[f64], rate: u32) -> [f32; 2] {
        self.line.samples[self.line.next] = frame;
        let voices = p[1].round().clamp(0., 32.) as usize;
        let separation = p[0] * 0.001 * f64::from(rate);
        let stereo = p[6] >= 0.5;
        let wet = p[8] * 0.01 * if p[5] >= 0.5 { -1. } else { 1. };
        let output = std::array::from_fn(|ch| {
            let mut sum = 0.;
            for i in 0..voices {
                if stereo && i % 2 != ch {
                    continue;
                }
                let phase = self.phase + i as f64 * p[4] / 360.;
                let delay = separation * ((i + 1) as f64 + 0.5 * p[3] * 0.01 * (TAU * phase).sin());
                sum += self.line.read(ch, delay);
            }
            frame[ch] * (p[7] * 0.01) as f32 + sum * (wet / voices.max(1) as f64) as f32
        });
        self.phase = (self.phase + p[2] / f64::from(rate)).fract();
        self.line.advance();
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zero_depth_preserves_audio_and_full_tremolo_tracks_the_lfo() {
        let mut fx = Modulator::default();
        fx.prepare(1000, &[], 0);
        let neutral = [1., 250., 0., 0.];
        assert_eq!(fx.apply([0.25, -0.75], &neutral, 1000), [0.25, -0.75]);
        fx.seek(0., 1000, 1., &neutral);
        let tremolo = [1., 250., 0., 100.];
        for expected in [0.5, 1., 0.5, 0.] {
            let out = fx.apply([1.; 2], &tremolo, 1000);
            assert!((out[0] - expected).abs() < 0.00001);
        }
    }
    #[test]
    fn chorus_returns_delayed_impulse_in_phase_or_inverted() {
        for sign in [1., -1.] {
            let mut effect = Chorus::default();
            let p = [
                4.,
                1.,
                0.,
                0.,
                0.,
                if sign < 0. { 1. } else { 0. },
                0.,
                0.,
                100.,
            ];
            effect.prepare(1000, &p, 0);
            for i in 0..8 {
                let out = effect.apply(if i == 0 { [1., 0.] } else { [0.; 2] }, &p, 1000);
                assert_eq!(out, [if i == 4 { sign } else { 0. }, 0.]);
            }
        }
    }
}
