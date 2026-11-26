use super::packages;
use crate::Registry;

pub(crate) fn register(registry: &mut Registry) {
    packages(
        registry,
        &[embedded!(
            "immersive",
            [
                "shaders/sphere.wgsl",
                "shaders/projection.wgsl",
                "shaders/stereo.wgsl",
                "shaders/vr_converter.wgsl",
                "shaders/vr_plane_to_sphere.wgsl",
                "shaders/vr_rotate_sphere.wgsl",
                "shaders/vr_sphere_to_plane.wgsl",
                "shaders/vr_blur.wgsl",
                "shaders/neighborhood.wgsl",
                "shaders/vr_de_noise.wgsl",
                "shaders/vr_glow.wgsl",
                "shaders/vr_sharpen.wgsl",
            ]
        )],
    );
}
