use super::*;

#[test]
fn point_defaults_follow_source_size_on_apply_and_reset() {
    assert_reference_points(&[
        ("ADBE Mirror", 1, [31., 6.5]),
        ("ADBE Corner Pin", 4, [31., 13.]),
        ("ADBE Geometry2", 2, [15.5, 6.5]),
        ("ADBE Geometry", 1, [15.5, 6.5]),
        ("ADBE Geometry", 2, [15.5, 6.5]),
    ]);
}
