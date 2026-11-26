//! The UI layer shared by the window and the headless smoke test: the egui
//! renderer on the shared device and the `bae-ui` workbench.

mod renderer;
mod workspace;

pub use renderer::{UiOutput, UiRenderer};
#[cfg(test)]
pub use workspace::ShortcutRow;
#[cfg(test)]
pub(crate) use workspace::gpu_kind as workspace_gpu_kind;
pub(crate) use workspace::hardware_information;
#[cfg(test)]
pub(crate) use workspace::plugin_notice_template;
pub use workspace::{
    PlaceholderViewer, REVEAL_PREFERENCES, StatusInfo, Workspace, failure_notice, gpu_summary,
    plugin_notice, register_panels,
};

use bae_gpu::viewport::{ViewportTextureId, ViewportTextureRegistry};
use bae_gpu::{GpuContext, transfer, wgpu};

/// Publishes a placeholder "composition" (a 16:9 test pattern) so the
/// zero-copy viewer path (registry → egui-wgpu → panel) is exercised before the
/// renderer (workstream 04) exists. Returns `None` if the upload fails.
pub fn publish_placeholder_comp(
    gpu: &GpuContext,
    registry: &ViewportTextureRegistry,
) -> Option<ViewportTextureId> {
    const W: u32 = 480;
    const H: u32 = 270;
    let mut pixels = Vec::with_capacity((W * H * 4) as usize);
    for y in 0..H {
        for x in 0..W {
            let checker = ((x / 30) + (y / 30)) % 2 == 0;
            let base: u8 = if checker { 58 } else { 44 };
            // Premultiplied, opaque; a soft gradient so scaling is visible.
            let r = base.saturating_add((x * 40 / W) as u8);
            let g = base.saturating_add((y * 40 / H) as u8);
            pixels.extend_from_slice(&[r, g, base.saturating_add(24), 255]);
        }
    }
    let texture = transfer::create_texture_with_data(
        gpu,
        &wgpu::TextureDescriptor {
            label: Some("bae.placeholder-comp"),
            size: wgpu::Extent3d {
                width: W,
                height: H,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        &pixels,
    )
    .map_err(|error| tracing::warn!("placeholder comp upload failed: {error}"))
    .ok()?;
    let id = registry.create("composition viewer");
    registry
        .publish(id, &texture, wgpu::FilterMode::Linear)
        .map_err(|error| tracing::warn!("placeholder comp publish failed: {error}"))
        .ok()?;
    Some(id)
}
