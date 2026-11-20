// Separable box blur: a compute-style declarative effect (spec §11.4, §11.5).
//
// Pass 1 `blur_horizontal`: reads `input`, writes the intermediate texture `tmp`.
// Pass 2 `blur_vertical`:   reads `tmp`,   writes `output`.
//
// The host assembles one module per pass; in each, `bae_store` writes that pass's target
// and `bae_target_pixel` / `bae_in_target` describe that target's rect. Loads outside a
// source's rect return transparent black (AE semantics), so no edge handling is needed.

// Upper bound on taps per side, so a huge radius cannot hang the GPU (spec §21.1).
const MAX_TAPS: i32 = 512;

fn taps_per_side() -> i32 {
    // `radius` is in full-resolution pixels; scale it to the render resolution.
    return clamp(i32(round(bae_scale(params.radius))), 0, MAX_TAPS);
}

@compute @workgroup_size(8, 8)
fn blur_horizontal(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = bae_target_pixel(id);
    if (!bae_in_target(p)) {
        return;
    }
    let n = taps_per_side();
    var sum = vec4<f32>(0.0);
    for (var i = -n; i <= n; i = i + 1) {
        sum = sum + bae_load_input(p + vec2<i32>(i, 0));
    }
    bae_store(p, sum / f32(2 * n + 1));
}

@compute @workgroup_size(8, 8)
fn blur_vertical(@builtin(global_invocation_id) id: vec3<u32>) {
    let p = bae_target_pixel(id);
    if (!bae_in_target(p)) {
        return;
    }
    let n = taps_per_side();
    var sum = vec4<f32>(0.0);
    for (var i = -n; i <= n; i = i + 1) {
        sum = sum + bae_load_tmp(p + vec2<i32>(0, i));
    }
    bae_store(p, sum / f32(2 * n + 1));
}
