use super::Processor;

#[derive(Default)]
pub(super) struct Main {
    ring: Vec<[f32; 2]>,
    index: usize,
}
impl Processor for Main {
    fn apply(&mut self, frame: [f32; 2], p: &[f64], rate: u32) -> [f32; 2] {
        if self.ring.is_empty() {
            self.ring.resize(rate as usize * 5 + 2, [0.0; 2]);
        }
        let delay = (p[0].clamp(0.0, 5000.0) * f64::from(rate) * 0.001).round() as usize;
        let echo = if delay == 0 {
            frame
        } else {
            self.ring[(self.index + self.ring.len() - delay) % self.ring.len()]
        };
        let feedback = (p[2] * 0.01) as f32;
        self.ring[self.index] =
            std::array::from_fn(|ch| (frame[ch] + echo[ch] * feedback).clamp(-64.0, 64.0));
        self.index = (self.index + 1) % self.ring.len();
        std::array::from_fn(|ch| {
            frame[ch] * (p[3] * 0.01) as f32 + echo[ch] * (p[1] * p[4] * 0.0001) as f32
        })
    }
}
#[test]
fn impulse_and_feedback_arrive_at_the_requested_samples() {
    let mut delay = Main::default();
    let p = [10.0, 100.0, 50.0, 0.0, 100.0];
    for n in 0..22 {
        let out = delay.apply(if n == 0 { [1.0, 0.0] } else { [0.0; 2] }, &p, 1000);
        assert_eq!(
            out,
            [
                match n {
                    10 => 1.0,
                    20 => 0.5,
                    _ => 0.0,
                },
                0.0
            ]
        );
    }
}
