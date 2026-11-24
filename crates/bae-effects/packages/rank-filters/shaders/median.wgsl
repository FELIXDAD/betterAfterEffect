@compute @workgroup_size(32,1)
fn median(@builtin(workgroup_id) group: vec3<u32>, @builtin(local_invocation_index) lane: u32) {
    rank_row(group,lane,0.0,false);
}
