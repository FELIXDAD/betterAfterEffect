// Working buffer (premultiplied float) -> output image: 8-bit (PNG / TIFF
// export, agent screenshots, thumbnails) or float (EXR export). Values are the
// working space's encoded values, so no transfer function is applied here
// (colour conversion is workstream 06).

struct Params {
    // Background (straight RGB) for mode 2.
    bg: vec4<f32>,
    // x: 0 premultiplied, 1 straight alpha, 2 over the background (opaque).
    // y: 1 = float target: keep colour outside [0, alpha] (32-bpc super-whites).
    mode: vec4<u32>,
}

@group(0) @binding(0) var t_in: texture_2d<f32>;
@group(0) @binding(1) var<uniform> params: Params;

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    // One triangle covering the viewport.
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_out(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    var c = textureLoad(t_in, vec2<i32>(floor(pos.xy)), 0);
    c.a = clamp(c.a, 0.0, 1.0);
    if params.mode.y == 0u {
        c = vec4<f32>(clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(c.a)), c.a);
    }
    switch params.mode.x {
        case 1u: {
            if c.a > 0.0 {
                c = vec4<f32>(c.rgb / c.a, c.a);
            } else {
                c = vec4<f32>(0.0);
            }
        }
        case 2u: {
            c = vec4<f32>(c.rgb + (1.0 - c.a) * params.bg.rgb, 1.0);
        }
        default: {}
    }
    // Resolve halfway values before the adapter's UNORM conversion.
    if params.mode.y == 0u {
        return floor(saturate(c) * 255.0 + 0.5) / 255.0;
    }
    return c;
}
