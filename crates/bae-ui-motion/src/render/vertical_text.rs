//! The editor and the lightweight viewer paint the same glyphs as the
//! compositor: the layout rasterized off the UI thread at the screen's scale
//! (rounded up to a power of two, so zooming re-rasterizes only now and then).
//! Until a new image is ready the previous one stays on screen, so typing
//! does not flicker.
use std::sync::{Arc, OnceLock, Weak, mpsc};

use bae_model::TextDocument;
use bae_text::TextLayout;
use egui::{Color32, Id, Painter, Pos2, Shape, TextureHandle};

type Image = (TextureHandle, [f64; 4]);

pub(crate) struct Preview {
    doc: TextDocument,
    pub layout: TextLayout,
    /// Raster pixels per layer pixel.
    scale: f32,
    image: OnceLock<Image>,
    /// The image shown before this one, until this one is ready.
    fallback: Option<Image>,
}

type Job = (Weak<Preview>, egui::Context);
static QUEUE: OnceLock<mpsc::Sender<Job>> = OnceLock::new();

/// Start once with the Character panel's font scan. Superseded edit frames
/// release their preview, so the worker skips them before rasterizing.
pub(crate) fn start_worker(init_fonts: impl FnOnce() + Send + 'static) {
    QUEUE.get_or_init(|| {
        let (send, receive) = mpsc::channel::<Job>();
        let _ = std::thread::Builder::new()
            .name("bae-text-preview".into())
            .spawn(move || {
                bae_gpu::warm_up::lower_priority();
                init_fonts();
                for (preview, ctx) in receive {
                    let Some(preview) = preview.upgrade() else {
                        continue;
                    };
                    let scale = f64::from(preview.scale);
                    if let Some((b, image)) =
                        bae_text::render::preview_image(&preview.layout, scale)
                    {
                        let size = [usize::from(image.width()), usize::from(image.height())];
                        let pixels = egui::ColorImage::from_rgba_premultiplied(
                            size,
                            image.data_as_u8_slice(),
                        );
                        let texture =
                            ctx.load_texture("vertical text", pixels, egui::TextureOptions::LINEAR);
                        let _ = preview.image.set((texture, [b.x0, b.y0, b.x1, b.y1]));
                        ctx.request_repaint();
                    }
                }
            });
        send
    });
}

/// `scale` rounded up to a power of two (1/8 … 8).
fn bucket(scale: f32) -> f32 {
    2f32.powf(scale.clamp(0.125, 8.0).log2().ceil())
}

impl Preview {
    /// The preview of `doc` drawn at `scale` screen pixels per layer pixel.
    pub fn get(painter: &Painter, id: Id, doc: &TextDocument, scale: f32) -> Option<Arc<Self>> {
        let scale = bucket(scale);
        let previous = painter.ctx().data(|d| d.get_temp::<Arc<Self>>(id));
        if let Some(p) = &previous
            && p.doc == *doc
            && p.scale == scale
        {
            return previous;
        }
        // Only the scale changed: the same layout.
        let layout = match &previous {
            Some(p) if p.doc == *doc => p.layout.clone(),
            _ => {
                let Some(layout) = crate::ae_panels::character::hit_layout(doc) else {
                    painter
                        .ctx()
                        .request_repaint_after(std::time::Duration::from_millis(25));
                    return previous;
                };
                layout
            }
        };
        let fallback = previous.and_then(|p| p.image.get().cloned().or_else(|| p.fallback.clone()));
        let preview = Arc::new(Self {
            doc: doc.clone(),
            layout,
            scale,
            image: OnceLock::new(),
            fallback,
        });
        if let Some(queue) = QUEUE.get() {
            let _ = queue.send((Arc::downgrade(&preview), painter.ctx().clone()));
        }
        painter
            .ctx()
            .data_mut(|d| d.insert_temp(id, preview.clone()));
        Some(preview)
    }

    pub fn shape(&self, screen: impl Fn([f64; 2]) -> Pos2, alpha: f32) -> Shape {
        let Some((texture, [x0, y0, x1, y1])) = self.image.get().or(self.fallback.as_ref()) else {
            return Shape::Noop;
        };
        let mut mesh = egui::Mesh::with_texture(texture.id());
        let color = Color32::WHITE.gamma_multiply(alpha);
        for (p, uv) in [
            ([*x0, *y0], [0., 0.]),
            ([*x1, *y0], [1., 0.]),
            ([*x1, *y1], [1., 1.]),
            ([*x0, *y1], [0., 1.]),
        ] {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: screen(p),
                uv: egui::pos2(uv[0], uv[1]),
                color,
            });
        }
        mesh.indices.extend([0, 1, 2, 0, 2, 3]);
        Shape::mesh(mesh)
    }
}
