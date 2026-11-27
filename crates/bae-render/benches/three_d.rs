//! 3D and motion-blur benchmark (docs/perf.md §5): 1080p, 50 large solids.
//!
//! * `3d`: the layers as 3D planes at different depths and X/Y/Z rotations (many
//!   intersect), seen through a camera that orbits every frame;
//! * `flat`: the same layers drawn 2D (the 3D path's overhead);
//! * `mb_2d`: 2D layers moving ~100 px per frame with motion blur (16 samples
//!   per frame, adaptive up to 128: ~27 placements each), and `still` the same
//!   without blur;
//! * `mb_3d`: the 3D scene with the orbiting camera motion-blurred (16 samples).
//!
//! `cargo bench -p bae-render --bench three_d`. Environment: `BAE_BENCH_FRAMES`
//! (default 120), `BAE_BENCH_OUT` (JSON report directory, default `target/bench`),
//! `BAE_BENCH_4K` (3840 × 2160).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use bae_gpu::{GpuContext, GpuContextDescriptor};
use bae_model::{PrimPath, Time};
use bae_render::camera::{Camera, xyz_rotation};
use bae_render::fixture::Lcg;
use bae_render::{
    CompMotionBlur, LayerContent, LayerSample, RenderComp, RenderLayer, RenderRequest, Renderer,
    RendererConfig, Resolution,
};
use glam::{DAffine2, DMat4, DVec2, DVec3};

/// The comp size: 1920 × 1080, or 3840 × 2160 with `BAE_BENCH_4K` (the 3D
/// scene framed the same: the camera's zoom follows the width).
fn size() -> (u32, u32) {
    if std::env::var_os("BAE_BENCH_4K").is_some() {
        (3840, 2160)
    } else {
        (1920, 1080)
    }
}
const LAYERS: usize = 50;
const SAMPLES: usize = 16;

fn solid(i: usize, seed: &[f64; 8]) -> RenderLayer {
    let mut l = RenderLayer::new(
        PrimPath::new(&format!("/comps/bench/layers/l{i}")).unwrap(),
        LayerContent::Solid {
            color: [seed[0] as f32, seed[1] as f32, seed[2] as f32, 1.0],
            width: 600,
            height: 400,
        },
    );
    l.opacity = 0.8;
    l
}

/// Layer → comp for the 3D scene.
fn plane(seed: &[f64; 8]) -> DMat4 {
    let centre = DVec3::new(
        200.0 + seed[3] * 1520.0,
        150.0 + seed[4] * 780.0,
        -300.0 + seed[5] * 1800.0,
    );
    let rot = DVec3::new(
        seed[6] * 120.0 - 60.0,
        seed[7] * 120.0 - 60.0,
        seed[0] * 90.0,
    );
    DMat4::from_translation(centre)
        * xyz_rotation(rot)
        * DMat4::from_translation(DVec3::new(-300.0, -200.0, 0.0))
}

fn flatten(m: &DMat4) -> DAffine2 {
    DAffine2::from_cols(
        m.x_axis.truncate().truncate(),
        m.y_axis.truncate().truncate(),
        m.w_axis.truncate().truncate(),
    )
}

/// The orbiting two-node camera at time `t` (seconds).
fn camera(t: f64) -> Camera {
    // The 1080p dolly; at 4K only the lens zooms in (the same framing).
    let base = bae_render::camera::default_zoom(1920);
    let zoom = base * f64::from(size().0) / 1920.0;
    let a = t * 1.5;
    let poi = DVec3::new(960.0, 540.0, 600.0);
    let position = poi
        + DVec3::new(
            a.sin() * 900.0,
            -200.0,
            -(base + 600.0) * a.cos().abs().max(0.6),
        );
    Camera::from_transform(
        DMat4::IDENTITY,
        position,
        Some(poi),
        DVec3::ZERO,
        DVec3::ZERO,
        zoom,
    )
}

fn main() {
    let frames: usize = std::env::var("BAE_BENCH_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);
    let gpu = match GpuContext::new_headless(GpuContextDescriptor {
        validation: false,
        panic_on_uncaptured_error: true,
        ..GpuContextDescriptor::default()
    }) {
        Ok(gpu) => gpu,
        Err(e) => {
            eprintln!("three_d: no GPU ({e}); skipped");
            return;
        }
    };
    let mut rng = Lcg::new(7);
    let seeds: Vec<[f64; 8]> = (0..LAYERS)
        .map(|_| std::array::from_fn(|_| rng.next_f64()))
        .collect();
    let frame_s = 1.0 / 30.0;
    let base = |f: usize| {
        let (w, h) = size();
        let mut c = RenderComp::new(PrimPath::new("/comps/bench").unwrap(), w, h);
        c.time = Time::from_seconds(f as f64 * frame_s);
        c
    };
    let scene_3d = |f: usize, blur: bool| {
        let t = f as f64 * frame_s;
        let mut c = base(f);
        c.camera = Some(camera(t));
        if blur {
            // 180° shutter, phase −90°: samples from t − T/4, T/2 / 16 apart.
            let times: Vec<f64> = (0..SAMPLES)
                .map(|k| t - frame_s / 4.0 + frame_s / 2.0 * k as f64 / SAMPLES as f64)
                .collect();
            c.motion_blur = Some(Arc::new(CompMotionBlur {
                lights: Vec::new(),
                times: times.iter().map(|s| Time::from_seconds(*s)).collect(),
                cameras: times.iter().map(|s| Some(camera(*s))).collect(),
                adaptive_limit: 128,
            }));
        }
        c.layers = seeds
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let mut l = solid(i, s);
                let m = plane(s);
                l.matrix_3d = Some(m);
                l.transform = flatten(&m);
                if blur {
                    l.motion_samples = vec![
                        LayerSample {
                            transform: l.transform,
                            matrix_3d: Some(m),
                        };
                        SAMPLES
                    ]
                    .into();
                }
                l
            })
            .collect();
        c
    };
    let flat = |f: usize| {
        let mut c = scene_3d(f, false);
        for l in &mut c.layers {
            l.matrix_3d = None;
        }
        c
    };
    let moving = |f: usize, blur: bool| {
        let t = f as f64 * frame_s;
        let mut c = base(f);
        if blur {
            c.motion_blur = Some(Arc::new(CompMotionBlur {
                lights: Vec::new(),
                times: (0..SAMPLES).map(|k| Time::from_flicks(k as i64)).collect(),
                cameras: vec![None; SAMPLES],
                adaptive_limit: 128,
            }));
        }
        c.layers = seeds
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let mut l = solid(i, s);
                let velocity = DVec2::new(s[6] - 0.5, s[7] - 0.5).normalize_or_zero() * 3000.0;
                let at = |t: f64| {
                    DAffine2::from_translation(
                        DVec2::new(s[3] * 1320.0, s[4] * 680.0) + velocity * (t % 0.4 - 0.2),
                    ) * DAffine2::from_angle(s[5] * 0.5)
                };
                l.transform = at(t);
                if blur {
                    l.motion_samples = (0..SAMPLES)
                        .map(|k| LayerSample {
                            transform: at(
                                t - frame_s / 4.0 + frame_s / 2.0 * k as f64 / SAMPLES as f64
                            ),
                            matrix_3d: None,
                        })
                        .collect();
                }
                l
            })
            .collect();
        c
    };

    // Spot, point and parallel lights plus ambient over the 3D scene.
    let lit = |mut c: RenderComp| {
        use bae_render::light::{Falloff, Light, LightKind};
        let at = |kind, p: DVec3, aim: DVec3, intensity| Light {
            kind,
            world: DMat4::from_translation(p) * bae_render::camera::look_at(aim - p),
            point_of_interest: aim,
            color: [1.0, 0.95, 0.9],
            intensity,
            cone_angle: 60.0,
            cone_feather: 50.0,
            falloff: Falloff::Smooth,
            radius: 800.0,
            falloff_distance: 1500.0,
            casts_shadows: false,
            shadow_darkness: 100.0,
            shadow_diffusion: 0.0,
            scope_below: None,
            environment: None,
        };
        let centre = DVec3::new(960.0, 540.0, 600.0);
        c.lights = vec![
            at(
                LightKind::Spot,
                DVec3::new(300.0, 100.0, -800.0),
                centre,
                120.0,
            ),
            at(
                LightKind::Point,
                DVec3::new(1600.0, 300.0, -300.0),
                centre,
                70.0,
            ),
            at(
                LightKind::Parallel,
                DVec3::new(0.0, 0.0, -500.0),
                centre,
                40.0,
            ),
            at(LightKind::Ambient, DVec3::ZERO, centre, 20.0),
        ];
        c
    };
    // Every layer casts and receives shadows from the spot and point lights.
    let shadows = |mut c: RenderComp| {
        for l in &mut c.lights {
            l.casts_shadows = l.kind != bae_render::light::LightKind::Ambient;
        }
        for l in &mut c.layers {
            l.material.casts_shadows = bae_render::light::ShadowMode::On;
        }
        c
    };
    let mut renderer = Renderer::with_config(
        gpu.clone(),
        RendererConfig {
            frame_cache_bytes: 0,
            ..RendererConfig::default()
        },
    );
    let full = RenderRequest::full();
    let half = RenderRequest {
        resolution: Resolution::Half,
        ..full
    };
    let mut fps = |name: &str, make: &dyn Fn(usize) -> RenderComp, req: &RenderRequest| {
        let comps: Vec<RenderComp> = (0..frames).map(make).collect();
        for c in comps.iter().take(3) {
            renderer.render_frame(c, req).expect("render");
        }
        renderer.wait_idle().expect("idle");
        let draws = renderer
            .render_frame(&comps[0], req)
            .expect("render")
            .stats
            .draw_calls;
        renderer.wait_idle().expect("idle");
        let t = Instant::now();
        for c in &comps {
            renderer.render_frame(c, req).expect("render");
        }
        renderer.wait_idle().expect("idle");
        let fps = frames as f64 / t.elapsed().as_secs_f64();
        println!("  {name:<22} {fps:8.1} fps   {draws:4} draw calls");
        (name.to_owned(), fps)
    };
    println!("three_d — {}", gpu.summary());
    let results = [
        fps("3d full", &|f| scene_3d(f, false), &full),
        fps("3d half", &|f| scene_3d(f, false), &half),
        fps("flat full", &flat, &full),
        fps("mb_2d full", &|f| moving(f, true), &full),
        fps("still full", &|f| moving(f, false), &full),
        fps("mb_3d full", &|f| scene_3d(f, true), &full),
        fps("3d_lit full", &|f| lit(scene_3d(f, false)), &full),
        fps(
            "3d_shadows full",
            &|f| shadows(lit(scene_3d(f, false))),
            &full,
        ),
        fps(
            "3d_shadows half",
            &|f| shadows(lit(scene_3d(f, false))),
            &half,
        ),
        // A light moving every frame: the shadows are tested per pixel.
        fps(
            "3d_shadows moving_light",
            &|f| {
                let mut c = shadows(lit(scene_3d(f, false)));
                let shift = DMat4::from_translation(DVec3::new(f as f64 * 15.0, 0.0, 0.0));
                c.lights[0].world = shift * c.lights[0].world;
                c
            },
            &full,
        ),
    ];
    let report = serde_json::json!({
        "scenario": "three_d",
        "description": "1080p, 50 solids 600x400 at 80% opacity: 3D planes (rotated, intersecting) under an orbiting camera, the same flat, 2D layers moving ~100 px/frame with and without motion blur (16 samples, adaptive to 128), and the 3D scene with camera motion blur",
        "gpu": gpu.summary(),
        "frames": frames,
        "fps": results.iter().map(|(n, f)| (n.clone(), serde_json::json!(f))).collect::<serde_json::Map<_, _>>(),
    });
    let out = std::env::var_os("BAE_BENCH_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("../bench"));
    if std::fs::create_dir_all(&out).is_ok() {
        let _ = std::fs::write(
            out.join("three_d.json"),
            serde_json::to_string_pretty(&report).unwrap_or_default(),
        );
    }
}
