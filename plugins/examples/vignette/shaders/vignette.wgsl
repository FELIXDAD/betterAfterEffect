// Vignette: a pixel-style declarative effect (docs/spec/plugin-architecture.md §11.4).
//
// `frame`, `params`, `BaePixel` and the `bae_*` helpers come from the prelude the host
// generates from bae-plugin.toml (§11.3); this file declares no bindings. Generated
// params struct for this manifest:
//
//   struct BaeParams { amount: f32, center: vec2<f32>, radius: f32,
//                      softness: f32, shape: u32, color: vec4<f32> }
//
// The function receives the premultiplied working-space input color at one output pixel
// and returns the output color. It reads no textures, so the host may fuse it with
// neighbouring pixel-style effects into one kernel (§10.9).

const SHAPE_FIT_LAYER: u32 = 2u;

fn vignette(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    // Measure in full-resolution layer pixels so the look is identical at any preview
    // resolution. `params.center` arrives in render pixels (downsample applied).
    let layer_size = vec2<f32>(frame.layer_bounds.max - frame.layer_bounds.min) / frame.downsample;
    let half_size = max(0.5 * layer_size, vec2<f32>(1.0));
    let d = px.layer_pos - bae_to_layer(params.center);

    // Normalized distance: 1.0 at the layer corner (circle: half diagonal).
    var r: f32;
    if (params.shape == SHAPE_FIT_LAYER) {
        r = length(d / half_size) * 0.70710678;
    } else {
        r = length(d) / length(half_size);
    }

    let t = smoothstep(params.radius, params.radius + params.softness, r)
        * (params.amount / 100.0) * params.color.a;

    // Blend the unpremultiplied color toward the vignette color in linear light, keep the
    // layer's coverage (alpha) unchanged, and premultiply again.
    let lin = bae_to_linear(bae_unpremultiply(c));
    let tint = bae_to_linear(vec4<f32>(params.color.rgb, 1.0));
    let mixed = vec4<f32>(mix(lin.rgb, tint.rgb, t), c.a);
    return bae_premultiply(bae_from_linear(mixed));
}
