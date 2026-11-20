//! Footage layers through `bae-media` (workstream 08): files are opened once,
//! frames decode straight into premultiplied RGBA16F textures on the shared
//! device and are sampled without a copy.
//!
//! Needs the `media` feature (on by default). Without it footage layers render
//! as missing with a warning; the app links the decoders through 08's module.

#![cfg_attr(not(feature = "media"), allow(dead_code))]

pub(crate) use imp::FootageCache;
#[cfg(feature = "media")]
pub use imp::FootageSource;

#[cfg(not(feature = "media"))]
mod imp {
    use std::path::Path;
    use std::sync::Arc;

    use bae_gpu::GpuContext;

    use crate::source::LayerSource;

    /// Stand-in without the `media` feature: every file is reported as unsupported.
    pub(crate) struct FootageCache;

    impl FootageCache {
        pub fn new(_gpu: Arc<GpuContext>) -> Self {
            Self
        }

        pub fn get(&self, path: &Path) -> Result<Arc<dyn LayerSource>, String> {
            Err(format!(
                "footage {}: bae-render was built without the `media` feature",
                path.display()
            ))
        }

        pub fn clear(&self) {}
    }
}

#[cfg(feature = "media")]
mod imp {

    use std::hash::{Hash, Hasher};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use bae_media::{AlphaMode, FrameRequest, MediaFormats, MediaGpu, MediaSource, OpenOptions};
    use bae_model::Time;
    use parking_lot::Mutex;
    use rustc_hash::{FxHashMap, FxHasher};

    use bae_gpu::GpuContext;

    use crate::error::RenderError;
    use crate::source::{EncodeContext, GpuImage, ImageAlpha, LayerSource, SourceRequest};

    /// A footage file as a layer source.
    pub struct FootageSource {
        path: PathBuf,
        media: Mutex<Box<dyn MediaSource>>,
        gpu: Arc<MediaGpu>,
        size: [u32; 2],
        identity: u64,
        last: Mutex<Option<(i64, GpuImage)>>,
    }

    impl std::fmt::Debug for FootageSource {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("FootageSource")
                .field("path", &self.path)
                .field("size", &self.size)
                .finish_non_exhaustive()
        }
    }

    impl FootageSource {
        /// Opens `path` with the built-in formats.
        pub fn open(
            path: &Path,
            formats: &MediaFormats,
            gpu: Arc<MediaGpu>,
        ) -> Result<Self, RenderError> {
            let err = |e: bae_media::MediaError| RenderError::Footage {
                path: path.to_path_buf(),
                message: e.to_string(),
            };
            let media = formats.open(path, &OpenOptions::default()).map_err(err)?;
            let video = media
                .info()
                .video
                .as_ref()
                .ok_or_else(|| RenderError::Footage {
                    path: path.to_path_buf(),
                    message: "no video stream".into(),
                })?;
            let size = [video.width, video.height];
            let mut h = FxHasher::default();
            path.hash(&mut h);
            if let Ok(meta) = std::fs::metadata(path) {
                meta.len().hash(&mut h);
                meta.modified().ok().hash(&mut h);
            }
            Ok(Self {
                path: path.to_path_buf(),
                media: Mutex::new(media),
                gpu,
                size,
                identity: h.finish(),
                last: Mutex::new(None),
            })
        }

        fn frame_index(&self, time: Time) -> i64 {
            self.media.lock().frame_at(time)
        }
    }

    impl LayerSource for FootageSource {
        fn size(&self) -> [u32; 2] {
            self.size
        }

        fn content_hash(&self, time: Time) -> u64 {
            let mut h = FxHasher::default();
            self.identity.hash(&mut h);
            self.frame_index(time).hash(&mut h);
            h.finish()
        }

        fn render(
            &self,
            _ctx: &mut EncodeContext<'_>,
            request: &SourceRequest,
        ) -> Result<GpuImage, RenderError> {
            let index = self.frame_index(request.time);
            if let Some((i, image)) = self.last.lock().as_ref()
                && *i == index
            {
                return Ok(image.clone());
            }
            let frame = self
                .media
                .lock()
                .decode_gpu(&FrameRequest::new(index), &self.gpu)
                .map_err(|e| RenderError::Footage {
                    path: self.path.clone(),
                    message: e.to_string(),
                })?;
            let alpha = match frame.alpha {
                AlphaMode::Straight => ImageAlpha::Straight,
                AlphaMode::Ignore => ImageAlpha::Opaque,
                AlphaMode::Premultiplied | AlphaMode::Opaque => ImageAlpha::Premultiplied,
            };
            let image = GpuImage::from_texture(frame.texture, alpha);
            *self.last.lock() = Some((index, image.clone()));
            Ok(image)
        }
    }

    /// Open footage files, by path.
    pub(crate) struct FootageCache {
        formats: MediaFormats,
        gpu: Arc<MediaGpu>,
        sources: FxHashMap<PathBuf, Result<Arc<FootageSource>, String>>,
    }

    impl FootageCache {
        pub fn new(gpu: Arc<GpuContext>) -> Self {
            Self {
                formats: MediaFormats::builtin(),
                gpu: Arc::new(MediaGpu::new(gpu)),
                sources: FxHashMap::default(),
            }
        }

        /// The source for `path` (opened once; failures are remembered and reported
        /// as warnings by the caller).
        pub fn get(&mut self, path: &Path) -> Result<Arc<dyn LayerSource>, String> {
            if let Some(entry) = self.sources.get(path) {
                return entry.clone().map(|s| s as Arc<dyn LayerSource>);
            }
            let entry = FootageSource::open(path, &self.formats, self.gpu.clone())
                .map(Arc::new)
                .map_err(|e| e.to_string());
            self.sources.insert(path.to_path_buf(), entry.clone());
            entry.map(|s| s as Arc<dyn LayerSource>)
        }

        /// Forgets every open file (after relinking assets).
        pub fn clear(&mut self) {
            self.sources.clear();
        }
    }
}
