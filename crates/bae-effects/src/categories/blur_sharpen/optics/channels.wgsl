struct Lane {
    source: vec4i,
    out: vec4i,
    factor: vec2i,
    radius: i32,
    padding: u32,
}
struct Params {
    lane: array<Lane,4>,
    roi: vec4i,
    source: vec4i,
    horizontal: u32,
    vertical: u32,
    repeat_edge: u32,
    floating: u32,
    native_scale: vec4f,
    weights: array<vec4f,19>,
}
@group(0) @binding(0) var<uniform> u: Params;
@group(0) @binding(1) var a: texture_2d<f32>;
@group(0) @binding(2) var b: texture_2d<f32>;
@group(0) @binding(3) var c: texture_2d<f32>;
@group(0) @binding(4) var d: texture_2d<f32>;
@group(0) @binding(5) var color_out: texture_storage_2d<WORKING_FORMAT,write>;
@group(0) @binding(6) var alpha_out: texture_storage_2d<WORKING_FORMAT,write>;

fn load_edge(tex: texture_2d<f32>, q: vec2i, rect: vec4i) -> vec4f {
    if u.repeat_edge != 0u {return textureLoad(tex,clamp(q,rect.xy,rect.zw-1)-rect.xy,0);}
    if any(q<rect.xy) || any(q>=rect.zw) {return vec4f(0.);}
    return textureLoad(tex,q-rect.xy,0);
}
fn hor(tex: texture_2d<f32>, id: vec2i, ch: u32) -> vec2f {
    let l = u.lane[ch];
    let halo = select(0,l.radius,u.vertical!=0u);
    if l.radius == 0 || any(id >= l.out.zw-l.out.xy+vec2i(0,2*halo)) { return vec2f(0.); }
    let p = id+l.out.xy-vec2i(0,halo);
    let r = select(0,l.radius,u.horizontal!=0u);
    let center = load_edge(tex,p,l.source);
    if r == 0 {return vec2f(center[ch],center.a);}
    var sum = vec2f(center[ch],center.a)*u.weights[0][ch];
    for (var i=1; i<=r; i++) {
        let dx=vec2i(i,0);
        let pair=load_edge(tex,p-dx,l.source)+load_edge(tex,p+dx,l.source);
        sum += vec2f(pair[ch],pair.a)*u.weights[i][ch];
    }
    return sum;
}
@compute @workgroup_size(8,8)
fn horizontal(@builtin(global_invocation_id) invocation: vec3u) {
    let p=vec2i(invocation.xy);
    if any(invocation.xy>=textureDimensions(color_out)){return;}
    let r=hor(a,p,0u); let g=hor(b,p,1u); let blue=hor(c,p,2u); let alpha=hor(d,p,3u);
    textureStore(color_out,p,vec4f(r.x,g.x,blue.x,alpha.x));
    textureStore(alpha_out,p,vec4f(r.y,g.y,blue.y,alpha.y));
}
@compute @workgroup_size(8,8)
fn vertical(@builtin(global_invocation_id) invocation: vec3u) {
    if any(invocation.xy>=textureDimensions(color_out)){return;}
    let p=vec2i(invocation.xy);
    var col=vec4f(0.); var cov=vec4f(0.);
    for(var ch=0u;ch<4u;ch++) {
        let l=u.lane[ch];
        if l.radius==0 || any(p>=l.out.zw-l.out.xy){continue;}
        let r=select(0,l.radius,u.vertical!=0u);
        for(var i=-r;i<=r;i++) {
            let q=p+vec2i(0,i+r);
            let w=select(1.,u.weights[abs(i)][ch],r!=0);
            col[ch] += w*textureLoad(a,q,0)[ch];
            cov[ch] += w*textureLoad(b,q,0)[ch];
        }
    }
    textureStore(color_out,p,col);
    textureStore(alpha_out,p,cov);
}
fn level_pair(q:vec2i,ch:u32)->vec2f {return vec2f(textureLoad(a,q,0)[ch],textureLoad(b,q,0)[ch]);}

fn result(p:vec2f,ch:u32)->vec2f {
    let l=u.lane[ch];
    let q=p/vec2f(l.factor)-0.5;
    let base=vec2i(floor(q)); let f=fract(q);
    if l.radius==0 {
        var pixel=load_edge(c,base,u.source);
        if any(f!=vec2f(0.)) {
            pixel=mix(mix(pixel,load_edge(c,base+vec2i(1,0),u.source),f.x),
                      mix(load_edge(c,base+vec2i(0,1),u.source),load_edge(c,base+vec2i(1,1),u.source),f.x),f.y);
        }
        return vec2f(pixel[ch],pixel.a);
    }
    let local=base-l.out.xy;
    let center=level_pair(local,ch);
    if all(f==vec2f(0.)){return center;}
    return mix(mix(center,level_pair(local+vec2i(1,0),ch),f.x),
               mix(level_pair(local+vec2i(0,1),ch),level_pair(local+vec2i(1,1),ch),f.x),f.y);
}
@compute @workgroup_size(8,8)
fn resolve(@builtin(global_invocation_id) invocation:vec3u) {
    if any(invocation.xy>=textureDimensions(color_out)){return;}
    let p=(vec2f(u.roi.xy)+vec2f(invocation.xy)+0.5)*u.native_scale.x;
    var rgb=vec3f(0.);
    for(var i=0u;i<3u;i++){
        let pair=result(p,i);
        if pair.y!=0. {rgb[i]=pair.x/pair.y;}
    }
    var alpha=result(p,3u).x;
    if u.floating==0u {rgb=saturate(rgb);alpha=saturate(alpha);}
    textureStore(color_out,invocation.xy,vec4f(rgb*alpha,alpha));
}
