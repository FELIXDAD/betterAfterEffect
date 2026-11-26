//! Lumetri scope configurations and color transforms of the published display frame.
use super::{Parade, Scope, State, Wave};
use bae_color::{Chromaticities, ColorSpace, Encoding, TransferFunction, builtin};
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Space {
    #[default]
    Automatic,
    Rec601,
    Rec709,
    Rec2020,
    Hlg,
    Pq,
}

impl Space {
    pub(super) const ALL: [(Self, &str); 4] = [
        (Self::Automatic, "Automatic"),
        (Self::Rec601, "Rec. 601"),
        (Self::Rec709, "Rec. 709"),
        (Self::Rec2020, "Rec. 2020"),
    ];

    pub(super) fn resolve(self, stage: &bae_model::Stage) -> Self {
        if self != Self::Automatic {
            return self;
        }
        let path = bae_model::PrimPath::new(bae_motion::project_settings::PROJECT).unwrap();
        let id = stage
            .prim(&path)
            .and_then(|p| bae_motion::query::static_value(&p, "working_space"));
        let name = id
            .as_ref()
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if name.contains("hlg") {
            Self::Hlg
        } else if name.contains("pq") {
            Self::Pq
        } else if name.contains("2020") {
            Self::Rec2020
        } else if name.contains("601") {
            Self::Rec601
        } else {
            Self::Rec709
        }
    }

    pub(super) fn weights(self) -> [f32; 2] {
        match self {
            Self::Rec601 => [0.299, 0.114],
            Self::Rec2020 | Self::Hlg | Self::Pq => [0.2627, 0.0593],
            _ => [0.2126, 0.0722],
        }
    }
}

/// All transforms compile in the existing GPU warm-up task. The coefficients are constants
/// because these standardized spaces cannot change during a session.
pub(super) fn shader_source() -> String {
    let ntsc = ColorSpace::rgb(
        "scope-601",
        "Rec. 601",
        Encoding::Sdr,
        Chromaticities::BT601_525,
        TransferFunction::Rec709,
    );
    let mut source = String::new();
    let mut selector =
        String::from("fn scope_color(c: vec3f, space: u32) -> vec3f { switch space {");
    for (id, space) in [
        (Space::Rec601, ntsc),
        (Space::Rec2020, builtin::REC2020),
        (Space::Hlg, builtin::REC2100_HLG),
        (Space::Pq, builtin::REC2100_PQ),
    ] {
        let code = bae_color::convert(&builtin::REC709, &space)
            .expect("built-in color transform")
            .to_wgsl(&format!("scope{}", id as u32));
        source.push_str(&code.wgsl);
        let values = code
            .params
            .iter()
            .map(|v| format!("vec4f({:?},{:?},{:?},{:?})", v[0], v[1], v[2], v[3]))
            .collect::<Vec<_>>()
            .join(",");
        let args = format!(
            "{}(array<vec4f,{}>({values}))",
            code.params_type(),
            code.params.len()
        );
        writeln!(
            selector,
            "case {}u: {{ return {}; }}",
            id as u32,
            code.call("c", &args, &[], false)
        )
        .unwrap();
    }
    selector.push_str("default: { return c; } } }");
    source.push_str(&selector);
    source.push_str(include_str!("scopes.wgsl"));
    source
}

pub(super) struct Preset {
    pub title: &'static str,
    shown: &'static [Scope],
    parade: Parade,
    wave: Wave,
}
impl Preset {
    pub(super) fn matches(&self, state: &State) -> bool {
        state.shown == self.shown
            && (!self.shown.contains(&Scope::Parade) || state.parade == self.parade)
            && (!self.shown.contains(&Scope::Waveform) || state.wave == self.wave)
    }
    pub(super) fn apply(&self, state: &mut State) {
        state.shown = self.shown.to_vec();
        state.parade = self.parade;
        state.wave = self.wave;
        if self.title == "SpeedGrade 4 Scope Luma/RGB (601 SD)" {
            state.space = Space::Rec601;
        }
    }
}
const fn preset(
    title: &'static str,
    shown: &'static [Scope],
    parade: Parade,
    wave: Wave,
) -> Preset {
    Preset {
        title,
        shown,
        parade,
        wave,
    }
}
use Scope::{Histogram, Parade as Channels, Vectorscope, Waveform};
pub(super) const PRESETS: &[Preset] = &[
    preset("Vectorscope YUV", &[Vectorscope], Parade::Rgb, Wave::Luma),
    preset("Parade RGB", &[Channels], Parade::Rgb, Wave::Luma),
    preset("Waveform RGB", &[Waveform], Parade::Rgb, Wave::Rgb),
    preset("Histogram", &[Histogram], Parade::Rgb, Wave::Luma),
    preset(
        "Parade/Waveform RGB",
        &[Channels, Waveform],
        Parade::Rgb,
        Wave::Rgb,
    ),
    preset(
        "Parade RGB/Waveform YC",
        &[Channels, Waveform],
        Parade::Rgb,
        Wave::Yc,
    ),
    preset(
        "Vectorscope YUV/Parade RGB/Waveform YC",
        &[Vectorscope, Channels, Waveform],
        Parade::Rgb,
        Wave::Yc,
    ),
    preset(
        "Vectorscope YUV/Parade YUV/Waveform YC",
        &[Vectorscope, Channels, Waveform],
        Parade::Yuv,
        Wave::Yc,
    ),
    preset(
        "Premiere 4 Scope YUV (Float, Unclamped)",
        &[Vectorscope, Channels, Histogram, Waveform],
        Parade::Yuv,
        Wave::Yc,
    ),
    preset(
        "SpeedGrade 4 Scope Luma/RGB (601 SD)",
        &[Waveform, Vectorscope, Histogram, Channels],
        Parade::Rgb,
        Wave::Luma,
    ),
    preset("All Scopes RGB", &Scope::ALL, Parade::Rgb, Wave::Rgb),
    preset("All Scopes YUV/YC", &Scope::ALL, Parade::Yuv, Wave::Yc),
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presets_set_the_scopes_and_color_spaces_move_chroma_targets() {
        bae_ui::i18n::register(super::super::ZH);
        let mut state = State::default();
        for preset in PRESETS {
            preset.apply(&mut state);
            assert!(preset.matches(&state));
            assert_eq!(
                PRESETS.iter().filter(|other| other.matches(&state)).count(),
                1
            );
        }
        assert_eq!(
            bae_ui::i18n::translation(PRESETS.last().unwrap().title),
            Some("所有示波器 YUV/YC")
        );
        assert_eq!(
            Space::Automatic.resolve(&bae_model::Stage::new()),
            Space::Rec709
        );
        let red = [1.0, 0.0, 0.0];
        let c601 = super::super::cb_cr(red, Space::Rec601);
        let c709 = super::super::cb_cr(red, Space::Rec709);
        assert!((c601[0] + 0.1687).abs() < 0.001 && (c709[0] + 0.1146).abs() < 0.001);
        assert_eq!(c601[1], 0.5);
    }
}
