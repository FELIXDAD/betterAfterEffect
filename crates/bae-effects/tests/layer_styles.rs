mod common;

#[test]
fn native_stroke_and_overlay_follow_the_transformed_silhouette() {
    use bae_model::{PrimPath, Value, layer_styles::RENDER_ID};
    use bae_render::{
        LayerContent, OutputAlpha, RenderComp, RenderEffect, RenderLayer, RenderRequest, Renderer,
    };
    use glam::{DAffine2, DVec2};
    let Some(gpu) = context() else {
        return;
    };
    let mut renderer = Renderer::new(gpu);
    renderer
        .effects_mut()
        .register(get(RENDER_ID).unwrap().effect());
    let path = PrimPath::new("/comps/C").unwrap();
    let mut comp = RenderComp::new(path.clone(), 48, 32);
    let mut layer = RenderLayer::new(
        path.child("a").unwrap(),
        LayerContent::Solid {
            color: [0., 1., 0., 1.],
            width: 8,
            height: 8,
        },
    );
    layer.transform =
        DAffine2::from_scale_angle_translation(DVec2::new(2., 1.), 0., DVec2::new(12., 12.));
    layer.layer_styles = Some(RenderEffect {
        path: layer.path.child("layer_styles").unwrap(),
        effect: RENDER_ID.into(),
        enabled: true,
        params: vec![
            (700, Value::Bool(true)),
            (900, Value::Bool(true)),
            (902, Value::Color([0., 0., 1., 1.])),
        ],
    });
    comp.layers.push(layer);
    let frame = renderer
        .render_frame(&comp, &RenderRequest::full())
        .unwrap();
    let img = renderer.read_rgba8(&frame, OutputAlpha::Straight).unwrap();
    let pix = |x: usize, y: usize| &img.data[(y * 48 + x) * 4..(y * 48 + x + 1) * 4];
    assert_eq!(pix(18, 16), [255, 0, 0, 255]);
    assert_eq!(pix(10, 16), [0, 0, 255, 255]);
    assert_eq!(pix(8, 16)[3], 0);
    assert_eq!(pix(18, 10), [0, 0, 255, 255]);
    // A red multiply shadow on blue is black. Layer opacity mixes that
    // intersection with blue once, after the style has blended with the backdrop.
    let mut back = RenderLayer::new(
        path.child("back").unwrap(),
        LayerContent::Solid {
            color: [0., 0., 1., 1.],
            width: 48,
            height: 32,
        },
    );
    back.transform = DAffine2::IDENTITY;
    let front = &mut comp.layers[0];
    front.opacity = 0.5;
    front.layer_styles.as_mut().unwrap().params = vec![
        (100, Value::Bool(true)),
        (102, Value::Color([1., 0., 0., 1.])),
        (103, Value::Float(100.)),
        (105, Value::Float(180.)),
        (106, Value::Float(8.)),
        (108, Value::Float(0.)),
    ];
    comp.layers.push(back);
    let frame = renderer
        .render_frame(&comp, &RenderRequest::full())
        .unwrap();
    let img = renderer.read_rgba8(&frame, OutputAlpha::Straight).unwrap();
    let at = |x: usize| &img.data[(16 * 48 + x) * 4..(16 * 48 + x + 1) * 4];
    assert_eq!(at(2), [0, 0, 255, 255]);
    let shadow = at(32);
    assert!(
        shadow[0] == 0 && shadow[1] == 0 && (126..=129).contains(&shadow[2]) && shadow[3] == 255,
        "{shadow:?}"
    );
}

use bae_effects::get;
use bae_render::IRect;
use common::{F32, Pixels, context, run};

#[test]
fn photoshop_styles_follow_the_alpha_edge() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(32, 32);
    let input = Pixels::new(rect, |x, y| {
        if (8..24).contains(&x) && (8..24).contains(&y) {
            [0.35, 0.35, 0.35, 1.]
        } else {
            [0.; 4]
        }
    });
    for name in [
        "Drop Shadow",
        "Inner Shadow",
        "Outer Glow",
        "Inner Glow",
        "Bevel Emboss",
    ] {
        let fx = get(&format!("ADBE PSL {name}")).unwrap();
        let out = run(&gpu, F32, fx, vec![], &input, rect, 1.);
        assert!(out.data.iter().flatten().all(|v| v.is_finite()));
        if matches!(name, "Drop Shadow" | "Outer Glow") {
            assert!(out.get(24, 18)[3] > 0., "{name}");
            assert_eq!(out.get(15, 15), input.get(15, 15));
        } else {
            assert_eq!(out.get(25, 18)[3], 0., "{name}");
            assert!(
                out.data
                    .iter()
                    .zip(&input.data)
                    .any(|(a, b)| (a[0] - b[0]).abs() > 0.02),
                "{name}"
            );
        }
    }
}

#[test]
fn gradient_stops_and_precise_glow_use_the_original_shape() {
    use bae_model::{Value, layer_styles::RENDER_ID};
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(32, 32);
    let shape = Pixels::new(rect, |x, y| {
        if (8..24).contains(&x) && (8..24).contains(&y) {
            [0., 0., 0., 1.]
        } else {
            [0.; 4]
        }
    });
    let fx = get(RENDER_ID).unwrap();
    // Cross two uniform page boundaries while interpolating red, green and blue.
    let stops = (0..=200)
        .flat_map(|i| {
            let x = f64::from(i) / 200.;
            let r = (1. - 2. * x).max(0.);
            let b = (2. * x - 1.).max(0.);
            [x, r, 1. - r - b, b, 1.]
        })
        .collect();
    let params = vec![
        (800, Value::Bool(true)),
        (805, Value::Float(0.)),
        (803, Value::FloatList(stops)),
        (300, Value::Bool(true)),
        (308, Value::Int(2)),
    ];
    let out = run(&gpu, F32, fx, params, &shape, rect, 1.);
    assert!(out.get(16, 16)[1] > 0.9);
    assert!(out.get(8, 16)[0] > out.get(23, 16)[0]);
    assert!(out.get(23, 16)[2] > out.get(8, 16)[2]);
    assert!(out.get(25, 16)[3] > 0. && out.get(0, 0)[3] == 0.);
}
