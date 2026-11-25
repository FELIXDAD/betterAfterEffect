struct Wave {geom:vec4f,color:vec4f,stroke:vec4f,pad:vec4f}
struct Waves {src:FxImage,map:FxImage,roi:vec4i,shape:vec4f,polygon:vec4f,contour:vec4f,threshold:vec4f,layer:vec4f,waves:array<Wave,128>}
@group(0) @binding(0) var<uniform> u:Waves;
@group(0) @binding(2) var map:texture_2d<f32>;
@group(0) @binding(3) var out:texture_storage_2d<WORKING_FORMAT,write>;

fn contour_value(p:vec2f)->f32 {
    let c=fx_bilinear(map,u.map,p,true);
    let hi=max(c.r,max(c.g,c.b)); let lo=min(c.r,min(c.g,c.b));
    switch u32(u.contour.z) {
        case 2u:{return c.r;} case 3u:{return c.g;} case 4u:{return c.b;} case 5u:{return c.a;}
        case 6u:{return fx_hsl(c.rgb).x;} case 7u:{return (hi+lo)*0.5;}
        case 8u:{return (hi-lo)/max(hi,0.0001);} case 9u:{return hi;}
        default:{return dot(c.rgb,vec3f(0.2126,0.7152,0.0722));}
    }
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3u) {
    let p=u.roi.xy+vec2i(id.xy);
    if any(p>=u.roi.zw){return;}
    let pos=vec2f(p)+0.5;
    var result=vec4f(0);
    for(var i=0u;i<u32(u.shape.x);i++){
        let w=u.waves[i]; let rel=pos-w.geom.xy;
        let theta=atan2(rel.y,rel.x)-w.geom.w;
        let sector=6.2831853/u.shape.z;
        let side=cos((fract(theta/sector+0.5)-0.5)*sector)/cos(sector*0.5);
        let radius=w.geom.z*(1.0+u.polygon.z*(0.5+0.5*cos(theta*u.shape.z)));
        var d=length(rel)*mix(side,1.0,saturate(u.polygon.x*(0.5+u.polygon.y*0.5)))-radius;
        if u.shape.y!=1.0 {
            if u.shape.y==3.0 && u.layer.z==0.0 {continue;}
            let scale=max(0.01,w.geom.z/max(min(u.layer.x,u.layer.y)*0.5,1.0));
            let q=u.contour.xy+vec2f(cos(theta),sin(theta))*length(rel)/scale;
            let r=max(1.0,u.threshold.x);
            let dxy=vec2f(r,0);
            let samples=vec4f(contour_value(q+dxy),contour_value(q-dxy),contour_value(q+dxy.yx),contour_value(q-dxy.yx));
            var value=mix(contour_value(q),dot(samples,vec4f(0.25)),saturate(u.threshold.x));
            if u.threshold.w!=0 {value=1.0-value;}
            let levels=max(1.0,u.threshold.z);
            let threshold=select(u.contour.w,0.5,u.shape.y==3.0);
            let diff=(fract((value-threshold)*levels+0.5)-0.5)/levels;
            d=diff*2.0*r*scale/max(length(samples.xz-samples.yw),0.0001);
        }
        let width=w.stroke.y*0.5;
        let aa=1.0/sqrt(max(1.0,u.polygon.w));
        let k=clamp(d/max(width,0.001),-1.0,1.0);
        var profile=1.0;
        switch u32(u.shape.w) { case 2u:{profile=1.0-abs(k);} case 3u:{profile=0.5+0.5*k;}
          case 4u:{profile=0.5-0.5*k;} case 5u:{profile=exp(-3.0*k*k);} case 6u:{profile=1.0-k*k;} case 7u:{profile=cos(1.570796*k);} default:{} }
        let alpha=(1.0-smoothstep(max(0.0,width-aa),width+aa,abs(d)))*profile*w.stroke.x*w.color.a;
        result=vec4f(w.color.rgb*alpha,alpha)+result*(1.0-alpha);
    }
    textureStore(out,vec2i(id.xy),result);
}
