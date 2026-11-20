//! Minimal ISO-BMFF / QuickTime box reading: just enough to find the encoder
//! delay ("priming") and valid length of the first audio track, which the
//! symphonia MP4 reader does not apply. Without it AAC audio starts ~44 ms late
//! (2112 samples at 48 kHz) against the picture.
//!
//! Two signals are understood:
//! * the audio track's edit list (`moov/trak/edts/elst`): the first edit's
//!   media time is the priming, its duration the valid length (cameras,
//!   AVAssetWriter, AE);
//! * iTunes gapless info (`iTunSMPB` in `moov/udta/meta/ilst`): priming,
//!   padding and the original sample count (afconvert, iTunes, many encoders).

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Gapless trimming of an audio track, in sample frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AudioTrim {
    /// Decoded frames to drop at the start.
    pub priming: u64,
    /// Valid frames after the priming, if known.
    pub valid: Option<u64>,
}

/// Reads the trim of the first audio track of an MP4 / QuickTime file.
/// `None` when the file has neither signal or cannot be parsed.
pub(crate) fn audio_trim(path: &Path, sample_rate: u32) -> Option<AudioTrim> {
    let moov = read_moov(path)?;
    let movie_timescale = find(&moov, b"mvhd").and_then(mvhd_timescale)?;
    let track = children(&moov)
        .filter(|(kind, _)| kind == b"trak")
        .map(|(_, body)| body)
        .find(|trak| is_sound(trak))?;
    if let Some(trim) = elst_trim(track, movie_timescale, sample_rate) {
        return Some(trim);
    }
    itunsmpb(&moov)
}

fn read_moov(path: &Path) -> Option<Vec<u8>> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut offset = 0_u64;
    while offset + 8 <= len {
        file.seek(SeekFrom::Start(offset)).ok()?;
        let mut header = [0_u8; 16];
        file.read_exact(&mut header[..8]).ok()?;
        let mut size = u64::from(u32::from_be_bytes(header[..4].try_into().ok()?));
        let mut header_len = 8;
        if size == 1 {
            file.read_exact(&mut header[8..16]).ok()?;
            size = u64::from_be_bytes(header[8..16].try_into().ok()?);
            header_len = 16;
        } else if size == 0 {
            size = len - offset;
        }
        if size < header_len {
            return None;
        }
        if &header[4..8] == b"moov" {
            // A movie header larger than 64 MB is not something to parse here.
            let body_len = usize::try_from(size - header_len).ok()?.min(64 << 20);
            let mut body = vec![0; body_len];
            file.read_exact(&mut body).ok()?;
            return Some(body);
        }
        offset += size;
    }
    None
}

/// Iterates the child boxes of a container body: (type, body).
fn children(body: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> + '_ {
    let mut offset = 0_usize;
    std::iter::from_fn(move || {
        if offset + 8 > body.len() {
            return None;
        }
        let size32 = u32::from_be_bytes(body[offset..offset + 4].try_into().ok()?) as usize;
        let kind: [u8; 4] = body[offset + 4..offset + 8].try_into().ok()?;
        let (size, header) = match size32 {
            1 => (
                usize::try_from(u64::from_be_bytes(
                    body.get(offset + 8..offset + 16)?.try_into().ok()?,
                ))
                .ok()?,
                16,
            ),
            0 => (body.len() - offset, 8),
            n => (n, 8),
        };
        if size < header || offset + size > body.len() {
            return None;
        }
        let child = &body[offset + header..offset + size];
        offset += size;
        Some((kind, child))
    })
}

/// The first box of `kind` among the children of `body`.
fn find<'a>(body: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    children(body).find(|(k, _)| k == kind).map(|(_, b)| b)
}

/// Follows a path of box types.
fn find_path<'a>(body: &'a [u8], path: &[&[u8; 4]]) -> Option<&'a [u8]> {
    path.iter().try_fold(body, |body, kind| find(body, kind))
}

fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn be_u64(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

/// `mvhd` / `mdhd` timescale (full box: version byte + flags first).
fn mvhd_timescale(body: &[u8]) -> Option<u32> {
    let version = *body.first()?;
    be_u32(body, if version == 1 { 20 } else { 12 })
}

fn is_sound(trak: &[u8]) -> bool {
    // hdlr: version/flags (4), pre_defined (4), handler_type (4).
    find_path(trak, &[b"mdia", b"hdlr"]).is_some_and(|hdlr| hdlr.get(8..12) == Some(b"soun"))
}

fn elst_trim(trak: &[u8], movie_timescale: u32, sample_rate: u32) -> Option<AudioTrim> {
    let media_timescale = find_path(trak, &[b"mdia", b"mdhd"]).and_then(mvhd_timescale)?;
    let elst = find_path(trak, &[b"edts", b"elst"])?;
    let version = *elst.first()?;
    let count = be_u32(elst, 4)?;
    let mut at = 8;
    for _ in 0..count {
        let (duration, media_time) = if version == 1 {
            let d = be_u64(elst, at)?;
            let m = be_u64(elst, at + 8)? as i64;
            at += 20;
            (d, m)
        } else {
            let d = u64::from(be_u32(elst, at)?);
            let m = i64::from(be_u32(elst, at + 4)? as i32);
            at += 12;
            (d, m)
        };
        // -1 = empty edit (a delay before the track starts); skip it.
        if media_time < 0 {
            continue;
        }
        let to_frames = |value: u64, timescale: u32| {
            (u128::from(value) * u128::from(sample_rate) / u128::from(timescale.max(1))) as u64
        };
        return Some(AudioTrim {
            priming: to_frames(media_time as u64, media_timescale),
            valid: (duration > 0).then(|| to_frames(duration, movie_timescale)),
        });
    }
    None
}

/// iTunes gapless info: `" 00000000 PRIMING PADDING SAMPLECOUNT ..."` (hex).
fn itunsmpb(moov: &[u8]) -> Option<AudioTrim> {
    let meta = find_path(moov, &[b"udta", b"meta"])?;
    // `meta` is a full box: skip version and flags.
    let ilst = find(meta.get(4..)?, b"ilst")?;
    for (kind, item) in children(ilst) {
        if &kind != b"----" {
            continue;
        }
        let name = find(item, b"name").and_then(|n| n.get(4..));
        if name != Some(b"iTunSMPB".as_slice()) {
            continue;
        }
        // data: version/flags (4), locale (4), then the text.
        let text = find(item, b"data").and_then(|d| d.get(8..))?;
        let text = std::str::from_utf8(text).ok()?;
        let fields: Vec<u64> = text
            .split_whitespace()
            .filter_map(|f| u64::from_str_radix(f, 16).ok())
            .collect();
        let priming = *fields.get(1)?;
        let valid = fields.get(3).copied().filter(|&n| n > 0);
        return Some(AudioTrim { priming, valid });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    fn mvhd(timescale: u32) -> Vec<u8> {
        let mut body = vec![0_u8; 100];
        body[12..16].copy_from_slice(&timescale.to_be_bytes());
        boxed(b"mvhd", &body)
    }

    fn sound_trak(edit: Option<(u32, i32)>, media_timescale: u32) -> Vec<u8> {
        let mut hdlr = vec![0_u8; 24];
        hdlr[8..12].copy_from_slice(b"soun");
        let mut mdhd = vec![0_u8; 24];
        mdhd[12..16].copy_from_slice(&media_timescale.to_be_bytes());
        let mdia = boxed(
            b"mdia",
            &[boxed(b"mdhd", &mdhd), boxed(b"hdlr", &hdlr)].concat(),
        );
        let mut body = Vec::new();
        if let Some((duration, media_time)) = edit {
            let mut elst = vec![0, 0, 0, 0];
            elst.extend_from_slice(&2_u32.to_be_bytes());
            // An empty edit first, then the real one.
            elst.extend_from_slice(&100_u32.to_be_bytes());
            elst.extend_from_slice(&(-1_i32).to_be_bytes());
            elst.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
            elst.extend_from_slice(&duration.to_be_bytes());
            elst.extend_from_slice(&media_time.to_be_bytes());
            elst.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
            body.extend(boxed(b"edts", &boxed(b"elst", &elst)));
        }
        body.extend(mdia);
        boxed(b"trak", &body)
    }

    #[test]
    fn edit_list_priming() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.mov");
        // Movie timescale 600, audio media timescale 48 kHz: 2112 priming
        // samples, 1 s of valid audio.
        let moov = boxed(
            b"moov",
            &[mvhd(600), sound_trak(Some((600, 2112)), 48_000)].concat(),
        );
        let file = [
            boxed(b"ftyp", b"qt  \0\0\0\0"),
            boxed(b"mdat", &[0; 32]),
            moov,
        ]
        .concat();
        std::fs::write(&path, file).unwrap();
        assert_eq!(
            audio_trim(&path, 48_000),
            Some(AudioTrim {
                priming: 2112,
                valid: Some(48_000)
            })
        );
    }

    #[test]
    fn itunes_gapless_info() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.m4a");
        let text = b" 00000000 00000840 000003C0 000000000000BB80 00000000";
        let mut data = vec![0_u8; 8];
        data.extend_from_slice(text);
        let mut name = vec![0_u8; 4];
        name.extend_from_slice(b"iTunSMPB");
        let item = boxed(
            b"----",
            &[
                boxed(b"mean", b"\0\0\0\0com.apple.iTunes"),
                boxed(b"name", &name),
                boxed(b"data", &data),
            ]
            .concat(),
        );
        let mut meta = vec![0_u8; 4];
        meta.extend(boxed(b"ilst", &item));
        let udta = boxed(b"udta", &boxed(b"meta", &meta));
        let moov = boxed(
            b"moov",
            &[mvhd(48_000), sound_trak(None, 48_000), udta].concat(),
        );
        std::fs::write(&path, [boxed(b"ftyp", b"M4A \0\0\0\0"), moov].concat()).unwrap();
        assert_eq!(
            audio_trim(&path, 48_000),
            Some(AudioTrim {
                priming: 0x840,
                valid: Some(0xBB80)
            })
        );
    }

    #[test]
    fn no_signal_or_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("plain.mp4");
        let moov = boxed(b"moov", &[mvhd(1000), sound_trak(None, 44_100)].concat());
        std::fs::write(&plain, moov).unwrap();
        assert_eq!(audio_trim(&plain, 44_100), None);
        let junk = dir.path().join("junk.mp4");
        std::fs::write(&junk, [0xFF_u8; 64]).unwrap();
        assert_eq!(audio_trim(&junk, 44_100), None);
    }
}
