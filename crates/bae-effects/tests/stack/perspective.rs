use super::*;

#[test]
fn point_defaults_follow_source_size_on_apply_and_reset() {
    assert_reference_points(&[("ADBE Radial Shadow", 3, [7.75, 3.25])]);
}
