//! Viewer display colour management (AE's View ▸ Use Display Color
//! Management, Shift+Numpad /, on by default, and View ▸ Simulate Output):
//! the transform every preview applies from buffer values to the display,
//! from the project's working space (Project Settings) and the viewer's
//! choices. bae-render applies it in the viewers' output pass only; exports
//! never. Unmanaged projects (no working space) show their numbers as they
//! are, as before.
//!
//! The display is sRGB: the window is tagged sRGB, and the system converts
//! it to the monitor's own profile and gamut.

use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use bae_color::{
    Chromaticities, ColorContext, ColorSpace, ColorTransform, DisplayTarget, TransferFunction,
    WorkingSpace, builtin, primaries,
};
use bae_model::{PrimPath, Stage, Value};

/// View ▸ Simulate Output: how the composition would look in an output
/// format (AE's presets; the Kodak film prints need Adobe's film profiles
/// and stay unavailable).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SimulateOutput {
    #[default]
    None,
    Rec709,
    Ntsc,
    Pal,
    LegacyMac,
    Srgb,
}

impl SimulateOutput {
    /// The catalog's `{"profile": …}` value.
    pub(crate) fn from_param(profile: &str) -> Option<Self> {
        Some(match profile {
            "none" => Self::None,
            "rec709" => Self::Rec709,
            "ntsc" => Self::Ntsc,
            "pal" => Self::Pal,
            "legacy_mac" => Self::LegacyMac,
            "srgb" => Self::Srgb,
            _ => return None,
        })
    }

    /// The output space the composition is converted to and, when its
    /// numbers are kept (Preserve RGB), the space they are then shown as.
    fn output(self) -> Option<(ColorSpace, Option<ColorSpace>)> {
        let sdtv = |id: &str, name: &str, red, green, blue| {
            ColorSpace::custom_rgb(
                id,
                name,
                Chromaticities {
                    red,
                    green,
                    blue,
                    white: primaries::white::D65,
                },
                TransferFunction::Rec709,
            )
        };
        Some(match self {
            Self::None => return None,
            Self::Rec709 => (builtin::REC709, None),
            // SMPTE-C and EBU primaries with the Rec. 601 curve.
            Self::Ntsc => (
                sdtv(
                    "sdtv-ntsc",
                    "SDTV NTSC",
                    [0.630, 0.340],
                    [0.310, 0.595],
                    [0.155, 0.070],
                ),
                None,
            ),
            Self::Pal => (
                sdtv(
                    "sdtv-pal",
                    "SDTV PAL",
                    [0.640, 0.330],
                    [0.290, 0.600],
                    [0.150, 0.060],
                ),
                None,
            ),
            // sRGB numbers seen on a gamma 1.8 Macintosh.
            Self::LegacyMac => (builtin::SRGB, Some(builtin::GAMMA18)),
            Self::Srgb => (builtin::SRGB, None),
        })
    }
}

/// AE's working space names (Project Settings) → colour spaces.
pub(crate) fn working_space(name: &str) -> Option<ColorSpace> {
    Some(match name {
        "sRGB IEC61966-2.1" => builtin::SRGB,
        "HDTV (Rec. 709)" => builtin::REC709,
        "Adobe RGB (1998)" => builtin::ADOBE_RGB,
        "image P3" => builtin::DISPLAY_P3,
        "ProPhoto RGB" => ColorSpace::custom_rgb(
            "prophoto",
            "ProPhoto RGB",
            Chromaticities {
                red: [0.7347, 0.2653],
                green: [0.1596, 0.8404],
                blue: [0.0366, 0.0001],
                white: primaries::white::D50,
            },
            TransferFunction::Gamma(1.8),
        ),
        n if n.starts_with("ACEScg") => builtin::ACESCG,
        other => return builtin::by_id(other),
    })
}

/// Buffer values → display for a project with `working` settings.
fn transform(
    name: &str,
    linearize: bool,
    gamma_1: bool,
    managed: bool,
    simulate: SimulateOutput,
) -> Option<ColorTransform> {
    let working = WorkingSpace::from_project_attrs(name, linearize, gamma_1, working_space).ok()?;
    if !working.is_managed() || !managed {
        return None;
    }
    let display = DisplayTarget::default();
    let t = match simulate.output() {
        None => ColorContext::new(working)
            .with_display(display)
            .display_transform()
            .ok()?,
        Some((output, shown_as)) => {
            let buffer = working.buffer_space()?;
            let shown = shown_as.unwrap_or_else(|| output.clone());
            buffer
                .transform_to(&output)
                .ok()?
                .then(&shown.transform_to(&display.monitor).ok()?)
        }
    };
    (!t.is_identity()).then_some(t)
}

pub(crate) type Current = (u64, Option<Arc<ColorTransform>>);

fn cell() -> &'static Mutex<Current> {
    static CURRENT: Mutex<Current> = Mutex::new((0, None));
    &CURRENT
}

/// Recomputes the transform when the project's working space or the
/// viewer's choices changed; returns it with its key (part of the playback
/// cache key: cached frames are display-converted).
pub(crate) fn update(stage: &Stage, managed: bool, simulate: SimulateOutput) -> Current {
    let project = PrimPath::new(bae_motion::project_settings::PROJECT)
        .ok()
        .and_then(|p| stage.prim(&p));
    let get = |attr: &str| {
        project
            .as_ref()
            .and_then(|v| bae_motion::query::static_value(v, attr))
    };
    let name = match get("working_space") {
        Some(Value::String(s) | Value::Enum(s)) => s,
        _ => String::new(),
    };
    let flag = |attr: &str| matches!(get(attr), Some(Value::Bool(true)));
    let (linearize, gamma_1) = (flag("linearize"), flag("blend_colors_using_gamma_1"));
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (&name, linearize, gamma_1, managed, simulate).hash(&mut h);
    let key = h.finish().max(1);
    let mut current = cell().lock().unwrap();
    if current.0 != key {
        *current = (
            key,
            transform(&name, linearize, gamma_1, managed, simulate).map(Arc::new),
        );
    }
    current.clone()
}

/// The current transform and its key (0 before any viewer computed one).
pub(crate) fn current() -> Current {
    cell().lock().unwrap().clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmanaged_or_off_shows_the_numbers() {
        assert!(transform("", false, false, true, SimulateOutput::None).is_none());
        assert!(transform("HDTV (Rec. 709)", false, false, false, SimulateOutput::None).is_none());
        // sRGB on an sRGB display: nothing to do.
        assert!(
            transform(
                "sRGB IEC61966-2.1",
                false,
                false,
                true,
                SimulateOutput::None
            )
            .is_none()
        );
    }

    #[test]
    fn managed_projects_convert_to_the_display() {
        // Linearized sRGB: linear 0.5 is shown encoded (≈ 0.735).
        let t = transform("sRGB IEC61966-2.1", true, false, true, SimulateOutput::None).unwrap();
        let v = t.apply([0.5; 3]);
        assert!((v[0] - 0.7354).abs() < 1e-3, "{v:?}");
        // Legacy Macintosh RGB shows sRGB numbers as gamma 1.8: lighter.
        let mac = transform(
            "sRGB IEC61966-2.1",
            false,
            false,
            true,
            SimulateOutput::LegacyMac,
        )
        .unwrap();
        assert!(mac.apply([0.5; 3])[0] > 0.55);
        // Every working space of Project Settings resolves.
        for name in [
            "sRGB IEC61966-2.1",
            "HDTV (Rec. 709)",
            "Adobe RGB (1998)",
            "image P3",
            "ProPhoto RGB",
            "ACEScg ACES Working Space AMPAS S-2014-004",
        ] {
            assert!(working_space(name).is_some(), "{name}");
        }
    }
}
