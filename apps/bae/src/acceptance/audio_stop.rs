//! Preview audio stops with the picture: every way playback stops silences the
//! device from its next buffer on (0.2.0: Spacebar stopped the video while the
//! sound played on).
use super::*;
use bae_model::{PrimPath, Time, Value};
use serde_json::json;
use std::time::{Duration, Instant};

/// A mono 16-bit WAV of `secs` seconds of a 440 Hz tone.
fn tone_wav(secs: f64) -> Vec<u8> {
    let rate = 48_000u32;
    let n = (f64::from(rate) * secs) as u32;
    let data = n * 2;
    let mut out = Vec::with_capacity(44 + data as usize);
    for chunk in [
        &b"RIFF"[..],
        &(36 + data).to_le_bytes(),
        b"WAVEfmt ",
        &16u32.to_le_bytes(),
        &1u16.to_le_bytes(),
        &1u16.to_le_bytes(),
        &rate.to_le_bytes(),
        &(rate * 2).to_le_bytes(),
        &2u16.to_le_bytes(),
        &16u16.to_le_bytes(),
        b"data",
        &data.to_le_bytes(),
    ] {
        out.extend_from_slice(chunk);
    }
    for i in 0..n {
        let t = f64::from(i) / f64::from(rate);
        let v = ((t * 440.0 * std::f64::consts::TAU).sin() * 12_000.0) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Runs frames until `f` holds or `secs` pass.
fn within(h: &mut Ui, secs: f64, f: impl Fn(&Ui) -> bool) -> bool {
    let until = Instant::now() + Duration::from_secs_f64(secs);
    loop {
        if f(h) {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        frame(h);
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn audible(h: &Ui) -> bool {
    h.state().session().lock().audio_audible()
}

/// A 2 s comp with a 4 s tone, opened in the timeline.
fn comp_with_tone(h: &mut Ui, name: &str) -> PrimPath {
    h.state_mut().dispatch(Invocation::with_params(
        "comp.create",
        json!({"name": name, "width": 320, "height": 180, "fps": 30,
               "duration": Time::from_seconds(2.0).0}),
    ));
    let comp = bae_motion::query::comps(h.state().stage())
        .into_iter()
        .find(|c| bae_motion::query::display_name_of(h.state().stage(), c) == name)
        .unwrap();
    let asset = h
        .state()
        .stage()
        .children(&PrimPath::new(bae_motion::query::ASSETS_FOLDER).unwrap())
        .into_iter()
        .find(|a| bae_motion::query::display_name_of(h.state().stage(), a) == "tone.wav")
        .unwrap();
    h.state_mut().dispatch(Invocation::with_params(
        "layer.create",
        json!({"kind": "audio", "comp": comp, "source": asset}),
    ));
    h.state().session().lock().open_comp(&comp);
    to_front(h, "timeline");
    comp
}

/// Spacebar, a click in the time ruler, the end of a range that does not loop,
/// another composition and closing the project each silence the sound at once.
/// Skipped without an audio output device.
#[test]
fn every_way_playback_stops_silences_the_sound_at_once() {
    // With the GPU viewer when there is one: it renders the frames while the
    // device plays (the user's setup).
    let mut app = Gui::new();
    app.with_gpu();
    let mut h = gui_with(app);
    if !within(&mut h, 3.0, |h| {
        h.state().session().lock().audio_output_info().is_some()
    }) {
        eprintln!("no audio output device: skipped");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("tone.wav");
    std::fs::write(&wav, tone_wav(4.0)).unwrap();
    h.state_mut().dispatch(Invocation::with_params(
        "item.import",
        json!({"files":[{"path": wav, "name": "tone.wav", "attrs": {
            "has_audio": Value::Bool(true), "duration": Value::Time(Time::from_seconds(4.0))
        }}]}),
    ));
    let first = comp_with_tone(&mut h, "A");
    let other = comp_with_tone(&mut h, "B");
    let play = |h: &mut Ui, comp: &PrimPath| {
        h.state().session().lock().open_comp(comp);
        to_front(h, "timeline");
        key(h, Modifiers::NONE, Key::Space);
        assert!(h.state().is_playing(), "Spacebar did not play");
        assert!(within(h, 3.0, audible), "the tone never sounded");
    };
    let silent = |h: &mut Ui, how: &str| {
        assert!(
            within(h, 0.25, |h| !audible(h)),
            "still sounding after {how}"
        );
        // And it stays silent.
        std::thread::sleep(Duration::from_millis(150));
        assert!(!audible(h), "sounding again after {how}");
    };

    play(&mut h, &first);
    key(&mut h, Modifiers::NONE, Key::Space);
    assert!(!h.state().is_playing());
    silent(&mut h, "Spacebar");

    play(&mut h, &first);
    let head = timeline_header(&h).expect("timeline header");
    // AE 26's ruler labels (#844): 01:00f under a second's step, 01s above.
    let tick = find_texts(&h, |t| t == "01:00f" || t == "01s" || t == "0:00:01:00")
        .into_iter()
        .find(|r| r.left() > head.right())
        .unwrap_or_else(|| panic!("no ruler label in {:?}", painted_texts(&h)));
    click_at(&mut h, tick.center());
    silent(&mut h, "a click in the time ruler");

    // The end of the comp, not looping.
    {
        let session = h.state().session();
        let mut s = session.lock();
        for profile in &mut s.preview.profiles {
            profile.repeat = false;
        }
    }
    play(&mut h, &first);
    assert!(
        within(&mut h, 4.0, |h| !h.state().is_playing()),
        "playback did not end"
    );
    silent(&mut h, "the end of the range");

    play(&mut h, &first);
    h.state().session().lock().open_comp(&other);
    frame(&mut h);
    silent(&mut h, "switching to another composition");

    play(&mut h, &other);
    h.state_mut()
        .dialogs
        .answers
        .push_back(crate::files::UnsavedChoice::DontSave);
    h.state_mut()
        .dispatch(Invocation::new(bae_ui::action::ids::CLOSE_PROJECT));
    settle(&mut h);
    silent(&mut h, "closing the project");
}
