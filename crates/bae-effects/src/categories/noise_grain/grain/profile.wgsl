fn auto_point(i:u32)->vec2f {
    let uv=fract((f32(i)+0.5)*vec2f(0.618034,0.414214));
    return mix(vec2f(u.layer.xy),vec2f(u.layer.zw),0.1+0.8*uv);
}
// One thread per high-pass sample. Each 4x4 patch gets a row of 16 samples.
// Matching can request a second set of rows for the current layer's noise.
fn measure(p:vec2f, compensation:bool)->vec3f {
    if compensation || !has(0u) {return fx_sample(input_tex,u.images[0],p,true).rgb;}
    let uv=(p-vec2f(u.layer.xy))/max(vec2f(u.layer.zw-u.layer.xy),vec2f(1.));
    let q=u.images[1].origin+clamp(uv*vec2f(u.images[1].size),vec2f(0.5),vec2f(u.images[1].size)-0.5)/u.images[1].scale;
    return fx_sample(map_a,u.images[1],q,true).rgb;
}
fn evaluate(p:vec2f)->vec4f {
    let xy=vec2u(p);
    let matching=u.index==1u;
    let count=u32(value(select(3u,10u,matching)));
    let compensation=xy.y>=count;
    let i=xy.y%count;
    var center=auto_point(i);
    if value(select(21u,18u,matching))==2. {
        let first=select(30u,select(50u,70u,compensation),matching);
        center=u.values[first+i].xy;
    }
    let size=value(select(4u,11u,matching))*u.scale;
    let q=center+((vec2f(f32(xy.x%4u),f32(xy.x/4u))+0.5)/4.-0.5)*size;
    let mean=0.25*(measure(q+vec2f(1.,0.),compensation)+measure(q-vec2f(1.,0.),compensation)
                  +measure(q+vec2f(0.,1.),compensation)+measure(q-vec2f(0.,1.),compensation));
    let residual=measure(q,compensation)-mean;
    return vec4f(residual*residual,1.);
}
