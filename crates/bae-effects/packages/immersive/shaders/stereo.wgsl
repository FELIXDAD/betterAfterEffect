struct Eye { rect: BaeRect, fov: vec2f, side: f32 }
fn vr_eye(p: vec2f) -> Eye {
    var rect = frame.layer_bounds;
    let size = rect_size(rect);
    var packing = params.frame_layout;
    var fov = radians(vec2f(params.horizontal_field_of_view,params.vertical_field_of_view));
    // Media without projection metadata: infer the common full-sphere packing.
    if params.auto_vr_properties != 0u {
        let aspect = size.x / size.y;
        packing = select(select(1u,2u,aspect < 1.414214),3u,aspect > 2.828427);
        fov = FULL;
    }
    var side = 0.0;
    if packing == 2u && size.y >= 2.0 {
        let mid = rect.min.y + (rect.max.y - rect.min.y)/2;
        if p.y < f32(mid) { rect.max.y = mid; side = -1.0; }
        else { rect.min.y = mid; side = 1.0; }
    }
    if packing == 3u && size.x >= 2.0 {
        let mid = rect.min.x + (rect.max.x - rect.min.x)/2;
        if p.x < f32(mid) { rect.max.x = mid; side = -1.0; }
        else { rect.min.x = mid; side = 1.0; }
    }
    return Eye(rect,fov,side);
}
