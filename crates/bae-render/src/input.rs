//! What the renderer consumes: one composition evaluated at one time.
//!
//! [`RenderComp`] is the renderer's input contract. It is plain data (no GPU types)
//! so that the animation evaluator (workstream 03, `bae-anim`) can produce it, or
//! an adapter from 03's `EvaluatedComp` can fill it, without depending on wgpu.
//! Until 03 lands, [`crate::eval::MockEvaluator`] builds it directly from a
//! [`bae_model::Stage`] (static values + linear keyframes).
//!
//! Everything here is already *evaluated*: transforms are final matrices
//! (anchor, position, scale, rotation and parenting combined), opacity is a
//! number, nested comps are evaluated at their mapped time. The renderer never
//! looks at keyframes or expressions.

use std::any::Any;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use bae_model::{BezierPath, FrameRate, PrimPath, Time, Value};
use glam::{DAffine2, DMat4};
use rustc_hash::FxHasher;

use crate::blend::{BlendMode, MaskMode, TrackMatteMode};
use crate::camera::Camera;
use crate::geom::Rect;
use crate::light::{Light, Material};
use crate::motion::{CompMotionBlur, LayerSample};

/// One composition at one time, ready to render.
#[derive(Clone, Debug)]
pub struct RenderComp {
    /// The `Comp` prim.
    pub path: PrimPath,
    /// Comp time this frame was evaluated at.
    pub time: Time,
    /// Comp width in pixels.
    pub width: u32,
    /// Comp height in pixels.
    pub height: u32,
    /// Pixel aspect ratio (1.0 = square pixels).
    pub pixel_aspect: f64,
    /// Comp frame rate.
    pub frame_rate: FrameRate,
    /// Background colour, straight RGBA in the working space. Not part of the
    /// composite (AE-CMP-010): used by viewers and by outputs without alpha.
    pub background: [f32; 4],
    /// Shutter angle and phase in degrees (motion blur; passed to effects).
    pub shutter: [f64; 2],
    /// Layers, **top first** (AE layer index 1 is `layers[0]`). Includes layers
    /// that are not drawn themselves but are referenced as track mattes.
    pub layers: Vec<RenderLayer>,
    /// The active camera (AE-CMP-423): 3D layers are seen through it. `None` =
    /// AE's default comp camera ([`Camera::default_for`], AE-CMP-403).
    pub camera: Option<Camera>,
    /// Enabled lights (data only for now: 3D layers render unlit, see
    /// [`crate::light`]).
    pub lights: Vec<Light>,
    /// Motion blur samples of this frame (AE-CMP-064); `None` when the comp's
    /// Enable Motion Blur switch is off (or no layer is blurred). Blurred layers
    /// carry their placements in [`RenderLayer::motion_samples`].
    pub motion_blur: Option<Arc<CompMotionBlur>>,
}

impl RenderComp {
    /// An empty comp (no layers, black background, 30 fps).
    pub fn new(path: PrimPath, width: u32, height: u32) -> Self {
        Self {
            path,
            time: Time::ZERO,
            width,
            height,
            pixel_aspect: 1.0,
            frame_rate: FrameRate::FPS_30,
            background: [0.0, 0.0, 0.0, 1.0],
            shutter: [180.0, -90.0],
            layers: Vec::new(),
            camera: None,
            lights: Vec::new(),
            motion_blur: None,
        }
    }

    /// In Photoshop a hidden clipping base hides its clipped layers as well.
    pub(crate) fn photoshop_clipping_visibility(&mut self, stage: &bae_model::Stage) {
        for index in 0..self.layers.len() {
            let layer = &self.layers[index];
            let Some(matte) = layer.track_matte.and_then(|m| self.layers.get(m.layer)) else {
                continue;
            };
            if !matte.visible
                && stage.prim(&layer.path).is_some_and(|p| {
                    p.value("psd:clip_base").and_then(Value::as_prim_ref) == Some(&matte.path)
                })
            {
                self.layers[index].visible = false;
            }
        }
    }

    /// The camera 3D layers are seen through: the active camera or AE's default
    /// camera for this comp size.
    pub fn view_camera(&self) -> Camera {
        self.camera
            .unwrap_or_else(|| Camera::default_for(self.width, self.height))
    }

    /// The camera at motion-blur sample `k` (the frame's camera when the comp
    /// has no samples).
    pub fn sample_camera(&self, k: usize) -> Camera {
        match &self.motion_blur {
            Some(mb) => mb
                .cameras
                .get(k)
                .copied()
                .flatten()
                .unwrap_or_else(|| Camera::default_for(self.width, self.height)),
            None => self.view_camera(),
        }
    }

    /// Comp rectangle in comp pixels.
    pub fn rect(&self) -> Rect {
        Rect::from_size(f64::from(self.width), f64::from(self.height))
    }

    /// Hash of everything that affects the rendered pixels (not the background,
    /// which is not part of the composite). Equal hashes mean equal output, so a
    /// comp that does not change over time hashes the same at every frame (time
    /// itself is not hashed; time-dependent content such as footage frames carries
    /// its own source time or content hash).
    pub fn content_hash(&self) -> u64 {
        let mut h = FxHasher::default();
        self.hash_content(&mut h, None);
        h.finish()
    }

    pub(crate) fn hash_content(&self, h: &mut impl Hasher, effects: Option<&crate::EffectLibrary>) {
        self.path.hash(h);
        self.width.hash(h);
        self.height.hash(h);
        self.pixel_aspect.to_bits().hash(h);
        self.layers.len().hash(h);
        let mut scene_used = false;
        let has_scene = self.camera.is_some() || !self.lights.is_empty();
        let mut any_blur = false;
        for layer in &self.layers {
            layer.hash_composite(h, effects);
            if layer.blend_mode == BlendMode::DancingDissolve {
                self.frame_rate.time_to_frame(self.time).hash(h);
            }
            scene_used |= layer.matrix_3d.is_some()
                || (has_scene
                    && !scene_used
                    && effects.is_some_and(|lib| {
                        layer.effects.iter().filter(|fx| fx.enabled).any(|fx| {
                            lib.get(&fx.effect)
                                .is_some_and(|e| e.uses_scene(&crate::EffectParams::new(fx)))
                        })
                    }));
            any_blur |= !layer.motion_samples.is_empty();
        }
        // An environment light's image can show behind every layer.
        scene_used |= self
            .lights
            .iter()
            .any(|l| l.environment.is_some_and(|e| e.background.is_some()));
        if scene_used {
            hash_camera(&self.camera, h);
            self.lights.len().hash(h);
            for l in &self.lights {
                hash_light(l, h);
            }
            if any_blur && let Some(mb) = &self.motion_blur {
                for c in &mb.cameras {
                    hash_camera(c, h);
                }
                for lights in &mb.lights {
                    lights.len().hash(h);
                    for l in lights.iter() {
                        hash_light(l, h);
                    }
                }
            }
        }
        if any_blur && let Some(mb) = &self.motion_blur {
            mb.adaptive_limit.hash(h);
        }
    }

    /// Every prim this frame was built from, with the time at which it was
    /// sampled in that prim's own time base: the comp at comp time, each layer at
    /// layer time, nested comps at their mapped time, footage at source time. The
    /// frame cache uses this for precise invalidation from
    /// [`bae_model::StageEvent`]s (see [`crate::cache`]).
    pub fn dependencies(&self) -> Vec<Dependency> {
        let mut out = Vec::new();
        self.collect_dependencies(&mut out);
        out
    }

    fn collect_dependencies(&self, out: &mut Vec<Dependency>) {
        out.push(Dependency {
            path: self.path.clone(),
            time: self.time,
        });
        for layer in &self.layers {
            if layer.inert {
                out.push(Dependency {
                    path: layer.path.clone(),
                    time: Dependency::INACTIVE,
                });
                continue;
            }
            out.push(Dependency {
                path: layer.path.clone(),
                time: layer.time,
            });
            match &layer.content {
                LayerContent::Comp(nested) => nested.collect_dependencies(out),
                LayerContent::Footage(f) => out.push(Dependency {
                    path: f.asset.clone(),
                    time: f.source_time,
                }),
                LayerContent::Model(m) => out.push(Dependency {
                    path: m.asset.clone(),
                    time: Time::ZERO,
                }),
                LayerContent::Custom(c) if c.prim != layer.path => out.push(Dependency {
                    path: c.prim.clone(),
                    time: layer.time,
                }),
                _ => {}
            }
        }
    }
}

/// A prim a rendered frame depends on, and the time it was sampled at.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Dependency {
    /// The prim (its whole subtree counts unless a deeper dependency covers it).
    pub path: PrimPath,
    /// The time the prim was evaluated at, in the prim's own time base, or
    /// [`Dependency::INACTIVE`].
    pub time: Time,
}

impl Dependency {
    /// The time of a layer that does not show in the frame
    /// ([`RenderLayer::inert`]): only changes to its timing and visibility
    /// switches ([`Dependency::SWITCHES`]) can affect the frame.
    pub const INACTIVE: Time = Time::MIN;

    /// The layer attributes that can make an inert layer show.
    pub const SWITCHES: &[&str] = &[
        "in_point",
        "out_point",
        "start_time",
        "stretch",
        "time_remap_enabled",
        "time_remap",
        "enabled",
        "solo",
        "guide",
    ];
}

/// One evaluated layer.
#[derive(Clone, Debug)]
pub struct RenderLayer {
    /// The `Layer` prim.
    pub path: PrimPath,
    /// Layer time of this frame: `(comp time − start_time) × 100 / stretch`. The
    /// layer's own keyframes (and those of its masks, effects and contents) are in
    /// layer time (data-model.md §10), so cache invalidation uses it.
    pub time: Time,
    /// Composition time corresponding to zero on the layer clock.
    pub start_time: Time,
    /// Earliest comp time at which this layer is active.
    pub in_point: Time,
    /// Comp-time seconds per layer-time second (AE time stretch).
    pub time_scale: f64,
    /// What the layer draws.
    pub content: LayerContent,
    /// Layer space → comp space (pixels). Combines anchor point, position, scale,
    /// rotation and the parent chain: `T(position) · R · S · T(-anchor)`.
    pub transform: DAffine2,
    /// Opacity in `[0, 1]`.
    pub opacity: f32,
    /// Blending mode with the layers below.
    pub blend_mode: BlendMode,
    /// Layer sampling quality after any nested quality limit.
    pub quality: bae_anim::LayerQuality,
    /// Advanced 3D Extrusion Depth of a 3D text or shape layer, in layer
    /// pixels (0: flat): drawn as a solid ([`crate::model::extrude`]).
    pub extrusion: f64,
    /// Its bevel (Geometry Options), when extruded.
    pub bevel: Option<crate::model::extrude::Bevel>,
    /// 保留基础透明度: the layer only shows where the layers below are opaque.
    pub preserve_transparency: bool,
    /// Track matte (AE 23.0+ model: any layer of the same comp).
    pub track_matte: Option<TrackMatte>,
    /// Video switch (eye). A layer used as a matte usually has it off, but is
    /// still rendered as the matte.
    pub visible: bool,
    /// The comp time is within the layer's in/out range (and solo does not
    /// exclude it). An inactive layer draws nothing, also not as a matte.
    pub active: bool,
    /// Nothing of the layer can show in this frame: not active and visible,
    /// and neither a parent nor an active layer's track matte. The frame then
    /// depends only on the switches that could make it show
    /// ([`Dependency::INACTIVE`]), not on its content.
    pub inert: bool,
    /// Adjustment layer: not drawn itself; its effects apply to the layers
    /// below, mixed back by its own pixels' coverage (see `graph`).
    pub adjustment: bool,
    /// Masks, top first (applied to the source before effects, AE-CMP-038).
    pub masks: Vec<RenderMask>,
    /// Effects in stack order (applied through the effect registry).
    pub effects: Vec<RenderEffect>,
    /// Photoshop styles applied in composition space before the matte and blend.
    pub layer_styles: Option<RenderEffect>,
    /// 3D layer (AE-CMP-400): layer space → comp space as a 4×4 matrix (anchor,
    /// scale, X/Y/Z rotation, orientation, position, parents). The layer is a
    /// plane seen through [`RenderComp::camera`] and depth sorted with the
    /// adjacent 3D layers. `None` for 2D layers, which use `transform`.
    pub matrix_3d: Option<DMat4>,
    /// Motion blur (AE-CMP-064): the layer's placement at each of the comp's
    /// sample times ([`RenderComp::motion_blur`]); empty when the layer is not
    /// blurred (its switch or the comp's is off).
    pub motion_samples: Arc<[LayerSample]>,
    /// Material Options of 3D layers (data only for now).
    pub material: Material,
}

impl RenderLayer {
    pub(crate) fn resolve_photoshop_group(
        &mut self,
        stage: &bae_model::Stage,
        assets: &crate::assets::AssetFiles,
    ) {
        let LayerContent::Comp(nested) = &mut self.content else {
            return;
        };
        let Some(index) = self.masks.iter().position(|m| {
            stage
                .prim(&m.path)
                .is_some_and(|p| p.value("psd:channel_mask") == Some(&Value::Bool(true)))
        }) else {
            return;
        };
        let mask = self.masks.remove(index);
        if mask.mode == MaskMode::None {
            return;
        }
        let Some(view) = stage.prim(&mask.path) else {
            return;
        };
        let Some(mut selection) = bae_model::psd::LayerImage::from_asset(&view) else {
            return;
        };
        let Some(Value::Vec2(origin)) = view.value("psd:origin") else {
            return;
        };
        let size = [nested.width, nested.height];
        selection.mask = bae_model::psd::PixelMask::Group {
            origin: origin.map(|v| v as i32),
            size,
            inverted: mask.inverted ^ (mask.mode == MaskMode::Subtract),
            density: (mask.opacity.clamp(0., 1.) * 65535.).round() as u16,
        };
        let footage = FootageRef {
            asset: mask.path.clone(),
            file: assets.file(&view).ok().flatten(),
            size: Some(size),
            source_time: Time::ZERO,
            sequence: None,
            alphabetical: false,
            file_revision: bae_model::asset::file_revision(&view),
            psd_layer: Some(selection),
            blend: None,
            scaled: false,
        };
        let mut stencil = Self::new(mask.path, LayerContent::Footage(footage));
        stencil.blend_mode = BlendMode::StencilAlpha;
        let nested = Arc::make_mut(nested);
        for layer in &mut nested.layers {
            if let Some(matte) = &mut layer.track_matte {
                matte.layer += 1;
            }
        }
        nested.layers.insert(0, stencil);
    }
    /// A Photoshop pixel mask samples the source channel. Timeline mask controls
    /// edit that channel without tracing soft edges into a polygon.
    pub fn resolve_photoshop_mask(&mut self, stage: &bae_model::Stage) {
        let LayerContent::Footage(f) = &mut self.content else {
            return;
        };
        let Some(sel) = &mut f.psd_layer else { return };
        if !stage
            .prim(&self.path)
            .is_some_and(|p| p.value("psd:separate_vectors") == Some(&Value::Bool(true)))
            && let Ok(folder) = f.asset.child("masks")
        {
            for path in stage.children(&folder) {
                let Some(mask) = stage.prim(&path) else {
                    continue;
                };
                let Some(Value::Bezier(shape)) = mask.value("path") else {
                    continue;
                };
                self.masks.push(RenderMask {
                    path: path.clone(),
                    shape: shape.clone(),
                    mode: mask
                        .value("mode")
                        .and_then(Value::as_str)
                        .and_then(MaskMode::from_token)
                        .unwrap_or_default(),
                    inverted: mask
                        .value("inverted")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    opacity: 1.,
                    feather: [0.; 2],
                    expansion: 0.,
                });
            }
        }
        if !stage
            .prim(&self.path)
            .is_some_and(|p| p.value("psd:separate_mask") == Some(&Value::Bool(true)))
        {
            return;
        }
        use bae_model::psd::PixelMask;
        sel.mask = PixelMask::Disabled;
        self.masks.retain(|m| {
            if !stage
                .prim(&m.path)
                .is_some_and(|p| p.value("psd:channel_mask") == Some(&Value::Bool(true)))
            {
                return true;
            }
            if m.mode != MaskMode::None {
                sel.mask = PixelMask::Channel {
                    inverted: m.inverted ^ (m.mode == MaskMode::Subtract),
                    density: (m.opacity.clamp(0., 1.) * 65535.).round() as u16,
                };
            }
            false
        });
    }
    /// A visible, active, Normal layer with identity transform and full opacity.
    pub fn new(path: PrimPath, content: LayerContent) -> Self {
        Self {
            path,
            time: Time::ZERO,
            start_time: Time::ZERO,
            time_scale: 1.0,
            in_point: Time::ZERO,
            content,
            transform: DAffine2::IDENTITY,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            quality: bae_anim::LayerQuality::Best,
            extrusion: 0.0,
            bevel: None,
            preserve_transparency: false,
            track_matte: None,
            visible: true,
            active: true,
            inert: false,
            adjustment: false,
            masks: Vec::new(),
            effects: Vec::new(),
            layer_styles: None,
            matrix_3d: None,
            motion_samples: Arc::new([]),
            material: Material::default(),
        }
    }

    /// Hash of the layer's own pixels in layer space (content, masks, effects),
    /// independent of transform, opacity and blending.
    pub fn content_hash(&self) -> u64 {
        let mut h = FxHasher::default();
        self.hash_content(&mut h, None);
        h.finish()
    }

    fn hash_content(&self, h: &mut impl Hasher, effects: Option<&crate::EffectLibrary>) {
        self.in_point.hash(h);
        self.content.hash_into(h, effects);
        self.masks.len().hash(h);
        for m in &self.masks {
            m.hash_into(h);
        }
        if let Some(styles) = &self.layer_styles {
            styles.hash_into(h);
        }
        self.effects.len().hash(h);
        for e in &self.effects {
            e.hash_into(h);
        }
    }

    pub(crate) fn hash_composite(
        &self,
        h: &mut impl Hasher,
        effects: Option<&crate::EffectLibrary>,
    ) {
        self.path.hash(h);
        self.hash_content(h, effects);
        for v in self.transform.to_cols_array() {
            v.to_bits().hash(h);
        }
        self.opacity.to_bits().hash(h);
        self.blend_mode.hash(h);
        self.quality.hash(h);
        self.extrusion.to_bits().hash(h);
        if let Some(b) = &self.bevel {
            (b.style, b.inward).hash(h);
            [b.depth, b.hole_scale].map(f64::to_bits).hash(h);
        }
        self.preserve_transparency.hash(h);
        self.track_matte.hash(h);
        self.visible.hash(h);
        self.active.hash(h);
        self.adjustment.hash(h);
        if let Some(m) = &self.matrix_3d {
            hash_mat4(m, h);
            let mat = &self.material;
            (
                mat.casts_shadows,
                mat.accepts_shadows,
                mat.accepts_lights,
                [
                    mat.light_transmission,
                    mat.ambient,
                    mat.diffuse,
                    mat.specular_intensity,
                    mat.specular_shininess,
                    mat.metal,
                    mat.transparency,
                    mat.transparency_rolloff,
                    mat.reflection_intensity,
                    mat.reflection_sharpness,
                    mat.reflection_rolloff,
                ]
                .map(f32::to_bits),
                mat.shadow_color.map(f32::to_bits),
            )
                .hash(h);
        }
        self.motion_samples.len().hash(h);
        for s in self.motion_samples.iter() {
            for v in s.transform.to_cols_array() {
                v.to_bits().hash(h);
            }
            if let Some(m) = &s.matrix_3d {
                hash_mat4(m, h);
            }
        }
    }
}

fn hash_mat4(m: &DMat4, h: &mut impl Hasher) {
    for v in m.to_cols_array() {
        v.to_bits().hash(h);
    }
}

pub(crate) fn hash_light(l: &Light, h: &mut impl Hasher) {
    (l.kind, l.falloff, l.casts_shadows, l.scope_below).hash(h);
    if let Some(e) = &l.environment {
        e.layer.hash(h);
        e.background.map(|b| b.map(f32::to_bits)).hash(h);
    }
    hash_mat4(&l.world, h);
    l.point_of_interest.to_array().map(f64::to_bits).hash(h);
    l.color.map(f32::to_bits).hash(h);
    [
        l.intensity,
        l.cone_angle,
        l.cone_feather,
        l.radius,
        l.falloff_distance,
        l.shadow_darkness,
        l.shadow_diffusion,
    ]
    .map(f32::to_bits)
    .hash(h);
}

pub(crate) fn hash_camera(c: &Option<Camera>, h: &mut impl Hasher) {
    c.is_some().hash(h);
    if let Some(c) = c {
        hash_mat4(&c.world, h);
        c.zoom.to_bits().hash(h);
        if let crate::camera::Projection::Orthographic { scale } = c.projection {
            scale.to_bits().hash(h);
        }
        // Depth of field changes the pixels of 3D layers.
        c.depth_of_field.hash(h);
        if c.depth_of_field {
            let i = &c.iris;
            for v in [
                c.focus_distance,
                c.aperture,
                c.blur_level,
                i.rotation,
                i.roundness,
                i.aspect_ratio,
                i.diffraction_fringe,
                i.highlight_gain,
                i.highlight_threshold,
                i.highlight_saturation,
            ] {
                v.to_bits().hash(h);
            }
            i.shape.hash(h);
        }
    }
}

/// A layer's track matte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TrackMatte {
    /// Index of the matte layer in [`RenderComp::layers`].
    pub layer: usize,
    /// Alpha / luma, normal / inverted.
    pub mode: TrackMatteMode,
}

/// What a layer draws, in layer space (origin top-left, pixels).
#[derive(Clone, Debug, Default)]
pub enum LayerContent {
    /// Nothing (null, camera, light, audio, or a layer without a source).
    #[default]
    None,
    /// A solid colour rectangle `[0, width) × [0, height)`.
    Solid {
        /// Straight RGBA in the working space.
        color: [f32; 4],
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
    },
    /// A still image or a frame of a footage item.
    Footage(FootageRef),
    /// A nested composition, already evaluated at the mapped time.
    Comp(Arc<RenderComp>),
    /// Content drawn by a registered [`crate::source::SourceProvider`] (shapes and
    /// text from 05, plugin generators …).
    Custom(CustomContent),
    /// A 3D model (glTF / GLB / OBJ, [`crate::model`]), drawn through the
    /// camera with a depth buffer.
    Model(ModelRef),
}

/// A 3D model layer's source.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ModelRef {
    /// The `Asset` prim.
    pub asset: PrimPath,
    /// The resolved file (`None` when missing; renders nothing and warns).
    pub file: Option<PathBuf>,
}

impl LayerContent {
    /// Size of the content in layer pixels, when known without decoding.
    pub fn known_size(&self) -> Option<[u32; 2]> {
        match self {
            LayerContent::None => None,
            LayerContent::Solid { width, height, .. } => Some([*width, *height]),
            LayerContent::Footage(f) => f.size,
            LayerContent::Comp(c) => Some([c.width, c.height]),
            LayerContent::Custom(c) => c.size,
            LayerContent::Model(_) => None,
        }
    }

    fn hash_into(&self, h: &mut impl Hasher, effects: Option<&crate::EffectLibrary>) {
        std::mem::discriminant(self).hash(h);
        match self {
            LayerContent::None => {}
            LayerContent::Solid {
                color,
                width,
                height,
            } => {
                color.map(f32::to_bits).hash(h);
                width.hash(h);
                height.hash(h);
            }
            LayerContent::Footage(f) => {
                f.asset.hash(h);
                f.file.hash(h);
                f.source_time.hash(h);
                f.sequence.hash(h);
                f.psd_layer.hash(h);
                f.file_revision.hash(h);
                f.blend.hash(h);
                if f.scaled {
                    f.size.hash(h);
                }
            }
            LayerContent::Comp(c) => c.hash_content(h, effects),
            LayerContent::Custom(c) => {
                c.provider.hash(h);
                c.prim.hash(h);
                c.hash.hash(h);
            }
            LayerContent::Model(m) => m.hash(h),
        }
    }
}

/// A footage frame (image file, image sequence or video frame).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FootageRef {
    /// The `Asset` prim.
    pub asset: PrimPath,
    /// The resolved file (`None` when missing; renders nothing and warns).
    pub file: Option<PathBuf>,
    /// Pixel size if the evaluator knows it (from asset metadata); otherwise the
    /// renderer reads it from the file.
    pub size: Option<[u32; 2]>,
    /// Time inside the footage (after time remapping / stretch).
    pub source_time: Time,
    /// Image sequences: the frame rate of the sequence (its numbered files are
    /// frames from `file` on); `None` opens `file` alone (a still or a movie).
    pub sequence: Option<FrameRate>,
    /// Sequence uses actual filenames sorted alphabetically.
    pub alphabetical: bool,
    pub file_revision: i64,
    pub psd_layer: Option<bae_model::psd::LayerImage>,
    /// Frame blending, when the comp and layer switches are on.
    pub blend: Option<FrameBlend>,
    /// `file` is a proxy (AE File ▸ Set Proxy): its frames are drawn at
    /// `size`, the item's own size, whatever the proxy's resolution.
    pub scaled: bool,
}

/// Frame blending of a footage layer (AE-CMP-065): the source frames one comp
/// frame covers are mixed with AE's Frame Mix weights.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FrameBlend {
    /// The source time one comp frame spans (playback speed × frame duration).
    pub span: Time,
    /// Pixel Motion (optical flow) was asked for. It renders as Frame Mix with a
    /// warning for now; a flow pass would replace the mix with a warp of the two
    /// nearest frames (the same flow as the Pixel Motion Blur effect).
    pub pixel_motion: bool,
}

/// Content drawn by a [`crate::source::SourceProvider`].
#[derive(Clone)]
pub struct CustomContent {
    /// Provider ID, e.g. `bae.vector` or `com.example.particles`.
    pub provider: String,
    /// The prim the content comes from (usually the layer).
    pub prim: PrimPath,
    /// Layer-space size if known up front.
    pub size: Option<[u32; 2]>,
    /// Hash of every input that affects the pixels (the provider's cache key).
    pub hash: u64,
    /// Provider-specific evaluated data.
    pub data: Arc<dyn Any + Send + Sync>,
}

impl fmt::Debug for CustomContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CustomContent")
            .field("provider", &self.provider)
            .field("prim", &self.prim)
            .field("size", &self.size)
            .field("hash", &self.hash)
            .finish_non_exhaustive()
    }
}

/// An evaluated mask (rendered by [`crate::mask`]).
#[derive(Clone, Debug, PartialEq)]
pub struct RenderMask {
    /// The `Mask` prim.
    pub path: PrimPath,
    /// Mask path in layer pixels.
    pub shape: BezierPath,
    /// Combination with the masks above.
    pub mode: MaskMode,
    /// 反转
    pub inverted: bool,
    /// Mask opacity `[0, 1]`.
    pub opacity: f32,
    /// Feather radius (x, y) in pixels.
    pub feather: [f32; 2],
    /// Expansion in pixels (negative contracts).
    pub expansion: f32,
}

impl RenderMask {
    pub(crate) fn hash_into(&self, h: &mut impl Hasher) {
        self.path.hash(h);
        hash_bezier(&self.shape, h);
        self.mode.hash(h);
        self.inverted.hash(h);
        self.opacity.to_bits().hash(h);
        self.feather.map(f32::to_bits).hash(h);
        self.expansion.to_bits().hash(h);
    }
}

/// An evaluated effect instance.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderEffect {
    /// The `Effect` prim.
    pub path: PrimPath,
    /// Effect ID (the AE match name for AE effects, e.g. `ADBE Gaussian Blur 2`).
    pub effect: String,
    /// Effect switch (fx).
    pub enabled: bool,
    /// Parameter values at this time by parameter ID (`param:0001` → 1, the AE
    /// parameter matchName suffix; see `bae_model::effect`).
    pub params: Vec<(u32, Value)>,
}

impl RenderEffect {
    /// The value of parameter `id`.
    pub fn param(&self, id: u32) -> Option<&Value> {
        self.params.iter().find(|(n, _)| *n == id).map(|(_, v)| v)
    }

    pub(crate) fn hash_into(&self, h: &mut impl Hasher) {
        self.path.hash(h);
        self.effect.hash(h);
        self.enabled.hash(h);
        for (id, value) in &self.params {
            id.hash(h);
            hash_value(value, h);
        }
    }
}

/// Hashes a [`Value`] (floats by bit pattern).
pub fn hash_value(value: &Value, h: &mut impl Hasher) {
    std::mem::discriminant(value).hash(h);
    match value {
        Value::Float(v) => v.to_bits().hash(h),
        Value::Int(v) => v.hash(h),
        Value::Bool(v) => v.hash(h),
        Value::String(v) | Value::Enum(v) => v.hash(h),
        Value::Vec2(v) => v.map(f64::to_bits).hash(h),
        Value::Vec3(v) => v.map(f64::to_bits).hash(h),
        Value::Color(v) => v.map(f64::to_bits).hash(h),
        Value::Bezier(b) => hash_bezier(b, h),
        Value::AssetRef(p) | Value::PrimRef(p) => p.hash(h),
        Value::Bytes(b) => b.hash(h),
        Value::FloatList(l) => {
            l.len().hash(h);
            for v in l {
                v.to_bits().hash(h);
            }
        }
        Value::Time(t) => t.hash(h),
        Value::FrameRate(r) => r.hash(h),
        // Rare and large: hash the canonical JSON.
        Value::TextDocument(doc) => serde_json::to_vec(doc).unwrap_or_default().hash(h),
    }
}

fn hash_bezier(b: &BezierPath, h: &mut impl Hasher) {
    b.closed.hash(h);
    for list in [&b.vertices, &b.in_tangents, &b.out_tangents] {
        list.len().hash(h);
        for p in list.iter() {
            p.map(f64::to_bits).hash(h);
        }
    }
    b.feather.len().hash(h);
    for f in &b.feather {
        f.segment.hash(h);
        [f.t, f.radius, f.tension, f.corner_angle]
            .map(f64::to_bits)
            .hash(h);
        f.hold.hash(h);
        f.inner.hash(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PrimPath {
        PrimPath::new(s).unwrap()
    }

    fn solid(path: &str, color: [f32; 4]) -> RenderLayer {
        RenderLayer::new(
            p(path),
            LayerContent::Solid {
                color,
                width: 100,
                height: 100,
            },
        )
    }

    #[test]
    fn hashes_track_relevant_changes() {
        let mut comp = RenderComp::new(p("/comps/main"), 1920, 1080);
        comp.layers
            .push(solid("/comps/main/layers/a", [1.0, 0.0, 0.0, 1.0]));
        let base = comp.content_hash();
        let content = comp.layers[0].content_hash();

        comp.background = [1.0; 4];
        assert_eq!(
            comp.content_hash(),
            base,
            "background is not part of the composite"
        );

        comp.layers[0].opacity = 0.5;
        assert_ne!(comp.content_hash(), base);
        assert_eq!(
            comp.layers[0].content_hash(),
            content,
            "opacity is not layer content"
        );

        comp.layers[0].content = LayerContent::Solid {
            color: [0.0, 1.0, 0.0, 1.0],
            width: 100,
            height: 100,
        };
        assert_ne!(comp.layers[0].content_hash(), content);
    }

    #[test]
    fn dependencies_cover_nested_comps_and_footage() {
        let mut inner = RenderComp::new(p("/comps/inner"), 100, 100);
        inner.time = Time::from_flicks(5);
        let mut comp = RenderComp::new(p("/comps/main"), 1920, 1080);
        comp.time = Time::from_flicks(10);
        comp.layers.push(RenderLayer::new(
            p("/comps/main/layers/pre"),
            LayerContent::Comp(Arc::new(inner)),
        ));
        comp.layers.push(RenderLayer::new(
            p("/comps/main/layers/img"),
            LayerContent::Footage(FootageRef {
                asset: p("/assets/img"),
                file: None,
                size: None,
                source_time: Time::ZERO,
                sequence: None,
                alphabetical: false,
                psd_layer: None,
                file_revision: 0,
                blend: None,
                scaled: false,
            }),
        ));
        let deps = comp.dependencies();
        assert!(deps.contains(&Dependency {
            path: p("/comps/main"),
            time: Time::from_flicks(10)
        }));
        assert!(deps.contains(&Dependency {
            path: p("/comps/inner"),
            time: Time::from_flicks(5)
        }));
        assert!(deps.contains(&Dependency {
            path: p("/assets/img"),
            time: Time::ZERO
        }));
        // main, its two layers, inner, and the footage asset.
        assert_eq!(deps.len(), 5);
    }
}
