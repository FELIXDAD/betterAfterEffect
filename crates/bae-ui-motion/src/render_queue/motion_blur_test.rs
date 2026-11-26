use super::*;
use crate::common::EvalCache;
use crate::gpu_preview::GpuPreview;
use bae_gpu::viewport::ViewportTextureRegistry;
use bae_model::{AuthorId, CommandRegistry, Keyframe, Value};
use bae_render::fixture::SceneBuilder;
use serde_json::json;

fn alpha(rgba: &[u8]) -> Vec<u8> {
    let (pixels, _) = rgba.as_chunks::<4>();
    pixels.iter().map(|pixel| pixel[3]).collect()
}

fn export_alpha(gpu: &Arc<GpuContext>, stage: &Stage, comp: &PrimPath, frame: i64) -> Vec<u8> {
    export_with_switches(gpu, stage, comp, frame, true)
}

fn export_with_switches(
    gpu: &Arc<GpuContext>,
    stage: &Stage,
    comp: &PrimPath,
    frame: i64,
    nested_switches: bool,
) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("frame.png");
    let queue = SharedRenderQueue::default();
    {
        let mut q = queue.lock();
        let id = q.add(
            &bae_motion::query::comp_info(stage, comp).unwrap(),
            dir.path(),
        );
        let item = q.item_mut(id).unwrap();
        item.set_format(bae_render::ExportFormat::Png);
        item.output = file.to_string_lossy().into_owned();
        item.channels = Channels::RgbAlpha;
        (item.start, item.end) = (frame, frame);
        assert!(q.begin());
    }
    run_worker(
        &queue,
        &crate::preview_snapshot::PreviewSnapshot::take(stage),
        gpu.clone(),
        &[],
        None,
        nested_switches,
    );
    let (w, h, rgba) = bae_render::png_out::read_rgba8(&file).unwrap();
    assert_eq!((w, h), (120, 40));
    alpha(&rgba)
}

#[test]
fn motion_blur_preview_and_export() {
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        return;
    };
    let rate = FrameRate::FPS_30;
    let mut scene = SceneBuilder::new();
    let comp = scene.comp("main", 120, 40, rate, Time::ONE_SECOND).unwrap();
    let layer = scene
        .solid_layer(&comp, "white", [1.0; 4], [20, 20], [40.0, 10.0])
        .unwrap();
    for path in [&comp, &layer] {
        scene.set(path, "motion_blur", Value::Bool(true)).unwrap();
    }
    let mut stage = scene.into_stage();
    let resources = Resources::new();
    let registry = Arc::new(ViewportTextureRegistry::new());
    let mut preview = GpuPreview::with(gpu.clone(), registry.clone());
    registry.set_display_handle(preview.viewport(), Some(1));
    let mut eval = EvalCache::default();
    let mut preview_alpha = |stage: &Stage, frame| {
        let ec = eval
            .get(stage)
            .evaluate_comp(stage, &comp, rate.frame_to_time(frame))
            .unwrap();
        preview
            .frame(&resources, stage, &ec, 1, None, false)
            .unwrap();
        let published = registry.get(preview.viewport()).unwrap();
        let pixels = bae_gpu::transfer::read_texture(&gpu, published.view.texture()).unwrap();
        alpha(&pixels.data)
    };
    let partial = |row: &[u8]| row.iter().filter(|&a| (1..255).contains(a)).count();
    let still = preview_alpha(&stage, 1);
    assert_eq!(still, export_alpha(&gpu, &stage, &comp, 1));
    assert_eq!(partial(&still), 0);
    assert_eq!(still.iter().filter(|&&a| a == 255).count(), 20 * 20);

    // Keep the middle frame's position unchanged while adding 30 px/frame of motion.
    let commands = CommandRegistry::with_builtins();
    let author = AuthorId::new("user:animator").unwrap();
    let edit = |stage: &mut Stage, id, params| {
        commands.execute(stage, &author, id, params).unwrap();
    };
    for (frame, x) in [(0, 20.0), (2, 80.0)] {
        let key = Keyframe::linear(Value::Vec3([x, 20.0, 0.0]));
        edit(
            &mut stage,
            "key.set",
            json!({"path": layer, "attr": "transform:position", "time": rate.frame_to_time(frame).flicks(), "keyframe": key}),
        );
    }
    let moving = export_alpha(&gpu, &stage, &comp, 1);
    let middle_row = &moving[20 * 120..21 * 120];
    assert_eq!(partial(&middle_row[..50]), 15);
    assert_eq!(partial(&middle_row[50..]), 15);
    assert_eq!(preview_alpha(&stage, 1), moving, "immediately after keying");
    for (comp_on, layer_on) in [(false, false), (true, false), (false, true), (true, true)] {
        for (path, on) in [(&comp, comp_on), (&layer, layer_on)] {
            edit(
                &mut stage,
                "attr.set",
                json!({"path": path, "attr": "motion_blur", "value": Value::Bool(on)}),
            );
        }
        let preview = preview_alpha(&stage, 1);
        let png = export_alpha(&gpu, &stage, &comp, 1);
        let expected = if comp_on && layer_on { &moving } else { &still };
        assert_eq!(&png, expected, "PNG: comp={comp_on} layer={layer_on}");
        assert_eq!(preview, png, "preview: comp={comp_on} layer={layer_on}");
    }
    assert_eq!(
        preview_alpha(&stage, 0),
        export_alpha(&gpu, &stage, &comp, 0)
    );
    assert_eq!(preview_alpha(&stage, 1), moving, "forward playback");
    assert_eq!(
        preview_alpha(&stage, 2),
        export_alpha(&gpu, &stage, &comp, 2)
    );
    assert_eq!(preview_alpha(&stage, 1), moving, "backward seek");

    edit(
        &mut stage,
        "key.remove",
        json!({"path": layer, "attr": "transform:position", "time": rate.frame_to_time(2).flicks()}),
    );
    let stopped = export_alpha(&gpu, &stage, &comp, 1);
    assert_eq!(partial(&stopped), 0);
    assert_eq!(stopped[20 * 120 + 20], 255);
    assert_eq!(
        preview_alpha(&stage, 1),
        stopped,
        "after deleting the end key"
    );
    stage.undo(&author).unwrap();
    assert_eq!(export_alpha(&gpu, &stage, &comp, 1), moving);
    assert_eq!(preview_alpha(&stage, 1), moving, "undo");
    stage.redo(&author).unwrap();
    assert_eq!(export_alpha(&gpu, &stage, &comp, 1), stopped);
    assert_eq!(preview_alpha(&stage, 1), stopped, "redo");
}

#[test]
fn nested_switches_update_the_same_preview_frame_and_exported_png() {
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        return;
    };
    let rate = FrameRate::FPS_30;
    let mut scene = SceneBuilder::new();
    let outer = scene
        .comp("outer", 120, 40, rate, Time::ONE_SECOND)
        .unwrap();
    let middle = scene
        .comp("middle", 120, 40, rate, Time::ONE_SECOND)
        .unwrap();
    let inner = scene
        .comp("inner", 120, 40, rate, Time::ONE_SECOND)
        .unwrap();
    let top = scene
        .layer(&outer, "Middle", "pre_comp", Some(&middle))
        .unwrap();
    let sub = scene
        .layer(&middle, "Inner", "pre_comp", Some(&inner))
        .unwrap();
    let solid = scene
        .solid_layer(&inner, "moving", [1.; 4], [20, 20], [20., 10.])
        .unwrap();
    for item in [&outer, &middle, &inner, &sub, &solid] {
        scene.set(item, "motion_blur", Value::Bool(true)).unwrap();
    }
    scene.set(&top, "motion_blur", Value::Bool(false)).unwrap();
    for (frame, x) in [(0, 20.), (2, 80.)] {
        scene
            .key(
                &solid,
                "transform:position",
                rate.frame_to_time(frame),
                Value::Vec3([x, 20., 0.]),
            )
            .unwrap();
    }
    let stage = scene.into_stage();
    let mut resources = Resources::new();
    let prefs = Arc::new(bae_prefs::PrefsStore::new(bae_prefs::Preferences::default()));
    resources.insert(prefs.clone());
    let registry = Arc::new(ViewportTextureRegistry::new());
    let mut preview = GpuPreview::with(gpu.clone(), registry.clone());
    registry.set_display_handle(preview.viewport(), Some(1));
    let ec = bae_anim::evaluate_comp(&stage, &outer, rate.frame_to_time(1)).unwrap();
    for enabled in [true, false, true] {
        let mut settings = (*prefs.current()).clone();
        settings.set("composition.switches_affect_nested_comps", enabled);
        prefs.set(settings).unwrap();
        preview
            .frame(&resources, &stage, &ec, 1, None, false)
            .unwrap();
        let published = registry.get(preview.viewport()).unwrap();
        let pixels = bae_gpu::transfer::read_texture(&gpu, published.view.texture()).unwrap();
        let shown = alpha(&pixels.data);
        assert!(shown.iter().any(|&a| a > 0));
        assert_eq!(shown.iter().any(|&a| (1..255).contains(&a)), !enabled);
        assert_eq!(
            shown,
            export_with_switches(&gpu, &stage, &outer, 1, enabled)
        );
    }
}
