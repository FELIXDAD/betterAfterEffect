//! Capability flags → BAE render-node requirements.
//!
//! An AE effect describes its behaviour through two flag words reported at
//! `PF_Cmd_GLOBAL_SETUP` (and mirrored in the PiPL), some of which can change
//! later through `PF_Cmd_QUERY_DYNAMIC_FLAGS` (SDK guide, "Effect Basics ›
//! PF_OutData › PF_OutFlags / PF_OutFlags2"). The P3 shim decodes the raw
//! words into [`EffectCaps`] (semantic booleans, no bit positions here);
//! [`EffectCaps::requirements`] then derives the [`NodeRequirements`] the
//! render graph (04) must honour for the node hosting this effect.
//!
//! Everything a node can require is *opt-in per node*: a project without AE
//! effects never constructs any of these and the graph pays nothing.

/// Semantic decode of the static and dynamic out-flags of one effect.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectCaps {
    // ---- render model --------------------------------------------------
    /// Uses the SmartFX pre-render/render protocol (`SUPPORTS_SMART_RENDER`).
    pub smart_render: bool,
    /// Handles 16 bpc worlds (`DEEP_COLOR_AWARE`).
    pub deep_color: bool,
    /// Handles 32 bpc float worlds (`FLOAT_COLOR_AWARE`; only honoured
    /// together with `smart_render`).
    pub float_color: bool,
    /// Can render on the GPU in 32-bit float (`SUPPORTS_GPU_RENDER_F32`).
    pub gpu_render_f32: bool,
    /// Supports the DirectX GPU framework (`SUPPORTS_DIRECTX_RENDERING`).
    pub gpu_directx: bool,

    // ---- threading / sequence data ------------------------------------
    /// Safe to render on several threads at once
    /// (`SUPPORTS_THREADED_RENDERING`, Multi-Frame Rendering).
    pub threaded_rendering: bool,
    /// Needs a private, writable copy of sequence data per render thread
    /// (`MUTABLE_RENDER_SEQUENCE_DATA_SLOWER`).
    pub mutable_render_sequence_data: bool,
    /// Implements `GET_FLATTENED_SEQUENCE_DATA`
    /// (`SUPPORTS_GET_FLATTENED_SEQUENCE_DATA`).
    pub get_flattened_sequence_data: bool,

    // ---- caching and dependencies -------------------------------------
    /// Reads params/layers at other times (`WIDE_TIME_INPUT`).
    pub wide_time_input: bool,
    /// Host tracks every checkout time (`AUTOMATIC_WIDE_TIME_INPUT`; only
    /// meaningful with `smart_render`).
    pub automatic_wide_time_input: bool,
    /// Output varies with something other than params and time — never reuse
    /// a frame rendered for a still (`NON_PARAM_VARY`, dynamic).
    pub non_param_vary: bool,
    /// Each output pixel depends only on the same input pixel
    /// (`PIX_INDEPENDENT`, dynamic).
    pub pixel_independent: bool,
    /// Output depends on the shutter angle (`I_USE_SHUTTER_ANGLE`, dynamic).
    pub uses_shutter_angle: bool,
    /// Reads the active 3D camera (`I_USE_3D_CAMERA`, dynamic).
    pub uses_3d_camera: bool,
    /// Reads 3D lights (`I_USE_3D_LIGHTS`, dynamic).
    pub uses_3d_lights: bool,
    /// Depends on comp start timecode / drop-frame (`I_USE_COMP_TIMECODE`).
    pub uses_comp_timecode: bool,
    /// Reads masks not referenced by a path param
    /// (`DEPENDS_ON_UNREFERENCED_MASKS`, dynamic).
    pub depends_on_unreferenced_masks: bool,
    /// Mixes its own bytes into the host cache key during pre-render
    /// (`I_MIX_GUID_DEPENDENCIES`).
    pub mix_guid_dependencies: bool,
    /// Output is watermarked (demo licence) (`OUTPUT_IS_WATERMARKED`, dynamic).
    pub output_is_watermarked: bool,
    /// Can turn zero-alpha pixels visible; the host must not trim them
    /// (`REVEALS_ZERO_ALPHA`, dynamic).
    pub reveals_zero_alpha: bool,
    /// Never changes fully opaque pixels (`PRESERVES_FULLY_OPAQUE_PIXELS`).
    pub preserves_fully_opaque_pixels: bool,
    /// Input may be trimmed to non-empty pixels (`DOESNT_NEED_EMPTY_PIXELS`,
    /// legacy render only).
    pub doesnt_need_empty_pixels: bool,
    /// Supports `QUERY_DYNAMIC_FLAGS`.
    pub query_dynamic_flags: bool,

    // ---- legacy buffer geometry (ignored for SmartFX) -------------------
    /// Grows its output buffer (`I_EXPAND_BUFFER`).
    pub expands_buffer: bool,
    /// Shrinks its output to the extent hint (`I_SHRINK_BUFFER`).
    pub shrinks_buffer: bool,
    /// Honours the output extent hint (`USE_OUTPUT_EXTENT`).
    pub uses_output_extent: bool,
    /// Writes into its input buffer (`I_WRITE_INPUT_BUFFER`).
    pub writes_input_buffer: bool,

    // ---- UI ------------------------------------------------------------
    /// Has custom UI and wants `PF_Cmd_EVENT` (`CUSTOM_UI`).
    pub custom_ui: bool,
    /// Wants `UPDATE_PARAMS_UI` (`SEND_UPDATE_PARAMS_UI`).
    pub send_update_params_ui: bool,
    /// Has an options dialog (`I_DO_DIALOG`).
    pub has_dialog: bool,
    /// Honours per-group start-collapsed state
    /// (`PARAM_GROUP_START_COLLAPSED_FLAG`).
    pub param_group_start_collapsed: bool,
    /// Needs its resources open outside global setup (`KEEP_RESOURCE_OPEN`).
    pub keep_resource_open: bool,
    /// Reports external dependencies (`I_HAVE_EXTERNAL_DEPENDENCIES`).
    pub has_external_dependencies: bool,
    /// Hidden from the effects menu, kept for old projects (`I_AM_OBSOLETE`).
    pub obsolete: bool,
    /// Listed under "Obsolete" (`I_AM_DEPRECATED`).
    pub deprecated: bool,
    /// Strings are UTF-8 rather than locale multibyte (`STRINGS_ARE_UTF8`,
    /// AE 27+).
    pub strings_utf8: bool,

    // ---- audio ---------------------------------------------------------
    /// Audio capabilities.
    pub audio: AudioCaps,
}

/// Audio-related capability flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AudioCaps {
    /// Alters audio as well as video (`AUDIO_EFFECT_TOO`).
    pub effect_too: bool,
    /// Alters only audio (`AUDIO_EFFECT_ONLY`).
    pub effect_only: bool,
    /// Requires float samples (`AUDIO_FLOAT_ONLY`).
    pub float_only: bool,
    /// IIR filter (`AUDIO_IIR`).
    pub iir: bool,
    /// Generates audio from silence (`I_SYNTHESIZE_AUDIO`).
    pub synthesizes: bool,
    /// Params depend on audio (`I_USE_AUDIO`).
    pub uses_audio: bool,
}

/// Per-call status flags a plug-in may set in out-data after any selector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CallOutFlags {
    /// Redraw effect controls, comp and layer viewers (`REFRESH_UI`).
    pub refresh_ui: bool,
    /// Something changed that requires a re-render (`FORCE_RERENDER`).
    pub force_rerender: bool,
    /// Discard the current legacy render (`NOP_RENDER`, during FRAME_SETUP).
    pub nop_render: bool,
    /// Show `return_msg` as an error (`DISPLAY_ERROR_MESSAGE`).
    pub display_error_message: bool,
    /// Show the dialog right after sequence setup (`SEND_DO_DIALOG`).
    pub send_do_dialog: bool,
}

/// How the render scheduler may call into the node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Concurrency {
    /// Any number of concurrent calls, on any render thread, sharing one
    /// read-only sequence-data snapshot.
    Parallel,
    /// Concurrent calls allowed, but every render thread needs its own
    /// writable copy of the instance state (discarded after each render span).
    ParallelWithPrivateState,
    /// One call at a time *per plug-in module* (all instances of every effect
    /// in that binary share globals): the AE behaviour for effects that do
    /// not opt into Multi-Frame Rendering.
    SerialPerModule,
}

/// How the node negotiates its region of interest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoiModel {
    /// SmartFX: ask the node which inputs (rect, time) it needs for a
    /// requested output rect; the node reports result and max-result rects.
    Negotiated,
    /// Legacy: the whole input layer is provided; the output may be resized
    /// (expand/shrink) and an extent hint is supplied.
    LegacyFullFrame {
        /// The node may grow its output beyond the input bounds.
        expands: bool,
        /// The node only fills the extent-hint rectangle.
        honours_extent: bool,
    },
}

/// Which CPU bit depths the node can process.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DepthSupport {
    /// 8 bpc (always true for valid effects).
    pub u8: bool,
    /// 16 bpc (0..=32768).
    pub u16: bool,
    /// 32 bpc float.
    pub f32: bool,
}

/// How the node depends on time beyond the current frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeDependency {
    /// Only the current time.
    CurrentOnly,
    /// Other times, untracked: invalidate the node's cache on *any* upstream
    /// change at *any* time.
    WideUntracked,
    /// Other times, tracked: the host records every (input, time) checkout of
    /// each render and invalidates precisely.
    WideTracked,
}

/// Extra inputs that must be folded into the node's cache key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheKeyInputs {
    /// Shutter angle and phase.
    pub shutter: bool,
    /// Active camera transform and settings.
    pub camera: bool,
    /// Light transforms and settings.
    pub lights: bool,
    /// Comp start timecode and drop-frame flag.
    pub comp_timecode: bool,
    /// Geometry of all masks on the layer (not only referenced ones).
    pub all_masks: bool,
    /// Opaque bytes supplied by the plug-in during pre-render.
    pub plugin_guid: bool,
}

/// What the render graph (04) must provide for one AE-backed effect node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeRequirements {
    /// Scheduler constraint.
    pub concurrency: Concurrency,
    /// ROI negotiation model.
    pub roi: RoiModel,
    /// May the host split the request into independent tiles?
    pub tileable: bool,
    /// CPU depths.
    pub depths: DepthSupport,
    /// GPU path available (float only).
    pub gpu: bool,
    /// Time dependency model.
    pub time: TimeDependency,
    /// Frames may be cached and reused for stills.
    pub cacheable_for_stills: bool,
    /// Extra cache-key inputs.
    pub cache_key: CacheKeyInputs,
    /// Host must keep RGB of zero-alpha input pixels (do not trim/zero them).
    pub keep_zero_alpha_rgb: bool,
    /// Node output is not for final delivery (watermarked); UI should badge it.
    pub watermarked: bool,
}

impl EffectCaps {
    /// Derive the render-graph requirements. Pure; re-run after every
    /// `QUERY_DYNAMIC_FLAGS` response.
    pub fn requirements(&self) -> NodeRequirements {
        let concurrency = match (self.threaded_rendering, self.mutable_render_sequence_data) {
            (false, _) => Concurrency::SerialPerModule,
            (true, false) => Concurrency::Parallel,
            (true, true) => Concurrency::ParallelWithPrivateState,
        };
        let roi = if self.smart_render {
            RoiModel::Negotiated
        } else {
            RoiModel::LegacyFullFrame {
                expands: self.expands_buffer,
                honours_extent: self.uses_output_extent,
            }
        };
        let time = if !(self.wide_time_input || self.automatic_wide_time_input) {
            TimeDependency::CurrentOnly
        } else if self.automatic_wide_time_input && self.smart_render {
            TimeDependency::WideTracked
        } else {
            // The SDK says AUTOMATIC_WIDE_TIME_INPUT without SmartFX is treated
            // as plain WIDE_TIME_INPUT.
            TimeDependency::WideUntracked
        };
        NodeRequirements {
            concurrency,
            roi,
            tileable: self.pixel_independent,
            depths: DepthSupport {
                u8: true,
                u16: self.deep_color,
                f32: self.float_color && self.smart_render,
            },
            gpu: self.gpu_render_f32 && self.smart_render,
            time,
            cacheable_for_stills: !self.non_param_vary,
            cache_key: CacheKeyInputs {
                shutter: self.uses_shutter_angle,
                camera: self.uses_3d_camera,
                lights: self.uses_3d_lights,
                comp_timecode: self.uses_comp_timecode,
                all_masks: self.depends_on_unreferenced_masks,
                plugin_guid: self.mix_guid_dependencies,
            },
            keep_zero_alpha_rgb: self.reveals_zero_alpha,
            watermarked: self.output_is_watermarked,
        }
    }

    /// Best CPU depth for a node whose graph works at `working` precision:
    /// the deepest supported depth not exceeding what the graph carries.
    /// BAE's working buffers are 16-bit float, so 32 bpc float is preferred
    /// (lossless for half floats), then 16 bpc, then 8 bpc.
    pub fn preferred_cpu_depth(&self) -> crate::pixel::PixelDepth {
        let r = self.requirements();
        if r.depths.f32 {
            crate::pixel::PixelDepth::F32
        } else if r.depths.u16 {
            crate::pixel::PixelDepth::U16
        } else {
            crate::pixel::PixelDepth::U8
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel::PixelDepth;

    #[test]
    fn legacy_8bit_effect_is_serial_full_frame() {
        let r = EffectCaps::default().requirements();
        assert_eq!(r.concurrency, Concurrency::SerialPerModule);
        assert_eq!(
            r.roi,
            RoiModel::LegacyFullFrame {
                expands: false,
                honours_extent: false
            }
        );
        assert!(!r.depths.f32 && !r.gpu && r.cacheable_for_stills);
        assert_eq!(EffectCaps::default().preferred_cpu_depth(), PixelDepth::U8);
    }

    #[test]
    fn float_and_tracked_time_require_smartfx() {
        let caps = EffectCaps {
            float_color: true,
            automatic_wide_time_input: true,
            wide_time_input: true,
            gpu_render_f32: true,
            ..Default::default()
        };
        let r = caps.requirements();
        assert!(!r.depths.f32 && !r.gpu);
        assert_eq!(r.time, TimeDependency::WideUntracked);
        let r = EffectCaps {
            smart_render: true,
            ..caps
        }
        .requirements();
        assert!(r.depths.f32 && r.gpu);
        assert_eq!(r.time, TimeDependency::WideTracked);
        assert_eq!(r.roi, RoiModel::Negotiated);
    }

    #[test]
    fn mfr_modes() {
        let c = EffectCaps {
            threaded_rendering: true,
            ..Default::default()
        };
        assert_eq!(c.requirements().concurrency, Concurrency::Parallel);
        let c = EffectCaps {
            mutable_render_sequence_data: true,
            ..c
        };
        assert_eq!(
            c.requirements().concurrency,
            Concurrency::ParallelWithPrivateState
        );
    }
}
