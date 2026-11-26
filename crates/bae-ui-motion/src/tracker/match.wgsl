struct Params {
    ref_center: vec2<i32>, origin: vec2<i32>,
    feature: vec2<i32>, grid: vec2<u32>,
    channel: u32, subpixel: u32, enhance: u32, radius: i32,
    ref_field:u32, cur_field:u32, _pad:vec2<u32>,
}
@group(0) @binding(0) var reference: texture_2d<f32>;
@group(0) @binding(1) var current: texture_2d<f32>;
@group(0) @binding(2) var<uniform> p: Params;
@group(0) @binding(3) var<storage,read_write> scores: array<f32>;
@group(0) @binding(4) var<storage,read_write> result: vec4<f32>;

// Reconstruct the missing scan lines from adjacent lines of the same field.
fn raw(tex:texture_2d<f32>, at:vec2<i32>, field:u32)->vec3<f32>{
    let lim=vec2<i32>(textureDimensions(tex))-1;
    let q=clamp(at,vec2<i32>(0),lim);
    if field==0u || u32(q.y%2)==field-1u || lim.y==0{return textureLoad(tex,q,0).rgb;}
    let y0=select(q.y-1,q.y+1,q.y==0);
    let y1=select(q.y+1,q.y-1,q.y==lim.y);
    return 0.5*(textureLoad(tex,vec2<i32>(q.x,y0),0).rgb+textureLoad(tex,vec2<i32>(q.x,y1),0).rgb);
}
fn pixel(tex:texture_2d<f32>, at:vec2<i32>,field:u32) -> vec3<f32> {
    var rgb=raw(tex,at,field);
    if p.enhance!=0u {
        let dx=vec2<i32>(p.radius,0);let dy=vec2<i32>(0,p.radius);
        let surround=raw(tex,at+dx,field)+raw(tex,at-dx,field)+raw(tex,at+dy,field)+raw(tex,at-dy,field);
        let blur=(rgb*4.0+surround)*0.125;
        rgb=select(2.0*rgb-blur,blur,p.enhance==1u);
    }
    if p.channel==1u {return vec3<f32>(dot(rgb,vec3<f32>(0.2126,0.7152,0.0722)));}
    if p.channel==2u {
        let hi=max(max(rgb.r,rgb.g),rgb.b);
        return vec3<f32>((hi-min(min(rgb.r,rgb.g),rgb.b))/max(hi,1e-7));
    }
    return rgb;
}
// One invocation per candidate translation. Ignore windows crossing an image edge.
@compute @workgroup_size(8,8)
fn correlate(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(id.xy>=p.grid){return;}
    let idx=id.y*p.grid.x+id.x;
    let a0=p.ref_center-p.feature/2;
    let b0=p.origin+vec2<i32>(id.xy)-p.feature/2;
    if any(min(a0,b0)<vec2<i32>(0)) || any(a0+p.feature>vec2<i32>(textureDimensions(reference))) ||
        any(b0+p.feature>vec2<i32>(textureDimensions(current))) {scores[idx]=-2;return;}
    var a=vec3<f32>(0); var b=vec3<f32>(0);
    var aa=0.0;var bb=0.0;var ab=0.0;
    for(var y=0;y<p.feature.y;y++){for(var x=0;x<p.feature.x;x++){
        let u=pixel(reference,a0+vec2<i32>(x,y),p.ref_field);let v=pixel(current,b0+vec2<i32>(x,y),p.cur_field);
        a+=u;b+=v;aa+=dot(u,u);bb+=dot(v,v);ab+=dot(u,v);
    }}
    let inv=1.0/f32(p.feature.x*p.feature.y);
    let va=aa-dot(a,a)*inv;let vb=bb-dot(b,b)*inv;
    if min(va,vb)<=0.00001 {scores[idx]=-2;return;}
    scores[idx]=clamp((ab-dot(a,b)*inv)*inverseSqrt(va*vb),-1.0,1.0);
}
fn refine(a:f32,b:f32,c:f32)->f32 {
    let denom=a-2*b+c;
    if min(a,c)< -1.0 || denom>= -1e-7{return 0;}
    return clamp(0.5*(a-c)/denom,-0.5,0.5);
}
var<workgroup> best:array<f32,64>;
var<workgroup> index:array<u32,64>;
@compute @workgroup_size(64)
fn peak(@builtin(local_invocation_index) lane:u32){
    var score= -3.0;var at=0u;
    for(var i=lane;i<p.grid.x*p.grid.y;i+=64u){
        if scores[i]>score {score=scores[i];at=i;}
    }
    best[lane]=score;index[lane]=at;
    for(var step=32u;step>0u;step/=2u){
        workgroupBarrier();
        if lane<step && (best[lane+step]>best[lane] || (best[lane+step]==best[lane] && index[lane+step]<index[lane])){
            best[lane]=best[lane+step];index[lane]=index[lane+step];
        }
    }
    if lane!=0u{return;}
    at=index[0];let x=at%p.grid.x;let y=at/p.grid.x;
    var pos=vec2<f32>(p.origin)+vec2<f32>(f32(x),f32(y));
    if p.subpixel!=0u && best[0]>= -1.0 {
        if x>0u && x+1u<p.grid.x {pos.x+=refine(scores[at-1u],best[0],scores[at+1u]);}
        if y>0u && y+1u<p.grid.y {pos.y+=refine(scores[at-p.grid.x],best[0],scores[at+p.grid.x]);}
    }
    result=vec4<f32>(pos,best[0],0);
}
