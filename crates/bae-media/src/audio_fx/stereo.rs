use super::Processor;

#[derive(Default)]
pub(super) struct Main;
impl Processor for Main {
    fn apply(&mut self, frame: [f32; 2], p: &[f64], _: u32) -> [f32; 2] {
        let phase = if p[4] >= 0.5 { -1.0 } else { 1.0 };
        let left = frame[0] * p[0] as f32 * 0.01 * phase;
        let right = frame[1] * p[1] as f32 * 0.01 * phase;
        let pan = |v: f64| ((v + 100.0) * 0.005) as f32;
        [
            left * (1.0 - pan(p[2])) + right * (1.0 - pan(p[3])),
            left * pan(p[2]) + right * pan(p[3]),
        ]
    }
}
#[test]
fn channel_pan_can_exchange_and_invert_the_inputs() {
    assert_eq!(
        Main.apply([0.25, -0.5], &[100.0, 200.0, 100.0, -100.0, 1.0], 48000),
        [1.0, -0.25]
    );
}
