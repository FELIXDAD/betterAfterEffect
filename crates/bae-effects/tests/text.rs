mod common;
use bae_model::Value;
use bae_render::IRect;
use common::*;

#[test]
fn changing_numbers_changes_only_the_stamp() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(120, 56);
    let src = test_image(rect);
    let render = |n| {
        run(
            &gpu,
            F32,
            builtin("ADBE Numbers2"),
            vec![
                (3, Value::Float(n)),
                (6, Value::Vec2([5., 3.])),
                (11, Value::Float(30.)),
            ],
            &src,
            rect,
            1.,
        )
    };
    let first = render(12.);
    let next = render(13.);
    assert_ne!(first.data, next.data);
    assert_eq!(first.get(119, 55), src.get(119, 55));
}

#[test]
fn editable_text_and_paths_draw_shaped_glyphs_on_the_layer() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(240, 120);
    let clear = Pixels::new(rect, |_, _| [0.; 4]);
    let render = |effect, mut params: Vec<(u32, Value)>| {
        params.extend([(1, Value::String("BAE 字".into())), (3, Value::Float(22.))]);
        run(&gpu, F32, builtin(effect), params, &clear, rect, 1.)
    };
    let stamp = render("ADBE Basic Text2", vec![(10, Value::Vec2([120., 60.]))]);
    assert!(stamp.data.iter().filter(|p| p[3] > 0.1).count() > 60);
    let path = |y| {
        render(
            "ADBE Path Text",
            vec![
                (11, Value::Int(4)),
                (12, Value::Vec2([12., y])),
                (15, Value::Vec2([220., y])),
            ],
        )
    };
    let a = path(40.);
    let b = path(80.);
    let row_alpha = |p: &Pixels, y: usize| {
        p.data[y * 240..(y + 1) * 240]
            .iter()
            .map(|c| c[3])
            .sum::<f32>()
    };
    assert!(a.data.iter().any(|p| p[3] > 0.5));
    for y in 0..70 {
        assert!((row_alpha(&a, y) - row_alpha(&b, y + 40)).abs() < 0.1);
    }
}
