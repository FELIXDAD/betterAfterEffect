struct Settings {
  input_rect:vec4<i32>, output_rect:vec4<i32>, inner:vec4<f32>, outer:vec4<f32>,
  radius:f32, softness:f32, composite:u32, blend:u32, count:u32, pad0:u32, pad1:u32, pad2:u32,
}
struct Node { bounds:vec4<f32>, segment:vec4<f32>, traversal:vec4<f32> }
@group(0) @binding(0) var<uniform> u:Settings;
@group(0) @binding(1) var source:texture_2d<f32>;
@group(0) @binding(2) var<storage,read> nodes:array<Node>;
@group(0) @binding(3) var result:texture_storage_2d<WORKING_FORMAT,write>;

fn hue(rgb:vec3<f32>, turns:f32)->vec3<f32>{
  let axis=vec3<f32>(0.577350269);
  let angle=turns*6.283185307;
  return max(vec3<f32>(0),rgb*cos(angle)+cross(axis,rgb)*sin(angle)+axis*dot(axis,rgb)*(1.0-cos(angle)));
}
@compute @workgroup_size(8,8)
fn draw(@builtin(global_invocation_id) gid:vec3<u32>){
  let local=vec2<i32>(gid.xy);
  if any(local>=u.output_rect.zw-u.output_rect.xy){return;}
  let pixel=local+u.output_rect.xy;
  let point=vec2<f32>(pixel)+0.5;
  var base=vec4<f32>(0);
  if u.composite!=0u && all(pixel>=u.input_rect.xy) && all(pixel<u.input_rect.zw){
    base=textureLoad(source,pixel-u.input_rect.xy,0);
  }
  var ink=vec3<f32>(0); var alpha=0.0; var sum=0.0; var i=0u;
  while i<u.count {
    let n=nodes[i];
    if any(point<n.bounds.xy) || any(point>n.bounds.zw) {i=u32(n.traversal.x);continue;}
    i+=1u;
    if n.traversal.y==0.0 {continue;}
    let ab=n.segment.zw-n.segment.xy;
    let along=clamp(dot(point-n.segment.xy,ab)/max(dot(ab,ab),0.000001),0.0,1.0);
    let d=distance(point,n.segment.xy+ab*along);
    let feather=max(0.5,u.radius*u.softness);
    let coverage=1.0-smoothstep(max(0.0,u.radius-feather),u.radius+0.5,d);
    if coverage<=0.0 {continue;}
    let col=hue(mix(u.inner.rgb,u.outer.rgb,clamp(d/max(0.01,u.radius),0.0,1.0)),n.traversal.z);
    if u.blend!=0u {
      ink+=col*coverage;sum+=coverage;
    }else if coverage>alpha {
      ink=col;
    }
    alpha=max(alpha,coverage);
  }
  if u.blend!=0u && sum>0.0 {ink/=sum;}
  textureStore(result,local,vec4<f32>(ink*alpha,alpha)+base*(1.0-alpha));
}
