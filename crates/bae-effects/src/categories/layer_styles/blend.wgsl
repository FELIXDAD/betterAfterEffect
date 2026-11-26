// Most style blend operators share the generated-colour kernel. The remaining
// operators need absolute pixel coordinates, premultiplied colour or coverage.
fn style_random(p:vec2f, seed:u32)->f32 {
    let pixel=bitcast<vec2u>(vec2i(floor(p)));
    var h=seed ^ (pixel.x*0x9e3779b9u) ^ (pixel.y*0x85ebca6bu);
    h=(h^(h>>16u))*0x7feb352du;
    h=(h^(h>>15u))*0x846ca68bu;
    return f32((h^(h>>16u))>>8u)/16777216.;
}
fn style_blend(base:vec4f,color:vec3f,alpha:f32,mode:u32,p:vec2f)->vec4f {
    let a=saturate(alpha);
    let combined_alpha=a+(1.-a)*base.a;
    let rgb=fx_unpremult(base).rgb;
    var overlap=color;
    switch mode {
        case 31u,32u: {
            let seed=select(0u,u32(abs(u.time*u.fps)),mode==32u);
            return select(base,vec4f(color,1.),style_random(p,seed)<a);
        }
        case 33u: {overlap=select(rgb,color,dot(color,vec3f(1))<dot(rgb,vec3f(1)));}
        case 34u: {overlap=select(rgb,color,dot(color,vec3f(1))>dot(rgb,vec3f(1)));}
        case 35u: {overlap=rgb-color;}
        case 36u: {overlap=select(vec3f(0),rgb/max(color,vec3f(1e-6)),rgb>vec3f(0));}
        case 37u: {return base*a;}
        case 38u: {return base*a*dot(color,vec3f(.299,.587,.114));}
        case 39u: {return base*(1.-a);}
        case 40u: {return base*(1.-a*dot(color,vec3f(.299,.587,.114)));}
        case 41u: {let combined=min(1.,a+base.a);return vec4f(color*a+rgb*(combined-a),combined);}
        case 42u: {return vec4f(max(vec3f(0),combined_alpha-(base.a-base.rgb)/max(vec3f(1e-6),1.-a+color*a)),combined_alpha);}
        case 43u: {return vec4f(min(vec3f(combined_alpha),base.rgb/max(vec3f(1e-6),1.-color*a)),combined_alpha);}
        case 44u: {return vec4f(abs(base.rgb-color*a),combined_alpha);}
        default: {return generated_blend(base,color,a,mode);}
    }
    return vec4f((1.-a)*base.rgb+a*mix(color,overlap,base.a),combined_alpha);
}
