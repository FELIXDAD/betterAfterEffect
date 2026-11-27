// 3D model shadows in a receiver's shadow map (exec/mesh.rs, graph
// ShadowMapNode): the model's triangles projected from the light onto the
// receiver plane, into the light's slot. Each covered texel keeps at most
// what passes the model (Min blending: the front and back faces of a model
// darken once). Only the parts on the light's side of the receiver cast.

struct Params {
    // Comp space → slot clip space (planar projection from the light).
    clip: mat4x4<f32>,
    // Model space → comp space.
    model: mat4x4<f32>,
    // The receiver plane, positive on the light's side.
    side: vec4<f32>,
    // Light that passes the model (1 − shadow darkness).
    value: vec4<f32>,
}

@group(0) @binding(0) var<uniform> u: Params;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) side: f32,
}

@vertex
fn vs_shadow(@location(0) pos: vec3<f32>) -> VsOut {
    let world = u.model * vec4<f32>(pos, 1.0);
    var o: VsOut;
    o.pos = u.clip * world;
    o.side = dot(u.side.xyz, world.xyz) + u.side.w;
    return o;
}

@fragment
fn fs_shadow(i: VsOut) -> @location(0) vec4<f32> {
    if i.side <= 1e-3 {
        discard;
    }
    return vec4<f32>(u.value.rgb, 1.0);
}

// Light-view depth of the 3D models that cast shadows on models
// (exec/mesh.rs): `clip` is the light's view-projection.
@vertex
fn vs_caster(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    return u.clip * (u.model * vec4<f32>(pos, 1.0));
}
