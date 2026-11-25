use super::*;

#[test]
fn point_defaults_follow_source_size_on_apply_and_reset() {
    assert_reference_points(&[
        ("CC Kaleida", 1, [15.5, 6.5]),
        ("CC Glass", 13, [15.5, 6.5]),
        ("CS Vignette", 3, [15.5, 6.5]),
        ("ADBE Tile", 1, [15.5, 6.5]),
    ]);
}
