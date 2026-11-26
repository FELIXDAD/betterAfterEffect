//! The native Photoshop style pass reads the transformed layer, the composite
//! below it and the optional track matte in composition coordinates.
use super::*;
use bae_model::Value;

/// Graph-only inputs, outside the editable style parameter blocks.
pub const OPACITY: u32 = 1000;
pub const MODE: u32 = 1001;
pub const PRESERVE_ALPHA: u32 = 1002;
pub const MATTE: u32 = 1003;
pub const COMP_SIZE: u32 = 1004;

pub fn backdrop_input(styles: &RenderEffect, time: Time) -> LayerDependency {
    LayerDependency::source(styles.path.clone(), time)
}

pub fn matte_input(styles: &RenderEffect, time: Time) -> LayerDependency {
    LayerDependency {
        stage: LayerStage::Masks,
        ..backdrop_input(styles, time)
    }
}

impl Builder<'_, '_> {
    /// Rasterize the transformed silhouette before applying styles. Only the
    /// final draw receives the track matte, layer opacity and blending mode.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn styled_layer(
        &mut self,
        comp: &RenderComp,
        index: usize,
        canvas: IRect,
        scale: f64,
        depth: usize,
        visiting: &mut Vec<usize>,
        effect: Arc<dyn Effect>,
        styles: &RenderEffect,
        below: &mut Vec<LayerOp>,
    ) -> Result<(), RenderError> {
        let layer = &comp.layers[index];
        let frame = EffectFrame {
            time: layer.time,
            comp_time: comp.time,
            layer_start_time: layer.start_time,
            layer_time_scale: layer.time_scale,
            layer_in: layer.in_point,
            scale: 1. / scale,
            layer_bounds: comp.rect().round_out(),
            frame_rate: comp.frame_rate,
            time_step: comp.frame_rate.frame_duration(),
            linear: self.linear,
            purpose: self.purpose,
            float: !self.clamp,
            shutter: comp.shutter,
        };
        let params = frame.params(styles);
        let needed = effect.input_roi(&params, canvas);
        let Some(mut source) =
            self.plan_layer(comp, index, needed, scale, depth, visiting, false)?
        else {
            return Ok(());
        };
        let bounds = effect
            .output_bounds(&params, source.bounds)
            .intersect(&canvas);
        if bounds.is_empty() {
            return Ok(());
        }
        let matte = match layer.track_matte {
            Some(tm) => match self.matte(comp, tm.layer, bounds, scale, depth, visiting)? {
                Some(node) => Some(MatteRef {
                    node,
                    mode: tm.mode,
                }),
                None if tm.mode.is_inverted() => None,
                None => return Ok(()),
            },
            None => None,
        };
        let rect = source.bounds;
        let layer_bounds = source
            .content
            .transformed_bounds(&source.transform)
            .round_out();
        let mut key = FxHasher::default();
        "style silhouette".hash(&mut key);
        layer.hash_composite(&mut key, Some(self.effects));
        if let OpInput::Image(node) = source.input {
            self.nodes[node.index()].key.hash(&mut key);
        }
        if let Some(mask) = source.mask {
            self.nodes[mask.node.index()].key.hash(&mut key);
        }
        rect.hash(&mut key);
        scale.to_bits().hash(&mut key);
        comp.time.hash(&mut key);
        source.opacity = 1.;
        source.blend = BlendMode::Normal;
        source.preserve_transparency = false;
        let input = self.push(Node {
            kind: NodeKind::Composite(CompositeNode {
                prim: layer.path.clone(),
                time: comp.time,
                ops: vec![source],
                clamp: self.clamp,
            }),
            rect,
            pixel_scale: scale,
            key: key.finish(),
            cacheable: false,
        });
        let frame = EffectFrame {
            layer_bounds,
            ..frame
        };
        // The native stack needs the actual backdrop for exterior blend modes.
        // Scene images and all intermediate shading stay on the GPU.
        let mut layers = Vec::new();
        let output_rect = if below.is_empty() {
            bounds
        } else {
            let mut key = FxHasher::default();
            "style backdrop".hash(&mut key);
            comp.hash_content(&mut key, Some(self.effects));
            (index, canvas, scale.to_bits(), self.clamp).hash(&mut key);
            let backdrop = self.push(Node {
                kind: NodeKind::Composite(CompositeNode {
                    prim: comp.path.clone(),
                    time: comp.time,
                    ops: std::mem::take(below),
                    clamp: self.clamp,
                }),
                rect: canvas,
                pixel_scale: scale,
                key: key.finish(),
                cacheable: false,
            });
            layers.push((backdrop_input(styles, comp.time), backdrop));
            canvas
        };
        if let Some(matte) = matte {
            layers.push((matte_input(styles, comp.time), matte.node));
        }
        let mut instance = styles.clone();
        instance.params.extend([
            (OPACITY, Value::Float(layer.opacity.into())),
            (MODE, Value::Int(layer.blend_mode.effective() as i64)),
            (PRESERVE_ALPHA, Value::Bool(layer.preserve_transparency)),
            (MATTE, Value::Int(matte.map_or(0, |m| m.mode as i64))),
            (
                COMP_SIZE,
                Value::Vec2([comp.width.into(), comp.height.into()]),
            ),
        ]);
        let mut key = FxHasher::default();
        "layer styles".hash(&mut key);
        instance.hash_into(&mut key);
        self.nodes[input.index()].key.hash(&mut key);
        for (_, id) in &layers {
            self.nodes[id.index()].key.hash(&mut key);
        }
        (output_rect, comp.time, scale.to_bits(), self.clamp).hash(&mut key);
        let out = self.push(Node {
            kind: NodeKind::Effect(Box::new(EffectNode {
                feedback: None,
                layer: layer.path.clone(),
                effect,
                instance,
                scene: None,
                frame,
                input,
                layers,
                masks: Vec::new(),
                samples: None,
            })),
            rect: output_rect,
            pixel_scale: scale,
            key: key.finish(),
            cacheable: false,
        });
        let bounds = output_rect;
        below.push(LayerOp {
            layer: layer.path.clone(),
            input: OpInput::Image(out),
            transform: DAffine2::IDENTITY,
            opacity: 1.,
            blend: BlendMode::Normal,
            dissolve_seed: 0,
            preserve_transparency: false,
            matte: None,
            mask: None,
            bounds,
            content: Rect::new(
                f64::from(bounds.x0) * scale,
                f64::from(bounds.y0) * scale,
                f64::from(bounds.x1) * scale,
                f64::from(bounds.y1) * scale,
            ),
            projection: None,
            motion: None,
            clips: Vec::new(),
            lighting: None,
            occlusion: None,
        });
        Ok(())
    }
}
