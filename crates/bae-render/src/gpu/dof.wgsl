// Depth of field of one 3D layer (graph::DepthBlurNode). The layer, drawn
// alone, is blurred per pixel by the circle of confusion of its plane: the
// plane's 1 / depth is affine on screen, and the blur's diameter at depth z is
// coc · |1 − focus / z| (a thin lens of AE's Aperture focused at Focus
// Distance, seen at the camera's Zoom). The blur gathers the layer over the
// camera's iris (a polygon of AE's blade count, rounded, stretched and turned)
// with brighter highlights and rim. Large blurs gather a prefiltered
// reduced copy (`fs_down`) and are resolved against the sharp layer
// (`fs_resolve`) so in-focus parts stay exact.

struct Img {
    // Covered region in space pixels (x0, y0, x1, y1).
    rect: vec4<f32>,
    // Space pixels per texel, 1 / texture width, 1 / texture height.
    info: vec4<f32>,
}

struct Params {
    // Output texel (0, 0)'s corner in space pixels, space pixels per texel,
    // reduction of the gathered copy (1: none).
    origin: vec2<f32>,
    scale: f32,
    reduction: f32,
    // 1 / depth = plane.x · x + plane.y · y + plane.z; plane.w: focus depth.
    plane: vec4<f32>,
    // Blur diameter far away and the largest radius (space pixels), blades
    // (4 for Fast Rectangle), rotation (radians).
    lens: vec4<f32>,
    // Roundness 0–1, aspect ratio, diffraction fringe 0–5.
    iris: vec4<f32>,
    // Highlight gain, threshold 0–1, saturation 0–1.
    light: vec4<f32>,
    src: Img,
    low: Img,
}

@group(0) @binding(0) var<uniform> u: Params;
@group(0) @binding(1) var t_src: texture_2d<f32>;
@group(0) @binding(2) var t_low: texture_2d<f32>;
@group(0) @binding(3) var s_linear: sampler;

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    // One triangle covering the viewport.
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

// Premultiplied colour of `t` at space point `q` (transparent outside it).
fn fetch(t: texture_2d<f32>, img: Img, q: vec2<f32>) -> vec4<f32> {
    if any(q < img.rect.xy) || any(q > img.rect.zw) {
        return vec4<f32>(0.0);
    }
    let uv = (q - img.rect.xy) / img.info.x * img.info.yz;
    return textureSampleLevel(t, s_linear, uv, 0.0);
}

// Blur radius in space pixels at space point `q`.
fn radius(q: vec2<f32>) -> f32 {
    let inv_depth = dot(u.plane.xy, q) + u.plane.z;
    if inv_depth <= 0.0 {
        return u.lens.y;
    }
    return min(0.5 * u.lens.x * abs(1.0 - u.plane.w * inv_depth), u.lens.y);
}

// Highlights above the threshold get brighter (gain) and keep or lose their
// colour (saturation), as AE's camera Highlight options.
fn highlight(c: vec4<f32>) -> vec4<f32> {
    if u.light.x <= 0.0 {
        return c;
    }
    let luma = dot(c.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let over = max(luma - u.light.y * c.a, 0.0);
    let tint = mix(vec3<f32>(1.0), c.rgb / max(luma, 1e-4), u.light.z);
    return vec4<f32>(c.rgb + over * u.light.x * tint, c.a);
}

// The iris offset of tap `i` of `n` (unit radius): a golden-angle spiral over
// the polygon, rounded, stretched by the aspect ratio and turned.
fn iris_tap(i: u32, n: u32) -> vec3<f32> {
    let angle = f32(i) * 2.3999632;
    let radial = sqrt((f32(i) + 0.5) / f32(n));
    let sector = 6.2831853 / u.lens.z;
    let edge = cos(0.5 * sector) / cos((fract(angle / sector + 0.5) - 0.5) * sector);
    var p = vec2<f32>(cos(angle), sin(angle)) * radial * mix(edge, 1.0, u.iris.x);
    p.x *= u.iris.y;
    let s = sin(u.lens.w);
    let c = cos(u.lens.w);
    return vec3<f32>(c * p.x - s * p.y, s * p.x + c * p.y, radial);
}

fn gather(t: texture_2d<f32>, img: Img, q: vec2<f32>, r: f32) -> vec4<f32> {
    let center = highlight(fetch(t, img, q));
    // Taps by the radius in the gathered image's texels.
    let texels = r / img.info.x;
    if texels < 0.5 {
        return center;
    }
    let n = u32(clamp(ceil(texels * 2.0), 12.0, 64.0));
    var sum = center;
    var norm = 1.0;
    for (var i = 0u; i < n; i++) {
        let tap = iris_tap(i, n);
        // A brighter rim (Diffraction Fringe).
        let w = 1.0 + u.iris.z * tap.z * tap.z;
        sum += highlight(fetch(t, img, q + tap.xy * r)) * w;
        norm += w;
    }
    return sum / norm;
}

// Prefiltered reduced copy: the box of source texels each output texel covers.
@fragment
fn fs_down(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let q = u.origin + pos.xy * u.scale;
    let d = 0.25 * u.scale;
    return 0.25 * (fetch(t_src, u.src, q + vec2<f32>(-d, -d)) + fetch(t_src, u.src, q + vec2<f32>(d, -d))
        + fetch(t_src, u.src, q + vec2<f32>(-d, d)) + fetch(t_src, u.src, q + vec2<f32>(d, d)));
}

// The blur at the output's resolution, gathering `t_src`.
@fragment
fn fs_gather(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let q = u.origin + pos.xy * u.scale;
    return gather(t_src, u.src, q, radius(q));
}

// Full resolution: the sharp layer where the blur is under a reduced texel,
// the gathered reduced copy where it is larger.
@fragment
fn fs_resolve(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let q = u.origin + pos.xy * u.scale;
    let r = radius(q);
    let sharp = fetch(t_src, u.src, q);
    let blurred = fetch(t_low, u.low, q);
    let k = clamp((r - 0.5 * u.scale) / max(u.reduction * u.scale, 1e-4), 0.0, 1.0);
    return mix(sharp, blurred, k);
}
