use super::*;
use bae_model::{FrameRate, TimeRange};
use std::time::{Duration, Instant};

#[test]
fn idle_cache_waits_and_fills_the_selected_range_in_the_viewer() {
    let mut app = Gui::new();
    if !app.with_gpu() {
        return;
    }
    app.app
        .insert_resource(Arc::new(bae_gpu::viewport::ViewportTextureRegistry::new()));
    let mut h = gui_with(app);
    let rate = FrameRate::from_fps(24.0);
    let at = |n| rate.frame_to_time(n);
    let settings = |h: &Ui, delay: i64, range: &str| {
        let mut prefs = (*h.state().prefs.current()).clone();
        prefs.set("previews.idle_delay_before_caching", delay);
        prefs.set("previews.cache_range", range);
        h.state().prefs.set(prefs).unwrap();
    };
    settings(&h, 999, "work_area");
    h.state_mut().dispatch(Invocation::with_params(
        "comp.create",
        serde_json::json!({
            "name": "Idle cache", "width": 64, "height": 64, "fps": 24, "duration": at(10)
        }),
    ));
    let comp = bae_motion::query::comps(h.state().stage()).pop().unwrap();
    h.state().session().lock().open_comp(&comp);
    h.state_mut().dispatch(Invocation::with_params(
        "comp.set_work_area",
        serde_json::json!({
            "comp": comp, "start": at(3), "end": at(6)
        }),
    ));
    let await_spans = |h: &mut Ui, ranges: &[(i64, i64)]| {
        let expected: Vec<_> = ranges
            .iter()
            .map(|&(a, b)| TimeRange::new(at(a), at(b)))
            .collect();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            frame(h);
            let session = h.state().session();
            let locked = session.lock();
            let actual = locked.cached_frames(&comp, h.state().stage().revision());
            if actual == expected {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "cached {actual:?}, wanted {expected:?}"
            );
            drop(locked);
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    await_spans(&mut h, &[(0, 1)]);
    std::thread::sleep(Duration::from_millis(350));
    frame(&mut h);
    assert_eq!(
        h.state()
            .session()
            .lock()
            .cached_frames(&comp, h.state().stage().revision()),
        &[TimeRange::new(at(0), at(1))]
    );
    let menu = h
        .state()
        .menu_items()
        .into_iter()
        .find(|item| item.invocation.id == "motion.comp.cache_frames_when_idle")
        .unwrap()
        .id;
    assert!(h.state_mut().menu(&menu));
    settle(&mut h);
    let enabled = |h: &Ui| {
        h.state()
            .prefs
            .current()
            .bool(bae_prefs::keys::CACHE_FRAMES_WHEN_IDLE)
    };
    assert!(!enabled(&h));
    settings(&h, 0, "work_area");
    settle(&mut h);
    std::thread::sleep(Duration::from_millis(80));
    await_spans(&mut h, &[(0, 1)]);
    to_front(&mut h, "footage");
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::I);
    assert!(enabled(&h));
    settle(&mut h);
    std::thread::sleep(Duration::from_millis(80));
    await_spans(&mut h, &[(0, 1)]);
    to_front(&mut h, "composition");
    await_spans(&mut h, &[(0, 1), (3, 6)]);
    settings(&h, 0, "work_area_extended_by_current_time");
    await_spans(&mut h, &[(0, 6)]);
    settings(&h, 0, "entire_duration");
    await_spans(&mut h, &[(0, 10)]);
    eprintln!("GPU timeline cache: playhead alone, work area 3..6, extended 0..6, entire 0..10.");
}
