// Affine, source-over RGBA images without masks or layer intersections.
// Compute the sampling footprint once per vertex and pass the small draw
// constants directly to the fragment stage on Apple's tile-based GPU.
struct ImageVertex {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>, // source texels
    @location(1) @interpolate(flat) rect: vec4<f32>,
    @location(2) @interpolate(flat) sampling: vec4<f32>, // inverse image size, edge scale
    @location(3) @interpolate(flat) steps: vec4<f32>, // inverse transform's x and y derivatives
    @location(4) @interpolate(flat) clip: vec4<f32>,
    @location(5) @interpolate(flat) opacity: f32,
    @location(6) @interpolate(flat) straight: u32,
    @location(7) @interpolate(flat) grid: u32,
}

@vertex
fn vs_image(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> ImageVertex {
    let d = draws[ii];
    let right = vi == 1u || vi == 4u || vi == 5u;
    let bottom = vi == 2u || vi == 3u || vi == 5u;
    let corner = vec3<f32>(select(d.quad.x, d.quad.z, right), select(d.quad.y, d.quad.w, bottom), 1.0);
    let t = vec2<f32>(dot(d.fwd_x.xyz, corner), dot(d.fwd_y.xyz, corner));
    let dx = vec2<f32>(d.inv_x.x, d.inv_y.x);
    let dy = vec2<f32>(d.inv_x.y, d.inv_y.y);
    var out: ImageVertex;
    out.pos = vec4<f32>(t.x / d.tgt.x * 2.0 - 1.0, 1.0 - t.y / d.tgt.y * 2.0, 0.5, 1.0);
    out.uv = corner.xy;
    out.rect = d.rect;
    out.sampling = vec4<f32>(d.src.xy, min(d.src.zw, vec2<f32>(1.0)));
    out.steps = vec4<f32>(dx, dy);
    out.opacity = d.tgt.z;
    out.grid = u32(clamp(ceil(max(length(dx), length(dy))), 1.0, 4.0));
    out.straight = d.mode.y & FLAG_STRAIGHT;
    out.clip = d.matte;
    return out;
}

@fragment
fn fs_image(in: ImageVertex) -> @location(0) vec4<f32> {
    if any(in.pos.xy < in.clip.xy) || any(in.pos.xy >= in.clip.zw) {
        return vec4<f32>(0.0);
    }
    let u = in.uv;
    let scale = in.sampling.zw;
    let dl = (u - in.rect.xy) * scale;
    let dr = (in.rect.zw - u) * scale;
    let coverage = clamp(min(dl, vec2<f32>(0.5)) + min(dr, vec2<f32>(0.5)), vec2<f32>(0.0), vec2<f32>(1.0));
    let lo = in.rect.xy + 0.5;
    let hi = max(in.rect.zw - 0.5, lo);
    let n = in.grid;
    var src: vec4<f32>;
    if n == 1u {
        src = textureSampleLevel(t_src, s_linear, clamp(u, lo, hi) * in.sampling.xy, 0.0);
    } else {
        let dx = in.steps.xy;
        let dy = in.steps.zw;
        for (var j = 0u; j < n; j++) {
            for (var i = 0u; i < n; i++) {
                let offset = (vec2<f32>(f32(i), f32(j)) + 0.5) / f32(n) - 0.5;
                let sample_pos = clamp(u + offset.x * dx + offset.y * dy, lo, hi);
                src += textureSampleLevel(t_src, s_linear, sample_pos * in.sampling.xy, 0.0);
            }
        }
        src /= f32(n * n);
    }
    if in.straight != 0u {
        src = vec4<f32>(src.rgb * src.a, src.a);
    }
    return src * (coverage.x * coverage.y * in.opacity);
}
