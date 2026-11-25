use super::*;

#[test]
fn point_defaults_follow_source_size_on_apply_and_reset() {
    assert_reference_points(&[("ADBE Tile", 1, [15.5, 6.5])]);
}
