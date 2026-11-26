//! Native footage channels in display-window coordinates, independent of working color.
use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::wgpu;

/// How a channel's samples are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelKind {
    /// f32 bits in [`ChannelPlanes::words`].
    Float,
    /// Half bits in [`ChannelPlanes::halves`].
    Half,
    /// Unsigned integers in [`ChannelPlanes::words`].
    Uint,
}

/// One auxiliary channel (the beauty pass is the image itself, not a channel).
#[derive(Clone, Debug)]
pub struct Channel {
    pub name: String,
    pub kind: ChannelKind,
    /// Its layer in the array of its kind.
    pub slice: u32,
}

/// The uploaded channel planes: `R32Uint` and `R16Uint` 2D arrays (at least
/// one 1×1 layer each, so both always bind).
#[derive(Debug)]
pub struct ChannelPlanes {
    pub words: wgpu::TextureView,
    pub halves: wgpu::TextureView,
    pub bytes: u64,
}

type Loader = Box<dyn Fn() -> Option<ChannelPlanes> + Send + Sync>;

/// The auxiliary channels of one decoded frame. Names are known at decode;
/// the planes are uploaded the first time an effect asks for them, so
/// footage without channel effects costs no extra GPU memory.
pub struct FootageChannels {
    pub channels: Vec<Channel>,
    /// EXR cryptomatte/<key>/{name,manifest,...} strings.
    pub attributes: BTreeMap<String, String>,
    pub size: [u32; 2],
    planes: OnceLock<Option<ChannelPlanes>>,
    load: Loader,
}

impl std::fmt::Debug for FootageChannels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FootageChannels")
            .field("channels", &self.channels)
            .field("size", &self.size)
            .field("loaded", &self.planes.get().is_some())
            .finish_non_exhaustive()
    }
}

impl FootageChannels {
    pub fn new(
        channels: Vec<Channel>,
        attributes: BTreeMap<String, String>,
        size: [u32; 2],
        load: impl Fn() -> Option<ChannelPlanes> + Send + Sync + 'static,
    ) -> Self {
        Self {
            channels,
            attributes,
            size,
            planes: OnceLock::new(),
            load: Box::new(load),
        }
    }

    /// The planes, uploaded on first use (`None` when that failed).
    pub fn planes(&self) -> Option<&ChannelPlanes> {
        self.planes.get_or_init(|| (self.load)()).as_ref()
    }

    /// GPU memory of the planes once uploaded.
    pub fn byte_size(&self) -> u64 {
        self.planes
            .get()
            .and_then(Option::as_ref)
            .map_or(0, |p| p.bytes)
    }

    /// Prefer an exact name, then a qualified name such as ViewLayer.Depth.Z.
    pub fn find(&self, names: &[&str]) -> Option<usize> {
        for name in names {
            if let Some(i) = self
                .channels
                .iter()
                .position(|c| c.name.eq_ignore_ascii_case(name))
            {
                return Some(i);
            }
        }
        for name in names {
            if let Some(i) = self.channels.iter().position(|c| {
                c.name
                    .get(..c.name.len().saturating_sub(name.len()))
                    .is_some_and(|p| p.ends_with('.'))
                    && c.name
                        .get(c.name.len().saturating_sub(name.len())..)
                        .is_some_and(|s| s.eq_ignore_ascii_case(name))
            }) {
                return Some(i);
            }
        }
        None
    }
}
