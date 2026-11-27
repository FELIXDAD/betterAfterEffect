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
//!
//! Shadows (AE-CMP-437, also measured): a light with Casts Shadows on scales its
//! term at a point by `Π over casters (1 − darkness · a · (1 − transmission · C_c))`
//! where `a` is the caster's coverage (alpha × opacity) between the point and the
//! light and `C_c` its own (unlit) colour. Shadow Diffusion acts as a square light
//! of that half-size: `a` is the caster's coverage averaged over the light's
//! footprint on the caster plane, at least [`SHADOW_SOFTNESS`] receiver pixels
//! (AE's shadow-map edge). Casters and receivers are the 3D layers of one 3D group
//! with Casts / Accepts Shadows on.
//! A comp without enabled lights, a 2D layer, Accepts Lights off, or a layer
//! no light reaches (lights with the Adjustment Layer switch light only the
//! layers below them): unlit.
//! Shadows: casters in the same 3D group, tested per pixel (`shadow` in
//! `composite.wgsl`).

use std::sync::Arc;

use glam::{DAffine2, DMat4, DVec2, DVec3};

use crate::geom::Rect;
use crate::graph::NodeId;

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
    /// Environment lights with an image (AE's Advanced 3D): the source layer
    /// lights 3D models from every direction ([`EnvironmentSource`]).
    pub environment: Option<EnvironmentSource>,
}

/// The image of an environment light: an equirectangular (latitude /
/// longitude) picture of everything around the scene, turned by the light's
/// rotation. It lights 3D models (diffuse and reflections) and can show
/// behind the layers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvironmentSource {
    /// The source layer: its index in [`crate::RenderComp::layers`].
    pub layer: usize,
    /// Background Visible: the image's opacity and blur behind the layers
    /// (both 0–1).
    pub background: Option<[f32; 2]>,
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
    /// Comp-space size of one comp pixel at the layer: perspective cameras
    /// `1 / zoom` (times the distance to the eye), orthographic views
    /// `−1 / scale` (a constant, stored negative). Sizes the shadow edge.
    pub world_per_pixel: f64,
    /// The other layers of its 3D group that cast shadows on it (empty when it
    /// does not accept shadows or no light casts shadows).
    pub casters: Arc<[ShadowCaster]>,
    /// Motion blur: the layer, its lights and the eye at every sample (empty:
    /// every sample is lit as the frame).
    pub samples: Arc<[LightingSample]>,
    /// The layer's drawn rectangle in layer pixels (the shadow tile grid).
    pub rect: Rect,
    /// Its receiver-space shadow map ([`crate::graph::ShadowMapNode`]): lit
    /// draws sample it instead of testing the casters.
    pub shadow_map: Option<crate::graph::NodeId>,
    /// 3D models of its group that cast shadows on it (drawn into its shadow
    /// map only: a layer with model casters always uses the map).
    pub models: Arc<[ModelCaster]>,
    /// Advanced 3D reflections of an environment light's picture.
    pub reflection: Option<Reflection>,
}

/// Advanced 3D Reflection Intensity, Sharpness and Rolloff of a 3D layer,
/// reflecting the picture of an environment light that reaches it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reflection {
    pub environment: crate::graph::EnvironmentInput,
    /// 0–1.
    pub intensity: f32,
    /// 0–1: 1 a mirror, lower blurrier.
    pub sharpness: f32,
    /// 0–1: reflections only at glancing angles at 1.
    pub rolloff: f32,
    /// A linear working space (the picture is not display-encoded).
    pub linear: bool,
}

/// A 3D model layer casting shadows on a lit layer.
#[derive(Clone, Debug)]
pub struct ModelCaster {
    pub model: Arc<crate::model::Model>,
    /// Model space → comp space.
    pub matrix: DMat4,
    /// Advanced 3D Shadow Color (straight RGB).
    pub shadow_color: [f32; 3],
}

impl PartialEq for ModelCaster {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.model, &other.model)
            && self.matrix == other.matrix
            && self.shadow_color == other.shadow_color
    }
}

/// Largest side of a shadow map slot, in texels.
const SHADOW_MAP_SIDE: f64 = 1024.0;

impl Lighting {
    /// The lights with a shadow map slot (casting shadows), in slot order.
    pub(crate) fn shadow_slots(&self) -> Vec<usize> {
        (0..self.lights.len())
            .filter(|&i| self.lights[i].casts_shadows)
            .take(8)
            .collect()
    }

    /// Layer pixels per shadow map texel, and a slot's size in texels: twice
    /// the layer's own resolution (sampling keeps the edges sharp), at most
    /// [`SHADOW_MAP_SIDE`] a side.
    pub(crate) fn shadow_map_size(&self) -> (f64, [u32; 2]) {
        let (w, h) = (self.rect.x1 - self.rect.x0, self.rect.y1 - self.rect.y0);
        let k = (w.max(h) / SHADOW_MAP_SIDE).max(0.5);
        (
            k,
            [
                (w / k).ceil().max(1.0) as u32,
                (h / k).ceil().max(1.0) as u32,
            ],
        )
    }
}

/// Tiles per side of a lit layer's shadow grid.
const SHADOW_TILES: usize = 8;
/// Most casters the shadow grid lists (4 × 32 bits per tile).
const SHADOW_TILE_CASTERS: usize = 128;

/// A motion-blur sample of a lit layer.
#[derive(Clone, Debug, PartialEq)]
pub struct LightingSample {
    /// Layer pixels → comp space at the sample time.
    pub matrix: DMat4,
    /// The lights that reach the layer then (as many as at the frame time).
    pub lights: Arc<[Light]>,
    /// Where the layer is seen from then.
    pub eye: DVec3,
}

/// A 3D layer casting shadows (Casts Shadows on, AE-CMP-440).
#[derive(Clone, Debug, PartialEq)]
pub struct ShadowCaster {
    /// Layer pixels → comp space.
    pub matrix: DMat4,
    /// Its drawn rectangle in layer pixels.
    pub rect: Rect,
    /// Layer opacity, 0–1.
    pub opacity: f32,
    /// Light Transmission in percent.
    pub transmission: f32,
    /// What blocks the light.
    pub pixels: CasterPixels,
    /// Advanced 3D Shadow Color (straight RGB; black by default): what the
    /// shadow turns the light into where the caster blocks it.
    pub shadow_color: [f32; 3],
    /// Per light of the receiver it is attached to: where its shadow can
    /// fall on the receiver (receiver layer pixels, the softest edge
    /// included; empty: nowhere). Missing: anywhere.
    pub footprints: Arc<[Rect]>,
}

/// A caster's pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CasterPixels {
    /// A solid (straight RGBA).
    Solid([f32; 4]),
    /// The layer's image (its content after masks and effects).
    Image(NodeId),
}

/// The shadow edge AE draws without diffusion, in receiver pixels (fitted to
/// AE 26.2.1's shadow-map filtering at Comp Size resolution).
pub const SHADOW_SOFTNESS: f64 = 1.0;

/// GPU record of a [`Lighting`] (`struct` layout in `composite.wgsl`, as vec4s):
/// header vec4s, [`LIGHT_VEC4S`] per light, then [`CASTER_VEC4S`] per caster.
pub(crate) const HEADER_VEC4S: usize = 7;
/// Vec4s per light in a lighting record.
pub(crate) const LIGHT_VEC4S: usize = 5;
/// Vec4s per shadow caster in a lighting record.
pub(crate) const CASTER_VEC4S: usize = 9;
/// Reflection block at the end of a record (its offset in the first row's w).
pub(crate) const REFLECTION_VEC4S: usize = 4;

impl Lighting {
    /// Vec4s of the GPU records (one per motion-blur sample when sampled).
    pub(crate) fn record_len(&self) -> usize {
        let one = HEADER_VEC4S
            + LIGHT_VEC4S * self.lights.len()
            + (CASTER_VEC4S + self.lights.len()) * self.casters.len()
            + 1
            + SHADOW_TILES * SHADOW_TILES * self.lights.len()
            + if self.reflection.is_some() {
                REFLECTION_VEC4S
            } else {
                0
            };
        one * self.samples.len().max(1)
    }

    /// Appends the record for input texels `from_texel` (input texel → layer
    /// pixel) to `out`; returns its offset (in vec4s). `pixel_scale` is comp
    /// pixels per target texel; `atlas` maps an image caster to its place in the
    /// shadow atlas: layer px → uv (`x · [0].xy + [0].zw`) and the uv clip
    /// rectangle `[1]` (`None`: the image is missing, the caster is skipped).
    pub(crate) fn pack(
        &self,
        from_texel: &DAffine2,
        pixel_scale: f64,
        atlas: &dyn Fn(NodeId) -> Option<[[f32; 4]; 2]>,
        use_map: bool,
        reflection_levels: u32,
        out: &mut Vec<[f32; 4]>,
    ) -> u32 {
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
        out.push(v4(self.eye, (self.world_per_pixel * pixel_scale) as f32));
        let mat = &self.material;
        out.push([
            mat.ambient / 100.0,
            mat.diffuse / 100.0,
            mat.specular_intensity / 100.0,
            mat.specular_shininess,
        ]);
        // With a shadow map the casters stay out of the record: the draw
        // samples the map (texels axis-aligned with the layer only).
        let slots = self.shadow_slots();
        let (k, [map_w, map_h]) = self.shadow_map_size();
        let map = (use_map
            && self.shadow_map.is_some()
            && from_texel.matrix2.x_axis.y == 0.0
            && from_texel.matrix2.y_axis.x == 0.0)
            .then(|| {
                [
                    (from_texel.matrix2.x_axis.x / k) as f32,
                    ((from_texel.translation.x - self.rect.x0) / k) as f32,
                    (from_texel.matrix2.y_axis.y / k) as f32,
                    ((from_texel.translation.y - self.rect.y0) / k) as f32,
                ]
            });
        // Casters are counted after packing (missing images are skipped).
        let header = out.len();
        // w: Accepts Shadows Only (the layer shows only the shadows on it).
        let shadows_only = f32::from(u8::from(mat.accepts_shadows == ShadowMode::Only));
        out.push([mat.metal / 100.0, 0.0, 0.0, shadows_only]);
        for (li, l) in self.lights.iter().enumerate() {
            let slot = map
                .and_then(|_| slots.iter().position(|&s| s == li))
                .map_or(-1.0, |s| s as f32);
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
                map_h as f32,
            ]);
            let darkness =
                if l.casts_shadows && !(self.casters.is_empty() && self.models.is_empty()) {
                    l.shadow_darkness.clamp(0.0, 100.0) / 100.0
                } else {
                    0.0
                };
            // z: the light's shadow map slot (−1: test the casters), w: the
            // slot width in texels (the height is in the previous row's w).
            out.push([darkness, l.shadow_diffusion.max(0.0), slot, map_w as f32]);
        }
        let mut count = 0.0f32;
        let block = (out.len() as u32 - first) as f32;
        let to_texel = from_texel.inverse();
        let mut footprints: Vec<Vec<[f32; 4]>> = Vec::with_capacity(self.casters.len());
        let casters: &[ShadowCaster] = if map.is_some() { &[] } else { &self.casters };
        for c in casters {
            let (color, kind, map) = match c.pixels {
                CasterPixels::Solid(color) => (color, 0.0, [[0.0; 4]; 2]),
                CasterPixels::Image(id) => match atlas(id) {
                    Some(map) => ([0.0; 4], 1.0, map),
                    None => continue,
                },
            };
            let m = c.matrix;
            let n = m.x_axis.truncate().cross(m.y_axis.truncate());
            let Some(normal) = n.try_normalize() else {
                continue;
            };
            let inv = m.inverse();
            let row = |i: usize| {
                let r = inv.row(i);
                [r.x as f32, r.y as f32, r.z as f32, r.w as f32]
            };
            let offset = -normal.dot(m.w_axis.truncate());
            out.push(v4(normal, offset as f32));
            out.push(row(0));
            out.push(row(1));
            out.push([
                c.rect.x0 as f32,
                c.rect.y0 as f32,
                c.rect.x1 as f32,
                c.rect.y1 as f32,
            ]);
            out.push(color);
            out.push(map[0]);
            out.push(map[1]);
            let px_per_world = 1.0 / m.x_axis.truncate().length().max(1e-12);
            out.push([
                c.opacity.clamp(0.0, 1.0),
                c.transmission.clamp(0.0, 100.0) / 100.0,
                px_per_world as f32,
                kind,
            ]);
            let [r, g, b] = c.shadow_color;
            out.push([r, g, b, 0.0]);
            // Per light: the footprint in input texels (the shader skips the
            // caster outside it).
            let mut feet = Vec::with_capacity(self.lights.len());
            for i in 0..self.lights.len() {
                let bounds = match c.footprints.get(i) {
                    None => [f32::MIN, f32::MIN, f32::MAX, f32::MAX],
                    Some(r) if r.is_empty() => [1.0, 1.0, 0.0, 0.0],
                    Some(r) => {
                        let pts = r.corners().map(|p| to_texel.transform_point2(p));
                        let b = crate::three_d::points_bounds(&pts);
                        [b.x0 as f32, b.y0 as f32, b.x1 as f32, b.y1 as f32]
                    }
                };
                out.push(bounds);
                feet.push(bounds);
            }
            footprints.push(feet);
            count += 1.0;
        }
        out[header][1] = count;
        out[header][2] = block;
        if let Some(xf) = map {
            // In the grid header's place: input texel → slot texel
            // (`x · [0] + [1]`, `y · [2] + [3]`); no masks are read.
            out.push(xf);
            out.extend(std::iter::repeat_n(
                [0.0; 4],
                SHADOW_TILES * SHADOW_TILES * self.lights.len(),
            ));
        } else {
            self.pack_tiles(&to_texel, &footprints, out);
        }
        // Reflections (`reflection_levels`: the picture's mip levels, 0
        // while it is missing): rows of comp → picture directions with the
        // levels, the encode flag, intensity; the light's colour with the
        // sharpness. Its offset goes in the first row's w, the rolloff in the
        // second's.
        if let Some(r) = self.reflection.filter(|_| reflection_levels > 0) {
            let at = out.len() - first as usize;
            out[first as usize][3] = at as f32;
            out[first as usize + 1][3] = r.rolloff;
            let rows = r.environment.rotation;
            let row = |i: usize, w: f32| {
                let v = rows.row(i);
                [v.x as f32, v.y as f32, v.z as f32, w]
            };
            out.push(row(0, reflection_levels as f32));
            out.push(row(1, f32::from(u8::from(!r.linear))));
            out.push(row(2, r.intensity));
            let [cr, cg, cb] = r.environment.tint;
            out.push([cr, cg, cb, r.sharpness]);
        }
        first
    }

    /// The shadow grid after the casters: a header `(x0, y0, tiles / width,
    /// tiles / height)` in input texels (`z = 0`: no grid, every caster is
    /// tested), then per light and tile (row by row) a 128-bit mask of the
    /// casters whose footprint reaches the tile.
    fn pack_tiles(
        &self,
        to_texel: &DAffine2,
        footprints: &[Vec<[f32; 4]>],
        out: &mut Vec<[f32; 4]>,
    ) {
        let pts = self.rect.corners().map(|p| to_texel.transform_point2(p));
        let g = crate::three_d::points_bounds(&pts);
        let (w, h) = (g.x1 - g.x0, g.y1 - g.y0);
        if footprints.len() > SHADOW_TILE_CASTERS || w <= 0.0 || h <= 0.0 {
            out.push([0.0; 4]);
            return;
        }
        let n = SHADOW_TILES as f64;
        out.push([g.x0 as f32, g.y0 as f32, (n / w) as f32, (n / h) as f32]);
        for li in 0..self.lights.len() {
            for ty in 0..SHADOW_TILES {
                for tx in 0..SHADOW_TILES {
                    let x0 = g.x0 + w * tx as f64 / n;
                    let y0 = g.y0 + h * ty as f64 / n;
                    let (x1, y1) = (x0 + w / n, y0 + h / n);
                    let mut mask = [0u32; 4];
                    for (ci, feet) in footprints.iter().enumerate() {
                        let f = feet[li];
                        let f = [
                            f64::from(f[0]),
                            f64::from(f[1]),
                            f64::from(f[2]),
                            f64::from(f[3]),
                        ];
                        // The outer tiles reach past the grid (pixels at its edge).
                        let inside_x =
                            (tx == 0 || f[2] >= x0) && (tx == SHADOW_TILES - 1 || f[0] <= x1);
                        let inside_y =
                            (ty == 0 || f[3] >= y0) && (ty == SHADOW_TILES - 1 || f[1] <= y1);
                        if f[0] <= f[2] && inside_x && inside_y {
                            mask[ci / 32] |= 1 << (ci % 32);
                        }
                    }
                    out.push(mask.map(f32::from_bits));
                }
            }
        }
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
    shade_with_shadows(color, point, normal, eye, material, lights, &[], 0.0)
}

/// [`shade`] with shadows from solid `casters` (image casters are skipped;
/// `world_per_pixel` as [`Lighting::world_per_pixel`]).
#[allow(clippy::too_many_arguments)]
pub fn shade_with_shadows(
    color: [f32; 3],
    point: DVec3,
    normal: DVec3,
    eye: DVec3,
    material: &Material,
    lights: &[Light],
    casters: &[ShadowCaster],
    world_per_pixel: f64,
) -> [f32; 3] {
    let pixel = if world_per_pixel < 0.0 {
        -world_per_pixel
    } else {
        world_per_pixel * (eye - point).length()
    };
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
        let mut k = rgb * atten * spot;
        if l.casts_shadows {
            k *= shadow(point, l, casters, pixel);
        }
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

/// How much of light `l` reaches `point` past solid `casters` (per channel);
/// `pixel` = comp-space size of a receiver pixel there.
fn shadow(point: DVec3, l: &Light, casters: &[ShadowCaster], pixel: f64) -> DVec3 {
    let darkness = f64::from(l.shadow_darkness.clamp(0.0, 100.0)) / 100.0;
    let to = match l.kind {
        LightKind::Parallel => -l.direction() * 1e6,
        _ => l.position() - point,
    };
    let light_dist = (l.position() - point).length().max(1e-9);
    let mut f = DVec3::ONE;
    for c in casters {
        let CasterPixels::Solid(color) = c.pixels else {
            continue;
        };
        let m = c.matrix;
        let Some(n) = m
            .x_axis
            .truncate()
            .cross(m.y_axis.truncate())
            .try_normalize()
        else {
            continue;
        };
        let denom = n.dot(to);
        if denom.abs() < 1e-12 {
            continue;
        }
        let t = n.dot(m.w_axis.truncate() - point) / denom;
        if t <= 1e-6 || t >= 1.0 {
            continue;
        }
        let hit = point + to * t;
        let u = m.inverse().transform_point3(hit).truncate();
        let frac = (t * to.length() / light_dist).clamp(0.0, 1.0);
        // Parallel lights: an orthographic shadow map, the edge does not shrink.
        let shrink = if l.kind == LightKind::Parallel {
            1.0
        } else {
            1.0 - frac
        };
        let world = (f64::from(l.shadow_diffusion) * frac).max(SHADOW_SOFTNESS * pixel * shrink);
        let s = (world / m.x_axis.truncate().length().max(1e-12)).max(1e-6);
        let cover = box_overlap(u, s, &c.rect);
        let a = cover * f64::from(color[3]) * f64::from(c.opacity);
        let tr = f64::from(c.transmission) / 100.0;
        let col = DVec3::new(
            f64::from(color[0]),
            f64::from(color[1]),
            f64::from(color[2]),
        );
        let [r, g, b] = c.shadow_color.map(f64::from);
        // What passes: the transmitted colour, else the shadow colour.
        let pass = tr * col + (1.0 - tr) * DVec3::new(r, g, b);
        f *= DVec3::ONE - darkness * a * (DVec3::ONE - pass);
    }
    f
}

/// Fraction of the square `[u − s, u + s]²` inside `rect`.
fn box_overlap(u: DVec2, s: f64, rect: &Rect) -> f64 {
    let span = |c: f64, lo: f64, hi: f64| ((c + s).min(hi) - (c - s).max(lo)).max(0.0) / (2.0 * s);
    span(u.x, rect.x0, rect.x1) * span(u.y, rect.y0, rect.y1)
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
    /// Advanced 3D: Transparency in percent (default 0): the layer lets what
    /// is behind it through.
    pub transparency: f32,
    /// Advanced 3D: Transparency Rolloff in percent (default 0): less
    /// transparency where the layer is seen at a glancing angle.
    pub transparency_rolloff: f32,
    /// Advanced 3D: Shadow Color (straight RGB, default black).
    pub shadow_color: [f32; 3],
    /// Advanced 3D: Reflection Intensity, Sharpness, Rolloff in percent
    /// (defaults 0, 100, 0).
    pub reflection_intensity: f32,
    pub reflection_sharpness: f32,
    pub reflection_rolloff: f32,
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
            transparency: 0.0,
            transparency_rolloff: 0.0,
            shadow_color: [0.0; 3],
            reflection_intensity: 0.0,
            reflection_sharpness: 100.0,
            reflection_rolloff: 0.0,
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
            environment: None,
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
