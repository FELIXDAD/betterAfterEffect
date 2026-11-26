use super::Processor;

use super::svf::{Filter, warped_hz};

fn band_gain(boost: f64) -> f64 {
    10.0_f64.powf(boost * 0.24 / 20.0)
}

fn damping(width: f64) -> f64 {
    (width * 0.02).max(0.01)
}

/// Magnitude in dB of one parametric EQ band at the requested frequency.
pub fn eq_band_response_db(hz: f64, center: f64, width: f64, boost: f64, rate: u32) -> f64 {
    let omega = warped_hz(hz, rate) / warped_hz(center, rate);
    let real_sq = (1.0 - omega * omega).powi(2);
    let imag_sq = (omega * damping(width)).powi(2);
    10.0 * ((real_sq + band_gain(boost).powi(2) * imag_sq) / (real_sq + imag_sq)).log10()
}
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
            self.gain = (band_gain(boost) - 1.0) as f32;
        }
        let pass = self.filter.split(frame, hz, 1.0 / damping(width), rate)[1];
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
/// Bass & Treble: a low shelf at 120 Hz and a high shelf at 7 kHz (AE's
/// controls are shelving, not peaking), ±100 mapping to ±24 dB like the EQ bands.
#[derive(Default)]
pub(super) struct Shelves {
    low: Filter,
    high: Filter,
    boosts: [f64; 2],
    gains: [f32; 2],
}
impl Processor for Shelves {
    fn apply(&mut self, frame: [f32; 2], p: &[f64], rate: u32) -> [f32; 2] {
        for i in 0..2 {
            if self.boosts[i] != p[i] {
                self.boosts[i] = p[i];
                self.gains[i] = (band_gain(p[i]) - 1.0) as f32;
            }
        }
        let q = std::f64::consts::FRAC_1_SQRT_2;
        let low = self.low.split(frame, 120.0, q, rate)[0];
        let high = self.high.split(frame, 7000.0, q, rate)[2];
        std::array::from_fn(|ch| frame[ch] + low[ch] * self.gains[0] + high[ch] * self.gains[1])
    }
}
#[test]
fn displayed_band_response_matches_filtered_audio() {
    for (hz, boost) in [(1000.0, 50.0), (3000.0, -50.0)] {
        let mut band = Band::default();
        let mut energy = 0.0;
        for i in 0..9600 {
            let input = (std::f64::consts::TAU * hz * f64::from(i) / 48_000.0).sin() as f32;
            let output = band.equalize([input; 2], 1000.0, 20.0, boost, 48_000)[0];
            if i >= 4800 {
                energy += f64::from(output).powi(2);
            }
        }
        let measured_db = 10.0 * (energy / 2400.0).log10();
        assert!((measured_db - eq_band_response_db(hz, 1000.0, 20.0, boost, 48_000)).abs() < 0.001);
    }
}

#[test]
fn bass_and_treble_shelve_the_low_and_high_ends() {
    let gain = |boosts: [f64; 2], hz: f64| {
        let mut shelves = Shelves::default();
        let mut peak = 0.0_f32;
        for i in 0..48_000 {
            let input = (std::f64::consts::TAU * hz * f64::from(i) / 48_000.0).sin() as f32;
            let out = shelves.apply([input; 2], &boosts, 48_000)[0];
            if i >= 24_000 {
                peak = peak.max(out.abs());
            }
        }
        f64::from(peak)
    };
    let boost = band_gain(50.0);
    assert!(
        (gain([50.0, 0.0], 30.0) / boost - 1.0).abs() < 0.05,
        "bass shelf"
    );
    assert!(
        (gain([50.0, 0.0], 3000.0) - 1.0).abs() < 0.02,
        "mids untouched"
    );
    assert!(
        (gain([0.0, 50.0], 18_000.0) / boost - 1.0).abs() < 0.08,
        "treble shelf"
    );
}
