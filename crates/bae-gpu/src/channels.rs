//! Native footage channels in display-window coordinates, independent of working color.
use std::collections::BTreeMap;

use crate::wgpu;

/// One array slice. FLOAT and HALF are stored as f32 bits, UINT as unsigned integers.
#[derive(Clone, Debug)]
pub struct Channel {
    pub name: String,
    pub unsigned: bool,
}

/// Channels captured with one decoded frame. The texture array is immutable.
#[derive(Debug)]
pub struct FootageChannels {
    pub view: wgpu::TextureView,
    pub channels: Vec<Channel>,
    /// EXR cryptomatte/<key>/{name,manifest,...} strings.
    pub attributes: BTreeMap<String, String>,
    pub size: [u32; 2],
}

impl FootageChannels {
    pub fn byte_size(&self) -> u64 {
        u64::from(self.size[0]) * u64::from(self.size[1]) * self.channels.len() as u64 * 4
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
