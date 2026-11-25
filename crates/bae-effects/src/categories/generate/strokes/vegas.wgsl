struct Params {
    input: FxImage,
    rect: vec4i,
    channel: u32,
    invert: u32,
    stretch: u32,
    reserved: u32,
    threshold: f32,
    pad: u32,
    samples: vec2u,
}

@group(0) @binding(0) var<uniform> u: Params;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var dst: texture_storage_2d<WORKING_FORMAT, write>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let dims = vec2u(u.rect.zw - u.rect.xy);
    if any(id.xy >= u.samples) { return; }
    let pixel = (vec2f(id.xy) + 0.5)*vec2f(dims)/vec2f(u.samples);
    let size = vec2f(u.input.size) / u.input.scale;
    let offset = (size - vec2f(dims)) * 0.5;
    let p = u.input.origin + select(pixel + offset, pixel * size / vec2f(dims), u.stretch != 0u);
    let c = fx_bilinear(src, u.input, p, true);
    var value: f32;
    switch u.channel {
        case 2u: { value = c.r; }
        case 3u: { value = c.g; }
        case 4u: { value = c.b; }
        case 5u: { value = c.a; }
        case 6u: { value = fx_hsl(c.rgb).x; }
        case 7u: { value = fx_hsl(c.rgb).z; }
        case 8u: { value = fx_hsl(c.rgb).y; }
        case 9u: { value = max(c.r, max(c.g, c.b)); }
        default: { value = fx_luma(c.rgb); }
    }
    let classified = select(value, 1.0 - value, u.invert != 0u) - u.threshold;
    textureStore(dst, vec2i(id.xy), vec4f(classified, 0.0, 0.0, 1.0));
}
