struct Uniforms {
    rect: vec4i, kind: u32, mode: u32, original: f32, pad: f32, ripple: vec4f, material: vec4f, light: vec4f, position: vec4f, metal: vec4f, unused: vec4f
}

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var input_tex: texture_2d<f32>;
@group(0) @binding(2) var overlay: texture_2d<f32>;
@group(0) @binding(3) var output_tex: texture_storage_2d<WORKING_FORMAT, write>;
@compute @workgroup_size(8, 8)
fn composite(@builtin(global_invocation_id) tid: vec3u) {
    let xy = vec2i(tid.xy);
    if any(xy >= u.rect.zw) {
        return;
    }
    let src = xy + u.rect.xy;
    let bg = textureLoad(input_tex, src, 0);
    let fg = textureLoad(overlay, xy, 0);
    var result = fg;
    if u.kind==13u{
        let field=fg.r;
        let grad=vec2f(textureLoad(overlay,xy+vec2i(1,0),0).r-textureLoad(overlay,xy-vec2i(1,0),0).r,
                       textureLoad(overlay,xy+vec2i(0,1),0).r-textureLoad(overlay,xy-vec2i(0,1),0).r);
        let alpha=smoothstep(.22,.32,field)*u.metal.w;
        let n=normalize(vec3f(-grad*12.,1.));
        let direction=select(vec2f(sin(u.position.z),-cos(u.position.z)),normalize(u.position.xy-vec2f(src)+.0001),u.position.w==2.);
        let light=normalize(vec3f(direction,u.ripple.w));
        let reflection=textureLoad(input_tex,vec2i(clamp(vec2f(src)+n.xy*16.,vec2f(0.),u.metal.yz-1.)),0);
        let base=mix(vec3f(.7),reflection.rgb/max(.001,reflection.a),u.metal.x);
        let color=base*(u.material.x+u.material.y*max(0.,dot(n,light))*u.ripple.z*u.light.rgb)
            +u.material.z*u.ripple.z*u.light.rgb*pow(max(0.,dot(n,normalize(light+vec3f(0.,0.,1.)))),1./max(.002,u.material.w));
        textureStore(output_tex,xy,vec4f(color*alpha,alpha));return;
    }
    if u.kind == 5u {
        let slope = fg.rg * u.ripple.y;
        let refract = vec2i(clamp(vec2f(src) + slope * u.ripple.x, vec2f(0.), u.metal.yz-1.));
        let base = textureLoad(input_tex, refract, 0);
        let direction = select(vec2f(sin(u.position.z), -cos(u.position.z)), normalize(u.position.xy-vec2f(src) + .0001), u.position.w == 2.);
        let light = normalize(vec3f(direction, max(.001, u.ripple.w)));
        let n = normalize(vec3f(-slope, 1.));
        let diffuse = u.material.y * max(0., dot(n, light));
        let highlight = pow(max(0., dot(n, normalize(light + vec3f(0., 0., 1.)))), 1./max(.002, u.material.w));
        let spec = u.material.z * highlight * length(slope);
        result = vec4f(base.rgb * u.material.x + u.ripple.z * u.light.rgb * (base.rgb * diffuse + mix(vec3f(base.a), base.rgb, u.metal.x) * spec), base.a);
    } else if u.kind == 6u {
        result = mix(fg, bg, u.original);
    } else {
        let back = bg * u.original;
        result = fg + back * (1.-fg.a);
        switch u.mode {
            case 2u: {
                result = vec4f(fg.rgb + back.rgb-fg.rgb * back.rgb, result.a);
            }
            case 3u: {
                result = vec4f(fg.rgb + back.rgb, result.a);
            }
            case 4u: {
                result.a = 1.;
            }
            case 5u: {
                result = vec4f(max(fg.rgb, back.rgb), result.a);
            }
            default: {
            }
        }
    }
    if u.kind == 9u && u.ripple.w > 0. {
        let guides=array<vec4f,2>(u.ripple,u.material);
        for(var i=0u;i<2u;i++){
            let guide=guides[i];
            let distance_from_edge=abs(length(vec2f(xy)+.5-guide.xy)-guide.z);
            let line=select(0.,1.-smoothstep(.4,1.4,distance_from_edge),guide.z>0.);
            result=mix(result,vec4f(1.,.65,.22,1.),line);
        }
    }
    textureStore(output_tex, xy, result);
}
