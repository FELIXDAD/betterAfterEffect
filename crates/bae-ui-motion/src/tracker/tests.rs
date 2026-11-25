use super::gpu::{Matcher, Options, Region};
use bae_gpu::WarmUp;

#[test]
fn background_analysis_follows_motion_inside_a_precomp() {
    use bae_model::{AuthorId, CommandRegistry, FrameRate, PrimPath, Time, Value};
    use bae_ui::{PanelContext, PanelId, PanelRequests};
    use serde_json::json;
    use std::sync::Arc;
    let Some(gpu) = shared_context() else {
        return;
    };
    let mut scene = bae_render::fixture::SceneBuilder::new();
    let fps = FrameRate::FPS_30;
    let dt = fps.frame_duration();
    let nested = scene.comp("source", 64, 48, fps, Time::ONE_SECOND).unwrap();
    let dot = scene
        .solid_layer(&nested, "dot", [1., 1., 1., 1.], [8, 8], [24., 20.])
        .unwrap();
    for n in 0..3 {
        scene
            .key(
                &dot,
                "transform:position",
                Time(dt.0 * n),
                Value::Vec3([28. + 3. * n as f64, 24., 0.]),
            )
            .unwrap();
    }
    let comp = scene.comp("main", 64, 48, fps, Time::ONE_SECOND).unwrap();
    let source = scene
        .layer(&comp, "plate", "pre_comp", Some(&nested))
        .unwrap();
    scene
        .set(&source, "out_point", Value::Time(Time(3 * dt.0)))
        .unwrap();
    let mut stage = scene.into_stage();
    let mut registry = CommandRegistry::with_builtins();
    bae_motion::register_commands(&mut registry);
    let author = AuthorId::new("user:tracker").unwrap();
    let track = registry
        .execute(
            &mut stage,
            &author,
            "tracker.create",
            json!({"layer":source,"center":[28,24]}),
        )
        .unwrap()
        .output;
    let track = PrimPath::new(track.as_str().unwrap()).unwrap();
    let cache = Arc::new(super::gpu::TrackingGpu::default());
    cache.run(&gpu).unwrap();
    let mut resources = bae_app::Resources::new();
    resources.insert(gpu);
    resources.insert(cache);
    let mut requests = PanelRequests::default();
    let id = PanelId::new("tracker");
    let cx = PanelContext::new(&id, "Tracker", &stage, &author, &resources, &mut requests);
    let job =
        super::worker::start(&cx, &egui::Context::default(), &track, Time::ZERO, 1, false).unwrap();
    let mut frames = Vec::new();
    loop {
        match job
            .updates
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap()
        {
            super::worker::Update::Frame(f) => frames.push(f),
            super::worker::Update::End(result) => {
                result.unwrap();
                break;
            }
        }
    }
    assert_eq!(frames.len(), 3);
    for (i, f) in frames.iter().enumerate() {
        assert_eq!(f.time, Time(i as i64 * dt.0));
        let p = &f.points[0];
        assert!(
            (p.center[0] - 28. - 3. * i as f64).abs() < 0.3 && p.confidence > 99.,
            "{p:?}"
        );
    }
    registry
        .execute(
            &mut stage,
            &author,
            "tracker.record",
            json!({"tracker":track,"frames":frames}),
        )
        .unwrap();
    let point = bae_motion::tracker::points(&stage, &track).remove(0);
    assert_eq!(
        stage
            .prim(&point)
            .unwrap()
            .attr("attach_point")
            .unwrap()
            .keyframes
            .len(),
        3
    );
}
use bae_gpu::{testing::shared_context, transfer::create_texture_with_data, wgpu};
#[test]
fn gpu_finds_translated_feature_in_all_channels() {
    let Some(gpu) = shared_context() else {
        return;
    };
    let matcher = Matcher::new(gpu.device());
    let texture = |dx: i32, dy: i32| {
        let mut pixels = Vec::new();
        for y in 0..40 {
            for x in 0..48 {
                let (x, y) = (x - dx, y - dy);
                let v = (x * x + 13 * y * y + 11 * x * y).rem_euclid(251) as u8;
                pixels.extend_from_slice(&[v, v.wrapping_mul(3), 170, 255]);
            }
        }
        create_texture_with_data(
            &gpu,
            &wgpu::TextureDescriptor {
                label: Some("tracker fixture"),
                size: wgpu::Extent3d {
                    width: 48,
                    height: 40,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            &pixels,
        )
        .unwrap()
    };
    let reference = texture(0, 0);
    let current = texture(3, -2);
    for channel in 0..3 {
        let region = Region {
            reference: &reference,
            field: 0,
            center: [20, 18],
            search_center: [20, 18],
            feature: [12, 12],
            search: [26, 26],
        };
        let m = matcher
            .analyze(
                &gpu,
                &current,
                &[region],
                Options {
                    channel,
                    subpixel: true,
                    enhance: 0,
                    radius: 2,
                    field: 0,
                },
            )
            .unwrap()[0];
        assert!(
            (m[0] - 23.).abs() < 0.25 && (m[1] - 16.).abs() < 0.25 && m[2] > 0.99,
            "channel {channel}: {m:?}"
        );
    }
}
