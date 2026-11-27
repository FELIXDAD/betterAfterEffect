use crate::{
    Builtin, EffectTarget, TargetEffect,
    catalog::DefBuilder,
    common,
    effects::{layer_input, sampled::Shader, temporal},
    gpu,
    params::Params,
    rust_builtin,
};
use bae_gpu::wgpu;
use bae_model::{EffectDef, ParamKind, Time, Value};
use bae_render::{
    Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError, effect::LayerDependency,
};
use std::sync::Arc;

static KEY: Shader = Shader {
    label: "bae.effects.difference_matte",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("difference.wgsl")
    ),
};
static WIRE: Shader = Shader {
    label: "bae.effects.wire_removal",
    source: concat!(
        include_str!("../../effects/sampled.wgsl"),
        include_str!("wire.wgsl")
    ),
};

pub(super) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    KEY.kernel(rt, format)?;
    WIRE.kernel(rt, format)?;
    Ok(())
}

enum Method {
    Key,
    Wire,
}
struct Cleanup {
    def: EffectDef,
    method: Method,
}

pub(super) fn builtins() -> [Builtin; 2] {
    let (def, names) = DefBuilder::new("ADBE Difference Matte2", "Difference Matte", "Keying")
        .popup(
            1,
            "View",
            "视图",
            1,
            &["Final Output", "Source Only", "Matte Only"],
        )
        .layer(2, "Difference Layer", "差值图层")
        .layer_stage(2)
        .popup(
            3,
            "If Layer Sizes Differ",
            "如果图层大小不同",
            1,
            &["Center", "Stretch to Fit"],
        )
        .slider(
            4,
            "Matching Tolerance",
            "匹配容差",
            15.,
            [0., 100.],
            [0., 100.],
            1,
            "percent",
        )
        .slider(
            5,
            "Matching Softness",
            "匹配柔和度",
            0.,
            [0., 100.],
            [0., 100.],
            1,
            "percent",
        )
        .slider(
            6,
            "Blur Before Difference",
            "差值前模糊",
            0.,
            [0., 1000.],
            [0., 100.],
            1,
            "pixels",
        )
        .build();
    let mut key = rust_builtin(
        (
            Arc::new(Cleanup {
                def,
                method: Method::Key,
            }),
            names,
        ),
        "difference-matte",
        "差值遮罩",
        &["clean plate", "matte", "抠像"],
    );
    for (id, options) in [
        (1, &["最终输出", "仅限源", "仅限遮罩"][..]),
        (3, &["居中", "伸缩以适合"][..]),
    ] {
        key.info
            .param_options_zh
            .push((id, options.iter().map(|s| (*s).to_owned()).collect()));
    }

    let mut b = DefBuilder::new("CC Simple Wire Removal", "CC Simple Wire Removal", "Keying");
    for (id, name, y) in [(1, "Point A", 270.), (2, "Point B", 90.)] {
        b.push(
            id,
            name,
            name,
            ParamKind::Point,
            Some(Value::Vec2([320., y])),
        );
    }
    let (def, names) = b
        .reference_points()
        .popup(
            3,
            "Removal Style",
            "Removal Style",
            3,
            &["Fade", "Frame Offset", "Displace", "Displace Horizontal"],
        )
        .slider(
            4,
            "Thickness",
            "Thickness",
            2.,
            [0., 100.],
            [0., 100.],
            1,
            "pixels",
        )
        .slider(5, "Slope", "Slope", 0.5, [0., 1.], [0., 1.], 2, "none")
        .slider(
            6,
            "Mirror Blend",
            "Mirror Blend",
            0.25,
            [0., 1.],
            [0., 1.],
            2,
            "none",
        )
        .int(7, "Frame Offset", "Frame Offset", 5, [-120, 120])
        .build();
    let wire = rust_builtin(
        (
            Arc::new(Cleanup {
                def,
                method: Method::Wire,
            }),
            names,
        ),
        "simple-wire-removal",
        "CC Simple Wire Removal",
        &["wire", "去线", "抠像"],
    );
    [key, wire]
}

impl Cleanup {
    fn dependency(&self, v: Params<'_>) -> Option<LayerDependency> {
        match self.method {
            Method::Key if v.popup(1) != 2 => layer_input::dependency(v.p, 2, true),
            Method::Wire if v.popup(3) == 2 && v.f(7) != 0. => {
                let dt = v.f(7) * v.p.time_step.as_seconds() * v.p.layer_time_scale;
                temporal::before_effect(v.p, v.p.comp_time + Time::from_seconds(dt))
            }
            _ => None,
        }
    }
}

impl Effect for Cleanup {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn layer_dependencies(&self, p: &EffectParams<'_>) -> Vec<LayerDependency> {
        self.dependency(Params::new(&self.def, p))
            .into_iter()
            .collect()
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        if roi.is_empty() {
            return roi;
        }
        let v = Params::new(&self.def, p);
        let radius = match self.method {
            Method::Key if v.popup(1) != 2 => 2. * v.f(6) * p.scale,
            // Displace reads (2 * half width + 1) / |axis . normal| from a
            // pixel inside the wire.
            Method::Wire if v.popup(3) >= 3 => {
                let line = match (v.value(1), v.value(2)) {
                    (Some(Value::Vec2(a)), Some(Value::Vec2(b))) => [b[0] - a[0], b[1] - a[1]],
                    _ => [1., 0.],
                };
                let length = line[0].hypot(line[1]).max(1e-9);
                let across = if v.popup(3) == 4 {
                    (line[1] / length).abs()
                } else {
                    1.
                };
                (v.f(4) * p.scale + 1.) / across.max(0.05) + 2.
            }
            _ => 0.,
        };
        roi.inflate(radius.ceil() as i32)
    }
    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(ctx, p, input, roi)
    }
}

impl TargetEffect for Cleanup {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        if roi.is_empty() {
            return Ok(input.clone());
        }
        let v = Params::new(&self.def, p);
        let dep = self.dependency(v);
        let map = dep.as_ref().and_then(|d| target.effect_layer(d)).cloned();
        match self.method {
            Method::Key => {
                if v.popup(1) == 2 {
                    return Ok(input.clone());
                }
                let map = map.as_ref().or_else(|| {
                    (p.layer_bounds.is_empty() && p.value(2).is_none()).then_some(input)
                });
                let Some(map) = map else {
                    return Ok(input.clone());
                };
                let plate = common::blurred(target, map, v.f(6))?;
                let source = common::blurred(target, input, v.f(6))?;
                KEY.dispatch(target, v, roi, input, [Some(&plate), Some(&source)], [0, 0])
            }
            Method::Wire => WIRE.dispatch(target, v, roi, input, [map.as_ref()], [0, 0]),
        }
    }
}
