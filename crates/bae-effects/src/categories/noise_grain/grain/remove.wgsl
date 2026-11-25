fn blend_ids()->vec4u {return vec4u(23,9,10,11);}
fn selection_id()->u32 {return 21u;}
fn first_point()->u32 {return 30u;}
fn count_id()->u32 {return 3u;}
fn size_id()->u32 {return 4u;}
fn invert_id()->u32 {return 12u;}
fn placement_id()->u32 {return 70u;}
fn mask_mode_id()->u32 {return 71u;}
fn combine_id()->u32 {return 72u;}
fn blur_id()->u32 {return 68u;}
fn amount_id()->u32 {return 8u;}
fn view_id()->u32 {return 15u;}
fn mask_view()->u32 {return 3u;}
fn sample_box_id()->u32 {return 54u;}
fn center_id()->u32 {return 16u;}
fn width_id()->u32 {return 18u;}
fn height_id()->u32 {return 19u;}
fn box_id()->u32 {return 56u;}
fn box_color_id()->u32 {return 57u;}

fn candidate(q:vec2f,history:bool)->vec3f {
    let bounds=vec2f(u.layer.xy);let hi=vec2f(u.layer.zw)-0.5;
    let p=clamp(q,bounds+0.5,hi);
    if history{return fx_sample(map_c,u.images[3],p,true).rgb;}
    return fx_sample(input_tex,u.images[0],p,true).rgb;
}
fn nlm_step()->f32 {return max(0.5,(1.+0.5*value(60))*u.scale);}
fn nlm_halfwidth()->i32 {return i32(clamp(value(20),1.,6.));}
fn nlm_temporal()->bool {return has(2u) && enabled(22);}

// The search window of an 8x8 workgroup, loaded once into workgroup memory:
// every patch tap of every pixel reads it instead of the textures (about 1000
// texel loads per pixel at six passes, twice that with temporal). Windows wider
// than the tile fall back to direct loads.
const TILE_REACH:i32=8;
const TILE_SIZE:i32=576; // (8 + 2 * TILE_REACH)^2
var<workgroup> tile_r:array<f32,1152>; // current frame, then the previous frame
var<workgroup> tile_g:array<f32,1152>;
var<workgroup> tile_b:array<f32,1152>;
var<private> tiled:bool;
var<private> tile_width:i32;
var<private> tile_origin:vec2f;

fn tile_texel(i:i32)->vec3f {return vec3f(tile_r[i],tile_g[i],tile_b[i]);}
fn fetch(q:vec2f,history:bool)->vec3f {
    if !tiled {return candidate(q,history);}
    // The nearest tile entry: what `candidate` loads at `q`.
    let cell=vec2i(floor(q-tile_origin+0.5));
    return tile_texel(select(0,TILE_SIZE,history)+cell.y*tile_width+cell.x);
}

@compute @workgroup_size(8,8)
fn remove_main(
    @builtin(global_invocation_id) id:vec3u,
    @builtin(workgroup_id) wg:vec3u,
    @builtin(local_invocation_index) lane:u32,
) {
    let view=u32(value(view_id()));
    let reach=i32(ceil(f32(nlm_halfwidth()+1)*nlm_step()))+1;
    tiled=reach<=TILE_REACH && value(1)>0. && view!=2u && view!=mask_view();
    if tiled {
        tile_width=8+2*reach;
        tile_origin=vec2f(u.roi.xy+vec2i(wg.xy*8u)-reach)+0.5;
        let area=tile_width*tile_width;
        let layers=select(1,2,nlm_temporal());
        for(var k=i32(lane);k<area*layers;k+=64){
            let j=k%area;let t=k/area;
            let c=candidate(tile_origin+vec2f(f32(j%tile_width),f32(j/tile_width)),t!=0);
            let slot=t*TILE_SIZE+j;
            tile_r[slot]=c.r;tile_g[slot]=c.g;tile_b[slot]=c.b;
        }
    }
    workgroupBarrier();
    let xy=u.roi.xy+vec2i(id.xy);
    if all(xy<u.roi.zw) {textureStore(out_tex,vec2i(id.xy),evaluate(vec2f(xy)+0.5));}
}

fn process(p:vec2f,c:vec4f,sigma:vec3f)->vec3f {
    let noise=max(sigma,vec3f(0.002));
    let channels=vec3f(value(65),value(66),value(67));
    let strength=value(1)*noise*channels;
    if all(strength==vec3f(0.)){return c.rgb;}
    var sum=c.rgb;var weights=vec3f(1.);
    let step_size=nlm_step();
    let halfwidth=nlm_halfwidth();
    let count=select(1,2,nlm_temporal());
    let h=max(vec3f(1e-5),strength*select(1.,1.5,enabled(63)));
    let inv_h2=1./(h*h);
    let bias=2.*noise*noise;
    let single=value(59)==1.;
    let history_weight=value(14)*0.01;
    let history_falloff=value(61)/max(0.001,length(noise));
    let offsets=array<vec2f,5>(vec2f(0.),vec2f(1.,0.),vec2f(-1.,0.),vec2f(0.,1.),vec2f(0.,-1.));
    var center:array<vec3f,5>;
    for(var i=0;i<5;i++){center[i]=fetch(p+offsets[i]*step_size,false);}
    for(var t=0;t<count;t++){for(var y=-halfwidth;y<=halfwidth;y++){for(var x=-halfwidth;x<=halfwidth;x++){
        if x==0 && y==0 && t==0{continue;}
        let q=p+vec2f(f32(x),f32(y))*step_size;
        var dist=vec3f(0.);
        var rgb=vec3f(0.);
        for(var i=0;i<5;i++){
            let s=fetch(q+offsets[i]*step_size,t!=0);
            if i==0 {rgb=s;}
            let delta=center[i]-s;
            dist+=delta*delta;
        }
        dist=max(vec3f(0.),dist*0.2-bias);
        var error=dist*inv_h2;
        if single {error=vec3f(fx_luma(error));}
        var w=exp(-min(error,vec3f(30.)));
        if t!=0{w*=history_weight*exp(-history_falloff*length(rgb-c.rgb));}
        weights+=w;sum+=w*rgb;
    }}}
    var result=sum/weights;
    let removed=c.rgb-result;
    result+=mix(removed,vec3f(fx_luma(removed)),saturate(value(64)))*value(62);
    if value(2)>0. && value(50)>0. {
        let offset=vec2f(value(50)*u.scale,0.);
        let detail=result-0.25*(src(p+offset).rgb+src(p-offset).rgb+src(p+offset.yx).rgb+src(p-offset.yx).rgb);
        result+=select(vec3f(0.),detail,abs(detail)>vec3f(value(51)))*value(2);
    }
    return result;
}
