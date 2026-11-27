use egui::{ColorImage, Context};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, mpsc};

pub(super) type ResultImage = (PathBuf, Option<ColorImage>);

struct Request {
    path: PathBuf,
    reply: mpsc::Sender<ResultImage>,
    ctx: Context,
}

#[derive(Default)]
pub(super) struct Thumbnails(OnceLock<Option<mpsc::SyncSender<Request>>>);

impl Thumbnails {
    fn worker(&self) -> Option<&mpsc::SyncSender<Request>> {
        self.0
            .get_or_init(|| {
                let (tx, rx) = mpsc::sync_channel::<Request>(2);
                std::thread::Builder::new()
                    .name("bae-media-thumbnails".into())
                    .spawn(move || {
                        bae_gpu::warm_up::lower_priority();
                        let _ = crate::import::formats();
                        for job in rx {
                            let image = thumbnail(&job.path);
                            let _ = job.reply.send((job.path, image));
                            job.ctx.request_repaint();
                        }
                    })
                    .ok()?;
                Some(tx)
            })
            .as_ref()
    }

    pub(super) fn request(
        &self,
        path: &Path,
        reply: &mpsc::Sender<ResultImage>,
        ctx: &Context,
    ) -> bool {
        self.worker().is_some_and(|tx| {
            tx.try_send(Request {
                path: path.to_owned(),
                reply: reply.clone(),
                ctx: ctx.clone(),
            })
            .is_ok()
        })
    }
}

impl bae_gpu::WarmUp for Thumbnails {
    fn name(&self) -> &str {
        "media browser thumbnails"
    }
    fn run(&self, _: &Arc<bae_gpu::GpuContext>) -> Result<(), String> {
        self.worker()
            .map(|_| ())
            .ok_or_else(|| "could not start media thumbnail worker".into())
    }
}

fn thumbnail(path: &Path) -> Option<ColorImage> {
    let mut source = crate::import::formats()
        .open(path, &bae_media::OpenOptions::default())
        .ok()?;
    let frame = source
        .decode_cpu(&bae_media::FrameRequest::new(0).straight())
        .ok()?;
    let (w, h) = (frame.width, frame.height);
    if w == 0 || h == 0 {
        return None;
    }
    let scale = (96.0 / w as f64).min(64.0 / h as f64).min(1.0);
    let width = (w as f64 * scale).round().max(1.0) as u32;
    let height = (h as f64 * scale).round().max(1.0) as u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            rgba.extend(
                frame
                    .pixel_f32(x * w / width, y * h / height)
                    .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8),
            );
        }
    }
    Some(ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        &rgba,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_and_import_read_the_same_media_and_its_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("still.png");
        std::fs::write(&path, crate::tests::PNG_1X1).unwrap();
        std::fs::write(path.with_extension("xmp"), r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:dc="http://purl.org/dc/elements/1.1/"><rdf:Description><dc:title><rdf:Alt><rdf:li>Still</rdf:li></rdf:Alt></dc:title></rdf:Description></rdf:RDF>"#).unwrap();
        let image = thumbnail(&path).expect("decoded thumbnail");
        assert_eq!(image.size, [1, 1]);
        let result = crate::import::probe(&[path], &Default::default());
        assert_eq!(
            result.files[0].attrs["dc:title"],
            bae_model::Value::String("Still".into())
        );
    }
}
