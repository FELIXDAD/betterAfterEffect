//! Selector weights, group composition and AE frame measurements.
use bae_anim::EvaluatedPrim;
use bae_model::{PrimPath, PrimType, TextDocument, Value};
use bae_text::{FontSystem, TextLayout, animator::apply_animators, layout_text};
use kurbo::{Affine, Vec2};

fn prim(ty: PrimType, attrs: &[(&str, Value)]) -> EvaluatedPrim {
    EvaluatedPrim {
        path: PrimPath::new("/test").unwrap(),
        prim_type: Some(ty),
        attrs: attrs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
        children: vec![],
    }
}

fn selector(based_on: &str, start: f64, end: f64, offset: f64) -> EvaluatedPrim {
    prim(
        PrimType::TEXT_SELECTOR,
        &[
            ("based_on", Value::Enum(based_on.into())),
            ("units", Value::Enum("index".into())),
            ("start", start.into()),
            ("end", end.into()),
            ("offset", offset.into()),
        ],
    )
}

#[test]
fn square_units_offset_reversal_and_add() {
    let mut fonts = FontSystem::new();
    let mut layout = layout_text(&mut fonts, &TextDocument::new("AB C\rD", "ArialMT", 40.0));
    let mut group = prim(
        PrimType::TEXT_ANIMATOR,
        &[
            ("animator:position", Value::Vec3([0.0, 20.0, 0.0])),
            ("animator:opacity", 20.0.into()),
        ],
    );
    for (unit, expected) in [
        ("characters", [0., 1., 0., 0., 0.]),
        ("characters_excluding_spaces", [0., 1., 0., 0., 0.]),
        ("words", [0., 0., 0., 1., 0.]),
        ("lines", [0., 0., 0., 0., 1.]),
    ] {
        group.children = vec![selector(unit, 1.0, 0.0, 1.0)];
        apply_animators(&mut layout, std::slice::from_ref(&group));
        let delta: Vec<_> = layout
            .styles
            .iter()
            .zip(&layout.chars)
            .filter(|(_, c)| c.kind != bae_text::CharKind::LineBreak)
            .map(|(s, c)| (s.transform * c.origin).y - c.origin.y)
            .collect();
        assert_eq!(delta, expected.map(|v| v * 20.), "{unit}");
    }
    group.children = vec![selector("characters_excluding_spaces", 2.25, 3.0, 0.0); 2];
    apply_animators(&mut layout, &[group]);
    let i = layout
        .chars
        .iter()
        .position(|c| c.source.start == 3)
        .unwrap();
    assert_eq!(layout.styles[i].transform.translation(), Vec2::new(0., 20.));
    assert!((layout.styles[i].opacity - 0.2).abs() < 1e-5);
}

#[test]
fn rotation_scale_color_and_balanced_tracking() {
    let mut fonts = FontSystem::new();
    let mut layout = layout_text(&mut fonts, &TextDocument::new("HH", "ArialMT", 40.0));
    let mut group = prim(
        PrimType::TEXT_ANIMATOR,
        &[
            ("animator:scale", Value::Vec3([300., 100., 100.])),
            ("animator:rotation", 180.0.into()),
            ("animator:tracking", 12.0.into()),
            ("animator:fill_color", Value::Color([1., 0., 0., 1.])),
        ],
    );
    group.children.push(selector("characters", 0., 0.5, 0.));
    let hash = layout.content_hash();
    apply_animators(&mut layout, &[group]);
    let pivot = layout.chars[0].origin + Vec2::new(layout.chars[0].advance * 0.5, 0.);
    let transformed = layout.styles[0].transform * (pivot + Vec2::new(1.0, 0.0));
    assert!((transformed - (pivot + Vec2::new(0., 2.))).hypot() < 1e-7);
    assert_eq!(layout.styles[0].paint.fill, Some([1., 0.5, 0.5, 1.]));
    assert_eq!(layout.styles[1].transform.translation(), Vec2::new(3., 0.));
    assert_ne!(layout.content_hash(), hash);
    apply_animators(&mut layout, &[]);
    assert_eq!(layout.content_hash(), hash);
}

// For each isolated H, measure the top ink row and peak alpha. Comparing the
// change from the plain frame removes platform differences in font hinting.
fn glyph_metrics(rgba: &[u8], width: usize) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    let mut ink = None;
    for x in 0..width {
        let mut column = rgba
            .chunks_exact(width * 4)
            .enumerate()
            .filter_map(|(y, row)| {
                let a = row[4 * x + 3];
                (a > 1).then_some((y as i32, i32::from(a)))
            })
            .peekable();
        if column.peek().is_some() {
            let (top, peak) = ink.get_or_insert((i32::MAX, 0));
            for (y, a) in column {
                *top = (*top).min(y);
                *peak = (*peak).max(a);
            }
        } else if let Some(metrics) = ink.take() {
            out.push(metrics);
        }
    }
    out.extend(ink);
    out
}

fn golden_metrics(bytes: &[u8]) -> Vec<(i32, i32)> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut frame = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut frame).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    glyph_metrics(&frame, info.width as usize)
}

fn render_metrics(layout: &TextLayout, width: u16, height: u16) -> Vec<(i32, i32)> {
    let mut ctx = vello_cpu::RenderContext::new(width, height);
    bae_text::render::draw_cpu(&mut ctx, layout, Affine::translate((16., 50.)));
    let mut pixels = vello_cpu::Pixmap::new(width, height);
    ctx.render(&mut pixels, &mut vello_cpu::Resources::new());
    glyph_metrics(pixels.data_as_u8_slice(), usize::from(width))
}

#[test]
fn square_position_and_opacity_match_ae_frames() {
    let spec: serde_json::Value = serde_json::from_str(include_str!("ae/cases.json")).unwrap();
    let (w, h) = (
        spec["width"].as_u64().unwrap() as u16,
        spec["height"].as_u64().unwrap() as u16,
    );
    let doc = TextDocument::new(
        spec["text"].as_str().unwrap(),
        spec["font"].as_str().unwrap(),
        spec["size"].as_f64().unwrap(),
    );
    let mut layout = layout_text(&mut FontSystem::new(), &doc);
    let base = render_metrics(&layout, w, h);
    let case = &spec["cases"][1];
    let number = |key: &str| case[key].as_f64().unwrap();
    let pos = &case["position"];
    let mut group = prim(
        PrimType::TEXT_ANIMATOR,
        &[
            (
                "animator:position",
                Value::Vec3([pos[0].as_f64().unwrap(), pos[1].as_f64().unwrap(), 0.]),
            ),
            ("animator:opacity", number("opacity").into()),
        ],
    );
    group.children.push(prim(
        PrimType::TEXT_SELECTOR,
        &[
            ("start", number("start").into()),
            ("end", number("end").into()),
            ("offset", number("offset").into()),
        ],
    ));
    apply_animators(&mut layout, &[group]);
    let animated = render_metrics(&layout, w, h);
    let ae_base = golden_metrics(include_bytes!("ae/golden/plain.png"));
    let ae = golden_metrics(include_bytes!("ae/golden/square_half.png"));
    for metrics in [&base, &animated, &ae_base, &ae] {
        assert_eq!(metrics.len(), 4);
    }
    for i in 0..4 {
        assert_eq!(animated[i].0 - base[i].0, ae[i].0 - ae_base[i].0);
        assert!((animated[i].1 - ae[i].1).abs() <= 1);
    }
}
