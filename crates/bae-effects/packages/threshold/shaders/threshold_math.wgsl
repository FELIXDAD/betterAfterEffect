fn threshold_step(value:f32,level:f32)->f32{
    if (frame.flags&BAE_FRAME_FLOAT)==0u && level<1. {return f32(round(value*255.)>floor(level*255.));}
    return f32(value>=level);
}