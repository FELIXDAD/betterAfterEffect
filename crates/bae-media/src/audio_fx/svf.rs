//! Topology-preserving state-variable filter. Low, band and high outputs share the same integrators.
#[derive(Default)]
pub(super) struct Filter {
    last: (f64, f64, u32),
    a1: f64,
    a2: f64,
    a3: f64,
    damping: f64,
    z1: [f64; 2],
    z2: [f64; 2],
}
pub(super) fn warped_hz(hz: f64, rate: u32) -> f64 {
    (std::f64::consts::PI * (hz / f64::from(rate)).clamp(1e-8, 0.499)).tan()
}
impl Filter {
    pub fn split(&mut self, input: [f32; 2], hz: f64, q: f64, rate: u32) -> [[f32; 2]; 3] {
        if self.last != (hz, q, rate) {
            self.last = (hz, q, rate);
            let g = warped_hz(hz, rate);
            self.damping = 1.0 / q;
            self.a1 = 1.0 / (1.0 + g * (g + self.damping));
            self.a2 = g * self.a1;
            self.a3 = g * self.a2;
        }
        let mut output = [[0.0; 2]; 3];
        for (ch, sample) in input.into_iter().enumerate() {
            let x = f64::from(sample);
            let v = x - self.z2[ch];
            let band = self.a1 * self.z1[ch] + self.a2 * v;
            let low = self.z2[ch] + self.a2 * self.z1[ch] + self.a3 * v;
            self.z1[ch] = 2.0 * band - self.z1[ch];
            self.z2[ch] = 2.0 * low - self.z2[ch];
            output[0][ch] = low as f32;
            output[1][ch] = (band * self.damping) as f32;
            output[2][ch] = (x - low - band * self.damping) as f32;
        }
        output
    }
}
