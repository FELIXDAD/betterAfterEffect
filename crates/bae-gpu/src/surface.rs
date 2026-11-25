//! Window surfaces: swapchain configuration, resize, present mode, frame latency,
//! lost/outdated handling, and HDR / wide-gamut format discovery.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::context::{GpuContext, GpuError};

/// How frames are paced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PresentModePreference {
    /// VSync without tearing; lowest power (`FifoRelaxed` if available, else `Fifo`).
    #[default]
    Vsync,
    /// Prefer tear-free mailbox presentation; use FIFO if mailbox is unavailable.
    LowLatency,
    /// Uncapped (`Immediate`, else `Mailbox`, else `Fifo`). For benchmarks.
    NoVsync,
}

impl PresentModePreference {
    /// Picks the best supported mode. `Fifo` is always supported.
    pub fn pick(self, supported: &[wgpu::PresentMode]) -> wgpu::PresentMode {
        use wgpu::PresentMode as P;
        let order: &[P] = match self {
            Self::Vsync => &[P::FifoRelaxed, P::Fifo],
            Self::LowLatency => &[P::Mailbox, P::Fifo],
            Self::NoVsync => &[P::Immediate, P::Mailbox, P::Fifo],
        };
        order
            .iter()
            .copied()
            .find(|mode| supported.contains(mode))
            .unwrap_or(P::Fifo)
    }
}

/// Which kind of output the surface should use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SurfaceColorPreference {
    /// 8-bit non-sRGB format (BGRA/RGBA), sRGB color space. egui blends in gamma
    /// space, so the UI wants a non-sRGB swapchain; an sRGB view format is added
    /// for renderers that want hardware encoding.
    #[default]
    Sdr,
    /// The best extended-range format the display offers (EDR on macOS, scRGB on
    /// Windows), falling back to SDR. Chosen by the color workstream (06).
    Hdr,
}

/// Swapchain options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceOptions {
    /// Frame pacing.
    pub present_mode: PresentModePreference,
    /// Frames the CPU may queue ahead of the GPU (1 = lowest latency, 2 = smoother
    /// under load). Clamped to 1..=3.
    pub frame_latency: u32,
    /// SDR or HDR output.
    pub color: SurfaceColorPreference,
}

impl Default for SurfaceOptions {
    fn default() -> Self {
        Self {
            present_mode: PresentModePreference::Vsync,
            frame_latency: 2,
            color: SurfaceColorPreference::Sdr,
        }
    }
}

/// A format together with the color spaces the surface supports for it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FormatColorSpaces {
    /// Texture format.
    pub format: String,
    /// Supported color spaces, e.g. `["Srgb", "DisplayP3"]`.
    pub color_spaces: Vec<String>,
}

/// What the surface + display support, for the color workstream and `gpu-info`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SurfaceColorInfo {
    /// Formats usable with the default (`Auto`) color space, best first.
    pub formats: Vec<String>,
    /// Every format with its color spaces (includes HDR-only formats).
    pub format_color_spaces: Vec<FormatColorSpaces>,
    /// Supported present modes.
    pub present_modes: Vec<String>,
    /// Supported alpha modes.
    pub alpha_modes: Vec<String>,
    /// Extended linear sRGB (scRGB / EDR) with `Rgba16Float`.
    pub extended_srgb_linear: bool,
    /// Display P3 color space (wide gamut SDR).
    pub display_p3: bool,
    /// BT.2100 PQ (HDR10).
    pub hdr10: bool,
    /// The display currently reports high dynamic range (if known).
    pub display_hdr: Option<bool>,
    /// Current EDR headroom (macOS: max / SDR white), if known.
    pub headroom: Option<f32>,
    /// Peak luminance in nits, if known.
    pub max_nits: Option<f32>,
    /// SDR reference white in nits, if known.
    pub sdr_white_nits: Option<f32>,
    /// The best HDR configuration, if any: (format, color space).
    #[serde(skip)]
    pub best_hdr: Option<(wgpu::TextureFormat, wgpu::SurfaceColorSpace)>,
}

impl SurfaceColorInfo {
    /// Queries `surface` on `adapter`. On macOS the display HDR state is only
    /// available on the main thread (wgpu returns "unknown" elsewhere).
    pub fn query(surface: &wgpu::Surface<'_>, adapter: &wgpu::Adapter) -> Self {
        let caps = surface.get_capabilities(adapter);
        let hdr = surface.display_hdr_info(adapter);
        let spaces_of = |format| {
            caps.format_capabilities
                .iter()
                .find(|c| c.format == format)
                .map(|c| c.color_spaces)
                .unwrap_or(wgpu::SurfaceColorSpaces::empty())
        };
        use wgpu::SurfaceColorSpaces as S;
        use wgpu::TextureFormat as F;
        let extended_srgb_linear = spaces_of(F::Rgba16Float).contains(S::EXTENDED_SRGB_LINEAR);
        let hdr10 = [F::Rgb10a2Unorm, F::Rgba16Float]
            .iter()
            .any(|&f| spaces_of(f).contains(S::BT2100_PQ));
        let display_p3 = caps
            .format_capabilities
            .iter()
            .any(|c| c.color_spaces.contains(S::DISPLAY_P3));
        let best_hdr = if extended_srgb_linear {
            Some((F::Rgba16Float, wgpu::SurfaceColorSpace::ExtendedSrgbLinear))
        } else if spaces_of(F::Rgba16Float).contains(S::EXTENDED_DISPLAY_P3) {
            Some((F::Rgba16Float, wgpu::SurfaceColorSpace::ExtendedDisplayP3))
        } else if spaces_of(F::Rgb10a2Unorm).contains(S::BT2100_PQ) {
            Some((F::Rgb10a2Unorm, wgpu::SurfaceColorSpace::Bt2100Pq))
        } else {
            None
        };
        Self {
            formats: caps.formats.iter().map(|f| format!("{f:?}")).collect(),
            format_color_spaces: caps
                .format_capabilities
                .iter()
                .map(|c| FormatColorSpaces {
                    format: format!("{:?}", c.format),
                    color_spaces: c
                        .color_spaces
                        .iter_names()
                        .map(|(name, _)| name.to_owned())
                        .collect(),
                })
                .collect(),
            present_modes: caps
                .present_modes
                .iter()
                .map(|m| format!("{m:?}"))
                .collect(),
            alpha_modes: caps.alpha_modes.iter().map(|m| format!("{m:?}")).collect(),
            extended_srgb_linear,
            display_p3,
            hdr10,
            display_hdr: hdr.coarse.and_then(|c| c.high_dynamic_range),
            headroom: hdr.headroom.and_then(|h| h.current),
            max_nits: hdr.luminance.and_then(|l| l.max_nits),
            sdr_white_nits: hdr.luminance.and_then(|l| l.sdr_white_nits),
            best_hdr,
        }
    }
}

/// Picks the SDR swapchain format: 8-bit non-sRGB, BGRA preferred (native on
/// both Metal and D3D12), else whatever the surface prefers.
pub fn pick_sdr_format(formats: &[wgpu::TextureFormat]) -> Option<wgpu::TextureFormat> {
    use wgpu::TextureFormat as F;
    [F::Bgra8Unorm, F::Rgba8Unorm]
        .into_iter()
        .find(|f| formats.contains(f))
        .or_else(|| formats.iter().copied().find(|f| !f.is_srgb()))
        .or_else(|| formats.first().copied())
}

/// A presentable window surface on the shared device.
pub struct WindowSurface {
    ctx: Arc<GpuContext>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    options: SurfaceOptions,
    color: SurfaceColorInfo,
    /// `false` while the window has zero area (minimized).
    configured: bool,
    /// The window edge is being dragged (see [`WindowSurface::set_live_resize`]).
    live_resize: bool,
}

impl std::fmt::Debug for WindowSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowSurface")
            .field("size", &(self.config.width, self.config.height))
            .field("format", &self.config.format)
            .field("present_mode", &self.config.present_mode)
            .finish_non_exhaustive()
    }
}

impl WindowSurface {
    /// Configures `surface` (from [`GpuContext::new_with_surface`] or
    /// [`GpuContext::create_surface`]) for a window of `width × height` physical
    /// pixels.
    pub fn new(
        ctx: &Arc<GpuContext>,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
        options: SurfaceOptions,
    ) -> Result<Self, GpuError> {
        let caps = surface.get_capabilities(ctx.adapter());
        if caps.formats.is_empty() {
            return Err(GpuError::SurfaceUnsupported(
                ctx.adapter_info().name.clone(),
            ));
        }
        let color = SurfaceColorInfo::query(&surface, ctx.adapter());
        let (format, color_space) = match (options.color, color.best_hdr) {
            (SurfaceColorPreference::Hdr, Some(best)) => best,
            _ => (
                pick_sdr_format(&caps.formats)
                    .ok_or_else(|| GpuError::SurfaceUnsupported(ctx.adapter_info().name.clone()))?,
                wgpu::SurfaceColorSpace::Auto,
            ),
        };
        let view_formats = if format.is_srgb() {
            vec![format.remove_srgb_suffix()]
        } else if format.add_srgb_suffix() != format {
            vec![format.add_srgb_suffix()]
        } else {
            Vec::new()
        };
        let alpha_mode = [
            wgpu::CompositeAlphaMode::Opaque,
            wgpu::CompositeAlphaMode::Auto,
        ]
        .into_iter()
        .find(|m| caps.alpha_modes.contains(m))
        .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space,
            width: width.max(1),
            height: height.max(1),
            present_mode: options.present_mode.pick(&caps.present_modes),
            desired_maximum_frame_latency: options.frame_latency.clamp(1, 3),
            alpha_mode,
            view_formats,
        };
        let mut this = Self {
            ctx: ctx.clone(),
            surface,
            config,
            options,
            color,
            configured: false,
            live_resize: false,
        };
        this.resize(width, height);
        tracing::debug!(
            format = ?this.config.format,
            present_mode = ?this.config.present_mode,
            latency = this.config.desired_maximum_frame_latency,
            "surface configured"
        );
        Ok(this)
    }

    /// The context this surface presents with.
    pub fn context(&self) -> &Arc<GpuContext> {
        &self.ctx
    }

    /// The wgpu surface.
    pub fn surface(&self) -> &wgpu::Surface<'static> {
        &self.surface
    }

    /// Current configuration.
    pub fn config(&self) -> &wgpu::SurfaceConfiguration {
        &self.config
    }

    /// Swapchain format (render targets drawing into the frame must use it).
    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Size in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Options in effect.
    pub fn options(&self) -> SurfaceOptions {
        self.options
    }

    /// Format / color-space support of the surface and display.
    pub fn color_info(&self) -> &SurfaceColorInfo {
        &self.color
    }

    /// Re-queries HDR state (call on the main thread after the window moved to
    /// another display or the OS HDR setting changed).
    pub fn refresh_color_info(&mut self) -> &SurfaceColorInfo {
        self.color = SurfaceColorInfo::query(&self.surface, self.ctx.adapter());
        &self.color
    }

    /// Resizes the swapchain. A zero-sized window (minimized) pauses presentation:
    /// [`Self::acquire`] returns `Ok(None)` until a non-zero size arrives.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            self.configured = false;
            return;
        }
        let max = self.ctx.limits().max_texture_dimension_2d;
        self.config.width = width.min(max);
        self.config.height = height.min(max);
        self.reconfigure();
    }

    /// Changes the number of frames the CPU may run ahead (1..=3).
    pub fn set_frame_latency(&mut self, frames: u32) {
        self.options.frame_latency = frames.clamp(1, 3);
        self.config.desired_maximum_frame_latency = self.options.frame_latency;
        self.reconfigure_if_active();
    }

    /// Switches to an explicit format and color space (the color workstream's HDR
    /// path). Fails if the surface does not support the combination.
    pub fn set_color(
        &mut self,
        format: wgpu::TextureFormat,
        color_space: wgpu::SurfaceColorSpace,
    ) -> Result<(), GpuError> {
        let caps = self.surface.get_capabilities(self.ctx.adapter());
        let supported = if color_space == wgpu::SurfaceColorSpace::Auto {
            caps.formats.contains(&format)
        } else {
            caps.format_capabilities.iter().any(|c| {
                c.format == format && c.color_spaces.contains(color_space_flag(color_space))
            })
        };
        if !supported {
            return Err(GpuError::Unsupported(format!(
                "surface format {format:?} with color space {color_space:?}"
            )));
        }
        self.config.format = format;
        self.config.color_space = color_space;
        self.config.view_formats = if format.add_srgb_suffix() != format {
            vec![format.add_srgb_suffix()]
        } else {
            Vec::new()
        };
        self.reconfigure_if_active();
        Ok(())
    }

    fn reconfigure_if_active(&mut self) {
        if self.configured {
            self.reconfigure();
        }
    }

    fn reconfigure(&mut self) {
        self.surface.configure(self.ctx.device(), &self.config);
        self.configured = true;
    }

    /// Acquires the next frame. Returns `Ok(None)` when this frame should be
    /// skipped (minimized, occluded, timeout); outdated / suboptimal swapchains are
    /// reconfigured transparently. [`GpuError::SurfaceLost`] means the surface
    /// must be recreated from the window ([`Self::replace_surface`]);
    /// [`GpuError::DeviceLost`] means the whole context must be recreated.
    pub fn acquire(&mut self) -> Result<Option<SurfaceFrame>, GpuError> {
        if !self.configured {
            return Ok(None);
        }
        if let Some(info) = self.ctx.lost_info() {
            return Err(GpuError::DeviceLost(info));
        }
        for attempt in 0..2 {
            match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(texture) => {
                    return Ok(Some(SurfaceFrame::new(texture, &self.config, &self.ctx)));
                }
                wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                    // Present this one; reconfigure before the next acquire.
                    let frame = SurfaceFrame::new(texture, &self.config, &self.ctx);
                    return Ok(Some(frame.suboptimal()));
                }
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return Ok(None);
                }
                wgpu::CurrentSurfaceTexture::Outdated if attempt == 0 => self.reconfigure(),
                wgpu::CurrentSurfaceTexture::Outdated => return Ok(None),
                wgpu::CurrentSurfaceTexture::Lost => {
                    if let Some(info) = self.ctx.lost_info() {
                        return Err(GpuError::DeviceLost(info));
                    }
                    return Err(GpuError::SurfaceLost);
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    tracing::warn!("surface acquire raised a validation error; skipping frame");
                    return Ok(None);
                }
            }
        }
        Ok(None)
    }

    /// Tells the surface whether the user is live-resizing the window. On macOS
    /// this makes the `CAMetalLayer` present inside the window server's
    /// transaction while resizing, which removes the stretched / jittering frames
    /// AppKit otherwise shows when a window edge is dragged (wgpu-hal honours the
    /// flag at acquire time). Presenting is slightly slower in that mode, so the
    /// shell turns it off again shortly after the last resize event. No-op on
    /// other platforms.
    pub fn set_live_resize(&mut self, resizing: bool) {
        if self.live_resize == resizing {
            return;
        }
        self.live_resize = resizing;
        #[cfg(target_os = "macos")]
        {
            // SAFETY: only a CAMetalLayer property is toggled through the hal
            // guard, which is dropped immediately; no wgpu-owned object is
            // destroyed or retained.
            if let Some(hal) = unsafe { self.surface.as_hal::<wgpu::hal::api::Metal>() } {
                hal.render_layer()
                    .lock()
                    .setPresentsWithTransaction(resizing);
            }
        }
    }

    /// `true` while [`Self::set_live_resize`] is on.
    pub fn is_live_resizing(&self) -> bool {
        self.live_resize
    }

    /// Call after presenting a frame that was [`SurfaceFrame::is_suboptimal`].
    pub fn handle_suboptimal(&mut self) {
        self.reconfigure_if_active();
    }

    /// Replaces a lost surface with a new one created from the same window.
    pub fn replace_surface(&mut self, surface: wgpu::Surface<'static>) {
        self.surface = surface;
        self.color = SurfaceColorInfo::query(&self.surface, self.ctx.adapter());
        self.reconfigure_if_active();
    }
}

fn color_space_flag(space: wgpu::SurfaceColorSpace) -> wgpu::SurfaceColorSpaces {
    use wgpu::SurfaceColorSpace as C;
    use wgpu::SurfaceColorSpaces as S;
    match space {
        C::Auto | C::Srgb => S::SRGB,
        C::ExtendedSrgbLinear => S::EXTENDED_SRGB_LINEAR,
        C::DisplayP3 => S::DISPLAY_P3,
        C::Bt2100Pq => S::BT2100_PQ,
        C::Bt2100Hlg => S::BT2100_HLG,
        C::ExtendedSrgb => S::EXTENDED_SRGB,
        C::ExtendedDisplayP3 => S::EXTENDED_DISPLAY_P3,
    }
}

/// An acquired swapchain image. Render into [`Self::view`], submit, then
/// [`Self::present`].
pub struct SurfaceFrame {
    texture: wgpu::SurfaceTexture,
    view: wgpu::TextureView,
    queue: wgpu::Queue,
    suboptimal: bool,
}

impl std::fmt::Debug for SurfaceFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurfaceFrame")
            .field("suboptimal", &self.suboptimal)
            .finish_non_exhaustive()
    }
}

impl SurfaceFrame {
    fn new(
        texture: wgpu::SurfaceTexture,
        config: &wgpu::SurfaceConfiguration,
        ctx: &GpuContext,
    ) -> Self {
        let view = texture.texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("bae.surface.view"),
            format: Some(config.format),
            ..Default::default()
        });
        Self {
            texture,
            view,
            queue: ctx.queue().clone(),
            suboptimal: false,
        }
    }

    fn suboptimal(mut self) -> Self {
        self.suboptimal = true;
        self
    }

    /// The swapchain texture.
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture.texture
    }

    /// A view of the swapchain texture in the surface format.
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// `true` if the swapchain should be reconfigured after presenting.
    pub fn is_suboptimal(&self) -> bool {
        self.suboptimal
    }

    /// Presents the frame (after its command buffers were submitted).
    pub fn present(self) {
        let Self {
            texture,
            view,
            queue,
            ..
        } = self;
        drop(view);
        queue.present(texture);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::PresentMode as P;

    #[test]
    fn present_mode_fallbacks() {
        let metal = [P::Fifo, P::Immediate];
        assert_eq!(PresentModePreference::Vsync.pick(&metal), P::Fifo);
        assert_eq!(PresentModePreference::LowLatency.pick(&metal), P::Fifo);
        assert_eq!(
            PresentModePreference::LowLatency.pick(&[P::FifoRelaxed, P::Fifo]),
            P::Fifo
        );
        assert_eq!(PresentModePreference::NoVsync.pick(&metal), P::Immediate);
        let dx12 = [P::Fifo, P::FifoRelaxed, P::Mailbox, P::Immediate];
        assert_eq!(PresentModePreference::Vsync.pick(&dx12), P::FifoRelaxed);
        assert_eq!(PresentModePreference::LowLatency.pick(&dx12), P::Mailbox);
        assert_eq!(PresentModePreference::Vsync.pick(&[]), P::Fifo);
    }

    #[test]
    fn sdr_format_prefers_non_srgb_bgra() {
        use wgpu::TextureFormat as F;
        assert_eq!(
            pick_sdr_format(&[F::Bgra8UnormSrgb, F::Bgra8Unorm, F::Rgba16Float]),
            Some(F::Bgra8Unorm)
        );
        assert_eq!(
            pick_sdr_format(&[F::Rgba8UnormSrgb, F::Rgba8Unorm]),
            Some(F::Rgba8Unorm)
        );
        assert_eq!(
            pick_sdr_format(&[F::Rgba16Float, F::Bgra8UnormSrgb]),
            Some(F::Rgba16Float)
        );
        assert_eq!(pick_sdr_format(&[]), None);
    }
}
