fn bae_unpremultiply(c:vec4f)->vec4f { return fx_unpremult(c); }
fn evaluate(p:vec2f)->vec4f {
    let base=fx_sample(input_tex,u.images[0],p,false);
    let color=textureLoad(map_a,vec2i(0),0).rgb;
    let stencil=enabled(3u);
    let amount=value(4u)*0.01;
    let mode=u32(value(5u));
    if mode==1u {
        if stencil { return mix(base,vec4f(color*base.a,base.a),amount); }
        return mix(base,vec4f(color,1),amount);
    }
    // The shared blend table also contains a leading Copy slot.
    if stencil {
        let c=generated_blend(vec4f(fx_unpremult(base).rgb,1),color,amount,mode+1u);
        return output(c.rgb,base.a);
    }
    return generated_blend(base,color,amount,mode+1u);
}
