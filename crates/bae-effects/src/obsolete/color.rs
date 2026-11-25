//! Color aliases keep the old AE controls and translate only the changed parameters.
use super::*;
use crate::catalog::DefBuilder;

pub(super) fn register(target: &Builtin, out: &mut Vec<Builtin>) {
    let alias = match target.info.match_name.as_str() {
        "ADBE Brightness & Contrast 2" => brightness(target),
        "ADBE Exposure2" => exposure(target),
        "ADBE PhotoFilterPS" => photo_filter(target),
        "ADBE Pro Levels2" => levels(target, true),
        "ADBE Easy Levels2" => {
            for (old, id, scale) in [
                ("ADBE Alpha Levels2", "alpha-levels-8bit", 255.),
                ("ADBE Alpha Levels3", "alpha-levels", 1.),
            ] {
                out.push(alpha_levels(target, old, id, scale));
            }
            levels(target, false)
        }
        _ => return,
    };
    out.push(alias);
}

fn range(p: &mut EffectParamDef, lo: f64, hi: f64) {
    if let ParamKind::Slider {
        valid_min,
        valid_max,
        slider_min,
        slider_max,
        ..
    } = &mut p.kind
    {
        *valid_min = lo;
        *valid_max = hi;
        *slider_min = slider_min.max(lo);
        *slider_max = slider_max.min(hi);
    }
}

fn brightness(target: &Builtin) -> Builtin {
    let mut a = Definition::inherit(
        target,
        "ADBE Brightness & Contrast",
        "brightness-contrast-legacy",
        "Brightness & Contrast",
        "亮度和对比度",
    );
    a.def.params.retain(|p| p.id <= 2);
    range(a.param(1), -100., 100.);
    for p in &mut a.def.params {
        if let ParamKind::Slider { display, .. } = &mut p.kind {
            *display = "pixels".into();
        }
    }
    a.finish(
        target,
        Some(|p| {
            vec![
                (1, Value::Float(p.f(1))),
                (2, Value::Float(p.f(2))),
                (3, Value::Bool(true)),
            ]
        }),
    )
}

fn legacy_channel(a: &mut Definition) {
    a.param(1).name = "Channel:".into();
    if let Some((_, name)) = a.info.param_names_zh.iter_mut().find(|(id, _)| *id == 1) {
        *name = "通道:".into();
    }
    let mut group = None;
    for param in &a.def.params {
        match param.kind {
            ParamKind::GroupStart => group = a.info.param_name_zh(param.id).map(str::to_owned),
            ParamKind::GroupEnd => {
                if let Some(name) = group.take() {
                    a.info.param_names_zh.push((param.id, name));
                }
            }
            _ => {}
        }
    }
}

fn exposure(target: &Builtin) -> Builtin {
    let mut a = Definition::inherit(
        target,
        "ADBE Exposure",
        "exposure-legacy",
        "Exposure",
        "曝光度",
    );
    legacy_channel(&mut a);
    if let Some((_, choices)) = a.info.param_options_zh.iter_mut().find(|(id, _)| *id == 1) {
        *choices = vec!["主要通道".into(), "单个通道".into()];
    }
    for id in [3, 8, 13, 18] {
        range(a.param(id), -100., 100.);
    }
    for id in [5, 10, 15, 20] {
        let param = a.param(id);
        param.name = param.name.replace(" Correction", "");
        range(param, 0.1, 10.);
    }
    for (id, name) in &mut a.info.param_names_zh {
        if [5, 10, 15, 20].contains(id) {
            *name = name.replace("校正", "");
        }
    }
    a.finish(target, None)
}

fn photo_filter(target: &Builtin) -> Builtin {
    let mut a = Definition::inherit(
        target,
        "ADBE Photo Filter",
        "photo-filter-legacy",
        "Photo Filter",
        "照片滤镜",
    );
    // LBA and LBB were inserted at modern popup positions 2 and 5.
    if let ParamKind::Popup { options } = &mut a.param(1).kind {
        options.remove(4);
        options.remove(1);
    }
    if let Some((_, options)) = a.info.param_options_zh.iter_mut().find(|(id, _)| *id == 1) {
        options.remove(4);
        options.remove(1);
    }
    a.param(2).default = Some(Value::Color([0.92549, 0.541176, 0., 0.913725]));
    a.finish(
        target,
        Some(|p| {
            let mut values = p.p.effect.params.clone();
            if let Some(color) = p.value(2) {
                assign(&mut values, 2, color.clone());
            }
            let old = p.popup(1);
            let new = match old {
                1 => 1,
                2 | 3 => old + 1,
                _ => old + 2,
            };
            assign(&mut values, 1, Value::Int(new));
            values
        }),
    )
}

fn levels(target: &Builtin, individual: bool) -> Builtin {
    let (old, id, black) = if individual {
        ("ADBE Pro Levels", "individual-levels-legacy", 38)
    } else {
        ("ADBE Easy Levels", "levels-legacy", 8)
    };
    let mut a = Definition::inherit(target, old, id, &target.info.name, &target.info.name_zh);
    legacy_channel(&mut a);
    for id in [black, black + 1] {
        let param = a.param(id);
        param.kind = ParamKind::Checkbox;
        param.default = Some(Value::Bool(true));
    }
    for p in &mut a.def.params {
        if matches!(p.kind, ParamKind::Slider { .. }) {
            if p.name.contains("Gamma") {
                range(p, 0., 5.);
            } else {
                range(p, -10000., 10000.);
            }
        }
    }
    a.finish(target, Some(map_levels))
}

fn map_levels(p: Params<'_>) -> Vec<(u32, Value)> {
    let id = if p.def.match_name == "ADBE Pro Levels" {
        38
    } else {
        8
    };
    let mut values =
        p.p.effect
            .params
            .iter()
            .filter(|(i, _)| *i != 2)
            .cloned()
            .collect();
    for id in [id, id + 1] {
        assign(&mut values, id, Value::Int(if p.b(id) { 1 } else { 2 }));
    }
    values
}

fn alpha_levels(target: &Builtin, old: &str, id: &str, white: f64) -> Builtin {
    let mut a = Definition::inherit(target, old, id, "Alpha Levels", "Alpha 色阶");
    let mut b = DefBuilder::new(old, "Alpha Levels", "Obsolete");
    for (i, en, zh, default) in [
        (1, "Input Black Level", "输入黑色阶", 0.),
        (2, "Input White Level", "输入白色阶", white),
        (3, "Gamma", "灰度系数", 1.),
        (4, "Output Black Level", "输出黑色阶", 0.),
        (5, "Output White Level", "输出白色阶", white),
    ] {
        let limits = if i == 3 { [0.000015, 10.] } else { [0., white] };
        b = b.slider(i, en, zh, default, limits, limits, 3, "none");
    }
    (a.def, a.info.param_names_zh) = b.build();
    a.info.param_options_zh.clear();
    a.finish(
        target,
        Some(|p| {
            let white = if p.def.match_name == "ADBE Alpha Levels2" {
                255.
            } else {
                1.
            };
            vec![
                (1, Value::Int(5)),
                (3, Value::Float(p.f(1) / white)),
                (4, Value::Float(p.f(2) / white)),
                (5, Value::Float(p.f(3))),
                (6, Value::Float(p.f(4) / white)),
                (7, Value::Float(p.f(5) / white)),
                (8, Value::Int(1)),
                (9, Value::Int(1)),
            ]
        }),
    )
}
