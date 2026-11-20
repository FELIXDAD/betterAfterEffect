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
use glam::DAffine2;
use rustc_hash::FxHasher;

use crate::blend::{BlendMode, MaskMode, TrackMatteMode};
use crate::geom::Rect;

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
        self.hash_content(&mut h);
        h.finish()
    }

    pub(crate) fn hash_content(&self, h: &mut impl Hasher) {
        self.path.hash(h);
        self.width.hash(h);
        self.height.hash(h);
        self.pixel_aspect.to_bits().hash(h);
        self.layers.len().hash(h);
        for layer in &self.layers {
            layer.hash_composite(h);
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
    /// The time the prim was evaluated at, in the prim's own time base.
    pub time: Time,
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
    /// What the layer draws.
    pub content: LayerContent,
    /// Layer space → comp space (pixels). Combines anchor point, position, scale,
    /// rotation and the parent chain: `T(position) · R · S · T(-anchor)`.
    pub transform: DAffine2,
    /// Opacity in `[0, 1]`.
    pub opacity: f32,
    /// Blending mode with the layers below.
    pub blend_mode: BlendMode,
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
    /// 调整图层 (reserved: not rendered yet).
    pub adjustment: bool,
    /// Masks, top first (reserved: not rendered yet).
    pub masks: Vec<RenderMask>,
    /// Effects in stack order (applied through the effect registry).
    pub effects: Vec<RenderEffect>,
}

impl RenderLayer {
    /// A visible, active, Normal layer with identity transform and full opacity.
    pub fn new(path: PrimPath, content: LayerContent) -> Self {
        Self {
            path,
            time: Time::ZERO,
            content,
            transform: DAffine2::IDENTITY,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            preserve_transparency: false,
            track_matte: None,
            visible: true,
            active: true,
            adjustment: false,
            masks: Vec::new(),
            effects: Vec::new(),
        }
    }

    /// Hash of the layer's own pixels in layer space (content, masks, effects),
    /// independent of transform, opacity and blending.
    pub fn content_hash(&self) -> u64 {
        let mut h = FxHasher::default();
        self.hash_content(&mut h);
        h.finish()
    }

    fn hash_content(&self, h: &mut impl Hasher) {
        self.content.hash_into(h);
        self.masks.len().hash(h);
        for m in &self.masks {
            m.hash_into(h);
        }
        self.effects.len().hash(h);
        for e in &self.effects {
            e.hash_into(h);
        }
    }

    fn hash_composite(&self, h: &mut impl Hasher) {
        self.path.hash(h);
        self.hash_content(h);
        for v in self.transform.to_cols_array() {
            v.to_bits().hash(h);
        }
        self.opacity.to_bits().hash(h);
        self.blend_mode.hash(h);
        self.preserve_transparency.hash(h);
        self.track_matte.hash(h);
        self.visible.hash(h);
        self.active.hash(h);
        self.adjustment.hash(h);
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
        }
    }

    fn hash_into(&self, h: &mut impl Hasher) {
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
            }
            LayerContent::Comp(c) => c.hash_content(h),
            LayerContent::Custom(c) => {
                c.provider.hash(h);
                c.prim.hash(h);
                c.hash.hash(h);
            }
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

/// An evaluated mask (reserved: masks are not rendered yet).
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
    fn hash_into(&self, h: &mut impl Hasher) {
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

    fn hash_into(&self, h: &mut impl Hasher) {
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
