//! Adapter from workstream 03's evaluator (`bae-anim`) to the renderer's input.
//!
//! `bae_anim::Evaluator` does the animation work (AE easing, spatial paths,
//! parenting, layer time, time remapping, later expressions); this module maps
//! its [`EvaluatedComp`] onto [`RenderComp`]: world matrices become 2D affines
//! (and stay 4×4 for 3D layers), the active camera and enabled lights are
//! collected, motion-blurred layers are sampled at 03's shutter times, sources
//! are resolved (solid assets, footage files; nested comps arrive already
//! evaluated at their mapped time), tokens become enums.

use std::path::PathBuf;
use std::sync::Arc;

use std::hash::{Hash, Hasher};

use bae_anim::{
    CameraState, EvaluatedComp, EvaluatedLayer, EvaluatedPrim, Evaluator, FrameBlending,
    LayerSource, LightState,
};
use bae_model::{LayerKind, PrimPath, Stage, StageEvent, Time};
use glam::{DAffine2, DMat4, DVec2};
use rustc_hash::FxHasher;

use crate::assets::AssetFiles;
use crate::blend::{BlendMode, MaskMode, TrackMatteMode};
use crate::camera::Camera;
use crate::error::RenderError;
use crate::eval::CompEvaluator;
use crate::graph::MAX_DEPTH;
use crate::input::{
    CustomContent, FootageRef, FrameBlend, LayerContent, RenderComp, RenderEffect, RenderLayer,
    RenderMask, TrackMatte, hash_value,
};
use crate::light::{Falloff, Light, LightKind, Material, ShadowMode};
use crate::motion::{CompMotionBlur, LayerSample};
use crate::request::RenderPurpose;

/// [`CompEvaluator`] backed by `bae_anim::Evaluator` (keeps compiled comps across
/// frames; forward [`StageEvent`]s to [`CompEvaluator::invalidate`]).
#[derive(Debug)]
pub struct AnimEvaluator {
    inner: Evaluator,
    scalar_tracks: crate::parameter_track::TrackCache,
    assets: AssetFiles,
    purpose: RenderPurpose,
    warnings: Vec<String>,
    /// Shape and text layers' content of the previous conversion, by layer:
    /// reused while what the provider draws is unchanged, so a layer that only
    /// moves costs no deep clone, hash or JSON per frame. Swept after each
    /// top-level conversion.
    customs: rustc_hash::FxHashMap<PrimPath, (CustomContent, bool)>,
}

impl AnimEvaluator {
    /// An evaluator whose footage paths resolve against `base_dir` (the
    /// project's directory).
    pub fn new(base_dir: Option<PathBuf>) -> Self {
        Self {
            inner: Evaluator::new(),
            scalar_tracks: Default::default(),
            assets: AssetFiles::new(base_dir),
            purpose: RenderPurpose::Final,
            warnings: Vec::new(),
            customs: rustc_hash::FxHashMap::default(),
        }
    }

    /// An evaluator for an open project: footage is found like `bae_package`
    /// does, including assets embedded in the `.baez` it was opened from.
    pub fn for_project(project: &bae_model::Project) -> Result<Self, RenderError> {
        Ok(Self::new(None).with_assets(AssetFiles::for_project(project)?))
    }

    /// Uses `assets` to find footage files.
    #[must_use]
    pub fn with_assets(mut self, assets: AssetFiles) -> Self {
        self.assets = assets;
        self
    }

    /// Preview or final (guide layers render only in a top-level preview).
    #[must_use]
    pub fn with_purpose(mut self, purpose: RenderPurpose) -> Self {
        self.purpose = purpose;
        self
    }

    /// Drops compiled animation when the caller observed edits without individual stage events.
    pub fn clear_compiled(&mut self) {
        self.inner.clear();
    }

    /// Evaluates with uncommitted values (a value drag in a panel):
    /// [`bae_anim::Evaluator::set_overrides`]. Motion blur samples and effects
    /// reading other frames see them too.
    pub fn set_overrides(&mut self, overrides: Vec<bae_anim::AttrOverride>) {
        self.inner.set_overrides(overrides);
    }

    fn warn(&mut self, message: String) {
        if !self.warnings.contains(&message) {
            self.warnings.push(message);
        }
    }

    fn eval(
        &mut self,
        stage: &Stage,
        comp: &PrimPath,
        time: Time,
        depth: usize,
    ) -> Result<RenderComp, RenderError> {
        if depth > MAX_DEPTH {
            return Err(RenderError::Cycle {
                path: comp.clone(),
                max: MAX_DEPTH,
            });
        }
        let ec = self
            .inner
            .evaluate_comp(stage, comp, time)
            .map_err(|e| RenderError::Other(format!("{comp}: {e}")))?;
        self.convert(stage, &ec, depth)
    }

    /// Converts one evaluated comp (nested comps are evaluated on demand).
    pub fn convert(
        &mut self,
        stage: &Stage,
        ec: &EvaluatedComp,
        depth: usize,
    ) -> Result<RenderComp, RenderError> {
        for e in &ec.errors {
            self.warn(format!("{}.{}: {}", e.prim, e.attr, e.message));
        }
        let mut rc = RenderComp {
            path: ec.path.clone(),
            time: ec.time,
            width: ec.width,
            height: ec.height,
            pixel_aspect: ec.pixel_aspect,
            frame_rate: ec.frame_rate,
            background: ec.bg_color.map(|v| v as f32),
            shutter: [ec.motion_blur.shutter_angle, ec.motion_blur.shutter_phase],
            layers: Vec::with_capacity(ec.layers.len()),
            camera: active_camera(ec),
            lights: comp_lights(ec).to_vec(),
            motion_blur: None,
        };
        let mut samples = match self.motion_samples(stage, ec)? {
            Some((mb, samples)) => {
                rc.motion_blur = Some(Arc::new(mb));
                samples
            }
            None => Vec::new(),
        };
        for (i, layer) in ec.layers.iter().enumerate() {
            let mut out = self.layer(stage, ec, layer, depth)?;
            if let Some(s) = samples.get_mut(i) {
                out.motion_samples = std::mem::take(s).into();
            }
            rc.layers.push(out);
        }
        // Layers that cannot show in this frame (outside their in/out points,
        // hidden, not a parent or an active layer's matte): the frame does not
        // depend on their content (cache invalidation, `RenderLayer::inert`).
        let parents: Vec<usize> = ec.layers.iter().filter_map(|l| l.parent).collect();
        let mattes: Vec<usize> = rc
            .layers
            .iter()
            .filter(|l| l.active)
            .filter_map(|l| l.track_matte.map(|m| m.layer))
            .collect();
        for (i, l) in rc.layers.iter_mut().enumerate() {
            l.inert = !(l.active && l.visible) && !parents.contains(&i) && !mattes.contains(&i);
        }
        if depth == 0 {
            self.customs.retain(|_, (_, used)| std::mem::take(used));
        }
        Ok(rc)
    }

    /// The comp evaluated at its motion-blur sample times (AE-ANI-103): 03 puts
    /// the times on every blurred layer (both switches on); all layers of a comp
    /// share them. Returns the comp's samples and, per layer, its placement at
    /// each time (empty for layers that are not blurred).
    #[allow(clippy::type_complexity)]
    fn motion_samples(
        &mut self,
        stage: &Stage,
        ec: &EvaluatedComp,
    ) -> Result<Option<(CompMotionBlur, Vec<Vec<LayerSample>>)>, RenderError> {
        let blurred = |l: &EvaluatedLayer| {
            l.active
                && !l.motion_blur_times.is_empty()
                && !matches!(
                    l.kind,
                    LayerKind::Camera | LayerKind::Light | LayerKind::Audio
                )
        };
        let Some(times) = ec
            .layers
            .iter()
            .find(|l| blurred(l))
            .map(|l| l.motion_blur_times.clone())
        else {
            return Ok(None);
        };
        let mut layers: Vec<Vec<LayerSample>> = vec![Vec::new(); ec.layers.len()];
        let mut cameras = Vec::with_capacity(times.len());
        let mut lights = Vec::with_capacity(times.len());
        for &t in &times {
            // Transform-only sampling would be cheaper; 03 evaluates whole comps.
            let at = self
                .inner
                .evaluate_comp(stage, &ec.path, t)
                .map_err(|e| RenderError::Other(format!("{}: {e}", ec.path)))?;
            if at.layers.len() != ec.layers.len() {
                self.warn(format!(
                    "{}: layers changed between motion-blur samples",
                    ec.path
                ));
                return Ok(None);
            }
            cameras.push(active_camera(&at));
            lights.push(comp_lights(&at));
            for (i, l) in ec.layers.iter().enumerate() {
                if blurred(l) {
                    let s = &at.layers[i];
                    layers[i].push(LayerSample {
                        transform: affine_2d(&s.world_matrix),
                        matrix_3d: matrix_3d(s),
                    });
                }
            }
        }
        Ok(Some((
            CompMotionBlur {
                times,
                cameras,
                lights,
                adaptive_limit: ec.motion_blur.adaptive_limit,
            },
            layers,
        )))
    }

    fn layer(
        &mut self,
        stage: &Stage,
        ec: &EvaluatedComp,
        l: &EvaluatedLayer,
        depth: usize,
    ) -> Result<RenderLayer, RenderError> {
        let guide_ok = !l.switches.guide || (depth == 0 && self.purpose == RenderPurpose::Preview);
        let mut out = RenderLayer::new(l.path.clone(), LayerContent::None);
        out.time = l.layer_time;
        out.start_time = l.timing.start_time;
        out.time_scale = l.timing.stretch_percent() / 100.;
        out.in_point = l.timing.in_point.min(l.timing.out_point);
        out.active = l.active && guide_ok;
        out.visible = l.visible;
        out.adjustment = l.switches.adjustment || l.kind == LayerKind::Adjustment;
        out.transform = affine_2d(&l.world_matrix);
        out.matrix_3d = matrix_3d(l);
        if out.matrix_3d.is_some()
            && let Some(m) = &l.material
        {
            out.material = material(m);
        }
        out.opacity = l.opacity.clamp(0.0, 1.0) as f32;
        out.blend_mode = BlendMode::from_token(&l.blend_mode).unwrap_or_else(|| {
            self.warn(format!("{}: unknown blend mode `{}`", l.path, l.blend_mode));
            BlendMode::Normal
        });
        out.preserve_transparency = l.switches.preserve_transparency;
        out.track_matte = matte_mode(l.track_matte)
            .and_then(|mode| l.track_matte_layer.map(|layer| TrackMatte { layer, mode }));
        if out.active {
            out.content = self.content(stage, ec, l, depth)?;
            // A mask a text layer uses as its path does not mask (AE).
            let text_path = l.text_path.as_ref().map(|t| &t.mask);
            out.masks = l
                .masks
                .iter()
                .filter(|m| text_path != Some(&m.path))
                .map(|m| RenderMask {
                    path: m.path.clone(),
                    shape: m.shape.clone(),
                    mode: MaskMode::from_token(&m.mode).unwrap_or_default(),
                    inverted: m.inverted,
                    opacity: m.opacity.clamp(0.0, 1.0) as f32,
                    feather: m.feather.map(|v| v as f32),
                    expansion: m.expansion as f32,
                })
                .collect();
            // The fx switch turns the whole effect stack off.
            out.effects = l
                .effects
                .iter()
                .filter(|_| l.switches.effects)
                .map(|e| {
                    let mut params: Vec<(u32, bae_model::Value)> = e
                        .params
                        .iter()
                        .filter_map(|(id, v)| Some((id.parse().ok()?, v.clone())))
                        .collect();
                    // The Paint effect's strokes visible at this frame.
                    if e.match_name == crate::paint::PAINT_EFFECT {
                        let strokes = crate::paint::strokes(&e.children, l.layer_time);
                        params.push((crate::paint::STROKES_PARAM, crate::paint::pack(&strokes)));
                    }
                    RenderEffect {
                        path: e.path.clone(),
                        effect: e.match_name.clone(),
                        enabled: e.enabled,
                        params,
                    }
                })
                .collect();
        }
        Ok(out)
    }

    /// The previous conversion's content of shape or text layer `l` while what
    /// its provider draws is unchanged; otherwise `make`'s (remembered).
    fn reuse(
        &mut self,
        l: &EvaluatedLayer,
        make: impl FnOnce(&EvaluatedLayer) -> LayerContent,
    ) -> LayerContent {
        if let Some((content, used)) = self.customs.get_mut(&l.path)
            && let Some(prev) = content.data.downcast_ref::<EvaluatedLayer>()
            && same_drawing(prev, l)
        {
            *used = true;
            return LayerContent::Custom(content.clone());
        }
        let content = make(l);
        if let LayerContent::Custom(c) = &content {
            self.customs.insert(l.path.clone(), (c.clone(), true));
        }
        content
    }

    fn content(
        &mut self,
        stage: &Stage,
        _ec: &EvaluatedComp,
        l: &EvaluatedLayer,
        depth: usize,
    ) -> Result<LayerContent, RenderError> {
        if matches!(
            l.kind,
            LayerKind::Null | LayerKind::Camera | LayerKind::Light | LayerKind::Audio
        ) {
            return Ok(LayerContent::None);
        }
        match &l.source {
            // Shape and text layers are drawn by 05's providers from the
            // evaluated layer (contents, source text, animators).
            LayerSource::None if l.kind == LayerKind::Shape => Ok(self.reuse(l, |l| {
                custom(SHAPE_PROVIDER, l, |h| {
                    hash_prims(&l.contents, h);
                    if has_wiggle(&l.contents) {
                        l.layer_time.hash(h);
                    }
                })
            })),
            LayerSource::None if l.kind == LayerKind::Text => Ok(self.reuse(l, |l| {
                custom(TEXT_PROVIDER, l, |h| {
                    if let Some(doc) = &l.text {
                        serde_json::to_vec(doc).unwrap_or_default().hash(h);
                    }
                    hash_prims(&l.text_animators, h);
                    // Text on a path: its options and the mask's shape.
                    if let Some(tp) = &l.text_path {
                        serde_json::to_vec(tp).unwrap_or_default().hash(h);
                        if let Some(m) = l.masks.iter().find(|m| m.path == tp.mask) {
                            serde_json::to_vec(&m.shape).unwrap_or_default().hash(h);
                        }
                    }
                })
            })),
            LayerSource::None => Ok(LayerContent::None),
            LayerSource::Comp { comp, nested, .. } => match nested {
                // bae-anim evaluates nested comps itself (at the mapped time).
                Some(nested) => {
                    let rc = self.convert(stage, nested, depth + 1)?;
                    Ok(LayerContent::Comp(Arc::new(rc)))
                }
                None => {
                    self.warn(format!("{}: pre-comp {comp} is missing or cyclic", l.path));
                    Ok(LayerContent::None)
                }
            },
            LayerSource::Asset { asset, time, info } => Ok(match info.kind.as_str() {
                "solid" => LayerContent::Solid {
                    color: info
                        .solid_color
                        .unwrap_or([0.5, 0.5, 0.5, 1.0])
                        .map(|v| v as f32),
                    width: info.width,
                    height: info.height,
                },
                "footage" | "sequence" => LayerContent::Footage(FootageRef {
                    asset: asset.clone(),
                    file: match stage.prim(asset).map(|view| self.assets.file(&view)) {
                        Some(Ok(file)) => file,
                        Some(Err(message)) => {
                            self.warn(format!("{}: {message}", l.path));
                            None
                        }
                        None => None,
                    },
                    size: (info.width > 0 && info.height > 0).then_some([info.width, info.height]),
                    source_time: *time,
                    sequence: (info.kind == "sequence")
                        .then(|| info.frame_rate.unwrap_or(bae_model::FrameRate::FPS_30)),
                    blend: l.frame_blend_span.map(|span| FrameBlend {
                        span,
                        pixel_motion: l.switches.frame_blending == FrameBlending::PixelMotion,
                    }),
                }),
                _ => LayerContent::None,
            }),
        }
    }
}

impl CompEvaluator for AnimEvaluator {
    fn evaluate(
        &mut self,
        stage: &Stage,
        comp: &PrimPath,
        time: Time,
    ) -> Result<RenderComp, RenderError> {
        self.eval(stage, comp, time, 0)
    }

    fn attribute(
        &mut self,
        stage: &Stage,
        path: &PrimPath,
        name: &str,
        time: Time,
    ) -> Option<bae_model::Value> {
        self.inner.evaluate_attribute(stage, path, name, time).ok()
    }

    fn integral(
        &mut self,
        stage: &Stage,
        path: &PrimPath,
        name: &str,
        frame: &crate::graph::EffectFrame,
        attr: &bae_model::AttrSpec,
    ) -> Option<Arc<crate::effect::ScalarTrack>> {
        let slot = self.scalar_tracks.get(stage, path, name, attr, frame);
        crate::parameter_track::prepare(slot, attr, frame, |t| {
            let (value, reads) = self
                .inner
                .evaluate_attribute_traced(stage, path, name, t)
                .ok()?;
            Some((value.as_f64()?, reads))
        })
    }

    fn invalidate(&mut self, event: &StageEvent) {
        self.inner.invalidate(event);
    }

    fn set_image_samples(&mut self, samples: Option<Arc<bae_anim::ImageSamples>>) {
        self.inner.set_image_samples(samples);
    }

    fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }
}

/// Provider ID of shape layers (05's `bae_vector::SHAPE_PROVIDER`).
pub const SHAPE_PROVIDER: &str = "bae.shape";
/// Provider ID of text layers (05's text provider).
pub const TEXT_PROVIDER: &str = "bae.text";

fn custom(provider: &str, l: &EvaluatedLayer, hash: impl FnOnce(&mut FxHasher)) -> LayerContent {
    let mut h = FxHasher::default();
    provider.hash(&mut h);
    hash(&mut h);
    LayerContent::Custom(CustomContent {
        provider: provider.to_owned(),
        prim: l.path.clone(),
        size: None,
        hash: h.finish(),
        data: Arc::new(l.clone()),
    })
}

/// Whether a shape or text provider draws `a` and `b` the same (what the
/// custom content hashes; the transform does not matter).
fn same_drawing(a: &EvaluatedLayer, b: &EvaluatedLayer) -> bool {
    a.kind == b.kind
        && match b.kind {
            LayerKind::Shape => {
                a.contents == b.contents
                    && (a.layer_time == b.layer_time || !has_wiggle(&b.contents))
            }
            LayerKind::Text => {
                a.text == b.text
                    && a.text_animators == b.text_animators
                    && a.text_path == b.text_path
                    && (b.text_path.is_none() || a.masks == b.masks)
            }
            _ => false,
        }
}

fn has_wiggle(prims: &[EvaluatedPrim]) -> bool {
    prims.iter().any(|p| {
        matches!(
            p.get("match_name")
                .or_else(|| p.get("operator"))
                .and_then(bae_model::Value::as_str),
            Some("ADBE Vector Filter - Roughen" | "wiggle_paths" | "roughen")
        ) && p
            .f64("ae:ADBE_Vector_Temporal_Freq")
            .or_else(|| p.f64("wiggle_paths:frequency"))
            != Some(0.0)
            || has_wiggle(&p.children)
    })
}

fn hash_prims(prims: &[EvaluatedPrim], h: &mut FxHasher) {
    prims.len().hash(h);
    for p in prims {
        p.path.hash(h);
        p.prim_type.hash(h);
        p.attrs.len().hash(h);
        for (name, value) in p.attrs.iter() {
            name.hash(h);
            hash_value(value, h);
        }
        hash_prims(&p.children, h);
    }
}

fn matte_mode(m: bae_anim::TrackMatte) -> Option<TrackMatteMode> {
    match m {
        bae_anim::TrackMatte::None => None,
        bae_anim::TrackMatte::Alpha => Some(TrackMatteMode::Alpha),
        bae_anim::TrackMatte::AlphaInverted => Some(TrackMatteMode::AlphaInverted),
        bae_anim::TrackMatte::Luma => Some(TrackMatteMode::Luma),
        bae_anim::TrackMatte::LumaInverted => Some(TrackMatteMode::LumaInverted),
    }
}

/// The layer's 4×4 world matrix when it is a 3D layer that draws (cameras and
/// lights are 3D but have no pixels).
fn matrix_3d(l: &EvaluatedLayer) -> Option<DMat4> {
    (l.switches.three_d && !matches!(l.kind, LayerKind::Camera | LayerKind::Light))
        .then_some(l.world_matrix)
}

/// The comp's active camera (03 picks it, AE-CMP-423).
fn active_camera(ec: &EvaluatedComp) -> Option<Camera> {
    let c: &CameraState = ec.layers.get(ec.camera?)?.camera.as_ref()?;
    Some(Camera {
        world: c.world_matrix,
        zoom: c.zoom,
        depth_of_field: c.depth_of_field,
        focus_distance: c.focus_distance,
        aperture: c.aperture,
        blur_level: c.blur_level,
        iris: c.iris,
        projection: crate::camera::Projection::Perspective,
    })
}

/// The lights of `ec` that light its layers.
fn comp_lights(ec: &EvaluatedComp) -> Arc<[Light]> {
    ec.layers
        .iter()
        .enumerate()
        .filter(|(_, l)| l.switches.enabled && l.active)
        .filter_map(|(i, l)| Some(light(i, l, l.light.as_ref()?)))
        .collect()
}

fn light(index: usize, l: &EvaluatedLayer, s: &LightState) -> Light {
    use bae_anim::{LightFalloff, LightType};
    Light {
        kind: match s.light_type {
            LightType::Parallel => LightKind::Parallel,
            LightType::Spot => LightKind::Spot,
            LightType::Point => LightKind::Point,
            LightType::Ambient => LightKind::Ambient,
            LightType::Environment => LightKind::Environment,
        },
        world: s.world_matrix,
        point_of_interest: s.point_of_interest,
        color: [s.color[0] as f32, s.color[1] as f32, s.color[2] as f32],
        intensity: s.intensity as f32,
        cone_angle: s.cone_angle as f32,
        cone_feather: s.cone_feather as f32,
        falloff: match s.falloff {
            LightFalloff::None => Falloff::None,
            LightFalloff::Smooth => Falloff::Smooth,
            LightFalloff::InverseSquareClamped => Falloff::InverseSquareClamped,
        },
        radius: s.radius as f32,
        falloff_distance: s.falloff_distance as f32,
        casts_shadows: s.casts_shadows,
        shadow_darkness: s.shadow_darkness as f32,
        shadow_diffusion: s.shadow_diffusion as f32,
        scope_below: l.switches.adjustment.then_some(index),
    }
}

/// Material Options of a 3D layer, as 03 evaluated them.
fn material(m: &bae_anim::MaterialState) -> Material {
    let mode = |v: u8| match v {
        1 => ShadowMode::On,
        2 => ShadowMode::Only,
        _ => ShadowMode::Off,
    };
    let percent = |v: f64| v.clamp(0.0, 100.0) as f32;
    Material {
        casts_shadows: mode(m.casts_shadows),
        light_transmission: percent(m.light_transmission),
        accepts_shadows: mode(m.accepts_shadows),
        accepts_lights: m.accepts_lights,
        ambient: percent(m.ambient),
        diffuse: percent(m.diffuse),
        specular_intensity: percent(m.specular_intensity),
        specular_shininess: percent(m.specular_shininess),
        metal: percent(m.metal),
    }
}

/// The 2D part of a layer → comp matrix (orthographic; z is dropped).
pub fn affine_2d(m: &DMat4) -> DAffine2 {
    DAffine2::from_cols(
        DVec2::new(m.x_axis.x, m.x_axis.y),
        DVec2::new(m.y_axis.x, m.y_axis.y),
        DVec2::new(m.w_axis.x, m.w_axis.y),
    )
}
