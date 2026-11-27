//! On-disk analysis track shared by the tracker command and Warp Stabilizer.
//! Each sample is the cumulative layer-pixel motion of nine cells in row-major order.
pub const MATCH_NAME: &str = "ADBE SubspaceStabilizer";
pub const MOTION: u32 = 9001;
pub const QUALITY: u32 = 9002;
pub const CELLS: usize = 9;

/// Triples of cell index, all-frames flag and layer time, stored in the
/// effect's custom deleted-track-points parameter.
pub const DELETED: u32 = 305;
pub fn excluded(value: Option<&crate::Value>, cell: usize, time: crate::Time) -> bool {
    let Some(crate::Value::FloatList(entries)) = value else {
        return false;
    };
    entries
        .as_chunks::<3>()
        .0
        .iter()
        .any(|[index, all, t]| *index == cell as f64 && (*all != 0. || *t == time.0 as f64))
}
