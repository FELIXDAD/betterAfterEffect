fn evaluate(p:vec2f)->vec4f {
    return output(vec3f(src(p).r,aux_a(p).g,aux_b(p).b),aux_c(p).a);
}
