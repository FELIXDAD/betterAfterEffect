// Cell coordinates are three-dimensional, so the same direction at the
// longitude seam always produces the same random value.
fn cell_noise(cell: vec3i, seed: u32) -> f32 {
    return bae_hash(seed ^ (bitcast<u32>(cell.z)*2246822519u),cell.xy);
}
fn smooth_noise(p: vec3f, seed: u32) -> f32 {
    let cell = vec3i(floor(p));
    let t = fract(p);
    let f = t*t*(3.0-2.0*t);
    let a = mix(cell_noise(cell,seed),cell_noise(cell+vec3i(1,0,0),seed),f.x);
    let b = mix(cell_noise(cell+vec3i(0,1,0),seed),cell_noise(cell+vec3i(1,1,0),seed),f.x);
    let c = mix(cell_noise(cell+vec3i(0,0,1),seed),cell_noise(cell+vec3i(1,0,1),seed),f.x);
    let d = mix(cell_noise(cell+vec3i(0,1,1),seed),cell_noise(cell+1,seed),f.x);
    return mix(mix(a,b,f.y),mix(c,d,f.y),f.z);
}
