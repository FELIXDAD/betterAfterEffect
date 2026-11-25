fn bae_hash(seed: u32, p: vec2<i32>) -> f32 {
    var v = seed ^ (bitcast<u32>(p.x) * 747796405u) ^ (bitcast<u32>(p.y) * 2891336453u);
    v = v * 747796405u + 2891336453u;
    let w = ((v >> ((v >> 28u) + 4u)) ^ v) * 277803737u;
    return f32((w >> 22u) ^ w) / 4294967295.0;
}
