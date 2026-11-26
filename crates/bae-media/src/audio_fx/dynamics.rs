use super::Processor;

fn smoothing(milliseconds: f64, rate: u32) -> f64 {
    if milliseconds <= 0. {
        0.
    } else {
        (-1000. / (milliseconds * f64::from(rate))).exp()
    }
}

#[derive(Default)]
pub(super) struct Compressor {
    reduction_db: f64,
    threshold: f64,
    knee: f64,
    slope: f64,
    attack: f64,
    release: f64,
    makeup_db: f64,
    ceiling: f64,
}
impl Processor for Compressor {
    fn update(&mut self, p: &[f64], rate: u32) {
        self.threshold = p[0];
        self.slope = 1. / p[1].max(0.4) - 1.;
        self.knee = p[2];
        self.attack = smoothing(p[3], rate);
        let release = if p[5] >= 0.5 {
            p[4] * (0.25 + 0.75 * (self.reduction_db.abs() / 18.).min(1.))
        } else {
            p[4]
        };
        self.release = smoothing(release, rate);
        self.makeup_db = p[6];
        self.ceiling = 10_f64.powf(p[7] / 20.);
    }
    fn apply(&mut self, frame: [f32; 2], _: &[f64], _: u32) -> [f32; 2] {
        let peak = f64::from(frame[0].abs().max(frame[1].abs())).max(1e-12);
        let over = 20. * peak.log10() - self.threshold;
        let bend = if over <= -self.knee * 0.5 {
            0.
        } else if over >= self.knee * 0.5 {
            over
        } else {
            (over + self.knee * 0.5).powi(2) / (2. * self.knee)
        };
        let target = self.slope * bend;
        let coeff = if target.abs() > self.reduction_db.abs() {
            self.attack
        } else {
            self.release
        };
        self.reduction_db = target + (self.reduction_db - target) * coeff;
        let gain = 10_f64
            .powf((self.reduction_db + self.makeup_db) / 20.)
            .min(self.ceiling / peak);
        frame.map(|sample| sample * gain as f32)
    }
}

#[derive(Default)]
pub(super) struct Gate {
    gain: f64,
    hold: u32,
    threshold: f32,
    attack: f64,
    release: f64,
    hold_samples: u32,
}
impl Processor for Gate {
    fn update(&mut self, p: &[f64], rate: u32) {
        self.threshold = 10_f32.powf(p[0] as f32 / 20.);
        self.attack = smoothing(p[1], rate);
        self.hold_samples = (p[2] * f64::from(rate) * 0.001).round() as u32;
        self.release = smoothing(p[3], rate);
    }
    fn apply(&mut self, frame: [f32; 2], _: &[f64], _: u32) -> [f32; 2] {
        let above = frame[0].abs().max(frame[1].abs()) >= self.threshold;
        let open = above || self.hold > 0;
        self.hold = if above {
            self.hold_samples
        } else {
            self.hold.saturating_sub(1)
        };
        self.gain = if open {
            1. + (self.gain - 1.) * self.attack
        } else {
            self.gain * self.release
        };
        frame.map(|v| v * self.gain as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linked_compression_and_gate_hold() {
        let mut compressor = Compressor::default();
        compressor.update(&[-20., 2., 0., 0., 100., 0., 0., 0.], 1000);
        let out = compressor.apply([1., 0.5], &[-20., 2., 0., 0., 100., 0., 0., 0.], 1000);
        assert!((out[0] - 0.316228).abs() < 0.00001);
        assert_eq!(out[1], out[0] * 0.5);
        let mut gate = Gate::default();
        let p = [-20., 0., 2., 5.];
        gate.update(&p, 1000);
        assert_eq!(gate.apply([1.; 2], &p, 1000), [1.; 2]);
        for _ in 0..2 {
            assert_eq!(gate.apply([0.05; 2], &p, 1000), [0.05; 2]);
        }
        assert!(gate.apply([0.05; 2], &p, 1000)[0] < 0.05);
    }
}
