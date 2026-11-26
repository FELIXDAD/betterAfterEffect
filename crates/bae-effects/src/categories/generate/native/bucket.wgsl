struct Bucket {
    src:FxImage, rect:vec4i, roi:vec4i, color:vec4f, selection:vec4f,
    selector:u32, stroke:u32, blend:u32, flags:u32, opacity:f32, stride:u32, pad:vec2u,
}
@group(0) @binding(0) var<uniform> u:Bucket;
@group(0) @binding(1) var image:texture_2d<f32>;
@group(0) @binding(2) var<storage,read_write> parent:array<atomic<u32>>;
@group(0) @binding(3) var<storage,read_write> previous:array<u32>;
@group(0) @binding(4) var<storage,read_write> next_seed:array<u32>;
@group(0) @binding(5) var out:texture_storage_2d<WORKING_FORMAT,write>;
const EMPTY:u32=0xffffffffu;

fn inside(q:vec2i)->bool{return all(q>=vec2i(0)) && all(q<u.rect.zw-u.rect.xy);}
fn index(q:vec2i)->u32{return u32(q.y*(u.rect.z-u.rect.x)+q.x);}
fn root(i:u32)->u32{
    var node=i;
    loop {
        let next=atomicLoad(&parent[node]);
        if next==node || next==EMPTY {return next;}
        let grandparent=atomicLoad(&parent[next]);
        atomicMin(&parent[node],grandparent);
        node=grandparent;
    }
    return EMPTY;
}
fn join(a:u32,b:u32){
    var left=root(a); var right=root(b);
    loop {
        if left==EMPTY || right==EMPTY || left==right {break;}
        let hi=max(left,right);let lo=min(left,right);
        let old=atomicMin(&parent[hi],lo);
        if old==hi {break;}
        left=root(old);right=root(lo);
    }
}
fn selected(q:vec2i)->bool{
    let seed=vec2i(floor(u.selection.xy))-u.rect.xy;
    if !inside(q) || !inside(seed){return false;}
    let component=root(index(q));return component!=EMPTY && component==root(index(seed));
}
@compute @workgroup_size(8,8)
fn classify(@builtin(global_invocation_id) id:vec3u){
    let q=vec2i(id.xy);if !inside(q){return;}
    let c=fx_sample(image,u.src,vec2f(q+u.rect.xy)+0.5,true);
    let seed=fx_sample(image,u.src,floor(u.selection.xy)+0.5,true);
    let diff=abs(c-seed);
    var error=max(max(diff.r,diff.g),max(diff.b,diff.a));
    switch u.selector {
        case 2u:{error=max(diff.r,max(diff.g,diff.b));}
        case 4u:{error=c.a;}
        case 5u:{error=1.0-c.a;}
        case 6u:{error=diff.a;}
        default:{}
    }
    atomicStore(&parent[index(q)],select(EMPTY,index(q),error<=u.selection.z));
}
@compute @workgroup_size(8,8)
fn connect(@builtin(global_invocation_id) id:vec3u){
    let q=vec2i(id.xy);if !inside(q){return;}
    let i=index(q);
    if atomicLoad(&parent[i])==EMPTY{return;}
    if q.x>0{join(i,i-1u);}
    if q.y>0{join(i,i-u32(u.rect.z-u.rect.x));}
}
@compute @workgroup_size(8,8)
fn boundary(@builtin(global_invocation_id) id:vec3u){
    let q=vec2i(id.xy);if !inside(q){return;}
    let v=selected(q);
    let edge=v && !(selected(q+vec2i(1,0)) && selected(q-vec2i(1,0)) &&
        selected(q+vec2i(0,1)) && selected(q-vec2i(0,1)));
    next_seed[index(q)]=select(EMPTY,index(q),edge);
}
fn seed_distance(q:vec2i,seed:u32)->f32{
    if seed==EMPTY{return 1e20;}
    let width=u32(u.rect.z-u.rect.x);
    return length(vec2f(q-vec2i(i32(seed%width),i32(seed/width))));
}
@compute @workgroup_size(8,8)
fn jump(@builtin(global_invocation_id) id:vec3u){
    let q=vec2i(id.xy);if !inside(q){return;}
    var best=previous[index(q)];var distance=seed_distance(q,best);
    for(var y=-1;y<=1;y++){for(var x=-1;x<=1;x++){
        let other=q+vec2i(x,y)*i32(u.stride);
        if !inside(other){continue;}
        let candidate=previous[index(other)];let d=seed_distance(q,candidate);
        if d<distance{distance=d;best=candidate;}
    }}
    next_seed[index(q)]=best;
}
@compute @workgroup_size(8,8)
fn paint(@builtin(global_invocation_id) id:vec3u){
    let p=vec2i(id.xy)+u.roi.xy;if any(p>=u.roi.zw){return;}
    let q=p-u.rect.xy;let member=selected(q);
    var coverage=f32(member);
    if (u.flags&8u)!=0u{
        let distance=seed_distance(q,previous[index(q)]);
        let signed=select(distance-0.5,-distance-0.5,member);
        let r=u.selection.w;
        switch u.stroke {
            case 2u:{coverage=1.0-smoothstep(-r,r,signed);}
            case 3u:{coverage=saturate(r-signed+0.5);}
            case 4u:{coverage=saturate(-r-signed+0.5);}
            case 5u:{coverage=saturate(r*0.5-abs(signed)+0.5);}
            default:{}
        }
    }
    if (u.flags&1u)!=0u{coverage=1.0-coverage;}
    if (u.flags&2u)!=0u{textureStore(out,vec2i(id.xy),vec4f(vec3f(coverage),1));return;}
    let src=fx_sample(image,u.src,vec2f(p)+0.5,false);
    let alpha=coverage*u.opacity*u.color.a;
    var result=generated_blend(src,u.color.rgb,alpha,select(u.blend,u.blend-1u,u.blend>3u));
    if u.blend==3u {result=src*alpha;}
    if (u.flags&4u)==0u {result=vec4f(clamp(result.rgb,vec3f(0),vec3f(saturate(result.a))),saturate(result.a));}
    textureStore(out,vec2i(id.xy),result);
}
