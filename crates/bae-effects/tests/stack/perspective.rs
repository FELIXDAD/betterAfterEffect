use super::*;

#[test]
fn point_defaults_follow_source_size_on_apply_and_reset() {
    assert_reference_points(&[
        ("CC Sphere", 7, [15.5, 6.5]),
        (
            "CC Spotlight",
            1,
            [576.0 * 31.0 * (1.0 / 640.0), 36.0 * 13.0 * (1.0 / 360.0)],
        ),
        ("CC Spotlight", 2, [15.5, 6.5]),
        ("ADBE Radial Shadow", 3, [7.75, 3.25]),
    ]);
}
