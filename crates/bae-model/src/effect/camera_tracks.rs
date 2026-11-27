//! Per-frame measurements shared by the tracker overlay and the effect renderer.
pub const DISPLAY_POINTS: u32 = 9001;

#[derive(Clone, Copy)]
pub struct TrackPoint {
    pub id: u32,
    pub source: [f32; 2],
    pub projected: [f32; 2],
    /// Negative for a 2D measurement without a solved landmark.
    pub error: f32,
}
impl TrackPoint {
    pub const BYTES: usize = 24;
    pub fn append(self, buffer: &mut Vec<u8>) {
        buffer.extend(self.id.to_le_bytes());
        for f in [
            self.source[0],
            self.source[1],
            self.projected[0],
            self.projected[1],
            self.error,
        ] {
            buffer.extend(f.to_le_bytes());
        }
    }
    pub fn read(buffer: &[u8]) -> impl Iterator<Item = Self> + '_ {
        buffer.as_chunks::<24>().0.iter().map(|row| {
            let words = row.as_chunks::<4>().0;
            Self {
                id: u32::from_le_bytes(words[0]),
                source: [f32::from_le_bytes(words[1]), f32::from_le_bytes(words[2])],
                projected: [f32::from_le_bytes(words[3]), f32::from_le_bytes(words[4])],
                error: f32::from_le_bytes(words[5]),
            }
        })
    }
    pub fn position(self, source: bool) -> Option<[f32; 2]> {
        if source {
            Some(self.source)
        } else {
            (self.error >= 0.).then_some(self.projected)
        }
    }
}
