//! Lights and material options of 3D layers (AE-CMP-429 … 441) and AE's Classic
//! 3D lighting model (AE-CMP-436), measured in AE 26.2.1
//! (`crates/bae-render/tests/ae/README.md`):
//!
//! ```text
//! C   = layer colour (straight), N = layer normal turned towards the eye,
//! V   = towards the eye, L = towards the light, H = normalize(L + V)
//! out = C · (ambient · Σ ambient lights + diffuse · Σ light · max(0, N·L))
//!     + specular · mix(1, C, metal) · Σ light · (N·H)^shininess   (where N·L > 0)
//! light = colour · intensity/100 · falloff · spot
//! spot    = smoothstep(cos(cone/2), cos(cone/2 · (1 − feather/100)), cos θ)
//! falloff = 1 (none) | 1 − smoothstep(R, R + F, d) (smooth) | min(1, (R/d)²) (inverse square)
//! ```
//!
//! Material percentages are fractions here (`shininess` is used as the exponent
//! in percent: 5 % → 5). The result is never negative; 8/16 bpc clip it to 1.
//! A comp without enabled lights, a 2D layer, Accepts Lights off, or a layer
//! no light reaches (lights with the Adjustment Layer switch light only the
//! layers below them): unlit.
//! Shadows are not rendered yet (casts / accepts shadows are data only).

use std::sync::Arc;

use glam::{DAffine2, DMat4, DVec3};

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
    /// `None` lights every 3D layer of the comp. A layer no light reaches is
    /// unlit (not dark; measured in AE 26.2.1).
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

/// How a 3D layer is lit in one frame (built by the render graph).
#[derive(Clone, Debug, PartialEq)]
pub struct Lighting {
    /// Layer pixels → comp space (the layer's 3D matrix at the frame time).
    pub matrix: DMat4,
    /// The layer's Material Options.
    pub material: Material,
    /// The lights that reach the layer (at least one).
    pub lights: Arc<[Light]>,
    /// Where the layer is seen from ([`crate::camera::Camera::eye`]).
    pub eye: DVec3,
}

/// GPU record of a [`Lighting`] (`struct` layout in `composite.wgsl`, as vec4s):
/// header vec4s, then [`LIGHT_VEC4S`] per light.
pub(crate) const HEADER_VEC4S: usize = 7;
/// Vec4s per light in a lighting record.
pub(crate) const LIGHT_VEC4S: usize = 4;

impl Lighting {
    /// Appends the record for input texels `from_texel` (input texel → layer
    /// pixel) to `out`; returns its offset (in vec4s).
    pub(crate) fn pack(&self, from_texel: &DAffine2, out: &mut Vec<[f32; 4]>) -> u32 {
        let first = out.len() as u32;
        let m = self.matrix;
        let s = from_texel.matrix2;
        let o = from_texel.translation;
        let v4 = |v: DVec3, w: f32| [v.x as f32, v.y as f32, v.z as f32, w];
        // P(u) = c0 · u.x + c1 · u.y + c2 in comp space.
        let x = m.x_axis.truncate();
        let y = m.y_axis.truncate();
        out.push(v4(x * s.x_axis.x + y * s.x_axis.y, 0.0));
        out.push(v4(x * s.y_axis.x + y * s.y_axis.y, 0.0));
        out.push(v4(m.transform_point3(o.extend(0.0)), 0.0));
        let normal = x.cross(y).normalize_or_zero();
        out.push(v4(normal, self.lights.len() as f32));
        out.push(v4(self.eye, 0.0));
        let mat = &self.material;
        out.push([
            mat.ambient / 100.0,
            mat.diffuse / 100.0,
            mat.specular_intensity / 100.0,
            mat.specular_shininess,
        ]);
        out.push([mat.metal / 100.0, 0.0, 0.0, 0.0]);
        for l in self.lights.iter() {
            let kind = match l.kind {
                LightKind::Ambient | LightKind::Environment => 0.0,
                LightKind::Parallel => 1.0,
                LightKind::Point => 2.0,
                LightKind::Spot => 3.0,
            };
            let falloff = match l.falloff {
                Falloff::None => 0.0,
                Falloff::Smooth => 1.0,
                Falloff::InverseSquareClamped => 2.0,
            };
            let k = l.intensity / 100.0;
            let outer = (f64::from(l.cone_angle) / 2.0).to_radians();
            let inner = outer * (1.0 - f64::from(l.cone_feather) / 100.0);
            out.push(v4(l.position(), kind));
            out.push(v4(l.direction(), falloff));
            out.push([l.color[0] * k, l.color[1] * k, l.color[2] * k, l.radius]);
            out.push([
                outer.cos() as f32,
                inner.cos() as f32,
                l.falloff_distance,
                0.0,
            ]);
        }
        first
    }
}

/// AE's lighting of one layer point (see the module docs): `color` straight
/// RGB, `point` and `normal` in comp space. The CPU reference of the shader.
pub fn shade(
    color: [f32; 3],
    point: DVec3,
    normal: DVec3,
    eye: DVec3,
    material: &Material,
    lights: &[Light],
) -> [f32; 3] {
    let v = (eye - point).normalize_or_zero();
    let mut n = normal.normalize_or_zero();
    if n.dot(v) < 0.0 {
        n = -n;
    }
    let (mut amb, mut diff, mut spec) = (DVec3::ZERO, DVec3::ZERO, DVec3::ZERO);
    for l in lights {
        let rgb = DVec3::new(
            f64::from(l.color[0]),
            f64::from(l.color[1]),
            f64::from(l.color[2]),
        ) * f64::from(l.intensity)
            / 100.0;
        if matches!(l.kind, LightKind::Ambient | LightKind::Environment) {
            amb += rgb;
            continue;
        }
        let (dir_l, d) = match l.kind {
            // From the point towards the light: against the light's direction.
            LightKind::Parallel => (-l.direction(), (point - l.position()).dot(l.direction())),
            _ => {
                let to = l.position() - point;
                (to.normalize_or_zero(), to.length())
            }
        };
        let r = f64::from(l.radius);
        let f = f64::from(l.falloff_distance);
        let atten = match l.falloff {
            Falloff::None => 1.0,
            Falloff::Smooth => 1.0 - smoothstep(r, r + f, d),
            Falloff::InverseSquareClamped if d > r => (r / d).powi(2),
            Falloff::InverseSquareClamped => 1.0,
        };
        let spot = if l.kind == LightKind::Spot {
            let outer = (f64::from(l.cone_angle) / 2.0).to_radians();
            let inner = outer * (1.0 - f64::from(l.cone_feather) / 100.0);
            smoothstep(outer.cos(), inner.cos(), l.direction().dot(-dir_l))
        } else {
            1.0
        };
        let nl = n.dot(dir_l);
        if nl <= 0.0 {
            continue;
        }
        let k = rgb * atten * spot;
        diff += k * nl;
        let h = (dir_l + v).normalize_or_zero();
        spec += k * n
            .dot(h)
            .max(0.0)
            .powf(f64::from(material.specular_shininess));
    }
    let c = DVec3::new(
        f64::from(color[0]),
        f64::from(color[1]),
        f64::from(color[2]),
    );
    let m = material;
    let metal = f64::from(m.metal) / 100.0;
    let spec_color = DVec3::ONE + (c - DVec3::ONE) * metal;
    let out = c * (f64::from(m.ambient) / 100.0 * amb + f64::from(m.diffuse) / 100.0 * diff)
        + f64::from(m.specular_intensity) / 100.0 * spec_color * spec;
    let out = out.max(DVec3::ZERO);
    [out.x as f32, out.y as f32, out.z as f32]
}

/// GLSL-style smoothstep (`e0 > e1` allowed: falls from 1 to 0).
fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    if e0 == e1 {
        return if x < e0 { 0.0 } else { 1.0 };
    }
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn light(kind: LightKind, at: DVec3) -> Light {
        Light {
            kind,
            world: DMat4::from_translation(at),
            point_of_interest: at + DVec3::Z,
            color: [1.0; 3],
            intensity: 100.0,
            cone_angle: 60.0,
            cone_feather: 0.0,
            falloff: Falloff::None,
            radius: 40.0,
            falloff_distance: 80.0,
            casts_shadows: false,
            shadow_darkness: 100.0,
            shadow_diffusion: 0.0,
            scope_below: None,
        }
    }

    const EYE: DVec3 = DVec3::new(100.0, 100.0, -277.8);
    const N: DVec3 = DVec3::new(0.0, 0.0, -1.0);

    fn diffuse_only() -> Material {
        Material {
            ambient: 0.0,
            diffuse: 100.0,
            specular_intensity: 0.0,
            ..Material::default()
        }
    }

    #[test]
    fn ambient_and_lambert_match_ae() {
        // AE 26.2.1 `amb`: ambient 50 %, material ambient 40 % → 51/255.
        let mut a = light(LightKind::Ambient, DVec3::ZERO);
        a.intensity = 50.0;
        let m = Material {
            ambient: 40.0,
            ..Material::default()
        };
        let c = shade([1.0; 3], DVec3::new(10.0, 10.0, 0.0), N, EYE, &m, &[a]);
        assert!((c[0] * 255.0 - 51.0).abs() < 0.5);
        // `pt_near`: point light 100 px in front of the centre; corner pixel 148.
        let p = light(LightKind::Point, DVec3::new(100.0, 100.0, -100.0));
        let c = shade(
            [1.0; 3],
            DVec3::new(0.5, 0.5, 0.0),
            N,
            EYE,
            &diffuse_only(),
            &[p],
        );
        assert!((c[0] * 255.0 - 148.0).abs() < 0.6, "{c:?}");
        // Seen from behind: the normal turns to the eye; lights behind are dark.
        let c = shade(
            [1.0; 3],
            DVec3::new(100.0, 100.0, 0.0),
            -N,
            EYE,
            &diffuse_only(),
            &[p],
        );
        assert!((c[0] - 1.0).abs() < 1e-6);
        let behind = light(LightKind::Point, DVec3::new(100.0, 100.0, 100.0));
        let c = shade(
            [1.0; 3],
            DVec3::new(100.0, 100.0, 0.0),
            N,
            EYE,
            &diffuse_only(),
            &[behind],
        );
        assert_eq!(c, [0.0; 3]);
    }

    #[test]
    fn spot_feather_is_smooth_in_cosine() {
        // `spot_f50` at θ ≈ 22°: AE 158/255 (cone 60°, feather 50 %).
        let mut s = light(LightKind::Spot, DVec3::new(100.0, 100.0, -150.0));
        s.cone_feather = 50.0;
        let p = DVec3::new(160.5, 100.5, 0.0);
        let c = shade([1.0; 3], p, N, EYE, &diffuse_only(), &[s]);
        let want = 158.0;
        assert!((c[0] * 255.0 - want).abs() < 1.5, "{}", c[0] * 255.0);
    }

    #[test]
    fn falloff_shapes() {
        let mut p = light(LightKind::Point, DVec3::new(100.0, 100.0, -20.0));
        p.falloff = Falloff::Smooth;
        let at = DVec3::new(140.5, 100.5, 0.0);
        // AE `fall_smooth` x = 140: 112; `fall_invsq`: 89.
        let c = shade([1.0; 3], at, N, EYE, &diffuse_only(), &[p]);
        assert!((c[0] * 255.0 - 112.0).abs() < 1.0, "{}", c[0] * 255.0);
        p.falloff = Falloff::InverseSquareClamped;
        let c = shade([1.0; 3], at, N, EYE, &diffuse_only(), &[p]);
        assert!((c[0] * 255.0 - 89.0).abs() < 1.0, "{}", c[0] * 255.0);
    }
}
