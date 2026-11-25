fn limit_signal(rgb:vec3f,peak:f32,limit:f32)->vec3f {
    return rgb * select(1.,min(1.,limit/max(0.000001,peak)),peak>0.);
}
