#![cfg(feature = "media")]

use bae_gpu::transfer::ImageData;
use bae_gpu::wgpu::TextureFormat;
use bae_model::{FrameRate, PrimPath, Time, Value};
use bae_render::export::{ExportFormat, ExportJob, ExportSettings, export_frames};
use bae_render::fixture::SceneBuilder;
use bae_render::{
    AnimEvaluator, CompEvaluator, OutputAlpha, RenderPurpose, RenderRequest, Renderer,
};

const FPS: FrameRate = FrameRate::FPS_30;
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

fn preview(renderer: &mut Renderer, scene: &SceneBuilder, comp: &PrimPath, frame: i64) -> Vec<u8> {
    let mut eval = AnimEvaluator::new(None).with_purpose(RenderPurpose::Preview);
    let comp = eval
        .evaluate(scene.stage(), comp, FPS.frame_to_time(frame))
        .unwrap();
    let request = RenderRequest {
        purpose: RenderPurpose::Preview,
        ..Default::default()
    };
    let result = renderer.render_frame(&comp, &request).unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    renderer
        .read_rgba8(&result, OutputAlpha::Straight)
        .unwrap()
        .data
}

#[test]
fn stretched_footage_switches_boundaries_and_png_export() {
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    for (i, color) in [RED, BLUE].into_iter().enumerate() {
        bae_render::png_out::write_rgba8(
            &dir.path().join(format!("source_{i}.png")),
            &ImageData {
                width: 2,
                height: 2,
                format: TextureFormat::Rgba8Unorm,
                bytes_per_row: 8,
                data: color.repeat(4),
            },
        )
        .unwrap();
    }
    let mut scene = SceneBuilder::new();
    let comp = scene.comp("main", 2, 2, FPS, FPS.frame_to_time(6)).unwrap();
    let asset = scene
        .footage_asset(
            "source",
            &dir.path().join("source_0.png"),
            None,
            Some([2, 2]),
        )
        .unwrap();
    scene
        .set(&asset, "kind", Value::Enum("sequence".into()))
        .unwrap();
    scene
        .set(&asset, "frame_rate", Value::FrameRate(FPS))
        .unwrap();
    let layer = scene
        .layer(&comp, "source", "footage", Some(&asset))
        .unwrap();
    scene.set(&layer, "stretch", Value::Float(200.0)).unwrap();
    scene
        .set(&layer, "frame_blending", Value::Enum("frame_mix".into()))
        .unwrap();

    let mut renderer = Renderer::new(gpu);
    assert_eq!(preview(&mut renderer, &scene, &comp, 1), RED.repeat(4));
    scene
        .set(&comp, "frame_blending", Value::Bool(true))
        .unwrap();
    // At 200% stretch comp frame 1 lies halfway between red and blue.
    let mixed = preview(&mut renderer, &scene, &comp, 1);
    assert!(
        mixed
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[0].abs_diff(128) <= 1
                && p[1] == 0
                && p[2].abs_diff(128) <= 1
                && p[3] == 255),
        "{mixed:?}"
    );
    let first = preview(&mut renderer, &scene, &comp, 0);
    let last = preview(&mut renderer, &scene, &comp, 2);
    assert!(
        first[0] > 235 && first[2] < 20 && last[2] > 235 && last[0] < 20,
        "{first:?} {last:?}"
    );
    assert_eq!(preview(&mut renderer, &scene, &comp, 3), BLUE.repeat(4));

    scene
        .set(&comp, "frame_blending", Value::Bool(false))
        .unwrap();
    assert_eq!(preview(&mut renderer, &scene, &comp, 1), RED.repeat(4));
    scene
        .set(&comp, "frame_blending", Value::Bool(true))
        .unwrap();
    assert_eq!(preview(&mut renderer, &scene, &comp, 1), mixed);
    scene
        .set(&layer, "frame_blending", Value::Enum("off".into()))
        .unwrap();
    assert_eq!(preview(&mut renderer, &scene, &comp, 1), RED.repeat(4));
    scene
        .set(&layer, "frame_blending", Value::Enum("frame_mix".into()))
        .unwrap();

    // A reversed time-remap plays the same frames in the opposite order.
    scene.set(&layer, "stretch", Value::Float(100.0)).unwrap();
    scene
        .set(&layer, "time_remap_enabled", Value::Bool(true))
        .unwrap();
    scene
        .key(
            &layer,
            "time_remap",
            Time::ZERO,
            Value::Time(FPS.frame_to_time(1)),
        )
        .unwrap();
    scene
        .key(
            &layer,
            "time_remap",
            FPS.frame_to_time(2),
            Value::Time(Time::ZERO),
        )
        .unwrap();
    assert_eq!(preview(&mut renderer, &scene, &comp, 0), last);
    assert_eq!(preview(&mut renderer, &scene, &comp, 1), mixed);
    assert_eq!(preview(&mut renderer, &scene, &comp, 2), first);
    let path = dir.path().join("export.png");
    let job = ExportJob {
        stage: scene.stage(),
        comp: &comp,
        times: &[(Some(1), FPS.frame_to_time(1))],
        rate: FPS,
        request: &RenderRequest {
            purpose: RenderPurpose::Final,
            ..Default::default()
        },
        settings: &ExportSettings::new(ExportFormat::Png, &path),
        encoders: None,
    };
    let out = export_frames(
        &mut renderer,
        &mut AnimEvaluator::new(None),
        &job,
        None,
        &mut |_| {},
    )
    .unwrap();
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(bae_render::png_out::read_rgba8(&path).unwrap().2, mixed);
}
