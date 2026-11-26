struct Fill {
    src: FxImage, sampled: FxImage, roi: vec4i,
    blend: f32, keep_alpha: u32, mode: u32, float_color: u32,
}
@group(0) @binding(0) var<uniform> u: Fill;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var mean: texture_2d<f32>;
@group(0) @binding(3) var out: texture_storage_2d<WORKING_FORMAT, write>;

@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let p = u.roi.xy + vec2i(id.xy);
    if any(p >= u.roi.zw) { return; }
    let original = fx_sample(src, u.src, vec2f(p) + 0.5, false);
    let color = textureLoad(mean, vec2i(0), 0);
    let alpha = select(select(1.0, color.a, u.mode == 4u), original.a, u.keep_alpha != 0u);
    let mixed = mix(vec4f(color.rgb * alpha, alpha), original, u.blend);
    textureStore(out, vec2i(id.xy), fx_output(fx_unpremult(mixed).rgb, mixed.a, u.float_color != 0u));
}
