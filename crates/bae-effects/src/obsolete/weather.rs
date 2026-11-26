//! Legacy weather uses the modern particle engines. Interlace flicker is vertical Gaussian blur.
use super::*;
use crate::catalog::DefBuilder;

pub(super) fn register(target: &Builtin, out: &mut Vec<Builtin>) {
    let fx = match target.info.match_name.as_str() {
        "CSRainfall" => rain(target),
        "CSSnowfall" => snow(target),
        "ADBE Gaussian Blur 2" => flicker(target),
        _ => return,
    };
    out.push(fx);
}

fn rain(target: &Builtin) -> Builtin {
    let mut alias = Definition::inherit(
        target,
        "CC Rain",
        "rain-legacy",
        "CC Rain (obsolete)",
        "CC Rain (obsolete)",
    );
    let mut b = DefBuilder::new("CC Rain", "CC Rain (obsolete)", "Obsolete");
    b = b.slider(
        1,
        "Amount",
        "Amount",
        300.,
        [0., 10000.],
        [0., 10000.],
        2,
        "none",
    );
    b = b.slider(
        2,
        "Speed",
        "Speed",
        1.,
        [0.099991, 10.],
        [0.099991, 10.],
        2,
        "none",
    );
    b = b.slider(
        3,
        "Angle",
        "Angle",
        10.,
        [-60., 60.],
        [-60., 60.],
        2,
        "pixels",
    );
    b = b.slider(
        4,
        "Angle Variation",
        "Angle Variation",
        10.,
        [0., 60.],
        [0., 60.],
        2,
        "none",
    );
    b = b.slider(
        5,
        "Drop Size",
        "Drop Size",
        2.,
        [0., 30.],
        [0., 30.],
        2,
        "none",
    );
    b = b.slider(
        6,
        "Opacity",
        "Opacity",
        0.199997,
        [0., 1.],
        [0., 1.],
        2,
        "percent",
    );
    b = b.slider(
        7,
        "Source Depth",
        "Source Depth",
        1.,
        [0., 1.],
        [0., 1.],
        3,
        "none",
    );
    (alias.def, alias.info.param_names_zh) = b.build();
    alias.info.param_options_zh.clear();
    alias.finish(target, Some(map_rain))
}

fn snow(target: &Builtin) -> Builtin {
    let mut alias = Definition::inherit(
        target,
        "CC Snow",
        "snow-legacy",
        "CC Snow (obsolete)",
        "CC Snow (obsolete)",
    );
    let mut b = DefBuilder::new("CC Snow", "CC Snow (obsolete)", "Obsolete");
    b = b.slider(
        1,
        "Amount",
        "Amount",
        1000.,
        [0., 32000.],
        [0., 32000.],
        2,
        "none",
    );
    b = b.slider(2, "Speed", "Speed", 1., [-30., 30.], [-30., 30.], 2, "none");
    b = b.slider(
        3,
        "Amplitude",
        "Amplitude",
        2.,
        [0., 50.],
        [0., 50.],
        2,
        "none",
    );
    b = b.slider(
        4,
        "Frequency",
        "Frequency",
        1.,
        [0., 10.],
        [0., 10.],
        2,
        "none",
    );
    b = b.slider(
        5,
        "Flake Size",
        "Flake Size",
        2.,
        [0., 50.],
        [0., 50.],
        2,
        "none",
    );
    b = b.slider(
        6,
        "Source Depth",
        "Source Depth",
        1.,
        [0., 1.],
        [0., 1.],
        3,
        "none",
    );
    b = b.slider(
        7,
        "Opacity",
        "Opacity",
        0.5,
        [0., 1.],
        [0., 1.],
        2,
        "percent",
    );
    (alias.def, alias.info.param_names_zh) = b.build();
    alias.info.param_options_zh.clear();
    alias.finish(target, Some(map_snow))
}

fn flicker(target: &Builtin) -> Builtin {
    let mut alias = Definition::inherit(
        target,
        "ADBE Reduce Interlace Flicker",
        "flicker-legacy",
        "Reduce Interlace Flicker",
        "减少交错闪烁",
    );
    let mut b = DefBuilder::new(
        "ADBE Reduce Interlace Flicker",
        "Reduce Interlace Flicker",
        "Obsolete",
    );
    b = b.slider(
        1,
        "Softness",
        "柔和度",
        0.,
        [0., 1000.],
        [0., 1000.],
        2,
        "none",
    );
    (alias.def, alias.info.param_names_zh) = b.build();
    alias.info.param_options_zh.clear();
    alias.finish(target, Some(map_flicker))
}

fn map_rain(p: Params<'_>) -> Translation<'_> {
    // Resolve speed along the fall direction into the vertical speed and wind
    // components (pixels/second) used by Rainfall.
    let speed = 4000. * p.f(2);
    let angle = p.f(3).to_radians();
    let spread = speed * p.f(4).to_radians().sin();
    Translation::replace([
        (1, Value::Float(p.f(1))),
        (2, Value::Float(p.f(5))),
        (144, Value::Float(p.f(7))),
        (4, Value::Float(speed * angle.cos())),
        (5, Value::Float(speed * angle.sin())),
        (7, Value::Float(spread)),
        (9, Value::Float(p.f(6) * 100.)),
        (11, Value::Float(0.)),
        (18, Value::Int(2)),
        (15, Value::Int(1)),
        (18, Value::Int(2)),
    ])
}
fn map_snow(p: Params<'_>) -> Translation<'_> {
    Translation::replace([
        (1, Value::Float(p.f(1))),
        (2, Value::Float(p.f(5))),
        (4, Value::Float(if p.b(6) { 5000. } else { 0. })),
        (5, Value::Float(250. * p.f(2))),
        (11, Value::Float(p.f(3) * 10.)),
        (13, Value::Float(p.f(4))),
        (15, Value::Bool(false)),
        (19, Value::Float(p.f(7) * 100.)),
        (21, Value::Float(0.)),
        (144, Value::Float(p.f(6))),
        (25, Value::Int(1)),
    ])
}
fn map_flicker(p: Params<'_>) -> Translation<'_> {
    Translation::replace([
        (1, Value::Float(p.f(1))),
        (2, Value::Int(3)),
        (3, Value::Bool(true)),
    ])
}
#[cfg(test)]
mod tests {
    use super::*;
    use bae_model::PrimPath;

    #[test]
    fn weather_keeps_signed_speed_opacity_and_source_depth() {
        for (name, opacity, speed_id, opacity_id, depth_id) in
            [("CC Rain", 6, 4, 9, 7), ("CC Snow", 7, 5, 19, 6)]
        {
            let fx = crate::get(name).unwrap();
            let speed = if name == "CC Snow" { -2. } else { 2. };
            let instance = RenderEffect {
                path: PrimPath::new("/comp/layer/effects/weather").unwrap(),
                effect: name.into(),
                enabled: true,
                params: vec![
                    (2, Value::Float(speed)),
                    (3, Value::Float(0.)),
                    (opacity, Value::Float(0.125)),
                    (depth_id, Value::Float(0.4)),
                ],
            };
            let (_, mapped) = fx.effect().resolve_alias(&instance).unwrap();
            let expected_speed = if name == "CC Snow" { -500. } else { 8000. };
            assert_eq!(mapped.param(speed_id), Some(&Value::Float(expected_speed)));
            assert_eq!(mapped.param(opacity_id), Some(&Value::Float(12.5)));
            assert_eq!(mapped.param(144), Some(&Value::Float(0.4)));
        }
    }
}
