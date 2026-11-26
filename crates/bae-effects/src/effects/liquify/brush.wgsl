struct Brush {
    origin: vec2f, center: vec2f, drag: vec2f, source: vec2f,
    cell: f32, radius: f32, pressure: f32, jitter: f32,
    size: vec2u, tool: u32, mode: u32,
}
@group(0) @binding(0) var<uniform> b: Brush;
@group(0) @binding(1) var<storage, read> original: array<vec4f>;
@group(0) @binding(2) var<storage, read> previous: array<vec4f>;
@group(0) @binding(3) var<storage, read_write> next_mesh: array<vec4f>;

fn index_of(p: vec2u) -> u32 {
    let stride = ((b.size.x + 15u) / 16u) * 16u;
    return p.y * stride + p.x;
}

fn initial(p: vec2f) -> vec2f {
    let g = clamp((p-b.origin)/b.cell, vec2f(0.), vec2f(b.size-1u));
    let lo = vec2u(g);
    let hi = min(lo+1u, b.size-1u);
    let f = fract(g);
    return mix(mix(original[index_of(vec2u(lo.x,lo.y))].xy, original[index_of(vec2u(hi.x,lo.y))].xy, f.x),
               mix(original[index_of(vec2u(lo.x,hi.y))].xy, original[index_of(vec2u(hi.x,hi.y))].xy, f.x), f.y);
}
@compute @workgroup_size(8,8)
fn brush(@builtin(global_invocation_id) gid: vec3u) {
    if any(gid.xy >= b.size) { return; }
    let i = index_of(gid.xy);
    let old = previous[i];
    let q = b.origin + vec2f(gid.xy)*b.cell;
    let start = b.center-b.drag;
    let t = clamp(dot(q-start,b.drag)/max(dot(b.drag,b.drag),0.00001),0.,1.);
    let v = q-(start+t*b.drag);
    let d = length(v)/max(b.radius, b.cell*0.75);
    let f = pow(max(0.,1.-d*d),2.)*b.pressure*old.z;
    var o = old.xy;
    if f > 0. {
        switch b.tool {
            case 0u: { o -= b.drag*f; }
            case 1u: {
                let phase = q*(1.+b.jitter*8.)/max(1.,b.radius);
                o += vec2f(sin(phase.y+cos(phase.x)),cos(phase.x+sin(phase.y)))*b.radius*.09*f;
            }
            case 2u,3u: {
                let a = select(-0.14,0.14,b.tool==3u)*f;
                o += (cos(a)-1.)*v+sin(a)*vec2f(-v.y,v.x);
            }
            case 4u: { o += v*.12*f; }
            case 5u: { o -= v*.12*f; }
            case 6u: { o += vec2f(b.drag.y,-b.drag.x)*f; }
            case 7u: {
                let normal = vec2f(-b.drag.y,b.drag.x)/max(.001,length(b.drag));
                let reflected = q-2.*normal*dot(v,normal);
                o = mix(o,reflected-q+initial(reflected),f);
            }
            case 8u: { o = mix(o,initial(q-b.center+b.source),f); }
            case 9u: {
                var target_offset = vec2f(0.);
                if b.mode>0u { target_offset = initial(b.source); }
                if b.mode>=2u {
                    let dx = (initial(b.source+vec2f(b.cell,0.))-initial(b.source-vec2f(b.cell,0.)))/(2.*b.cell);
                    let dy = (initial(b.source+vec2f(0.,b.cell))-initial(b.source-vec2f(0.,b.cell)))/(2.*b.cell);
                    let rel = q-b.source;
                    if b.mode==2u {
                        let s = (dx.x+dy.y)*.5;
                        let r = (dx.y-dy.x)*.5;
                        target_offset += s*rel+r*vec2f(-rel.y,rel.x);
                    } else { target_offset += dx*rel.x+dy*rel.y; }
                }
                o = mix(o,target_offset,f*.35);
            }
            default: {}
        }
    }
    next_mesh[i] = vec4f(o,old.zw);
}
