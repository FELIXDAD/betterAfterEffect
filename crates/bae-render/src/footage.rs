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

        pub fn get(
            &self,
            path: &Path,
            _asset: &bae_model::PrimPath,
            _revision: i64,
            _sequence: Option<bae_model::FrameRate>,
            _alphabetical: bool,
            _psd_layer: Option<bae_model::psd::LayerImage>,
        ) -> Result<Arc<dyn LayerSource>, String> {
            Err(format!(
                "footage {}: bae-render was built without the `media` feature",
                path.display()
            ))
        }

        pub fn channels(
            &self,
            _: &crate::input::FootageRef,
        ) -> Option<Arc<bae_gpu::channels::ChannelInfo>> {
            None
        }

        pub fn clear(&self) {}
    }
}

#[cfg(feature = "media")]
mod imp {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use bae_gpu::GpuContext;
    use bae_media::{
        AlphaMode, FrameCache, FrameRequest, MediaFormats, MediaGpu, MediaLibrary, OpenOptions,
        SequenceMode,
    };
    use bae_model::{FrameRate, Time};
    use rustc_hash::FxHashMap;

    use crate::error::RenderError;
    use crate::source::{EncodeContext, GpuImage, ImageAlpha, LayerSource, SourceRequest};

    /// Frame cache of a library the renderer creates for itself.
    const OWN_CACHE_BYTES: u64 = 1 << 30;

    /// A footage file as a layer source: a thin wrapper over 08's
    /// [`MediaLibrary`] (shared open files and frame cache, stable content hashes).
    pub struct FootageSource {
        library: Arc<MediaLibrary>,
        gpu: Arc<MediaGpu>,
        path: PathBuf,
        open: OpenOptions,
        request: FrameRequest,
        size: [u32; 2],
        rate: Option<FrameRate>,
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
        /// Opens `path` through `library`: alone, or (`sequence`) as the image
        /// sequence it starts at that frame rate.
        pub fn open(
            library: Arc<MediaLibrary>,
            gpu: Arc<MediaGpu>,
            path: &Path,
            sequence: Option<FrameRate>,
            psd_layer: Option<bae_model::psd::LayerImage>,
        ) -> Result<Self, RenderError> {
            Self::open_with_options(library, gpu, path, open_options(sequence, false, psd_layer))
        }

        fn open_with_options(
            library: Arc<MediaLibrary>,
            gpu: Arc<MediaGpu>,
            path: &Path,
            open: OpenOptions,
        ) -> Result<Self, RenderError> {
            let info = library
                .info(path, &open)
                .map_err(|e| RenderError::Footage {
                    path: path.to_path_buf(),
                    message: e.to_string(),
                })?;
            let video = info.video.as_ref().ok_or_else(|| RenderError::Footage {
                path: path.to_path_buf(),
                message: "no video stream".into(),
            })?;
            Ok(Self {
                size: [video.width, video.height],
                rate: video
                    .frame_rate
                    .filter(|_| info.kind != bae_media::MediaKind::Still),
                library,
                gpu,
                path: path.to_path_buf(),
                open,
                request: FrameRequest::new(0),
            })
        }
    }

    impl LayerSource for FootageSource {
        fn size(&self) -> [u32; 2] {
            self.size
        }

        fn content_hash(&self, time: Time) -> u64 {
            self.library
                .content_hash(&self.path, &self.open, &self.request, time)
                .unwrap_or(0)
        }

        fn frame_rate(&self) -> Option<FrameRate> {
            self.rate
        }

        fn render(
            &self,
            _ctx: &mut EncodeContext<'_>,
            request: &SourceRequest,
        ) -> Result<GpuImage, RenderError> {
            let frame = self
                .library
                .frame_at(
                    &self.path,
                    &self.open,
                    &self.request,
                    request.time,
                    &self.gpu,
                )
                .map_err(|e| RenderError::Footage {
                    path: self.path.clone(),
                    message: e.to_string(),
                })?;
            let alpha = match frame.alpha {
                AlphaMode::Straight => ImageAlpha::Straight,
                AlphaMode::Ignore => ImageAlpha::Opaque,
                AlphaMode::Premultiplied | AlphaMode::Opaque => ImageAlpha::Premultiplied,
            };
            let mut image = GpuImage::from_texture(frame.texture, alpha);
            image.channels = frame.channels;
            Ok(image)
        }
    }

    fn open_options(
        sequence: Option<FrameRate>,
        alphabetical: bool,
        psd_layer: Option<bae_model::psd::LayerImage>,
    ) -> OpenOptions {
        let mut options = sequence.map(OpenOptions::sequence).unwrap_or_default();
        options.psd_layer = psd_layer;
        if alphabetical && sequence.is_some() {
            options.sequence = SequenceMode::Alphabetical;
        }
        options
    }

    /// Footage sources by path, over one [`MediaLibrary`].
    pub(crate) struct FootageCache {
        library: Arc<MediaLibrary>,
        gpu: Arc<MediaGpu>,
        revisions: FxHashMap<bae_model::PrimPath, i64>,
        sources: FxHashMap<(PathBuf, OpenOptions), Result<Arc<FootageSource>, String>>,
    }

    impl FootageCache {
        pub fn new(gpu: Arc<GpuContext>) -> Self {
            let library = MediaLibrary::new(
                MediaFormats::builtin(),
                Arc::new(FrameCache::new(OWN_CACHE_BYTES)),
            );
            Self {
                library: Arc::new(library),
                gpu: Arc::new(MediaGpu::new(gpu)),
                revisions: FxHashMap::default(),
                sources: FxHashMap::default(),
            }
        }

        /// Uses a shared library (the app's `MediaModule` resource).
        pub fn set_library(&mut self, library: Arc<MediaLibrary>) {
            self.library = library;
            self.sources.clear();
        }

        /// The source for `path` (opened once; failures are remembered and
        /// reported as warnings by the caller).
        pub fn get(
            &mut self,
            path: &Path,
            asset: &bae_model::PrimPath,
            revision: i64,
            sequence: Option<FrameRate>,
            alphabetical: bool,
            psd_layer: Option<bae_model::psd::LayerImage>,
        ) -> Result<Arc<dyn LayerSource>, String> {
            if self.revisions.insert(asset.clone(), revision).unwrap_or(0) != revision {
                self.sources.retain(|(file, _), _| file != path);
                self.library.invalidate(path);
            }
            let options = open_options(sequence, alphabetical, psd_layer);
            let key = (path.to_path_buf(), options);
            if let Some(entry) = self.sources.get(&key) {
                return entry.clone().map(|s| s as Arc<dyn LayerSource>);
            }
            let entry = FootageSource::open_with_options(
                self.library.clone(),
                self.gpu.clone(),
                path,
                options,
            )
            .map(Arc::new)
            .map_err(|e| e.to_string());
            self.sources.insert(key, entry.clone());
            entry.map(|s| s as Arc<dyn LayerSource>)
        }

        pub fn channels(
            &self,
            footage: &crate::input::FootageRef,
        ) -> Option<Arc<bae_gpu::channels::ChannelInfo>> {
            let path = footage.file.as_ref()?;
            let source = self
                .sources
                .get(&(
                    path.clone(),
                    open_options(footage.sequence, footage.alphabetical, footage.psd_layer),
                ))?
                .as_ref()
                .ok()?;
            source
                .library
                .cached_channels(path, &source.open, &source.request, footage.source_time)
        }

        /// Forgets open files (they changed on disk or were relinked).
        pub fn clear(&mut self) {
            for (path, _) in self.sources.keys() {
                self.library.invalidate(path);
            }
            self.sources.clear();
        }
    }
}
