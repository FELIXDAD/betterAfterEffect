use super::*;
use gpu::{Binding, Kernel, KernelKey, KernelSource};
pub(super) const BYTES: u32 = 272 + 100 * 16;

pub(super) fn pipeline(
    rt: &gpu::Runtime,
    format: wgpu::TextureFormat,
) -> Result<Arc<Kernel>, RenderError> {
    let label = "bae.effects.channel_3d";
    let key = KernelKey {
        owner: label.into(),
        entry: "main".into(),
        variant: gpu::format_key(format),
    };
    rt.kernel(&key, || {
        Ok(KernelSource {
            label,
            entry: "main",
            workgroup: [8, 8, 1],
            wgsl: [
                image_grid::CHANNEL_MATH,
                include_str!("../../shaders/image_grid.wgsl"),
                include_str!("channels.wgsl"),
            ]
            .concat()
            .replace("WORKING_FORMAT", crate::blur::format_name(format)?),
            bindings: vec![
                (0, Binding::Uniform(BYTES)),
                (1, Binding::Texture),
                (2, Binding::UintTextureArray),
                (3, Binding::Texture),
                (4, Binding::Storage(format)),
                (5, Binding::UintTextureArray),
            ],
        })
    })
}
