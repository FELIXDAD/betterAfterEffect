fn complex_mul(a:vec2f,b:vec2f)->vec2f { return vec2f(a.x*b.x-a.y*b.y,a.x*b.y+a.y*b.x); }
fn complex_power(z:vec2f) -> vec2f {
    let sq=complex_mul(z,z);
    if params.equation==1u { return sq; }
    if params.equation==2u { return complex_mul(sq,z); }
    let fourth=complex_mul(sq,sq);
    if params.equation==3u { return complex_mul(fourth,z); }
    return complex_mul(complex_mul(fourth,sq),z);
}
fn fractal_sample(uv:vec2f,julia:bool,inverse:bool)->vec4f {
    let size=vec2f(frame.layer_bounds.max-frame.layer_bounds.min);
    let c0=vec2f(params.mandelbrot_x_real,params.mandelbrot_y_imaginary);
    let center=select(c0,vec2f(params.julia_x_real,params.julia_y_imaginary),julia);
    let zoom=select(params.mandelbrot_magnification,params.julia_magnification,julia);
    var q=(uv-0.5)*vec2f(size.x,-size.y)*3.0/max(1.0,size.y)*exp2(-zoom)+center;
    if inverse { q=vec2f(q.x,-q.y)/max(1e-30,dot(q,q))+vec2f(params.offset_after_inversion_x_real,params.offset_after_inversion_y_imaginary); }
    let c=select(q,c0,julia);
    var z=select(vec2f(0),q,julia);
    let limit=select(params.mandelbrot_escape_limit,params.julia_escape_limit,julia);
    var i=0i;
    var interior=false;
    if !julia && params.equation==1u {
        let x=c.x-0.25; let r=x*x+c.y*c.y;
        interior=r*(r+x)<=0.25*c.y*c.y || dot(c+vec2f(1,0),c+vec2f(1,0))<0.0625;
    }
    var previous=z; var interval=8i;
    while !interior && i<limit && dot(z,z)<=4.0 {
        z=complex_power(z)+c;
        i++;
        if all(previous==z) { interior=true; }
        if i==interval { previous=z; interval*=2; }
    }
    if interior || i==limit {
        return select(vec4f(0,0,0,1),vec4f(0),params.transparency!=0u);
    }
    let phase=fract(f32(i)/f32(params.cycle_steps)+params.cycle_offset/360.0);
    let hue=params.hue/360.0;
    let wheel=clamp(abs(fract(phase+hue+vec3f(0.0,0.667,0.333))*6.0-3.0)-1.0,vec3f(0),vec3f(1));
    var rgb=wheel;
    switch params.palette {
        case 1u: { rgb=(0.5+0.5*cos(6.2831853*(hue+vec3f(0,0.3333,0.6667))))*phase; }
        case 3u: { rgb=vec3f(step(0.5,phase)); }
        case 4u: { rgb=0.5+0.5*cos(6.2831853*(hue+vec3f(0,0.3333,0.6667))); }
        case 5u: { rgb=0.5+0.5*cos(atan2(z.y,z.x)+6.2831853*(hue+vec3f(0,0.3333,0.6667))); }
        case 6u: { rgb=vec3f(phase); }
        case 7u: { let apple=array<vec3f,6>(vec3f(0),vec3f(0.65,0,0.85),vec3f(0.1,0.85,0),vec3f(0,0.45,1),vec3f(1,0.45,0),vec3f(1)); rgb=apple[min(5u,u32(phase*6.0))]; }
        default: {}
    }
    return vec4f(rgb,1);
}
fn fractal_color(uv:vec2f)->vec4f {
    let mode=params.set_choice;
    if mode>=5u { return fractal_sample(uv,true,mode==6u); }
    let mandel=fractal_sample(uv,false,mode==2u || mode==4u);
    if (mode==3u || mode==4u) && all(mandel.rgb==vec3f(0)) { return fractal_sample(uv,true,false); }
    return mandel;
}
fn fractal(c:vec4f,px:BaePixel)->vec4f {
    let size=max(vec2f(1),vec2f(frame.layer_bounds.max-frame.layer_bounds.min));
    let high_quality = (frame.flags & BAE_FRAME_HIGH_QUALITY) != 0u;
    let highlight = params.edge_highlight_force_lq != 0u;
    var oversample = high_quality && params.oversample_method == 2u && !highlight;
    var color = vec4f(0);
    if !oversample {
        color = fractal_color(px.uv);
        if high_quality || highlight {
            let neighbor = fractal_color(px.uv + 0.5 / size);
            let edge = any(abs(neighbor - color) > vec4f(0.005));
            if highlight {
                color = select(color, vec4f(1), edge);
            } else {
                oversample = edge;
            }
        }
    }
    if oversample {
        let n = params.oversample_factor;
        color = vec4f(0);
        for (var y = 0i; y < n; y++) {
            for (var x = 0i; x < n; x++) {
                let offset = (vec2f(f32(x), f32(y)) + 0.5) / f32(n) - 0.5;
                color += fractal_color(px.uv + offset / size);
            }
        }
        color /= f32(n * n);
    }
    if params.overlay!=0u { return color+c*(1.0-color.a); }
    return color;
}
