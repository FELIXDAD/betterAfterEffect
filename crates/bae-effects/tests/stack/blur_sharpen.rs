use super::*;

#[test]
fn point_defaults_follow_source_size_on_apply_and_reset() {
    assert_reference_points(&[("ADBE Radial Blur", 2, [15.5, 6.5])]);
}
