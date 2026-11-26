//! Uncompressed stereo 24-bit PCM WAV output. The final path is published
//! only after the sizes and data have been flushed successfully.
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

pub struct WavWriter {
    out: BufWriter<NamedTempFile>,
    path: PathBuf,
    bytes: u32,
}

impl WavWriter {
    pub fn create(path: &Path, rate: u32) -> io::Result<Self> {
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir)?;
        let mut out = BufWriter::new(NamedTempFile::new_in(dir)?);
        let byte_rate = rate
            .checked_mul(6)
            .filter(|r| *r > 0)
            .ok_or_else(|| io::Error::other("Invalid WAV sample rate"))?;
        out.write_all(b"RIFF\0\0\0\0WAVEfmt ")?;
        out.write_all(&16_u32.to_le_bytes())?;
        out.write_all(&1_u16.to_le_bytes())?;
        out.write_all(&2_u16.to_le_bytes())?;
        out.write_all(&rate.to_le_bytes())?;
        out.write_all(&byte_rate.to_le_bytes())?;
        out.write_all(&6_u16.to_le_bytes())?;
        out.write_all(&24_u16.to_le_bytes())?;
        out.write_all(b"data\0\0\0\0")?;
        Ok(Self {
            out,
            path: path.to_owned(),
            bytes: 0,
        })
    }
    pub fn write(&mut self, frames: &[[f32; 2]]) -> io::Result<()> {
        let bytes = (self.bytes as u64).saturating_add(frames.len() as u64 * 6);
        if bytes > u64::from(u32::MAX) - 36 {
            return Err(io::Error::other("PCM data exceeds the RIFF size limit"));
        }
        for &stereo in frames {
            for sample in stereo {
                let scaled = (sample.clamp(-1.0, 1.0) * 8_388_608.0).round() as i32;
                self.out
                    .write_all(&scaled.min(8_388_607).to_le_bytes()[..3])?;
            }
        }
        self.bytes = bytes as u32;
        Ok(())
    }
    pub fn finish(mut self) -> io::Result<u64> {
        self.out.seek(SeekFrom::Start(4))?;
        self.out.write_all(&(self.bytes + 36).to_le_bytes())?;
        self.out.seek(SeekFrom::Start(40))?;
        self.out.write_all(&self.bytes.to_le_bytes())?;
        self.out.flush()?;
        let temp = self.out.into_inner().map_err(|e| e.into_error())?;
        temp.persist(&self.path)?;
        Ok(u64::from(self.bytes) + 44)
    }
}
