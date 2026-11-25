//! Parameter-only AE Expression Controls.

use std::sync::Arc;

use bae_model::{EffectDef, EffectParamDef, ParamFlags, ParamKind, Value};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};

use crate::target::{EffectTarget, TargetEffect};
use crate::{Builtin, rust_builtin};

const MENU_ITEMS: &[&str] = &["Item 1", "Item 2", "Item 3"];

pub(super) struct Control(pub(super) EffectDef);

impl Effect for Control {
    fn def(&self) -> &EffectDef {
        &self.0
    }

    fn renders_pixels(&self) -> bool {
        false
    }

    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        params: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(ctx, params, input, roi)
    }
}

impl TargetEffect for Control {
    fn encode(
        &self,
        _target: &mut dyn EffectTarget,
        _params: &EffectParams<'_>,
        input: &GpuImage,
        _roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        Ok(input.clone())
    }
}

#[allow(clippy::too_many_arguments)]
fn control(
    slug: &str,
    match_name: &str,
    name: &str,
    zh: &str,
    param_name: &str,
    param_zh: &str,
    kind: ParamKind,
    default: Option<Value>,
) -> Builtin {
    let layer = kind == ParamKind::Layer;
    let param = EffectParamDef {
        id: 1,
        name: param_name.into(),
        match_name: Some(if match_name == "ADBE Dropdown Control" {
            "Pseudo/@@wtM8H841Sg2uh023dgZBCw-0001".into()
        } else {
            format!("{match_name}-0001")
        }),
        kind,
        default,
        flags: ParamFlags {
            cannot_animate: layer,
            cannot_time_vary: layer,
            ..ParamFlags::default()
        },
    };
    let effect = Control(EffectDef {
        match_name: match_name.into(),
        name: name.into(),
        category: "Expression Controls".into(),
        plugin: None,
        params: vec![param],
        thread_safe: true,
        gpu: false,
    });
    rust_builtin(
        (Arc::new(effect), vec![(1, param_zh.into())]),
        slug,
        zh,
        &["expression", "control", "表达式", zh],
    )
}

pub(crate) fn builtins() -> [Builtin; 8] {
    [
        control(
            "slider-control",
            "ADBE Slider Control",
            "Slider Control",
            "滑块控制",
            "Slider",
            "滑块",
            ParamKind::Slider {
                valid_min: -1_000_000.0,
                valid_max: 1_000_000.0,
                slider_min: 0.0,
                slider_max: 100.0,
                precision: 2,
                integer: false,
                display: "none".into(),
            },
            Some(Value::Float(0.0)),
        ),
        control(
            "point-control",
            "ADBE Point Control",
            "Point Control",
            "点控制",
            "Point",
            "点",
            ParamKind::Point,
            Some(Value::Vec2([320.0, 180.0])),
        ),
        control(
            "point-3d-control",
            "ADBE Point3D Control",
            "3D Point Control",
            "3D 点控制",
            "3D Point",
            "3D 点",
            ParamKind::Point3d,
            Some(Value::Vec3([320.0, 180.0, 0.0])),
        ),
        control(
            "angle-control",
            "ADBE Angle Control",
            "Angle Control",
            "角度控制",
            "Angle",
            "角度",
            ParamKind::Angle,
            Some(Value::Float(0.0)),
        ),
        control(
            "checkbox-control",
            "ADBE Checkbox Control",
            "Checkbox Control",
            "复选框控制",
            "Checkbox",
            "复选框",
            ParamKind::Checkbox,
            Some(Value::Bool(false)),
        ),
        control(
            "color-control",
            "ADBE Color Control",
            "Color Control",
            "颜色控制",
            "Color",
            "颜色",
            ParamKind::Color,
            Some(Value::Color([1.0, 0.0, 0.0, 1.0])),
        ),
        control(
            "layer-control",
            "ADBE Layer Control",
            "Layer Control",
            "图层控制",
            // A missing reference resolves to the layer that owns the control.
            "Layer",
            "图层",
            ParamKind::Layer,
            None,
        ),
        control(
            "dropdown-control",
            "ADBE Dropdown Control",
            "Dropdown Menu Control",
            "下拉菜单控件",
            "Menu",
            "菜单",
            ParamKind::Popup {
                options: MENU_ITEMS.iter().map(|s| (*s).into()).collect(),
            },
            Some(Value::Int(1)),
        ),
    ]
}
