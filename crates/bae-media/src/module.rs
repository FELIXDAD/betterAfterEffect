//! [`MediaModule`]: the media layer's app module.

use std::sync::Arc;

use bae_app::{AppBuilder, AppModule};

use crate::cache::FrameCache;
use crate::export::EncoderFormat;
use crate::library::MediaLibrary;
use crate::source::MediaFormat;
use crate::source::MediaFormats;
use crate::trim::Trimmer;

/// Default GPU memory budget of the shared [`FrameCache`]: 1 GiB (about 15 4K
/// or 60 1080p working-format frames).
pub const DEFAULT_CACHE_BUDGET: u64 = 1 << 30;

/// Registers the media layer:
///
/// * every built-in [`MediaFormat`] into `Registry<dyn MediaFormat>` under
///   `bae.media.<format id>` (plugins such as the optional FFmpeg plugin register
///   into the same registry; open files with
///   [`MediaFormats::from_registry`](crate::MediaFormats::from_registry)),
/// * every built-in [`EncoderFormat`] into `Registry<dyn EncoderFormat>`
///   (`bae.media.encoder.<id>`) and [`Trimmer`] into `Registry<dyn Trimmer>`
///   (`bae.media.trim.<id>`),
/// * resource `Arc<FrameCache>` with [`DEFAULT_CACHE_BUDGET`] unless another
///   module inserted one first,
/// * resource `Arc<MediaLibrary>` (in `finish`, over every registered format
///   and that cache) — the footage access the renderer and timeline share.
///
/// [`MediaGpu`](crate::MediaGpu) needs the `GpuContext`, which is created later
/// by the shell or the headless runner; the compositor creates it from there.
#[derive(Debug, Default)]
pub struct MediaModule;

impl AppModule for MediaModule {
    fn name(&self) -> &str {
        "bae-media"
    }

    fn build(&self, app: &mut AppBuilder) {
        for format in crate::builtin_formats() {
            let id = format!("bae.media.{}", format.id());
            if let Err(error) = app.register_arc::<dyn MediaFormat>(id, format) {
                tracing::warn!("media format not registered: {error}");
            }
        }
        for encoder in crate::builtin_encoders() {
            let id = format!("bae.media.encoder.{}", encoder.id());
            if let Err(error) = app.register_arc::<dyn EncoderFormat>(id, encoder) {
                tracing::warn!("encoder not registered: {error}");
            }
        }
        for trimmer in crate::builtin_trimmers() {
            let id = format!("bae.media.trim.{}", trimmer.id());
            if let Err(error) = app.register_arc::<dyn Trimmer>(id, trimmer) {
                tracing::warn!("trimmer not registered: {error}");
            }
        }
        if app.resource::<Arc<FrameCache>>().is_none() {
            app.insert_resource(Arc::new(FrameCache::new(DEFAULT_CACHE_BUDGET)));
        }
    }

    /// After every module registered its formats: the shared
    /// `Arc<MediaLibrary>` over them and the frame cache. (Formats a plugin
    /// registers later need a new library; the plugin host rebuilds it.)
    fn finish(&self, app: &mut AppBuilder) {
        if app.resource::<Arc<MediaLibrary>>().is_some() {
            return;
        }
        let formats = MediaFormats::from_registry(&app.registry::<dyn MediaFormat>());
        let cache = app
            .resource::<Arc<FrameCache>>()
            .cloned()
            .unwrap_or_else(|| Arc::new(FrameCache::new(DEFAULT_CACHE_BUDGET)));
        app.insert_resource(Arc::new(MediaLibrary::new(formats, cache)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MediaFormats;

    #[test]
    fn registers_builtin_formats_and_cache() {
        let mut builder = AppBuilder::new();
        builder.add_module(MediaModule);
        let mut app = builder.build();
        let registry = app.registry::<dyn MediaFormat>();
        for id in ["png", "jpeg", "webp", "exr"] {
            assert!(registry.contains(&format!("bae.media.{id}")), "{id}");
        }
        let formats = MediaFormats::from_registry(&registry);
        assert_eq!(formats.iter().count(), crate::builtin_formats().len());
        assert!(app.resource::<Arc<FrameCache>>().is_some());
        let library = app.resource::<Arc<MediaLibrary>>().unwrap();
        assert!(Arc::ptr_eq(
            library.cache(),
            app.resource::<Arc<FrameCache>>().unwrap()
        ));
        let encoders = app.registry::<dyn EncoderFormat>();
        assert!(encoders.contains("bae.media.encoder.image-sequence"));
        assert!(
            app.registry::<dyn Trimmer>()
                .contains("bae.media.trim.image-sequence")
        );
    }
}
