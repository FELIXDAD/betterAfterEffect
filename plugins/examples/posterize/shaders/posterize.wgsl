// Posterize: a pixel-style declarative effect (docs/plugins/quickstart.md).
//
// The host generates `params` from bae-plugin.toml:
//   struct BaeParams { levels: i32, linear: u32 }
// and calls this function once per output pixel with the premultiplied input color.

fn posterize(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    // Work on straight (unpremultiplied) color so edges keep their coverage.
    var s = bae_unpremultiply(c);
    if (params.linear != 0u) {
        s = bae_to_linear(s);
    }
    let n = f32(max(params.levels, 2) - 1);
    let q = vec4<f32>(floor(clamp(s.rgb, vec3<f32>(0.0), vec3<f32>(1.0)) * n + 0.5) / n, s.a);
    var out = q;
    if (params.linear != 0u) {
        out = bae_from_linear(q);
    }
    return bae_premultiply(out);
}
