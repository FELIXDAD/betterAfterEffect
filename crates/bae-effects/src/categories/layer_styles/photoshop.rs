use super::edge::{Kind, Shading};
use crate::{
    Builtin, EffectTarget, TargetEffect, catalog::DefBuilder, params::Params, rust_builtin,
};
use bae_model::{EffectDef, ParamKind, Value};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};
use std::sync::Arc;

struct Photoshop {
    def: EffectDef,
    kind: Kind,
}

pub(super) fn entries() -> Vec<Builtin> {
    use Kind::*;
    [
        (DropShadow, "Drop Shadow", "投影", "ps-drop-shadow"),
        (InnerShadow, "Inner Shadow", "内阴影", "ps-inner-shadow"),
        (OuterGlow, "Outer Glow", "外发光", "ps-outer-glow"),
        (InnerGlow, "Inner Glow", "内发光", "ps-inner-glow"),
        (Bevel, "Bevel Emboss", "斜面和浮雕", "ps-bevel-emboss"),
    ]
    .into_iter()
    .map(|(kind, name, zh, slug)| {
        let mut b = DefBuilder::new(
            &format!("ADBE PSL {name}"),
            &format!("Photoshop {name}"),
            "Obsolete",
        );
        let color = match kind {
            DropShadow | InnerShadow => [0., 0., 0., 1.],
            OuterGlow | InnerGlow => [0.980392, 0.980392, 0.745098, 1.],
            Bevel => [1.; 4],
        };
        b.push(
            1,
            "Color",
            "颜色",
            ParamKind::Color,
            Some(Value::Color(color)),
        );
        let mut options: Vec<(u32, Vec<String>)> = Vec::new();
        if matches!(kind, DropShadow | InnerShadow | Bevel) {
            b.push(
                2,
                "Angle",
                "角度",
                ParamKind::Angle,
                Some(Value::Float(120.)),
            );
            b = if kind == Bevel {
                b.slider(3, "Depth", "深度", 5., [1., 20.], [1., 20.], 0, "percent")
            } else {
                b.slider(
                    3,
                    "Distance",
                    "距离",
                    5.,
                    [0., 300.],
                    [0., 300.],
                    1,
                    "pixels",
                )
            };
        }
        let blur = if matches!(kind, OuterGlow | InnerGlow) {
            2
        } else {
            4
        };
        b = b.slider(blur, "Blur", "模糊", 5., [0., 50.], [0., 50.], 1, "pixels");
        if kind == Bevel {
            b = b
                .popup(5, "Direction", "方向", 2, &["Up", "Down"])
                .popup(6, "Style", "样式", 1, &["Emboss", "Pillow Emboss"])
                .popup(7, "Show Bevel:", "显示斜面:", 2, &["Highlight", "Shadow"]);
            for (id, labels) in [
                (5, ["向上", "向下"]),
                (6, ["浮雕", "枕状浮雕"]),
                (7, ["高光", "阴影"]),
            ] {
                options.push((id, labels.map(String::from).to_vec()));
            }
        } else {
            b = b.slider(
                blur + 1,
                "Intensity",
                "强度",
                0.,
                [0., 600.],
                [0., 100.],
                1,
                "percent",
            );
            if kind == InnerGlow {
                b = b.popup(4, "Glow Type", "发光类型", 2, &["Center", "Edge"]);
                options.push((4, vec!["中心".into(), "边缘".into()]));
            }
        }
        let (def, names) = b.build();
        let mut builtin = rust_builtin(
            (Arc::new(Photoshop { def, kind }), names),
            slug,
            &format!("Photoshop {zh}"),
            &["Photoshop", "Layer Styles", "图层样式"],
        );
        builtin.info.param_options_zh = options;
        builtin
    })
    .collect()
}

impl Photoshop {
    fn shading(&self, params: &EffectParams<'_>) -> Shading {
        let p = Params::new(&self.def, params);
        let color = match p.value(1) {
            Some(Value::Color(c)) => c.map(|v| v as f32),
            _ => [0.; 4],
        };
        let glow = matches!(self.kind, Kind::OuterGlow | Kind::InnerGlow);
        let bevel = self.kind == Kind::Bevel;
        let blur = if glow { 2 } else { 4 };
        let angle = p.f(2).to_radians();
        let axis = [-angle.cos(), angle.sin()];
        let distance = if bevel || glow {
            0.
        } else {
            p.f(3) * params.scale
        };
        let dir = if p.popup(5) == 1 { 1. } else { -1. };
        let highlight = !bevel || p.popup(7) == 1;
        Shading {
            kind: self.kind,
            color: if highlight { color } else { [0.; 4] },
            shadow_color: if highlight { [0.; 4] } else { color },
            offset: axis.map(|v| v * distance),
            sigma: p.f(blur).clamp(0., 50.) * 0.5 * params.scale,
            gain: if bevel {
                p.f(3) as f32
            } else {
                1. + p.f(blur + 1) as f32 / 100.
            },
            center: self.kind == Kind::InnerGlow && p.popup(4) == 1,
            light: axis.map(|v| (v * dir) as f32),
            pillow: bevel && p.popup(6) == 2,
            mode: 2,
            shadow_mode: 2,
        }
    }
}

impl Effect for Photoshop {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn output_bounds(&self, p: &EffectParams<'_>, r: IRect) -> IRect {
        self.shading(p).bounds(r)
    }
    fn input_roi(&self, p: &EffectParams<'_>, r: IRect) -> IRect {
        self.shading(p).read(r)
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

impl TargetEffect for Photoshop {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.shading(p)
            .encode(target, Params::new(&self.def, p), input, input, roi)
    }
}
