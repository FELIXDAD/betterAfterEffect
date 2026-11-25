struct CompoundBlur {
    map_image: FxImage,
    levels: array<FxImage, 7>,
    roi: vec4i,
    layer: vec4i,
    radii_0: vec4f,
    radii_1: vec4f,
    grid: f32,
    placement: u32,
    invert: u32,
    is_float: u32,
}

@group(0) @binding(0) var<uniform> u: CompoundBlur;
@group(0) @binding(1) var map_tex: texture_2d<f32>;
@group(0) @binding(2) var level_0: texture_2d<f32>;
@group(0) @binding(3) var level_1: texture_2d<f32>;
@group(0) @binding(4) var level_2: texture_2d<f32>;
@group(0) @binding(5) var level_3: texture_2d<f32>;
@group(0) @binding(6) var level_4: texture_2d<f32>;
@group(0) @binding(7) var level_5: texture_2d<f32>;
@group(0) @binding(8) var level_6: texture_2d<f32>;
@group(0) @binding(9) var output_tex: texture_storage_2d<WORKING_FORMAT, write>;

fn level(index: u32, p: vec2f) -> vec4f {
    switch index {
        case 0u: { return fx_sample(level_0, u.levels[0], p, false); }
        case 1u: { return fx_bilinear(level_1, u.levels[1], p, false); }
        case 2u: { return fx_bilinear(level_2, u.levels[2], p, false); }
        case 3u: { return fx_bilinear(level_3, u.levels[3], p, false); }
        case 4u: { return fx_bilinear(level_4, u.levels[4], p, false); }
        case 5u: { return fx_bilinear(level_5, u.levels[5], p, false); }
        default: { return fx_bilinear(level_6, u.levels[6], p, false); }
    }
}

fn gaussian_cdf(x: vec2f) -> vec2f {
    let q = clamp(x, vec2f(-6.0), vec2f(6.0));
    return 1.0 / (1.0 + exp(-1.59577 * (q + 0.044715 * q * q * q)));
}

// The measured edge response has a narrower transition and a longer tail
// than a single Gaussian.
fn compound_cdf(distance: vec2f, radius: f32) -> vec2f {
    let x = (distance + 0.19) / (radius + 0.84) - 0.27;
    return mix(1.0 / (1.0 + exp(-1.85 * x)), 1.0 / (1.0 + exp(-10.2 * x)), 0.10);
}

fn gaussian_coverage(near: vec2f, far: vec2f, radius: f32) -> f32 {
    if radius == 0.0 { return 1.0; }
    let sigma = 1.05 * radius + 0.6;
    let a = gaussian_cdf(near / sigma) + gaussian_cdf(far / sigma) - 1.0;
    return a.x * a.y;
}

fn edge_gain(px: vec2f, radius: f32, lo: f32, hi: f32, blend: f32) -> f32 {
    let near = (px - vec2f(u.layer.xy)) * u.grid;
    let far = (vec2f(u.layer.zw) - px) * u.grid;
    let r = max(radius, 1.0);
    let a = max(compound_cdf(near, r) + compound_cdf(far, r) - 1.0, vec2f(0.0));
    let desired = mix(1.0, a.x * a.y, saturate(radius));
    let model = mix(gaussian_coverage(near, far, lo), gaussian_coverage(near, far, hi), blend);
    return clamp(desired / max(model, 0.01), 0.0, 1.2);
}

fn map_luma(px: vec2f) -> f32 {
    let image = u.map_image;
    let size = vec2f(image.size) / image.scale;
    let bounds = vec2f(u.layer.zw - u.layer.xy);
    let local = px - vec2f(u.layer.xy);
    let p = select(local - floor((bounds - size) * 0.5), local * size / bounds, u.placement == 3u);
    // Stretch blends against transparent black at the edge of the map.
    return saturate(fx_luma(fx_bilinear(map_tex, image, image.origin + p, false).rgb));
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let p = u.roi.xy + vec2i(id.xy);
    if any(p >= u.roi.zw) { return; }
    let px = vec2f(p) + 0.5;
    let luma = map_luma(px);
    let radius = min(select(luma, 1.0 - luma, u.invert != 0u) * u.radii_1.w, 31.0);
    if radius == 0.0 {
        textureStore(output_tex, id.xy, level(0u, px));
        return;
    }

    let radii = array<f32, 7>(
        u.radii_0.x, u.radii_0.y, u.radii_0.z, u.radii_0.w,
        u.radii_1.x, u.radii_1.y, u.radii_1.z);
    var upper = 1u;
    for (var i = 1u; i < 6u; i++) {
        if radii[i] < radius { upper += 1u; }
    }
    let a = radii[upper - 1u];
    let b = radii[upper];
    let weight = saturate((radius - a) / (b - a));
    var c = level(upper, px);
    if weight < 1.0 {
        c = mix(level(upper - 1u, px), c, weight);
    }
    let rgba = c * edge_gain(px, radius, a, b, weight);
    textureStore(output_tex, id.xy, select(saturate(rgba), rgba, u.is_float != 0u));
}
