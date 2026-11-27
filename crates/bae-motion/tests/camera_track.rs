use bae_anim::glam::{DQuat, DVec3};
use bae_model::Time;
use bae_motion::camera_track::{Pose, Sample, Settings, solve};
#[test]
fn reconstructs_perspective_and_zoom_from_image_measurements() {
    for (shot, method) in [(4, 3), (1, 1), (2, 5)] {
        let zoom = 480.;
        let settings = Settings {
            shot,
            method,
            angle: 2. * (320_f64 / zoom).atan().to_degrees(),
            detailed: false,
        };
        let frames = (0..10)
            .map(|i| {
                let t = i as f64;
                let pose = Pose {
                    center: if method == 5 {
                        [0.; 3]
                    } else {
                        [0.06 * t, 0.003 * t * t, 0.02 * t]
                    },
                    rotation: DQuat::from_rotation_y(t * 0.012).to_array(),
                    zoom: if shot == 2 { zoom + 8. * t } else { zoom },
                };
                let samples = (0..80)
                    .map(|id| {
                        let p = DVec3::new(
                            f64::from(id % 10) * 0.3 - 1.5,
                            f64::from(id / 10) * 0.23 - 0.8,
                            3.5 + f64::from(id * 17 % 23) * 0.17,
                        );
                        Sample {
                            id,
                            pixel: pose.pixel(p).unwrap().as_vec2().to_array(),
                        }
                    })
                    .collect();
                (Time(i), samples)
            })
            .collect();
        let solution = solve([640, 360], frames, settings, || false).unwrap();
        assert_eq!(solution.frames.len(), 10);
        assert!(solution.points.len() >= 70);
        assert!(solution.error < 1.5, "{shot}: {}", solution.error);
        let last = solution.frames.last().unwrap().camera;
        if shot == 4 {
            assert!((last.zoom - zoom).abs() < 1e-7);
            assert!(last.position().length() > 0.1);
        }
        if shot == 2 {
            assert_eq!(last.center, [0.; 3]);
            assert!(last.zoom > solution.frames[0].camera.zoom * 1.05);
        }
    }
}
#[test]
fn empty_measurements_and_cancellation_do_not_produce_cameras() {
    let settings = Settings {
        shot: 1,
        angle: 0.,
        method: 1,
        detailed: false,
    };
    let empty = || vec![(Time(0), vec![]), (Time(1), vec![])];
    assert!(solve([640, 360], empty(), settings, || false).is_err());
    assert_eq!(
        solve([640, 360], empty(), settings, || true).unwrap_err(),
        "Tracking stopped"
    );
}
