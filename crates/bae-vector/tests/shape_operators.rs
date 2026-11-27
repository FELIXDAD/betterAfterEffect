//! AE 26.2.1 coverage comparisons. Regenerate the frames with tests/ae/render.jsx.
use bae_anim::EvaluatedPrim;
use bae_model::{PrimPath, PrimType, Value};
use bae_vector::{
    ShapeGroup, VectorScene, kurbo::Affine, operators::parameter_attr, render::draw_cpu,
};
use std::path::PathBuf;

fn prim(ty: PrimType, params: &[(&str, Value)]) -> EvaluatedPrim {
    EvaluatedPrim {
        prim_type: Some(ty),
        path: PrimPath::new("/probe").unwrap(),
        attrs: std::sync::Arc::new(
            params
                .iter()
                .cloned()
                .map(|(k, v)| (k.to_owned(), v))
                .collect(),
        ),
        children: Vec::new().into(),
    }
}

fn op(name: &str, params: &[(&str, f64)]) -> EvaluatedPrim {
    let mut op = prim(
        PrimType::SHAPE_OPERATOR,
        &[(
            "match_name",
            Value::String(format!("ADBE Vector Filter - {name}")),
        )],
    );
    for (k, v) in params {
        std::sync::Arc::make_mut(&mut op.attrs).insert(
            parameter_attr(&format!("ADBE Vector {k}")),
            Value::Float(*v),
        );
    }
    op
}

fn rect(x: f64) -> EvaluatedPrim {
    prim(
        PrimType::RECT,
        &[
            ("size", Value::Vec2([80.0, 40.0])),
            ("position", Value::Vec2([x, 0.0])),
        ],
    )
}

fn scene(items: &[EvaluatedPrim], time: f64) -> VectorScene {
    let mut warnings = vec![];
    let group = ShapeGroup::from_evaluated(items, &mut warnings);
    assert!(warnings.is_empty(), "{warnings:?}");
    VectorScene::at_time(&group, time)
}

fn coverage(items: &[EvaluatedPrim], time: f64) -> Vec<u8> {
    let mut ctx = vello_cpu::RenderContext::new(256, 160);
    draw_cpu(
        &mut ctx,
        &scene(items, time),
        Affine::translate((128.0, 80.0)),
    );
    ctx.flush();
    let mut pixmap = vello_cpu::Pixmap::new(256, 160);
    ctx.render(&mut pixmap, &mut vello_cpu::Resources::new());
    pixmap
        .data_as_u8_slice()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|px| px[3])
        .collect()
}

#[test]
fn ae_operator_frames() {
    let mut cases = vec![
        ("offset".to_string(), op("Offset", &[]), 0.0),
        (
            "offsetCopies".into(),
            op(
                "Offset",
                &[("Offset Copies", 3.0), ("Offset Copy Offset", 0.5)],
            ),
            0.0,
        ),
        (
            "pucker".into(),
            op("PB", &[("PuckerBloat Amount", 40.0)]),
            0.0,
        ),
        ("round".into(), op("RC", &[]), 0.0),
        ("repeater".into(), op("Repeater", &[]), 0.0),
        ("zigzag".into(), op("Zigzag", &[]), 0.0),
        ("twist".into(), op("Twist", &[("Twist Angle", 90.0)]), 0.0),
        ("wiggle".into(), op("Roughen", &[]), 0.5),
    ];
    for (name, params) in [
        (
            "offsetBevel",
            vec![("Offset Amount", 16.0), ("Offset Line Join", 3.0)],
        ),
        (
            "offsetRound",
            vec![("Offset Amount", 16.0), ("Offset Line Join", 2.0)],
        ),
        (
            "offsetMiterLimit",
            vec![("Offset Amount", 16.0), ("Offset Miter Limit", 1.0)],
        ),
        ("inset", vec![("Offset Amount", -12.0)]),
    ] {
        cases.push((name.into(), op("Offset", &params), 0.0));
    }
    cases.extend([
        (
            "zigSmooth".into(),
            op("Zigzag", &[("Zigzag Detail", 3.0), ("Zigzag Points", 2.0)]),
            0.0,
        ),
        (
            "zigCurve".into(),
            op("Zigzag", &[("Zigzag Detail", 3.0)]),
            0.0,
        ),
        (
            "twist180".into(),
            op("Twist", &[("Twist Angle", 180.0)]),
            0.0,
        ),
    ]);
    let mut repeat = op(
        "Repeater",
        &[
            ("Repeater Copies", 3.5),
            ("Repeater Offset", 0.5),
            ("Repeater Rotation", 30.0),
            ("Repeater Opacity 1", 80.0),
            ("Repeater Opacity 2", 20.0),
        ],
    );
    for (prop, v) in [
        ("Position", [24.0, 1.0]),
        ("Anchor", [4.0, 2.0]),
        ("Scale", [110.0, 80.0]),
    ] {
        std::sync::Arc::make_mut(&mut repeat.attrs).insert(
            parameter_attr(&format!("ADBE Vector Repeater {prop}")),
            Value::Vec2(v),
        );
    }
    cases.push(("repeaterPaint".into(), repeat, 0.0));
    for mode in 1..=5 {
        cases.push((
            format!("merge{mode}"),
            op("Merge", &[("Merge Type", mode as f64)]),
            0.0,
        ));
    }
    for (name, operator, time) in cases {
        let mut items = vec![rect(0.0)];
        if name.starts_with("merge") {
            items.push(rect(24.0));
        }
        if name == "zigCurve" {
            items[0].prim_type = Some(PrimType::ELLIPSE);
        }
        let fill = prim(PrimType::FILL, &[]);
        if name == "repeaterPaint" {
            items.extend([fill, operator]);
        } else {
            items.extend([operator, fill]);
        }
        let actual = coverage(&items, time);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/ae/golden")
            .join(format!("{name}.png"));
        let expected = image::open(path).unwrap().into_rgba8();
        let sum: f64 = expected.pixels().map(|p| p.0[3] as f64).sum();
        let error: f64 = expected
            .pixels()
            .zip(&actual)
            .map(|(p, &a)| p.0[3].abs_diff(a) as f64)
            .sum::<f64>()
            / sum;
        eprintln!("{name}: {error:.6}");
        let tolerance = match name.as_str() {
            "wiggle" => 0.24, // Same distribution and timing; different seeded noise.
            "twist180" => 0.03,
            "pucker" => 0.015,
            _ => 0.01,
        };
        assert!(error <= tolerance, "{name}: {error:.6} > {tolerance}");
    }
}

#[test]
fn wiggle_changes_with_time_and_is_repeatable() {
    let items = [rect(0.0), op("Roughen", &[]), prim(PrimType::FILL, &[])];
    let first = scene(&items, 0.25);
    assert_ne!(first, scene(&items, 0.75));
    assert_eq!(first, scene(&items, 0.25));
}

#[test]
fn wiggle_phase_drives_a_stationary_seeded_contour() {
    let render = |params: &[(&str, f64)], time| {
        scene(
            &[rect(0.0), op("Roughen", params), prim(PrimType::FILL, &[])],
            time,
        )
    };
    let frozen = [("Temporal Freq", 0.0), ("Temporal Phase", 180.0)];
    assert_eq!(render(&frozen, 0.0), render(&frozen, 1.0));
    assert_eq!(render(&frozen, 1.0), render(&[], 0.25));
    for parameter in [
        "Random Seed",
        "Correlation",
        "Spatial Phase",
        "Roughen Size",
        "Roughen Detail",
        "Roughen Points",
    ] {
        assert_ne!(
            render(&[], 0.25),
            render(&[(parameter, 2.0)], 0.25),
            "{parameter}"
        );
    }
}

#[test]
fn only_preceding_geometry_is_offset_in_group_coordinates() {
    let mut group = prim(
        PrimType::SHAPE_GROUP,
        &[("transform:scale", Value::Vec2([200.0, 100.0]))],
    );
    group.children = vec![rect(0.0), prim(PrimType::FILL, &[])].into();
    let items = [
        group,
        op("Offset", &[]),
        rect(150.0),
        prim(PrimType::FILL, &[]),
    ];
    let draws = scene(&items, 0.0);
    let bounds = draws.bounds(false).unwrap();
    assert_eq!(
        bounds,
        bae_vector::kurbo::Rect::new(-90.0, -30.0, 190.0, 30.0)
    );
}

#[test]
fn repeater_instances_paints_but_supplies_compound_geometry_to_later_paints() {
    let fill = prim(PrimType::FILL, &[]);
    let mut repeater = op(
        "Repeater",
        &[
            ("Repeater Copies", 3.0),
            ("Repeater Opacity 1", 25.0),
            ("Repeater Opacity 2", 75.0),
        ],
    );
    std::sync::Arc::make_mut(&mut repeater.attrs).insert(
        parameter_attr("ADBE Vector Repeater Position"),
        Value::Vec2([60.0, 0.0]),
    );
    let draws = scene(&[rect(0.0), fill.clone(), repeater, fill], 0.0);
    assert_eq!(draws.items.len(), 4);
    let bae_vector::DrawItem::Fill(compound) = &draws.items[0] else {
        panic!()
    };
    assert_eq!(bae_vector::path::from_bez_path(&compound.path).len(), 3);
    let mut opacities = draws.items[1..]
        .iter()
        .map(|draw| {
            let bae_vector::DrawItem::Fill(f) = draw else {
                panic!()
            };
            f.opacity
        })
        .collect::<Vec<_>>();
    assert!(
        opacities[0] > opacities[2],
        "copy zero draws in front in mode 1"
    );
    opacities.sort_by(f32::total_cmp);
    assert_eq!(opacities, [0.25, 5.0 / 12.0, 7.0 / 12.0]);
}

#[test]
fn negative_repeater_scale_reflects_alternate_copies_with_fractional_offset() {
    let mut repeat = op(
        "Repeater",
        &[("Repeater Copies", 2.0), ("Repeater Offset", 0.5)],
    );
    std::sync::Arc::make_mut(&mut repeat.attrs).insert(
        parameter_attr("ADBE Vector Repeater Scale"),
        Value::Vec2([-50.0, 100.0]),
    );
    let scene = scene(&[rect(20.0), prim(PrimType::FILL, &[]), repeat], 0.0);
    let centers: Vec<f64> = scene
        .items
        .iter()
        .map(|draw| {
            let bae_vector::DrawItem::Fill(f) = draw else {
                panic!()
            };
            (f.transform * bae_vector::kurbo::Point::new(20.0, 0.0)).x
        })
        .collect();
    assert!((centers[0] + 7.0711).abs() < 0.001 && (centers[1] - 14.1421).abs() < 0.001);
}
