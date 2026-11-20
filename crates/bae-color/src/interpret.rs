//! Footage interpretation (AE Interpret Footage > Color, AE-CMP-454/455):
//! which colour space a source is in, and the transform into the buffers.
//!
//! Precedence, as in AE: Preserve RGB > override > file tags > "assume the
//! working space" (untagged footage is not converted).

use serde::{Deserialize, Serialize};

use crate::ColorError;
use crate::context::WorkingSpace;
use crate::primaries::Chromaticities;
use crate::space::{ColorSpace, builtin};
use crate::transfer::TransferFunction;
use crate::transform::{ColorOp, ColorTransform};

/// The colour tags found in a file, independent of `bae-media` (enable the
/// `media` feature for `From<&bae_media::ColorInfo>`). `None` = not tagged.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SourceTags {
    /// Primaries and white.
    pub primaries: Option<Chromaticities>,
    /// Tone curve.
    pub transfer: Option<TransferFunction>,
}

impl SourceTags {
    /// sRGB-tagged.
    pub fn srgb() -> Self {
        SourceTags {
            primaries: Some(Chromaticities::REC709),
            transfer: Some(TransferFunction::Srgb),
        }
    }

    /// `true` when nothing is tagged.
    pub fn is_untagged(&self) -> bool {
        self.primaries.is_none() && self.transfer.is_none()
    }
}

/// AE "Interpret As Linear Light".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinearLight {
    /// Off.
    #[default]
    Off,
    /// On.
    On,
    /// On only in 32 bpc projects.
    On32Bpc,
}

/// Per-footage colour interpretation (stored on the footage prim).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Interpretation {
    /// "Override Media Color Space": a colour space id; `None` = use the tags.
    pub override_space: Option<String>,
    /// "Preserve RGB": no conversion at all.
    pub preserve_rgb: bool,
    /// "Interpret As Linear Light".
    pub linear_light: LinearLight,
}

impl Interpretation {
    /// Follow the file tags (the default).
    pub fn auto() -> Self {
        Self::default()
    }

    /// Override the media colour space with a space id.
    pub fn with_override(mut self, id: impl Into<String>) -> Self {
        self.override_space = Some(id.into());
        self
    }

    /// Preserve RGB.
    pub fn preserving_rgb(mut self) -> Self {
        self.preserve_rgb = true;
        self
    }

    /// Interpret As Linear Light.
    pub fn with_linear_light(mut self, mode: LinearLight) -> Self {
        self.linear_light = mode;
        self
    }

    /// From the footage prim's `color_space` attribute (empty = auto).
    pub fn from_footage_attr(color_space: &str) -> Self {
        Interpretation {
            override_space: (!color_space.is_empty()).then(|| color_space.to_string()),
            ..Self::default()
        }
    }

    fn linear_light_active(&self, bits_per_channel: u8) -> bool {
        match self.linear_light {
            LinearLight::Off => false,
            LinearLight::On => true,
            LinearLight::On32Bpc => bits_per_channel >= 32,
        }
    }
}

/// Recognizes the colour space the tags describe. Known combinations map to
/// built-in spaces; others become a custom RGB space with an id like
/// `media:<primaries>/<curve>`. Partially tagged sources assume Rec.709
/// primaries or the sRGB curve for the missing half. `None` when untagged.
pub fn detect(tags: &SourceTags) -> Option<ColorSpace> {
    if tags.is_untagged() {
        return None;
    }
    let gamut = tags.primaries.unwrap_or(Chromaticities::REC709);
    let transfer = tags.transfer.unwrap_or(TransferFunction::Srgb);
    const EPS: f64 = 0.002;
    let known = builtin::ALL.into_iter().find(|s| {
        s.gamut().is_some_and(|g| g.approx_eq(&gamut, EPS)) && s.transfer() == Some(transfer)
    });
    if let Some(space) = known {
        return Some(space);
    }
    // Same primaries as a built-in space but another curve: keep its adaptation.
    if let Some(base) = builtin::ALL
        .into_iter()
        .find(|s| s.gamut().is_some_and(|g| g.approx_eq(&gamut, EPS)))
    {
        return base.with_transfer(transfer);
    }
    let id = format!(
        "media:{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4}/{}",
        gamut.red[0],
        gamut.red[1],
        gamut.green[0],
        gamut.green[1],
        gamut.blue[0],
        gamut.blue[1],
        gamut.white[0],
        gamut.white[1],
        transfer.id()
    );
    Some(ColorSpace::custom_rgb(id.clone(), id, gamut, transfer))
}

/// The "Media Color Space" AE would show and use for a footage item: override
/// (resolved with `resolve`) or detected tags, or `None` when untagged. Preserve
/// RGB yields [`builtin::RAW`].
pub fn footage_space(
    tags: &SourceTags,
    interp: &Interpretation,
    resolve: impl Fn(&str) -> Option<ColorSpace>,
) -> Result<Option<ColorSpace>, ColorError> {
    if interp.preserve_rgb {
        return Ok(Some(builtin::RAW));
    }
    if let Some(id) = &interp.override_space {
        return resolve(id)
            .map(Some)
            .ok_or_else(|| ColorError::UnknownSpace(id.clone()));
    }
    Ok(detect(tags))
}

/// The transform from decoded footage values to buffer values, applying the
/// footage's interpretation and the project working space.
///
/// * Preserve RGB → identity.
/// * Unmanaged project → identity, except "Interpret As Linear Light", which
///   encodes the (linear) numbers with the working gamma (AE-CMP-455).
/// * Managed → media space (override / tags / working space if untagged), made
///   linear when "Interpret As Linear Light" is active, converted to the buffer
///   space.
pub fn input_transform(
    tags: &SourceTags,
    interp: &Interpretation,
    working: &WorkingSpace,
    bits_per_channel: u8,
    resolve: impl Fn(&str) -> Option<ColorSpace>,
) -> Result<ColorTransform, ColorError> {
    if interp.preserve_rgb {
        return Ok(ColorTransform::identity());
    }
    let linear_light = interp.linear_light_active(bits_per_channel);
    let Some(buffer) = working.buffer_space() else {
        return Ok(if linear_light {
            ColorTransform::from_ops([ColorOp::Encode(TransferFunction::Gamma(
                working.unmanaged_gamma.exponent(),
            ))])
        } else {
            ColorTransform::identity()
        });
    };
    let media = match footage_space(tags, interp, resolve)? {
        Some(space) => space,
        // Untagged: assume the (non-linearized) working space.
        None => working.space.clone().unwrap_or(builtin::RAW),
    };
    let media = if linear_light {
        media.linearized().unwrap_or(media)
    } else {
        media
    };
    media.transform_to(&buffer)
}

#[cfg(feature = "media")]
mod media {
    use super::SourceTags;
    use crate::primaries::Chromaticities;
    use crate::transfer::TransferFunction;

    fn chroma(c: bae_media::Chromaticities) -> Chromaticities {
        let f = |v: [f32; 2]| [v[0] as f64, v[1] as f64];
        Chromaticities {
            red: f(c.red),
            green: f(c.green),
            blue: f(c.blue),
            white: f(c.white),
        }
    }

    impl From<&bae_media::ColorInfo> for SourceTags {
        fn from(info: &bae_media::ColorInfo) -> Self {
            use bae_media::ColorPrimaries as P;
            use bae_media::TransferFunction as T;
            let primaries = match info.primaries {
                P::Unspecified => None,
                P::Bt709 => Some(Chromaticities::REC709),
                P::Bt470M => Some(Chromaticities::BT470M),
                P::Bt601Pal => Some(Chromaticities::BT601_625),
                P::Bt601Ntsc => Some(Chromaticities::BT601_525),
                P::Film => Some(Chromaticities::FILM),
                P::Bt2020 => Some(Chromaticities::REC2020),
                // XYZ has no RGB primaries; treat as untagged primaries.
                P::Xyz => None,
                P::DciP3 => Some(Chromaticities::DCI_P3),
                P::DisplayP3 => Some(Chromaticities::P3_D65),
                P::AcesAp0 => Some(Chromaticities::ACES_AP0),
                P::AcesAp1 => Some(Chromaticities::ACES_AP1),
                P::Custom(c) => Some(chroma(c)),
            };
            let transfer = match info.transfer {
                T::Unspecified => None,
                T::Bt709 => Some(TransferFunction::Rec709),
                T::Srgb => Some(TransferFunction::Srgb),
                T::Linear => Some(TransferFunction::Linear),
                T::Pq => Some(TransferFunction::Pq),
                T::Hlg => Some(TransferFunction::Hlg),
                T::Gamma(g) => Some(TransferFunction::Gamma(g as f64)),
            };
            SourceTags {
                primaries,
                transfer,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection() {
        assert_eq!(detect(&SourceTags::default()), None);
        assert_eq!(detect(&SourceTags::srgb()), Some(builtin::SRGB));
        let pq = SourceTags {
            primaries: Some(Chromaticities::REC2020),
            transfer: Some(TransferFunction::Pq),
        };
        assert_eq!(detect(&pq), Some(builtin::REC2100_PQ));
        let lin_p3 = SourceTags {
            primaries: Some(Chromaticities::P3_D65),
            transfer: Some(TransferFunction::Linear),
        };
        assert_eq!(detect(&lin_p3), Some(builtin::P3_D65_LINEAR));
        let odd = SourceTags {
            primaries: Some(Chromaticities::ACES_AP1),
            transfer: Some(TransferFunction::Srgb),
        };
        let s = detect(&odd).unwrap();
        assert_eq!(s.gamut(), Some(Chromaticities::ACES_AP1));
        let custom = SourceTags {
            primaries: Some(Chromaticities::BT601_525),
            transfer: Some(TransferFunction::Rec709),
        };
        assert!(detect(&custom).unwrap().id.starts_with("media:"));
    }

    #[test]
    fn input_transforms_follow_ae_precedence() {
        let ws = WorkingSpace::managed(builtin::SRGB);
        let r = builtin::by_id;
        // Untagged in a managed project: assumed to be the working space.
        assert!(
            input_transform(&SourceTags::default(), &Interpretation::auto(), &ws, 8, r)
                .unwrap()
                .is_identity()
        );
        // Tagged sRGB in sRGB: identity.
        assert!(
            input_transform(&SourceTags::srgb(), &Interpretation::auto(), &ws, 8, r)
                .unwrap()
                .is_identity()
        );
        // Override wins over tags.
        let t = input_transform(
            &SourceTags::srgb(),
            &Interpretation::auto().with_override("display-p3"),
            &ws,
            8,
            r,
        )
        .unwrap();
        assert!(!t.is_identity());
        // Preserve RGB wins over everything.
        let i = Interpretation::auto()
            .with_override("display-p3")
            .preserving_rgb();
        assert!(
            input_transform(&SourceTags::srgb(), &i, &ws, 8, r)
                .unwrap()
                .is_identity()
        );
        // Unknown override is an error.
        assert!(
            input_transform(
                &SourceTags::srgb(),
                &Interpretation::auto().with_override("x"),
                &ws,
                8,
                r
            )
            .is_err()
        );
        // Linear light in an unmanaged project encodes with gamma 2.2.
        let i = Interpretation::auto().with_linear_light(LinearLight::On32Bpc);
        let un = WorkingSpace::unmanaged();
        assert!(
            input_transform(&SourceTags::default(), &i, &un, 16, r)
                .unwrap()
                .is_identity()
        );
        let t = input_transform(&SourceTags::default(), &i, &un, 32, r).unwrap();
        assert_eq!(t.ops(), &[ColorOp::Encode(TransferFunction::Gamma(2.2))]);
        // Linearized sRGB working: sRGB footage gets one decode.
        let lin = WorkingSpace::managed(builtin::SRGB).linearized(true);
        let t = input_transform(&SourceTags::srgb(), &Interpretation::auto(), &lin, 32, r).unwrap();
        assert_eq!(t.ops(), &[ColorOp::Decode(TransferFunction::Srgb)]);
    }
}
