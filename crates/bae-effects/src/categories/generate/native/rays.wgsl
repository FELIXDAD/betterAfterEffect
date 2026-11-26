struct Rays {
    src: FxImage, map: FxImage, roi: vec4i, light: vec4f, ray: vec4f, tint: vec4f, flags: vec4f,
}
@group(0) @binding(0) var<uniform> u:Rays;
@group(0) @binding(1) var src:texture_2d<f32>;
@group(0) @binding(3) var out:texture_storage_2d<WORKING_FORMAT,write>;

@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3u) {
    let p=vec2i(id.xy)+u.roi.xy;
    if any(p>=u.roi.zw){return;}
    let pos=vec2f(p)+0.5;
    let base=fx_sample(src,u.src,pos,false);
    let offset=pos-u.light.xy;
    let dir=vec2f(cos(u.ray.y),sin(u.ray.y));
    let local=vec2f(dot(offset,dir),dot(offset,vec2f(-dir.y,dir.x)));
    let dist=select(length(local),max(abs(local.x),abs(local.y)),u.ray.z==2);
    let spot=u.ray.z<=2;
    var sum=vec4f(0);
    var weights=0.0;
    for(var i=0;i<32;i++){
        let t=f32(i)/31.0;
        let weight=select(1.0,1.0-t,u.ray.z==4);
        var scale=exp2(-u.ray.x*select(t,t-0.5,u.ray.z==5));
        if spot {scale=t*min(1.0,u.light.w/max(dist,0.001));}
        sum+=weight*fx_bilinear(src,u.src,u.light.xy+offset*scale,false);
        weights+=weight;
    }
    var light=sum/weights*u.light.z;
    if spot {light*=exp(-dist/max(0.001,u.light.w*(1.0+u.ray.x)));}
    if u.flags.x!=0 {light=vec4f(u.tint.rgb*dot(light.rgb,vec3f(0.2126,0.7152,0.0722)),light.a);}
    if u.flags.z==0 {light=saturate(light);}
    var rgb=light.rgb;
    switch u32(u.ray.w) {
        case 2u:{rgb+=base.rgb;}
        case 3u:{rgb=max(rgb,base.rgb);}
        case 4u:{rgb=rgb+base.rgb-rgb*base.rgb;}
        default:{}
    }
    var alpha=select(base.a,max(base.a,saturate(light.a)),u.flags.y!=0);
    if u.ray.w==1 {alpha=saturate(light.a);}
    if u.flags.w==0 {rgb=clamp(rgb,vec3f(0),vec3f(alpha));}
    textureStore(out,vec2i(id.xy),vec4f(rgb,alpha));
}
