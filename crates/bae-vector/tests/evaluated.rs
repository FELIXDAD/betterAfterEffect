//! End to end: a shape layer built with model commands, evaluated by bae-anim
//! (03), turned into a display list and drawn with Vello.

use std::sync::Arc;

use bae_anim::evaluate_comp;
use bae_gpu::GpuContext;
use bae_model::{AuthorId, CommandRegistry, FrameRate, Keyframe, PrimPath, Stage, Time, Value};
use bae_render::fixture::SceneBuilder;
use bae_render::{IRect, LayerSource, SourceRequest, testing};
use bae_vector::render::ShapeSource;
use bae_vector::{DrawItem, shape_layer_scene};
use serde_json::json;

struct Project {
    stage: Stage,
    registry: CommandRegistry,
    author: AuthorId,
}

impl Project {
    fn run(&mut self, id: &str, params: serde_json::Value) {
        self.registry
            .execute(&mut self.stage, &self.author, id, params)
            .unwrap_or_else(|e| panic!("{id}: {e}"));
    }

    fn create(&mut self, path: &str, prim_type: &str) -> PrimPath {
        self.run("prim.create", json!({"path": path, "prim_type": prim_type}));
        PrimPath::new(path).unwrap()
    }

    fn set(&mut self, prim: &PrimPath, attr: &str, value: Value) {
        let value = serde_json::to_value(&value).unwrap();
        self.run(
            "attr.set",
            json!({"path": prim.as_str(), "attr": attr, "value": value}),
        );
    }

    fn key(&mut self, prim: &PrimPath, attr: &str, time: Time, value: Value) {
        let keyframe = serde_json::to_value(Keyframe::linear(value)).unwrap();
        self.run(
            "key.set",
            json!({"path": prim.as_str(), "attr": attr, "time": time.flicks(), "keyframe": keyframe}),
        );
    }
}

/// A 200×200 comp with one shape layer at the comp centre holding a group with
/// an ellipse, a red fill, a white 8 px stroke and a Trim Paths whose End goes
/// 0 → 100 % over one second.
fn project() -> (Project, PrimPath, PrimPath) {
    let mut b = SceneBuilder::new();
    let comp = b
        .comp(
            "main",
            200,
            200,
            FrameRate::FPS_30,
            Time::from_flicks(2 * Time::FLICKS_PER_SECOND),
        )
        .unwrap();
    let layer = b.layer(&comp, "shape", "shape", None).unwrap();
    b.set(
        &layer,
        "transform:position",
        Value::Vec3([100.0, 100.0, 0.0]),
    )
    .unwrap();
    let mut p = Project {
        stage: b.into_stage(),
        registry: CommandRegistry::with_builtins(),
        author: AuthorId::system(),
    };
    let l = layer.as_str().to_owned();
    p.create(&format!("{l}/contents"), "Folder");
    p.create(&format!("{l}/contents/ellipse_group"), "ShapeGroup");
    let ellipse = p.create(&format!("{l}/contents/ellipse_group/ellipse"), "Ellipse");
    p.set(&ellipse, "size", Value::Vec2([120.0, 120.0]));
    let fill = p.create(&format!("{l}/contents/ellipse_group/fill"), "Fill");
    p.set(&fill, "color", Value::Color([1.0, 0.0, 0.0, 1.0]));
    let stroke = p.create(&format!("{l}/contents/ellipse_group/stroke"), "Stroke");
    p.set(&stroke, "width", Value::Float(8.0));
    p.set(&stroke, "composite", Value::Enum("above".into()));
    let trim = p.create(&format!("{l}/contents/ellipse_group/trim"), "ShapeOperator");
    p.set(&trim, "operator", Value::Enum("trim".into()));
    p.key(&trim, "trim:end", Time::ZERO, Value::Float(0.0));
    p.key(
        &trim,
        "trim:end",
        Time::from_flicks(Time::FLICKS_PER_SECOND),
        Value::Float(100.0),
    );
    (p, comp, layer)
}

#[test]
fn evaluated_contents_become_a_display_list() {
    let (p, comp, layer) = project();
    let half = Time::from_flicks(Time::FLICKS_PER_SECOND / 2);
    let evaluated = evaluate_comp(&p.stage, &comp, half).unwrap();
    let l = evaluated.layer(&layer).unwrap();
    let mut warnings = Vec::new();
    let scene = shape_layer_scene(l, &mut warnings);
    assert!(warnings.is_empty(), "{warnings:?}");
    // Fill (below) is drawn first, the stroke composited above it.
    assert_eq!(scene.items.len(), 2);
    assert!(matches!(scene.items[0], DrawItem::Fill(_)));
    let DrawItem::Stroke(s) = &scene.items[1] else {
        panic!("expected the stroke in front")
    };
    // Trim End is 50 % at 0.5 s: half the circumference, from 12 o'clock to 6.
    let len = bae_vector::ops::path_length(&s.path);
    let full = 2.0 * std::f64::consts::PI * 60.0;
    assert!((len - full / 2.0).abs() < 1.0, "{len} vs {}", full / 2.0);
    // At 0 s the trimmed paths draw nothing at all.
    let start = evaluate_comp(&p.stage, &comp, Time::ZERO).unwrap();
    let scene0 = shape_layer_scene(start.layer(&layer).unwrap(), &mut warnings);
    assert!(scene0.is_empty());
}

#[test]
fn evaluated_shape_layer_renders_on_the_gpu() {
    let Some(gpu): Option<Arc<GpuContext>> = bae_gpu::testing::shared_context() else {
        return;
    };
    // Vello's compute pipelines crash the process (STATUS_ACCESS_VIOLATION) on WARP,
    // the software D3D12 adapter of GPU-less Windows CI runners. Skip software
    // adapters like the no-GPU case; making the render path itself refuse CPU adapters
    // is tracked in 05's task book.
    if gpu.adapter_info().device_type == vello::wgpu::DeviceType::Cpu {
        eprintln!(
            "skipping Vello GPU test on software adapter {:?}",
            gpu.adapter_info().name
        );
        return;
    }
    let (p, comp, layer) = project();
    let evaluated = evaluate_comp(
        &p.stage,
        &comp,
        Time::from_flicks(Time::FLICKS_PER_SECOND / 2),
    )
    .unwrap();
    let scene = shape_layer_scene(evaluated.layer(&layer).unwrap(), &mut Vec::new());
    let source = ShapeSource::new(Arc::new(scene));
    let roi = IRect::new(-70, -70, 70, 70);
    let request = SourceRequest {
        roi,
        scale: 1.0,
        time: Time::ZERO,
    };
    let image = testing::render_source(&gpu, &source, &request).unwrap();
    let px = testing::read_pixels(&gpu, &image).unwrap();
    let rect = roi.intersect(&source.bounds());
    let width = image.texel_size()[0] as i32;
    let at = |x: i32, y: i32| {
        px[((y - rect.y0) * width + (x - rect.x0)) as usize].map(|v| (v * 255.0).round() as u8)
    };
    // The fill is above the Trim Paths too, so it fills the trimmed half
    // (closed by a straight chord along x = 0).
    assert_eq!(at(20, 0), [255, 0, 0, 255], "fill of the right half");
    assert_eq!(
        at(60, 0),
        [255, 255, 255, 255],
        "stroke in front of the fill"
    );
    assert_eq!(at(-15, 0)[3], 0, "nothing on the left");
}

#[test]
fn ae_match_names_drive_animated_shape_operators_through_commands() {
    let (mut p, comp, layer) = project();
    let path = format!("{}/contents/ellipse_group/offset", layer.as_str());
    let operator = p.create(&path, "ShapeOperator");
    p.set(
        &operator,
        "match_name",
        Value::String("ADBE Vector Filter - Offset".into()),
    );
    let attr = bae_vector::operators::parameter_attr("ADBE Vector Offset Amount");
    p.key(&operator, &attr, Time::ZERO, Value::Float(0.0));
    p.key(
        &operator,
        &attr,
        Time::from_seconds(1.0),
        Value::Float(20.0),
    );
    // Use the untrimmed frame, with both paints above the newly created operator.
    let comp = evaluate_comp(&p.stage, &comp, Time::from_seconds(1.0)).unwrap();
    let mut warnings = vec![];
    let scene = shape_layer_scene(comp.layer(&layer).unwrap(), &mut warnings);
    assert!(warnings.is_empty(), "{warnings:?}");
    let bounds = scene.bounds(false).unwrap();
    assert!((bounds.width() - 160.0).abs() < 0.1, "{bounds:?}");
}
