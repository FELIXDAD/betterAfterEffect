struct Overlay {
    image: FxImage,
    origin: vec2i,
    count:u32,
    radius:f32,
    points:array<vec4f,384>,
}
@group(0) @binding(0) var<uniform> settings:Overlay;
@group(0) @binding(1) var source:texture_2d<f32>;
@group(0) @binding(2) var dest:texture_storage_2d<WORKING_FORMAT,write>;
var<workgroup> nearby:array<vec4f,384>;
var<workgroup> used:atomic<u32>;
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) gid:vec3u,@builtin(workgroup_id) tile:vec3u,@builtin(local_invocation_index) lane:u32){
    if lane==0 {atomicStore(&used,0);}
    workgroupBarrier();
    let tile_center=vec2f(settings.origin)+vec2f(tile.xy*8u)+4.;
    for(var k=lane;k<settings.count;k+=64u){
        let p=settings.points[k];
        if all(abs(p.xy-tile_center)<=vec2f(4.+p.w)){
            nearby[atomicAdd(&used,1u)]=p;
        }
    }
    workgroupBarrier();
    if any(gid.xy>=textureDimensions(dest)){return;}
    let xy=vec2f(settings.origin)+vec2f(gid.xy)+0.5;
    var color=fx_sample(source,settings.image,xy,false);
    var err=2.;
    for(var k=0u;k<atomicLoad(&used);k++){
        let p=nearby[k];let delta=abs(xy-p.xy);
        if max(delta.x,delta.y)<=p.w && abs(delta.x-delta.y)<1.1 {err=min(err,p.z);}
    }
    if err<=1.{color=vec4f(mix(vec3f(0.12,0.9,0.32),vec3f(1.,0.15,0.05),err),1.);}
    textureStore(dest,vec2i(gid.xy),color);
}
