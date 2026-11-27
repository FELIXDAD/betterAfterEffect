//! MediaLibrary: shared footage access with time mapping, caching, content
//! hashes, invalidation and concurrent use.

mod common;

use std::sync::Arc;

use bae_media::{
    FrameCache, FrameRate, FrameRequest, MediaFormats, MediaGpu, MediaLibrary, OpenOptions, Time,
};
use common::{assert_close, write_solid_png};

const RATE: FrameRate = FrameRate::FPS_25;

fn library() -> MediaLibrary {
    MediaLibrary::new(
        MediaFormats::builtin(),
        Arc::new(FrameCache::new(256 << 20)),
    )
}

/// `shot_0001.png …` with red = frame number × 20.
fn sequence(dir: &std::path::Path, prefix: &str, frames: u8) -> std::path::PathBuf {
    for n in 1..=frames {
        write_solid_png(
            &dir.join(format!("{prefix}_{n:04}.png")),
            8,
            8,
            [n * 20, 0, 0, 255],
        );
    }
    dir.join(format!("{prefix}_0001.png"))
}

#[test]
fn alphabetical_sequences_decode_in_filename_order_without_frame_numbers() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("apple.png");
    let second = dir.path().join("zebra.png");
    write_solid_png(&second, 2, 2, [200, 0, 0, 255]);
    write_solid_png(&first, 2, 2, [0, 100, 0, 255]);
    let open = OpenOptions {
        sequence: bae_media::SequenceMode::Alphabetical,
        ..OpenOptions::sequence(RATE)
    };
    let library = library();
    let info = library.info(&second, &open).unwrap();
    assert_eq!(info.path, first);
    assert_eq!(info.sequence.unwrap().pattern, "*.png");
    let pixels = |i| {
        library
            .frame_cpu(&second, &open, &FrameRequest::new(i))
            .unwrap()
            .pixel(0, 0)
            .to_vec()
    };
    assert_eq!(pixels(0), [0, 100, 0, 255]);
    assert_eq!(pixels(1), [200, 0, 0, 255]);
}

#[test]
fn time_mapping_hashes_and_end_hold() {
    let dir = tempfile::tempdir().unwrap();
    let first = sequence(dir.path(), "shot", 5);
    let library = library();
    let open = OpenOptions::sequence(RATE);
    let request = FrameRequest::new(0);
    assert_eq!(
        library
            .info(&first, &open)
            .unwrap()
            .video
            .unwrap()
            .frame_count,
        Some(5)
    );

    let at = |frame: i64| RATE.frame_to_time(frame);
    assert_eq!(library.frame_index(&first, &open, at(3)).unwrap(), 3);
    // Just before the next frame still shows this one.
    assert_eq!(
        library
            .frame_index(&first, &open, at(4) - Time::from_flicks(1))
            .unwrap(),
        3
    );
    // Held at both ends.
    assert_eq!(library.frame_index(&first, &open, at(40)).unwrap(), 4);
    assert_eq!(library.frame_index(&first, &open, at(-3)).unwrap(), 0);

    let h = |t| library.content_hash(&first, &open, &request, t).unwrap();
    assert_eq!(h(at(2)), h(at(2) + Time::from_flicks(1000)));
    assert_ne!(h(at(2)), h(at(3)));
    assert_eq!(h(at(9)), h(at(4)), "held frames share the hash");
    let straight = library
        .content_hash(&first, &open, &request.straight(), at(2))
        .unwrap();
    assert_ne!(straight, h(at(2)), "delivery is part of the hash");

    // The same file opened as a still is a different source.
    let still = library
        .content_hash(&first, &OpenOptions::default(), &request, at(2))
        .unwrap();
    assert_ne!(still, h(at(2)));
    assert_eq!(library.open_count(), 2);

    let cpu = library
        .frame_cpu(&first, &open, &FrameRequest::new(2))
        .unwrap();
    assert_eq!(cpu.pixel(0, 0), [60, 0, 0, 255]);
}

#[test]
fn open_limit_closes_least_recently_used() {
    let dir = tempfile::tempdir().unwrap();
    let files: Vec<_> = (0..3)
        .map(|i| {
            let path = dir.path().join(format!("still{i}.png"));
            write_solid_png(&path, 2, 2, [i, 0, 0, 255]);
            path
        })
        .collect();
    let library = library().with_open_limit(2);
    for file in &files {
        library.info(file, &OpenOptions::default()).unwrap();
    }
    assert_eq!(library.open_count(), 2);
}

#[test]
fn gpu_frames_cache_and_invalidate() {
    let Some(ctx) = common::gpu() else { return };
    let media = MediaGpu::new(ctx);
    let dir = tempfile::tempdir().unwrap();
    let first = sequence(dir.path(), "g", 4);
    let library = library();
    let open = OpenOptions::sequence(RATE);
    let request = FrameRequest::new(0);

    let frame = library
        .frame_at(&first, &open, &request, RATE.frame_to_time(2), &media)
        .unwrap();
    let again = library
        .frame_at(&first, &open, &request, RATE.frame_to_time(2), &media)
        .unwrap();
    assert_eq!(
        frame.texture, again.texture,
        "second request comes from the cache"
    );
    let pixels = media.read_back(&frame).unwrap();
    assert_close(pixels.pixel_f32(1, 1), [60.0 / 255.0, 0.0, 0.0, 1.0], 1e-3);
    let stats = library.cache().stats();
    assert_eq!((stats.hits, stats.misses), (1, 1));

    // Re-render frame 3 on disk (the first file is untouched); invalidating
    // that frame's file reopens the sequence with new pixels and hashes.
    let file3 = dir.path().join("g_0003.png");
    let before = library
        .content_hash(&first, &open, &request, RATE.frame_to_time(2))
        .unwrap();
    write_solid_png(&file3, 16, 8, [0, 200, 0, 255]);
    library.invalidate(&dir.path().join("unrelated_0003.png"));
    assert_eq!(library.open_count(), 1, "other files keep their sources");
    library.invalidate(&file3);
    assert_eq!(library.open_count(), 0);
    assert_eq!(library.cache().stats().frames, 0);
    let frame = library
        .frame_at(&first, &open, &request, RATE.frame_to_time(2), &media)
        .unwrap();
    assert_eq!(frame.width(), 16);
    let after = library
        .content_hash(&first, &open, &request, RATE.frame_to_time(2))
        .unwrap();
    assert_ne!(before, after, "a new file version changes the hash");
}

#[test]
fn concurrent_requests_from_threads() {
    let Some(ctx) = common::gpu() else { return };
    let media = Arc::new(MediaGpu::new(ctx));
    let dir = tempfile::tempdir().unwrap();
    let firsts: Vec<_> = ["a", "b", "c"]
        .iter()
        .map(|p| sequence(dir.path(), p, 6))
        .collect();
    let library = Arc::new(library());
    let open = OpenOptions::sequence(RATE);
    std::thread::scope(|scope| {
        for t in 0..6 {
            let (library, media, firsts) = (library.clone(), media.clone(), &firsts);
            scope.spawn(move || {
                for i in 0..12_i64 {
                    let first = &firsts[(t + i as usize) % 3];
                    let index = (i * 5 + t as i64) % 6;
                    let frame = library
                        .frame_at(
                            first,
                            &open,
                            &FrameRequest::new(0),
                            RATE.frame_to_time(index),
                            &media,
                        )
                        .unwrap();
                    let pixels = media.read_back(&frame).unwrap();
                    let red = (index + 1) as f32 * 20.0 / 255.0;
                    assert_close(pixels.pixel_f32(0, 0), [red, 0.0, 0.0, 1.0], 1e-3);
                }
            });
        }
    });
    assert_eq!(library.open_count(), 3);
}

#[cfg(target_vendor = "apple")]
#[test]
fn movie_frames_by_time() {
    use common::movie::{Codec, frame_of, write_movie};

    let Some(ctx) = common::gpu() else { return };
    let media = MediaGpu::new(ctx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lib.mov");
    write_movie(
        &path,
        Codec::H264 { gop: 8 },
        64,
        16,
        FrameRate::FPS_29_97,
        24,
    );
    let library = library();
    let open = OpenOptions::default();
    for index in [5_i64, 6, 20, 2, 23] {
        let time = FrameRate::FPS_29_97.frame_to_time(index);
        let frame = library
            .frame_at(&path, &open, &FrameRequest::new(0), time, &media)
            .unwrap();
        let pixels = media.read_back(&frame).unwrap();
        assert_eq!(i64::from(frame_of(64, |x| pixels.pixel_f32(x, 8))), index);
    }
}

/// Builds a container: junk, then each file stored as-is; returns locations.
fn pack(
    container: &std::path::Path,
    files: &[(&str, std::path::PathBuf)],
) -> Vec<bae_media::MediaLocation> {
    let mut bytes = b"PK-ish header junk".to_vec();
    let mut locations = Vec::new();
    for (name, path) in files {
        let data = std::fs::read(path).unwrap();
        locations.push(bae_media::MediaLocation::Embedded {
            container: container.to_owned(),
            offset: bytes.len() as u64,
            len: data.len() as u64,
            name: (*name).to_owned(),
        });
        bytes.extend_from_slice(&data);
        bytes.extend_from_slice(b"--trailer between entries--");
    }
    std::fs::write(container, bytes).unwrap();
    locations
}

#[test]
fn embedded_entries_resolve_to_extracted_files() {
    let src = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let still = src.path().join("logo.png");
    write_solid_png(&still, 4, 4, [9, 8, 7, 255]);
    let frames: Vec<_> = (1..=3_u8)
        .map(|n| {
            let p = src.path().join(format!("seq_{n:04}.png"));
            write_solid_png(&p, 4, 4, [n * 30, 0, 0, 255]);
            p
        })
        .collect();
    let container = src.path().join("project.baez");
    let mut files = vec![("assets/logo.png", still)];
    let names: Vec<String> = (1..=3)
        .map(|n| format!("assets/shots/seq_{n:04}.png"))
        .collect();
    for (name, path) in names.iter().zip(&frames) {
        files.push((name.as_str(), path.clone()));
    }
    let locations = pack(&container, &files);
    let library = library().with_extract_dir(cache.path());

    let logo = library.resolve(&locations[0]).unwrap();
    assert!(logo.starts_with(cache.path()) && logo.ends_with("assets/logo.png"));
    let frame = library
        .frame_cpu(&logo, &OpenOptions::default(), &FrameRequest::new(0))
        .unwrap();
    assert_eq!(frame.pixel(0, 0), [9, 8, 7, 255]);

    // A sequence: resolve every frame, open the first as a sequence.
    let paths: Vec<_> = locations[1..]
        .iter()
        .map(|l| library.resolve(l).unwrap())
        .collect();
    let open = OpenOptions::sequence(RATE);
    assert_eq!(
        library
            .info(&paths[0], &open)
            .unwrap()
            .video
            .unwrap()
            .frame_count,
        Some(3)
    );
    let frame = library
        .frame_cpu(&paths[0], &open, &FrameRequest::new(2))
        .unwrap();
    assert_eq!(frame.pixel(0, 0), [90, 0, 0, 255]);

    // Resolving again reuses the extraction; files stay as they are.
    let modified = std::fs::metadata(&logo).unwrap().modified().unwrap();
    assert_eq!(library.resolve(&locations[0]).unwrap(), logo);
    assert_eq!(
        std::fs::metadata(&logo).unwrap().modified().unwrap(),
        modified
    );

    // A different container version extracts into a new place.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let locations = pack(&container, &files);
    let again = library.resolve(&locations[0]).unwrap();
    assert_ne!(again, logo);

    // Disk files pass through; out-of-range entries are refused.
    let disk = bae_media::MediaLocation::File(src.path().join("logo.png"));
    assert_eq!(library.resolve(&disk).unwrap(), src.path().join("logo.png"));
    let bad = bae_media::MediaLocation::Embedded {
        container: container.clone(),
        offset: 10,
        len: 1 << 40,
        name: "assets/huge.mov".into(),
    };
    assert!(library.resolve(&bad).is_err());
}

#[cfg(target_vendor = "apple")]
#[test]
fn embedded_movie_decodes() {
    use common::movie::{Codec, frame_of, write_movie};

    let src = tempfile::tempdir().unwrap();
    let movie = src.path().join("clip.mov");
    write_movie(&movie, Codec::ProRes422, 64, 16, RATE, 6);
    let container = src.path().join("p.baez");
    let locations = pack(&container, &[("assets/footage/clip.mov", movie)]);
    let library = library().with_extract_dir(src.path().join("cache"));
    let path = library.resolve(&locations[0]).unwrap();
    let frame = library
        .frame_cpu(&path, &OpenOptions::default(), &FrameRequest::new(4))
        .unwrap();
    assert_eq!(frame_of(64, |x| frame.pixel_f32(x, 8)), 4);
}
