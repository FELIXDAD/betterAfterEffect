//! A document is shared by its layer sources. Each source decodes just its own channels.
use crate::{
    AlphaMode, ColorInfo, CpuFrame, FrameRequest, MediaError, MediaFormat, MediaInfo, MediaKind,
    MediaSource, PixelAspect, PixelFormat, SampleType, TransferFunction, VideoInfo,
};
use bae_interop_psd::{bounds, decode, format::PsdFile};
use bae_model::psd::LayerImage;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Weak};
use std::time::SystemTime;

type DocumentCache = HashMap<(PathBuf, u64, SystemTime), Weak<PsdFile>>;
static DOCUMENTS: LazyLock<Mutex<DocumentCache>> = LazyLock::new(Mutex::default);

pub fn document(path: &Path) -> Result<Arc<PsdFile>, MediaError> {
    let metadata = path.metadata().map_err(|e| MediaError::io(path, e))?;
    let key = (
        path.to_owned(),
        metadata.len(),
        metadata.modified().map_err(|e| MediaError::io(path, e))?,
    );
    if let Some(doc) = DOCUMENTS.lock().get(&key).and_then(Weak::upgrade) {
        return Ok(doc);
    }
    let bytes = std::fs::read(path).map_err(|e| MediaError::io(path, e))?;
    let doc = Arc::new(PsdFile::from_bytes(&bytes).map_err(|e| MediaError::decode(path, e))?);
    let mut cache = DOCUMENTS.lock();
    cache.retain(|_, w| w.strong_count() > 0);
    cache.insert(key, Arc::downgrade(&doc));
    Ok(doc)
}

pub struct Photoshop;
impl MediaFormat for Photoshop {
    fn id(&self) -> &str {
        "psd"
    }
    fn extensions(&self) -> &[&str] {
        &["psd", "psb"]
    }
    fn sniff(&self, header: &[u8]) -> bool {
        header.starts_with(b"8BPS")
    }
    fn is_image(&self) -> bool {
        false
    }
    fn open(&self, path: &Path) -> Result<Box<dyn MediaSource>, MediaError> {
        open(path, None)
    }
}

pub fn open(path: &Path, layer: Option<LayerImage>) -> Result<Box<dyn MediaSource>, MediaError> {
    let doc = document(path)?;
    let rect = bounds(&doc, layer).map_err(|e| MediaError::decode(path, e))?;
    let depth = doc.header.depth;
    let mut color = ColorInfo::srgb();
    if depth == 32 {
        color.transfer = TransferFunction::Linear;
    }
    color.icc_profile = doc.icc_profile().map(Arc::from);
    let video = VideoInfo {
        width: rect.width() as u32,
        height: rect.height() as u32,
        bit_depth: depth as u8,
        sample_type: if depth == 32 {
            SampleType::Float
        } else {
            SampleType::UInt
        },
        alpha: AlphaMode::Straight,
        color,
        frame_rate: None,
        frame_count: None,
        pixel_aspect: PixelAspect::SQUARE,
        codec: "photoshop".into(),
    };
    let info = MediaInfo {
        path: path.to_owned(),
        kind: MediaKind::Still,
        format: "psd".into(),
        video: Some(video),
        audio: vec![],
        timecode: None,
        sequence: None,
    };
    Ok(Box::new(Source { doc, layer, info }))
}

struct Source {
    doc: Arc<PsdFile>,
    layer: Option<LayerImage>,
    info: MediaInfo,
}
impl MediaSource for Source {
    fn info(&self) -> &MediaInfo {
        &self.info
    }
    fn decode_cpu(&mut self, _: &FrameRequest) -> Result<CpuFrame, MediaError> {
        let pixels =
            decode(&self.doc, self.layer).map_err(|e| MediaError::decode(&self.info.path, e))?;
        let video = self.info.video.as_ref().expect("PSD video metadata");
        Ok(CpuFrame {
            width: pixels.width,
            height: pixels.height,
            format: PixelFormat::Rgba32Float,
            data: bytemuck::cast_slice(&pixels.rgba).to_vec(),
            alpha: video.alpha,
            color: video.color.clone(),
            pixel_aspect: video.pixel_aspect,
        })
    }
}

impl bae_gpu::WarmUp for Photoshop {
    fn name(&self) -> &str {
        "Photoshop decoder"
    }
    fn run(&self, _: &Arc<bae_gpu::GpuContext>) -> Result<(), String> {
        let file = PsdFile::from_bytes(include_bytes!(
            "../../bae-interop-psd/tests/fixtures/ink16.psd"
        ))
        .map_err(|e| e.to_string())?;
        let selection = LayerImage {
            key: bae_model::psd::LayerKey::Id(37),
            canvas: false,
        };
        decode(&file, Some(selection))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
