//! Original footage samples and the metadata used by source-channel controls.
use crate::wgpu;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug)]
pub struct Channel {
    pub name: String,
    pub unsigned: bool,
}

/// The rank-ordered ID/coverage planes of one Cryptomatte pass.
#[derive(Debug)]
pub struct CryptoLayer {
    pub name: String,
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

#[derive(Debug)]
pub struct FootageChannels {
    pub view: wgpu::TextureView,
    pub info: Arc<ChannelInfo>,
}

impl FootageChannels {
    pub fn byte_size(&self) -> u64 {
        let [w, h] = self.info.size;
        u64::from(w) * u64::from(h) * self.info.channels.len() as u64 * 4 + self.info.pick_bytes()
    }
}
