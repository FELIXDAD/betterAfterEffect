use super::Processor;

use super::svf::Filter;
#[derive(Default)]
struct Band {
    filter: Filter,
    last_boost: f64,
    gain: f32,
}
impl Band {
    fn equalize(
        &mut self,
        frame: [f32; 2],
        hz: f64,
        width: f64,
        boost: f64,
        rate: u32,
    ) -> [f32; 2] {
        if self.last_boost != boost {
            self.last_boost = boost;
            self.gain = (10.0_f64.powf(boost * 0.24 / 20.0) - 1.0) as f32;
        }
        let pass = self
            .filter
            .split(frame, hz, 1.0 / (width * 0.02).max(0.01), rate)[1];
        std::array::from_fn(|ch| frame[ch] + pass[ch] * self.gain)
    }
}
#[derive(Default)]
pub(super) struct Main([Band; 3]);
impl Processor for Main {
    fn apply(&mut self, mut frame: [f32; 2], p: &[f64], rate: u32) -> [f32; 2] {
        for (band, values) in self.0.iter_mut().zip(p[1..].as_chunks::<4>().0) {
            if values[0] >= 0.5 {
                frame = band.equalize(frame, values[1], values[2], values[3], rate);
            }
        }
        frame
    }
}
#[derive(Default)]
pub(super) struct Shelves(Main);
impl Processor for Shelves {
    fn apply(&mut self, frame: [f32; 2], p: &[f64], rate: u32) -> [f32; 2] {
        self.0.apply(
            frame,
            &[
                0.0, 1.0, 120.0, 99.0, p[0], 1.0, 7000.0, 99.0, p[1], 0.0, 0.0, 1.0, 0.0,
            ],
            rate,
        )
    }
}
#[test]
fn bass_and_treble_use_the_two_wide_equalizer_bands() {
    let mut alias = Shelves::default();
    let mut eq = Main::default();
    for n in 0..512 {
        let input = [(n as f32 * 0.02).sin(); 2];
        let actual = alias.apply(input, &[65.0, -15.0], 48000);
        assert_eq!(
            actual,
            eq.apply(
                input,
                &[
                    0.0, 1.0, 120.0, 99.0, 65.0, 1.0, 7000.0, 99.0, -15.0, 0.0, 0.0, 1.0, 0.0
                ],
                48000
            )
        );
    }
}
