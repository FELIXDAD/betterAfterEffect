// 3D model layers (graph::MeshNode): triangles through the comp's camera with
// a depth buffer (reversed: 1 near, 0 far), shaded with the metallic-roughness
// model (GGX) under the comp's lights. Lights follow the Classic 3D records of
// light.rs (kind, falloff, cone); a diffuse of 100 % matches Lambert. An
// environment light with a picture (exec/env.rs) adds image-based light:
// diffuse from its irradiance map, reflections from its mip chain (rougher
// surfaces read smaller levels). Without lights the model is lit from the
// camera so its shape reads.

struct Light {
    // Position (comp space) and kind (0 ambient, 1 parallel, 2 point, 3 spot).
    p: vec4<f32>,
    // Direction the light points and falloff (0 none, 1 smooth, 2 inverse square).
    d: vec4<f32>,
    // Colour × intensity and radius.
    c: vec4<f32>,
    // cos outer, cos inner, falloff distance.
    k: vec4<f32>,
}

struct Frame {
    // Camera space → clip space of the target.
    clip: mat4x4<f32>,
    // Comp space → camera space.
    view: mat4x4<f32>,
    // Model space → comp space, and its normal matrix.
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
    // Eye (comp space) and light count.
    eye: vec4<f32>,
    // x: display-encode the shading (a non-linear working space).
    out: vec4<f32>,
    // Comp directions → environment picture directions.
    env_rot: mat4x4<f32>,
    // Environment light colour × intensity and its level count (0: none).
    env: vec4<f32>,
    lights: array<Light, 8>,
}

struct Material {
    // Linear RGBA.
    base: vec4<f32>,
    // Metallic, roughness, textured (1), unused.
    pbr: vec4<f32>,
    // Linear emissive colour.
    emissive: vec4<f32>,
}

@group(0) @binding(0) var<uniform> f: Frame;
@group(1) @binding(0) var<uniform> m: Material;
@group(1) @binding(1) var t_base: texture_2d<f32>;
@group(1) @binding(2) var s_base: sampler;
@group(2) @binding(0) var t_env: texture_2d<f32>;
@group(2) @binding(1) var t_irr: texture_2d<f32>;
@group(2) @binding(2) var s_env: sampler;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

@vertex
fn vs_mesh(v: VsIn) -> VsOut {
    let world = f.model * vec4<f32>(v.pos, 1.0);
    var o: VsOut;
    o.clip = f.clip * (f.view * world);
    o.world = world.xyz;
    o.normal = (f.normal * vec4<f32>(v.normal, 0.0)).xyz;
    o.uv = v.uv;
    return o;
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

fn to_display(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    return select(1.055 * pow(x, vec3<f32>(1.0 / 2.4)) - 0.055, x * 12.92, x <= vec3<f32>(0.0031308));
}

fn smooth01(e0: f32, e1: f32, x: f32) -> f32 {
    if e0 == e1 { return select(0.0, 1.0, x >= e0); }
    let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

const PI: f32 = 3.14159265;

// Equirectangular uv of a direction (see env.wgsl).
fn equirect(d: vec3<f32>) -> vec2<f32> {
    let n = normalize(d);
    return vec2<f32>(0.5 + atan2(n.x, n.z) / (2.0 * PI), 0.5 + asin(clamp(n.y, -1.0, 1.0)) / PI);
}

// Split-sum environment BRDF, analytic fit (Karis 2014): the share of the
// reflected picture that leaves toward the eye.
fn env_brdf(f0: vec3<f32>, rough: f32, nv: f32) -> vec3<f32> {
    let r = rough * vec4<f32>(-1.0, -0.0275, -0.572, 0.022) + vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let a004 = min(r.x * r.x, exp2(-9.28 * nv)) * r.x + r.y;
    let ab = vec2<f32>(-1.04, 1.04) * a004 + r.zw;
    return f0 * ab.x + ab.y;
}

@fragment
fn fs_mesh(i: VsOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    var base = m.base;
    if m.pbr.z > 0.5 {
        let t = textureSample(t_base, s_base, i.uv);
        base = base * vec4<f32>(to_linear(t.rgb), t.a);
    }
    var n = normalize(i.normal);
    if !front { n = -n; }
    let v = normalize(f.eye.xyz - i.world);
    let metallic = clamp(m.pbr.x, 0.0, 1.0);
    let rough = clamp(m.pbr.y, 0.04, 1.0);
    let a2 = rough * rough * rough * rough;
    let f0 = mix(vec3<f32>(0.04), base.rgb, metallic);
    let nv = max(dot(n, v), 1e-4);
    let kg = (rough + 1.0) * (rough + 1.0) / 8.0;
    var color = m.emissive.rgb;
    let count = u32(f.eye.w);
    let levels = f.env.w;
    if count == 0u && levels == 0.0 {
        color += base.rgb * (0.25 + 0.75 * nv);
    }
    if levels > 0.0 {
        let irr = textureSampleLevel(t_irr, s_env, equirect((f.env_rot * vec4<f32>(n, 0.0)).xyz), 0.0).rgb;
        let r = reflect(-v, n);
        // The GGX lobe's width in level-0 texels picks the level.
        let width = f32(textureDimensions(t_env).x);
        let lod = clamp(log2(max(rough * rough * width / PI, 1.0)), 0.0, levels - 1.0);
        let pre = textureSampleLevel(t_env, s_env, equirect((f.env_rot * vec4<f32>(r, 0.0)).xyz), lod).rgb;
        let fe = env_brdf(f0, rough, nv);
        color += ((1.0 - fe) * (1.0 - metallic) * base.rgb * irr + fe * pre) * f.env.rgb;
    }
    for (var li = 0u; li < count; li++) {
        let l = f.lights[li];
        let kind = u32(l.p.w);
        if kind == 0u {
            color += base.rgb * (1.0 - metallic) * l.c.rgb;
            continue;
        }
        var to_light: vec3<f32>;
        var dist: f32;
        if kind == 1u {
            to_light = -l.d.xyz;
            dist = dot(i.world - l.p.xyz, l.d.xyz);
        } else {
            let t = l.p.xyz - i.world;
            dist = length(t);
            to_light = t / max(dist, 1e-6);
        }
        let nl = dot(n, to_light);
        if nl <= 0.0 { continue; }
        var atten = 1.0;
        let falloff = u32(l.d.w);
        if falloff == 1u {
            atten = 1.0 - smooth01(l.c.w, l.c.w + l.k.z, dist);
        } else if falloff == 2u && dist > l.c.w {
            atten = (l.c.w / dist) * (l.c.w / dist);
        }
        if kind == 3u {
            atten *= smooth01(l.k.x, l.k.y, dot(l.d.xyz, -to_light));
        }
        let h = normalize(to_light + v);
        let nh = max(dot(n, h), 0.0);
        let vh = max(dot(v, h), 0.0);
        let dd = nh * nh * (a2 - 1.0) + 1.0;
        let d = a2 / (PI * dd * dd);
        let g = nv / (nv * (1.0 - kg) + kg) * nl / (nl * (1.0 - kg) + kg);
        let fr = f0 + (1.0 - f0) * pow(1.0 - vh, 5.0);
        let spec = d * g * fr / (4.0 * nv * nl);
        let kd = (1.0 - fr) * (1.0 - metallic);
        color += (kd * base.rgb + spec * PI) * l.c.rgb * atten * nl;
    }
    if f.out.x > 0.5 {
        color = to_display(color);
    }
    return vec4<f32>(color * base.a, base.a);
}
