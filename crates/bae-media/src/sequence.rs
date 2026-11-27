//! Image sequences: numbered files (`name_0001.png`, `shot.1001.exr`) treated as
//! one source with one file per frame.
//!
//! The frame number is the **last run of digits** in the file stem (the same rule
//! AE, Nuke and most pipeline tools use). Zero-padded numbers fix the width
//! (`0001` → 4 digits, every frame must have 4 digits); unpadded numbers
//! (`f1.png … f120.png`) may vary in width.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::frame::{CpuFrame, FrameRequest, GpuFrame};
use crate::gpu::MediaGpu;
use crate::meta::{MediaInfo, MediaKind, SequenceInfo};
use crate::source::{MediaError, MediaFormat, MediaSource, check_index};
use crate::time::{DEFAULT_SEQUENCE_RATE, FrameRate, is_valid_rate};

/// A file name split around its frame number.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NumberedName {
    /// Everything before the number (`"name_"`).
    pub prefix: String,
    /// The frame number.
    pub number: i64,
    /// Digits as written (`4` for `0001`).
    pub digits: usize,
    /// Everything after the number, including the extension (`".png"`).
    pub suffix: String,
}

impl NumberedName {
    /// `true` if the number is written with leading zeros (fixed width).
    pub fn is_padded(&self) -> bool {
        self.digits > 1 && self.number.to_string().len() < self.digits
    }
}

/// Splits `file_name` around the last run of digits in its stem.
pub fn parse_numbered_name(file_name: &str) -> Option<NumberedName> {
    let (stem, ext) = match file_name.rfind('.') {
        Some(dot) if dot > 0 => (&file_name[..dot], &file_name[dot..]),
        _ => (file_name, ""),
    };
    let bytes = stem.as_bytes();
    let end = bytes.iter().rposition(u8::is_ascii_digit)? + 1;
    let start = bytes[..end]
        .iter()
        .rposition(|b| !b.is_ascii_digit())
        .map_or(0, |i| i + 1);
    let digits = &stem[start..end];
    // More than 18 digits is not a frame number (and would overflow i64).
    if digits.len() > 18 {
        return None;
    }
    Some(NumberedName {
        prefix: stem[..start].to_owned(),
        number: digits.parse().ok()?,
        digits: digits.len(),
        suffix: format!("{}{ext}", &stem[end..]),
    })
}

/// An image sequence found on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageSequence {
    /// Directory of the files.
    pub dir: PathBuf,
    /// File-name part before the number.
    pub prefix: String,
    /// File-name part after the number, including the extension.
    pub suffix: String,
    /// Zero-padded width, or 0 for unpadded numbers.
    pub padding: usize,
    /// Frame numbers present on disk, ascending, non-empty.
    pub frames: Vec<i64>,
    /// Actual paths in playback order for an alphabetical sequence.
    pub ordered_paths: Vec<PathBuf>,
}

impl ImageSequence {
    /// First frame number.
    pub fn first(&self) -> i64 {
        self.frames[0]
    }

    /// Last frame number.
    pub fn last(&self) -> i64 {
        self.frames[self.frames.len() - 1]
    }

    /// Number of frames from first to last, gaps included (the source length).
    pub fn len(&self) -> u64 {
        (self.last() - self.first() + 1) as u64
    }

    /// Always `false`: a sequence has at least one frame.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// `true` if frame `number` has a file.
    pub fn contains(&self, number: i64) -> bool {
        self.frames.binary_search(&number).is_ok()
    }

    /// Frame numbers between first and last that have no file.
    pub fn missing(&self) -> Vec<i64> {
        self.frames
            .windows(2)
            .flat_map(|pair| pair[0] + 1..pair[1])
            .collect()
    }

    /// The path of frame `number` (whether or not it exists).
    pub fn path_for(&self, number: i64) -> PathBuf {
        if let Some(path) = self.ordered_paths.get(number as usize) {
            return path.clone();
        }
        let digits = if number < 0 {
            format!("-{:0width$}", -number, width = self.padding)
        } else {
            format!("{number:0width$}", width = self.padding)
        };
        self.dir
            .join(format!("{}{digits}{}", self.prefix, self.suffix))
    }

    /// Display pattern with one `#` per padded digit (`name_####.png`), or `#`
    /// for unpadded numbers.
    pub fn pattern(&self) -> String {
        if !self.ordered_paths.is_empty() {
            return format!("*{}", self.suffix);
        }
        let hashes = "#".repeat(self.padding.max(1));
        format!("{}{hashes}{}", self.prefix, self.suffix)
    }

    /// Metadata block for [`MediaInfo::sequence`].
    pub fn info(&self) -> SequenceInfo {
        SequenceInfo {
            pattern: self.pattern(),
            first: self.first(),
            last: self.last(),
            missing: self.missing(),
        }
    }
}

/// Key that groups files of one sequence.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GroupKey {
    prefix: String,
    suffix_lower: String,
    /// Fixed width for padded names, 0 for unpadded ones.
    padding: usize,
}

fn group_key(name: &NumberedName) -> GroupKey {
    GroupKey {
        prefix: name.prefix.clone(),
        suffix_lower: name.suffix.to_ascii_lowercase(),
        padding: if name.is_padded() { name.digits } else { 0 },
    }
}

/// Whether `candidate` belongs to the same sequence as `reference`.
fn same_sequence(reference: &NumberedName, candidate: &NumberedName) -> bool {
    if reference.prefix != candidate.prefix
        || !reference.suffix.eq_ignore_ascii_case(&candidate.suffix)
    {
        return false;
    }
    match (reference.is_padded(), candidate.is_padded()) {
        // `0001` and `1000` are both 4-digit members of a padded sequence.
        (true, _) | (_, true) => reference.digits == candidate.digits,
        (false, false) => true,
    }
}

/// The image sequence `path` belongs to, or `None` if its name has no frame
/// number. A numbered file without siblings is a one-frame sequence.
pub fn detect_sequence(path: &Path) -> Result<Option<ImageSequence>, MediaError> {
    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return Ok(None);
    };
    let Some(reference) = parse_numbered_name(file_name) else {
        return Ok(None);
    };
    let dir = path.parent().unwrap_or(Path::new("")).to_owned();
    let read_dir = if dir.as_os_str().is_empty() {
        std::fs::read_dir(".")
    } else {
        std::fs::read_dir(&dir)
    }
    .map_err(|e| MediaError::io(&dir, e))?;
    let mut frames = Vec::new();
    let mut widths = std::collections::BTreeSet::new();
    let mut suffix = reference.suffix.clone();
    for entry in read_dir {
        let entry = entry.map_err(|e| MediaError::io(&dir, e))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(candidate) = parse_numbered_name(name) else {
            continue;
        };
        if same_sequence(&reference, &candidate) {
            if candidate.number == reference.number {
                suffix.clone_from(&candidate.suffix);
            }
            widths.insert(candidate.digits);
            frames.push(candidate.number);
        }
    }
    if frames.is_empty() {
        // The file itself was not listed (e.g. removed meanwhile).
        frames.push(reference.number);
        widths.insert(reference.digits);
    }
    frames.sort_unstable();
    frames.dedup();
    let padding = if reference.is_padded() || widths.len() == 1 && reference.digits > 1 {
        reference.digits
    } else {
        0
    };
    Ok(Some(ImageSequence {
        ordered_paths: Vec::new(),
        dir,
        prefix: reference.prefix,
        suffix,
        padding,
        frames,
    }))
}

/// Alphabetical import treats gaps and unrelated filenames as consecutive frames.
pub fn alphabetical_sequence(path: &Path) -> Result<ImageSequence, MediaError> {
    let ordered_paths =
        bae_model::asset::alphabetical_frames(path).map_err(|e| MediaError::io(path, e))?;
    if ordered_paths.is_empty() {
        return Err(MediaError::decode(path, "empty image sequence"));
    }
    Ok(ImageSequence {
        dir: path.parent().unwrap_or(Path::new(".")).into(),
        prefix: String::new(),
        suffix: format!(
            ".{}",
            path.extension().unwrap_or_default().to_string_lossy()
        ),
        padding: 0,
        frames: (0..ordered_paths.len() as i64).collect(),
        ordered_paths,
    })
}

/// One entry of [`scan_directory`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScannedItem {
    /// A file that is not part of a sequence.
    File(PathBuf),
    /// Two or more numbered files forming a sequence.
    Sequence(ImageSequence),
}

/// Groups the files of `dir` (non-recursive) into sequences and single files,
/// sorted by name. Only names with an extension in `extensions` (lower case) are
/// considered for sequences; pass an empty slice to accept any extension.
pub fn scan_directory(dir: &Path, extensions: &[&str]) -> Result<Vec<ScannedItem>, MediaError> {
    let mut groups: BTreeMap<GroupKey, Vec<(i64, String)>> = BTreeMap::new();
    let mut singles = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| MediaError::io(dir, e))? {
        let entry = entry.map_err(|e| MediaError::io(dir, e))?;
        if !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            singles.push(path);
            continue;
        };
        let ext_ok = extensions.is_empty()
            || path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| extensions.contains(&e.to_ascii_lowercase().as_str()));
        match parse_numbered_name(name).filter(|_| ext_ok) {
            Some(numbered) => groups
                .entry(group_key(&numbered))
                .or_default()
                .push((numbered.number, numbered.suffix)),
            None => singles.push(path),
        }
    }
    let mut items: Vec<ScannedItem> = singles.into_iter().map(ScannedItem::File).collect();
    for (key, mut members) in groups {
        members.sort_unstable_by_key(|(number, _)| *number);
        if members.len() == 1 {
            let (number, suffix) = &members[0];
            let seq = ImageSequence {
                ordered_paths: Vec::new(),
                dir: dir.to_owned(),
                prefix: key.prefix,
                suffix: suffix.clone(),
                padding: key.padding,
                frames: vec![*number],
            };
            items.push(ScannedItem::File(seq.path_for(*number)));
            continue;
        }
        let suffix = members[0].1.clone();
        items.push(ScannedItem::Sequence(ImageSequence {
            ordered_paths: Vec::new(),
            dir: dir.to_owned(),
            prefix: key.prefix,
            suffix,
            padding: key.padding,
            frames: members.into_iter().map(|(number, _)| number).collect(),
        }));
    }
    items.sort_by_key(item_sort_key);
    Ok(items)
}

fn item_sort_key(item: &ScannedItem) -> PathBuf {
    match item {
        ScannedItem::File(path) => path.clone(),
        ScannedItem::Sequence(seq) => seq.path_for(seq.first()),
    }
}

/// A [`MediaSource`] over an [`ImageSequence`]: frame index `i` is file number
/// `first + i`. Each frame is opened with the still-image format of the first
/// file.
pub struct ImageSequenceSource {
    format: Arc<dyn MediaFormat>,
    sequence: ImageSequence,
    info: MediaInfo,
}

impl ImageSequenceSource {
    /// Opens `sequence`, reading the metadata of its first file.
    pub fn open(
        format: Arc<dyn MediaFormat>,
        sequence: ImageSequence,
        frame_rate: FrameRate,
    ) -> Result<Self, MediaError> {
        let first_path = sequence.path_for(sequence.first());
        let first = format.open(&first_path)?;
        let mut info = first.info().clone();
        info.kind = MediaKind::ImageSequence;
        info.sequence = Some(sequence.info());
        if let Some(video) = info.video.as_mut() {
            video.frame_rate = Some(if is_valid_rate(frame_rate) {
                frame_rate
            } else {
                DEFAULT_SEQUENCE_RATE
            });
            video.frame_count = Some(sequence.len());
        }
        Ok(ImageSequenceSource {
            format,
            sequence,
            info,
        })
    }

    /// The underlying sequence.
    pub fn sequence(&self) -> &ImageSequence {
        &self.sequence
    }

    fn frame_source(&self, index: i64) -> Result<Box<dyn MediaSource>, MediaError> {
        check_index(&self.info, index)?;
        let number = self.sequence.first() + index;
        let path = self.sequence.path_for(number);
        if !self.sequence.contains(number) {
            return Err(MediaError::MissingFrame { number, path });
        }
        self.format.open(&path)
    }
}

impl MediaSource for ImageSequenceSource {
    fn info(&self) -> &MediaInfo {
        &self.info
    }

    fn decode_cpu(&mut self, request: &FrameRequest) -> Result<CpuFrame, MediaError> {
        let mut source = self.frame_source(request.index)?;
        source.decode_cpu(&FrameRequest {
            index: 0,
            ..*request
        })
    }

    fn decode_gpu(
        &mut self,
        request: &FrameRequest,
        gpu: &MediaGpu,
    ) -> Result<GpuFrame, MediaError> {
        let mut source = self.frame_source(request.index)?;
        source.decode_gpu(
            &FrameRequest {
                index: 0,
                ..*request
            },
            gpu,
        )
    }

    fn sync_frames(&mut self) -> Option<Vec<i64>> {
        Some((0..self.sequence.len() as i64).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_names() {
        let n = parse_numbered_name("name_0001.png").unwrap();
        assert_eq!(
            (n.prefix.as_str(), n.number, n.digits, n.suffix.as_str()),
            ("name_", 1, 4, ".png")
        );
        assert!(n.is_padded());
        let n = parse_numbered_name("shot.1001.exr").unwrap();
        assert_eq!((n.prefix.as_str(), n.number, n.digits), ("shot.", 1001, 4));
        assert!(!n.is_padded());
        let n = parse_numbered_name("comp_v2_0042_beauty.tif").unwrap();
        assert_eq!(
            (n.prefix.as_str(), n.number, n.suffix.as_str()),
            ("comp_v2_", 42, "_beauty.tif")
        );
        let n = parse_numbered_name("12.png").unwrap();
        assert_eq!((n.prefix.as_str(), n.number), ("", 12));
        assert!(parse_numbered_name("still.png").is_none());
        assert!(parse_numbered_name(".hidden").is_none());
    }

    #[test]
    fn paths_and_pattern() {
        let seq = ImageSequence {
            ordered_paths: Vec::new(),
            dir: PathBuf::from("/footage"),
            prefix: "name_".into(),
            suffix: ".png".into(),
            padding: 4,
            frames: vec![1, 2, 3, 5, 8],
        };
        assert_eq!(seq.path_for(7), PathBuf::from("/footage/name_0007.png"));
        assert_eq!(
            seq.path_for(12345),
            PathBuf::from("/footage/name_12345.png")
        );
        assert_eq!(seq.pattern(), "name_####.png");
        assert_eq!(seq.len(), 8);
        assert_eq!(seq.missing(), vec![4, 6, 7]);
        assert!(seq.contains(5) && !seq.contains(6));
    }

    fn touch(dir: &Path, names: &[&str]) {
        for name in names {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
    }

    #[test]
    fn detect_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        touch(
            dir.path(),
            &[
                "name_0001.png",
                "name_0002.png",
                "name_0004.png",
                "name_1000.png",
                "name_01.png",   // other width: not part of it
                "name_0003.jpg", // other extension
                "other_0001.png",
            ],
        );
        let seq = detect_sequence(&dir.path().join("name_0002.png"))
            .unwrap()
            .unwrap();
        assert_eq!(seq.frames, vec![1, 2, 4, 1000]);
        assert_eq!(seq.padding, 4);
        assert_eq!(seq.first(), 1);
        assert_eq!(seq.missing().len(), 996);

        // Unpadded names of varying width.
        touch(dir.path(), &["f1.exr", "f2.exr", "f10.exr"]);
        let seq = detect_sequence(&dir.path().join("f2.exr"))
            .unwrap()
            .unwrap();
        assert_eq!(seq.frames, vec![1, 2, 10]);
        assert_eq!(seq.padding, 0);
        assert_eq!(seq.path_for(10), dir.path().join("f10.exr"));

        assert!(
            detect_sequence(&dir.path().join("still.png"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn scan_groups_files() {
        let dir = tempfile::tempdir().unwrap();
        touch(
            dir.path(),
            &[
                "a_001.png",
                "a_002.png",
                "a_003.png",
                "logo.png",
                "b_7.png",
                "notes.txt",
            ],
        );
        let items = scan_directory(dir.path(), &["png"]).unwrap();
        assert_eq!(items.len(), 4, "{items:?}");
        let seqs: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                ScannedItem::Sequence(seq) => Some(seq),
                ScannedItem::File(_) => None,
            })
            .collect();
        assert_eq!(seqs.len(), 1);
        assert_eq!(seqs[0].frames, vec![1, 2, 3]);
        assert_eq!(seqs[0].pattern(), "a_###.png");
        assert!(items.contains(&ScannedItem::File(dir.path().join("b_7.png"))));
        assert!(items.contains(&ScannedItem::File(dir.path().join("notes.txt"))));
    }
}
