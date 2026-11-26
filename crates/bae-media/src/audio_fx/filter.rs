use super::Processor;

use super::svf::Filter;
#[derive(Default)]
pub(super) struct Main(Filter);
impl Processor for Main {
    fn apply(&mut self, frame: [f32; 2], p: &[f64], rate: u32) -> [f32; 2] {
        let outputs = self
            .0
            .split(frame, p[1], std::f64::consts::FRAC_1_SQRT_2, rate);
        let wet = if p[0] < 1.5 { outputs[2] } else { outputs[0] };
        std::array::from_fn(|ch| frame[ch] * (p[2] * 0.01) as f32 + wet[ch] * (p[3] * 0.01) as f32)
    }
}
#[test]
fn high_and_low_pass_separate_dc() {
    for (mode, expected) in [(1.0, 0.0), (2.0, 1.0)] {
        let mut filter = Main::default();
        let mut last = [0.0; 2];
        for _ in 0..1000 {
            last = filter.apply([1.0; 2], &[mode, 1000.0, 0.0, 100.0], 48000);
        }
        assert!((last[0] - expected).abs() < 1e-5);
    }
}
