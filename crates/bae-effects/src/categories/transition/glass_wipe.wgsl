fn gradient_height(p:vec2f)->f32 {
    if !has(1u) { return 0.5; }
    return fx_luma(aux_b(centered(p,u.images[2])).rgb);
}
fn glass(p:vec2f,t:f32)->vec4f {
    if t<=0.0 { return original(p); }
    if t>=1.0 { return reveal(p); }
    let d=max(1.0,value(4u)*0.15*u.scale);
    let left=gradient_height(p-vec2f(d,0)); let right=gradient_height(p+vec2f(d,0));
    let top=gradient_height(p-vec2f(0,d)); let bottom=gradient_height(p+vec2f(0,d));
    let h=(4.0*gradient_height(p)+left+right+top+bottom)*0.125;
    let width=0.04+value(4u)*0.004;
    let k=smoothstep(h-width,h+width,t*(1.0+2.0*width)-width);
    let offset=vec2f(right-left,bottom-top)*value(5u)*u.scale;
    return mix(original(p+offset*k),reveal(p-offset*(1.0-k)),k);
}
fn evaluate(p:vec2f)->vec4f { return glass(p,value(1u)*0.01); }
