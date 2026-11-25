//! Rust mirror of the C ABI in `sdk/c/bae_plugin.h` (ABI 1.0).
//!
//! The header is authoritative; this module mirrors the parts a host and a
//! Rust effect plugin need: versioning and status codes, the entry point and
//! exports, parameters, CPU images, the effect vtable and the `bae.core`,
//! `bae.param.define` and `bae.render` suites. Layouts are checked against the
//! sizes and offsets that `sdk/c/tests/abi_check.c` asserts for the header
//! (`tests` below). Structs that only appear behind pointers here (GPU,
//! instances, supervision) are opaque.
//!
//! Naming follows the header (`BaeHost`, `BAE_OK`, …) so code can be compared
//! with C side by side.
#![allow(non_camel_case_types, missing_docs)]

use core::ffi::{c_char, c_void};

// ---------------------------------------------------------------------------
// 1. Versioning
// ---------------------------------------------------------------------------

pub const BAE_ABI_VERSION_MAJOR: u32 = 1;
pub const BAE_ABI_VERSION_MINOR: u32 = 0;

/// `BAE_MAKE_VERSION(major, minor)`.
pub const fn bae_make_version(major: u32, minor: u32) -> u32 {
    (major << 16) | (minor & 0xFFFF)
}

pub const BAE_ABI_VERSION: u32 = bae_make_version(BAE_ABI_VERSION_MAJOR, BAE_ABI_VERSION_MINOR);

/// `BAE_HAS_FIELD`: does a struct whose producer set `struct_size` to
/// `struct_size` carry the field that ends at `offset + size`?
pub const fn has_field(struct_size: u32, offset: usize, size: usize) -> bool {
    struct_size as usize >= offset + size
}

// ---------------------------------------------------------------------------
// 3. Status codes
// ---------------------------------------------------------------------------

pub type BaeStatus = i32;

pub const BAE_OK: BaeStatus = 0;
pub const BAE_STATUS_FALLBACK: BaeStatus = 1;
pub const BAE_ERR_GENERIC: BaeStatus = -1;
pub const BAE_ERR_INVALID_ARGUMENT: BaeStatus = -2;
pub const BAE_ERR_OUT_OF_MEMORY: BaeStatus = -3;
pub const BAE_ERR_UNSUPPORTED: BaeStatus = -4;
pub const BAE_ERR_VERSION_MISMATCH: BaeStatus = -5;
pub const BAE_ERR_NOT_FOUND: BaeStatus = -6;
pub const BAE_ERR_ABORTED: BaeStatus = -7;
pub const BAE_ERR_BAD_STATE: BaeStatus = -8;
pub const BAE_ERR_PERMISSION_DENIED: BaeStatus = -9;
pub const BAE_ERR_BUFFER_TOO_SMALL: BaeStatus = -10;
pub const BAE_ERR_IO: BaeStatus = -11;
pub const BAE_ERR_PARSE: BaeStatus = -12;
pub const BAE_ERR_GPU_DEVICE_LOST: BaeStatus = -13;
pub const BAE_ERR_GPU: BaeStatus = -14;
pub const BAE_ERR_PANIC: BaeStatus = -15;
pub const BAE_ERR_TIMEOUT: BaeStatus = -16;
pub const BAE_ERR_INTERNAL: BaeStatus = -17;

/// Name of a status code (`BAE_ERR_PANIC` → `"panic"`), for messages.
pub fn status_name(status: BaeStatus) -> &'static str {
    match status {
        BAE_OK => "ok",
        BAE_STATUS_FALLBACK => "fallback",
        BAE_ERR_GENERIC => "error",
        BAE_ERR_INVALID_ARGUMENT => "invalid argument",
        BAE_ERR_OUT_OF_MEMORY => "out of memory",
        BAE_ERR_UNSUPPORTED => "unsupported",
        BAE_ERR_VERSION_MISMATCH => "version mismatch",
        BAE_ERR_NOT_FOUND => "not found",
        BAE_ERR_ABORTED => "aborted",
        BAE_ERR_BAD_STATE => "bad state",
        BAE_ERR_PERMISSION_DENIED => "permission denied",
        BAE_ERR_BUFFER_TOO_SMALL => "buffer too small",
        BAE_ERR_IO => "I/O error",
        BAE_ERR_PARSE => "parse error",
        BAE_ERR_GPU_DEVICE_LOST => "GPU device lost",
        BAE_ERR_GPU => "GPU error",
        BAE_ERR_PANIC => "panic",
        BAE_ERR_TIMEOUT => "timeout",
        BAE_ERR_INTERNAL => "internal error",
        _ => "unknown status",
    }
}

// ---------------------------------------------------------------------------
// 4. Basic types
// ---------------------------------------------------------------------------

pub type BaeBool = u32;
pub const BAE_FALSE: BaeBool = 0;
pub const BAE_TRUE: BaeBool = 1;

pub type BaeFlicks = i64;
pub const BAE_FLICKS_PER_SECOND: i64 = 705_600_000;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BaeRational {
    pub num: i32,
    pub den: i32,
}

/// Half-open `[x0, x1) × [y0, y1)`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BaeRect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl BaeRect {
    pub const fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    pub const fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    pub const fn width(&self) -> u32 {
        if self.x1 > self.x0 {
            (self.x1 - self.x0) as u32
        } else {
            0
        }
    }

    pub const fn height(&self) -> u32 {
        if self.y1 > self.y0 {
            (self.y1 - self.y0) as u32
        } else {
            0
        }
    }

    /// Intersection (may be empty).
    pub fn intersect(&self, o: &BaeRect) -> BaeRect {
        BaeRect::new(
            self.x0.max(o.x0),
            self.y0.max(o.y0),
            self.x1.min(o.x1),
            self.y1.min(o.y1),
        )
    }
}

pub const BAE_COORD_MIN: i32 = -(1 << 30);
pub const BAE_COORD_MAX: i32 = 1 << 30;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeStr {
    pub ptr: *const c_char,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeBytes {
    pub ptr: *const c_void,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeWriter {
    pub opaque: *mut c_void,
    pub write: Option<unsafe extern "C" fn(*mut c_void, *const c_void, usize) -> BaeStatus>,
}

macro_rules! opaque {
    ($($name:ident),* $(,)?) => {$(
        #[repr(C)]
        pub struct $name {
            _private: [u8; 0],
        }
    )*};
}

opaque!(
    BaeHostHandle,
    BaeParamBuilder,
    BaeParamEditor,
    BaeRenderCtx,
    BaeGpuSetupCtx,
    BaeGpuImage,
    // Argument structs this mirror does not spell out (GPU, instances,
    // supervision): only ever passed by pointer.
    BaeInstanceArgs,
    BaeGpuDeviceArgs,
    BaeGpuRenderArgs,
    BaeParamChangedArgs,
    BaeParamUiArgs,
);

// ---------------------------------------------------------------------------
// 5. Host, entry point, exports
// ---------------------------------------------------------------------------

pub const BAE_HOST_CAP_GPU_COMPUTE: u64 = 1 << 0;
pub const BAE_HOST_CAP_GPU_NATIVE: u64 = 1 << 1;
pub const BAE_HOST_CAP_OUT_OF_PROCESS: u64 = 1 << 2;
pub const BAE_HOST_CAP_HEADLESS: u64 = 1 << 3;

pub type GetSuiteFn = unsafe extern "C" fn(
    host: *mut BaeHostHandle,
    name: *const c_char,
    version: u32,
) -> *const c_void;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeHost {
    pub struct_size: u32,
    pub abi_version: u32,
    pub capabilities: u64,
    pub host_name: *const c_char,
    pub host_version: *const c_char,
    pub plugin_id: *const c_char,
    pub plugin_dir: *const c_char,
    pub handle: *mut BaeHostHandle,
    pub get_suite: Option<GetSuiteFn>,
}

pub const BAE_KIND_EFFECT: u32 = 1;
pub const BAE_KIND_SHAPE_OPERATOR: u32 = 2;
pub const BAE_KIND_TEXT_SELECTOR: u32 = 3;
pub const BAE_KIND_IMPORTER: u32 = 4;
pub const BAE_KIND_EXPORTER: u32 = 5;
pub const BAE_KIND_DECODER: u32 = 6;
pub const BAE_KIND_ENCODER: u32 = 7;
pub const BAE_KIND_COMMAND: u32 = 8;
pub const BAE_KIND_PANEL: u32 = 9;
pub const BAE_KIND_EXPRESSION_LIBRARY: u32 = 10;
pub const BAE_KIND_AUDIO_EFFECT: u32 = 11;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeContribution {
    pub struct_size: u32,
    pub kind: u32,
    pub id: *const c_char,
    pub vtable_version: u32,
    pub reserved: u32,
    pub vtable: *const c_void,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaePluginExports {
    pub struct_size: u32,
    pub abi_built: u32,
    pub abi_required: u32,
    pub reserved0: u32,
    pub plugin_id: *const c_char,
    pub plugin_version: *const c_char,
    pub plugin_data: *mut c_void,
    pub contribution_count: u32,
    pub reserved1: u32,
    pub contributions: *const BaeContribution,
    pub shutdown: Option<unsafe extern "C" fn(plugin_data: *mut c_void)>,
    pub get_suite: Option<
        unsafe extern "C" fn(
            plugin_data: *mut c_void,
            name: *const c_char,
            version: u32,
        ) -> *const c_void,
    >,
}

/// `bae_plugin_entry`.
pub const BAE_PLUGIN_ENTRY_NAME: &str = "bae_plugin_entry";
pub type BaePluginEntryFn =
    unsafe extern "C" fn(host: *const BaeHost, out_exports: *mut BaePluginExports) -> BaeStatus;

// ---------------------------------------------------------------------------
// 6. Pixels, images
// ---------------------------------------------------------------------------

pub type BaePixelFormat = u32;
pub const BAE_CHANNELS_RGBA: u32 = 1;
pub const BAE_CHANNELS_BGRA: u32 = 2;
pub const BAE_CHANNELS_ARGB: u32 = 3;
pub const BAE_COMPONENT_U8: u32 = 1;
pub const BAE_COMPONENT_U16: u32 = 2;
pub const BAE_COMPONENT_U16_15BIT: u32 = 3;
pub const BAE_COMPONENT_F16: u32 = 4;
pub const BAE_COMPONENT_F32: u32 = 5;
pub const BAE_ALPHA_PREMULTIPLIED: u32 = 1;
pub const BAE_ALPHA_STRAIGHT: u32 = 2;

/// `BAE_PIXEL_FORMAT(channels, component, alpha)`.
pub const fn bae_pixel_format(channels: u32, component: u32, alpha: u32) -> BaePixelFormat {
    channels | (component << 8) | (alpha << 16)
}

pub const BAE_PIXEL_FORMAT_RGBA_F16_PREMUL: BaePixelFormat = bae_pixel_format(
    BAE_CHANNELS_RGBA,
    BAE_COMPONENT_F16,
    BAE_ALPHA_PREMULTIPLIED,
);
pub const BAE_PIXEL_FORMAT_RGBA_F32_PREMUL: BaePixelFormat = bae_pixel_format(
    BAE_CHANNELS_RGBA,
    BAE_COMPONENT_F32,
    BAE_ALPHA_PREMULTIPLIED,
);
pub const BAE_PIXEL_FORMAT_RGBA_U8_PREMUL: BaePixelFormat =
    bae_pixel_format(BAE_CHANNELS_RGBA, BAE_COMPONENT_U8, BAE_ALPHA_PREMULTIPLIED);

pub const BAE_COLOR_SPACE_UNKNOWN: u32 = 0;
pub const BAE_COLOR_SPACE_SRGB: u32 = 1;
pub const BAE_COLOR_SPACE_LINEAR_SRGB: u32 = 2;

pub const BAE_IMAGE_FLAG_WRITABLE: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeCpuImage {
    pub struct_size: u32,
    pub format: BaePixelFormat,
    pub rect: BaeRect,
    pub row_bytes: i64,
    pub data: *mut c_void,
    pub flags: u32,
    pub reserved: u32,
}

// ---------------------------------------------------------------------------
// 7. Parameters
// ---------------------------------------------------------------------------

pub type BaeParamType = u32;
pub const BAE_PARAM_FLOAT: u32 = 1;
pub const BAE_PARAM_INT: u32 = 2;
pub const BAE_PARAM_ANGLE: u32 = 3;
pub const BAE_PARAM_CHECKBOX: u32 = 4;
pub const BAE_PARAM_COLOR: u32 = 5;
pub const BAE_PARAM_POINT: u32 = 6;
pub const BAE_PARAM_POINT_3D: u32 = 7;
pub const BAE_PARAM_POPUP: u32 = 8;
pub const BAE_PARAM_LAYER: u32 = 9;
pub const BAE_PARAM_GROUP: u32 = 10;
pub const BAE_PARAM_ARBITRARY: u32 = 11;
pub const BAE_PARAM_PATH: u32 = 12;
pub const BAE_PARAM_BUTTON: u32 = 13;
pub const BAE_PARAM_STRING: u32 = 14;
pub const BAE_PARAM_ASSET: u32 = 15;

pub const BAE_PARAM_FLAG_HIDDEN: u64 = 1 << 0;
pub const BAE_PARAM_FLAG_DISABLED: u64 = 1 << 1;
pub const BAE_PARAM_FLAG_SUPERVISE: u64 = 1 << 2;
pub const BAE_PARAM_FLAG_NOT_ANIMATABLE: u64 = 1 << 3;
pub const BAE_PARAM_FLAG_HOLD_INTERPOLATION: u64 = 1 << 4;
pub const BAE_PARAM_FLAG_START_COLLAPSED: u64 = 1 << 5;
pub const BAE_PARAM_FLAG_UI_ONLY: u64 = 1 << 6;
pub const BAE_PARAM_FLAG_NO_EXPRESSIONS: u64 = 1 << 7;
pub const BAE_PARAM_FLAG_TRANSITION_PROGRESS: u64 = 1 << 8;
pub const BAE_PARAM_FLAG_RELATIVE_DEFAULT: u64 = 1 << 9;

pub const BAE_DISPLAY_NUMBER: u32 = 0;
pub const BAE_DISPLAY_PERCENT: u32 = 1;
pub const BAE_DISPLAY_PIXELS: u32 = 2;
pub const BAE_DISPLAY_DEGREES: u32 = 3;
pub const BAE_DISPLAY_SECONDS: u32 = 4;
pub const BAE_DISPLAY_FRAMES: u32 = 5;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct BaeParamFloatDef {
    pub default_value: f64,
    pub valid_min: f64,
    pub valid_max: f64,
    pub slider_min: f64,
    pub slider_max: f64,
    pub precision: u32,
    pub display: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct BaeParamIntDef {
    pub default_value: i32,
    pub valid_min: i32,
    pub valid_max: i32,
    pub slider_min: i32,
    pub slider_max: i32,
    pub display: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct BaeParamAngleDef {
    pub default_degrees: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeParamCheckboxDef {
    pub default_value: BaeBool,
    pub reserved: u32,
    pub label: *const c_char,
}

pub const BAE_COLOR_DEF_HAS_ALPHA: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct BaeParamColorDef {
    pub default_rgba: [f32; 4],
    pub flags: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct BaeParamPointDef {
    pub default_xyz: [f64; 3],
    pub reserved: [u32; 2],
}

pub const BAE_CHOICE_FLAG_SEPARATOR: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaePopupChoice {
    pub id: u32,
    pub flags: u32,
    pub name: *const c_char,
    pub l10n_key: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeParamPopupDef {
    pub default_id: u32,
    pub choice_count: u32,
    pub choices: *const BaePopupChoice,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct BaeParamLayerDef {
    pub default_mode: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeParamButtonDef {
    pub label: *const c_char,
}

/// The type-specific part of [`BaeParamDef`] (ARBITRARY, STRING and ASSET
/// definitions are reachable through `reserved`).
#[repr(C)]
#[derive(Clone, Copy)]
pub union BaeParamDefUnion {
    pub float_def: BaeParamFloatDef,
    pub int_def: BaeParamIntDef,
    pub angle_def: BaeParamAngleDef,
    pub checkbox_def: BaeParamCheckboxDef,
    pub color_def: BaeParamColorDef,
    pub point_def: BaeParamPointDef,
    pub popup_def: BaeParamPopupDef,
    pub layer_def: BaeParamLayerDef,
    pub button_def: BaeParamButtonDef,
    pub reserved: [u8; 128],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BaeParamDef {
    pub struct_size: u32,
    pub type_: BaeParamType,
    pub id: u32,
    pub group_id: u32,
    pub key: *const c_char,
    pub name: *const c_char,
    pub l10n_key: *const c_char,
    pub flags: u64,
    pub u: BaeParamDefUnion,
}

impl BaeParamDef {
    /// A zeroed definition with `struct_size` set.
    pub fn zeroed() -> Self {
        Self {
            struct_size: core::mem::size_of::<Self>() as u32,
            type_: 0,
            id: 0,
            group_id: 0,
            key: core::ptr::null(),
            name: core::ptr::null(),
            l10n_key: core::ptr::null(),
            flags: 0,
            u: BaeParamDefUnion { reserved: [0; 128] },
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct BaeLayerRef {
    pub handle: u64,
    pub flags: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union BaeParamValueUnion {
    pub f: f64,
    pub i: i32,
    pub b: BaeBool,
    pub color: [f32; 4],
    pub point: [f64; 3],
    pub choice: u32,
    pub layer: BaeLayerRef,
    pub bytes: BaeBytes,
    pub str_: BaeStr,
    pub handle: u64,
    pub reserved: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BaeParamValue {
    pub id: u32,
    pub type_: BaeParamType,
    pub v: BaeParamValueUnion,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeParamValues {
    pub count: u32,
    pub reserved: u32,
    pub values: *const BaeParamValue,
}

// ---------------------------------------------------------------------------
// 8. Effects
// ---------------------------------------------------------------------------

pub const BAE_EFFECT_CLASS_FILTER: u32 = 1;
pub const BAE_EFFECT_CLASS_GENERATOR: u32 = 2;
pub const BAE_EFFECT_CLASS_TRANSITION: u32 = 3;

pub const BAE_EFFECT_FLAG_PIXEL_INDEPENDENT: u64 = 1 << 0;
pub const BAE_EFFECT_FLAG_EXPANDS_BUFFER: u64 = 1 << 1;
pub const BAE_EFFECT_FLAG_SHRINKS_BUFFER: u64 = 1 << 2;
pub const BAE_EFFECT_FLAG_NON_PARAM_VARY: u64 = 1 << 3;
pub const BAE_EFFECT_FLAG_WIDE_TIME_INPUT: u64 = 1 << 4;
pub const BAE_EFFECT_FLAG_NON_DETERMINISTIC: u64 = 1 << 8;
pub const BAE_EFFECT_FLAG_STATEFUL: u64 = 1 << 10;
pub const BAE_EFFECT_FLAG_GPU_NATIVE: u64 = 1 << 13;

pub const BAE_THREADING_SERIAL_GLOBAL: u32 = 0;
pub const BAE_THREADING_SERIAL_INSTANCE: u32 = 1;
pub const BAE_THREADING_CONCURRENT: u32 = 2;

pub const BAE_DEVICE_CPU: u32 = 1;
pub const BAE_DEVICE_GPU: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeEffectDescription {
    pub struct_size: u32,
    pub effect_class: u32,
    pub flags: u64,
    pub threading: u32,
    pub devices: u32,
    pub gpu_formats: u32,
    pub cpu_format_count: u32,
    pub cpu_formats: *const BaePixelFormat,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeGlobalSetupArgs {
    pub struct_size: u32,
    pub reserved: u32,
    pub host: *const BaeHost,
    pub contribution_id: *const c_char,
    pub locale: *const c_char,
}

pub const BAE_QUALITY_DRAFT: u32 = 0;
pub const BAE_QUALITY_HIGH: u32 = 1;

pub const BAE_RENDER_FLAG_LINEAR_BLENDING: u32 = 1;
pub const BAE_RENDER_FLAG_FINAL: u32 = 2;
pub const BAE_RENDER_FLAG_PREVIEW: u32 = 4;
pub const BAE_RENDER_FLAG_HEADLESS: u32 = 8;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeRenderInfo {
    pub struct_size: u32,
    pub quality: u32,
    pub time: BaeFlicks,
    pub time_step: BaeFlicks,
    pub frame_rate: BaeRational,
    pub downsample_x: BaeRational,
    pub downsample_y: BaeRational,
    pub pixel_aspect: BaeRational,
    pub shutter_angle: f32,
    pub shutter_phase: f32,
    pub color_space: u32,
    pub flags: u32,
    pub layer_bounds: BaeRect,
    pub render_id: u64,
    pub params: BaeParamValues,
    pub layer_duration: BaeFlicks,
}

pub const BAE_INPUT_SELF: u32 = 0;
pub const BAE_INPUT_PARAM_LAYER: u32 = 1;
pub const BAE_INPUT_TRANSITION_TO: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeInputRequest {
    pub struct_size: u32,
    pub source: u32,
    pub param_id: u32,
    pub checkout_id: u32,
    pub time: BaeFlicks,
    pub rect: BaeRect,
    pub flags: u32,
    pub reserved: u32,
}

pub const BAE_INPUT_INFO_EMPTY: u32 = 1;
pub const BAE_INPUT_INFO_SOLID: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct BaeInputInfo {
    pub struct_size: u32,
    pub flags: u32,
    pub result_rect: BaeRect,
    pub max_result_rect: BaeRect,
    pub layer_bounds: BaeRect,
    pub pixel_aspect: BaeRational,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaePreRenderArgs {
    pub struct_size: u32,
    pub device: u32,
    pub info: *const BaeRenderInfo,
    pub ctx: *mut BaeRenderCtx,
    pub request_rect: BaeRect,
    pub flags: u32,
    pub reserved: u32,
}

pub const BAE_PRERENDER_SOLID: u32 = 1;
pub const BAE_PRERENDER_GPU_POSSIBLE: u32 = 2;
pub const BAE_PRERENDER_RETURNS_EXTRA_PIXELS: u32 = 4;
pub const BAE_PRERENDER_IDENTITY: u32 = 8;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaePreRenderResult {
    pub struct_size: u32,
    pub flags: u32,
    pub result_rect: BaeRect,
    pub max_result_rect: BaeRect,
    pub frame_data: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeCpuRenderArgs {
    pub struct_size: u32,
    pub reserved: u32,
    pub info: *const BaeRenderInfo,
    pub ctx: *mut BaeRenderCtx,
    pub frame_data: *mut c_void,
    pub output: *mut BaeCpuImage,
}

/// Effect vtable, `vtable_version` 1. Required: `global_setup` and one of
/// `render_cpu` / `render_gpu`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeEffectVTable1 {
    pub struct_size: u32,
    pub reserved: u32,
    pub global_setup: Option<
        unsafe extern "C" fn(
            plugin_data: *mut c_void,
            args: *const BaeGlobalSetupArgs,
            out_desc: *mut BaeEffectDescription,
            out_global: *mut *mut c_void,
        ) -> BaeStatus,
    >,
    pub global_setdown: Option<unsafe extern "C" fn(global: *mut c_void)>,
    pub define_params: Option<
        unsafe extern "C" fn(global: *mut c_void, builder: *mut BaeParamBuilder) -> BaeStatus,
    >,
    pub gpu_device_setup: Option<
        unsafe extern "C" fn(
            global: *mut c_void,
            args: *const BaeGpuDeviceArgs,
            out_gpu_data: *mut *mut c_void,
        ) -> BaeStatus,
    >,
    pub gpu_device_setdown: Option<
        unsafe extern "C" fn(
            global: *mut c_void,
            args: *const BaeGpuDeviceArgs,
            gpu_data: *mut c_void,
        ),
    >,
    pub instance_create: Option<
        unsafe extern "C" fn(
            global: *mut c_void,
            args: *const BaeInstanceArgs,
            out_instance: *mut *mut c_void,
        ) -> BaeStatus,
    >,
    pub instance_serialize: Option<
        unsafe extern "C" fn(
            global: *mut c_void,
            instance: *const c_void,
            out: *mut BaeWriter,
        ) -> BaeStatus,
    >,
    pub instance_destroy: Option<unsafe extern "C" fn(global: *mut c_void, instance: *mut c_void)>,
    pub param_changed: Option<
        unsafe extern "C" fn(
            global: *mut c_void,
            instance: *mut c_void,
            args: *const BaeParamChangedArgs,
            out_flags: *mut u32,
        ) -> BaeStatus,
    >,
    pub update_params_ui: Option<
        unsafe extern "C" fn(
            global: *mut c_void,
            instance: *const c_void,
            args: *const BaeParamUiArgs,
        ) -> BaeStatus,
    >,
    pub query_dynamic_flags: Option<
        unsafe extern "C" fn(
            global: *mut c_void,
            instance: *const c_void,
            values: *const BaeParamValues,
            inout_flags: *mut u64,
        ) -> BaeStatus,
    >,
    pub pre_render: Option<
        unsafe extern "C" fn(
            global: *mut c_void,
            instance: *const c_void,
            args: *const BaePreRenderArgs,
            out_result: *mut BaePreRenderResult,
        ) -> BaeStatus,
    >,
    pub render_cpu: Option<
        unsafe extern "C" fn(
            global: *mut c_void,
            instance: *const c_void,
            args: *const BaeCpuRenderArgs,
        ) -> BaeStatus,
    >,
    pub render_gpu: Option<
        unsafe extern "C" fn(
            global: *mut c_void,
            instance: *const c_void,
            gpu_data: *mut c_void,
            args: *const BaeGpuRenderArgs,
        ) -> BaeStatus,
    >,
    pub release_frame_data: Option<
        unsafe extern "C" fn(global: *mut c_void, instance: *const c_void, frame_data: *mut c_void),
    >,
    pub get_dependencies: Option<
        unsafe extern "C" fn(
            global: *mut c_void,
            instance: *const c_void,
            values: *const BaeParamValues,
            out_json: *mut BaeWriter,
        ) -> BaeStatus,
    >,
    pub message: Option<
        unsafe extern "C" fn(
            global: *mut c_void,
            instance: *mut c_void,
            selector: *const c_char,
            input: BaeBytes,
            output: *mut BaeWriter,
        ) -> BaeStatus,
    >,
}

impl BaeEffectVTable1 {
    /// A vtable with every callback NULL.
    pub const EMPTY: BaeEffectVTable1 = BaeEffectVTable1 {
        struct_size: core::mem::size_of::<BaeEffectVTable1>() as u32,
        reserved: 0,
        global_setup: None,
        global_setdown: None,
        define_params: None,
        gpu_device_setup: None,
        gpu_device_setdown: None,
        instance_create: None,
        instance_serialize: None,
        instance_destroy: None,
        param_changed: None,
        update_params_ui: None,
        query_dynamic_flags: None,
        pre_render: None,
        render_cpu: None,
        render_gpu: None,
        release_frame_data: None,
        get_dependencies: None,
        message: None,
    };
}

// ---------------------------------------------------------------------------
// 13. Host suites
// ---------------------------------------------------------------------------

pub const BAE_SUITE_CORE: &core::ffi::CStr = c"bae.core";
pub const BAE_SUITE_CORE_VERSION: u32 = 1;
pub const BAE_SUITE_PARAM_DEFINE: &core::ffi::CStr = c"bae.param.define";
pub const BAE_SUITE_PARAM_DEFINE_VERSION: u32 = 1;
pub const BAE_SUITE_RENDER: &core::ffi::CStr = c"bae.render";
pub const BAE_SUITE_RENDER_VERSION: u32 = 1;

pub const BAE_LOG_ERROR: u32 = 1;
pub const BAE_LOG_WARN: u32 = 2;
pub const BAE_LOG_INFO: u32 = 3;
pub const BAE_LOG_DEBUG: u32 = 4;
pub const BAE_LOG_TRACE: u32 = 5;

pub const BAE_PATH_PLUGIN_DIR: u32 = 1;
pub const BAE_PATH_DATA_DIR: u32 = 2;
pub const BAE_PATH_CACHE_DIR: u32 = 3;
pub const BAE_PATH_TEMP_DIR: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeCoreSuite1 {
    pub log:
        Option<unsafe extern "C" fn(host: *mut BaeHostHandle, level: u32, message: *const c_char)>,
    pub set_error: Option<unsafe extern "C" fn(host: *mut BaeHostHandle, message: *const c_char)>,
    pub alloc: Option<
        unsafe extern "C" fn(host: *mut BaeHostHandle, size: usize, align: usize) -> *mut c_void,
    >,
    pub free: Option<unsafe extern "C" fn(host: *mut BaeHostHandle, ptr: *mut c_void)>,
    pub has_permission: Option<
        unsafe extern "C" fn(host: *mut BaeHostHandle, permission: *const c_char) -> BaeBool,
    >,
    pub get_path: Option<
        unsafe extern "C" fn(
            host: *mut BaeHostHandle,
            which: u32,
            out_utf8: *mut BaeWriter,
        ) -> BaeStatus,
    >,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeParamDefineSuite1 {
    pub add: Option<
        unsafe extern "C" fn(builder: *mut BaeParamBuilder, def: *const BaeParamDef) -> BaeStatus,
    >,
}

pub type BaeParallelFn =
    unsafe extern "C" fn(user: *mut c_void, index: u32, thread_index: u32) -> BaeStatus;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BaeRenderSuite1 {
    pub checkout_input: Option<
        unsafe extern "C" fn(
            ctx: *mut BaeRenderCtx,
            request: *const BaeInputRequest,
            out_info: *mut BaeInputInfo,
        ) -> BaeStatus,
    >,
    pub checkout_param: Option<
        unsafe extern "C" fn(
            ctx: *mut BaeRenderCtx,
            param_id: u32,
            time: BaeFlicks,
            out_value: *mut BaeParamValue,
        ) -> BaeStatus,
    >,
    pub get_input_cpu: Option<
        unsafe extern "C" fn(
            ctx: *mut BaeRenderCtx,
            checkout_id: u32,
            out_image: *mut BaeCpuImage,
        ) -> BaeStatus,
    >,
    pub get_input_gpu: Option<
        unsafe extern "C" fn(
            ctx: *mut BaeRenderCtx,
            checkout_id: u32,
            out_image: *mut *mut BaeGpuImage,
        ) -> BaeStatus,
    >,
    pub is_aborted: Option<unsafe extern "C" fn(ctx: *mut BaeRenderCtx) -> BaeBool>,
    pub report_progress:
        Option<unsafe extern "C" fn(ctx: *mut BaeRenderCtx, done: u64, total: u64)>,
    pub alloc_cpu_image: Option<
        unsafe extern "C" fn(
            ctx: *mut BaeRenderCtx,
            format: BaePixelFormat,
            rect: BaeRect,
            out_image: *mut BaeCpuImage,
        ) -> BaeStatus,
    >,
    pub parallel_for: Option<
        unsafe extern "C" fn(
            ctx: *mut BaeRenderCtx,
            count: u32,
            f: BaeParallelFn,
            user: *mut c_void,
        ) -> BaeStatus,
    >,
}

#[cfg(test)]
mod tests {
    use core::mem::{align_of, offset_of, size_of};

    use super::*;

    /// The sizes and offsets `sdk/c/tests/abi_check.c` asserts for the header.
    #[test]
    fn layouts_match_the_header() {
        assert_eq!(size_of::<BaeRational>(), 8);
        assert_eq!(size_of::<BaeRect>(), 16);
        assert_eq!(size_of::<BaeStr>(), 16);
        assert_eq!(size_of::<BaeBytes>(), 16);
        assert_eq!(size_of::<BaeWriter>(), 16);
        assert_eq!(size_of::<BaeHost>(), 64);
        assert_eq!(align_of::<BaeHost>(), 8);
        assert_eq!(size_of::<BaeContribution>(), 32);
        assert_eq!(size_of::<BaePluginExports>(), 72);
        assert_eq!(size_of::<BaeCpuImage>(), 48);
        assert_eq!(size_of::<BaeParamFloatDef>(), 48);
        assert_eq!(size_of::<BaeParamIntDef>(), 24);
        assert_eq!(size_of::<BaeParamAngleDef>(), 8);
        assert_eq!(size_of::<BaeParamCheckboxDef>(), 16);
        assert_eq!(offset_of!(BaeParamCheckboxDef, label), 8);
        assert_eq!(size_of::<BaeParamColorDef>(), 24);
        assert_eq!(offset_of!(BaeParamColorDef, flags), 16);
        assert_eq!(size_of::<BaeParamPointDef>(), 32);
        assert_eq!(offset_of!(BaeParamPointDef, reserved), 24);
        assert_eq!(size_of::<BaePopupChoice>(), 24);
        assert_eq!(size_of::<BaeParamPopupDef>(), 16);
        assert_eq!(size_of::<BaeParamLayerDef>(), 8);
        assert_eq!(size_of::<BaeParamButtonDef>(), 8);
        assert_eq!(size_of::<BaeParamDef>(), 176);
        assert_eq!(align_of::<BaeParamDef>(), 8);
        assert_eq!(offset_of!(BaeParamDef, u), 48);
        assert_eq!(size_of::<BaeLayerRef>(), 16);
        assert_eq!(size_of::<BaeParamValue>(), 40);
        assert_eq!(offset_of!(BaeParamValue, type_), 4);
        assert_eq!(offset_of!(BaeParamValue, v), 8);
        assert_eq!(size_of::<BaeParamValues>(), 16);
        assert_eq!(offset_of!(BaeParamValues, values), 8);
        assert_eq!(size_of::<BaeEffectDescription>(), 40);
        assert_eq!(size_of::<BaeGlobalSetupArgs>(), 32);
        assert_eq!(size_of::<BaeRenderInfo>(), 120);
        assert_eq!(align_of::<BaeRenderInfo>(), 8);
        assert_eq!(offset_of!(BaeRenderInfo, layer_bounds), 72);
        assert_eq!(offset_of!(BaeRenderInfo, params), 96);
        assert_eq!(size_of::<BaeInputRequest>(), 48);
        assert_eq!(size_of::<BaeInputInfo>(), 64);
        assert_eq!(size_of::<BaePreRenderArgs>(), 48);
        assert_eq!(size_of::<BaePreRenderResult>(), 48);
        assert_eq!(size_of::<BaeCpuRenderArgs>(), 40);
        assert_eq!(size_of::<BaeEffectVTable1>(), 144);
        assert_eq!(offset_of!(BaeEffectVTable1, render_cpu), 8 + 12 * 8);
        assert_eq!(size_of::<BaeCoreSuite1>(), 48);
        assert_eq!(size_of::<BaeParamDefineSuite1>(), 8);
        assert_eq!(size_of::<BaeRenderSuite1>(), 64);
    }

    #[test]
    fn constants_match_the_header() {
        assert_eq!(BAE_ABI_VERSION, 0x0001_0000);
        assert_eq!(BAE_PIXEL_FORMAT_RGBA_F32_PREMUL, 0x0001_0501);
        assert_eq!(BAE_PIXEL_FORMAT_RGBA_U8_PREMUL, 0x0001_0101);
        assert_eq!(BAE_COORD_MAX, 1 << 30);
    }

    /// Every `#define BAE_ERR_*`, `BAE_PARAM_*` type, `BAE_KIND_*` and the ABI
    /// version in the header has the same value here (drift test, both directions
    /// for the names this module mirrors).
    #[test]
    fn header_defines_match() {
        let header = include_str!("../../../sdk/c/bae_plugin.h");
        let define = |name: &str| -> Option<i64> {
            header.lines().find_map(|l| {
                let rest = l.trim().strip_prefix("#define ")?;
                let (n, v) = rest.split_once(char::is_whitespace)?;
                if n != name {
                    return None;
                }
                let v = v.split("/*").next()?.trim();
                let v = v
                    .trim_start_matches('(')
                    .trim_end_matches(')')
                    .trim_end_matches('u');
                v.parse().ok()
            })
        };
        let checks: &[(&str, i64)] = &[
            ("BAE_ABI_VERSION_MAJOR", BAE_ABI_VERSION_MAJOR.into()),
            ("BAE_ABI_VERSION_MINOR", BAE_ABI_VERSION_MINOR.into()),
            ("BAE_OK", BAE_OK.into()),
            ("BAE_ERR_GENERIC", BAE_ERR_GENERIC.into()),
            ("BAE_ERR_UNSUPPORTED", BAE_ERR_UNSUPPORTED.into()),
            ("BAE_ERR_VERSION_MISMATCH", BAE_ERR_VERSION_MISMATCH.into()),
            ("BAE_ERR_NOT_FOUND", BAE_ERR_NOT_FOUND.into()),
            ("BAE_ERR_ABORTED", BAE_ERR_ABORTED.into()),
            ("BAE_ERR_PANIC", BAE_ERR_PANIC.into()),
            ("BAE_ERR_TIMEOUT", BAE_ERR_TIMEOUT.into()),
            ("BAE_ERR_INTERNAL", BAE_ERR_INTERNAL.into()),
            ("BAE_KIND_EFFECT", BAE_KIND_EFFECT.into()),
            ("BAE_PARAM_FLOAT", BAE_PARAM_FLOAT.into()),
            ("BAE_PARAM_POPUP", BAE_PARAM_POPUP.into()),
            ("BAE_PARAM_GROUP", BAE_PARAM_GROUP.into()),
            ("BAE_PARAM_BUTTON", BAE_PARAM_BUTTON.into()),
            ("BAE_EFFECT_CLASS_FILTER", BAE_EFFECT_CLASS_FILTER.into()),
            ("BAE_THREADING_CONCURRENT", BAE_THREADING_CONCURRENT.into()),
            ("BAE_DEVICE_CPU", BAE_DEVICE_CPU.into()),
            ("BAE_PRERENDER_IDENTITY", BAE_PRERENDER_IDENTITY.into()),
            ("BAE_INPUT_SELF", BAE_INPUT_SELF.into()),
            ("BAE_LOG_ERROR", BAE_LOG_ERROR.into()),
            ("BAE_DISPLAY_PERCENT", BAE_DISPLAY_PERCENT.into()),
        ];
        for (name, value) in checks {
            assert_eq!(define(name), Some(*value), "{name}");
        }
    }
}
