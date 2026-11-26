// Gather source circles of confusion. Each tap contributes according to its own
// depth and aperture area, so an out-of-focus foreground can cover a sharp pixel.
fn coc(p:vec2f)->f32 {
    if !has(0u){return value(1)*u.scale;}
    let bounds=vec2f(u.layer.zw-u.layer.xy);let size=vec2f(u.images[1].size)/u.images[1].scale;
    let xy=p-vec2f(u.layer.xy);
    let q=u.images[1].origin+select(xy-0.5*(bounds-size),xy*size/max(bounds,vec2f(1.)),value(12)==2.);
    let map=aux_a(q);var depth=fx_luma(map.rgb);
    if value(11)>1.{depth=map[u32(value(11))-2u];}
    if enabled(14){depth=1.-depth;}
    return value(1)*u.scale*abs(depth-value(13));
}
fn light(c:vec4f)->vec4f {
    var rgb=c.rgb;
    if enabled(22){rgb=sign(rgb)*pow(abs(rgb),vec3f(2.2));}
    let lum=fx_luma(rgb);
    rgb+=max(0.,lum-value(18))*value(17)*0.05*mix(vec3f(1.),rgb/max(lum,0.001),value(19)*0.01);
    return vec4f(rgb*c.a,c.a);
}
fn evaluate(p:vec2f)->vec4f {
    let c=src(p);
    let radius=value(1)*u.scale;
    if radius<=0.001{return output(c.rgb,c.a);}
    let sides=value(3)+2.;let sector=6.2831853/sides;
    let rotation=radians(value(6));
    let cs=cos(rotation);let sn=sin(rotation);
    let taps=i32(clamp(ceil(radius*2.),16.,80.));
    let aspect=value(5);let roundness=value(4)*0.01;
    var sum=light(c);var weight=1.;
    // Equal-area golden-angle aperture samples, with bounded work for large radii.
    for(var i=0;i<taps;i++){
        let theta=f32(i)*2.399963;let radial=sqrt((f32(i)+0.5)/f32(taps));
        let polygon=cos(sector*0.5)/cos((fract(theta/sector+0.5)-0.5)*sector);
        let r=radial*mix(polygon,1.,roundness);
        let local=vec2f(aspect*cos(theta),sin(theta))*radius*r;
        var q=p+vec2f(cs*local.x-sn*local.y,sn*local.x+cs*local.y);
        if enabled(21){q=clamp(q,vec2f(u.layer.xy)+0.5,vec2f(u.layer.zw)-0.5);}
        let rc=coc(q);
        let cover=saturate(rc-radius*radial+0.5);
        let w=cover*radius*radius/max(1.,rc*rc)*mix(1.,1.+0.25*radial*radial,value(7)*0.01);
        sum+=light(src(q))*w;weight+=w;
    }
    let result=sum/weight;
    var rgb=result.rgb/max(result.a,1e-6);
    if enabled(22){rgb=sign(rgb)*pow(abs(rgb),vec3f(1./2.2));}
    return output(rgb,result.a);
}
