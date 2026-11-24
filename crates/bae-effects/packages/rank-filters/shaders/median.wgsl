@compute @workgroup_size(8,8)
fn median(@builtin(global_invocation_id) id: vec3<u32>) {
    rank_pixel(frame.target_rect.min + vec2<i32>(id.xy), 0.0, false);
}
