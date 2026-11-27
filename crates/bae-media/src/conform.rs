//! Persistent, source-rate floating-point audio for preview and scrubbing.
use crate::{AudioSource, FileAudio, MediaError, source_id_for};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use tempfile::NamedTempFile;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct CacheFolders {
    database: PathBuf,
    media: PathBuf,
}

impl CacheFolders {
    pub fn new(database_root: &Path, media_root: &Path) -> Self {
        let mut key = DefaultHasher::new();
        media_root.hash(&mut key);
        Self {
            database: database_root
                .join("BAE Media Index")
                .join(format!("{:016x}", key.finish())),
            media: media_root.join("BAE Conformed Audio"),
        }
    }

    fn entry(&self, id: u64) -> PathBuf {
        self.database.join(format!("{id:016x}.json"))
    }
    fn audio(&self, id: u64) -> PathBuf {
        self.media.join(format!("{id:016x}.wav"))
    }

    fn ready(&self, path: &Path, id: u64) -> Option<PathBuf> {
        let entry: Entry = serde_json::from_reader(File::open(self.entry(id)).ok()?).ok()?;
        let audio = self.audio(id);
        (entry.source == path
            && fs::metadata(&audio).is_ok_and(|m| m.is_file() && m.len() == entry.bytes))
        .then_some(audio)
    }

    fn conform(&self, path: &Path) -> io::Result<()> {
        let id = source_id_for(path);
        if self.ready(path, id).is_some() {
            return Ok(());
        }
        let mut source = FileAudio::open(path).map_err(io::Error::other)?;
        fs::create_dir_all(&self.media)?;
        fs::create_dir_all(&self.database)?;
        let mut audio = NamedTempFile::new_in(&self.media)?;
        let bytes = write_pcm(&mut source, audio.as_file_mut())?;
        if source_id_for(path) != id {
            return Ok(());
        }
        let mut entry = NamedTempFile::new_in(&self.database)?;
        serde_json::to_writer(
            entry.as_file_mut(),
            &Entry {
                source: path.to_owned(),
                bytes,
            },
        )?;
        audio.persist(self.audio(id))?;
        entry.persist(self.entry(id))?;
        Ok(())
    }

    /// Drop derived files only when their originals are missing. Permission or device
    /// errors do not establish that a file is missing.
    fn clean(&self) -> io::Result<()> {
        let entries = match fs::read_dir(&self.database) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            result => result?,
        };
        for file in entries.flatten() {
            let path = file.path();
            if !file.file_type().is_ok_and(|t| t.is_file())
                || path.extension().and_then(|x| x.to_str()) != Some("json")
            {
                continue;
            }
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|s| s.len() == 16)
                .and_then(|s| u64::from_str_radix(s, 16).ok())
            else {
                continue;
            };
            let Some(entry) = File::open(&path)
                .ok()
                .and_then(|f| serde_json::from_reader::<_, Entry>(f).ok())
            else {
                continue;
            };
            if fs::metadata(entry.source).is_err_and(|e| e.kind() == io::ErrorKind::NotFound) {
                match fs::remove_file(self.audio(id)) {
                    Err(e) if e.kind() != io::ErrorKind::NotFound => continue,
                    _ => {
                        fs::remove_file(path)?;
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
struct Entry {
    source: PathBuf,
    bytes: u64,
}

/// IEEE float WAV, preserving the decoder's native channel count, sample rate and headroom.
fn write_pcm(source: &mut FileAudio, file: &mut File) -> io::Result<u64> {
    let info = source.info();
    let channels = info.channels;
    let rate = info.sample_rate;
    let align = channels
        .checked_mul(4)
        .filter(|n| *n > 0)
        .ok_or_else(|| io::Error::other("invalid channel count"))?;
    let byte_rate = rate
        .checked_mul(u32::from(align))
        .ok_or_else(|| io::Error::other("invalid audio rate"))?;
    let max = u64::from(u32::MAX - 36);
    if info.frame_count.is_some_and(|n| n > max / u64::from(align)) {
        return Err(io::Error::other("audio exceeds RIFF capacity"));
    }
    let mut out = BufWriter::new(file);
    out.write_all(b"RIFF\0\0\0\0WAVEfmt ")?;
    for bytes in [
        &16_u32.to_le_bytes()[..],
        &3_u16.to_le_bytes(),
        &channels.to_le_bytes(),
        &rate.to_le_bytes(),
        &byte_rate.to_le_bytes(),
        &align.to_le_bytes(),
        &32_u16.to_le_bytes(),
        b"data\0\0\0\0",
    ] {
        out.write_all(bytes)?;
    }
    let mut pos = 0_u64;
    loop {
        let samples = source.read_window(pos, 32768).map_err(io::Error::other)?;
        if samples.is_empty() {
            break;
        }
        pos += (samples.len() / usize::from(channels)) as u64;
        if pos > max / u64::from(align) {
            return Err(io::Error::other("audio exceeds RIFF capacity"));
        }
        #[cfg(target_endian = "little")]
        out.write_all(bytemuck::cast_slice(samples))?;
        #[cfg(target_endian = "big")]
        for s in samples {
            out.write_all(&s.to_le_bytes())?;
        }
    }
    let bytes = (pos * u64::from(align)) as u32;
    out.seek(SeekFrom::Start(4))?;
    out.write_all(&(bytes + 36).to_le_bytes())?;
    out.seek(SeekFrom::Start(40))?;
    out.write_all(&bytes.to_le_bytes())?;
    out.flush()?;
    Ok(44 + u64::from(bytes))
}

enum Job {
    Prepare(CacheFolders, PathBuf),
    Clean(CacheFolders),
}

pub struct ConformedAudio {
    folders: RwLock<CacheFolders>,
    pending: Arc<Mutex<HashSet<(CacheFolders, PathBuf)>>>,
    jobs: mpsc::Sender<Job>,
    receiver: Mutex<Option<mpsc::Receiver<Job>>>,
}

impl ConformedAudio {
    pub fn open_source(cache: Option<&Self>, path: &Path) -> Result<FileAudio, MediaError> {
        cache.map_or_else(|| FileAudio::open(path), |c| c.open(path))
    }

    pub fn new(folders: CacheFolders) -> Self {
        let (jobs, receiver) = mpsc::channel();
        Self {
            folders: RwLock::new(folders),
            pending: Default::default(),
            jobs,
            receiver: Mutex::new(Some(receiver)),
        }
    }

    pub fn configure(&self, folders: CacheFolders) {
        *self.folders.write() = folders;
    }

    /// Safe on the UI thread, including during import. No files are opened here.
    pub fn prepare(&self, path: &Path) {
        let job = (self.folders.read().clone(), path.to_owned());
        if self.pending.lock().insert(job.clone()) {
            let _ = self.jobs.send(Job::Prepare(job.0, job.1));
        }
    }

    /// Called only on decode workers. The original remains immediately playable during conform.
    pub fn open(&self, path: &Path) -> Result<FileAudio, MediaError> {
        let id = source_id_for(path);
        let folders = self.folders.read().clone();
        if let Some(audio) = folders.ready(path, id) {
            if let Ok(source) = FileAudio::open(&audio) {
                return Ok(source);
            }
            // Let the worker replace a cache file whose decoder rejected its contents.
            let _ = fs::remove_file(folders.entry(id));
        }
        self.prepare(path);
        FileAudio::open(path)
    }

    pub fn clean(&self, folders: CacheFolders) {
        let _ = self.jobs.send(Job::Clean(folders));
    }

    fn start(&self) -> Result<(), String> {
        let Some(receiver) = self.receiver.lock().take() else {
            return Ok(());
        };
        let pending = self.pending.clone();
        std::thread::Builder::new()
            .name("bae-audio-conform".into())
            .spawn(move || {
                bae_gpu::warm_up::lower_priority();
                for job in receiver {
                    match job {
                        Job::Prepare(folders, path) => {
                            if let Err(e) = folders.conform(&path) {
                                tracing::debug!(%e, path = %path.display(), "audio conform unavailable");
                            }
                            pending.lock().remove(&(folders, path));
                        }
                        Job::Clean(folders) => {
                            if let Err(e) = folders.clean() {
                                tracing::warn!(%e, "could not clean the media cache");
                            }
                        }
                    }
                }
            })
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

impl bae_gpu::WarmUp for ConformedAudio {
    fn name(&self) -> &str {
        "conformed audio and media index"
    }
    fn priority(&self) -> u8 {
        85
    }
    fn run(&self, _: &Arc<bae_gpu::GpuContext>) -> Result<(), String> {
        self.start()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_reopens_pcm_and_clean_only_removes_missing_originals() {
        let dir = tempfile::tempdir().unwrap();
        let folders = CacheFolders::new(&dir.path().join("index"), &dir.path().join("pcm"));
        let source = dir.path().join("voice.wav");
        let mut wav = crate::wav::WavWriter::create(&source, 44_100).unwrap();
        wav.write(&[[0.25, -0.5], [0.5, 0.0]]).unwrap();
        wav.finish().unwrap();
        let id = source_id_for(&source);
        folders.conform(&source).unwrap();
        let audio = folders.ready(&source, id).unwrap();
        let mut decoded = ConformedAudio::new(folders.clone()).open(&source).unwrap();
        assert_eq!(
            (decoded.info().sample_rate, decoded.info().channels),
            (44100, 2)
        );
        assert_eq!(decoded.read_window(0, 2).unwrap(), &[0.25, -0.5, 0.5, 0.0]);
        drop(decoded);
        fs::write(folders.media.join("notes.wav"), b"unrelated").unwrap();
        folders.clean().unwrap();
        assert!(audio.is_file());
        fs::remove_file(&source).unwrap();
        folders.clean().unwrap();
        assert!(!audio.exists());
        assert!(!folders.entry(id).exists());
        assert!(folders.media.join("notes.wav").is_file());
    }
}
