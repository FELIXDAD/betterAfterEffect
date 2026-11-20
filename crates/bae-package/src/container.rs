//! Reading a `.baez` in place: embedded assets are found by name and read
//! straight from the container (media entries are stored uncompressed, so an
//! entry is a plain byte range of the file), without unpacking the package.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use zip::{CompressionMethod, ZipArchive};

use crate::error::{PackageError, io_err};

/// Prefix of embedded asset entries.
pub const ASSETS_DIR: &str = "assets/";

/// One embedded file of a `.baez`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PackagedEntry {
    /// Entry name, `/`-separated (`assets/footage/a.mov`).
    pub name: String,
    /// Uncompressed size in bytes.
    pub size: u64,
    /// Byte offset of the data in the container. For stored entries the data is
    /// the file itself: read `size` bytes at `offset`.
    pub offset: u64,
    /// Stored uncompressed (directly readable in place).
    pub stored: bool,
}

/// An open `.baez`: the list of embedded assets with their byte ranges.
#[derive(Clone, Debug)]
pub struct PackageReader {
    path: PathBuf,
    entries: BTreeMap<String, PackagedEntry>,
}

impl PackageReader {
    /// Index the assets embedded in a `.baez` (project data is read by
    /// `bae_model::Project::open`).
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PackageError> {
        let path = path.as_ref().to_owned();
        let file = fs::File::open(&path).map_err(io_err(&path))?;
        let mut zip = ZipArchive::new(io::BufReader::new(file))?;
        let mut entries = BTreeMap::new();
        for i in 0..zip.len() {
            let entry = zip.by_index(i)?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name()?.into_owned();
            if !name.starts_with(ASSETS_DIR) || !is_safe_name(&name) {
                continue;
            }
            let offset = entry
                .data_start()
                .ok_or_else(|| PackageError::Zip(format!("{name}: data offset unknown")))?;
            entries.insert(
                name.clone(),
                PackagedEntry {
                    name,
                    size: entry.size(),
                    offset,
                    stored: entry.compression() == CompressionMethod::Stored,
                },
            );
        }
        Ok(PackageReader { path, entries })
    }

    /// The `.baez` file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Embedded assets, by name.
    pub fn entries(&self) -> impl ExactSizeIterator<Item = &PackagedEntry> {
        self.entries.values()
    }

    pub fn entry(&self, name: &str) -> Option<&PackagedEntry> {
        self.entries.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// A reader over a stored entry's bytes, with its own file handle (several
    /// can be open at once, e.g. one per decoder thread).
    pub fn open_entry(&self, name: &str) -> Result<EntryReader, PackageError> {
        let entry = self.require(name)?;
        if !entry.stored {
            return Err(PackageError::Zip(format!(
                "{name} is compressed; use read_entry or materialize"
            )));
        }
        let mut file = fs::File::open(&self.path).map_err(io_err(&self.path))?;
        file.seek(SeekFrom::Start(entry.offset))
            .map_err(io_err(&self.path))?;
        Ok(EntryReader {
            file,
            start: entry.offset,
            len: entry.size,
            pos: 0,
        })
    }

    /// The whole entry in memory (small files; any compression).
    pub fn read_entry(&self, name: &str) -> Result<Vec<u8>, PackageError> {
        let entry = self.require(name)?;
        let mut buf = Vec::with_capacity(entry.size as usize);
        if entry.stored {
            self.open_entry(name)?
                .read_to_end(&mut buf)
                .map_err(io_err(&self.path))?;
        } else {
            let file = fs::File::open(&self.path).map_err(io_err(&self.path))?;
            let mut zip = ZipArchive::new(io::BufReader::new(file))?;
            zip.by_name(name)?
                .read_to_end(&mut buf)
                .map_err(io_err(&self.path))?;
        }
        Ok(buf)
    }

    /// Copy an entry out to `dest` (for decoders that need a file path).
    pub fn extract_entry(&self, name: &str, dest: &Path) -> Result<(), PackageError> {
        let entry = self.require(name)?;
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(io_err(parent))?;
        }
        let tmp = dest.with_file_name(format!(
            ".{}.part-{}",
            dest.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            std::process::id()
        ));
        let mut out = fs::File::create(&tmp).map_err(io_err(&tmp))?;
        let copied = if entry.stored {
            io::copy(&mut self.open_entry(name)?, &mut out)
        } else {
            let file = fs::File::open(&self.path).map_err(io_err(&self.path))?;
            let mut zip = ZipArchive::new(io::BufReader::new(file))?;
            let mut src = zip.by_name(name)?;
            io::copy(&mut src, &mut out)
        };
        copied.map_err(io_err(&tmp))?;
        drop(out);
        fs::rename(&tmp, dest).map_err(io_err(dest))
    }

    /// The entry as a file under `cache_dir` (same relative name), extracting it
    /// on first use. Reuses an earlier extraction of the same size.
    pub fn materialize(&self, name: &str, cache_dir: &Path) -> Result<PathBuf, PackageError> {
        let entry = self.require(name)?;
        let dest = name
            .split('/')
            .fold(cache_dir.to_owned(), |p, seg| p.join(seg));
        if fs::metadata(&dest).is_ok_and(|m| m.is_file() && m.len() == entry.size) {
            return Ok(dest);
        }
        self.extract_entry(name, &dest)?;
        Ok(dest)
    }

    fn require(&self, name: &str) -> Result<&PackagedEntry, PackageError> {
        self.entries
            .get(name)
            .ok_or_else(|| PackageError::NoSuchEntry(name.to_owned()))
    }
}

/// `Read + Seek` over one stored entry of a `.baez`.
#[derive(Debug)]
pub struct EntryReader {
    file: fs::File,
    start: u64,
    len: u64,
    pos: u64,
}

impl EntryReader {
    /// Entry size in bytes.
    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Read for EntryReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.len.saturating_sub(self.pos);
        if left == 0 || buf.is_empty() {
            return Ok(0);
        }
        let n = (buf.len() as u64).min(left) as usize;
        let read = self.file.read(&mut buf[..n])?;
        self.pos += read as u64;
        Ok(read)
    }
}

impl Seek for EntryReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(p) => Some(p),
            SeekFrom::End(d) => self.len.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        }
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before start"))?;
        self.file.seek(SeekFrom::Start(self.start + target))?;
        self.pos = target;
        Ok(target)
    }
}

/// A relative, `/`-separated name without `..`, empty or absolute segments.
pub(crate) fn is_safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && name
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != ".." && !s.contains(':'))
}
