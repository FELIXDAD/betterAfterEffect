//! Original footage samples and the metadata used by source-channel controls.
use crate::wgpu;
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

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

/// The rank-ordered ID/coverage planes of one Cryptomatte pass.
#[derive(Debug)]
pub struct CryptoLayer {
    pub name: String,
    /// Channel indices (into [`ChannelInfo::channels`]) per rank, -1 = none.
    pub ranks: Vec<[i32; 4]>,
    pub manifest: BTreeMap<u32, String>,
    /// First-ranked contribution at each source pixel, for synchronous viewport picks.
    pub dominant: Vec<u32>,
}

#[derive(Debug)]
pub struct ChannelInfo {
    pub channels: Vec<Channel>,
    pub crypto: Vec<CryptoLayer>,
    pub size: [u32; 2],
}

impl ChannelInfo {
    pub fn pick(&self, layer: &CryptoLayer, xy: [i32; 2]) -> Option<u32> {
        let [x, y] = xy.map(|i| i as u32);
        (x < self.size[0] && y < self.size[1])
            .then(|| {
                layer
                    .dominant
                    .get(y as usize * self.size[0] as usize + x as usize)
                    .copied()
            })
            .flatten()
            .filter(|&id| id != 0)
    }
    pub fn pick_bytes(&self) -> u64 {
        self.crypto
            .iter()
            .map(|l| l.dominant.len() as u64 * 4)
            .sum()
    }
    /// Prefer exact names before looking for a dot-qualified suffix.
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
                let Some(start) = c.name.len().checked_sub(name.len()) else {
                    return false;
                };
                c.name.get(..start).is_some_and(|s| s.ends_with('.'))
                    && c.name
                        .get(start..)
                        .is_some_and(|s| s.eq_ignore_ascii_case(name))
            }) {
                return Some(i);
            }
        }
        None
    }
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

/// The auxiliary channels of one decoded frame. Names (and Cryptomatte pick
/// tables) are known at decode; the planes are uploaded the first time an
/// effect asks for them, so footage without channel effects costs no extra
/// GPU memory.
pub struct FootageChannels {
    pub info: Arc<ChannelInfo>,
    planes: OnceLock<Option<ChannelPlanes>>,
    load: Loader,
}

impl std::fmt::Debug for FootageChannels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FootageChannels")
            .field("info", &self.info)
            .field("loaded", &self.planes.get().is_some())
            .finish_non_exhaustive()
    }
}

impl FootageChannels {
    pub fn new(
        info: Arc<ChannelInfo>,
        load: impl Fn() -> Option<ChannelPlanes> + Send + Sync + 'static,
    ) -> Self {
        Self {
            info,
            planes: OnceLock::new(),
            load: Box::new(load),
        }
    }

    /// The planes, uploaded on first use (`None` when that failed).
    pub fn planes(&self) -> Option<&ChannelPlanes> {
        self.planes.get_or_init(|| (self.load)()).as_ref()
    }

    /// Memory of the pick tables, plus the planes once uploaded.
    pub fn byte_size(&self) -> u64 {
        self.info.pick_bytes()
            + self
                .planes
                .get()
                .and_then(Option::as_ref)
                .map_or(0, |p| p.bytes)
    }
}
