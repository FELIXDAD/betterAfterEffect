//! Writing native CPU effects in Rust (spec §8, §14).
//!
//! Implement [`CpuEffect`] for a type and export it with
//! [`export_plugin!`](crate::export_plugin); the macro generates the
//! `bae_plugin_entry` symbol of the C ABI, the effect vtables and the
//! parameter definitions. Every callback is wrapped in `catch_unwind`: a
//! panic becomes `BAE_ERR_PANIC` with the panic message (rule R8), so the host
//! can disable the plugin instead of crashing.
//!
//! ```ignore
//! use bae_plugin_sdk::effect::{CpuEffect, Error, Param, RenderArgs};
//!
//! #[derive(Default)]
//! struct Gain;
//!
//! impl CpuEffect for Gain {
//!     const ID: &'static core::ffi::CStr = c"gain";
//!     const PARAMS: &'static [Param] = &[Param::float(1, c"gain", c"Gain", 100.0, [0.0, 400.0])];
//!     fn render(&self, args: &mut RenderArgs<'_>) -> Result<(), Error> {
//!         let gain = args.params.float(1).unwrap_or(100.0) as f32 / 100.0;
//!         args.map_pixels(|_, _, [r, g, b, a]| [r * gain, g * gain, b * gain, a])
//!     }
//! }
//!
//! bae_plugin_sdk::export_plugin!(id: "com.example.gain", version: "0.1.0", effects: [Gain]);
//! ```
//!
//! The host hands images in premultiplied RGBA 32-bit float (the effect may
//! list other formats in [`CpuEffect::FORMATS`]).

use core::ffi::{CStr, c_char, c_void};
use core::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;

use crate::abi::*;
use crate::version::{ApiVersion, plugin_accepts_host};

/// A parameter definition (`BaeParamDef`), built with the `const` constructors.
#[derive(Clone, Copy, Debug)]
pub struct Param {
    /// Stable id (> 0, never reused): the document stores values by it.
    pub id: u32,
    /// Stable machine key `[a-z][a-z0-9_]*` (scripts, expressions).
    pub key: &'static CStr,
    /// Default English display name.
    pub name: &'static CStr,
    /// `BAE_PARAM_FLAG_*`.
    pub flags: u64,
    /// Type and type-specific definition.
    pub kind: ParamKind,
}

/// The type-specific part of a [`Param`].
#[derive(Clone, Copy, Debug)]
pub enum ParamKind {
    /// Float slider.
    Float {
        /// Default value.
        default: f64,
        /// Hard limits.
        range: [f64; 2],
        /// UI slider range.
        slider: [f64; 2],
        /// Decimal places shown.
        precision: u32,
        /// `BAE_DISPLAY_*`.
        display: u32,
    },
    /// Integer slider.
    Int {
        /// Default value.
        default: i32,
        /// Hard limits.
        range: [i32; 2],
        /// UI slider range.
        slider: [i32; 2],
        /// `BAE_DISPLAY_*`.
        display: u32,
    },
    /// Angle in degrees.
    Angle {
        /// Default value.
        default: f64,
    },
    /// Checkbox.
    Checkbox {
        /// Default value.
        default: bool,
    },
    /// Straight RGBA color.
    Color {
        /// Default value.
        default: [f32; 4],
    },
    /// 2D point; the default is a fraction of the layer size.
    Point {
        /// Default as a fraction of the layer size (0.5, 0.5 = centre).
        default: [f64; 2],
    },
    /// Popup menu: `(stable choice id, name)`.
    Popup {
        /// Default choice id.
        default: u32,
        /// Choices in menu order.
        choices: &'static [(u32, &'static CStr)],
    },
}

impl Param {
    /// A float slider with `slider = range`, one decimal, plain numbers.
    pub const fn float(
        id: u32,
        key: &'static CStr,
        name: &'static CStr,
        default: f64,
        range: [f64; 2],
    ) -> Self {
        Self::new(
            id,
            key,
            name,
            ParamKind::Float {
                default,
                range,
                slider: range,
                precision: 1,
                display: BAE_DISPLAY_NUMBER,
            },
        )
    }

    /// An integer slider with `slider = range`.
    pub const fn int(
        id: u32,
        key: &'static CStr,
        name: &'static CStr,
        default: i32,
        range: [i32; 2],
    ) -> Self {
        Self::new(
            id,
            key,
            name,
            ParamKind::Int {
                default,
                range,
                slider: range,
                display: BAE_DISPLAY_NUMBER,
            },
        )
    }

    /// An angle in degrees.
    pub const fn angle(id: u32, key: &'static CStr, name: &'static CStr, default: f64) -> Self {
        Self::new(id, key, name, ParamKind::Angle { default })
    }

    /// A checkbox.
    pub const fn checkbox(id: u32, key: &'static CStr, name: &'static CStr, default: bool) -> Self {
        Self::new(id, key, name, ParamKind::Checkbox { default })
    }

    /// A straight RGBA color.
    pub const fn color(
        id: u32,
        key: &'static CStr,
        name: &'static CStr,
        default: [f32; 4],
    ) -> Self {
        Self::new(id, key, name, ParamKind::Color { default })
    }

    /// A 2D point whose default is a fraction of the layer size.
    pub const fn point(
        id: u32,
        key: &'static CStr,
        name: &'static CStr,
        default: [f64; 2],
    ) -> Self {
        Self::new(id, key, name, ParamKind::Point { default })
    }

    /// A popup menu.
    pub const fn popup(
        id: u32,
        key: &'static CStr,
        name: &'static CStr,
        default: u32,
        choices: &'static [(u32, &'static CStr)],
    ) -> Self {
        Self::new(id, key, name, ParamKind::Popup { default, choices })
    }

    const fn new(id: u32, key: &'static CStr, name: &'static CStr, kind: ParamKind) -> Self {
        Self {
            id,
            key,
            name,
            flags: 0,
            kind,
        }
    }

    /// Sets the UI slider range of a float or int slider.
    pub const fn slider(mut self, lo: f64, hi: f64) -> Self {
        match &mut self.kind {
            ParamKind::Float { slider, .. } => *slider = [lo, hi],
            ParamKind::Int { slider, .. } => *slider = [lo as i32, hi as i32],
            _ => {}
        }
        self
    }

    /// Sets the display unit (`BAE_DISPLAY_*`) of a float or int slider.
    pub const fn display(mut self, unit: u32) -> Self {
        match &mut self.kind {
            ParamKind::Float { display, .. } | ParamKind::Int { display, .. } => *display = unit,
            _ => {}
        }
        self
    }

    /// Sets the decimal places of a float slider.
    pub const fn precision(mut self, places: u32) -> Self {
        if let ParamKind::Float { precision, .. } = &mut self.kind {
            *precision = places;
        }
        self
    }

    /// Adds `BAE_PARAM_FLAG_*` bits.
    pub const fn flags(mut self, flags: u64) -> Self {
        self.flags |= flags;
        self
    }
}

/// An error returned by an effect: a status and a message for the user.
#[derive(Clone, Debug)]
pub struct Error {
    /// `BAE_ERR_*`.
    pub status: BaeStatus,
    /// What went wrong.
    pub message: String,
}

impl Error {
    /// An error with a status.
    pub fn new(status: BaeStatus, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    /// `BAE_ERR_UNSUPPORTED`.
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(BAE_ERR_UNSUPPORTED, message)
    }

    /// `BAE_ERR_ABORTED` (the host asked to stop).
    pub fn aborted() -> Self {
        Self::new(BAE_ERR_ABORTED, "aborted")
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self::new(BAE_ERR_GENERIC, message)
    }
}

/// Evaluated parameter values at the frame's time, in definition order.
#[derive(Clone, Copy)]
pub struct Params<'a> {
    values: &'a [BaeParamValue],
}

impl<'a> Params<'a> {
    /// Wraps the host's value array.
    ///
    /// # Safety
    /// `values` must point to `count` valid values for `'a`.
    pub unsafe fn from_raw(values: &'a BaeParamValues) -> Self {
        let values = if values.values.is_null() || values.count == 0 {
            &[][..]
        } else {
            // SAFETY: the caller guarantees `count` values behind the pointer.
            unsafe { core::slice::from_raw_parts(values.values, values.count as usize) }
        };
        Self { values }
    }

    fn get(&self, id: u32, types: &[u32]) -> Option<&'a BaeParamValue> {
        self.values
            .iter()
            .find(|v| v.id == id && types.contains(&v.type_))
    }

    /// A FLOAT or ANGLE value.
    pub fn float(&self, id: u32) -> Option<f64> {
        // SAFETY: the union member matches the type tag.
        self.get(id, &[BAE_PARAM_FLOAT, BAE_PARAM_ANGLE])
            .map(|v| unsafe { v.v.f })
    }

    /// An INT value.
    pub fn int(&self, id: u32) -> Option<i32> {
        // SAFETY: the union member matches the type tag.
        self.get(id, &[BAE_PARAM_INT]).map(|v| unsafe { v.v.i })
    }

    /// A CHECKBOX value.
    pub fn checkbox(&self, id: u32) -> Option<bool> {
        // SAFETY: the union member matches the type tag.
        self.get(id, &[BAE_PARAM_CHECKBOX])
            .map(|v| unsafe { v.v.b } != 0)
    }

    /// A COLOR value (straight RGBA).
    pub fn color(&self, id: u32) -> Option<[f32; 4]> {
        // SAFETY: the union member matches the type tag.
        self.get(id, &[BAE_PARAM_COLOR])
            .map(|v| unsafe { v.v.color })
    }

    /// A POINT value in render pixels.
    pub fn point(&self, id: u32) -> Option<[f64; 2]> {
        // SAFETY: the union member matches the type tag.
        self.get(id, &[BAE_PARAM_POINT, BAE_PARAM_POINT_3D])
            .map(|v| unsafe { [v.v.point[0], v.v.point[1]] })
    }

    /// A POPUP value (the stable choice id).
    pub fn choice(&self, id: u32) -> Option<u32> {
        // SAFETY: the union member matches the type tag.
        self.get(id, &[BAE_PARAM_POPUP])
            .map(|v| unsafe { v.v.choice })
    }
}

/// A CPU image borrowed from the host (premultiplied RGBA).
pub struct Image<'a> {
    raw: BaeCpuImage,
    _borrow: PhantomData<&'a [u8]>,
}

impl Image<'_> {
    /// The render-pixel rect the buffer covers.
    pub fn rect(&self) -> BaeRect {
        self.raw.rect
    }

    /// `BaePixelFormat`.
    pub fn format(&self) -> BaePixelFormat {
        self.raw.format
    }

    fn row_bytes(&self, y: i32) -> Option<*mut u8> {
        let r = self.raw.rect;
        if y < r.y0 || y >= r.y1 || self.raw.data.is_null() {
            return None;
        }
        let offset = (y - r.y0) as isize * self.raw.row_bytes as isize;
        // SAFETY: inside the buffer described by rect and row_bytes.
        Some(unsafe { self.raw.data.cast::<u8>().offset(offset) })
    }

    /// Row `y` as `4 × width` floats (`None` outside the rect or for other formats).
    pub fn row_f32(&self, y: i32) -> Option<&[f32]> {
        if self.raw.format != BAE_PIXEL_FORMAT_RGBA_F32_PREMUL {
            return None;
        }
        let p = self.row_bytes(y)?;
        // SAFETY: an F32 row holds 4 floats per pixel; rows are 16-byte aligned.
        Some(unsafe {
            core::slice::from_raw_parts(p.cast::<f32>(), 4 * self.raw.rect.width() as usize)
        })
    }

    /// Row `y` as `4 × width` bytes (`None` outside the rect or for other formats).
    pub fn row_u8(&self, y: i32) -> Option<&[u8]> {
        if self.raw.format != BAE_PIXEL_FORMAT_RGBA_U8_PREMUL {
            return None;
        }
        let p = self.row_bytes(y)?;
        // SAFETY: a U8 row holds 4 bytes per pixel.
        Some(unsafe { core::slice::from_raw_parts(p, 4 * self.raw.rect.width() as usize) })
    }

    /// Pixel `(x, y)` as premultiplied RGBA floats; transparent outside the rect.
    pub fn pixel(&self, x: i32, y: i32) -> [f32; 4] {
        let r = self.raw.rect;
        if x < r.x0 || x >= r.x1 {
            return [0.0; 4];
        }
        let i = 4 * (x - r.x0) as usize;
        if let Some(row) = self.row_f32(y) {
            return [row[i], row[i + 1], row[i + 2], row[i + 3]];
        }
        if let Some(row) = self.row_u8(y) {
            return core::array::from_fn(|k| f32::from(row[i + k]) / 255.0);
        }
        [0.0; 4]
    }
}

/// The writable output image.
pub struct ImageMut<'a> {
    image: Image<'a>,
}

impl ImageMut<'_> {
    /// The render-pixel rect to fill (every pixel must be written).
    pub fn rect(&self) -> BaeRect {
        self.image.rect()
    }

    /// `BaePixelFormat`.
    pub fn format(&self) -> BaePixelFormat {
        self.image.format()
    }

    /// Row `y` as `4 × width` floats (F32 output only).
    pub fn row_f32_mut(&mut self, y: i32) -> Option<&mut [f32]> {
        if self.image.raw.format != BAE_PIXEL_FORMAT_RGBA_F32_PREMUL {
            return None;
        }
        let p = self.image.row_bytes(y)?;
        let n = 4 * self.image.raw.rect.width() as usize;
        // SAFETY: the output buffer is writable and exclusively ours during the call.
        Some(unsafe { core::slice::from_raw_parts_mut(p.cast::<f32>(), n) })
    }

    /// Row `y` as `4 × width` bytes (U8 output only).
    pub fn row_u8_mut(&mut self, y: i32) -> Option<&mut [u8]> {
        if self.image.raw.format != BAE_PIXEL_FORMAT_RGBA_U8_PREMUL {
            return None;
        }
        let p = self.image.row_bytes(y)?;
        let n = 4 * self.image.raw.rect.width() as usize;
        // SAFETY: the output buffer is writable and exclusively ours during the call.
        Some(unsafe { core::slice::from_raw_parts_mut(p, n) })
    }
}

/// Everything `render_cpu` gets.
pub struct RenderArgs<'a> {
    /// Parameter values at the frame's time.
    pub params: Params<'a>,
    /// The frame request (time, downsample, layer bounds, flags).
    pub info: &'a BaeRenderInfo,
    /// The effect's input (layer pixels after the previous effects).
    pub input: Image<'a>,
    /// The output to fill.
    pub output: ImageMut<'a>,
    host: &'a Host,
    ctx: *mut BaeRenderCtx,
}

impl RenderArgs<'_> {
    /// Layer time in seconds.
    pub fn time_seconds(&self) -> f64 {
        self.info.time as f64 / BAE_FLICKS_PER_SECOND as f64
    }

    /// Downsample factors (1 at full resolution, 0.5 at half).
    pub fn downsample(&self) -> [f64; 2] {
        let f = |r: BaeRational| {
            if r.den == 0 {
                1.0
            } else {
                f64::from(r.num) / f64::from(r.den)
            }
        };
        [f(self.info.downsample_x), f(self.info.downsample_y)]
    }

    /// `true` when the host wants the render to stop (return [`Error::aborted`]).
    pub fn is_aborted(&self) -> bool {
        // SAFETY: suite pointer and ctx come from the host for this call.
        unsafe {
            (*self.host.render)
                .is_aborted
                .is_some_and(|f| f(self.ctx) != 0)
        }
    }

    /// Fills the output pixel by pixel: `f(x, y, input pixel)` returns the
    /// output pixel (premultiplied RGBA floats). Polls for abort once per row.
    pub fn map_pixels(
        &mut self,
        mut f: impl FnMut(i32, i32, [f32; 4]) -> [f32; 4],
    ) -> Result<(), Error> {
        let r = self.output.rect();
        for y in r.y0..r.y1 {
            if self.is_aborted() {
                return Err(Error::aborted());
            }
            let input = &self.input;
            if let Some(row) = self.output.row_f32_mut(y) {
                for (i, x) in (r.x0..r.x1).enumerate() {
                    row[4 * i..4 * i + 4].copy_from_slice(&f(x, y, input.pixel(x, y)));
                }
            } else if let Some(row) = self.output.row_u8_mut(y) {
                for (i, x) in (r.x0..r.x1).enumerate() {
                    let px = f(x, y, input.pixel(x, y));
                    for k in 0..4 {
                        row[4 * i + k] = (px[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    }
                }
            } else {
                return Err(Error::unsupported("unsupported output pixel format"));
            }
        }
        Ok(())
    }
}

/// A native CPU effect (filter class) written in Rust.
pub trait CpuEffect: Default + Send + Sync + 'static {
    /// Effect id: must equal the `[[effect]] id` in `bae-plugin.toml`.
    const ID: &'static CStr;
    /// Parameters in display order.
    const PARAMS: &'static [Param];
    /// `BAE_EFFECT_FLAG_*` (e.g. `BAE_EFFECT_FLAG_PIXEL_INDEPENDENT`).
    const FLAGS: u64 = 0;
    /// `BAE_THREADING_*`; the default promises `render` is thread-safe.
    const THREADING: u32 = BAE_THREADING_CONCURRENT;
    /// CPU pixel formats `render` accepts, in preference order.
    const FORMATS: &'static [BaePixelFormat] = &[BAE_PIXEL_FORMAT_RGBA_F32_PREMUL];

    /// Renders one frame: fill every pixel of `args.output`.
    fn render(&self, args: &mut RenderArgs<'_>) -> Result<(), Error>;
}

/// Host handle and suites, acquired in `bae_plugin_entry`.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct Host {
    handle: *mut BaeHostHandle,
    core: *const BaeCoreSuite1,
    define: *const BaeParamDefineSuite1,
    render: *const BaeRenderSuite1,
}

// SAFETY: suite tables are immutable and thread-safe (spec §14.2); the handle
// is only passed back to the host.
unsafe impl Send for Host {}
// SAFETY: as above.
unsafe impl Sync for Host {}

impl Host {
    fn set_error(&self, message: &str) {
        let text = std::ffi::CString::new(message.replace('\0', " ")).unwrap_or_default();
        // SAFETY: suite and handle come from the host and are valid until shutdown.
        unsafe {
            if let Some(f) = (*self.core).set_error {
                f(self.handle, text.as_ptr());
            }
        }
    }
}

struct Global<E> {
    host: Host,
    effect: E,
}

fn guard(host: Option<&Host>, f: impl FnOnce() -> Result<(), Error>) -> BaeStatus {
    let (status, message) = match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(())) => return BAE_OK,
        Ok(Err(e)) => (e.status, e.message),
        Err(panic) => {
            let text = panic
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_owned());
            (BAE_ERR_PANIC, format!("panic: {text}"))
        }
    };
    if let Some(host) = host {
        host.set_error(&message);
    }
    if status >= 0 { BAE_ERR_GENERIC } else { status }
}

unsafe extern "C" fn global_setup<E: CpuEffect>(
    plugin_data: *mut c_void,
    _args: *const BaeGlobalSetupArgs,
    out_desc: *mut BaeEffectDescription,
    out_global: *mut *mut c_void,
) -> BaeStatus {
    // SAFETY: plugin_data is the `Host` box created in `entry`.
    let host = unsafe { &*plugin_data.cast::<Host>() };
    guard(Some(host), || {
        if out_desc.is_null() || out_global.is_null() {
            return Err(Error::new(
                BAE_ERR_INVALID_ARGUMENT,
                "global_setup: NULL outputs",
            ));
        }
        // SAFETY: host-provided out-struct; R3 checked before writing.
        let desc = unsafe { &mut *out_desc };
        if !has_field(
            desc.struct_size,
            core::mem::offset_of!(BaeEffectDescription, cpu_formats),
            8,
        ) {
            return Err(Error::new(
                BAE_ERR_VERSION_MISMATCH,
                "host BaeEffectDescription is too small",
            ));
        }
        desc.effect_class = BAE_EFFECT_CLASS_FILTER;
        desc.flags = E::FLAGS;
        desc.threading = E::THREADING;
        desc.devices = BAE_DEVICE_CPU;
        desc.gpu_formats = 0;
        desc.cpu_format_count = E::FORMATS.len() as u32;
        desc.cpu_formats = E::FORMATS.as_ptr();
        let global = Box::new(Global {
            host: *host,
            effect: E::default(),
        });
        // SAFETY: checked non-null above.
        unsafe { *out_global = Box::into_raw(global).cast() };
        Ok(())
    })
}

unsafe extern "C" fn global_setdown<E: CpuEffect>(global: *mut c_void) {
    if !global.is_null() {
        let _ = catch_unwind(|| {
            // SAFETY: created by `global_setup::<E>`.
            drop(unsafe { Box::from_raw(global.cast::<Global<E>>()) });
        });
    }
}

unsafe extern "C" fn define_params<E: CpuEffect>(
    global: *mut c_void,
    builder: *mut BaeParamBuilder,
) -> BaeStatus {
    // SAFETY: created by `global_setup::<E>`.
    let g = unsafe { &*global.cast::<Global<E>>() };
    guard(Some(&g.host), || {
        // SAFETY: suite pointer from the host.
        let add = unsafe { (*g.host.define).add }
            .ok_or_else(|| Error::unsupported("bae.param.define has no add"))?;
        for p in E::PARAMS {
            let mut def = BaeParamDef::zeroed();
            def.id = p.id;
            def.key = p.key.as_ptr();
            def.name = p.name.as_ptr();
            def.flags = p.flags;
            let choices: Vec<BaePopupChoice>;
            match p.kind {
                ParamKind::Float {
                    default,
                    range,
                    slider,
                    precision,
                    display,
                } => {
                    def.type_ = BAE_PARAM_FLOAT;
                    def.u.float_def = BaeParamFloatDef {
                        default_value: default,
                        valid_min: range[0],
                        valid_max: range[1],
                        slider_min: slider[0],
                        slider_max: slider[1],
                        precision,
                        display,
                    };
                }
                ParamKind::Int {
                    default,
                    range,
                    slider,
                    display,
                } => {
                    def.type_ = BAE_PARAM_INT;
                    def.u.int_def = BaeParamIntDef {
                        default_value: default,
                        valid_min: range[0],
                        valid_max: range[1],
                        slider_min: slider[0],
                        slider_max: slider[1],
                        display,
                    };
                }
                ParamKind::Angle { default } => {
                    def.type_ = BAE_PARAM_ANGLE;
                    def.u.angle_def = BaeParamAngleDef {
                        default_degrees: default,
                    };
                }
                ParamKind::Checkbox { default } => {
                    def.type_ = BAE_PARAM_CHECKBOX;
                    def.u.checkbox_def = BaeParamCheckboxDef {
                        default_value: u32::from(default),
                        reserved: 0,
                        label: core::ptr::null(),
                    };
                }
                ParamKind::Color { default } => {
                    def.type_ = BAE_PARAM_COLOR;
                    def.u.color_def = BaeParamColorDef {
                        default_rgba: default,
                        flags: 0,
                        reserved: 0,
                    };
                }
                ParamKind::Point { default } => {
                    def.type_ = BAE_PARAM_POINT;
                    def.flags |= BAE_PARAM_FLAG_RELATIVE_DEFAULT;
                    def.u.point_def = BaeParamPointDef {
                        default_xyz: [default[0], default[1], 0.0],
                        reserved: [0; 2],
                    };
                }
                ParamKind::Popup {
                    default,
                    choices: list,
                } => {
                    def.type_ = BAE_PARAM_POPUP;
                    choices = list
                        .iter()
                        .map(|(id, name)| BaePopupChoice {
                            id: *id,
                            flags: 0,
                            name: name.as_ptr(),
                            l10n_key: core::ptr::null(),
                        })
                        .collect();
                    def.u.popup_def = BaeParamPopupDef {
                        default_id: default,
                        choice_count: choices.len() as u32,
                        choices: choices.as_ptr(),
                    };
                }
            }
            // SAFETY: the host copies the definition during the call.
            let status = unsafe { add(builder, &def) };
            if status != BAE_OK {
                return Err(Error::new(
                    status,
                    format!("param {} rejected by the host", p.id),
                ));
            }
        }
        Ok(())
    })
}

unsafe extern "C" fn render_cpu<E: CpuEffect>(
    global: *mut c_void,
    _instance: *const c_void,
    args: *const BaeCpuRenderArgs,
) -> BaeStatus {
    // SAFETY: created by `global_setup::<E>`.
    let g = unsafe { &*global.cast::<Global<E>>() };
    guard(Some(&g.host), || {
        // SAFETY: host-provided arguments, valid for the call.
        let args = unsafe { args.as_ref() }.ok_or("render_cpu: NULL arguments")?;
        // SAFETY: as above.
        let (info, output) = unsafe { (args.info.as_ref(), args.output.as_ref()) };
        let (info, output) = info.zip(output).ok_or("render_cpu: NULL info or output")?;
        let mut input = BaeCpuImage {
            struct_size: core::mem::size_of::<BaeCpuImage>() as u32,
            format: 0,
            rect: BaeRect::default(),
            row_bytes: 0,
            data: core::ptr::null_mut(),
            flags: 0,
            reserved: 0,
        };
        // SAFETY: suite pointer from the host; ctx from the args.
        let status = unsafe {
            let get = (*g.host.render)
                .get_input_cpu
                .ok_or_else(|| Error::unsupported("bae.render has no get_input_cpu"))?;
            get(args.ctx, 0, &mut input)
        };
        if status != BAE_OK {
            return Err(Error::new(status, "cannot get the input image"));
        }
        let mut render = RenderArgs {
            // SAFETY: the host's values are valid for the call.
            params: unsafe { Params::from_raw(&info.params) },
            info,
            input: Image {
                raw: input,
                _borrow: PhantomData,
            },
            output: ImageMut {
                image: Image {
                    raw: *output,
                    _borrow: PhantomData,
                },
            },
            host: &g.host,
            ctx: args.ctx,
        };
        g.effect.render(&mut render)
    })
}

/// One exported effect (generated by [`export_plugin!`](crate::export_plugin)).
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct EffectExport {
    id: &'static CStr,
    vtable: &'static BaeEffectVTable1,
}

struct VTable<E>(PhantomData<E>);

impl<E: CpuEffect> VTable<E> {
    const VTABLE: BaeEffectVTable1 = BaeEffectVTable1 {
        global_setup: Some(global_setup::<E>),
        global_setdown: Some(global_setdown::<E>),
        define_params: Some(define_params::<E>),
        render_cpu: Some(render_cpu::<E>),
        ..BaeEffectVTable1::EMPTY
    };
}

impl EffectExport {
    /// The export of effect `E`.
    pub fn of<E: CpuEffect>() -> Self {
        Self {
            id: E::ID,
            vtable: &VTable::<E>::VTABLE,
        }
    }
}

/// The contribution table, built once per process.
#[doc(hidden)]
pub struct Contributions(OnceLock<Box<[ContributionCell]>>);

#[doc(hidden)]
#[repr(transparent)]
pub struct ContributionCell(BaeContribution);

// SAFETY: the table only holds pointers to `'static` data and is never mutated.
unsafe impl Sync for ContributionCell {}
// SAFETY: as above.
unsafe impl Send for ContributionCell {}

impl Contributions {
    /// An empty table.
    pub const fn new() -> Self {
        Self(OnceLock::new())
    }
}

impl Default for Contributions {
    fn default() -> Self {
        Self::new()
    }
}

unsafe extern "C" fn shutdown(plugin_data: *mut c_void) {
    if !plugin_data.is_null() {
        // SAFETY: created by `entry`.
        drop(unsafe { Box::from_raw(plugin_data.cast::<Host>()) });
    }
}

/// The body of the generated `bae_plugin_entry`.
///
/// # Safety
/// Called by the host with valid `host` / `out` pointers (C ABI contract);
/// `id` and `version` are NUL-terminated.
#[doc(hidden)]
pub unsafe fn entry(
    host: *const BaeHost,
    out: *mut BaePluginExports,
    id: &'static str,
    version: &'static str,
    table: &'static Contributions,
    effects: &[EffectExport],
) -> BaeStatus {
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: host-provided pointers (contract of the entry point).
        let (Some(h), Some(out)) = (unsafe { host.as_ref() }, unsafe { out.as_mut() }) else {
            return BAE_ERR_INVALID_ARGUMENT;
        };
        if !has_field(h.struct_size, core::mem::offset_of!(BaeHost, get_suite), 8) {
            return BAE_ERR_VERSION_MISMATCH;
        }
        let (Some(get_suite), true) = (
            h.get_suite,
            plugin_accepts_host(ApiVersion::CURRENT, ApiVersion::unpack(h.abi_version)),
        ) else {
            return BAE_ERR_VERSION_MISMATCH;
        };
        // SAFETY: suites are acquired by name and version (R4).
        let suite = |name: &CStr, version| unsafe { get_suite(h.handle, name.as_ptr(), version) };
        let state = Host {
            handle: h.handle,
            core: suite(BAE_SUITE_CORE, BAE_SUITE_CORE_VERSION).cast(),
            define: suite(BAE_SUITE_PARAM_DEFINE, BAE_SUITE_PARAM_DEFINE_VERSION).cast(),
            render: suite(BAE_SUITE_RENDER, BAE_SUITE_RENDER_VERSION).cast(),
        };
        if state.core.is_null() || state.define.is_null() || state.render.is_null() {
            return BAE_ERR_UNSUPPORTED;
        }
        if !has_field(
            out.struct_size,
            core::mem::offset_of!(BaePluginExports, contributions),
            8,
        ) {
            state.set_error("host BaePluginExports is too small");
            return BAE_ERR_VERSION_MISMATCH;
        }
        let contributions = table.0.get_or_init(|| {
            effects
                .iter()
                .map(|e| {
                    ContributionCell(BaeContribution {
                        struct_size: core::mem::size_of::<BaeContribution>() as u32,
                        kind: BAE_KIND_EFFECT,
                        id: e.id.as_ptr(),
                        vtable_version: 1,
                        reserved: 0,
                        vtable: (e.vtable as *const BaeEffectVTable1).cast(),
                    })
                })
                .collect()
        });
        out.abi_built = BAE_ABI_VERSION;
        out.abi_required = ApiVersion::CURRENT.pack();
        out.reserved0 = 0;
        out.plugin_id = id.as_ptr().cast::<c_char>();
        out.plugin_version = version.as_ptr().cast::<c_char>();
        out.plugin_data = Box::into_raw(Box::new(state)).cast();
        out.contribution_count = contributions.len() as u32;
        out.reserved1 = 0;
        // `ContributionCell` is `repr(transparent)` over `BaeContribution`.
        out.contributions = contributions.as_ptr().cast::<BaeContribution>();
        if has_field(
            out.struct_size,
            core::mem::offset_of!(BaePluginExports, shutdown),
            8,
        ) {
            out.shutdown = Some(shutdown);
        }
        if has_field(
            out.struct_size,
            core::mem::offset_of!(BaePluginExports, get_suite),
            8,
        ) {
            out.get_suite = None;
        }
        BAE_OK
    }));
    result.unwrap_or(BAE_ERR_PANIC)
}

/// Exports the plugin: generates `bae_plugin_entry` for the listed
/// [`CpuEffect`](crate::effect::CpuEffect) types. `id` and `version` must equal
/// `[plugin] id` / `version` in `bae-plugin.toml`, whose `api` must be the SDK's
/// API (`"1.0"`).
#[macro_export]
macro_rules! export_plugin {
    (id: $id:literal, version: $version:literal, effects: [$($effect:ty),* $(,)?] $(,)?) => {
        /// The plugin entry point (generated by `bae_plugin_sdk::export_plugin!`).
        ///
        /// # Safety
        /// Called by the BAE host with valid pointers.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn bae_plugin_entry(
            host: *const $crate::abi::BaeHost,
            out: *mut $crate::abi::BaePluginExports,
        ) -> $crate::abi::BaeStatus {
            static TABLE: $crate::effect::Contributions = $crate::effect::Contributions::new();
            // SAFETY: forwarded entry-point contract.
            unsafe {
                $crate::effect::entry(
                    host,
                    out,
                    concat!($id, "\0"),
                    concat!($version, "\0"),
                    &TABLE,
                    &[$($crate::effect::EffectExport::of::<$effect>()),*],
                )
            }
        }
    };
}
