use super::*;

#[test]
fn point_defaults_follow_source_size_on_apply_and_reset() {
    assert_reference_points(&[("ADBE Radial Blur", 2, [15.5, 6.5])]);
}

/// What the viewer shows for `comp` at `resolution` (the same request as
/// `bae-ui-motion`'s GPU preview): the image is drawn over `image.rect` comp
/// pixels. Returns the bounding box (comp pixels) of texels with alpha > 50 %
/// and the straight RGBA8 at comp pixel `probe`; `None` without a GPU.
fn shown(
    fx: &mut Fx,
    comp: &PrimPath,
    resolution: bae_render::Resolution,
    probe: [i32; 2],
) -> Option<([i32; 4], [u8; 4])> {
    let renderer = fx.renderer.as_mut()?;
    let rc = fx.eval.evaluate(&fx.stage, comp, Time::ZERO).unwrap();
    let request = RenderRequest {
        resolution,
        purpose: bae_render::RenderPurpose::Preview,
        ..RenderRequest::full()
    };
    let frame = renderer.render_frame(&rc, &request).unwrap();
    let data = renderer.read_rgba8(&frame, OutputAlpha::Straight).unwrap();
    let r = frame.image.rect;
    // Texel (i, j) covers comp pixels r.x0 + i·sx … (the viewer stretches
    // the texture over `rect`).
    let sx = f64::from(r.width()) / f64::from(data.width);
    let sy = f64::from(r.height()) / f64::from(data.height);
    let mut bbox = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
    for j in 0..data.height {
        for i in 0..data.width {
            if data.pixel(i, j)[3] > 128 {
                let x0 = r.x0 + (f64::from(i) * sx).round() as i32;
                let y0 = r.y0 + (f64::from(j) * sy).round() as i32;
                let x1 = r.x0 + (f64::from(i + 1) * sx).round() as i32;
                let y1 = r.y0 + (f64::from(j + 1) * sy).round() as i32;
                bbox = [
                    bbox[0].min(x0),
                    bbox[1].min(y0),
                    bbox[2].max(x1),
                    bbox[3].max(y1),
                ];
            }
        }
    }
    let i = ((f64::from(probe[0] - r.x0) / sx) as u32).min(data.width - 1);
    let j = ((f64::from(probe[1] - r.y0) / sy) as u32).min(data.height - 1);
    let px = data.pixel(i, j);
    Some((bbox, [px[0], px[1], px[2], px[3]]))
}

#[test]
fn gaussian_blur_keeps_layer_bounds_at_reduced_preview_resolutions() {
    use bae_render::Resolution::{Full, Half, Quarter};
    let mut fx = Fx::new();
    let out = fx.run(
        "comp.create",
        json!({"width": 200, "height": 240, "fps": 30}),
    );
    let comp = p(out["path"].as_str().unwrap());
    // A tall red solid scaled to 50 % in the middle: [60, 40, 140, 200).
    let layer = fx.solid(&comp, [1.0, 0.0, 0.0], [160, 320]);
    fx.run(
        "attr.set",
        json!({"path": layer, "attr": "transform:scale", "value": {"type": "vec3", "value": [50.0, 50.0, 100.0]}}),
    );
    // Gaussian Blur 25 with Repeat Edge Pixels (the user's case): the solid
    // stays a sharp, full rectangle at every resolution, as in AE.
    let blur = fx.apply(&layer, "ADBE Gaussian Blur 2");
    fx.param(&blur, 1, json!({"type": "float", "value": 25.0}));
    fx.param(&blur, 3, json!({"type": "bool", "value": true}));
    for res in [Full, Half, Quarter] {
        let Some((bbox, edge)) = shown(&mut fx, &comp, res, [61, 41]) else {
            return;
        };
        assert_eq!(bbox, [60, 40, 140, 200], "{res:?}: covers the layer");
        assert!(
            edge[0] > 250 && edge[3] > 250,
            "{res:?}: sharp corner {edge:?}"
        );
    }
    // Without Repeat Edge Pixels the blur is half opaque at the layer's edges
    // on every side, at every resolution (it used to be cut or shifted).
    fx.param(&blur, 3, json!({"type": "bool", "value": false}));
    let (full, _) = shown(&mut fx, &comp, Full, [100, 120]).unwrap();
    let off = (0..4)
        .map(|k| (full[k] - [60, 40, 140, 200][k]).abs())
        .max()
        .unwrap();
    assert!(off <= 1, "{full:?}");
    for (res, s) in [(Half, 2), (Quarter, 4)] {
        let (got, _) = shown(&mut fx, &comp, res, [100, 120]).unwrap();
        let off = (0..4).map(|k| (got[k] - full[k]).abs()).max().unwrap();
        assert!(off <= 2 * s, "{res:?}: {got:?}, full {full:?}");
    }
}
