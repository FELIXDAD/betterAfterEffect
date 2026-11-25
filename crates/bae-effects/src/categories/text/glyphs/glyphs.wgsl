struct Glyph {mask:u32,left:f32,width:f32,pad:f32}
struct Display {src:FxImage,roi:vec4i,layout_data:vec4f,fill:vec4f,outline:vec4f,box_color:vec4f,style:vec4f,flags:vec4u,glyphs:array<Glyph,32>}
@group(0) @binding(0) var<uniform> u:Display;
@group(0) @binding(1) var image:texture_2d<f32>;
@group(0) @binding(2) var out:texture_storage_2d<WORKING_FORMAT,write>;

// Local stroke endpoints on a normalized character cell.
const lines=array<vec4f,17>(
    vec4f(0,0,1,0),vec4f(1,0,1,0.5),vec4f(1,0.5,1,1),vec4f(0,1,1,1),
    vec4f(0,0.5,0,1),vec4f(0,0,0,0.5),vec4f(0,0.5,1,0.5),
    vec4f(0,0,0.5,0.5),vec4f(1,0,0.5,0.5),vec4f(0,1,0.5,0.5),vec4f(1,1,0.5,0.5),
    vec4f(0),vec4f(0.5,0,0.5,0.5),vec4f(0.5,0.5,0.5,1),
    vec4f(0.5,0.96,0.5,0.98),vec4f(0.5,0.35,0.5,0.37),vec4f(0.5,0.97,0.27,1.14)
);
fn over(base:vec4f,ink:vec4f,coverage:f32)->vec4f{
    let a=ink.a*coverage;return vec4f(ink.rgb*a,a)+base*(1.0-a);
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3u){
    let p=vec2i(id.xy)+u.roi.xy;if any(p>=u.roi.zw){return;}
    let pos=vec2f(p)+0.5;
    let local=(pos-u.layout_data.xy)/max(u.layout_data.z,0.001);
    var src=vec4f(0);
    if u.flags.z!=0u {src=fx_sample(image,u.src,pos,false);}
    var ink=vec4f(0);
    if all(local>=vec2f(-0.13,-0.14)) && all(local<=vec2f(u.layout_data.w,1.18)){ink=over(ink,u.box_color,1.0);}
    var dist=1e8;
    let radius=u.style.y/max(u.layout_data.z,0.001)+0.1;
    if local.y>=-radius && local.y<=1.15+radius{
        for(var i=0u;i<u.flags.x;i++){
            let g=u.glyphs[i];let pt=local-vec2f(g.left,0);
            if pt.x < -radius || pt.x>g.width+radius {continue;}
            for(var j=0u;j<17u;j++){
                if (g.mask&(1u<<j))==0u {continue;}
                let seg=lines[j]*vec4f(g.width,1,g.width,1);
                let ab=seg.zw-seg.xy;
                let t=saturate(dot(pt-seg.xy,ab)/max(dot(ab,ab),0.000001));
                dist=min(dist,length(pt-seg.xy-ab*t)*u.layout_data.z);
            }
        }
    }
    let sdf=dist-u.layout_data.z*0.046;
    let fill=saturate(0.5-sdf);
    let stroke=saturate(0.5+u.style.y*0.5-abs(sdf));
    if u.flags.y==2u || u.flags.y==3u {ink=over(ink,u.outline,stroke);}
    if u.flags.y!=2u {ink=over(ink,u.fill,fill);}
    if u.flags.y==4u {ink=over(ink,u.outline,stroke);}
    ink*=u.style.x;
    var result=ink+src*(1.0-ink.a);
    if u.flags.w==0u {result=vec4f(clamp(result.rgb,vec3f(0),vec3f(saturate(result.a))),saturate(result.a));}
    textureStore(out,vec2i(id.xy),result);
}
