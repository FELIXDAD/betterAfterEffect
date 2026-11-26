use super::*;
use egui_kittest::kittest::Queryable as _;

#[test]
fn audio_page_shows_the_open_device_and_applies_channel_mapping() {
    let mut h = gui();
    h.state_mut().dispatch(Invocation::with_params(
        bae_ui::settings::ACTION,
        serde_json::json!({"page":"audio"}),
    ));
    settle(&mut h);
    let session = h.state().session();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let output = loop {
        if let Some((name, channels, rate)) = session.lock().audio_output_info() {
            break Some((name.to_owned(), channels, rate));
        }
        if std::time::Instant::now() >= until {
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    settle(&mut h);
    let Some((name, channels, rate)) = output else {
        assert!(find_text(&h, "44100 Hz").is_none());
        assert!(find_text(&h, "—").is_some());
        return;
    };
    eprintln!("Audio output: {name} ({channels} channels, {rate} Hz)");
    assert!(find_text(&h, &format!("{rate} Hz")).is_some());
    let device = if name.is_empty() { "—" } else { &name };
    assert!(find_text(&h, &format!("System Default - {device}")).is_some());
    if channels < 2 {
        return;
    }
    h.get_by_label(&format!("{device} 1")).click();
    settle(&mut h);
    h.get_by_role_and_label(egui::accesskit::Role::Button, &format!("{device} 2"))
        .click();
    settle(&mut h);
    h.get_by_label("OK").click();
    settle(&mut h);
    assert_eq!(
        h.state().prefs.current().str("audio.audio_map_left"),
        "channel_2"
    );
}

#[test]
fn audio_preferences_select_a_native_device_and_reopen_it_at_the_requested_latency() {
    let mut h = gui();
    let session = h.state().session();
    let show_audio = |h: &mut Ui| {
        h.state_mut().dispatch(Invocation::with_params(
            bae_ui::settings::ACTION,
            serde_json::json!({ "page": "audio" }),
        ));
        settle(h);
    };
    show_audio(&mut h);
    let wait = |h: &mut Ui| {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while session.lock().audio_device_pending() {
            assert!(
                std::time::Instant::now() < until,
                "device negotiation timed out"
            );
            frame(h);
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        settle(h);
    };
    wait(&mut h);
    let (id, name) = {
        let s = session.lock();
        let devices = s.output_devices();
        if devices.outputs.is_empty() {
            return;
        }
        devices
            .outputs
            .iter()
            .find(|(_, n)| n == &devices.default_name)
            .unwrap_or(&devices.outputs[0])
            .clone()
    };
    let system = format!(
        "System Default - {}",
        session.lock().output_devices().default_name
    );
    h.get_by_label(&system).click();
    settle(&mut h);
    h.get_by_role_and_label(egui::accesskit::Role::Button, &name)
        .click();
    settle(&mut h);
    h.get_by_label("200").click();
    settle(&mut h);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "50")
        .click();
    settle(&mut h);
    h.get_by_label("OK").click();
    settle(&mut h);
    wait(&mut h);
    let prefs = h.state().prefs.current();
    assert_eq!(prefs.str("audio.audio_default_output"), id);
    assert_eq!(prefs.str("audio.audio_latency_ms"), "50");
    assert_eq!(session.lock().audio_output_info().unwrap().0, name);
    if let Some(actual) = session.lock().audio_buffer_ms() {
        eprintln!("Selected output: {name}, requested 50 ms, driver buffer {actual:.1} ms");
        assert!(actual.is_finite() && actual > 0.0);
    }
    show_audio(&mut h);
    wait(&mut h);
    assert!(find_text(&h, &name).is_some());
    assert!(find_text(&h, "50").is_some());
}
