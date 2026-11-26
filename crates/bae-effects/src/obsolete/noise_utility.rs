//! Legacy identities for noise, Cineon conversion and original-layer compositing.
use super::*;

pub(super) fn register(target: &Builtin, out: &mut Vec<Builtin>) {
    let a = match target.info.match_name.as_str() {
        "ADBE Noise2" => noise(target, "ADBE Noise", "noise-legacy", &[]),
        "ADBE Noise Alpha2" => noise(
            target,
            "ADBE Noise Alpha",
            "noise-alpha-legacy",
            &[(2, 100.)],
        ),
        "ADBE Noise HLS2" => noise(
            target,
            "ADBE Noise HLS",
            "noise-hls-legacy",
            &[(2, 10.), (3, 10.), (4, 10.)],
        ),
        "ADBE Noise HLS Auto2" => noise(
            target,
            "ADBE Noise HLS Auto",
            "noise-hls-auto-legacy",
            &[(2, 10.), (3, 10.), (4, 10.)],
        ),
        "ADBE Cineon Converter2" => cineon(target),
        "CS Composite" => composite(target),
        _ => return,
    };
    out.push(a);
}

fn noise(target: &Builtin, old: &str, id: &str, limits: &[(u32, f64)]) -> Builtin {
    let mut a = Definition::inherit(target, old, id, &target.info.name, &target.info.name_zh);
    for &(id, max) in limits {
        if let ParamKind::Slider {
            valid_max,
            slider_max,
            ..
        } = &mut a.param(id).kind
        {
            *valid_max = max;
            *slider_max = slider_max.min(max);
        }
    }
    if old == "ADBE Noise Alpha" {
        a.info.param_names_zh.push((9, "循环（旋转次数）".into()));
    }
    a.finish(target, None)
}

fn cineon(target: &Builtin) -> Builtin {
    let mut a = Definition::inherit(
        target,
        "ADBE Cineon Converter",
        "cineon-converter-legacy",
        &target.info.name,
        &target.info.name_zh,
    );
    let ty = a.param(1);
    ty.kind = ParamKind::Popup {
        options: [
            "Log to Linear (obsolete)",
            "Linear to Log",
            "Log to Linear",
            "Log to Log",
            "Log to Linear (obsolete)",
            "Log to Log (obsolete)",
        ]
        .map(String::from)
        .to_vec(),
    };
    ty.default = Some(Value::Int(3));
    a.info.param_options_zh = vec![(
        1,
        [
            "对数到线性（过时的）",
            "线性到对数",
            "对数到线性",
            "对数到对数",
            "对数到线性（过时的）",
            "对数到对数（过时的）",
        ]
        .map(String::from)
        .to_vec(),
    )];
    a.finish(
        target,
        Some(|p| {
            let conversion = match p.popup(1) {
                2 => 1,
                4 | 6 => 3,
                _ => 2,
            };
            Translation::patch(p, [(1, Some(Value::Int(conversion)))])
        }),
    )
}

fn composite(target: &Builtin) -> Builtin {
    let mut a = Definition::inherit(
        target,
        "CC Composite",
        "cc-composite-legacy",
        "CC Composite (obsolete)",
        "CC Composite (obsolete)",
    );
    // The legacy always reads the original layer at its source stage.
    a.def.params.retain(|p| (1..=3).contains(&p.id));
    a.param(2).name = "Composite Original".into();
    if let Some((_, name)) = a.info.param_names_zh.iter_mut().find(|(id, _)| *id == 2) {
        *name = "Composite Original".into();
    }
    a.finish(
        target,
        Some(|p| {
            Translation::replace([
                (1, Value::Float(p.f(1))),
                (2, Value::Int(p.popup(2))),
                (3, Value::Bool(p.b(3))),
            ])
        }),
    )
}
