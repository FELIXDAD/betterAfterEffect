//! 3D layers on the GPU (docs/spec/ae/compositing-3d-color.md AE-CMP-400 … 406):
//! projection through AE's camera against a CPU ray-cast reference, depth order,
//! group splitting by 2D layers, intersections, camera layers and near-plane
//! clipping.

use std::sync::Arc;

use bae_gpu::GpuContext;
use bae_model::{PrimPath, Time, Value};
use bae_render::camera::Camera;
use bae_render::fixture::SceneBuilder;
use bae_render::{
    AnimEvaluator, CompEvaluator, LayerContent, OutputAlpha, RenderComp, RenderLayer,
    RenderRequest, Renderer,
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
    h: u32,
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
        h: img.height,
        data: img.data,
    }
}

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

fn solid(name: &str, color: [f32; 4], size: [u32; 2]) -> RenderLayer {
    RenderLayer::new(
        p(&format!("/comps/main/layers/{name}")),
        LayerContent::Solid {
            color,
            width: size[0],
            height: size[1],
        },
    )
}

/// A 3D solid with its centre (anchor) at `centre`, rotated by `rotation`
/// (degrees, AE's X/Y/Z Rotation order).
fn solid_3d(
    name: &str,
    color: [f32; 4],
    size: [u32; 2],
    centre: DVec3,
    rotation: DVec3,
) -> RenderLayer {
    let mut l = solid(name, color, size);
    let half = DVec3::new(f64::from(size[0]) / 2.0, f64::from(size[1]) / 2.0, 0.0);
    let m = DMat4::from_translation(centre)
        * bae_render::camera::xyz_rotation(rotation)
        * DMat4::from_translation(-half);
    l.matrix_3d = Some(m);
    l.transform = DAffine2::from_cols(
        m.x_axis.truncate().truncate(),
        m.y_axis.truncate().truncate(),
        m.w_axis.truncate().truncate(),
    );
    l
}

/// CPU reference: whether the pixel centre (x + ½, y + ½) sees the layer, by
/// casting the ray through the pixel onto the layer plane.
fn sees(comp: &RenderComp, layer: &RenderLayer, x: f64, y: f64) -> bool {
    let cam = comp.view_camera();
    let size = DVec2::new(f64::from(comp.width), f64::from(comp.height));
    let m = layer.matrix_3d.unwrap();
    let origin = cam.position();
    let dir = cam.unproject(DVec2::new(x, y), 1.0, size) - origin;
    // Plane: m · (u, v, 0, 1).
    let n = m.x_axis.truncate().cross(m.y_axis.truncate());
    let p0 = m.w_axis.truncate();
    let t = n.dot(p0 - origin) / n.dot(dir);
    if t <= 0.0 {
        return false;
    }
    let hit = origin + dir * t;
    let local = m.inverse().transform_point3(hit);
    let LayerContent::Solid { width, height, .. } = layer.content else {
        unreachable!()
    };
    local.x >= 0.0 && local.y >= 0.0 && local.x < f64::from(width) && local.y < f64::from(height)
}

/// Compares every pixel whose 2-pixel footprint (3×3 samples around the
/// centre) is fully in or fully out of the layer with the reference; edge
/// pixels are skipped. Returns (checked, mismatches).
fn compare_interior(
    img: &Image,
    comp: &RenderComp,
    layer: &RenderLayer,
    color: [u8; 4],
) -> (u32, u32) {
    let (mut checked, mut bad) = (0, 0);
    for y in 0..img.h {
        for x in 0..img.w {
            let mut inside = 0;
            for oy in [-0.5, 0.5, 1.5] {
                for ox in [-0.5, 0.5, 1.5] {
                    inside += u32::from(sees(comp, layer, f64::from(x) + ox, f64::from(y) + oy));
                }
            }
            let expect = match inside {
                9 => color,
                0 => [0, 0, 0, 0],
                _ => continue,
            };
            checked += 1;
            let got = img.px(x, y);
            if got.iter().zip(expect).any(|(a, b)| a.abs_diff(b) > 1) {
                bad += 1;
            }
        }
    }
    (checked, bad)
}

#[test]
fn flat_3d_layer_through_default_camera_equals_2d() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut flat = RenderComp::new(p("/comps/main"), 96, 64);
    let mut l = solid("a", [0.2, 0.6, 1.0, 0.8], [30, 17]);
    l.transform = DAffine2::from_translation(DVec2::new(40.3, 20.6))
        * DAffine2::from_angle(0.4)
        * DAffine2::from_scale(DVec2::new(1.3, 0.7));
    flat.layers.push(l.clone());
    let mut three = flat.clone();
    let t = l.transform;
    three.layers[0].matrix_3d = Some(DMat4::from_cols(
        t.matrix2.x_axis.extend(0.0).extend(0.0),
        t.matrix2.y_axis.extend(0.0).extend(0.0),
        DVec3::Z.extend(0.0),
        t.translation.extend(0.0).extend(1.0),
    ));
    let a = render(&mut r, &flat);
    let b = render(&mut r, &three);
    assert_eq!(
        a.data, b.data,
        "AE-CMP-400: z = 0 through the default camera is 2D"
    );
}

#[test]
fn doubled_distance_halves_the_size() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 100, 100);
    let zoom = Camera::default_for(100, 100).zoom;
    comp.layers.push(solid_3d(
        "a",
        RED,
        [40, 40],
        DVec3::new(50.0, 50.0, zoom),
        DVec3::ZERO,
    ));
    let img = render(&mut r, &comp);
    // 40 px at twice the distance: 20 px centred, [40, 60).
    assert_eq!(img.px(40, 40), [255, 0, 0, 255]);
    assert_eq!(img.px(59, 59), [255, 0, 0, 255]);
    assert_eq!(img.px(39, 50)[3], 0);
    assert_eq!(img.px(60, 50)[3], 0);
}

#[test]
fn rotated_layers_match_ray_cast_reference() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    for rotation in [
        DVec3::new(0.0, 60.0, 0.0),
        DVec3::new(-50.0, 0.0, 20.0),
        DVec3::new(30.0, -40.0, 10.0),
        // A floor seen from above the horizon (X rotation 85°).
        DVec3::new(85.0, 0.0, 0.0),
    ] {
        let mut comp = RenderComp::new(p("/comps/main"), 160, 120);
        let layer = solid_3d(
            "a",
            GREEN,
            [100, 80],
            DVec3::new(80.0, 60.0, 30.0),
            rotation,
        );
        comp.layers.push(layer.clone());
        let img = render(&mut r, &comp);
        let (checked, bad) = compare_interior(&img, &comp, &layer, [0, 255, 0, 255]);
        assert!(checked > 10_000, "{rotation}: {checked}");
        assert_eq!(bad, 0, "{rotation}: {bad} of {checked} pixels differ");
    }
}

#[test]
fn depth_order_beats_stacking_order() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 100, 100);
    // Top of the stack but farther away.
    comp.layers.push(solid_3d(
        "far",
        RED,
        [40, 40],
        DVec3::new(45.0, 50.0, 100.0),
        DVec3::ZERO,
    ));
    // Bottom of the stack, nearer.
    comp.layers.push(solid_3d(
        "near",
        BLUE,
        [40, 40],
        DVec3::new(55.0, 50.0, -100.0),
        DVec3::ZERO,
    ));
    let img = render(&mut r, &comp);
    assert_eq!(img.px(50, 50), [0, 0, 255, 255], "nearer layer in front");

    // A 2D layer between them splits the group: stacking order again.
    comp.layers.insert(1, {
        let mut l = solid("flat", GREEN, [4, 4]);
        l.transform = DAffine2::from_translation(DVec2::new(0.0, 0.0));
        l
    });
    let img = render(&mut r, &comp);
    assert_eq!(
        img.px(50, 50),
        [255, 0, 0, 255],
        "AE-CMP-406: groups composite by stack"
    );
    assert_eq!(img.px(1, 1), [0, 255, 0, 255]);
}

#[test]
fn coplanar_layers_keep_stacking_order() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 64, 64);
    let centre = DVec3::new(32.0, 32.0, 50.0);
    let rot = DVec3::new(0.0, 30.0, 0.0);
    comp.layers
        .push(solid_3d("top", RED, [20, 20], centre, rot));
    comp.layers
        .push(solid_3d("bottom", BLUE, [40, 40], centre, rot));
    let img = render(&mut r, &comp);
    assert_eq!(img.px(32, 32), [255, 0, 0, 255]);
}

#[test]
fn intersecting_layers_cut_each_other() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 120, 80);
    let centre = DVec3::new(60.0, 40.0, 0.0);
    // Flat red on top of the stack; green turned 45° about y through the same
    // centre: its left half recedes behind the red, its right half comes forward.
    comp.layers
        .push(solid_3d("flat", RED, [80, 40], centre, DVec3::ZERO));
    comp.layers.push(solid_3d(
        "turned",
        GREEN,
        [80, 60],
        centre,
        DVec3::new(0.0, 45.0, 0.0),
    ));
    let img = render(&mut r, &comp);
    assert_eq!(img.px(45, 40), [255, 0, 0, 255], "left: red in front");
    assert_eq!(img.px(75, 40), [0, 255, 0, 255], "right: green in front");
    // Above the red, the receding green shows on both sides.
    assert_eq!(img.px(50, 15)[1], 255);
    // The cut is anti-aliased without a seam: no transparent pixels on it.
    for y in 25..55 {
        assert_eq!(img.px(60, y)[3], 255, "seam at y = {y}");
    }
}

#[test]
fn camera_layer_moves_the_view() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 100, 100);
    comp.layers.push(solid_3d(
        "a",
        RED,
        [20, 20],
        DVec3::new(50.0, 50.0, 0.0),
        DVec3::ZERO,
    ));
    let zoom = Camera::default_for(100, 100).zoom;
    // Same zoom, 30 px to the right: the layer appears 30 px to the left.
    comp.camera = Some(Camera::new(
        DMat4::from_translation(DVec3::new(80.0, 50.0, -zoom)),
        zoom,
    ));
    let img = render(&mut r, &comp);
    assert_eq!(img.px(10, 50), [255, 0, 0, 255]);
    assert_eq!(img.px(29, 50), [255, 0, 0, 255]);
    assert_eq!(img.px(31, 50)[3], 0);
    // Rolled 90°: the 20×20 square stays put, a wide bar turns upright.
    comp.layers[0] = solid_3d("a", RED, [60, 10], DVec3::new(50.0, 50.0, 0.0), DVec3::ZERO);
    comp.camera = Some(Camera::from_transform(
        DMat4::IDENTITY,
        DVec3::new(50.0, 50.0, -zoom),
        None,
        DVec3::ZERO,
        DVec3::new(0.0, 0.0, 90.0),
        zoom,
    ));
    let img = render(&mut r, &comp);
    assert_eq!(img.px(50, 25), [255, 0, 0, 255]);
    assert_eq!(img.px(25, 50)[3], 0);
}

#[test]
fn geometry_behind_the_camera_is_clipped() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 100, 100);
    let zoom = Camera::default_for(100, 100).zoom;
    // A long floor 30 px below the camera, from far behind it to far ahead.
    let layer = solid_3d(
        "floor",
        GREEN,
        [200, 20_000],
        DVec3::new(50.0, 80.0, -zoom),
        DVec3::new(90.0, 0.0, 0.0),
    );
    comp.layers.push(layer.clone());
    let img = render(&mut r, &comp);
    // Ground below the horizon (y = 50), sky above: no wrap-around.
    assert_eq!(img.px(50, 95), [0, 255, 0, 255]);
    for y in 0..48 {
        assert_eq!(img.px(50, y)[3], 0, "sky at y = {y}");
    }
    let (checked, bad) = compare_interior(&img, &comp, &layer, [0, 255, 0, 255]);
    assert!(checked > 5_000);
    assert_eq!(bad, 0);
}

#[test]
fn evaluator_feeds_3d_layers_and_the_camera() {
    let Some(gpu) = gpu() else { return };
    let mut b = SceneBuilder::new();
    let comp = b
        .comp(
            "main",
            100,
            100,
            bae_model::FrameRate::FPS_30,
            Time::ONE_SECOND,
        )
        .unwrap();
    let cam = b.layer(&comp, "cam", "camera", None).unwrap();
    let zoom = Camera::default_for(100, 100).zoom;
    b.set(&cam, "camera:zoom", Value::Float(zoom)).unwrap();
    b.set(&cam, "camera:type", Value::Enum("one_node".into()))
        .unwrap();
    b.set(&cam, "transform:position", Value::Vec3([80.0, 50.0, -zoom]))
        .unwrap();
    let layer = b
        .solid_layer(&comp, "a", [1.0, 0.0, 0.0, 1.0], [20, 20], [40.0, 40.0])
        .unwrap();
    b.set(&layer, "three_d", Value::Bool(true)).unwrap();
    let stage = b.into_stage();
    let mut eval = AnimEvaluator::new(None);
    let rc = eval.evaluate(&stage, &comp, Time::ZERO).unwrap();
    assert!(rc.camera.is_some(), "the camera layer is the active camera");
    let a = rc.layers.iter().find(|l| l.path == layer).unwrap();
    assert!(a.matrix_3d.is_some());
    let mut r = Renderer::new(gpu);
    let img = render(&mut r, &rc);
    // Camera 30 px to the right: the square at [40, 60) shows at [10, 30).
    assert_eq!(img.px(20, 50), [255, 0, 0, 255]);
    assert_eq!(img.px(50, 50)[3], 0);
}

#[test]
fn orthographic_views_draw_without_perspective() {
    use bae_render::camera::OrthoView;
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 100, 100);
    // Far behind the default camera (it would not be drawn in perspective),
    // and a nearer layer that must cover it where they overlap.
    comp.layers.push(solid_3d(
        "far",
        RED,
        [40, 40],
        DVec3::new(40.0, 50.0, 5000.0),
        DVec3::ZERO,
    ));
    comp.layers.push(solid_3d(
        "near",
        BLUE,
        [40, 40],
        DVec3::new(60.0, 50.0, -5000.0),
        DVec3::ZERO,
    ));
    let centre = DVec3::new(50.0, 50.0, 0.0);
    comp.camera = Some(Camera::orthographic(OrthoView::Front, centre, 1.0));
    let img = render(&mut r, &comp);
    // 40 px squares at any depth, nearer (blue) in front.
    assert_eq!(img.px(25, 50), [255, 0, 0, 255]);
    assert_eq!(img.px(50, 50), [0, 0, 255, 255]);
    assert_eq!(img.px(79, 50), [0, 0, 255, 255]);
    assert_eq!(img.px(80, 50)[3], 0);
    // From the top, a floor (X rotation 90°) is its full rectangle: 60 wide,
    // 40 deep, the front edge (z = −20) at the bottom.
    comp.layers = vec![solid_3d(
        "floor",
        GREEN,
        [60, 40],
        DVec3::new(50.0, 80.0, 0.0),
        DVec3::new(90.0, 0.0, 0.0),
    )];
    comp.camera = Some(Camera::orthographic(OrthoView::Top, centre, 1.0));
    let img = render(&mut r, &comp);
    assert_eq!(img.px(20, 30), [0, 255, 0, 255]);
    assert_eq!(img.px(79, 69), [0, 255, 0, 255]);
    assert_eq!(img.px(19, 50)[3], 0);
    assert_eq!(img.px(50, 70)[3], 0);
    assert_eq!(img.px(50, 29)[3], 0);
}

#[test]
fn cuts_leave_no_see_through_seams() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 100, 100);
    // A small layer turned 60° about y crosses a wide flat one; its plane cuts
    // the flat layer along a line that runs on past the small layer, where
    // nothing covers the flat layer. (The small layer is at the bottom of the
    // stack, so its plane is the one that cuts.)
    comp.layers.push(solid_3d(
        "wide",
        RED,
        [80, 30],
        DVec3::new(50.0, 50.0, 0.0),
        DVec3::ZERO,
    ));
    comp.layers.push(solid_3d(
        "small",
        GREEN,
        [10, 10],
        DVec3::new(30.0, 50.0, 0.0),
        DVec3::new(0.0, 60.0, 0.0),
    ));
    let img = render(&mut r, &comp);
    for y in (36..44).chain(57..64) {
        for x in 26..35 {
            assert_eq!(img.px(x, y), [255, 0, 0, 255], "seam at ({x}, {y})");
        }
    }
    // Layers that do not overlap on screen are not cut at all.
    comp.layers[0] = solid_3d(
        "wide",
        RED,
        [80, 30],
        DVec3::new(50.0, 60.0, 0.0),
        DVec3::ZERO,
    );
    comp.layers[1] = solid_3d(
        "apart",
        GREEN,
        [20, 20],
        DVec3::new(50.0, 15.0, 0.0),
        DVec3::new(0.0, 60.0, 0.0),
    );
    let img = render(&mut r, &comp);
    for x in 12..88 {
        assert_eq!(img.px(x, 60), [255, 0, 0, 255], "x = {x}");
    }
}

/// Depth of field: a layer at the focus distance stays sharp; the same layer
/// farther away blurs across its edges, and a tilted layer blurs more where
/// it recedes.
#[test]
fn depth_of_field_blurs_layers_away_from_the_focus_plane() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let zoom = 200.0;
    let comp_with = |z: f64, rotation: DVec3, dof: bool| {
        let mut comp = RenderComp::new(p("/comps/main"), 200, 100);
        let mut camera = Camera::new(
            DMat4::from_translation(DVec3::new(100.0, 50.0, -zoom)),
            zoom,
        );
        camera.depth_of_field = dof;
        camera.focus_distance = zoom;
        camera.aperture = 20.0;
        comp.camera = Some(camera);
        comp.layers = vec![solid_3d(
            "a",
            RED,
            [100, 60],
            DVec3::new(100.0, 50.0, z),
            rotation,
        )];
        comp
    };
    // Pixels of the middle row partly covered.
    let soft = |img: &Image, xs: std::ops::Range<u32>| {
        xs.filter(|&x| (9..247).contains(&img.px(x, img.h / 2)[3]))
            .count()
    };
    let sharp = render(&mut r, &comp_with(0.0, DVec3::ZERO, true));
    assert!(
        soft(&sharp, 0..200) <= 4,
        "in focus: {}",
        soft(&sharp, 0..200)
    );
    // Twice the focus distance: a 10 px blur (20 px aperture, half of it).
    let far = render(&mut r, &comp_with(200.0, DVec3::ZERO, true));
    assert!(
        soft(&far, 0..200) >= 10,
        "out of focus: {}",
        soft(&far, 0..200)
    );
    let off = render(&mut r, &comp_with(200.0, DVec3::ZERO, false));
    assert!(soft(&off, 0..200) <= 4, "depth of field off");
    // A wide aperture: a 25 px blur, gathered over a reduced copy.
    let mut wide = comp_with(200.0, DVec3::ZERO, true);
    wide.camera.as_mut().unwrap().aperture = 100.0;
    let wide = render(&mut r, &wide);
    assert!(
        soft(&wide, 0..200) >= 30,
        "wide aperture: {}",
        soft(&wide, 0..200)
    );
    // Still red where it is not transparent (no dark fringe from the resolve).
    let mid = wide.px(100, 50);
    assert!(mid[0] > 240 && mid[1] < 10 && mid[2] < 10, "{mid:?}");
    // Turned about y: the right edge recedes, the left comes closer than
    // the focus plane but less so.
    let tilted = render(&mut r, &comp_with(0.0, DVec3::new(0.0, 50.0, 0.0), true));
    assert!(
        soft(&tilted, 100..200) > soft(&tilted, 0..100),
        "receding edge blurs more: {} vs {}",
        soft(&tilted, 100..200),
        soft(&tilted, 0..100)
    );
}

/// A 3D model layer (an OBJ cube) is drawn through the camera: opaque where
/// the cube is, lit differently per face, nothing around it.
#[test]
fn model_layers_draw_through_the_camera() {
    let Some(gpu) = gpu() else { return };
    let dir = std::env::temp_dir().join(format!("bae-model-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("cube.obj");
    let mut obj = String::new();
    for z in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            for x in [-1.0, 1.0] {
                obj += &format!("v {x} {y} {z}\n");
            }
        }
    }
    // Faces of the cube (1-based corners: x fastest, then y, then z).
    for f in [
        "1 3 4 2", "5 6 8 7", "1 2 6 5", "3 7 8 4", "1 5 7 3", "2 4 8 6",
    ] {
        obj += &format!("f {f}\n");
    }
    std::fs::write(&file, obj).unwrap();
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 200, 200);
    let mut layer = RenderLayer::new(
        p("/comps/main/layers/model"),
        LayerContent::Model(bae_render::ModelRef {
            asset: p("/assets/cube"),
            file: Some(file.clone()),
        }),
    );
    let m = DMat4::from_translation(DVec3::new(100.0, 100.0, 0.0))
        * bae_render::camera::xyz_rotation(DVec3::new(30.0, 40.0, 0.0));
    layer.matrix_3d = Some(m);
    comp.layers = vec![layer];
    let img = render(&mut r, &comp);
    assert_eq!(img.px(100, 100)[3], 255, "the cube");
    assert_eq!(img.px(2, 2)[3], 0, "around it");
    // Lit by a light: the faces facing it differ in brightness.
    use bae_render::light::{Falloff, Light, LightKind};
    comp.lights = vec![Light {
        kind: LightKind::Point,
        world: DMat4::from_translation(DVec3::new(0.0, 0.0, -300.0)),
        point_of_interest: DVec3::new(100.0, 100.0, 0.0),
        color: [1.0, 1.0, 1.0],
        intensity: 100.0,
        cone_angle: 90.0,
        cone_feather: 50.0,
        falloff: Falloff::None,
        radius: 500.0,
        falloff_distance: 500.0,
        casts_shadows: false,
        shadow_darkness: 100.0,
        shadow_diffusion: 0.0,
        scope_below: None,
        environment: None,
    }];
    let lit = render(&mut r, &comp);
    let luma: Vec<u32> = [(85, 85), (115, 85), (100, 120)]
        .into_iter()
        .map(|(x, y)| lit.px(x, y).iter().take(3).map(|&c| u32::from(c)).sum())
        .collect();
    assert!(
        luma.iter().any(|&l| l != luma[0]),
        "faces shade differently: {luma:?}"
    );
    std::fs::remove_dir_all(dir).ok();
}

/// An environment light with a picture (AE's Advanced 3D): a hidden 2D
/// layer, white above the horizon and black below, lights a model from
/// every direction (its top brighter than its bottom) and, with Background
/// Visible, shows behind the layers through the camera.
#[test]
fn environment_light_pictures_light_models_and_show_behind() {
    let Some(gpu) = gpu() else { return };
    let dir = std::env::temp_dir().join(format!("bae-env-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("cube.obj");
    let mut obj = String::new();
    for z in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            for x in [-1.0, 1.0] {
                obj += &format!("v {x} {y} {z}\n");
            }
        }
    }
    for f in [
        "1 3 4 2", "5 6 8 7", "1 2 6 5", "3 7 8 4", "1 5 7 3", "2 4 8 6",
    ] {
        obj += &format!("f {f}\n");
    }
    std::fs::write(&file, obj).unwrap();
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 200, 200);
    let mut model = RenderLayer::new(
        p("/comps/main/layers/model"),
        LayerContent::Model(bae_render::ModelRef {
            asset: p("/assets/cube"),
            file: Some(file.clone()),
        }),
    );
    // Tipped toward the camera: the top face shows above the front face.
    model.matrix_3d = Some(
        DMat4::from_translation(DVec3::new(100.0, 100.0, 0.0))
            * bae_render::camera::xyz_rotation(DVec3::new(-35.0, 0.0, 0.0)),
    );
    // The picture: a 200 × 100 comp, white in its upper half (the sky).
    let mut sky = RenderComp::new(p("/comps/sky"), 200, 100);
    let mut white = solid("white", [1.0; 4], [200, 50]);
    white.path = p("/comps/sky/layers/white");
    sky.layers = vec![white];
    let mut source = RenderLayer::new(
        p("/comps/main/layers/hdri"),
        LayerContent::Comp(Arc::new(sky)),
    );
    source.visible = false;
    comp.layers = vec![model, source];
    use bae_render::light::{EnvironmentSource, Falloff, Light, LightKind};
    let light = |background| Light {
        kind: LightKind::Environment,
        world: DMat4::IDENTITY,
        point_of_interest: DVec3::ZERO,
        color: [1.0, 1.0, 1.0],
        intensity: 100.0,
        cone_angle: 90.0,
        cone_feather: 50.0,
        falloff: Falloff::None,
        radius: 500.0,
        falloff_distance: 500.0,
        casts_shadows: false,
        shadow_darkness: 100.0,
        shadow_diffusion: 0.0,
        scope_below: None,
        environment: Some(EnvironmentSource {
            layer: 1,
            background,
        }),
    };
    comp.lights = vec![light(None)];
    let img = render(&mut r, &comp);
    let luma =
        |img: &Image, x, y| -> u32 { img.px(x, y).iter().take(3).map(|&c| u32::from(c)).sum() };
    // The face toward the sky is brighter than the face toward the ground.
    let (top, front) = (luma(&img, 100, 72), luma(&img, 100, 118));
    assert!(top > front + 30, "top {top} vs front {front}");
    assert_eq!(img.px(2, 2)[3], 0, "no background");
    // Background Visible: sky above, ground below, the model in front.
    comp.lights = vec![light(Some([1.0, 0.0]))];
    let img = render(&mut r, &comp);
    assert!(luma(&img, 100, 2) > 700, "sky {:?}", img.px(100, 2));
    assert!(luma(&img, 100, 197) < 30, "ground {:?}", img.px(100, 197));
    assert_eq!(img.px(2, 197)[3], 255, "opaque background");
    assert_eq!(luma(&img, 100, 72), top, "the model draws over it");
    std::fs::remove_dir_all(dir).ok();
}

/// A 3D plane through a model cuts it (true intersection): the plane,
/// tilted through the cube's centre, covers the cube where it is nearer
/// and goes under it where it is farther. Planes in front of or behind the
/// whole cube cover it or not at all.
#[test]
fn planes_intersect_models() {
    let Some(gpu) = gpu() else { return };
    let dir = std::env::temp_dir().join(format!("bae-intersect-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("cube.obj");
    let mut obj = String::new();
    for z in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            for x in [-1.0, 1.0] {
                obj += &format!("v {x} {y} {z}\n");
            }
        }
    }
    for f in [
        "1 3 4 2", "5 6 8 7", "1 2 6 5", "3 7 8 4", "1 5 7 3", "2 4 8 6",
    ] {
        obj += &format!("f {f}\n");
    }
    std::fs::write(&file, obj).unwrap();
    let mut r = Renderer::new(gpu);
    let mut model = RenderLayer::new(
        p("/comps/main/layers/model"),
        LayerContent::Model(bae_render::ModelRef {
            asset: p("/assets/cube"),
            file: Some(file.clone()),
        }),
    );
    model.matrix_3d = Some(DMat4::from_translation(DVec3::new(100.0, 100.0, 0.0)));
    let mut scene = |plane: RenderLayer| {
        let mut comp = RenderComp::new(p("/comps/main"), 200, 200);
        comp.layers = vec![model.clone(), plane];
        render(&mut r, &comp)
    };
    let red = |px: [u8; 4]| px[0] > 200 && px[1] < 40 && px[2] < 40;
    // Tilted 60° about x through the cube's centre.
    let tilted = scene(solid_3d(
        "plane",
        RED,
        [300, 300],
        DVec3::new(100.0, 100.0, 0.0),
        DVec3::new(60.0, 0.0, 0.0),
    ));
    let (upper, lower) = (tilted.px(100, 50), tilted.px(100, 150));
    assert!(
        red(upper) != red(lower),
        "the plane covers one end: {upper:?} {lower:?}"
    );
    // The cut is where the plane meets the front face (z = −50, ~37 px from
    // the centre), not at the cube's centre.
    for y in [85, 100, 115] {
        assert!(!red(tilted.px(100, y)), "y {y}: {:?}", tilted.px(100, y));
    }
    // In front of the whole cube: covers it; behind it: under it.
    let front = scene(solid_3d(
        "plane",
        RED,
        [300, 300],
        DVec3::new(100.0, 100.0, -80.0),
        DVec3::ZERO,
    ));
    assert!(red(front.px(100, 100)), "{:?}", front.px(100, 100));
    let behind = scene(solid_3d(
        "plane",
        RED,
        [300, 300],
        DVec3::new(100.0, 100.0, 80.0),
        DVec3::ZERO,
    ));
    assert!(!red(behind.px(100, 100)), "{:?}", behind.px(100, 100));
    assert!(red(behind.px(5, 5)), "around the cube");
    std::fs::remove_dir_all(dir).ok();
}

/// A 3D model with Casts Shadows on throws its shadow on a lit layer behind
/// it (drawn into the layer's shadow map): the wall darkens where the cube
/// blocks the light, and only there.
#[test]
fn models_cast_shadows_on_layers() {
    let Some(gpu) = gpu() else { return };
    let dir = std::env::temp_dir().join(format!("bae-model-shadow-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("cube.obj");
    let mut obj = String::new();
    for z in [-1.0, 1.0] {
        for y in [-1.0, 1.0] {
            for x in [-1.0, 1.0] {
                obj += &format!("v {x} {y} {z}\n");
            }
        }
    }
    for f in [
        "1 3 4 2", "5 6 8 7", "1 2 6 5", "3 7 8 4", "1 5 7 3", "2 4 8 6",
    ] {
        obj += &format!("f {f}\n");
    }
    std::fs::write(&file, obj).unwrap();
    let mut r = Renderer::new(gpu);
    use bae_render::light::{Falloff, Light, LightKind, ShadowMode};
    let scene = |casts: ShadowMode| {
        let mut comp = RenderComp::new(p("/comps/main"), 200, 200);
        let mut model = RenderLayer::new(
            p("/comps/main/layers/model"),
            LayerContent::Model(bae_render::ModelRef {
                asset: p("/assets/cube"),
                file: Some(file.clone()),
            }),
        );
        model.matrix_3d = Some(DMat4::from_translation(DVec3::new(100.0, 100.0, 0.0)));
        model.material.casts_shadows = casts;
        let wall = solid_3d(
            "wall",
            [1.0; 4],
            [600, 600],
            DVec3::new(100.0, 100.0, 150.0),
            DVec3::ZERO,
        );
        comp.layers = vec![model, wall];
        comp.lights = vec![Light {
            kind: LightKind::Point,
            world: DMat4::from_translation(DVec3::new(0.0, 100.0, -300.0)),
            point_of_interest: DVec3::new(100.0, 100.0, 0.0),
            color: [1.0, 1.0, 1.0],
            intensity: 100.0,
            cone_angle: 90.0,
            cone_feather: 50.0,
            falloff: Falloff::None,
            radius: 500.0,
            falloff_distance: 500.0,
            casts_shadows: true,
            shadow_darkness: 100.0,
            shadow_diffusion: 0.0,
            scope_below: None,
            environment: None,
        }];
        comp
    };
    let lit = render(&mut r, &scene(ShadowMode::Off));
    let shadowed = render(&mut r, &scene(ShadowMode::On));
    let luma =
        |img: &Image, x, y| -> i32 { img.px(x, y).iter().take(3).map(|&c| i32::from(c)).sum() };
    // Right of the cube the wall is in its shadow; left of it, lit.
    assert!(
        luma(&shadowed, 180, 100) + 150 < luma(&lit, 180, 100),
        "shadow: {:?} vs {:?}",
        shadowed.px(180, 100),
        lit.px(180, 100)
    );
    assert_eq!(luma(&shadowed, 20, 100), luma(&lit, 20, 100), "no shadow");
    std::fs::remove_dir_all(dir).ok();
}

/// A collapsed 3D pre-comp joins the comp's 3D scene (AE-CMP-424): its layers
/// take its place, placed by its matrix and seen by this comp's camera; a
/// pre-comp that is not collapsed stays one plane.
#[test]
fn collapsed_3d_precomp_layers_join_the_scene() {
    use bae_render::AnimEvaluator;
    use bae_render::fixture::SceneBuilder;
    let scene = |collapse: bool| {
        let mut b = SceneBuilder::new();
        let rate = bae_model::FrameRate::FPS_30;
        let main = b.comp("main", 200, 100, rate, Time::ONE_SECOND).unwrap();
        let inner = b.comp("inner", 200, 100, rate, Time::ONE_SECOND).unwrap();
        let solid = b
            .solid_layer(&inner, "s", [1.0, 0.0, 0.0, 1.0], [40, 40], [80.0, 30.0])
            .unwrap();
        b.set(&solid, "three_d", Value::Bool(true)).unwrap();
        b.set(
            &solid,
            "transform:position",
            Value::Vec3([100.0, 50.0, 200.0]),
        )
        .unwrap();
        let pre = b.layer(&main, "pre", "pre_comp", Some(&inner)).unwrap();
        b.set(&pre, "three_d", Value::Bool(true)).unwrap();
        b.set(&pre, "collapse_transformation", Value::Bool(collapse))
            .unwrap();
        let stage = b.into_stage();
        let rc = AnimEvaluator::new(None)
            .evaluate(&stage, &main, Time::ZERO)
            .unwrap();
        (rc, pre, solid)
    };
    let (rc, pre, _) = scene(false);
    assert!(
        matches!(
            rc.layers.iter().find(|l| l.path == pre).unwrap().content,
            LayerContent::Comp(_)
        ),
        "not collapsed: one plane"
    );
    let (rc, pre, solid) = scene(true);
    assert!(
        rc.layers.iter().all(|l| l.path != pre),
        "replaced by its layers"
    );
    let s = rc
        .layers
        .iter()
        .find(|l| l.path == solid)
        .expect("the inner solid");
    // The pre-comp layer sits at the comp centre with its anchor at the
    // pre-comp's centre: the inner solid keeps its comp-space placement.
    let centre = s
        .matrix_3d
        .unwrap()
        .transform_point3(DVec3::new(20.0, 20.0, 0.0));
    assert!(
        (centre - DVec3::new(100.0, 50.0, 200.0)).length() < 1e-6,
        "{centre}"
    );
}

/// Models receive the shadows of models (AE's Advanced 3D: spot and
/// parallel lights): a small cube in front of a slab, lit at an angle by a
/// parallel light, darkens the slab beside it when it casts shadows.
#[test]
fn models_receive_model_shadows() {
    let Some(gpu) = gpu() else { return };
    let dir = std::env::temp_dir().join(format!("bae-model-receive-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let boxed = |name: &str, z: f64| {
        let mut obj = String::new();
        for zz in [-z, z] {
            for y in [-1.0, 1.0] {
                for x in [-1.0, 1.0] {
                    obj += &format!("v {x} {y} {zz}\n");
                }
            }
        }
        for f in [
            "1 3 4 2", "5 6 8 7", "1 2 6 5", "3 7 8 4", "1 5 7 3", "2 4 8 6",
        ] {
            obj += &format!("f {f}\n");
        }
        let file = dir.join(name);
        std::fs::write(&file, obj).unwrap();
        file
    };
    let (cube, slab) = (boxed("cube.obj", 1.0), boxed("slab.obj", 0.05));
    let mut r = Renderer::new(gpu.clone());
    use bae_render::light::{Falloff, Light, LightKind, ShadowMode};
    let model = |name: &str, file: &std::path::Path, m: DMat4| {
        let mut l = RenderLayer::new(
            p(&format!("/comps/main/layers/{name}")),
            LayerContent::Model(bae_render::ModelRef {
                asset: p(&format!("/assets/{name}")),
                file: Some(file.to_owned()),
            }),
        );
        l.matrix_3d = Some(m);
        l
    };
    let scene = |casts: ShadowMode| {
        let mut comp = RenderComp::new(p("/comps/main"), 200, 200);
        let mut caster = model(
            "cube",
            &cube,
            DMat4::from_translation(DVec3::new(100.0, 100.0, 0.0))
                * DMat4::from_scale(DVec3::splat(0.4)),
        );
        caster.material.casts_shadows = casts;
        let receiver = model(
            "slab",
            &slab,
            DMat4::from_translation(DVec3::new(100.0, 100.0, 50.0))
                * DMat4::from_scale(DVec3::splat(2.0)),
        );
        comp.layers = vec![caster, receiver];
        let d = DVec3::new(0.3, 0.0, 1.0).normalize();
        comp.lights = vec![Light {
            kind: LightKind::Parallel,
            world: DMat4::from_translation(DVec3::new(100.0, 100.0, -300.0))
                * bae_render::camera::look_at(d),
            point_of_interest: DVec3::new(100.0, 100.0, 0.0),
            color: [1.0, 1.0, 1.0],
            intensity: 100.0,
            cone_angle: 90.0,
            cone_feather: 50.0,
            falloff: Falloff::None,
            radius: 500.0,
            falloff_distance: 500.0,
            casts_shadows: true,
            shadow_darkness: 100.0,
            shadow_diffusion: 0.0,
            scope_below: None,
            environment: None,
        }];
        comp
    };
    let lit = render(&mut r, &scene(ShadowMode::Off));
    let shadowed = render(&mut r, &scene(ShadowMode::On));
    let luma =
        |img: &Image, x, y| -> i32 { img.px(x, y).iter().take(3).map(|&c| i32::from(c)).sum() };
    assert!(
        luma(&shadowed, 128, 100) + 150 < luma(&lit, 128, 100),
        "shadow on the slab: {:?} vs {:?}",
        shadowed.px(128, 100),
        lit.px(128, 100)
    );
    assert_eq!(luma(&shadowed, 60, 100), luma(&lit, 60, 100), "lit");
    // A still scene draws its light maps once (no frame cache here).
    let mut r = Renderer::with_config(
        gpu,
        bae_render::RendererConfig {
            frame_cache_bytes: 0,
            ..bae_render::RendererConfig::default()
        },
    );
    // (Moved a little: the maps above are shared by every renderer.)
    let mut comp = scene(ShadowMode::On);
    comp.layers[0].matrix_3d = Some(
        DMat4::from_translation(DVec3::new(101.0, 100.0, 0.0))
            * DMat4::from_scale(DVec3::splat(0.4)),
    );
    let draws: Vec<usize> = (0..3)
        .map(|_| {
            r.render_frame(&comp, &RenderRequest::full())
                .unwrap()
                .stats
                .draw_calls
        })
        .collect();
    assert!(draws[1] < draws[0], "light maps reused: {draws:?}");
    assert_eq!(draws[1], draws[2], "{draws:?}");
    // A 3D solid casting shadows throws its shadow on the slab as well.
    let mut card_scene = scene(ShadowMode::Off);
    card_scene.layers[0].visible = false;
    let mut card = solid_3d(
        "card",
        RED,
        [40, 40],
        DVec3::new(100.0, 100.0, 0.0),
        DVec3::ZERO,
    );
    card.material.casts_shadows = ShadowMode::On;
    card_scene.layers.insert(0, card.clone());
    let with_card = render(&mut r, &card_scene);
    card.material.casts_shadows = ShadowMode::Off;
    card_scene.layers[0] = card;
    let without = render(&mut r, &card_scene);
    assert!(
        luma(&with_card, 125, 100) + 150 < luma(&without, 125, 100),
        "card shadow: {:?} vs {:?}",
        with_card.px(125, 100),
        without.px(125, 100)
    );
    assert_eq!(luma(&with_card, 60, 100), luma(&without, 60, 100));
    std::fs::remove_dir_all(dir).ok();
}

/// Advanced 3D Transparency lets what is behind a 3D layer through;
/// Transparency Rolloff takes transparency away at glancing angles.
#[test]
fn transparency_shows_what_is_behind() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let scene = |transparency: f32, rolloff: f32, tilt: f64| {
        let mut comp = RenderComp::new(p("/comps/main"), 200, 200);
        let mut front = solid_3d(
            "front",
            RED,
            [200, 200],
            DVec3::new(100.0, 100.0, 0.0),
            DVec3::new(tilt, 0.0, 0.0),
        );
        front.material.transparency = transparency;
        front.material.transparency_rolloff = rolloff;
        comp.layers = vec![front, solid("back", BLUE, [200, 200])];
        comp
    };
    let opaque = render(&mut r, &scene(0.0, 0.0, 0.0)).px(100, 100);
    assert!(opaque[0] > 250 && opaque[2] < 5, "{opaque:?}");
    let half = render(&mut r, &scene(50.0, 0.0, 0.0)).px(100, 100);
    assert!(
        (i32::from(half[0]) - 128).abs() <= 3 && (i32::from(half[2]) - 127).abs() <= 3,
        "{half:?}"
    );
    // Tilted 60° (seen at about cos 0.5 from the camera): full rolloff
    // halves the transparency.
    let tilted = render(&mut r, &scene(50.0, 100.0, 60.0)).px(100, 100);
    assert!(tilted[0] > half[0] + 20, "{tilted:?} vs {half:?}");
}

/// Advanced 3D reflections: a 3D layer with Reflection Intensity reflects
/// the picture of an environment light that reaches it.
#[test]
fn layers_reflect_the_environment_picture() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    use bae_render::light::{EnvironmentSource, Falloff, Light, LightKind};
    let scene = |intensity: f32| {
        let mut comp = RenderComp::new(p("/comps/main"), 200, 200);
        let mut plane = solid_3d(
            "plane",
            [1.0; 4],
            [200, 200],
            DVec3::new(100.0, 100.0, 0.0),
            DVec3::ZERO,
        );
        plane.material.reflection_intensity = intensity;
        let mut picture = solid("picture", RED, [400, 200]);
        picture.visible = false;
        comp.layers = vec![plane, picture];
        comp.lights = vec![Light {
            kind: LightKind::Environment,
            world: DMat4::IDENTITY,
            point_of_interest: DVec3::ZERO,
            color: [1.0, 1.0, 1.0],
            intensity: 100.0,
            cone_angle: 90.0,
            cone_feather: 50.0,
            falloff: Falloff::None,
            radius: 500.0,
            falloff_distance: 500.0,
            casts_shadows: false,
            shadow_darkness: 100.0,
            shadow_diffusion: 0.0,
            scope_below: None,
            environment: Some(EnvironmentSource {
                layer: 1,
                background: None,
            }),
        }];
        comp
    };
    let plain = render(&mut r, &scene(0.0)).px(100, 100);
    assert!(plain[1] > 200 && plain[2] > 200, "lit white: {plain:?}");
    let mirror = render(&mut r, &scene(100.0)).px(100, 100);
    assert!(
        mirror[0] > 200 && mirror[1] < 30 && mirror[2] < 30,
        "reflects the red picture: {mirror:?}"
    );
    let half = render(&mut r, &scene(50.0)).px(100, 100);
    assert!(half[1] > 60 && half[1] < 200, "half: {half:?}");
}
