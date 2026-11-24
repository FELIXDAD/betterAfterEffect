//! Lights and material options of 3D layers (AE-CMP-429 … 441): the data the
//! renderer receives. **Shading is not implemented yet**: 3D layers render unlit
//! (as AE does in a comp without lights). The next step evaluates AE-CMP-436's
//! lighting model per pixel from these values; the fields already carry
//! everything it needs, so producers (bae-anim adapter, importers, the UI) can
//! fill them now.

use glam::{DMat4, DVec3};

/// AE light type (AE-CMP-431).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LightKind {
    /// 平行光: a direction only.
    Parallel,
    /// 聚光: a cone from a position.
    #[default]
    Spot,
    /// 点光: omnidirectional from a position.
    Point,
    /// 周围光: uniform, no direction, no shadows.
    Ambient,
    /// 环境光 (Advanced 3D only in AE): image-based; kept as data.
    Environment,
}

/// Distance falloff (AE-CMP-433).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Falloff {
    /// No attenuation.
    #[default]
    None,
    /// Linear ramp from `radius` over `falloff_distance`.
    Smooth,
    /// `(radius / d)²` beyond `radius`.
    InverseSquareClamped,
}

/// One enabled light at one time (its video switch on, inside its in/out points).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    /// Light type.
    pub kind: LightKind,
    /// Light space → comp space (parents and auto-orient included). Spot and
    /// parallel lights shine along local +z; the position is the translation.
    pub world: DMat4,
    /// Point of interest, comp space.
    pub point_of_interest: DVec3,
    /// Straight RGB colour in the working space, 0–1 (overbright allowed).
    pub color: [f32; 3],
    /// Intensity in percent (negative subtracts light).
    pub intensity: f32,
    /// Spot cone angle in degrees (full cone).
    pub cone_angle: f32,
    /// Spot cone feather in percent.
    pub cone_feather: f32,
    /// Falloff type.
    pub falloff: Falloff,
    /// Falloff start radius in pixels.
    pub radius: f32,
    /// Smooth falloff distance in pixels.
    pub falloff_distance: f32,
    /// Casts shadows (not animatable).
    pub casts_shadows: bool,
    /// Shadow darkness in percent.
    pub shadow_darkness: f32,
    /// Shadow diffusion in pixels.
    pub shadow_diffusion: f32,
    /// Light layers with their Adjustment Layer switch on light only the layers
    /// below them (AE-CMP-435): the light's index in [`crate::RenderComp::layers`].
    /// `None` lights every 3D layer of the comp.
    pub scope_below: Option<usize>,
}

impl Light {
    /// Where the light is (comp space).
    pub fn position(&self) -> DVec3 {
        self.world.w_axis.truncate()
    }

    /// Unit direction the light shines in (local +z in comp space).
    pub fn direction(&self) -> DVec3 {
        self.world.z_axis.truncate().normalize_or(DVec3::Z)
    }
}

/// Off / On / Only switch of Casts Shadows and Accepts Shadows (AE-CMP-440).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ShadowMode {
    /// 关
    #[default]
    Off,
    /// 开
    On,
    /// 仅限: casts only (layer invisible) / shows only the shadows on it.
    Only,
}

/// A 3D layer's Material Options that matter for Classic 3D (AE-CMP-440/441).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// 投影 (AE default off).
    pub casts_shadows: ShadowMode,
    /// 透光率 in percent (AE default 0).
    pub light_transmission: f32,
    /// 接受阴影 (AE default on).
    pub accepts_shadows: ShadowMode,
    /// 接受光照 (AE default on).
    pub accepts_lights: bool,
    /// Ambient coefficient in percent (AE default 100).
    pub ambient: f32,
    /// Diffuse in percent (AE default 50).
    pub diffuse: f32,
    /// Specular intensity in percent (AE default 50).
    pub specular_intensity: f32,
    /// Specular shininess in percent (AE default 5).
    pub specular_shininess: f32,
    /// Metal in percent (AE default 100).
    pub metal: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            casts_shadows: ShadowMode::Off,
            light_transmission: 0.0,
            accepts_shadows: ShadowMode::On,
            accepts_lights: true,
            ambient: 100.0,
            diffuse: 50.0,
            specular_intensity: 50.0,
            specular_shininess: 5.0,
            metal: 100.0,
        }
    }
}
