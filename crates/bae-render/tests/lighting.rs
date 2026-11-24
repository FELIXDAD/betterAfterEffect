//! Lit 3D layers on the GPU (docs/spec/ae/compositing-3d-color.md AE-CMP-429 …
//! 436) against the CPU reference `bae_render::light::shade`, plus the cases
//! that stay unlit and the evaluator path from light layers.

use std::sync::Arc;

use bae_gpu::GpuContext;
use bae_model::{PrimPath, Time, Value};
use bae_render::fixture::SceneBuilder;
use bae_render::light::{Falloff, Light, LightKind, Material, shade};
use bae_render::{
    AnimEvaluator, CompEvaluator, CompMotionBlur, LayerContent, LayerSample, OutputAlpha,
    RenderComp, RenderLayer, RenderRequest, Renderer,
};
use glam::{DAffine2, DMat4, DVec2, DVec3};

fn gpu() -> Option<Arc<GpuContext>> {
    bae_gpu::testing::shared_context()
}

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

struct Image {
    w: u32,
    data: Vec<u8>,
}

impl Image {
    fn px(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.w + x) * 4) as usize;
        self.data[i..i + 4].try_into().unwrap()
    }
}

fn render(r: &mut Renderer, comp: &RenderComp) -> Image {
    let frame = r.render_frame(comp, &RenderRequest::full()).unwrap();
    let img = r.read_rgba8(&frame, OutputAlpha::Straight).unwrap();
    Image {
        w: img.width,
        data: img.data,
    }
}

const COLOR: [f32; 3] = [0.8, 0.6, 0.4];

/// A 3D solid covering the 200×200 comp at z = 0 (or turned about y).
fn plane(rot_y: f64) -> RenderLayer {
    let mut l = RenderLayer::new(
        p("/comps/main/layers/plane"),
        LayerContent::Solid {
            color: [COLOR[0], COLOR[1], COLOR[2], 1.0],
            width: 200,
            height: 200,
        },
    );
    let m = DMat4::from_translation(DVec3::new(100.0, 100.0, 0.0))
        * DMat4::from_rotation_y(rot_y.to_radians())
        * DMat4::from_translation(DVec3::new(-100.0, -100.0, 0.0));
    l.matrix_3d = Some(m);
    l.transform = DAffine2::from_cols(
        m.x_axis.truncate().truncate(),
        m.y_axis.truncate().truncate(),
        m.w_axis.truncate().truncate(),
    );
    l
}

fn light(kind: LightKind, at: DVec3, aim: DVec3) -> Light {
    Light {
        kind,
        world: DMat4::from_translation(at) * bae_render::camera::look_at(aim - at),
        point_of_interest: aim,
        color: [1.0; 3],
        intensity: 100.0,
        cone_angle: 70.0,
        cone_feather: 40.0,
        falloff: Falloff::None,
        radius: 500.0,
        falloff_distance: 500.0,
        casts_shadows: false,
        shadow_darkness: 100.0,
        shadow_diffusion: 0.0,
        scope_below: None,
    }
}

fn lights() -> Vec<Light> {
    let centre = DVec3::new(100.0, 100.0, 0.0);
    let mut spot = light(
        LightKind::Spot,
        DVec3::new(40.0, 60.0, -120.0),
        DVec3::new(70.0, 90.0, 0.0),
    );
    spot.color = [1.0, 0.8, 0.6];
    spot.falloff = Falloff::Smooth;
    spot.radius = 80.0;
    spot.falloff_distance = 120.0;
    let mut point = light(LightKind::Point, DVec3::new(160.0, 150.0, -60.0), centre);
    point.intensity = 70.0;
    point.falloff = Falloff::InverseSquareClamped;
    point.radius = 60.0;
    let mut parallel = light(
        LightKind::Parallel,
        DVec3::new(0.0, 0.0, -100.0),
        DVec3::new(30.0, 10.0, 0.0),
    );
    parallel.intensity = 40.0;
    let mut ambient = light(LightKind::Ambient, DVec3::ZERO, centre);
    ambient.intensity = 15.0;
    vec![spot, point, parallel, ambient]
}

/// Largest channel difference between the GPU render and `shade` over the
/// interior of the plane (straight colour, 8 bit).
fn worst(img: &Image, comp: &RenderComp, layer: &RenderLayer, lights: &[Light]) -> u8 {
    let m = layer.matrix_3d.unwrap();
    let normal = m.x_axis.truncate().cross(m.y_axis.truncate());
    let eye = comp.view_camera().eye();
    let size = DVec2::splat(200.0);
    let cam = comp.view_camera();
    let h = bae_render::three_d::Homography(cam.plane_homography(&m, size));
    let inv = h.inverse().unwrap();
    let mut worst = 0;
    for y in (4..196).step_by(3) {
        for x in (4..196).step_by(3) {
            let Some(u) = inv.map(DVec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5)) else {
                continue;
            };
            if u.x < 2.0 || u.y < 2.0 || u.x > 198.0 || u.y > 198.0 {
                continue;
            }
            let point = m.transform_point3(u.extend(0.0));
            let c = shade(COLOR, point, normal, eye, &layer.material, lights);
            let want = c.map(|v| (v.min(1.0) * 255.0).round() as i32);
            let got = img.px(x, y);
            for i in 0..3 {
                worst = worst.max((i32::from(got[i]) - want[i]).unsigned_abs() as u8);
            }
        }
    }
    worst
}

#[test]
fn lights_match_the_reference_model() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    for (rot, material) in [
        (0.0, Material::default()),
        (
            35.0,
            Material {
                specular_intensity: 80.0,
                specular_shininess: 40.0,
                metal: 30.0,
                ..Material::default()
            },
        ),
        // Seen from behind: lit on the side facing the camera.
        (180.0, Material::default()),
    ] {
        let mut comp = RenderComp::new(p("/comps/main"), 200, 200);
        comp.lights = lights();
        let mut layer = plane(rot);
        layer.material = material;
        comp.layers.push(layer.clone());
        let img = render(&mut r, &comp);
        let w = worst(&img, &comp, &layer, &comp.lights);
        assert!(w <= 2, "rotation {rot}: max difference {w}");
    }
}

#[test]
fn unlit_cases() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 200, 200);
    comp.layers.push(plane(20.0));
    let unlit = render(&mut r, &comp);
    assert_eq!(unlit.px(100, 100), [204, 153, 102, 255]);
    // Accepts Lights off.
    comp.lights = lights();
    comp.layers[0].material.accepts_lights = false;
    assert_eq!(render(&mut r, &comp).data, unlit.data);
    // 2D layers are never lit.
    comp.layers[0].material.accepts_lights = true;
    comp.layers[0].matrix_3d = None;
    comp.layers[0].transform = DAffine2::IDENTITY;
    let flat = render(&mut r, &comp);
    assert_eq!(flat.px(100, 100), [204, 153, 102, 255]);
    // A light whose Adjustment Layer switch is on lights only the layers below
    // it: a layer above it that no light reaches stays unlit (AE 26.2.1).
    comp.layers[0] = plane(0.0);
    let mut only = light(
        LightKind::Point,
        DVec3::new(100.0, 100.0, -100.0),
        DVec3::new(100.0, 100.0, 0.0),
    );
    only.scope_below = Some(1);
    comp.lights = vec![only];
    assert_eq!(render(&mut r, &comp).px(100, 100), [204, 153, 102, 255]);
    comp.lights[0].scope_below = None;
    let lit = render(&mut r, &comp).px(100, 100);
    // Diffuse 50 % + specular 50 % of a light straight ahead.
    assert!(lit[0] > 200 && lit[3] == 255, "{lit:?}");
}

#[test]
fn bright_light_clips_in_8_bpc() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 200, 200);
    let mut l = light(
        LightKind::Point,
        DVec3::new(100.0, 100.0, -100.0),
        DVec3::new(100.0, 100.0, 0.0),
    );
    l.intensity = 400.0;
    comp.lights = vec![l];
    comp.layers.push(plane(0.0));
    assert_eq!(render(&mut r, &comp).px(100, 100), [255, 255, 255, 255]);
}

#[test]
fn lit_layers_can_be_motion_blurred() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 200, 200);
    comp.lights = lights();
    let mut layer = plane(0.0);
    layer.content = LayerContent::Solid {
        color: [COLOR[0], COLOR[1], COLOR[2], 1.0],
        width: 60,
        height: 60,
    };
    let at = |x: f64| DMat4::from_translation(DVec3::new(x, 70.0, 0.0));
    layer.matrix_3d = Some(at(70.0));
    layer.motion_samples = (0..8)
        .map(|k| LayerSample {
            transform: DAffine2::IDENTITY,
            matrix_3d: Some(at(60.0 + 20.0 * f64::from(k) / 8.0)),
        })
        .collect();
    comp.motion_blur = Some(Arc::new(CompMotionBlur {
        times: (0..8).map(Time::from_flicks).collect(),
        cameras: vec![None; 8],
        adaptive_limit: 128,
    }));
    comp.layers.push(layer);
    let img = render(&mut r, &comp);
    // Blurred edge, lit interior (not the flat layer colour).
    let edge = img.px(65, 100)[3];
    assert!(edge > 0 && edge < 255, "{edge}");
    assert_ne!(img.px(100, 100), [204, 153, 102, 255]);
    assert_eq!(img.px(100, 100)[3], 255);
}

#[test]
fn evaluator_passes_lights_and_materials() {
    let Some(gpu) = gpu() else { return };
    let mut b = SceneBuilder::new();
    let comp = b
        .comp(
            "main",
            200,
            200,
            bae_model::FrameRate::FPS_30,
            Time::ONE_SECOND,
        )
        .unwrap();
    let light = b.layer(&comp, "light", "light", None).unwrap();
    b.set(&light, "light:type", Value::Enum("point".into()))
        .unwrap();
    b.set(
        &light,
        "transform:position",
        Value::Vec3([100.0, 100.0, -100.0]),
    )
    .unwrap();
    let layer = b
        .solid_layer(&comp, "a", [1.0, 1.0, 1.0, 1.0], [200, 200], [0.0, 0.0])
        .unwrap();
    b.set(&layer, "three_d", Value::Bool(true)).unwrap();
    for (attr, v) in [
        ("material:ambient", 0.0),
        ("material:diffuse", 100.0),
        ("material:specular_intensity", 0.0),
    ] {
        b.set(&layer, attr, Value::Float(v)).unwrap();
    }
    let stage = b.into_stage();
    let mut eval = AnimEvaluator::new(None);
    let rc = eval.evaluate(&stage, &comp, Time::ZERO).unwrap();
    assert_eq!(rc.lights.len(), 1);
    let a = rc.layers.iter().find(|l| l.path == layer).unwrap();
    assert_eq!(a.material.diffuse, 100.0);
    let mut r = Renderer::new(gpu);
    let img = render(&mut r, &rc);
    // AE `pt_near`: straight ahead 255, the corner 148 (Lambert).
    assert_eq!(img.px(100, 100)[0], 255);
    assert!(
        (i32::from(img.px(0, 0)[0]) - 148).abs() <= 1,
        "{:?}",
        img.px(0, 0)
    );
}
