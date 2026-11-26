use super::*;
use bae_motion::{camera_track, effect, query};

#[test]
fn animation_track_camera_applies_the_perspective_effect() {
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let plate = h
        .state()
        .selected_layers()
        .first()
        .expect("selected plate")
        .clone();
    let item = h
        .state()
        .menu_items()
        .into_iter()
        .find(|i| i.invocation.id == "motion.anim.track_camera")
        .expect("Animation > Track Camera");
    assert!(h.state_mut().menu(&item.id));
    settle(&mut h);
    let stage = h.state().stage();
    let effects = effect::effects_of(stage, &plate);
    assert_eq!(effects.len(), 1);
    assert_eq!(
        stage
            .prim(&effects[0])
            .and_then(|v| query::static_value(&v, "match_name"))
            .and_then(|v| v.as_str().map(str::to_owned))
            .as_deref(),
        Some(camera_track::MATCH_NAME)
    );
}

#[test]
fn marquee_and_context_actions_create_camera_keys_in_one_undo_step() {
    use bae_model::{Time, Value};
    use bae_motion::camera_track::{Frame, Landmark, Pose, Sample, Settings, Solution};
    use egui_kittest::kittest::Queryable as _;
    let mut h = gui();
    comp_with(&mut h, &["motion.layer.new.solid"]);
    let plate = h.state().selected_layers()[0].clone();
    let item = h
        .state()
        .menu_items()
        .into_iter()
        .find(|i| i.invocation.id == "motion.anim.track_camera")
        .unwrap();
    assert!(h.state_mut().menu(&item.id));
    settle(&mut h);
    let effect = effect::effects_of(h.state().stage(), &plate)[0].clone();
    let comp = h.state().active_comp().unwrap();
    let info = query::comp_info(h.state().stage(), &comp).unwrap();
    let coords = [[-20., -20., 200.], [20., -20., 200.], [0., 20., 200.]];
    let points = coords
        .iter()
        .enumerate()
        .map(|(id, &world)| (id as u32, Landmark { world, error: 0. }))
        .collect();
    let frames = (0..3)
        .map(|i| {
            let camera = Pose {
                center: [i as f64, 0., 0.],
                rotation: [0., 0., 0., 1.],
                zoom: 200.,
            };
            let samples = coords
                .iter()
                .enumerate()
                .map(|(id, p)| Sample {
                    id: id as u32,
                    pixel: [(p[0] - i as f64) as f32, p[1] as f32],
                })
                .collect();
            Frame {
                layer_from_comp: bae_anim::glam::DAffine2::IDENTITY.to_cols_array(),
                time: Time(i * info.frame_duration().0),
                camera,
                samples,
            }
        })
        .collect();
    let solution = Solution {
        points,
        frames,
        size: [info.width, info.height],
        error: 0.,
        settings: Settings::read(h.state().stage(), &effect),
    };
    let recording =
        camera_track::Recording::prepare(h.state().stage(), &effect, &solution).unwrap();
    h.state_mut().invoke(Invocation::with_params(
        "camera_tracker.record",
        serde_json::to_value(recording).unwrap(),
    ));
    settle(&mut h);
    let canvas = h
        .root()
        .query_all_by_label("Composition")
        .map(|n| n.rect())
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("composition viewer");
    let center = canvas.center();
    drag(
        &mut h,
        center - egui::vec2(72., 60.),
        center + egui::vec2(72., 60.),
        Modifiers::NONE,
    );
    let choose = |h: &mut Ui, label: &str| {
        send(h, Event::PointerMoved(center));
        for pressed in [true, false] {
            send(
                h,
                Event::PointerButton {
                    pos: center,
                    button: PointerButton::Secondary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            );
        }
        settle(h);
        let item =
            find_text(h, label).unwrap_or_else(|| panic!("missing tracker menu item {label}"));
        click_at(h, item.center());
        settle(h);
    };
    choose(&mut h, "Set Ground Plane and Origin");
    assert!(query::attr_spec(h.state().stage(), &effect, "tracker:ground").is_some());
    for (label, kind, extra) in [
        ("Create Text and Camera", "text", 2),
        ("Create Solid and Camera", "solid", 2),
        ("Create Null and Camera", "null", 2),
        ("Create Shadow Catcher, Camera and Light", "solid", 3),
    ] {
        choose(&mut h, label);
        let layers = h.state().layers();
        assert_eq!(layers.len(), 1 + extra, "{label}");
        let stage = h.state().stage();
        let camera = layers
            .iter()
            .find(|p| query::layer_kind(stage, p).as_deref() == Some("camera"))
            .unwrap();
        for attr in ["transform:orientation", "transform:position"] {
            assert_eq!(
                query::attr_spec(stage, camera, attr)
                    .unwrap()
                    .keyframes
                    .len(),
                3
            );
        }
        let object = layers
            .iter()
            .find(|p| *p != &plate && query::layer_kind(stage, p).as_deref() == Some(kind))
            .unwrap();
        assert_eq!(
            stage
                .prim(object)
                .and_then(|v| query::static_value(&v, "three_d")),
            Some(Value::Bool(true))
        );
        if extra == 3 {
            assert!(
                layers
                    .iter()
                    .any(|p| query::layer_kind(stage, p).as_deref() == Some("light"))
            );
            assert_eq!(
                stage
                    .prim(object)
                    .unwrap()
                    .value("material:accepts_shadows"),
                Some(&Value::Int(2))
            );
        }
        key(&mut h, Modifiers::COMMAND, Key::Z);
        assert_eq!(h.state().layers(), vec![plate.clone()]);
    }
}
