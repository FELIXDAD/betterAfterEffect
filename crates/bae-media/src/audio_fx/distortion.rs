use super::Processor;

#[derive(Default)]
pub(super) struct Distortion {
    remaining: u32,
    held: [f32; 2],
}
impl Processor for Distortion {
    fn apply(&mut self, frame: [f32; 2], p: &[f64], _: u32) -> [f32; 2] {
        if self.remaining > 0 {
            self.remaining -= 1;
            return self.held;
        }
        self.remaining = p[6].round().clamp(1., 48.) as u32 - 1;
        let drive = 1. + p[1] * 0.15;
        let gain = p[2] * 0.01;
        let wet = p[3] * 0.01;
        let volume = p[4] * 0.01;
        let bits = p[5].round().clamp(1., 24.) as i32;
        let scale = 2_f64.powi(bits - 1);
        self.held = frame.map(|sample| {
            let x = f64::from(sample) * gain * drive;
            let clipped = match p[7] as u32 {
                1 => x / (1. + x.abs()),
                2 => x.clamp(-1., 1.),
                3 => x.tanh(),
                4 => 2. * x.atan() / std::f64::consts::PI,
                5 => (x + 0.35).tanh() - 0.35_f64.tanh(),
                _ => x.signum() * (1. - (-3. * x.abs()).exp()),
            };
            let out = (f64::from(sample) * (1. - wet) + clipped * wet) * volume;
            if bits == 24 {
                out as f32
            } else if bits == 1 {
                if out >= 0. { 1. } else { -1. }
            } else {
                ((out * scale).round().clamp(-scale, scale - 1.) / scale) as f32
            }
        });
        self.held
    }
}

#[test]
fn bitcrusher_holds_the_quantized_stereo_sample() {
    let mut fx = Distortion::default();
    let parameters = [0., 0., 100., 0., 100., 3., 3., 1.];
    assert_eq!(fx.apply([0.33, -0.6], &parameters, 48000), [0.25, -0.5]);
    for _ in 0..2 {
        assert_eq!(fx.apply([0.9; 2], &parameters, 48000), [0.25, -0.5]);
    }
    assert_eq!(fx.apply([0.9; 2], &parameters, 48000), [0.75; 2]);
}
