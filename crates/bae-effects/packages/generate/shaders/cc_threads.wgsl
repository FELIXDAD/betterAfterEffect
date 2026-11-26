fn cc_threads(c:vec4f,px:BaePixel)->vec4f {
    let angle=radians(params.direction); let dir=vec2f(cos(angle),sin(angle));
    let d=vec2f(px.pos)+0.5-params.center;
    let spacing=max(vec2f(1),vec2f(params.width,params.height)*frame.downsample);
    let q=vec2f(dot(d,dir),dot(d,vec2f(-dir.y,dir.x)))/spacing;
    let cell=vec2i(floor(q));
    let edge=abs(fract(q)-0.5)*2.0;
    let coverage=params.coverage*0.01;
    let mask=saturate((coverage-edge)*spacing*0.5+0.5);
    let count=max(1,params.overlaps);
    let cycle=2*count;
    let horizontal=((cell.x+cell.y)%cycle+cycle)%cycle<count;
    let top=select(mask.x,mask.y,horizontal);
    let bottom=select(mask.y,mask.x,horizontal);
    let crown=sqrt(max(vec2f(0),1.0-pow(edge/max(coverage,0.001),vec2f(2))));
    let ridge=select(crown.x,crown.y,horizontal);
    let grain=sin(6.2831853*select(q.x,q.y,horizontal)*18.0)*0.1;
    let shade=(0.4+0.6*ridge)*(1.0-params.shadowing*0.007*(1.0-top)*bottom);
    let a=c.a*max(mask.x,mask.y);
    return vec4f(bae_input_straight(c).rgb*shade*(1.0+grain*params.texture*0.01)*a,a);
}
