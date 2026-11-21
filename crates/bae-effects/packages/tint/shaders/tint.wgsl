// Tint (ADBE Tint, AE-FX-036): luminance mapped onto the Map Black To → Map
// White To ramp, mixed with the original by Amount to Tint. Straight colour,
// alpha unchanged, no clamping (32 bpc behaviour).
fn tint(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let s = bae_unpremultiply(c);
    // Rec.709 weights (Skottie's reconstruction; AE's weights are UNVERIFIED).
    let l = dot(s.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let ramp = params.map_black.rgb + (params.map_white.rgb - params.map_black.rgb) * l;
    let rgb = mix(s.rgb, ramp, params.amount / 100.0);
    return vec4<f32>(rgb * c.a, c.a);
}
