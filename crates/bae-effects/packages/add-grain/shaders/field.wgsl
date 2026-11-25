// Sum independent samples for a symmetric, approximately Gaussian distribution.
fn grain_sample(cell: vec2<i32>, seed: u32) -> f32 {
    return bae_hash(seed, cell) + bae_hash(seed + 1u, cell) + bae_hash(seed + 2u, cell) - 1.5;
}

fn grain_lattice(uv:vec2f, seed:u32, softness:f32)->f32 {
    let cell = vec2<i32>(floor(uv));
    let t = smoothstep(vec2<f32>(0), vec2<f32>(1), fract(uv));
    let a = grain_sample(cell, seed);
    let b = grain_sample(cell + vec2<i32>(1,0), seed);
    let c = grain_sample(cell + vec2<i32>(0,1), seed);
    let d = grain_sample(cell + vec2<i32>(1,1), seed);
    return mix(a, mix(mix(a,b,t.x), mix(c,d,t.x), t.y), saturate(softness));
}
