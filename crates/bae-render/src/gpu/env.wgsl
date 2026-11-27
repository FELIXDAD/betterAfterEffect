// Environment light images (AE's Advanced 3D environment light,
// exec/env.rs). The source layer's picture is equirectangular: u turns
// around the vertical axis (u = 0.5 looks along +z, into the comp), v runs
// from straight up (0) to straight down (1); AE's y points down.
//
// - fs_level0: the source resampled into level 0 of a mip chain, in linear
//   light (Rgba16Float).
// - fs_down: one level from the one above (2×2 box).
// - fs_irradiance: the diffuse light arriving from every direction around a
//   normal (cosine-weighted, divided by π), from a small level.
// - fs_sky: the picture drawn behind the layers, through the camera.

const PI: f32 = 3.14159265;

struct Params {
    // fs_level0: uv scale of the covered texels (xy), decode to linear (z),
    // source texels per level-0 texel (w).
    // fs_sky: camera ray rows (env space ← camera space) in m0..m2.
    a: vec4<f32>,
    m0: vec4<f32>,
    m1: vec4<f32>,
    m2: vec4<f32>,
    // fs_sky: comp centre in target texels (xy), comp pixels per texel ÷
    // zoom (z), orthographic (w).
    b: vec4<f32>,
    // fs_sky: opacity (x), mip level (y), display-encode (z).
    c: vec4<f32>,
}

@group(0) @binding(0) var<uniform> u: Params;
@group(0) @binding(1) var t_src: texture_2d<f32>;
@group(0) @binding(2) var s_src: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> VsOut {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: VsOut;
    o.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(p.x, 1.0 - p.y);
    return o;
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    return select(pow((x + 0.055) / 1.055, vec3<f32>(2.4)), x / 12.92, x <= vec3<f32>(0.04045));
}

fn to_display(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    return select(1.055 * pow(x, vec3<f32>(1.0 / 2.4)) - 0.055, x * 12.92, x <= vec3<f32>(0.0031308));
}

// Unit direction of an equirectangular uv.
fn direction(uv: vec2<f32>) -> vec3<f32> {
    let lon = (uv.x - 0.5) * 2.0 * PI;
    let lat = (uv.y - 0.5) * PI;
    let c = cos(lat);
    return vec3<f32>(c * sin(lon), sin(lat), c * cos(lon));
}

// Equirectangular uv of a direction.
fn equirect(d: vec3<f32>) -> vec2<f32> {
    let n = normalize(d);
    return vec2<f32>(0.5 + atan2(n.x, n.z) / (2.0 * PI), 0.5 + asin(clamp(n.y, -1.0, 1.0)) / PI);
}

@fragment
fn fs_level0(i: VsOut) -> @location(0) vec4<f32> {
    // Four taps per output texel: level 0 is at most half the source's size.
    // (Pooled textures can be larger than the image: stay inside it.)
    let texel = 1.0 / vec2<f32>(textureDimensions(t_src));
    let uv = i.uv * u.a.xy;
    let d = 0.25 * texel * u.a.w;
    let lo = 0.5 * texel;
    let hi = u.a.xy - 0.5 * texel;
    var c = 0.25 * (textureSampleLevel(t_src, s_src, clamp(uv - d, lo, hi), 0.0)
        + textureSampleLevel(t_src, s_src, clamp(uv + vec2<f32>(d.x, -d.y), lo, hi), 0.0)
        + textureSampleLevel(t_src, s_src, clamp(uv + vec2<f32>(-d.x, d.y), lo, hi), 0.0)
        + textureSampleLevel(t_src, s_src, clamp(uv + d, lo, hi), 0.0));
    if u.a.z > 0.5 {
        // Premultiplied display-encoded colour → linear light.
        let a = max(c.a, 1e-6);
        c = vec4<f32>(to_linear(c.rgb / a) * a, c.a);
    }
    return c;
}

@fragment
fn fs_down(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(pos.xy) * 2;
    return 0.25 * (textureLoad(t_src, p, 0) + textureLoad(t_src, p + vec2<i32>(1, 0), 0)
        + textureLoad(t_src, p + vec2<i32>(0, 1), 0) + textureLoad(t_src, p + vec2<i32>(1, 1), 0));
}

@fragment
fn fs_irradiance(i: VsOut) -> @location(0) vec4<f32> {
    let n = direction(i.uv);
    let size = vec2<i32>(textureDimensions(t_src));
    let du = 2.0 * PI / f32(size.x);
    let dv = PI / f32(size.y);
    var sum = vec3<f32>(0.0);
    for (var y = 0; y < size.y; y++) {
        let v = (f32(y) + 0.5) / f32(size.y);
        let lat = (v - 0.5) * PI;
        let solid = du * dv * cos(lat);
        for (var x = 0; x < size.x; x++) {
            let d = direction(vec2<f32>((f32(x) + 0.5) / f32(size.x), v));
            let w = max(dot(n, d), 0.0) * solid;
            if w > 0.0 {
                sum += textureLoad(t_src, vec2<i32>(x, y), 0).rgb * w;
            }
        }
    }
    return vec4<f32>(sum / PI, 1.0);
}

@fragment
fn fs_sky(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    var ray = vec3<f32>(0.0, 0.0, 1.0);
    if u.b.w < 0.5 {
        ray = vec3<f32>((pos.xy - u.b.xy) * u.b.z, 1.0);
    }
    let d = vec3<f32>(dot(u.m0.xyz, ray), dot(u.m1.xyz, ray), dot(u.m2.xyz, ray));
    var c = textureSampleLevel(t_src, s_src, equirect(d), u.c.y).rgb;
    if u.c.z > 0.5 {
        c = to_display(c);
    }
    return vec4<f32>(c * u.c.x, u.c.x);
}
