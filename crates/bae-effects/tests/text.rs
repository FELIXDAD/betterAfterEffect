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
