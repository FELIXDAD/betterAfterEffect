//! macOS movie trimming for packaging: keyframe index, GOP-aligned
//! passthrough, exact re-encode. Output frame `i` must show source frame
//! `first_frame + i` (bar code).

#![cfg(target_vendor = "apple")]

mod common;

use bae_media::trim::Trimmer;
use bae_media::{
    FrameRate, FrameRequest, MediaFormats, MediaSource, OpenOptions, Time, TimeRange, TrimMode,
    TrimRequest,
};
use common::movie::{Codec, frame_of, write_movie};

const RATE: FrameRate = FrameRate::FPS_25;

fn open(path: &std::path::Path) -> Box<dyn MediaSource> {
    MediaFormats::builtin()
        .open(path, &OpenOptions::default())
        .unwrap()
}

fn request(
    source: &std::path::Path,
    output: &std::path::Path,
    frames: std::ops::Range<i64>,
    mode: TrimMode,
) -> TrimRequest {
    TrimRequest {
        source: source.to_owned(),
        output: output.to_owned(),
        range: TimeRange::new(
            RATE.frame_to_time(frames.start),
            RATE.frame_to_time(frames.end),
        ),
        handles: Time::from_flicks(0),
        mode,
        reencode_codec: None,
    }
}

fn check_offset(output: &std::path::Path, first: i64, count: u64) {
    let mut clip = open(output);
    let video = clip.info().video.clone().unwrap();
    assert_eq!(video.frame_count, Some(count), "{}", output.display());
    for i in [0, 1, count as i64 / 2, count as i64 - 1] {
        let frame = clip.decode_cpu(&FrameRequest::new(i)).unwrap();
        assert_eq!(
            i64::from(frame_of(64, |x| frame.pixel_f32(x, 8))),
            first + i,
            "output frame {i}"
        );
    }
}

#[test]
fn keyframes_come_from_the_sample_table() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gop.mov");
    write_movie(&path, Codec::H264 { gop: 10 }, 64, 16, RATE, 40);
    let mut source = open(&path);
    // VideoToolbox treats the interval as a maximum (it picks 9 here).
    let sync = source.sync_frames().unwrap();
    assert_eq!(sync[0], 0);
    assert!(sync.len() >= 4, "{sync:?}");
    assert!(
        sync.windows(2).all(|w| w[1] - w[0] <= 10 && w[1] > w[0]),
        "{sync:?}"
    );
    // That these really are sync samples is proven by the passthrough test: a
    // copy starting at one decodes correctly from its first frame.

    let intra = dir.path().join("intra.mov");
    write_movie(&intra, Codec::ProRes422, 64, 16, RATE, 6);
    assert_eq!(open(&intra).sync_frames(), Some((0..6).collect()));
}

#[test]
fn passthrough_copies_whole_gops() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("src.mov");
    write_movie(&source, Codec::H264 { gop: 10 }, 64, 16, RATE, 40);
    let sync = open(&source).sync_frames().unwrap();
    let first = bae_media::trim::gop_start(&sync, 13).unwrap();
    let end = bae_media::trim::next_gop_start(&sync, 26).unwrap();
    assert!(first <= 13 && end > 26);
    let output = dir.path().join("clip.mov");
    let result = bae_media::apple::AvTrimmer
        .trim(&request(&source, &output, 13..27, TrimMode::Passthrough))
        .unwrap();
    assert!(result.passthrough);
    assert_eq!(
        (result.first_frame, result.frame_count),
        (first, (end - first) as u64)
    );
    assert_eq!(
        result.source_range,
        TimeRange::new(RATE.frame_to_time(first), RATE.frame_to_time(end))
    );
    check_offset(&output, first, (end - first) as u64);
    // The copy starts on a keyframe, and its keyframes are the source's.
    let clip_sync = open(&output).sync_frames().unwrap();
    let expected: Vec<i64> = sync
        .iter()
        .filter(|&&k| k >= first && k < end)
        .map(|k| k - first)
        .collect();
    assert_eq!(clip_sync, expected);
}

#[test]
fn reencode_is_frame_exact() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("src.mov");
    write_movie(&source, Codec::H264 { gop: 10 }, 64, 16, RATE, 40);
    let output = dir.path().join("exact.mov");
    let result = bae_media::apple::AvTrimmer
        .trim(&request(&source, &output, 13..27, TrimMode::Reencode))
        .unwrap();
    assert!(!result.passthrough);
    assert_eq!((result.first_frame, result.frame_count), (13, 14));
    assert_eq!(
        open(&output).info().video.as_ref().unwrap().codec,
        "prores422hq"
    );
    check_offset(&output, 13, 14);
}

#[test]
fn intra_codecs_trim_exactly_without_reencoding() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("intra.mov");
    write_movie(&source, Codec::ProRes422, 64, 16, RATE, 20);
    let output = dir.path().join("intra_clip.mov");
    let result = bae_media::apple::AvTrimmer
        .trim(&request(
            &source,
            &output,
            5..9,
            TrimMode::PreferPassthrough,
        ))
        .unwrap();
    assert!(result.passthrough);
    assert_eq!((result.first_frame, result.frame_count), (5, 4));
    check_offset(&output, 5, 4);
}
