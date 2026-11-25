mod common;
use bae_model::Value;
use bae_render::IRect;
use common::*;

#[test]
fn layer_and_mask_controls_are_enabled() {
    for id in [1, 7, 3, 4] {
        assert!(!builtin("ADBE Fill").def().param(id).unwrap().flags.disabled);
    }
}

#[test]
fn fill_respects_project_depth() {
    let Some(gpu) = context() else { return };
    let r = IRect::from_size(3, 2);
    let source = Pixels::new(r, |_, _| [0.0, 0.0, 0.0, 0.5]);
    for (float, red) in [(true, 1.0), (false, 0.5)] {
        let result = run_depth(
            &gpu,
            F32,
            builtin("ADBE Fill"),
            vec![(2, Value::Color([2., 0., 0., 1.]))],
            &source,
            r,
            1.,
            float,
        );
        assert_eq!(result.get(0, 0), [red, 0., 0., 0.5]);
    }
}

#[test]
fn gradient_points_stay_with_layer_across_tiles() {
    use bae_effects::Standalone;
    use bae_render::EffectParams;
    let Some(gpu) = context() else { return };
    let bounds = IRect::from_size(128, 128);
    let tile = IRect::new(8, 16, 48, 32);
    let region = Pixels::new(tile, |_, _| [1.; 4]);
    let mut image = region.upload(&gpu, F32);
    // The half-resolution tile covers this region in layer pixels.
    image.rect = IRect::new(16, 32, 96, 64);
    image.pixel_scale = 2.0;
    let fx = builtin("ADBE Ramp");
    let inst = instance(&fx.info.match_name, vec![]);
    let p = EffectParams {
        layer_bounds: bounds,
        scale: 0.5,
        ..EffectParams::new(&inst)
    };
    let mut target = Standalone::new(gpu.clone(), F32);
    let out = fx.render(&mut target, &p, &image, tile).unwrap();
    target.finish().unwrap();
    assert_eq!(out.rect, image.rect);
    assert_eq!(out.texel_size(), tile.size());
    let data = bae_gpu::transfer::read_texture(&gpu, &out.texture).unwrap();
    for (x, y) in [(0, 0), (39, 15)] {
        let value = (tile.y0 as f32 + y as f32) / 64.0;
        let pixel = data.pixel(x, y);
        let rgba: [f32; 4] = std::array::from_fn(|i| {
            f32::from_le_bytes(pixel[4 * i..4 * i + 4].try_into().unwrap())
        });
        assert_eq!(rgba, [value, value, value, 1.]);
    }
}

#[test]
fn paint_bucket_connectivity_and_jump_flood_stroke() {
    let Some(gpu) = context() else { return };
    let rect = IRect::from_size(17, 13);
    let mut src = test_image(rect);
    for (i, pixel) in src.data.iter_mut().enumerate() {
        *pixel = if i % 17 == 8 {
            [1.; 4]
        } else {
            [0., 0., 0., 1.]
        };
    }
    for (stroke, last_red) in [(1, 7), (3, 9)] {
        let got = run(
            &gpu,
            F32,
            builtin("ADBE Paint Bucket"),
            vec![
                (2, Value::Vec2([3., 6.])),
                (8, Value::Int(stroke)),
                (5, Value::Float(2.)),
            ],
            &src,
            rect,
            1.,
        );
        assert_eq!(got.get(last_red, 6), [1., 0., 0., 1.]);
        assert_eq!(got.get(12, 6), [0., 0., 0., 1.]);
    }
}

#[test]
fn eyedropper_fills_a_distant_tile_with_the_sample_disk_mean() {
    let Some(gpu) = context() else { return };
    let input = Pixels::new(IRect::from_size(32, 16), |x, y| {
        if (x, y) == (8, 6) {
            [0.5, 0., 0., 0.5]
        } else {
            [0., 0., 0.5, 0.5]
        }
    });
    let tile = IRect::new(21, 4, 29, 12);
    for format in [F16, F32] {
        let out = run(
            &gpu,
            format,
            builtin("ADBE Eyedropper Fill"),
            vec![
                (1, Value::Vec2([8., 6.])),
                (2, Value::Float(1.)),
                (3, Value::Int(4)),
            ],
            &input,
            tile,
            1.,
        );
        for px in &out.data {
            for (got, want) in px.iter().zip([0.1, 0., 0.4, 0.5]) {
                assert!((got - want).abs() < 0.001, "{px:?}");
            }
        }
    }
}

#[test]
fn eyedropper_reduces_to_one_texel_at_supersampled_resolution() {
    let Some(gpu) = context() else { return };
    let src = Pixels::new(IRect::from_size(8, 8), |_, _| [0.25, 0., 0., 1.]);
    let input = src.upload(&gpu, F32);
    let inst = instance("ADBE Eyedropper Fill", vec![]);
    let params = bae_render::EffectParams {
        scale: 4.,
        layer_bounds: src.rect,
        ..bae_render::EffectParams::new(&inst)
    };
    let mut target = bae_effects::Standalone::new(gpu.clone(), F32);
    let out = builtin(&inst.effect)
        .render(&mut target, &params, &input, IRect::from_size(32, 32))
        .unwrap();
    target.finish().unwrap();
    assert!(
        Pixels::download(&gpu, &out)
            .data
            .iter()
            .all(|p| *p == [0.25, 0., 0., 1.])
    );
}

#[test]
fn radio_waves_batches_birth_controls_without_evaluating_other_frames() {
    use bae_model::{PrimPath, Time};
    use bae_render::{
        LayerContent, RenderComp, RenderEffect, RenderError, RenderLayer, RenderRequest, Renderer,
        effect::ParameterSample,
        graph::{CompFrames, EffectFrame},
    };

    #[derive(Default)]
    struct BirthControls {
        animated: bool,
        requests: Vec<Vec<Time>>,
    }

    impl CompFrames for BirthControls {
        fn animated_controls(&self, _: &PrimPath, _: &[u32]) -> bool {
            self.animated
        }

        fn parameters_at(
            &mut self,
            effect: &RenderEffect,
            ids: &[u32],
            _: &EffectFrame,
            times: &[Time],
        ) -> Vec<ParameterSample> {
            assert_eq!(effect.effect, "APC Radio Waves");
            assert!(ids.contains(&4) && !ids.contains(&34));
            self.requests.push(times.to_vec());
            times
                .iter()
                .map(|&time| ParameterSample {
                    time,
                    values: vec![(4, Value::Vec2([time.as_seconds(), 3.]))],
                })
                .collect()
        }

        fn comp_at(&mut self, _: &PrimPath, _: Time) -> Result<Option<RenderComp>, RenderError> {
            panic!("a wave birth requested a whole composition")
        }
    }

    let Some(gpu) = context() else { return };
    let mut r = Renderer::new(gpu);
    bae_effects::register(&mut bae_model::EffectRegistry::new(), r.effects_mut());
    let mut comp = RenderComp::new(PrimPath::new("/comps/main").unwrap(), 16, 8);
    comp.time = Time::from_seconds(2.);
    let mut layer = RenderLayer::new(
        PrimPath::new("/comps/main/layers/a").unwrap(),
        LayerContent::Solid {
            color: [0.; 4],
            width: 16,
            height: 8,
        },
    );
    layer.time = comp.time;
    layer
        .effects
        .push(instance("APC Radio Waves", vec![(34, Value::Float(2.))]));
    comp.layers.push(layer);
    let mut controls = BirthControls {
        animated: true,
        requests: Vec::new(),
    };
    r.render_frame_with(&comp, &RenderRequest::default(), Some(&mut controls))
        .unwrap();
    assert_eq!(
        controls.requests,
        [[0., 0.5, 1., 1.5].map(Time::from_seconds)]
    );
    controls.animated = false;
    controls.requests.clear();
    r.render_frame_with(&comp, &RenderRequest::default(), Some(&mut controls))
        .unwrap();
    assert!(controls.requests.is_empty());
}
