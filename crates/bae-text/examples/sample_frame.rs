//! Renders a sample frame with shapes and text to a PNG (visual check, PR
//! screenshots): `cargo run -p bae-text --example sample_frame -- out.png`.

use std::sync::Arc;

use bae_gpu::transfer::read_texture;
use bae_gpu::{GpuContext, GpuContextDescriptor};
use bae_model::TextDocument;
use bae_text::{FontSystem, layout_text};
use bae_vector::render::VelloRenderer;
use bae_vector::{
    BlendMode, Brush, Composite, Direction, EllipseShape, FillRule, Gradient, GradientKind,
    GradientStop, GroupTransform, LineCap, LineJoin, Operator, Paint, PaintKind, PathItem,
    PathSource, RectShape, ShapeGroup, ShapeItem, StarShape, StarType, StrokeStyle, TrimPaths,
    VectorScene,
};
use kurbo::{Affine, Point, Rect};

fn path(source: PathSource) -> ShapeItem {
    ShapeItem::Path(PathItem {
        source,
        direction: Direction::Normal,
    })
}

fn fill(brush: Brush) -> ShapeItem {
    ShapeItem::Paint(Paint {
        kind: PaintKind::Fill(FillRule::NonZero),
        brush,
        opacity: 1.0,
        blend: BlendMode::Normal,
        composite: Composite::Below,
    })
}

fn stroke(color: [f32; 4], width: f64, dashes: Vec<f64>) -> ShapeItem {
    ShapeItem::Paint(Paint {
        kind: PaintKind::Stroke(StrokeStyle {
            width,
            cap: LineCap::Round,
            join: LineJoin::Round,
            dashes,
            ..StrokeStyle::default()
        }),
        brush: Brush::Solid(color),
        opacity: 1.0,
        blend: BlendMode::Normal,
        composite: Composite::Above,
    })
}

fn group(x: f64, y: f64, items: Vec<ShapeItem>) -> ShapeItem {
    ShapeItem::Group(ShapeGroup {
        transform: GroupTransform {
            position: [x, y],
            ..GroupTransform::default()
        },
        items,
        ..ShapeGroup::new()
    })
}

fn text(s: &str, size: f64, justification: &str) -> TextDocument {
    TextDocument {
        text: s.to_owned(),
        font: "PingFangSC-Semibold".to_owned(),
        font_size: size,
        fill_color: [1.0, 1.0, 1.0, 1.0],
        stroke_color: None,
        stroke_width: 0.0,
        stroke_over_fill: false,
        justification: justification.to_owned(),
        tracking: 0.0,
        leading: None,
        baseline_shift: 0.0,
        faux_bold: false,
        faux_italic: false,
        all_caps: false,
        small_caps: false,
        box_size: None,
        box_position: None,
    }
}

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sample_frame.png".to_owned());
    let gpu = GpuContext::new_headless(GpuContextDescriptor::default().with_env())
        .expect("a GPU adapter");
    let (w, h) = (960u32, 540u32);
    let orange = [1.0, 0.55, 0.1, 1.0];
    let blue = [0.25, 0.55, 1.0, 1.0];
    let root = ShapeGroup::new()
        // Rounded rectangle with a linear gradient and a white stroke.
        .with(group(
            150.0,
            150.0,
            vec![
                path(PathSource::Rect(RectShape {
                    size: [200.0, 140.0],
                    position: [0.0, 0.0],
                    roundness: 30.0,
                })),
                stroke([1.0; 4], 4.0, vec![]),
                fill(Brush::Gradient(Gradient {
                    kind: GradientKind::Linear,
                    start: Point::new(-100.0, 0.0),
                    end: Point::new(100.0, 0.0),
                    stops: vec![
                        GradientStop {
                            offset: 0.0,
                            color: orange,
                        },
                        GradientStop {
                            offset: 1.0,
                            color: blue,
                        },
                    ],
                })),
            ],
        ))
        // Star with rounded inner points, radial gradient.
        .with(group(
            420.0,
            150.0,
            vec![
                path(PathSource::Star(StarShape {
                    inner_roundness: 40.0,
                    outer_radius: 80.0,
                    inner_radius: 38.0,
                    ..StarShape::default()
                })),
                fill(Brush::Gradient(Gradient {
                    kind: GradientKind::Radial {
                        highlight_length: 40.0,
                        highlight_angle: -135.0,
                    },
                    start: Point::new(0.0, 0.0),
                    end: Point::new(80.0, 0.0),
                    stops: vec![
                        GradientStop {
                            offset: 0.0,
                            color: [1.0, 0.95, 0.5, 1.0],
                        },
                        GradientStop {
                            offset: 1.0,
                            color: [0.9, 0.3, 0.1, 1.0],
                        },
                    ],
                })),
            ],
        ))
        // Hexagon with a dashed stroke.
        .with(group(
            640.0,
            150.0,
            vec![
                path(PathSource::Star(StarShape {
                    star_type: StarType::Polygon,
                    points: 6.0,
                    outer_radius: 70.0,
                    ..StarShape::default()
                })),
                stroke(blue, 6.0, vec![14.0, 10.0]),
            ],
        ))
        // Ellipse drawn on with Trim Paths (75 %).
        .with(group(
            830.0,
            150.0,
            vec![
                path(PathSource::Ellipse(EllipseShape {
                    size: [140.0, 140.0],
                    position: [0.0, 0.0],
                })),
                stroke(orange, 12.0, vec![]),
                ShapeItem::Operator(Operator::Trim(TrimPaths {
                    end: 75.0,
                    ..TrimPaths::default()
                })),
            ],
        ));
    let shapes = VectorScene::from_group(&root);

    let mut fonts = FontSystem::new();
    let title = layout_text(
        &mut fonts,
        &text("BAE 矢量与文字 · Vector & Text", 44.0, "center"),
    );
    let mut para = text(
        "段落文本按文本框宽度换行，中文在汉字之间断行，句号、逗号这样的标点不会出现在行首。Latin words wrap at spaces.",
        26.0,
        "left",
    );
    para.font = "PingFangSC-Regular".to_owned();
    para.fill_color = [0.85, 0.88, 0.95, 1.0];
    para.box_size = Some([820.0, 150.0]);
    para.box_position = Some([0.0, 0.0]);
    let para = layout_text(&mut fonts, &para);

    let renderer = VelloRenderer::shared(&gpu).unwrap();
    let mut scene = vello::Scene::new();
    let bg = Rect::new(0.0, 0.0, f64::from(w), f64::from(h));
    scene.fill(
        vello::peniko::Fill::NonZero,
        Affine::IDENTITY,
        vello::peniko::Color::new([0.11, 0.12, 0.15, 1.0]),
        None,
        &bg,
    );
    bae_vector::render::encode(&mut scene, &shapes, Affine::IDENTITY, bg);
    bae_text::render::encode(&mut scene, &title, Affine::translate((480.0, 315.0)));
    bae_text::render::encode(&mut scene, &para, Affine::translate((70.0, 350.0)));
    let texture = renderer.render(&gpu, &scene, w, h).unwrap();
    let pixels = read_texture(&gpu, &texture).unwrap();

    let file = std::fs::File::create(&out).expect("create output");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&pixels.data).unwrap();
    println!("wrote {out} ({w}×{h}), paragraph overset: {}", para.overset);
    let _ = Arc::strong_count(&renderer);
}
