//! The editor and the lightweight viewer paint the same glyphs as the compositor.
use std::sync::{Arc, OnceLock, Weak, mpsc};

use bae_model::TextDocument;
use bae_text::TextLayout;
use egui::{Color32, Id, Painter, Pos2, Shape, TextureHandle};

pub(crate) struct Preview {
    doc: TextDocument,
    pub layout: TextLayout,
    image: OnceLock<(TextureHandle, [f64; 4])>,
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
                    if let Some((b, image)) = bae_text::render::preview_image(&preview.layout) {
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

impl Preview {
    pub fn get(painter: &Painter, id: Id, doc: &TextDocument) -> Option<Arc<Self>> {
        if let Some(previous) = painter.ctx().data(|d| d.get_temp::<Arc<Self>>(id))
            && previous.doc == *doc
        {
            return Some(previous);
        }
        let Some(layout) = crate::ae_panels::character::hit_layout(doc) else {
            painter
                .ctx()
                .request_repaint_after(std::time::Duration::from_millis(25));
            return None;
        };
        let preview = Arc::new(Self {
            doc: doc.clone(),
            layout,
            image: OnceLock::new(),
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
        let Some((texture, [x0, y0, x1, y1])) = self.image.get() else {
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
