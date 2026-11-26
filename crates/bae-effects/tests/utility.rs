mod common;
use bae_model::Value;
use bae_render::IRect;
use common::*;

#[test]
fn overbrights_respects_channel_and_clip_color_in_hdr() {
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(2, 1);
    let image = Pixels::new(r, |x, _| if x == 0 { [2.0, 0., 0., 1.] } else { [0.5; 4] });
    let fx = builtin("CC Overbrights");
    let solid = run(&gpu, F32, fx, vec![(1, Value::Int(1))], &image, r, 1.);
    assert_eq!(solid.get(0, 0), [1., 0., 0., 1.]);
    assert_eq!(solid.get(1, 0), [0., 0., 0., 0.5]);
    let clipped = run(
        &gpu,
        F32,
        fx,
        vec![(2, Value::Color([0., 1., 0., 1.]))],
        &image,
        r,
        1.,
    );
    assert_eq!(clipped.get(0, 0), [0., 1., 0., 1.]);
    assert_eq!(clipped.get(1, 0), image.get(1, 0));
}
