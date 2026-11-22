//! Compile-time proof that BAE's GPU, 2D and UI dependencies form one coherent
//! version set.
//!
//! The renderer (vello), the UI (egui-wgpu) and plugins share a single
//! `wgpu::Device` (`bae_gpu::GpuContext`), vello draws what parley lays out, and
//! colors, shapes and fonts flow between all of them as peniko / kurbo / fontique
//! types. That only works when every crate resolves to the *same* copy of
//! wgpu, peniko, kurbo, fontique and epaint. If a bump in the root `Cargo.toml`
//! `[workspace.dependencies]` makes cargo pick a second copy of any of them, this
//! crate stops compiling (mismatched types, or the `SameType` diagnostic below)
//! instead of the problem surfacing later as a confusing error in a downstream
//! crate, a runtime surprise, or a silently bigger binary.
//!
//! Nothing here runs in the app: the crate is `publish = false` and nothing
//! depends on it. The functions below are never called by BAE; they exist so that
//! the compiler type-checks the exact cross-crate calls BAE makes (one device
//! for vello and egui-wgpu, vello output shown as an egui texture, parley glyph
//! runs drawn into a vello scene). The only code that executes is a tiny CPU-only
//! unit test.
//!
//! What this crate cannot see: crates whose types never cross a public API
//! (skrifa / read-fonts, harfrust, naga, wgpu-hal). Duplicates of those are found
//! with `cargo tree -d` and `cargo tree -i wgpu` (see `docs/spec/dependencies.md`).

#![forbid(unsafe_code)]

/// Implemented only when `Self` and `T` are the same type.
#[diagnostic::on_unimplemented(
    message = "`{Self}` and `{T}` are different types: two versions of the same crate are in the dependency graph",
    label = "incoherent dependency version set",
    note = "keep wgpu / vello / egui-wgpu / parley pins compatible; see docs/spec/dependencies.md"
)]
trait SameType<T: ?Sized> {}

impl<T: ?Sized> SameType<T> for T {}

/// Compiles only when `A` and `B` are the same type.
const fn assert_same_type<A: ?Sized + SameType<B>, B: ?Sized>() {}

/// `same_type!(A => B, ...)`: compile-time type-equality assertions.
macro_rules! same_type {
    ($($a:ty => $b:ty),+ $(,)?) => {
        $(const _: () = assert_same_type::<$a, $b>();)+
    };
}

// One wgpu: our pin, the one vello was built against, and the one egui-wgpu was
// built against. A device created by bae-gpu must be usable by both renderers.
same_type!(
    wgpu::Instance => vello::wgpu::Instance,
    wgpu::Device => vello::wgpu::Device,
    wgpu::Queue => vello::wgpu::Queue,
    wgpu::TextureView => vello::wgpu::TextureView,
    wgpu::TextureFormat => vello::wgpu::TextureFormat,
    wgpu::Instance => egui_wgpu::wgpu::Instance,
    wgpu::Device => egui_wgpu::wgpu::Device,
    wgpu::Queue => egui_wgpu::wgpu::Queue,
    wgpu::TextureView => egui_wgpu::wgpu::TextureView,
    wgpu::TextureFormat => egui_wgpu::wgpu::TextureFormat,
    wgpu::CommandEncoder => egui_wgpu::wgpu::CommandEncoder,
);

// One peniko / kurbo / color: brushes and geometry built by bae-vector and
// bae-text go straight into vello scenes.
same_type!(
    peniko::Color => vello::peniko::Color,
    peniko::Brush => vello::peniko::Brush,
    peniko::Fill => vello::peniko::Fill,
    peniko::ImageData => vello::peniko::ImageData,
    peniko::color::AlphaColor<peniko::color::Srgb> => vello::peniko::color::AlphaColor<vello::peniko::color::Srgb>,
    kurbo::Affine => vello::kurbo::Affine,
    kurbo::Affine => peniko::kurbo::Affine,
    kurbo::BezPath => vello::kurbo::BezPath,
    kurbo::Rect => vello::kurbo::Rect,
);

// One fontique / font-data handle: parley's font collection is ours, and the font
// data parley hands out is what vello's glyph drawing takes.
same_type!(
    fontique::Collection => parley::fontique::Collection,
    fontique::Blob<u8> => peniko::Blob<u8>,
    parley::FontData => peniko::FontData,
    parley::FontData => vello::peniko::FontData,
);

/// The two GPU renderers BAE runs on the one shared device.
pub struct SharedRenderers {
    /// Viewport / vector renderer.
    pub vello: vello::Renderer,
    /// UI renderer.
    pub egui: egui_wgpu::Renderer,
}

/// Creates vello and egui-wgpu renderers on the same `wgpu::Device`, as
/// `apps/bae` does with `bae_gpu::GpuContext`.
///
/// # Errors
///
/// Returns vello's error if its shaders cannot be created on `device`.
pub fn create_renderers(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
) -> Result<SharedRenderers, vello::Error> {
    let vello = vello::Renderer::new(
        device,
        vello::RendererOptions {
            use_cpu: false,
            antialiasing_support: vello::AaSupport::area_only(),
            num_init_threads: None,
            pipeline_cache: None,
        },
    )?;
    let egui = egui_wgpu::Renderer::new(
        device,
        surface_format,
        egui_wgpu::RendererOptions::default(),
    );
    Ok(SharedRenderers { vello, egui })
}

/// Renders a vello scene into `target` and makes it available to egui as a
/// texture (the viewport path: GPU result shown in a UI panel without a copy
/// through the CPU).
///
/// `target` must be an `Rgba8Unorm` view with `STORAGE_BINDING |
/// TEXTURE_BINDING` usage. Pass the id returned by a previous call to reuse the
/// egui texture slot.
///
/// # Errors
///
/// Returns vello's error if rendering fails.
#[expect(
    clippy::too_many_arguments,
    reason = "mirrors the real call site; this function only exists to be type-checked"
)]
pub fn render_viewport(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderers: &mut SharedRenderers,
    scene: &vello::Scene,
    target: &wgpu::TextureView,
    size: [u32; 2],
    background: peniko::Color,
    texture: Option<egui::TextureId>,
) -> Result<egui::TextureId, vello::Error> {
    let params = vello::RenderParams {
        base_color: background,
        width: size[0],
        height: size[1],
        antialiasing_method: vello::AaConfig::Area,
    };
    renderers
        .vello
        .render_to_texture(device, queue, scene, target, &params)?;
    let filter = wgpu::FilterMode::Linear;
    Ok(match texture {
        Some(id) => {
            renderers
                .egui
                .update_egui_texture_from_wgpu_texture(device, target, filter, id);
            id
        }
        None => renderers
            .egui
            .register_native_texture(device, target, filter),
    })
}

/// Uploads one egui frame (texture deltas and tessellated meshes) with egui-wgpu.
/// egui's `epaint` types must be the ones egui-wgpu was built against.
pub fn prepare_egui_frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    renderer: &mut egui_wgpu::Renderer,
    textures: &egui::TexturesDelta,
    paint_jobs: &[egui::ClippedPrimitive],
    screen: &egui_wgpu::ScreenDescriptor,
) -> Vec<wgpu::CommandBuffer> {
    for (id, deltas) in &textures.set {
        for delta in deltas {
            renderer.update_texture(device, queue, *id, delta);
        }
    }
    let commands = renderer.update_buffers(device, queue, encoder, paint_jobs, screen);
    for id in &textures.free {
        renderer.free_texture(id);
    }
    commands
}

/// Fills `shape` (our kurbo) with `color` (our peniko) in a vello scene.
pub fn fill_shape(scene: &mut vello::Scene, shape: &kurbo::BezPath, color: peniko::Color) {
    scene.fill(
        peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        color,
        None,
        shape,
    );
}

/// Draws every glyph run of a parley layout into a vello scene (what bae-text
/// does): parley's `FontData` is passed to vello unchanged.
pub fn draw_text_layout(
    scene: &mut vello::Scene,
    layout: &parley::Layout<peniko::Brush>,
    transform: kurbo::Affine,
) {
    for line in layout.lines() {
        for item in line.items() {
            let parley::PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };
            let run = glyph_run.run();
            let coords: Vec<vello::NormalizedCoord> = run
                .normalized_coords()
                .iter()
                .map(|coord| coord.to_bits())
                .collect();
            scene
                .draw_glyphs(run.font())
                .font_size(run.font_size())
                .transform(transform)
                .normalized_coords(&coords)
                .brush(&glyph_run.style().brush)
                .draw(
                    peniko::Fill::NonZero,
                    glyph_run.positioned_glyphs().map(|glyph| vello::Glyph {
                        id: glyph.id,
                        x: glyph.x,
                        y: glyph.y,
                    }),
                );
        }
    }
}

/// Opens parley / vello font data with our skrifa (font tables, outlines,
/// metrics). skrifa types do not cross vello's or parley's public API, so this
/// only proves the data handle is compatible; a second skrifa / read-fonts copy is
/// caught by `cargo tree -d` instead.
pub fn open_font(font: &peniko::FontData) -> Option<skrifa::FontRef<'_>> {
    skrifa::FontRef::from_index(font.data.data(), font.index).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The CPU-side halves of the probe run without a GPU.
    #[test]
    fn cpu_paths_build_a_scene() {
        let mut scene = vello::Scene::new();
        let rect = kurbo::Rect::new(0.0, 0.0, 16.0, 9.0);
        fill_shape(
            &mut scene,
            &kurbo::Shape::to_path(&rect, 0.1),
            peniko::Color::WHITE,
        );
        draw_text_layout(&mut scene, &parley::Layout::new(), kurbo::Affine::IDENTITY);
        assert!(!scene.encoding().is_empty());

        let empty = peniko::FontData::new(peniko::Blob::from(Vec::new()), 0);
        assert!(open_font(&empty).is_none());
    }
}
