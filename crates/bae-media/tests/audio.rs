//! Audio decoding with symphonia: exact PCM, random access, waveforms, and —
//! on macOS, with files made by the system's `afconvert` at test time — AAC
//! timing (encoder priming removed), FLAC and AIFF.

use std::path::Path;

use bae_media::audio;
use bae_media::{AudioSource, FileAudio, MemoryAudio, Time, Waveform};

const RATE: u32 = 48_000;

/// Interleaved stereo test signal: left 440 Hz sine, right a slow ramp.
fn signal(frames: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|i| {
            let t = i as f32 / RATE as f32;
            [
                (t * 440.0 * std::f32::consts::TAU).sin() * 0.5,
                (i as f32 / frames as f32) * 1.6 - 0.8,
            ]
        })
        .collect()
}

/// 16-bit PCM WAV writer (the format is simple enough to write by hand).
fn write_wav(path: &Path, channels: u16, samples: &[f32]) {
    let to_i16 = |v: f32| (v.clamp(-1.0, 1.0) * 32767.0).round() as i16;
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * u32::from(channels) * 2).to_le_bytes());
    out.extend_from_slice(&(channels * 2).to_le_bytes());
    out.extend_from_slice(&16_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        out.extend_from_slice(&to_i16(s).to_le_bytes());
    }
    std::fs::write(path, out).unwrap();
}

/// Exercise the device clock without making an audible sound.
#[test]
fn native_output_loops_and_stops_at_the_sample_boundary() {
    use bae_media::{mix::AudioMix, playback::AudioOutput};
    use std::time::{Duration, Instant};
    let mut output = match AudioOutput::open() {
        Ok(output) => output,
        Err(e) => {
            eprintln!("Skipping output device: {e}");
            return;
        }
    };
    let end = Time::from_seconds(0.1);
    let range = bae_model::TimeRange::new(Time::ZERO, end);
    output.start(AudioMix::default(), Time::ZERO, range, true);
    let deadline = Instant::now() + Duration::from_secs(4);
    while output.elapsed() <= end && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        output.elapsed() > end,
        "device clock did not cross the loop point"
    );
    output.start(AudioMix::default(), Time::ZERO, range, false);
    while !output.finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(output.finished());
    assert!((output.elapsed() - end).as_seconds().abs() < 0.001);
}

/// What the 16-bit file actually stores.
fn quantized(samples: &[f32]) -> Vec<f32> {
    samples
        .iter()
        .map(|&v| f32::from((v.clamp(-1.0, 1.0) * 32767.0).round() as i16) / 32768.0)
        .collect()
}

fn assert_samples_close(actual: &[f32], expected: &[f32], tolerance: f32) {
    assert_eq!(actual.len(), expected.len());
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!((a - e).abs() <= tolerance, "sample {i}: {a} vs {e}");
    }
}

#[test]
fn wav_decodes_exactly_with_random_access() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tone.wav");
    let frames = RATE as usize; // one second
    let source = signal(frames);
    write_wav(&path, 2, &source);
    let reference = quantized(&source);

    let streams = audio::probe(&path);
    assert_eq!(streams.len(), 1);
    assert_eq!((streams[0].sample_rate, streams[0].channels), (RATE, 2));
    assert_eq!(streams[0].frame_count, Some(frames as u64));

    let mut audio = FileAudio::open(&path).unwrap();
    // Sequential chunks.
    let mut all = Vec::new();
    let mut position = 0;
    loop {
        let chunk = audio.read(position, 1000).unwrap();
        if chunk.frames() == 0 {
            break;
        }
        position += chunk.frames() as u64;
        all.extend_from_slice(&chunk.samples);
    }
    assert_eq!(position, frames as u64);
    assert_samples_close(&all, &reference, 1e-6);

    // Random access: backwards, far ahead, near the end, past the end.
    for start in [30_000_u64, 100, 47_000, 12_345, 0] {
        let chunk = audio.read(start, 500).unwrap();
        let s = start as usize * 2;
        let expected = &reference[s..(s + chunk.samples.len()).min(reference.len())];
        assert_samples_close(&chunk.samples, expected, 1e-6);
    }
    assert_eq!(audio.read(47_900, 500).unwrap().frames(), 100);
    assert_eq!(audio.read(60_000, 10).unwrap().frames(), 0);
    assert_eq!(
        audio.read(48_000, 1).unwrap().duration(),
        Time::from_flicks(0)
    );
}

#[test]
fn waveform_from_file_matches_memory() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wave.wav");
    let source = signal(RATE as usize / 2);
    write_wav(&path, 2, &source);
    let mut file = FileAudio::open(&path).unwrap();
    let from_file = Waveform::from_source(&mut file, 480).unwrap();
    let mut memory = MemoryAudio::new(RATE, 2, quantized(&source));
    let from_memory = Waveform::from_source(&mut memory, 480).unwrap();
    assert_eq!(from_file.buckets(), 50);
    assert_eq!(from_file, from_memory);
    // The sine's peaks are ±0.5 in every 10 ms bucket.
    let [lo, hi] = from_file.peak(10, 0).unwrap();
    assert!(
        (hi - 0.5).abs() < 0.01 && (lo + 0.5).abs() < 0.01,
        "{lo} {hi}"
    );
}

#[cfg(target_vendor = "apple")]
fn afconvert(args: &[&str]) -> bool {
    std::process::Command::new("/usr/bin/afconvert")
        .args(args)
        .status()
        .is_ok_and(|s| s.success())
}

/// AAC in M4A: rate, length and timing. A click written at exactly 0.5 s must
/// decode at 0.5 s (the encoder's priming samples are removed).
#[cfg(target_vendor = "apple")]
#[test]
fn aac_keeps_sync() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("click.wav");
    let m4a = dir.path().join("click.m4a");
    let frames = RATE as usize;
    let mut samples = vec![0.0_f32; frames * 2];
    // A short burst centred on 0.5 s.
    let click = RATE as usize / 2;
    for i in click - 24..click + 24 {
        let v = if i % 2 == 0 { 0.9 } else { -0.9 };
        samples[2 * i] = v;
        samples[2 * i + 1] = v;
    }
    write_wav(&wav, 2, &samples);
    if !afconvert(&[
        "-f",
        "m4af",
        "-d",
        "aac",
        wav.to_str().unwrap(),
        m4a.to_str().unwrap(),
    ]) {
        eprintln!("skipping: afconvert could not write AAC");
        return;
    }
    let mut audio = FileAudio::open(&m4a).unwrap();
    let info = audio.info().clone();
    assert_eq!(
        (info.sample_rate, info.channels, info.codec.as_str()),
        (RATE, 2, "aac")
    );
    let count = info.frame_count.unwrap();
    assert!(count.abs_diff(frames as u64) <= 1, "{count} frames");
    let decoded = audio.read(0, frames as u64).unwrap();
    let peak = decoded
        .samples
        .as_chunks::<2>()
        .0
        .iter()
        .enumerate()
        .max_by(|a, b| a.1[0].abs().total_cmp(&b.1[0].abs()))
        .unwrap()
        .0;
    assert!(
        peak.abs_diff(click) <= 48,
        "click decoded at frame {peak}, expected {click} (priming not removed?)"
    );
    assert_eq!(
        audio::time_to_sample(audio::sample_to_time(peak as i64, RATE), RATE),
        peak as i64
    );
    // Seeking back to the click finds the same samples.
    let again = audio.read(click as u64 - 100, 200).unwrap();
    let s = (click - 100) * 2;
    assert_samples_close(&again.samples, &decoded.samples[s..s + 400], 1e-4);
}

#[cfg(target_vendor = "apple")]
#[test]
fn flac_and_aiff_are_lossless() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("src.wav");
    let source = signal(10_000);
    write_wav(&wav, 2, &source);
    let reference = quantized(&source);
    for (name, args) in [
        ("lossless.flac", ["-f", "flac", "-d", "flac"]),
        ("lossless.aif", ["-f", "AIFF", "-d", "BEI16"]),
    ] {
        let out = dir.path().join(name);
        let mut full = args.to_vec();
        full.extend([wav.to_str().unwrap(), out.to_str().unwrap()]);
        if !afconvert(&full) {
            eprintln!("skipping {name}: afconvert failed");
            continue;
        }
        let mut audio = FileAudio::open(&out).unwrap();
        assert_eq!(audio.info().frame_count, Some(10_000), "{name}");
        let decoded = audio.read(0, 10_000).unwrap();
        assert_samples_close(&decoded.samples, &reference, 1e-6);
        let tail = audio.read(9_000, 2_000).unwrap();
        assert_samples_close(&tail.samples, &reference[18_000..], 1e-6);
    }
}
