// Layer compositing. One instance per layer op (a run of solids is one instanced
// draw). Buffers hold premultiplied RGBA of the working space's encoded values;
// AE's straight-colour blend formulas (docs/spec/ae/compositing-3d-color.md,
// AE-CMP-012/031/032/034) are evaluated on unpremultiplied values.
//
// Normal, Add (float) and Screen use fixed-function blending (the shader returns
// the premultiplied source). Every other case reads the backdrop from a copy of
// the target (`t_dst`) and the pipeline replaces the pixel.

struct Draw {
    // input texel -> target texel: x' = a*x + b*y + c (a, b, c, _), y' likewise.
    fwd_x: vec4<f32>,
    fwd_y: vec4<f32>,
    // target texel -> input texel.
    inv_x: vec4<f32>,
    inv_y: vec4<f32>,
    // Content rectangle in input texels (x0, y0, x1, y1).
    rect: vec4<f32>,
    // Drawn quad in input texels (the rectangle grown by the anti-aliasing margin).
    quad: vec4<f32>,
    // Solid colour, straight RGBA.
    color: vec4<f32>,
    // 1 / texture width, 1 / texture height, perpendicular AA scale along input x, y.
    src: vec4<f32>,
    // Matte texel origin in target texels (x, y) and valid matte size (w, h).
    matte: vec4<f32>,
    // Target texture size (w, h), opacity, _.
    tgt: vec4<f32>,
    // Blend mode index, flags, matte mode (1 alpha, 2 alpha inv, 3 luma, 4 luma inv), _.
    mode: vec4<u32>,
    // Input texel -> mask texture uv: u * xy + zw.
    mask_uv: vec4<f32>,
    // Mask value a + b * sample (x, y).
    mask_val: vec4<f32>,
    // The mask's region in its atlas page (uv min, max).
    mask_clip: vec4<f32>,
}

const FLAG_TEXTURE: u32 = 1u;
const FLAG_STRAIGHT: u32 = 2u;
const FLAG_MATTE: u32 = 4u;
const FLAG_DST: u32 = 8u;
const FLAG_PRESERVE: u32 = 16u;
const FLAG_CLAMP: u32 = 32u;
const FLAG_OPAQUE: u32 = 64u;
const FLAG_MASK: u32 = 128u;

@group(0) @binding(0) var<storage, read> draws: array<Draw>;
@group(1) @binding(0) var t_src: texture_2d<f32>;
@group(1) @binding(1) var t_matte: texture_2d<f32>;
@group(1) @binding(2) var t_dst: texture_2d<f32>;
@group(1) @binding(3) var s_linear: sampler;
@group(1) @binding(4) var t_mask: texture_2d<f32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) draw: u32,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VsOut {
    let d = draws[ii];
    // Two triangles over the corners (x0,y0) (x1,y0) (x0,y1) | (x0,y1) (x1,y0) (x1,y1).
    let cx = vi == 1u || vi == 4u || vi == 5u;
    let cy = vi == 2u || vi == 3u || vi == 5u;
    let p = vec2<f32>(select(d.quad.x, d.quad.z, cx), select(d.quad.y, d.quad.w, cy));
    let t = vec2<f32>(dot(d.fwd_x.xyz, vec3<f32>(p, 1.0)), dot(d.fwd_y.xyz, vec3<f32>(p, 1.0)));
    var out: VsOut;
    out.pos = vec4<f32>(t.x / d.tgt.x * 2.0 - 1.0, 1.0 - t.y / d.tgt.y * 2.0, 0.0, 1.0);
    out.draw = ii;
    return out;
}

fn luma(c: vec3<f32>) -> f32 {
    // Rec.601 weights (AE-CMP-034).
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

// Separable blend functions B(Cb, Cs) on straight colour, indexed like
// `BlendMode` (the AE menu order).
fn blend(mode: u32, cb: vec3<f32>, cs: vec3<f32>) -> vec3<f32> {
    switch mode {
        // Multiply
        case 4u: {
            return cb * cs;
        }
        // Add
        case 9u: {
            return cb + cs;
        }
        // Screen
        case 11u: {
            return cb + cs - cb * cs;
        }
        // Overlay: Hard Light with the layers swapped.
        case 16u: {
            let lo = 2.0 * cs * cb;
            let hi = 1.0 - 2.0 * (1.0 - cs) * (1.0 - cb);
            return select(hi, lo, cb <= vec3<f32>(0.5));
        }
        default: {
            return cs;
        }
    }
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let d = draws[in.draw];
    let flags = d.mode.y;
    let p = in.pos.xy;
    let u = vec2<f32>(dot(d.inv_x.xyz, vec3<f32>(p, 1.0)), dot(d.inv_y.xyz, vec3<f32>(p, 1.0)));

    // Edge coverage, matching AE's resampling of a layer's raster: the edge ramps
    // over max(1 target pixel, 1 source texel) – a box filter when the layer is
    // drawn at or below its size, bilinear magnification with a transparent
    // border when it is enlarged. Edge distances are measured perpendicular, in
    // target pixels, then in units of that footprint.
    let footprint = max(vec2<f32>(1.0), d.src.zw);
    let dl = (u - d.rect.xy) * d.src.zw / footprint;
    let dr = (d.rect.zw - u) * d.src.zw / footprint;
    let cov2 = clamp(min(dl, vec2<f32>(0.5)) + min(dr, vec2<f32>(0.5)), vec2<f32>(0.0), vec2<f32>(1.0));

    var src: vec4<f32>;
    if (flags & FLAG_TEXTURE) != 0u {
        // Minified layers average a grid of bilinear samples over the pixel's
        // footprint in the source (a box prefilter, like AE's downsampling);
        // others take one sample.
        let du_dx = vec2<f32>(d.inv_x.x, d.inv_y.x);
        let du_dy = vec2<f32>(d.inv_x.y, d.inv_y.y);
        let n = u32(clamp(ceil(max(length(du_dx), length(du_dy))), 1.0, 4.0));
        if n == 1u {
            src = textureSampleLevel(t_src, s_linear, u * d.src.xy, 0.0);
        } else {
            var acc = vec4<f32>(0.0);
            for (var j = 0u; j < n; j++) {
                for (var i = 0u; i < n; i++) {
                    let o = (vec2<f32>(f32(i), f32(j)) + 0.5) / f32(n) - 0.5;
                    acc += textureSampleLevel(t_src, s_linear, (u + o.x * du_dx + o.y * du_dy) * d.src.xy, 0.0);
                }
            }
            src = acc / f32(n * n);
        }
        if (flags & FLAG_STRAIGHT) != 0u {
            src = vec4<f32>(src.rgb * src.a, src.a);
        }
        if (flags & FLAG_OPAQUE) != 0u {
            src.a = 1.0;
        }
    } else {
        src = vec4<f32>(d.color.rgb * d.color.a, d.color.a);
    }

    // Coverage x opacity (AE-CMP-031), then the track matte (AE-CMP-034) and masks.
    var k = cov2.x * cov2.y * d.tgt.z;
    if (flags & FLAG_MATTE) != 0u {
        let mt = vec2<i32>(floor(p - d.matte.xy));
        var v = 0.0;
        if all(mt >= vec2<i32>(0)) && all(vec2<f32>(mt) < d.matte.zw) {
            let m = textureLoad(t_matte, mt, 0);
            // Luma of premultiplied colour = Luma(C) * a: transparent counts as black.
            v = select(m.a, luma(m.rgb), d.mode.z >= 3u);
        }
        if d.mode.z == 2u || d.mode.z == 4u {
            v = 1.0 - v;
        }
        k = k * clamp(v, 0.0, 1.0);
    }
    // Layer masks (AE-CMP-038): the combined mask, or one mask's coverage with
    // its value transform, in layer space.
    if (flags & FLAG_MASK) != 0u {
        let muv = clamp(u * d.mask_uv.xy + d.mask_uv.zw, d.mask_clip.xy, d.mask_clip.zw);
        let mv = textureSampleLevel(t_mask, s_linear, muv, 0.0).x;
        k = k * clamp(d.mask_val.x + d.mask_val.y * mv, 0.0, 1.0);
    }
    src = src * k;

    if (flags & FLAG_DST) == 0u {
        return src;
    }

    // Generic blend (AE-CMP-012) / preserve underlying transparency (AE-CMP-032).
    let b = textureLoad(t_dst, vec2<i32>(floor(p)), 0);
    let a_s = src.a;
    let a_b = b.a;
    let cs = src.rgb / max(a_s, 1e-6);
    let cb = b.rgb / max(a_b, 1e-6);
    let clamped = (flags & FLAG_CLAMP) != 0u;
    var bl = blend(d.mode.x, cb, cs);
    if clamped {
        bl = clamp(bl, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    var co: vec3<f32>;
    var ao: f32;
    if d.mode.x == 9u && (flags & FLAG_PRESERVE) == 0u {
        // AE's Add is additive on premultiplied colour (measured against AE 26.2,
        // AE-CMP-018): backdrop + source x alpha, alpha is the union.
        ao = a_s + a_b * (1.0 - a_s);
        co = src.rgb + b.rgb;
    } else if (flags & FLAG_PRESERVE) != 0u {
        ao = a_b;
        co = a_s * a_b * bl + (1.0 - a_s) * b.rgb;
    } else {
        ao = a_s + a_b * (1.0 - a_s);
        co = (1.0 - a_b) * src.rgb + (1.0 - a_s) * b.rgb + a_s * a_b * bl;
    }
    if clamped {
        ao = clamp(ao, 0.0, 1.0);
        co = clamp(co, vec3<f32>(0.0), vec3<f32>(ao));
    }
    return vec4<f32>(co, ao);
}
