//! Windows video decoding through Media Foundation — placeholder.
//!
//! Planned design (next PR, mirroring [`crate::apple`]): `IMFSourceReader`
//! with a DXGI device manager on a D3D11-on-12 device sharing the wgpu D3D12
//! device, hardware H.264 / HEVC decode into NV12 / P010 textures, opened in
//! D3D12 through shared handles and converted by [`MediaGpu`](crate::MediaGpu)
//! (`TEXTURE_FORMAT_NV12` / `TEXTURE_FORMAT_P010`). ProRes goes through the
//! optional FFmpeg plugin. Until then this format recognizes movies and reports
//! that decoding is not available yet.

use std::path::Path;

use crate::source::{MediaError, MediaFormat, MediaSource, sniff_iso_bmff};

/// QuickTime / MPEG-4 movies through Media Foundation (not implemented yet).
#[derive(Debug, Default, Clone, Copy)]
pub struct MediaFoundationFormat;

impl MediaFormat for MediaFoundationFormat {
    fn id(&self) -> &str {
        "mediafoundation"
    }

    fn extensions(&self) -> &[&str] {
        &["mp4", "mov", "m4v"]
    }

    fn sniff(&self, header: &[u8]) -> bool {
        sniff_iso_bmff(header)
    }

    fn is_image(&self) -> bool {
        false
    }

    fn open(&self, path: &Path) -> Result<Box<dyn MediaSource>, MediaError> {
        Err(MediaError::Unsupported(format!(
            "video decoding on Windows (Media Foundation) is not implemented yet: {}",
            path.display()
        )))
    }
}
