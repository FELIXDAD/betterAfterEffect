// Each workgroup follows one feature. Lost features start a new track in their tile.
@group(0) @binding(0) var previous: texture_2d<f32>;
@group(0) @binding(1) var current: texture_2d<f32>;
@group(0) @binding(2) var<storage,read_write> tracks: array<vec4f>;
struct Params { columns:u32, rows:u32, first:u32, radius:i32 }
@group(0) @binding(3) var<uniform> params:Params;
var<workgroup> peaks:array<vec4f,64>;
fn gray(tex:texture_2d<f32>, p:vec2i)->f32 {
    return dot(textureLoad(tex,p,0).rgb,vec3f(0.2126,0.7152,0.0722));
}
fn corner(p:vec2i)->f32 {
    var g=vec3f(0);
    for(var i=-2;i<=2;i+=2){for(var j=-2;j<=2;j+=2){
        let a=p+vec2i(i,j);
        let x=gray(current,a+vec2i(1,0))-gray(current,a-vec2i(1,0));
        let y=gray(current,a+vec2i(0,1))-gray(current,a-vec2i(0,1));
        g+=vec3f(x*x,x*y,y*y);
    }}
    return 0.5*(g.x+g.z-length(vec2f(g.x-g.z,2*g.y)));
}
fn ncc(a:vec2i,b:vec2i)->f32{
    let bounds=vec2i(textureDimensions(current));
    if any(b<vec2i(4)) || any(b>=bounds-vec2i(4)){return -2;}
    var sum=vec2f(0);var squares=vec2f(0);var product=0.;
    for(var j=-3;j<=3;j++){for(var i=-3;i<=3;i++){
        let off=vec2i(i,j);
        let pair=vec2f(gray(previous,a+off),gray(current,b+off));
        sum+=pair;squares+=pair*pair;product+=pair.x*pair.y;
    }}
    let energy=max(vec2f(0),squares-sum*sum/49.);
    return (product-sum.x*sum.y/49.)*inverseSqrt(max(1e-10,energy.x*energy.y));
}
fn reduce(lane:u32,peak:vec4f){
    peaks[lane]=peak;
    workgroupBarrier();
    for(var step=32u;step>0;step/=2u){
        if lane<step && peaks[lane+step].z>peaks[lane].z{peaks[lane]=peaks[lane+step];}
        workgroupBarrier();
    }
}
fn subpixel(l:f32,c:f32,r:f32)->f32{
    return clamp((l-r)/(2.*min(-1e-5,l-2.*c+r)),-0.5,0.5);
}
@compute @workgroup_size(64)
fn follow(@builtin(workgroup_id) tile:vec3u,@builtin(local_invocation_index) lane:u32){
    let index=tile.y*params.columns+tile.x;
    let size=textureDimensions(current);
    let old=tracks[index];
    var best=vec4f(0,0,-2,0);
    let a=vec2i(round(old.xy));
    let span=u32(params.radius*2+1);
    if params.first==0 && old.z>0 {
        for(var n=lane;n<span*span;n+=64){
            let b=a+vec2i(i32(n%span)-params.radius,i32(n/span)-params.radius);
            let score=ncc(a,b);
            if score>best.z{best=vec4f(vec2f(b),score,0);}
        }
    }
    reduce(lane,best);
    // All lanes execute each barrier, including when this track is still visible.
    let matched=peaks[0];
    workgroupBarrier();
    let lo=tile.xy*size/vec2u(params.columns,params.rows);
    let hi=(tile.xy+1u)*size/vec2u(params.columns,params.rows);
    best=vec4f(0,0,-2,1);
    if matched.z<0.82{
        let width=max(1u,hi.x-lo.x);
        let stride=max(1u,(max(hi.x-lo.x,hi.y-lo.y)+31u)/32u);
        let samples=(width+stride-1)/stride;
        for(var n=lane;n<samples*((hi.y-lo.y+stride-1)/stride);n+=64){
            let p=vec2i(lo+vec2u(n%samples,n/samples)*stride);
            if all(p>=vec2i(5)) && all(p<vec2i(size)-vec2i(5)){
                let score=corner(p);
                if score>best.z{best=vec4f(vec2f(p),score,1);}
            }
        }
    }
    reduce(lane,best);
    if lane==0{
        if matched.z>=0.82{
            let b=vec2i(matched.xy);
            let sub=vec2f(subpixel(ncc(a,b-vec2i(1,0)),matched.z,ncc(a,b+vec2i(1,0))),subpixel(ncc(a,b-vec2i(0,1)),matched.z,ncc(a,b+vec2i(0,1))));
            tracks[index]=vec4f(matched.xy+sub+(old.xy-round(old.xy)),matched.z,0);
        }else{
            let p=peaks[0];
            tracks[index]=vec4f(p.xy,select(-1.,p.z,p.z>0.002),1.);
        }
    }
}
