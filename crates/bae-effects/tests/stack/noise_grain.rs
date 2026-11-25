use super::*;

#[test]
fn point_defaults_follow_source_size_on_apply_and_reset() {
    assert_reference_points(&[
        ("VISINF Grain Duplication", 14, [15.5, 6.5]),
        ("VISINF Grain Removal", 30, [15.5, 6.5]),
    ]);
}
