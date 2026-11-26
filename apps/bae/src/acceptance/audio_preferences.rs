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
