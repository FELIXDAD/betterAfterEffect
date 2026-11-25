//! Legacy blur, transform, matte, spill and fill controls.
use super::*;
use crate::catalog::DefBuilder;

pub(super) fn register(target: &Builtin, out: &mut Vec<Builtin>) {
    let alias = match target.info.match_name.as_str() {
        "ADBE Box Blur2" => fast_blur(target),
        "ADBE Unsharp Mask2" => {
            let mut a = Definition::inherit(
                target,
                "ADBE Unsharp Mask",
                "unsharp-mask-legacy",
                "Unsharp Mask",
                "钝化蒙版",
            );
            if let ParamKind::Slider {
                valid_max,
                slider_max,
                precision,
                ..
            } = &mut a.param(3).kind
            {
                *valid_max = 255.;
                *slider_max = 255.;
                *precision = 0;
            }
            a.finish(
                target,
                Some(|p| {
                    let mut values = p.p.effect.params.clone();
                    assign(&mut values, 3, Value::Float(p.f(3) / 255.));
                    values
                }),
            )
        }
        "ADBE Set Matte3" => Definition::inherit(
            target,
            "ADBE Set Matte2",
            "set-matte-legacy",
            "Set Matte",
            "设置遮罩",
        )
        .finish(target, None),
        "ADBE Geometry2" => {
            let mut a = Definition::inherit(
                target,
                "ADBE Geometry",
                "transform-legacy",
                "Transform",
                "变换",
            );
            a.param(11).default = Some(Value::Bool(false));
            a.finish(
                target,
                Some(|p| {
                    let mut values = p.p.effect.params.clone();
                    assign(&mut values, 11, Value::Bool(p.b(11)));
                    values
                }),
            )
        }
        "ADBE Fill" => {
            let mut a = Definition::inherit(
                target,
                "ADBE PSL Solid Fill",
                "ps-solid-fill",
                "Photoshop Solid Fill",
                "Photoshop 纯色填充",
            );
            let mut b = DefBuilder::new(&a.def.match_name, &a.def.name, "Obsolete");
            b.push(
                1,
                "Color",
                "颜色",
                ParamKind::Color,
                Some(Value::Color([0., 0., 1., 1.])),
            );
            (a.def, a.info.param_names_zh) = b.build();
            a.finish(target, Some(|p| vec![(2, p.value(1).unwrap().clone())]))
        }
        "ADBE Spill2" => spill(target),
        _ => return,
    };
    out.push(alias);
}

fn fast_blur(target: &Builtin) -> Builtin {
    let mut a = Definition::inherit(
        target,
        "ADBE Fast Blur",
        "fast-blur-legacy",
        "Fast Blur (Legacy)",
        "快速模糊（旧版）",
    );
    (a.def, a.info.param_names_zh) = DefBuilder::new(&a.def.match_name, &a.def.name, "Obsolete")
        .slider(
            1,
            "Blurriness",
            "模糊度",
            0.,
            [0., 30000.],
            [0., 50.],
            1,
            "pixels",
        )
        .popup(
            2,
            "Blur Dimensions",
            "模糊方向",
            1,
            &["Horizontal and Vertical", "Horizontal", "Vertical"],
        )
        .checkbox(3, "Repeat Edge Pixels", "重复边缘像素", true)
        .build();
    for (id, _) in &mut a.info.param_options_zh {
        if *id == 3 {
            *id = 2;
        }
    }
    a.finish(
        target,
        Some(|p| {
            vec![
                (1, Value::Float(p.f(1))),
                (2, Value::Int(3)),
                (3, Value::Int(p.popup(2))),
                (4, Value::Bool(p.b(3))),
            ]
        }),
    )
}

fn spill(target: &Builtin) -> Builtin {
    let mut a = Definition::inherit(
        target,
        "ADBE Spill Suppressor",
        "spill-suppressor-legacy",
        "Spill Suppressor",
        "溢出抑制",
    );
    let mut b = DefBuilder::new(&a.def.match_name, &a.def.name, "Obsolete");
    b.push(
        1,
        "Color to Suppress",
        "要抑制的颜色",
        ParamKind::Color,
        Some(Value::Color([0., 0., 1., 1.])),
    );
    (a.def, a.info.param_names_zh) = b
        .slider(
            3,
            "Suppression",
            "抑制",
            100.,
            [0., 200.],
            [0., 200.],
            1,
            "percent",
        )
        .build();
    // Ultra accepts a custom key. Its desaturation gain is linear, with unity at 50%.
    a.finish(
        target,
        Some(|p| {
            vec![
                (1, Value::Int(2)),
                (3, p.value(1).unwrap().clone()),
                (5, Value::Float(p.f(3) * 0.5)),
            ]
        }),
    )
}
