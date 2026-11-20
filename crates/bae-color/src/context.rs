//! Working space, view and display: the colour context of a render.
//!
//! [`ColorContext`] is what the render graph (04) needs per frame:
//!
//! * [`ColorContext::input_transform`] (via [`crate::interpret`]) — footage →
//!   buffer values, applied when a footage texture enters the graph;
//! * [`ColorContext::blend_space`] / [`ColorContext::blend_linearization`] —
//!   how layer blending treats the buffer values (AE's two modes, below);
//! * [`ColorContext::display_transform`] — buffer → swapchain, applied in the
//!   final viewer pass (never baked into cached frames, AE-CMP-457);
//! * [`ColorContext::output_transform`] — buffer → output file space (render
//!   queue, AE-CMP-456).
//!
//! # AE's two blending models
//!
//! * **Non-linear blending (AE default).** Buffers hold the working space's
//!   *encoded* values (e.g. sRGB-encoded); blend modes, opacity and effects
//!   operate on those numbers. [`BlendSpace::Stored`].
//! * **Linearize Working Space.** Buffers hold *linear* values in the working
//!   primaries; everything (blending, resampling, blur …) is linear.
//!   [`BlendSpace::LinearBuffers`]: the buffer space itself is linear, so
//!   blending needs no extra conversion.
//! * **Blend Colors Using 1.0 Gamma** (without linearize). Buffers stay
//!   encoded, only layer blending happens in linear light: decode both inputs,
//!   blend, re-encode. [`BlendSpace::LinearBlend`] with a
//!   [`BlendLinearization`]. With colour management off AE uses gamma 2.2 for
//!   this (AE-CMP-452).

use std::hash::{Hash, Hasher};

use rustc_hash::FxHasher;
use serde::{Deserialize, Serialize};

use crate::ColorError;
use crate::space::{ColorSpace, builtin};
use crate::transfer::TransferFunction;
use crate::transform::{ColorOp, ColorTransform};

/// AE `Project.workingGamma`: the display gamma assumed when colour management
/// is off (Working Color Space = None).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnmanagedGamma {
    /// 2.2 (also the gamma of "Blend Colors Using 1.0 Gamma" when unmanaged).
    #[default]
    G22,
    /// 2.4.
    G24,
}

impl UnmanagedGamma {
    /// The exponent.
    pub fn exponent(self) -> f64 {
        match self {
            UnmanagedGamma::G22 => 2.2,
            UnmanagedGamma::G24 => 2.4,
        }
    }

    /// The pure-gamma Rec.709 space it implies.
    pub fn space(self) -> ColorSpace {
        match self {
            UnmanagedGamma::G22 => builtin::GAMMA22,
            UnmanagedGamma::G24 => builtin::REC709_GAMMA24,
        }
    }
}

/// How layer blending treats buffer values. See the module docs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlendSpace {
    /// Blend the stored (encoded) values (AE default).
    #[default]
    Stored,
    /// Buffers are linear ("Linearize Working Space"); blend the stored values.
    LinearBuffers,
    /// Buffers are encoded; blending decodes to linear and re-encodes ("Blend
    /// Colors Using 1.0 Gamma").
    LinearBlend,
}

/// Decode / encode pair around a blend for [`BlendSpace::LinearBlend`]. Both are
/// per-channel curves (no matrix), so 04 may apply them to straight colour
/// right before and after the blend-mode maths.
#[derive(Clone, Debug, PartialEq)]
pub struct BlendLinearization {
    /// Buffer values → linear.
    pub decode: ColorTransform,
    /// Linear → buffer values.
    pub encode: ColorTransform,
}

/// Project colour settings (AE Project Settings > Color, Adobe engine), as
/// stored on the `/project` prim (`working_space`, `linearize`,
/// `blend_colors_using_gamma_1`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct WorkingSpace {
    /// The working space; `None` = AE "None" (colour management off).
    pub space: Option<ColorSpace>,
    /// "Linearize Working Space" (ignored when unmanaged).
    pub linearize: bool,
    /// "Blend Colors Using 1.0 Gamma" (ignored when linearized).
    pub blend_gamma_1: bool,
    /// Working gamma when unmanaged.
    pub unmanaged_gamma: UnmanagedGamma,
}

impl WorkingSpace {
    /// Colour management off (AE default for new projects).
    pub fn unmanaged() -> Self {
        Self::default()
    }

    /// A managed working space with AE's default (non-linear) blending.
    pub fn managed(space: ColorSpace) -> Self {
        WorkingSpace {
            space: Some(space),
            ..Self::default()
        }
    }

    /// With "Linearize Working Space".
    pub fn linearized(mut self, on: bool) -> Self {
        self.linearize = on;
        self
    }

    /// With "Blend Colors Using 1.0 Gamma".
    pub fn blend_gamma_1(mut self, on: bool) -> Self {
        self.blend_gamma_1 = on;
        self
    }

    /// From the `/project` prim attributes. `working_space` is a colour space
    /// id (empty = None), resolved with `resolve` (e.g. [`builtin::by_id`] or an
    /// OCIO config).
    pub fn from_project_attrs(
        working_space: &str,
        linearize: bool,
        blend_colors_using_gamma_1: bool,
        resolve: impl Fn(&str) -> Option<ColorSpace>,
    ) -> Result<Self, ColorError> {
        let space = if working_space.is_empty() {
            None
        } else {
            Some(
                resolve(working_space)
                    .ok_or_else(|| ColorError::UnknownSpace(working_space.to_string()))?,
            )
        };
        Ok(WorkingSpace {
            space,
            linearize,
            blend_gamma_1: blend_colors_using_gamma_1,
            unmanaged_gamma: UnmanagedGamma::default(),
        })
    }

    /// `true` when colour management is on.
    pub fn is_managed(&self) -> bool {
        self.space.is_some()
    }

    /// The space render buffers hold: the working space, or its linear version
    /// with "Linearize Working Space". `None` when unmanaged (buffers hold the
    /// footage numbers as they are).
    pub fn buffer_space(&self) -> Option<ColorSpace> {
        let space = self.space.as_ref()?;
        if self.linearize {
            Some(space.linearized().unwrap_or_else(|| space.clone()))
        } else {
            Some(space.clone())
        }
    }

    /// How blending treats buffer values.
    pub fn blend_space(&self) -> BlendSpace {
        let buffers_linear = self.buffer_space().is_some_and(|s| s.is_linear());
        if buffers_linear {
            BlendSpace::LinearBuffers
        } else if self.blend_gamma_1 {
            BlendSpace::LinearBlend
        } else {
            BlendSpace::Stored
        }
    }

    /// The curve pair for [`BlendSpace::LinearBlend`]; `None` otherwise.
    pub fn blend_linearization(&self) -> Option<BlendLinearization> {
        if self.blend_space() != BlendSpace::LinearBlend {
            return None;
        }
        let tf = match self.buffer_space() {
            Some(space) => space.transfer().unwrap_or(TransferFunction::Linear),
            None => TransferFunction::Gamma(self.unmanaged_gamma.exponent()),
        };
        if tf.is_linear() {
            return None;
        }
        Some(BlendLinearization {
            decode: ColorTransform::from_ops([ColorOp::Decode(tf)]),
            encode: ColorTransform::from_ops([ColorOp::Encode(tf)]),
        })
    }
}

/// What the viewer does between buffer values and the display.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ViewTransform {
    /// AE "Use Display Color Management" off: buffer numbers are shown as if
    /// they were the monitor's own encoded values.
    Raw,
    /// Colorimetric conversion buffer → display (AE "Use Display Color
    /// Management" on, the default).
    #[default]
    Colorimetric,
}

/// How the swapchain stores what the final shader writes. Chosen by the
/// platform layer (bae-gpu's `SurfaceInfo`) — bae-color only describes it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceEncoding {
    /// Unorm, non-sRGB format: the shader writes monitor-encoded values.
    Direct,
    /// `*UnormSrgb` format: the hardware applies the sRGB curve on write, so the
    /// shader writes sRGB-decoded monitor values.
    HardwareSrgb,
    /// Float swapchain in an extended linear space (macOS EDR / Windows scRGB:
    /// linear sRGB primaries; `ExtendedDisplayP3`: linear P3). 1.0 = SDR white,
    /// values above 1 use the HDR headroom, negatives express wide gamut.
    ExtendedLinear(ColorSpace),
    /// HDR10 swapchain (BT.2100 PQ, Rec.2020 primaries).
    Pq,
}

/// The display a viewer draws to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DisplayTarget {
    /// The monitor's native space (from the OS display profile: ColorSync on
    /// macOS, Advanced Color on Windows — provided by the platform layer).
    pub monitor: ColorSpace,
    /// Swapchain storage.
    pub surface: SurfaceEncoding,
    /// Brightest displayable value relative to SDR white (1.0 for SDR, the EDR
    /// headroom on macOS).
    pub headroom: f32,
}

impl Eq for DisplayTarget {}

impl Hash for DisplayTarget {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.monitor.hash(state);
        self.surface.hash(state);
        self.headroom.to_bits().hash(state);
    }
}

impl Default for DisplayTarget {
    fn default() -> Self {
        Self::sdr(builtin::SRGB, false)
    }
}

impl DisplayTarget {
    /// An SDR display. `hardware_srgb`: the swapchain format is `*UnormSrgb`.
    pub fn sdr(monitor: ColorSpace, hardware_srgb: bool) -> Self {
        DisplayTarget {
            monitor,
            surface: if hardware_srgb {
                SurfaceEncoding::HardwareSrgb
            } else {
                SurfaceEncoding::Direct
            },
            headroom: 1.0,
        }
    }

    /// macOS EDR / Windows scRGB: `Rgba16Float` with extended linear sRGB.
    pub fn extended_linear_srgb(monitor: ColorSpace, headroom: f32) -> Self {
        DisplayTarget {
            monitor,
            surface: SurfaceEncoding::ExtendedLinear(builtin::SRGB_LINEAR),
            headroom: headroom.max(1.0),
        }
    }

    /// `Rgba16Float` with extended linear Display P3.
    pub fn extended_linear_p3(monitor: ColorSpace, headroom: f32) -> Self {
        DisplayTarget {
            monitor,
            surface: SurfaceEncoding::ExtendedLinear(builtin::P3_D65_LINEAR),
            headroom: headroom.max(1.0),
        }
    }

    /// HDR10 (PQ) swapchain.
    pub fn hdr10(headroom: f32) -> Self {
        DisplayTarget {
            monitor: builtin::REC2100_PQ,
            surface: SurfaceEncoding::Pq,
            headroom: headroom.max(1.0),
        }
    }

    /// `true` when values above SDR white are visible.
    pub fn is_hdr(&self) -> bool {
        self.headroom > 1.0
    }

    /// `src` (colorimetric) → what the shader must write.
    fn transform_from(&self, src: &ColorSpace) -> Result<ColorTransform, ColorError> {
        Ok(match &self.surface {
            SurfaceEncoding::Direct => src.transform_to(&self.monitor)?,
            SurfaceEncoding::HardwareSrgb => {
                src.transform_to(&self.monitor)?
                    .then(&ColorTransform::from_ops([ColorOp::Decode(
                        TransferFunction::Srgb,
                    )]))
            }
            SurfaceEncoding::ExtendedLinear(space) => src.transform_to(space)?,
            SurfaceEncoding::Pq => src.transform_to(&builtin::REC2100_PQ)?,
        })
    }
}

/// The colour context of a render: working space, view and display.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ColorContext {
    /// Project working space settings.
    pub working: WorkingSpace,
    /// Viewer transform.
    pub view: ViewTransform,
    /// Display.
    pub display: DisplayTarget,
}

impl ColorContext {
    /// A context with the given working space, colorimetric view and an sRGB display.
    pub fn new(working: WorkingSpace) -> Self {
        ColorContext {
            working,
            ..Self::default()
        }
    }

    /// With a view transform.
    pub fn with_view(mut self, view: ViewTransform) -> Self {
        self.view = view;
        self
    }

    /// With a display.
    pub fn with_display(mut self, display: DisplayTarget) -> Self {
        self.display = display;
        self
    }

    /// See [`WorkingSpace::blend_space`].
    pub fn blend_space(&self) -> BlendSpace {
        self.working.blend_space()
    }

    /// See [`WorkingSpace::blend_linearization`].
    pub fn blend_linearization(&self) -> Option<BlendLinearization> {
        self.working.blend_linearization()
    }

    /// Footage in `footage_space` → buffer values. Unmanaged projects pass
    /// numbers through. Prefer [`crate::interpret::input_transform`], which
    /// also applies the footage's Interpret Footage settings.
    pub fn input_transform(
        &self,
        footage_space: &ColorSpace,
    ) -> Result<ColorTransform, ColorError> {
        match self.working.buffer_space() {
            Some(buffer) => footage_space.transform_to(&buffer),
            None => Ok(ColorTransform::identity()),
        }
    }

    /// Buffer values → swapchain values for the viewer.
    pub fn display_transform(&self) -> Result<ColorTransform, ColorError> {
        let managed = self.view == ViewTransform::Colorimetric;
        let src = match self.working.buffer_space() {
            Some(buffer) if managed => buffer,
            // Unmanaged or "display colour management" off: the numbers are the
            // monitor's own values.
            _ => self.display.monitor.clone(),
        };
        self.display.transform_from(&src)
    }

    /// The output module's default colour space (AE: "Working Color Space"):
    /// the working space *without* linearization — AE converts linearized
    /// buffers back to the working space's curve when writing files (measured
    /// in AE 26.2). `None` when unmanaged.
    pub fn default_output_space(&self) -> Option<ColorSpace> {
        self.working.space.clone()
    }

    /// Buffer values → an output file's colour space (render queue). Unmanaged
    /// projects write numbers unchanged.
    pub fn output_transform(&self, output: &ColorSpace) -> Result<ColorTransform, ColorError> {
        match self.working.buffer_space() {
            Some(buffer) => buffer.transform_to(output),
            None => Ok(ColorTransform::identity()),
        }
    }

    /// Converts one working-space colour (a colour property / picker value,
    /// interpreted in the working space as AE does, AE-CMP-447) to what the
    /// viewer shows, for UI swatches that must match the image.
    pub fn display_rgb(&self, working_rgb: [f32; 3]) -> Result<[f32; 3], ColorError> {
        Ok(self.display_transform()?.apply(working_rgb))
    }

    /// Hash of everything that changes rendered (pre-display) pixels: the
    /// frame-cache key part contributed by colour. The display and view are not
    /// included because they never touch cached frames.
    pub fn render_fingerprint(&self) -> u64 {
        let mut h = FxHasher::default();
        self.working.hash(&mut h);
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ae_blend_modes() {
        let ws = WorkingSpace::unmanaged();
        assert_eq!(ws.blend_space(), BlendSpace::Stored);
        assert!(ws.blend_linearization().is_none());

        let ws = WorkingSpace::unmanaged().blend_gamma_1(true);
        assert_eq!(ws.blend_space(), BlendSpace::LinearBlend);
        let lin = ws.blend_linearization().unwrap();
        assert_eq!(
            lin.decode.ops(),
            &[ColorOp::Decode(TransferFunction::Gamma(2.2))]
        );

        let ws = WorkingSpace::managed(builtin::SRGB).blend_gamma_1(true);
        let lin = ws.blend_linearization().unwrap();
        assert_eq!(lin.encode.ops(), &[ColorOp::Encode(TransferFunction::Srgb)]);

        let ws = WorkingSpace::managed(builtin::SRGB)
            .linearized(true)
            .blend_gamma_1(true);
        assert_eq!(ws.blend_space(), BlendSpace::LinearBuffers);
        assert!(ws.blend_linearization().is_none());
        assert_eq!(
            ws.buffer_space().unwrap().transfer(),
            Some(TransferFunction::Linear)
        );

        let ws = WorkingSpace::managed(builtin::ACESCG);
        assert_eq!(ws.blend_space(), BlendSpace::LinearBuffers);
    }

    #[test]
    fn project_attrs() {
        let ws = WorkingSpace::from_project_attrs("", false, true, builtin::by_id).unwrap();
        assert!(!ws.is_managed());
        let ws = WorkingSpace::from_project_attrs("acescg", false, false, builtin::by_id).unwrap();
        assert_eq!(ws.space, Some(builtin::ACESCG));
        assert!(WorkingSpace::from_project_attrs("bogus", false, false, builtin::by_id).is_err());
    }

    #[test]
    fn display_transforms() {
        // sRGB working, sRGB monitor, non-sRGB surface: identity.
        let ctx = ColorContext::new(WorkingSpace::managed(builtin::SRGB));
        assert!(ctx.display_transform().unwrap().is_identity());
        // Linearized sRGB working on the same display: one encode.
        let ctx = ColorContext::new(WorkingSpace::managed(builtin::SRGB).linearized(true));
        assert_eq!(
            ctx.display_transform().unwrap().ops(),
            &[ColorOp::Encode(TransferFunction::Srgb)]
        );
        // ... on a hardware-sRGB surface: nothing left to do.
        let ctx = ctx.with_display(DisplayTarget::sdr(builtin::SRGB, true));
        assert!(ctx.display_transform().unwrap().is_identity());
        // Unmanaged on hardware sRGB: numbers must survive the hardware encode.
        let ctx = ColorContext::default().with_display(DisplayTarget::sdr(builtin::SRGB, true));
        assert_eq!(
            ctx.display_transform().unwrap().ops(),
            &[ColorOp::Decode(TransferFunction::Srgb)]
        );
        // EDR: ACEScg → linear sRGB is one matrix.
        let ctx = ColorContext::new(WorkingSpace::managed(builtin::ACESCG)).with_display(
            DisplayTarget::extended_linear_srgb(builtin::DISPLAY_P3, 2.0),
        );
        assert!(ctx.display_transform().unwrap().as_matrix().is_some());
        // Raw view: monitor numbers.
        let ctx =
            ColorContext::new(WorkingSpace::managed(builtin::ACESCG)).with_view(ViewTransform::Raw);
        assert!(ctx.display_transform().unwrap().is_identity());
    }

    #[test]
    fn linearized_output_returns_to_working_curve() {
        let ctx = ColorContext::new(WorkingSpace::managed(builtin::SRGB).linearized(true));
        let input = ctx.input_transform(&builtin::SRGB).unwrap();
        let output = ctx
            .output_transform(&ctx.default_output_space().unwrap())
            .unwrap();
        assert!(input.then(&output).is_identity());
    }

    #[test]
    fn display_rgb_for_ui() {
        let ctx = ColorContext::new(WorkingSpace::managed(builtin::SRGB).linearized(true));
        let v = ctx
            .display_rgb([0.214_041_14, 0.214_041_14, 0.214_041_14])
            .unwrap();
        assert!((v[0] - 0.5).abs() < 1e-5, "{v:?}");
    }
}
