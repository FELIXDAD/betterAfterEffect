use super::Processor;

#[derive(Default)]
pub(super) struct Main;
impl Processor for Main {
    fn apply(&mut self, frame: [f32; 2], p: &[f64], _: u32) -> [f32; 2] {
        if p[0] >= 0.5 {
            [frame[1], frame[0]]
        } else {
            frame
        }
    }
}

#[test]
fn backwards_reverses_the_source_clock_and_can_swap_stereo() {
    let cfg = super::DEFINITIONS
        .iter()
        .find(|d| d.name == "ADBE Aud Reverse")
        .unwrap();
    assert!(cfg.reverse);
    assert_eq!(Main.apply([0.25, -0.5], &[1.0], 48000), [-0.5, 0.25]);
}
