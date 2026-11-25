// Tritone (ADBE Tritone, AE-FX-037): luminance mapped onto Shadows → Midtones →
// Highlights (midtones at 0.5), then mixed back towards the original.
fn tritone(c: vec4<f32>, px: BaePixel) -> vec4<f32> {
    let s = bae_unpremultiply(c);
    // Rec.601 weights, like Tint (fitted to AE 26 renders).
    let l = dot(s.rgb, vec3<f32>(0.299, 0.587, 0.114));
    let ramp=tone_ramp(l,params.shadows.rgb,params.midtones.rgb,params.highlights.rgb);
    let rgb = mix(ramp, s.rgb, params.blend / 100.0);
    return vec4<f32>(rgb * c.a, c.a);
}
