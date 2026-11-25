//! GPU rendering against a CPU reference of AE's compositing formulas
//! (docs/spec/ae/compositing-3d-color.md AE-CMP-012/031/032/034).

use std::sync::Arc;

use bae_gpu::GpuContext;
use bae_model::{PrimPath, Time};
use bae_render::{
    BitDepth, BlendMode, IRect, LayerContent, OutputAlpha, RenderComp, RenderLayer, RenderRequest,
    Renderer, Resolution, TrackMatte, TrackMatteMode,
};
use glam::{DAffine2, DVec2};

fn gpu() -> Option<Arc<GpuContext>> {
    bae_gpu::testing::shared_context()
}

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

fn solid(name: &str, color: [f32; 4], size: [u32; 2], at: [f64; 2]) -> RenderLayer {
    let mut l = RenderLayer::new(
        p(&format!("/comps/main/layers/{name}")),
        LayerContent::Solid {
            color,
            width: size[0],
            height: size[1],
        },
    );
    l.transform = DAffine2::from_translation(DVec2::new(at[0], at[1]));
    l
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

fn render(r: &mut Renderer, comp: &RenderComp, req: &RenderRequest, alpha: OutputAlpha) -> Image {
    let frame = r.render_frame(comp, req).unwrap();
    let img = r.read_rgba8(&frame, alpha).unwrap();
    Image {
        w: img.width,
        data: img.data,
    }
}

#[path = "ae/blend_reference.rs"]
mod blend_reference;

fn close(a: [u8; 4], b: [u8; 4], tol: u8) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= tol)
}

#[test]
fn solids_are_pixel_exact_and_cover_their_rectangles() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 64, 48);
    comp.layers
        .push(solid("a", [1.0, 0.0, 0.0, 1.0], [20, 10], [10.0, 5.0]));
    let img = render(&mut r, &comp, &RenderRequest::full(), OutputAlpha::Straight);
    assert_eq!(img.px(10, 5), [255, 0, 0, 255]);
    assert_eq!(img.px(29, 14), [255, 0, 0, 255]);
    assert_eq!(img.px(9, 5), [0, 0, 0, 0]);
    assert_eq!(img.px(30, 14), [0, 0, 0, 0]);
    assert_eq!(img.px(10, 15), [0, 0, 0, 0]);

    // Half-pixel offset: edge pixels are half covered (box filter, like AE's resampling).
    comp.layers[0].transform = DAffine2::from_translation(DVec2::new(10.5, 5.0));
    let img = render(
        &mut r,
        &comp,
        &RenderRequest::full(),
        OutputAlpha::Premultiplied,
    );
    assert_eq!(img.px(10, 6), [128, 0, 0, 128]);
    assert_eq!(img.px(11, 6), [255, 0, 0, 255]);
    assert_eq!(img.px(30, 6), [128, 0, 0, 128]);
}

#[test]
fn rotated_layer_coverage_sums_to_its_area() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 100, 100);
    let mut l = solid("a", [1.0; 4], [40, 40], [0.0, 0.0]);
    l.transform = DAffine2::from_translation(DVec2::new(50.0, 50.0))
        * DAffine2::from_angle(30f64.to_radians())
        * DAffine2::from_translation(DVec2::new(-20.0, -20.0));
    comp.layers.push(l);
    let img = render(
        &mut r,
        &comp,
        &RenderRequest::full(),
        OutputAlpha::Premultiplied,
    );
    let total: f64 = img
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| f64::from(p[3]) / 255.0)
        .sum();
    assert!((total - 1600.0).abs() < 8.0, "coverage {total}");
}

#[test]
fn blend_modes_match_the_reference() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu.clone());
    for depth in [BitDepth::Bpc8, BitDepth::Bpc16, BitDepth::Float32] {
        for mode in BlendMode::ALL {
            let pairs = [
                ([0.25, 0.75, 1., 1.], [0., 0.50, 0.75, 1.], 1.),
                ([0.75, 0.375, 0.125, 0.5], [0.25, 0.625, 0.875, 0.75], 0.75),
                ([0.25, 0.25, 0.25, 1.], [0.625, 0., 0.25, 0.5], 1.),
            ];
            for (backdrop, src, opacity) in pairs {
                let mut comp = RenderComp::new(p("/comps/main"), 4, 4);
                let mut top = solid("top", src, [4, 4], [0., 0.]);
                top.blend_mode = mode;
                top.opacity = opacity;
                comp.layers = vec![top, solid("bottom", backdrop, [4, 4], [0., 0.])];
                let frame = r
                    .render_frame(
                        &comp,
                        &RenderRequest {
                            depth,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                let pixels = bae_render::testing::read_pixels(&gpu, &frame.image).unwrap();
                for (idx, got) in pixels.iter().enumerate() {
                    let random = blend_reference::random(
                        idx as u32 % 4,
                        idx as u32 / 4,
                        comp.layers[0].path.as_str(),
                        0,
                    );
                    let want = blend_reference::pixel(mode, src, opacity, backdrop, depth, random);
                    for c in 0..4 {
                        assert!(
                            (want[c] - got[c]).abs() <= 0.002 * want[c].abs().max(1.),
                            "{mode} {depth:?} {src:?} over {backdrop:?}: {got:?} vs {want:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn clamped_add_differs_from_float_add() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 4, 4);
    let mut top = solid("top", [0.8, 0.8, 0.8, 1.0], [4, 4], [0.0, 0.0]);
    top.blend_mode = BlendMode::Add;
    comp.layers.push(top);
    let mut mid = solid("mid", [0.8, 0.8, 0.8, 1.0], [4, 4], [0.0, 0.0]);
    mid.blend_mode = BlendMode::Multiply;
    comp.layers.push(mid);
    comp.layers
        .push(solid("bottom", [0.5, 0.5, 0.5, 1.0], [4, 4], [0.0, 0.0]));
    // 8 bpc: 0.5·0.8 = 0.4, + 0.8 → 1.0 (clamped). Float keeps 1.2, shown as 255 too,
    // so check the clamp through a following multiply instead.
    let mut over = solid("over", [0.5, 0.5, 0.5, 1.0], [4, 4], [0.0, 0.0]);
    over.blend_mode = BlendMode::Multiply;
    comp.layers.insert(0, over);
    let clamped = render(&mut r, &comp, &RenderRequest::full(), OutputAlpha::Straight);
    let float = render(
        &mut r,
        &comp,
        &RenderRequest {
            depth: BitDepth::Float32,
            ..RenderRequest::full()
        },
        OutputAlpha::Straight,
    );
    assert!(
        close(clamped.px(1, 1), [128, 128, 128, 255], 1),
        "{:?}",
        clamped.px(1, 1)
    );
    assert!(
        close(float.px(1, 1), [153, 153, 153, 255], 1),
        "{:?}",
        float.px(1, 1)
    );
}

#[test]
fn track_mattes_alpha_and_luma() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 20, 10);
    // Matte: left half white at 50 % opacity, eye off.
    let mut matte = solid("matte", [1.0, 1.0, 1.0, 1.0], [10, 10], [0.0, 0.0]);
    matte.visible = false;
    matte.opacity = 0.5;
    comp.layers.push(matte);
    let mut fill = solid("fill", [0.0, 1.0, 0.0, 1.0], [20, 10], [0.0, 0.0]);
    fill.track_matte = Some(TrackMatte {
        layer: 0,
        mode: TrackMatteMode::Alpha,
    });
    comp.layers.push(fill);
    let img = render(
        &mut r,
        &comp,
        &RenderRequest::full(),
        OutputAlpha::Premultiplied,
    );
    assert!(
        close(img.px(2, 5), [0, 128, 0, 128], 1),
        "{:?}",
        img.px(2, 5)
    );
    assert_eq!(img.px(15, 5), [0, 0, 0, 0]);

    comp.layers[1].track_matte = Some(TrackMatte {
        layer: 0,
        mode: TrackMatteMode::AlphaInverted,
    });
    let img = render(
        &mut r,
        &comp,
        &RenderRequest::full(),
        OutputAlpha::Premultiplied,
    );
    assert!(close(img.px(2, 5), [0, 128, 0, 128], 1));
    assert_eq!(img.px(15, 5), [0, 255, 0, 255]);

    // Luma: a 40 % grey opaque matte → 0.4; inverted → 0.6 (and 1 outside the matte).
    comp.layers[0].opacity = 1.0;
    comp.layers[0].content = LayerContent::Solid {
        color: [0.4, 0.4, 0.4, 1.0],
        width: 10,
        height: 10,
    };
    comp.layers[1].track_matte = Some(TrackMatte {
        layer: 0,
        mode: TrackMatteMode::Luma,
    });
    let img = render(
        &mut r,
        &comp,
        &RenderRequest::full(),
        OutputAlpha::Premultiplied,
    );
    assert!(
        close(img.px(2, 5), [0, 102, 0, 102], 1),
        "{:?}",
        img.px(2, 5)
    );
    assert_eq!(img.px(15, 5), [0, 0, 0, 0]);
    comp.layers[1].track_matte = Some(TrackMatte {
        layer: 0,
        mode: TrackMatteMode::LumaInverted,
    });
    let img = render(
        &mut r,
        &comp,
        &RenderRequest::full(),
        OutputAlpha::Premultiplied,
    );
    assert!(
        close(img.px(2, 5), [0, 153, 0, 153], 1),
        "{:?}",
        img.px(2, 5)
    );
    assert_eq!(img.px(15, 5), [0, 255, 0, 255]);
}

#[test]
fn preserve_transparency_keeps_backdrop_alpha() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 20, 10);
    let mut top = solid("top", [1.0, 0.0, 0.0, 1.0], [20, 10], [0.0, 0.0]);
    top.preserve_transparency = true;
    comp.layers.push(top);
    comp.layers
        .push(solid("bottom", [0.0, 0.0, 1.0, 0.5], [10, 10], [0.0, 0.0]));
    let img = render(
        &mut r,
        &comp,
        &RenderRequest::full(),
        OutputAlpha::Premultiplied,
    );
    assert!(
        close(img.px(2, 5), [128, 0, 0, 128], 1),
        "{:?}",
        img.px(2, 5)
    );
    assert_eq!(img.px(15, 5), [0, 0, 0, 0]);
}

#[test]
fn precomps_half_resolution_roi_and_cache() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut inner = RenderComp::new(p("/comps/inner"), 40, 40);
    inner.layers.push(RenderLayer {
        path: p("/comps/inner/layers/a"),
        ..solid("a", [0.0, 0.0, 1.0, 1.0], [20, 20], [0.0, 0.0])
    });
    let mut comp = RenderComp::new(p("/comps/main"), 80, 80);
    let mut pre = RenderLayer::new(
        p("/comps/main/layers/pre"),
        LayerContent::Comp(Arc::new(inner)),
    );
    pre.transform = DAffine2::from_translation(DVec2::new(40.0, 40.0))
        * DAffine2::from_scale(DVec2::splat(2.0));
    comp.layers.push(pre);
    let img = render(&mut r, &comp, &RenderRequest::full(), OutputAlpha::Straight);
    // Magnified 2×, the nested raster's edges ramp over one source texel (two
    // pixels), like AE's bilinear resampling with a transparent border: the corner
    // pixel is 0.75² covered (AE renders 143 here too, see tests/ae).
    assert!(
        close(img.px(40, 40), [0, 0, 255, 143], 1),
        "{:?}",
        img.px(40, 40)
    );
    assert_eq!(img.px(42, 42), [0, 0, 255, 255]);
    assert_eq!(img.px(77, 77), [0, 0, 255, 255]);
    assert_eq!(img.px(37, 42), [0, 0, 0, 0]);
    // Inside, the solid's edge (at 20 of 40) is softened by bilinear filtering.
    assert!(
        close(img.px(79, 79), [0, 0, 255, 143], 1),
        "{:?}",
        img.px(79, 79)
    );

    let half = RenderRequest {
        resolution: Resolution::Half,
        ..RenderRequest::full()
    };
    let frame = r.render_frame(&comp, &half).unwrap();
    assert_eq!(frame.image.rect, IRect::new(0, 0, 80, 80));
    assert_eq!(frame.image.texel_size(), [40, 40]);
    let img = render(&mut r, &comp, &half, OutputAlpha::Straight);
    assert_eq!(img.px(21, 21), [0, 0, 255, 255]);
    assert_eq!(img.px(18, 21), [0, 0, 0, 0]);

    let roi = RenderRequest {
        roi: Some(IRect::new(30, 30, 50, 50)),
        ..RenderRequest::full()
    };
    let frame = r.render_frame(&comp, &roi).unwrap();
    assert_eq!(frame.image.rect, IRect::new(30, 30, 50, 50));
    let img = render(&mut r, &comp, &roi, OutputAlpha::Straight);
    assert_eq!(img.px(7, 12), [0, 0, 0, 0]);
    assert_eq!(img.px(13, 13), [0, 0, 255, 255]);

    // Second render of the same frame comes from the cache; a change misses it.
    let again = r.render_frame(&comp, &RenderRequest::full()).unwrap();
    assert!(again.from_cache);
    comp.layers[0].opacity = 0.5;
    assert!(
        !r.render_frame(&comp, &RenderRequest::full())
            .unwrap()
            .from_cache
    );
    assert_eq!(r.cached_frames(&p("/comps/main")), vec![Time::ZERO]);
}

#[cfg(feature = "media")]
#[test]
fn footage_png_is_sampled_with_straight_alpha_premultiplied() {
    let Some(gpu) = gpu() else { return };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("img.png");
    // 4×2: left half opaque red, right half 50 % green (straight alpha).
    let mut data = Vec::new();
    for _y in 0..2 {
        for x in 0..4 {
            data.extend_from_slice(if x < 2 {
                &[255, 0, 0, 255]
            } else {
                &[0, 255, 0, 128]
            });
        }
    }
    bae_render::png_out::write_rgba8(
        &path,
        &bae_gpu::transfer::ImageData {
            width: 4,
            height: 2,
            format: bae_gpu::wgpu::TextureFormat::Rgba8Unorm,
            bytes_per_row: 16,
            data,
        },
    )
    .unwrap();
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 8, 4);
    let mut l = RenderLayer::new(
        p("/comps/main/layers/img"),
        LayerContent::Footage(bae_render::FootageRef {
            asset: p("/assets/img"),
            file: Some(path),
            size: None,
            source_time: Time::ZERO,
            sequence: None,
            blend: None,
        }),
    );
    l.transform = DAffine2::from_translation(DVec2::new(2.0, 1.0));
    comp.layers.push(l);
    let frame = r.render_frame(&comp, &RenderRequest::full()).unwrap();
    assert!(frame.warnings.is_empty(), "{:?}", frame.warnings);
    let img = r.read_rgba8(&frame, OutputAlpha::Straight).unwrap();
    let img = Image {
        w: img.width,
        data: img.data,
    };
    assert_eq!(img.px(2, 1), [255, 0, 0, 255]);
    assert!(
        close(img.px(5, 2), [0, 255, 0, 128], 1),
        "{:?}",
        img.px(5, 2)
    );
    assert_eq!(img.px(1, 1), [0, 0, 0, 0]);
}

#[cfg(feature = "media")]
#[test]
fn frame_mix_blends_neighbouring_sequence_frames() {
    let Some(gpu) = gpu() else { return };
    let dir = tempfile::tempdir().unwrap();
    // A 3-frame sequence: red, green, blue.
    for (i, rgb) in [[255u8, 0, 0], [0, 255, 0], [0, 0, 255]].iter().enumerate() {
        let data = [rgb[0], rgb[1], rgb[2], 255].repeat(4);
        bae_render::png_out::write_rgba8(
            &dir.path().join(format!("seq_{i:04}.png")),
            &bae_gpu::transfer::ImageData {
                width: 2,
                height: 2,
                format: bae_gpu::wgpu::TextureFormat::Rgba8Unorm,
                bytes_per_row: 8,
                data,
            },
        )
        .unwrap();
    }
    let fps = bae_model::FrameRate::FPS_30;
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 2, 2);
    let footage = |blend| {
        LayerContent::Footage(bae_render::FootageRef {
            asset: p("/assets/seq"),
            file: Some(dir.path().join("seq_0000.png")),
            size: None,
            source_time: fps.frame_to_time(1),
            sequence: Some(fps),
            blend,
        })
    };
    comp.layers
        .push(RenderLayer::new(p("/comps/main/layers/seq"), footage(None)));
    let plain = render(&mut r, &comp, &RenderRequest::full(), OutputAlpha::Straight);
    assert_eq!(plain.px(0, 0), [0, 255, 0, 255]);
    comp.layers[0].content = footage(Some(bae_render::FrameBlend {
        span: fps.frame_duration(),
        pixel_motion: false,
    }));
    let frame = r.render_frame(&comp, &RenderRequest::full()).unwrap();
    assert!(frame.warnings.is_empty(), "{:?}", frame.warnings);
    let img = r.read_rgba8(&frame, OutputAlpha::Straight).unwrap();
    let px = Image {
        w: img.width,
        data: img.data,
    }
    .px(1, 1);
    // ~5.5 % red + 89 % green + 5.5 % blue, still opaque.
    assert!(close(px, [14, 228, 14, 255], 4), "{px:?}");
}

#[test]
fn dissolve_frames_and_roi_use_composition_coordinates() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 32, 24);
    comp.layers
        .push(solid("grain", [1., 1., 1., 0.5], [32, 24], [0., 0.]));
    for mode in [BlendMode::Dissolve, BlendMode::DancingDissolve] {
        comp.layers[0].blend_mode = mode;
        comp.time = Time::ZERO;
        let first = render(
            &mut r,
            &comp,
            &RenderRequest::full(),
            OutputAlpha::Premultiplied,
        );
        comp.time = comp.frame_rate.frame_to_time(1);
        let second = render(
            &mut r,
            &comp,
            &RenderRequest::full(),
            OutputAlpha::Premultiplied,
        );
        assert_eq!(first.data == second.data, mode == BlendMode::Dissolve);
        let req = RenderRequest {
            roi: Some(IRect::new(5, 7, 21, 18)),
            ..Default::default()
        };
        let crop = render(&mut r, &comp, &req, OutputAlpha::Premultiplied);
        for y in 0..11 {
            for x in 0..16 {
                let frame = u32::from(mode == BlendMode::DancingDissolve);
                let random =
                    blend_reference::random(x + 5, y + 7, comp.layers[0].path.as_str(), frame);
                assert_eq!(crop.px(x, y), [if random < 0.5 { 255 } else { 0 }; 4]);
                assert_eq!(crop.px(x, y), second.px(x + 5, y + 7));
            }
        }
    }
}

#[test]
fn nested_dissolve_time_invalidates_only_dancing_frames() {
    let mut inner = RenderComp::new(p("/comps/pre"), 4, 4);
    inner.layers.push(solid("grain", [1.; 4], [4, 4], [0., 0.]));
    let mut outer = RenderComp::new(p("/comps/main"), 4, 4);
    outer.layers.push(RenderLayer::new(
        p("/comps/main/layers/pre"),
        LayerContent::None,
    ));
    for mode in [BlendMode::Dissolve, BlendMode::DancingDissolve] {
        inner.layers[0].blend_mode = mode;
        let mut hashes = vec![];
        for n in [0, 1, 0] {
            inner.time = inner.frame_rate.frame_to_time(n);
            outer.layers[0].content = LayerContent::Comp(Arc::new(inner.clone()));
            hashes.push(outer.content_hash());
        }
        assert_eq!(hashes[0] == hashes[1], mode == BlendMode::Dissolve);
        assert_eq!(hashes[0], hashes[2]);
    }
}

#[test]
fn empty_stencil_clears_layers_below_and_keeps_layers_above() {
    let Some(gpu) = gpu() else { return };
    let mut renderer = Renderer::new(gpu);
    let mut comp = RenderComp::new(p("/comps/main"), 8, 8);
    comp.layers = vec![
        solid("above", [1., 0., 0., 1.], [1, 1], [0., 0.]),
        solid("stencil", [1.; 4], [3, 3], [3., 3.]),
        solid("middle", [0., 1., 0., 1.], [4, 8], [0., 0.]),
        solid("bottom", [0., 0., 1., 1.], [8, 8], [0., 0.]),
    ];
    for mode in [BlendMode::StencilAlpha, BlendMode::StencilLuma] {
        comp.layers[1].blend_mode = mode;
        for (offset, opacity) in [(3., 1.), (3., 0.), (30., 1.)] {
            comp.layers[1].transform = DAffine2::from_translation(DVec2::splat(offset));
            comp.layers[1].opacity = opacity;
            let img = render(
                &mut renderer,
                &comp,
                &RenderRequest::full(),
                OutputAlpha::Premultiplied,
            );
            assert_eq!(img.px(0, 0), [255, 0, 0, 255]);
            assert_eq!(img.px(1, 2), [0; 4]);
            assert_eq!(img.px(7, 7), [0; 4]);
            if offset == 3. && opacity == 1. {
                assert_eq!(img.px(3, 3), [0, 255, 0, 255]);
                assert_eq!(img.px(4, 3), [0, 0, 255, 255]);
            } else {
                assert_eq!(img.px(3, 3), [0; 4]);
            }
        }
    }
}

#[test]
fn float_arithmetic_preserves_hdr_and_negative_values() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu.clone());
    for mode in [
        BlendMode::Divide,
        BlendMode::Subtract,
        BlendMode::LinearBurn,
        BlendMode::LinearLight,
        BlendMode::Multiply,
    ] {
        let mut c = RenderComp::new(p("/comps/main"), 2, 2);
        let b = [2., -0.25, 0.75, 1.];
        let s = [0.5, 0.25, 0., 1.];
        c.layers = vec![
            solid("src", s, [2, 2], [0., 0.]),
            solid("base", b, [2, 2], [0., 0.]),
        ];
        c.layers[0].blend_mode = mode;
        let req = RenderRequest {
            depth: BitDepth::Float32,
            format: bae_render::WorkFormat::Rgba32Float,
            ..Default::default()
        };
        let image = r.render_frame(&c, &req).unwrap().image;
        let got = bae_render::testing::read_pixels(&gpu, &image).unwrap()[0];
        let want = blend_reference::pixel(mode, s, 1., b, req.depth, 0.);
        for i in 0..4 {
            assert!(
                (got[i] - want[i]).abs() < 0.001,
                "{mode}: {got:?} vs {want:?}"
            );
        }
    }
}
