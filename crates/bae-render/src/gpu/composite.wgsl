// Layer compositing. One instance per layer op (a run of solids is one instanced
// draw). Buffers hold premultiplied RGBA of the working space's encoded values;
// AE's straight-colour blend formulas (docs/spec/ae/compositing-3d-color.md,
// AE-CMP-012/031/032/034) are evaluated on unpremultiplied values.
//
// Normal, Add (float) and Screen use fixed-function blending (the shader returns
// the premultiplied source). Every other case reads the backdrop from a copy of
// the target (`t_dst`) and the pipeline replaces the pixel.
//
// 3D layers (FLAG_PROJECTIVE) use full homographies: the vertex stage emits
// homogeneous clip positions (w = camera depth, clipped at the near plane) and
// the fragment stage maps each pixel back into the layer with the inverse,
// measuring edge ramps with the local Jacobian. Pieces of layers split at 3D
// intersections carry clip half-planes in layer space. Motion-blurred layers
// (FLAG_MOTION) draw one quad over all placements and average the layer at
// every sample placement per pixel (AE-CMP-064). Lit 3D layers (FLAG_LIT) are
// shaded with AE's Classic 3D lighting from a record in `lighting`
// (light.rs: layout, CPU reference). Plain, lit and blurred draws use separate
// entry points so the plain path carries none of that code.

struct Draw {
    // input texel -> target texel: x' = a * x + b * y + c (a, b, c, ROI origin x), y' likewise.
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
    // Target texture size (w, h), opacity, integer channel maximum (0 for float).
    tgt: vec4<f32>,
    // Blend mode index, flags, matte mode, dissolve seed.
    mode: vec4<u32>,
    // Input texel -> mask texture uv: u * xy + zw.
    mask_uv: vec4<f32>,
    // Mask value a + b * sample (x, y).
    mask_val: vec4<f32>,
    // The mask's region in its atlas page (uv min, max).
    mask_clip: vec4<f32>,
    // Forward homography w row (input texel -> target texel), near-plane weight.
    fwd_w: vec4<f32>,
    // Inverse homography w row.
    inv_w: vec4<f32>,
    // Clip half-planes a * u + b * v + c >= 0 in input texels; w: coverage
    // reach past the cut in target pixels.
    clips: array<vec4<f32>, 4>,
    // Clip count, first motion sample, sample count, lighting record (vec4 index).
    extra: vec4<u32>,
}

// One motion-blur placement: target texel -> input texel (rows of the inverse
// homography). 3D samples: inv_w.w = 1 / near. 2D samples: the inverse is
// affine (inv_w = (0, 0, 1)), so inv_w.xy carries the edge scales instead.
struct Sample {
    inv_x: vec4<f32>,
    inv_y: vec4<f32>,
    inv_w: vec4<f32>,
}

// Discriminants of BlendMode in blend.rs.
const MODE_DISSOLVE: u32 = 1u;
const MODE_DANCING_DISSOLVE: u32 = 2u;
const MODE_DARKEN: u32 = 3u;
const MODE_MULTIPLY: u32 = 4u;
const MODE_COLOR_BURN: u32 = 5u;
const MODE_CLASSIC_COLOR_BURN: u32 = 6u;
const MODE_LINEAR_BURN: u32 = 7u;
const MODE_DARKER_COLOR: u32 = 8u;
const MODE_ADD: u32 = 9u;
const MODE_LIGHTEN: u32 = 10u;
const MODE_SCREEN: u32 = 11u;
const MODE_COLOR_DODGE: u32 = 12u;
const MODE_CLASSIC_COLOR_DODGE: u32 = 13u;
const MODE_LINEAR_DODGE: u32 = 14u;
const MODE_LIGHTER_COLOR: u32 = 15u;
const MODE_OVERLAY: u32 = 16u;
const MODE_SOFT_LIGHT: u32 = 17u;
const MODE_HARD_LIGHT: u32 = 18u;
const MODE_LINEAR_LIGHT: u32 = 19u;
const MODE_VIVID_LIGHT: u32 = 20u;
const MODE_PIN_LIGHT: u32 = 21u;
const MODE_HARD_MIX: u32 = 22u;
const MODE_DIFFERENCE: u32 = 23u;
const MODE_CLASSIC_DIFFERENCE: u32 = 24u;
const MODE_EXCLUSION: u32 = 25u;
const MODE_SUBTRACT: u32 = 26u;
const MODE_DIVIDE: u32 = 27u;
const MODE_HUE: u32 = 28u;
const MODE_SATURATION: u32 = 29u;
const MODE_COLOR: u32 = 30u;
const MODE_LUMINOSITY: u32 = 31u;
const MODE_STENCIL_ALPHA: u32 = 32u;
const MODE_STENCIL_LUMA: u32 = 33u;
const MODE_SILHOUETTE_ALPHA: u32 = 34u;
const MODE_SILHOUETTE_LUMA: u32 = 35u;
const MODE_ALPHA_ADD: u32 = 36u;
const MODE_LUMINESCENT_PREMUL: u32 = 37u;

const FLAG_TEXTURE: u32 = 1u;
const FLAG_STRAIGHT: u32 = 2u;
const FLAG_MATTE: u32 = 4u;
const FLAG_DST: u32 = 8u;
const FLAG_PRESERVE: u32 = 16u;
const FLAG_CLAMP: u32 = 32u;
const FLAG_OPAQUE: u32 = 64u;
const FLAG_MASK: u32 = 128u;
const FLAG_PROJECTIVE: u32 = 256u;
const FLAG_MOTION: u32 = 512u;
const FLAG_LIT: u32 = 1024u;
// Motion blur: a lighting record per sample, back to back from extra.w.
const FLAG_LIT_SAMPLES: u32 = 2048u;

@group(0) @binding(0) var<storage, read> draws: array<Draw>;
@group(0) @binding(1) var<storage, read> samples: array<Sample>;
// Lighting records (light.rs `Lighting::pack`): P(u) = r0 u.x + r1 u.y + r2,
// normal + light count, eye + comp-space pixel size (> 0: per unit distance,
// < 0: constant), material (ambient, diffuse, specular, shininess), (metal,
// caster count, caster block offset), then 5 vec4s per light: position + kind
// (0 ambient, 1 parallel, 2 point, 3 spot), direction + falloff (0 none,
// 1 smooth, 2 inverse square), colour · intensity + radius, (cos outer,
// cos inner, falloff distance), (shadow darkness, diffusion); then 8 vec4s per
// shadow caster: plane, world -> layer px rows (2), rect, solid colour, atlas
// uv map, atlas uv clip, (opacity, transmission, layer px per comp px, kind).
@group(0) @binding(2) var<storage, read> lighting: array<vec4<f32>>;
@group(1) @binding(0) var t_src: texture_2d<f32>;
@group(1) @binding(1) var t_matte: texture_2d<f32>;
@group(1) @binding(2) var t_dst: texture_2d<f32>;
@group(1) @binding(3) var s_linear: sampler;
@group(1) @binding(4) var t_mask: texture_2d<f32>;
// Image shadow casters of the composite (exec/shadow.rs).
@group(1) @binding(5) var t_shadow: texture_2d<f32>;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) draw: u32,
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VsOut {
    let d = draws[ii];
    // Two triangles over the corners (x0, y0) (x1, y0) (x0, y1) | (x0, y1) (x1, y0) (x1, y1).
    let cx = vi == 1u || vi == 4u || vi == 5u;
    let cy = vi == 2u || vi == 3u || vi == 5u;
    let p = vec3<f32>(select(d.quad.x, d.quad.z, cx), select(d.quad.y, d.quad.w, cy), 1.0);
    // Homogeneous target position (w = 1 for affine draws).
    var t = vec2<f32>(dot(d.fwd_x.xyz, p), dot(d.fwd_y.xyz, p));
    var w = dot(d.fwd_w.xyz, p);
    if d.mode.x == MODE_STENCIL_ALPHA || d.mode.x == MODE_STENCIL_LUMA {
        // Outside the source a stencil clears the entire underlying composite.
        t = vec2<f32>(select(0.0, d.tgt.x, cx), select(0.0, d.tgt.y, cy));
        w = 1.0;
    }
    var out: VsOut;
    // z = (w - near) / 2 keeps 0 <= z <= w only in front of the near plane.
    out.pos = vec4<f32>(t.x / d.tgt.x * 2.0 - w, w - t.y / d.tgt.y * 2.0, 0.5 * (w - d.fwd_w.w), w);
    out.draw = ii;
    return out;
}

fn luma(c: vec3<f32>) -> f32 {
    // Rec.601 weights (AE-CMP-034).
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

fn burn(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return select(select(1.0 - min(vec3<f32>(1), (1.0 - b) / s), vec3<f32>(0), s <= vec3<f32>(0)),
                  vec3<f32>(1), b >= vec3<f32>(1));
}

fn dodge(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return 1.0 - burn(1.0 - b, 1.0 - s);
}

fn hard_light(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return select(1.0 - 2.0 * (1.0 - b) * (1.0 - s), 2.0 * b * s, s <= vec3<f32>(0.5));
}

fn lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.30, 0.59, 0.11));
}

fn sat(c: vec3<f32>) -> f32 {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

fn set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    let range = sat(c);
    if range == 0.0 { return vec3<f32>(0); }
    return (c - min(c.r, min(c.g, c.b))) * (s / range);
}

fn set_lum(c: vec3<f32>, light: f32) -> vec3<f32> {
    var v = c + (light - lum(c));
    let lo = min(v.r, min(v.g, v.b));
    let hi = max(v.r, max(v.g, v.b));
    if lo < 0.0 && light > lo { v = light + (v - light) * (light / (light - lo)); }
    if hi > 1.0 && hi > light { v = light + (v - light) * ((1.0 - light) / (hi - light)); }
    return saturate(v);
}

// Straight-colour overlap functions in AE menu order (AE-CMP-015 through 026).
fn blend(mode: u32, b: vec3<f32>, s: vec3<f32>, bounded: bool) -> vec3<f32> {
    switch mode {
        case MODE_DARKEN: { return min(b, s); }
        case MODE_MULTIPLY: { return b * s; }
        case MODE_COLOR_BURN: { return burn(b, s); }
        case MODE_LINEAR_BURN: { return b + s - 1.0; }
        case MODE_DARKER_COLOR: { return select(b, s, dot(s, vec3<f32>(1)) < dot(b, vec3<f32>(1))); }
        case MODE_ADD: { return b + s; }
        case MODE_LIGHTEN: { return max(b, s); }
        case MODE_SCREEN: { return b + s - b * s; }
        case MODE_COLOR_DODGE: { return dodge(b, s); }
        case MODE_LINEAR_DODGE: { return min(b + s, vec3<f32>(1)); }
        case MODE_LIGHTER_COLOR: { return select(b, s, dot(s, vec3<f32>(1)) > dot(b, vec3<f32>(1))); }
        case MODE_OVERLAY: { return hard_light(s, b); }
        case MODE_SOFT_LIGHT: {
            let d = select(sqrt(max(b, vec3<f32>(0))), ((16.0 * b - 12.0) * b + 4.0) * b, b <= vec3<f32>(0.25));
            return select(b + (2.0 * s - 1.0) * (d - b), b - (1.0 - 2.0 * s) * b * (1.0 - b), s <= vec3<f32>(0.5));
        }
        case MODE_HARD_LIGHT: { return hard_light(b, s); }
        case MODE_LINEAR_LIGHT: { return b + 2.0 * s - 1.0; }
        case MODE_VIVID_LIGHT: { return select(dodge(b, 2.0 * s - 1.0), burn(b, 2.0 * s), s <= vec3<f32>(0.5)); }
        case MODE_PIN_LIGHT: { return select(max(b, 2.0 * s - 1.0), min(b, 2.0 * s), s <= vec3<f32>(0.5)); }
        case MODE_HARD_MIX: { return step(vec3<f32>(1), b + s); }
        case MODE_DIFFERENCE: { return abs(b - s); }
        case MODE_EXCLUSION: { return b + s - 2.0 * b * s; }
        case MODE_SUBTRACT: { return b - s; }
        case MODE_DIVIDE: {
            if bounded { return select(select(vec3<f32>(0), vec3<f32>(1), b > vec3<f32>(0)), b / s, s > vec3<f32>(0)); }
            return clamp(b / max(s, vec3<f32>(1e-6)), vec3<f32>(-65504), vec3<f32>(65504));
        }
        case MODE_HUE: { return set_lum(set_sat(s, sat(b)), lum(b)); }
        case MODE_SATURATION: { return set_lum(set_sat(b, sat(s)), lum(b)); }
        case MODE_COLOR: { return set_lum(s, lum(b)); }
        case MODE_LUMINOSITY: { return set_lum(b, lum(s)); }
        default: { return s; }
    }
}

// AE-CMP-026: for partial alpha, AE takes the base colour from the
// destination-over-source composite before transferring hue/saturation/luma.
fn component_mix(mode: u32, src: vec3<f32>, back: vec3<f32>, a: f32, ab: f32, levels: f32) -> vec4<f32> {
    var opacity = a;
    var base_alpha = ab;
    var bp = back * ab;
    var union_alpha = a + ab * (1.0 - a);
    if levels > 0.0 {
        opacity = floor(a * levels + 0.5) / levels;
        base_alpha = floor(ab * levels + 0.5) / levels;
        bp = floor(back * base_alpha * levels + 0.5) / levels;
        union_alpha = floor((opacity + base_alpha * (1.0 - opacity)) * levels + 0.5) / levels;
    }
    if union_alpha <= 0.0 { return vec4<f32>(0); }
    var base = (bp + src * opacity * (1.0 - base_alpha)) / union_alpha;
    if levels > 0.0 { base = floor(base * levels + 0.5) / levels; }
    let transferred = blend(mode, base, src, levels > 0.0);
    return vec4<f32>(union_alpha * mix(base, transferred, opacity), union_alpha);
}

fn noise(pixel: vec2<u32>, seed: u32) -> f32 {
    var h = seed ^ (pixel.x * 0x9e3779b9u) ^ (pixel.y * 0x85ebca6bu);
    h = (h ^ (h >> 16u)) * 0x7feb352du;
    h = (h ^ (h >> 15u)) * 0x846ca68bu;
    return f32((h ^ (h >> 16u)) >> 8u) / 16777216.0;
}

// A layer's pixel at target position p for one placement of the layer (rows
// ix, iy, iw of the target texel -> input texel map): premultiplied colour, the
// raw texel colour (Luminescent Premul) and the coverage factor k (edge ramps,
// mask, 3D clip lines; not opacity or matte).
struct Px {
    src: vec4<f32>,
    light: vec3<f32>,
    k: f32,
}

// GLSL smoothstep that also handles e0 == e1 (a hard step) and e0 > e1.
fn step_smooth(e0: f32, e1: f32, x: f32) -> f32 {
    if e0 == e1 { return select(0.0, 1.0, x >= e0); }
    let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// AE's Classic 3D lighting (AE-CMP-436, light.rs `shade`) of straight colour
// `c` at input texel `u`, from the lighting record at `o`.
struct Shaded {
    color: vec3<f32>,
    // The share of the shadow-casting lights' diffuse light the layer's
    // shadows take away (Accepts Shadows: Only shows just that).
    shadow: f32,
}

fn shade(o: u32, u: vec2<f32>, c: vec3<f32>) -> Shaded {
    let p = lighting[o].xyz * u.x + lighting[o + 1u].xyz * u.y + lighting[o + 2u].xyz;
    let header = lighting[o + 3u];
    let eye = lighting[o + 4u];
    let v = normalize(eye.xyz - p);
    let pixel = select(-eye.w, eye.w * length(eye.xyz - p), eye.w > 0.0);
    let mat = lighting[o + 5u];
    var n = header.xyz;
    if dot(n, v) < 0.0 { n = -n; }
    var amb = vec3<f32>(0.0);
    var diff = vec3<f32>(0.0);
    var spec = vec3<f32>(0.0);
    var free = 0.0;
    var kept = 0.0;
    let count = u32(header.w);
    for (var i = 0u; i < count; i++) {
        let b = o + 7u + 5u * i;
        let l0 = lighting[b];
        let l1 = lighting[b + 1u];
        let l2 = lighting[b + 2u];
        let l3 = lighting[b + 3u];
        let l4 = lighting[b + 4u];
        let kind = u32(l0.w);
        if kind == 0u {
            amb += l2.rgb;
            continue;
        }
        var to_light: vec3<f32>;
        var dist: f32;
        if kind == 1u {
            to_light = -l1.xyz;
            dist = dot(p - l0.xyz, l1.xyz);
        } else {
            let t = l0.xyz - p;
            dist = length(t);
            to_light = t / max(dist, 1e-6);
        }
        let nl = dot(n, to_light);
        if nl <= 0.0 { continue; }
        var atten = 1.0;
        let falloff = u32(l1.w);
        if falloff == 1u {
            atten = 1.0 - step_smooth(l2.w, l2.w + l3.z, dist);
        } else if falloff == 2u && dist > l2.w {
            atten = (l2.w / dist) * (l2.w / dist);
        }
        if kind == 3u {
            atten *= step_smooth(l3.x, l3.y, dot(l1.xyz, -to_light));
        }
        var k = l2.rgb * atten;
        if l4.x > 0.0 {
            let full = dot(k * nl, vec3<f32>(0.2126, 0.7152, 0.0722));
            if l4.z >= 0.0 {
                // The layer's shadow map (shadowmap.wgsl): slot l4.z of
                // l4.w × l3.w texels; the transform sits in the casters' place.
                let xf = lighting[o + u32(lighting[o + 6u].z)];
                let t = vec2<f32>(u.x * xf.x + xf.y, u.y * xf.z + xf.w);
                let size = vec2<f32>(l4.w, l3.w);
                let q = vec2<f32>(l4.z * l4.w, 0.0) + clamp(t, vec2<f32>(0.5), size - 0.5);
                k *= textureSampleLevel(t_shadow, s_linear, q / vec2<f32>(textureDimensions(t_shadow)), 0.0).rgb;
            } else {
                k *= shadow(o, p, u, i, l0, l1, l4, pixel);
            }
            free += full;
            kept += dot(k * nl, vec3<f32>(0.2126, 0.7152, 0.0722));
        }
        diff += k * nl;
        let nh = max(dot(n, normalize(to_light + v)), 1e-8);
        spec += k * select(pow(nh, mat.w), 1.0, mat.w <= 0.0);
    }
    let spec_color = mix(vec3<f32>(1.0), c, lighting[o + 6u].x);
    var out: Shaded;
    out.color = max(c * (mat.x * amb + mat.y * diff) + mat.z * spec_color * spec, vec3<f32>(0.0));
    out.shadow = select(0.0, clamp(1.0 - kept / free, 0.0, 1.0), free > 0.0);
    return out;
}

// `d` is draw `di`, loaded once by the caller (motion blur calls this per
// sample); `k_affine` are the edge scales of an affine placement; `lit` turns
// on lighting (a constant at each call site, so plain draws compile without).
fn layer_px(d: Draw, di: u32, p: vec2<f32>, ix: vec4<f32>, iy: vec4<f32>, iw: vec4<f32>, projective: bool, k_affine: vec2<f32>, lit: bool, rec: u32) -> Px {
    let flags = d.mode.y;
    let hp = vec3<f32>(p, 1.0);
    var out: Px;
    var u: vec2<f32>;
    var du_dx: vec2<f32>;
    var du_dy: vec2<f32>;
    // Target pixels per input texel across the edges x = const and y = const.
    var kxy: vec2<f32>;
    if projective {
        let uh = vec3<f32>(dot(ix.xyz, hp), dot(iy.xyz, hp), dot(iw.xyz, hp));
        // uh.z = 1 / camera depth: nothing at or behind the near plane.
        if iw.w > 0.0 && (uh.z <= 0.0 || uh.z > iw.w) { return out; }
        u = uh.xy / uh.z;
        du_dx = (vec2<f32>(ix.x, iy.x) - u * iw.x) / uh.z;
        du_dy = (vec2<f32>(ix.y, iy.y) - u * iw.y) / uh.z;
        let gx = length(vec2<f32>(du_dx.x, du_dy.x));
        let gy = length(vec2<f32>(du_dx.y, du_dy.y));
        kxy = 1.0 / max(vec2<f32>(gx, gy), vec2<f32>(1e-20));
    } else {
        u = vec2<f32>(dot(ix.xyz, hp), dot(iy.xyz, hp));
        du_dx = vec2<f32>(ix.x, iy.x);
        du_dy = vec2<f32>(ix.y, iy.y);
        kxy = k_affine;
    }

    // Edge coverage, matching AE's resampling of a layer's raster: the edge ramps
    // over max(1 target pixel, 1 source texel) – a box filter when the layer is
    // drawn at or below its size, bilinear magnification with a transparent
    // border when it is enlarged. Edge distances are measured perpendicular, in
    // target pixels, then in units of that footprint.
    let footprint = max(vec2<f32>(1.0), kxy);
    let dl = (u - d.rect.xy) * kxy / footprint;
    let dr = (d.rect.zw - u) * kxy / footprint;
    let cov2 = clamp(min(dl, vec2<f32>(0.5)) + min(dr, vec2<f32>(0.5)), vec2<f32>(0.0), vec2<f32>(1.0));
    var k = cov2.x * cov2.y;
    if k <= 0.0 { return out; }

    var src: vec4<f32>;
    var light: vec3<f32>;
    if (flags & FLAG_TEXTURE) != 0u {
        // Minified layers average a grid of bilinear samples over the pixel's
        // footprint in the source (a box prefilter, like AE's downsampling);
        // others take one sample.
        // Lookups stay half a texel inside the content, so texels beyond it
        // (pooled textures are larger) never bleed in: edges come from the
        // coverage ramp alone, alike on all four sides.
        let lo = d.rect.xy + 0.5;
        let hi = max(d.rect.zw - 0.5, lo);
        let n = u32(clamp(ceil(max(length(du_dx), length(du_dy))), 1.0, 4.0));
        if n == 1u {
            src = textureSampleLevel(t_src, s_linear, clamp(u, lo, hi) * d.src.xy, 0.0);
        } else {
            var acc = vec4<f32>(0.0);
            for (var j = 0u; j < n; j++) {
                for (var i = 0u; i < n; i++) {
                    let o = (vec2<f32>(f32(i), f32(j)) + 0.5) / f32(n) - 0.5;
                    let us = clamp(u + o.x * du_dx + o.y * du_dy, lo, hi);
                    acc += textureSampleLevel(t_src, s_linear, us * d.src.xy, 0.0);
                }
            }
            src = acc / f32(n * n);
        }
        light = src.rgb;
        if (flags & FLAG_STRAIGHT) != 0u {
            src = vec4<f32>(src.rgb * src.a, src.a);
        }
        if (flags & FLAG_OPAQUE) != 0u {
            src.a = 1.0;
        }
    } else {
        light = d.color.rgb;
        src = vec4<f32>(d.color.rgb * d.color.a, d.color.a);
    }
    if lit && src.a > 0.0 {
        let shaded = shade(rec, u, src.rgb / src.a);
        if lighting[rec + 6u].w > 0.5 {
            // Accepts Shadows: Only: just the darkness of the shadows on it.
            src = vec4<f32>(0.0, 0.0, 0.0, src.a * shaded.shadow);
            light = vec3<f32>(0.0);
        } else {
            var c = shaded.color;
            if (flags & FLAG_CLAMP) != 0u { c = min(c, vec3<f32>(1.0)); }
            src = vec4<f32>(c * src.a, src.a);
            light = c;
        }
    }

    // Layer masks (AE-CMP-038): the combined mask, or one mask's coverage with
    // its value transform, in layer space.
    if (flags & FLAG_MASK) != 0u {
        let muv = clamp(u * d.mask_uv.xy + d.mask_uv.zw, d.mask_clip.xy, d.mask_clip.zw);
        let mv = textureSampleLevel(t_mask, s_linear, muv, 0.0).x;
        k = k * clamp(d.mask_val.x + d.mask_val.y * mv, 0.0, 1.0);
    }
    // Pieces of 3D layers cut at intersections (AE-CMP-405): the piece drawn
    // first reaches c.w pixels past the cut, the one drawn over it
    // anti-aliases it, so a cut never leaves a see-through seam.
    let clip_count = min(d.extra.x, 4u);
    for (var i = 0u; i < clip_count; i++) {
        let c = draws[di].clips[i];
        let g = vec2<f32>(dot(c.xy, du_dx), dot(c.xy, du_dy));
        k = k * clamp((dot(c.xy, u) + c.z) / max(length(g), 1e-20) + 0.5 + c.w, 0.0, 1.0);
    }
    out.src = src;
    out.light = light;
    out.k = k;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let d = draws[in.draw];
    let px = layer_px(d, in.draw, in.pos.xy, d.inv_x, d.inv_y, d.inv_w, (d.mode.y & FLAG_PROJECTIVE) != 0u, d.src.zw, false, 0u);
    return finish(d, in.pos.xy, px.src, px.light, px.k);
}

// Lit 3D layers (their own pipeline).
@fragment
fn fs_lit(in: VsOut) -> @location(0) vec4<f32> {
    let d = draws[in.draw];
    let px = layer_px(d, in.draw, in.pos.xy, d.inv_x, d.inv_y, d.inv_w, (d.mode.y & FLAG_PROJECTIVE) != 0u, d.src.zw, true, d.extra.w);
    return finish(d, in.pos.xy, px.src, px.light, px.k);
}

// Motion-blurred draws (their own pipeline, so the sample loop costs plain
// draws nothing): the equal-weight average of the layer at every sample
// placement, premultiplied, before opacity and matte (AE-CMP-064).
@fragment
fn fs_motion(in: VsOut) -> @location(0) vec4<f32> {
    let d = draws[in.draw];
    let p = in.pos.xy;
    var acc = vec4<f32>(0.0);
    var acc_light = vec3<f32>(0.0);
    let n = max(d.extra.z, 1u);
    let projective = (d.mode.y & FLAG_PROJECTIVE) != 0u;
    let lit = (d.mode.y & FLAG_LIT) != 0u;
    // Lit per sample: records of 7 + 5 per light + (8 + lights) per caster
    // + 1 + 64 per light (the shadow grid) vec4s.
    var stride = 0u;
    if lit && (d.mode.y & FLAG_LIT_SAMPLES) != 0u {
        let o = d.extra.w;
        let nl = u32(lighting[o + 3u].w);
        stride = 7u + 5u * nl + (8u + nl) * u32(lighting[o + 6u].y) + 1u + 64u * nl;
    }
    for (var i = 0u; i < n; i++) {
        let s = samples[d.extra.y + i];
        let px = layer_px(d, in.draw, p, s.inv_x, s.inv_y, s.inv_w, projective, s.inv_w.xy, lit, d.extra.w + i * stride);
        acc += px.src * px.k;
        acc_light += px.light * px.k;
    }
    return finish(d, p, acc / f32(n), acc_light / f32(n), 1.0);
}

// Opacity, track matte and blending of a layer pixel: `src` premultiplied,
// `light` the raw colour (Luminescent Premul), `k_in` its coverage.
fn finish(d: Draw, p: vec2<f32>, src_in: vec4<f32>, light: vec3<f32>, k_in: f32) -> vec4<f32> {
    let flags = d.mode.y;
    var src = src_in;
    var k = k_in;
    // Opacity (AE-CMP-031), then the track matte (AE-CMP-034).
    k = k * d.tgt.z;
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
    src = src * k;

    if (flags & FLAG_DST) == 0u {
        return src;
    }

    // Generic blend (AE-CMP-012) / preserve underlying transparency (AE-CMP-032).
    let b = textureLoad(t_dst, vec2<i32>(floor(p)), 0);
    let mode = d.mode.x;
    if mode >= MODE_STENCIL_ALPHA && mode <= MODE_SILHOUETTE_LUMA {
        var m = saturate(select(src.a, luma(src.rgb), mode == MODE_STENCIL_LUMA || mode == MODE_SILHOUETTE_LUMA));
        if mode >= MODE_SILHOUETTE_ALPHA { m = 1.0 - m; }
        return b * m;
    }
    if mode == MODE_DISSOLVE || mode == MODE_DANCING_DISSOLVE {
        let pixel = vec2<u32>(p + vec2<f32>(d.fwd_x.w, d.fwd_y.w));
        if noise(pixel, d.mode.w) >= src.a { return b; }
        src = vec4<f32>(src.rgb / src.a, 1.0);
    }
    let a_s = src.a;
    let a_b = b.a;
    let cs = src.rgb / select(1.0, a_s, a_s > 0.0);
    let cb = b.rgb / select(1.0, a_b, a_b > 0.0);
    let clamped = (flags & FLAG_CLAMP) != 0u;
    // Avoid integer re-quantization on uncovered pixels. Luminescent Premul
    // can intentionally carry light even in samples with zero intrinsic alpha.
    if mode != MODE_LUMINESCENT_PREMUL {
        if a_s == 0.0 { return b; }
        if a_b == 0.0 {
            return select(src, vec4<f32>(0), (flags & FLAG_PRESERVE) != 0u);
        }
    }
    if mode >= MODE_HUE && mode <= MODE_LUMINOSITY && (flags & FLAG_PRESERVE) == 0u {
        return component_mix(mode, cs, cb, a_s, a_b, d.tgt.w);
    }
    var bl = blend(mode, cb, cs, clamped);
    if clamped {
        bl = clamp(bl, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    var co: vec3<f32>;
    var ao = a_s + a_b * (1.0 - a_s);
    switch mode {
        // The legacy operators act directly on premultiplied channels. Burn
        // is the complement of Dodge in the union-alpha output.
        case MODE_CLASSIC_COLOR_BURN: {
            let denominator = 1.0 - a_s + src.rgb;
            let value = ao - (a_b - b.rgb) / max(denominator, vec3<f32>(1e-6));
            co = select(max(vec3<f32>(0), value), vec3<f32>(0), denominator <= vec3<f32>(0));
        }
        case MODE_ADD: { co = src.rgb + b.rgb; }
        case MODE_CLASSIC_COLOR_DODGE: {
            let value = b.rgb / max(1.0 - src.rgb, vec3<f32>(1e-6));
            co = select(min(vec3<f32>(ao), value), vec3<f32>(ao), src.rgb >= vec3<f32>(1));
        }
        case MODE_CLASSIC_DIFFERENCE: { co = abs(b.rgb - src.rgb); }
        case MODE_ALPHA_ADD: {
            ao = min(1.0, a_s + a_b);
            co = src.rgb + cb * (ao - a_s);
        }
        case MODE_LUMINESCENT_PREMUL: { co = light * k + b.rgb * (1.0 - a_s); }
        default: { co = (1.0 - a_b) * src.rgb + (1.0 - a_s) * b.rgb + a_s * a_b * bl; }
    }
    if (flags & FLAG_PRESERVE) != 0u {
        // Remove the source contribution outside the underlying coverage.
        if mode == MODE_ADD {
            co = a_s * a_b * bl + (1.0 - a_s) * b.rgb;
        } else {
            co -= (1.0 - a_b) * select(src.rgb, light * k, mode == MODE_LUMINESCENT_PREMUL);
        }
        ao = a_b;
    }
    if clamped {
        ao = clamp(ao, 0.0, 1.0);
        co = clamp(co, vec3<f32>(0.0), vec3<f32>(ao));
    }
    return vec4<f32>(co, ao);
}
