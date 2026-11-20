//! Image sequence members on disk or inside a package.
//!
//! Same numbering rule as `bae-media::sequence` (workstream 08): the frame number
//! is the last run of digits in the file stem; zero-padded numbers fix the width.
//! Re-implemented here because `bae-media` depends on the GPU crate and packaging
//! must stay headless.

use std::path::{Path, PathBuf};

/// A file name split around its frame number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Numbered {
    pub prefix: String,
    pub number: i64,
    pub digits: usize,
    /// After the number, extension included.
    pub suffix: String,
}

impl Numbered {
    fn padded(&self) -> bool {
        self.digits > 1 && self.number.to_string().len() < self.digits
    }

    /// Whether `other` is a frame of the same sequence.
    pub fn same_sequence(&self, other: &Numbered) -> bool {
        if self.prefix != other.prefix || !self.suffix.eq_ignore_ascii_case(&other.suffix) {
            return false;
        }
        if self.padded() || other.padded() {
            self.digits == other.digits
        } else {
            true
        }
    }
}

/// Split `file_name` around the last run of digits in its stem.
pub(crate) fn parse(file_name: &str) -> Option<Numbered> {
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
    if digits.len() > 18 {
        return None;
    }
    Some(Numbered {
        prefix: stem[..start].to_owned(),
        number: digits.parse().ok()?,
        digits: digits.len(),
        suffix: format!("{}{ext}", &stem[end..]),
    })
}

/// The reference frame's name parts: from the file name, or from the `####`
/// pattern when the file name has no number.
fn reference(file_name: &str, pattern: Option<&str>) -> Option<Numbered> {
    if let Some(n) = parse(file_name) {
        return Some(n);
    }
    let pattern = pattern?;
    let first = pattern.find('#')?;
    let last = pattern.rfind('#')?;
    Some(Numbered {
        prefix: pattern[..first].to_owned(),
        number: 0,
        digits: last - first + 1,
        suffix: pattern[last + 1..].to_owned(),
    })
}

/// Every frame file of the sequence `file` belongs to, sorted by frame number.
/// A file without a frame number is its own one-frame sequence.
pub(crate) fn members_on_disk(file: &Path, pattern: Option<&str>) -> std::io::Result<Vec<PathBuf>> {
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Some(reference) = reference(&name, pattern) else {
        return Ok(vec![file.to_owned()]);
    };
    let dir = file.parent().unwrap_or(Path::new("."));
    let mut frames: Vec<(i64, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(dir)?.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_file() && !(kind.is_symlink() && entry.path().is_file()) {
            continue;
        }
        let candidate = entry.file_name().to_string_lossy().into_owned();
        if let Some(n) = parse(&candidate)
            && reference.same_sequence(&n)
        {
            frames.push((n.number, entry.path()));
        }
    }
    if frames.is_empty() {
        return Ok(vec![file.to_owned()]);
    }
    frames.sort();
    Ok(frames.into_iter().map(|(_, p)| p).collect())
}

/// Every entry of the sequence `entry` belongs to among `names` (zip entry names,
/// `/`-separated), sorted by frame number.
pub(crate) fn members_in<'a>(
    entry: &str,
    pattern: Option<&str>,
    names: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    let (dir, name) = entry.rsplit_once('/').unwrap_or(("", entry));
    let Some(reference) = reference(name, pattern) else {
        return vec![entry.to_owned()];
    };
    let mut frames: Vec<(i64, String)> = names
        .filter_map(|candidate| {
            let (d, n) = candidate.rsplit_once('/').unwrap_or(("", candidate));
            if d != dir {
                return None;
            }
            let parsed = parse(n)?;
            reference
                .same_sequence(&parsed)
                .then(|| (parsed.number, candidate.to_owned()))
        })
        .collect();
    if frames.is_empty() {
        return vec![entry.to_owned()];
    }
    frames.sort();
    frames.into_iter().map(|(_, n)| n).collect()
}

/// A directory name for a sequence inside the package: the file name without its
/// frame number (`shot_0001.png` → `shot`).
pub(crate) fn folder_name(file_name: &str) -> String {
    let base = match parse(file_name) {
        Some(n) => n.prefix,
        None => file_name
            .rsplit_once('.')
            .map_or(file_name, |(s, _)| s)
            .to_owned(),
    };
    let trimmed = base.trim_end_matches(['_', '.', '-', ' ']);
    if trimmed.is_empty() {
        "sequence".to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbering() {
        let n = parse("shot_0010.png").unwrap();
        assert_eq!((n.prefix.as_str(), n.number, n.digits), ("shot_", 10, 4));
        assert_eq!(n.suffix, ".png");
        assert!(n.same_sequence(&parse("shot_1000.PNG").unwrap()));
        assert!(!n.same_sequence(&parse("shot_10000.png").unwrap()));
        assert!(!n.same_sequence(&parse("take_0010.png").unwrap()));
        assert!(parse("plate.exr").is_none());
        assert_eq!(parse("a.1001.exr").unwrap().number, 1001);
        assert_eq!(folder_name("shot_0010.png"), "shot");
        assert_eq!(folder_name("0001.png"), "sequence");
        assert_eq!(folder_name("plate.exr"), "plate");
    }

    #[test]
    fn package_members() {
        let names = [
            "assets/s/a_001.png",
            "assets/s/a_002.png",
            "assets/s/b_001.png",
            "assets/t/a_003.png",
            "assets/s/a_010.png",
        ];
        assert_eq!(
            members_in("assets/s/a_002.png", None, names.iter().copied()),
            vec![
                "assets/s/a_001.png",
                "assets/s/a_002.png",
                "assets/s/a_010.png"
            ]
        );
        assert_eq!(
            members_in("assets/s/x.png", Some("a_###.png"), names.iter().copied()),
            vec![
                "assets/s/a_001.png",
                "assets/s/a_002.png",
                "assets/s/a_010.png"
            ]
        );
    }
}
