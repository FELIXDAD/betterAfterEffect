// One workgroup samples the three discs in each image. Only the six means
// cross into the per-pixel correction pass; all intermediate data stays on GPU.
var<workgroup> sums:array<vec4f,64>;
@compute @workgroup_size(8,8)
fn measure(@builtin(local_invocation_index) lane:u32){
    for(var i=0u;i<6u;i++){
        let reference_frame=i>=3u;
        let center=u.values[2u+i%3u+select(0u,10u,reference_frame)].xy;
        let radius=max(0.5,0.005*select(value(5u),value(15u),reference_frame)*f32(u.layer.z-u.layer.x));
        let diameter=u32(ceil(2.0*radius))+1u;
        let n=min(diameter,65u);
        var sum=vec4f(0);
        for(var k=lane;k<n*n;k+=64u){
            let delta=vec2f(vec2u(k%n,k/n))/f32(n-1u)*2.0-1.0;
            if dot(delta,delta)>1.0 && n>2u{continue;}
            let q=clamp(center+delta*radius,vec2f(u.layer.xy)+0.5,vec2f(u.layer.zw)-0.5);
            let c=select(src(q),aux_a(q),reference_frame);
            sum+=vec4f(c.rgb*c.a,c.a);
        }
        sums[lane]=sum;workgroupBarrier();
        for(var step=32u;step>0u;step/=2u){
            if lane<step{sums[lane]+=sums[lane+step];}
            workgroupBarrier();
        }
        if lane==0u {textureStore(out_tex,vec2i(vec2u(i%3u,i/3u)),vec4f(sums[0].rgb/max(sums[0].a,1e-8),sums[0].a));}
        workgroupBarrier();
    }
}
fn curve(x:f32,b:f32,m:f32,w:f32,rb:f32,rm:f32,rw:f32,mode:u32)->f32{
    if mode==1u || abs(w-b)<1e-5{return x+rb-b;}
    let t=(x-b)/(w-b);let linear=mix(rb,rw,t);
    if mode!=3u{return linear;}
    let pivot=(m-b)/(w-b);
    if pivot<=1e-4 || pivot>=0.9999{return linear;}
    // Interpolate the pivot with a piecewise linear curve, including HDR extensions.
    if t<=pivot{return mix(rb,rm,t/pivot);}
    return mix(rm,rw,(t-pivot)/(1.0-pivot));
}
fn evaluate(p:vec2f)->vec4f{
    let c=src(p);let b=textureLoad(map_a,vec2i(0,0),0);let m=textureLoad(map_a,vec2i(1,0),0);
    let w=textureLoad(map_a,vec2i(2,0),0);let rb=textureLoad(map_a,vec2i(0,1),0);
    let rm=textureLoad(map_a,vec2i(1,1),0);let rw=textureLoad(map_a,vec2i(2,1),0);
    if min(b.a,rb.a)<=0.0{return output(c.rgb,c.a);}
    var mode=u32(value(1u));
    if min(w.a,rw.a)<=0.0{mode=1u;}
    if mode==3u && min(m.a,rm.a)<=0.0{mode=2u;}
    var rgb=c.rgb;for(var i=0u;i<3u;i++){rgb[i]=curve(c[i],b[i],m[i],w[i],rb[i],rm[i],rw[i],mode);}
    return output(rgb,c.a);
}
