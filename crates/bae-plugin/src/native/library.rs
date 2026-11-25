//! A native plugin library loaded into the current process (spec §7.2, §8,
//! §14): `bae_plugin_entry`, the host suites `bae.core`, `bae.param.define`
//! and `bae.render`, and the effect callbacks the host drives
//! (`global_setup`, `define_params`, `pre_render`, `render_cpu`,
//! `release_frame_data`, `global_setdown`, `shutdown`).
//!
//! The same code runs in the host (in-process mode) and in the runner
//! (out-of-process mode, [`super::runner`]): only the transport differs.

use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use bae_plugin_sdk::abi::*;
use parking_lot::Mutex;

use super::pixels::{self, AlignedBuf};
use super::{
    CallError, EffectInfo, FrameRequest, LibraryInfo, ParamInfo, ParamWire, PreRenderOut,
    RenderOut, rect, rect_of,
};

thread_local! {
    /// Message attached by `bae.core.set_error` to the error being returned.
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn take_error() -> Option<String> {
    LAST_ERROR.with(|e| e.borrow_mut().take())
}

fn status_error(status: BaeStatus, what: &str) -> CallError {
    let detail = take_error().unwrap_or_else(|| status_name(status).to_owned());
    CallError::Status {
        status,
        message: format!("{what}: {detail}"),
    }
}

/// Copies a NUL-terminated string from the plugin.
///
/// # Safety
/// `p` is NULL or a valid C string.
unsafe fn c_text(p: *const c_char) -> Option<String> {
    // SAFETY: guaranteed by the caller.
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
}

/// What the plugin sees as `BaeHostHandle`.
struct HostState {
    host: BaeHost,
    _strings: [CString; 4],
    plugin_dir: String,
    logs: Mutex<Vec<(u32, String)>>,
}

const MAX_LOGS: usize = 256;

/// One effect contribution after `global_setup` and `define_params`.
struct LoadedEffect {
    info: EffectInfo,
    vtable: BaeEffectVTable1,
    global: *mut c_void,
    /// Held around render-side calls unless the effect is fully concurrent.
    serial: Option<Mutex<()>>,
}

/// A loaded native library.
pub struct Library {
    effects: Vec<LoadedEffect>,
    exports: BaePluginExports,
    info: LibraryInfo,
    host: Box<HostState>,
    // Last: unmapped after every callback above has run. `None` for plugins
    // linked into the executable ([`Library::load_static`]).
    _lib: Option<libloading::Library>,
}

// SAFETY: the plugin's raw pointers are only used through the C ABI under the
// threading contract it declared (`serial` serializes non-concurrent
// effects); suite tables and the host state are thread-safe.
unsafe impl Send for Library {}
// SAFETY: as above.
unsafe impl Sync for Library {}

impl std::fmt::Debug for Library {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Library")
            .field("plugin", &self.info.plugin_id)
            .finish_non_exhaustive()
    }
}

fn open(path: &Path) -> Result<libloading::Library, String> {
    #[cfg(unix)]
    let lib = {
        use libloading::os::unix::{Library, RTLD_LOCAL, RTLD_NOW};
        // SAFETY: loading a library runs its initializers; the user trusted
        // the plugin (spec §21).
        unsafe { Library::open(Some(path), RTLD_NOW | RTLD_LOCAL) }.map(Into::into)
    };
    #[cfg(windows)]
    let lib = {
        use libloading::os::windows::{
            LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, Library,
        };
        // SAFETY: as above. The plugin's own directory resolves its dependencies.
        unsafe {
            Library::load_with_flags(
                path,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            )
        }
        .map(Into::into)
    };
    lib.map_err(|e| format!("cannot load `{}`: {e}", path.display()))
}

impl Library {
    /// Loads `path`, calls `bae_plugin_entry` presenting `host_abi`, then
    /// sets up every effect contribution. `capabilities` are `BAE_HOST_CAP_*`.
    pub fn load(
        path: &Path,
        plugin_id: &str,
        plugin_dir: &Path,
        host_abi: u32,
        capabilities: u64,
    ) -> Result<Library, String> {
        let lib = open(path)?;
        // SAFETY: the symbol has the documented entry-point signature (R1).
        let entry: BaePluginEntryFn = unsafe {
            *lib.get::<BaePluginEntryFn>(b"bae_plugin_entry\0")
                .map_err(|e| format!("`bae_plugin_entry` not found in `{}`: {e}", path.display()))?
        };
        Self::start(
            entry,
            Some(lib),
            plugin_id,
            plugin_dir,
            host_abi,
            capabilities,
        )
    }

    /// A plugin linked into the executable (built-in features through the
    /// plugin path, spec §19; tests): the same protocol without a library file.
    pub fn load_static(
        entry: BaePluginEntryFn,
        plugin_id: &str,
        plugin_dir: &Path,
        host_abi: u32,
        capabilities: u64,
    ) -> Result<Library, String> {
        Self::start(entry, None, plugin_id, plugin_dir, host_abi, capabilities)
    }

    fn start(
        entry: BaePluginEntryFn,
        lib: Option<libloading::Library>,
        plugin_id: &str,
        plugin_dir: &Path,
        host_abi: u32,
        capabilities: u64,
    ) -> Result<Library, String> {
        let cstring = |s: &str| CString::new(s.replace('\0', "")).unwrap_or_default();
        let strings = [
            cstring("BAE"),
            cstring(env!("CARGO_PKG_VERSION")),
            cstring(plugin_id),
            cstring(&plugin_dir.to_string_lossy()),
        ];
        let mut host = Box::new(HostState {
            host: BaeHost {
                struct_size: std::mem::size_of::<BaeHost>() as u32,
                abi_version: host_abi,
                capabilities,
                host_name: strings[0].as_ptr(),
                host_version: strings[1].as_ptr(),
                plugin_id: strings[2].as_ptr(),
                plugin_dir: strings[3].as_ptr(),
                handle: std::ptr::null_mut(),
                get_suite: Some(get_suite),
            },
            _strings: strings,
            plugin_dir: plugin_dir.to_string_lossy().into_owned(),
            logs: Mutex::new(Vec::new()),
        });
        host.host.handle = std::ptr::from_mut::<HostState>(&mut host).cast();

        let mut exports = BaePluginExports {
            struct_size: std::mem::size_of::<BaePluginExports>() as u32,
            abi_built: 0,
            abi_required: 0,
            reserved0: 0,
            plugin_id: std::ptr::null(),
            plugin_version: std::ptr::null(),
            plugin_data: std::ptr::null_mut(),
            contribution_count: 0,
            reserved1: 0,
            contributions: std::ptr::null(),
            shutdown: None,
            get_suite: None,
        };
        take_error();
        // SAFETY: valid host and exports structs; the entry point is called once.
        let status = unsafe { entry(&host.host, &mut exports) };
        if status != BAE_OK {
            return Err(status_error(status, "bae_plugin_entry failed").to_string());
        }
        let shutdown = |exports: &BaePluginExports| {
            if let Some(f) = exports.shutdown {
                // SAFETY: called once, after every contribution was torn down.
                unsafe { f(exports.plugin_data) };
            }
        };
        // SAFETY: strings exported by the plugin, static lifetime (spec §14).
        let (id, version) = unsafe { (c_text(exports.plugin_id), c_text(exports.plugin_version)) };
        let mut info = LibraryInfo {
            plugin_id: id.unwrap_or_default(),
            plugin_version: version.unwrap_or_default(),
            abi_built: exports.abi_built,
            abi_required: exports.abi_required,
            effects: Vec::new(),
            ignored: Vec::new(),
        };
        let contributions = if exports.contributions.is_null() {
            &[][..]
        } else {
            // SAFETY: `contribution_count` entries with static lifetime.
            unsafe {
                std::slice::from_raw_parts(
                    exports.contributions,
                    exports.contribution_count as usize,
                )
            }
        };
        let mut effects: Vec<LoadedEffect> = Vec::new();
        let mut failure = None;
        for c in contributions {
            // SAFETY: static string from the plugin.
            let cid = unsafe { c_text(c.id) }.unwrap_or_default();
            if c.kind != BAE_KIND_EFFECT {
                info.ignored.push(format!("{cid} (kind {})", c.kind));
                continue;
            }
            if c.vtable_version != 1 || c.vtable.is_null() {
                failure = Some(format!(
                    "effect `{cid}`: vtable version {} is not supported (this BAE implements 1)",
                    c.vtable_version
                ));
                break;
            }
            match setup_effect(&host, &exports, c, &cid) {
                Ok(e) => effects.push(e),
                Err(e) => {
                    failure = Some(format!("effect `{cid}`: {e}"));
                    break;
                }
            }
        }
        if let Some(message) = failure {
            for e in effects.iter().rev() {
                teardown(e);
            }
            shutdown(&exports);
            return Err(message);
        }
        info.effects = effects.iter().map(|e| e.info.clone()).collect();
        Ok(Library {
            effects,
            exports,
            info,
            host,
            _lib: lib,
        })
    }

    /// Ids, versions and effects the library exported.
    pub fn info(&self) -> &LibraryInfo {
        &self.info
    }

    /// Log lines written through `bae.core.log` since the last call.
    pub fn take_logs(&self) -> Vec<(u32, String)> {
        std::mem::take(&mut *self.host.logs.lock())
    }

    fn effect(&self, index: u32) -> Result<&LoadedEffect, CallError> {
        self.effects
            .get(index as usize)
            .ok_or_else(|| CallError::Failed(format!("no effect #{index}")))
    }

    /// `pre_render` alone (ROI and bounds queries); frame data is released at once.
    pub fn pre_render(&self, frame: &FrameRequest) -> Result<PreRenderOut, CallError> {
        let e = self.effect(frame.effect)?;
        let values = param_values(&e.info.params, &frame.params);
        let info = render_info(frame, &values);
        let mut ctx = RenderCtx::new(frame, &values, None);
        let _guard = e.serial.as_ref().map(|m| m.lock());
        let pre = self.call_pre_render(e, &info, &mut ctx, rect(frame.request))?;
        if !pre.frame_data.is_null() {
            self.release(e, pre.frame_data);
        }
        Ok(PreRenderOut {
            flags: pre.flags,
            result: rect_of(pre.result_rect),
            max_result: rect_of(pre.max_result_rect),
            input: ctx.self_checkout.map(rect_of),
        })
    }

    /// `pre_render` + `render_cpu` + `release_frame_data` for one frame.
    /// `input` covers `frame.input`, `output` covers `frame.request`, both in
    /// `frame.format` with [`pixels::row_bytes`] strides.
    pub fn render(
        &self,
        frame: &FrameRequest,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<RenderOut, CallError> {
        let e = self.effect(frame.effect)?;
        let format = frame.format;
        let (input_rect, request) = (rect(frame.input), rect(frame.request));
        if input.len() < pixels::image_len(format, input_rect)
            || output.len() < pixels::image_len(format, request)
        {
            return Err(CallError::Failed("frame buffers are too small".into()));
        }
        let values = param_values(&e.info.params, &frame.params);
        let info = render_info(frame, &values);
        let input_image = BaeCpuImage {
            struct_size: std::mem::size_of::<BaeCpuImage>() as u32,
            format,
            rect: input_rect,
            row_bytes: pixels::row_bytes(format, input_rect.width()) as i64,
            data: input.as_ptr().cast_mut().cast(),
            flags: 0,
            reserved: 0,
        };
        let mut ctx = RenderCtx::new(frame, &values, Some(input_image));
        let _guard = e.serial.as_ref().map(|m| m.lock());
        let pre = self.call_pre_render(e, &info, &mut ctx, request)?;
        if pre.flags & BAE_PRERENDER_IDENTITY != 0 {
            if !pre.frame_data.is_null() {
                self.release(e, pre.frame_data);
            }
            return Ok(RenderOut::Identity);
        }
        let out_rect = pre.result_rect.intersect(&request);
        if out_rect.is_empty() {
            if !pre.frame_data.is_null() {
                self.release(e, pre.frame_data);
            }
            return Ok(RenderOut::Rendered(rect_of(out_rect)));
        }
        let stride = pixels::row_bytes(format, request.width());
        let offset = (out_rect.y0 - request.y0) as usize * stride
            + (out_rect.x0 - request.x0) as usize * pixels::bytes_per_pixel(format);
        let mut out_image = BaeCpuImage {
            struct_size: std::mem::size_of::<BaeCpuImage>() as u32,
            format,
            rect: out_rect,
            row_bytes: stride as i64,
            // SAFETY: `offset` is inside `output` (out_rect ⊂ request).
            data: unsafe { output.as_mut_ptr().add(offset) }.cast(),
            flags: BAE_IMAGE_FLAG_WRITABLE,
            reserved: 0,
        };
        ctx.phase_render();
        let args = BaeCpuRenderArgs {
            struct_size: std::mem::size_of::<BaeCpuRenderArgs>() as u32,
            reserved: 0,
            info: &info,
            ctx: ctx.as_raw(),
            frame_data: pre.frame_data,
            output: &mut out_image,
        };
        take_error();
        let render = e.vtable.render_cpu.ok_or_else(|| {
            CallError::Failed(format!("effect `{}` has no render_cpu", e.info.id))
        })?;
        // SAFETY: valid arguments for the duration of the call.
        let status = unsafe { render(e.global, std::ptr::null(), &args) };
        let error = (status != BAE_OK).then(|| status_error(status, "render_cpu"));
        if !pre.frame_data.is_null() {
            self.release(e, pre.frame_data);
        }
        match error {
            Some(error) => Err(error),
            None => Ok(RenderOut::Rendered(rect_of(out_rect))),
        }
    }

    fn call_pre_render(
        &self,
        e: &LoadedEffect,
        info: &BaeRenderInfo,
        ctx: &mut RenderCtx<'_>,
        request: BaeRect,
    ) -> Result<BaePreRenderResult, CallError> {
        let input = ctx.input_rect;
        let mut out = BaePreRenderResult {
            struct_size: std::mem::size_of::<BaePreRenderResult>() as u32,
            flags: 0,
            result_rect: BaeRect::default(),
            max_result_rect: BaeRect::default(),
            frame_data: std::ptr::null_mut(),
        };
        let Some(pre) = e.vtable.pre_render else {
            // NULL pre_render: the self input as id 0 with rect = request,
            // and the input's rects (identity ROI, spec §8.7 item 2).
            ctx.self_checkout = Some(request);
            out.result_rect = input.intersect(&request);
            out.max_result_rect = input;
            return Ok(out);
        };
        let args = BaePreRenderArgs {
            struct_size: std::mem::size_of::<BaePreRenderArgs>() as u32,
            device: BAE_DEVICE_CPU,
            info,
            ctx: ctx.as_raw(),
            request_rect: request,
            flags: 0,
            reserved: 0,
        };
        take_error();
        // SAFETY: valid arguments for the duration of the call.
        let status = unsafe { pre(e.global, std::ptr::null(), &args, &mut out) };
        if status != BAE_OK {
            if !out.frame_data.is_null() {
                self.release(e, out.frame_data);
            }
            return Err(status_error(status, "pre_render"));
        }
        Ok(out)
    }

    fn release(&self, e: &LoadedEffect, frame_data: *mut c_void) {
        if let Some(f) = e.vtable.release_frame_data {
            // SAFETY: frame data returned by this effect's pre_render.
            unsafe { f(e.global, std::ptr::null(), frame_data) };
        }
    }

    /// The plugin directory the host presented.
    pub fn plugin_dir(&self) -> &str {
        &self.host.plugin_dir
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // Spec §7.4 step 4: reverse creation order, then shutdown, then unmap.
        for e in self.effects.iter().rev() {
            teardown(e);
        }
        if let Some(f) = self.exports.shutdown {
            // SAFETY: called once, after every contribution was torn down.
            unsafe { f(self.exports.plugin_data) };
        }
    }
}

fn teardown(e: &LoadedEffect) {
    if let Some(f) = e.vtable.global_setdown {
        // SAFETY: the global returned by this effect's global_setup.
        unsafe { f(e.global) };
    }
}

fn setup_effect(
    host: &HostState,
    exports: &BaePluginExports,
    c: &BaeContribution,
    id: &str,
) -> Result<LoadedEffect, String> {
    // R3: copy what the plugin's struct_size covers, the rest stays NULL.
    let mut vtable = BaeEffectVTable1::EMPTY;
    // SAFETY: `vtable` points to a struct starting with struct_size (R3).
    let size = unsafe { *c.vtable.cast::<u32>() } as usize;
    let n = size.min(std::mem::size_of::<BaeEffectVTable1>());
    // SAFETY: copy `n` bytes the plugin declared; the destination is large enough.
    unsafe {
        std::ptr::copy_nonoverlapping(
            c.vtable.cast::<u8>(),
            std::ptr::from_mut(&mut vtable).cast::<u8>(),
            n,
        );
    }
    vtable.struct_size = std::mem::size_of::<BaeEffectVTable1>() as u32;
    let setup = vtable.global_setup.ok_or("no global_setup")?;
    if vtable.render_cpu.is_none() {
        return Err("no render_cpu: GPU-only native effects are not run by this BAE yet".into());
    }
    let contribution_id = CString::new(id).unwrap_or_default();
    let args = BaeGlobalSetupArgs {
        struct_size: std::mem::size_of::<BaeGlobalSetupArgs>() as u32,
        reserved: 0,
        host: &host.host,
        contribution_id: contribution_id.as_ptr(),
        locale: c"en-US".as_ptr(),
    };
    let mut desc = BaeEffectDescription {
        struct_size: std::mem::size_of::<BaeEffectDescription>() as u32,
        effect_class: BAE_EFFECT_CLASS_FILTER,
        flags: 0,
        threading: BAE_THREADING_SERIAL_GLOBAL,
        devices: BAE_DEVICE_CPU,
        gpu_formats: 0,
        cpu_format_count: 0,
        cpu_formats: std::ptr::null(),
    };
    let mut global = std::ptr::null_mut();
    take_error();
    // SAFETY: valid arguments; the plugin data is the one the entry point returned.
    let status = unsafe { setup(exports.plugin_data, &args, &mut desc, &mut global) };
    if status != BAE_OK {
        return Err(status_error(status, "global_setup").to_string());
    }
    let formats = if desc.cpu_formats.is_null() {
        Vec::new()
    } else {
        // SAFETY: valid until global_setdown (spec §14).
        unsafe { std::slice::from_raw_parts(desc.cpu_formats, desc.cpu_format_count as usize) }
            .to_vec()
    };
    let mut effect = LoadedEffect {
        info: EffectInfo {
            id: id.to_owned(),
            class: desc.effect_class,
            flags: desc.flags,
            threading: desc.threading,
            devices: desc.devices,
            cpu_formats: formats,
            has_pre_render: vtable.pre_render.is_some(),
            params: Vec::new(),
        },
        vtable,
        global,
        serial: (desc.threading != BAE_THREADING_CONCURRENT).then(|| Mutex::new(())),
    };
    let check = || -> Result<(), String> {
        if effect.info.class != BAE_EFFECT_CLASS_FILTER {
            return Err(format!(
                "effect class {} is not run by this BAE yet (filters only)",
                effect.info.class
            ));
        }
        if effect.info.devices & BAE_DEVICE_CPU == 0 {
            return Err("the effect does not declare the CPU device".into());
        }
        if effect.info.flags & BAE_EFFECT_FLAG_STATEFUL != 0 {
            return Err("stateful effects (instance state) are not run by this BAE yet".into());
        }
        if pixels::choose_format(&effect.info.cpu_formats).is_none() {
            return Err("none of the effect's CPU pixel formats is supported (RGBA F32 or U8, premultiplied)".into());
        }
        Ok(())
    };
    if let Err(e) = check() {
        teardown(&effect);
        return Err(e);
    }
    if let Some(define) = vtable.define_params {
        let mut collector = ParamCollector::default();
        take_error();
        // SAFETY: the builder is ours for the duration of the call.
        let status = unsafe { define(global, std::ptr::from_mut(&mut collector).cast()) };
        if status != BAE_OK || collector.error.is_some() {
            teardown(&effect);
            return Err(collector
                .error
                .unwrap_or_else(|| status_error(status, "define_params").to_string()));
        }
        effect.info.params = collector.params;
    }
    Ok(effect)
}

// ---------------------------------------------------------------------------
// Frame data
// ---------------------------------------------------------------------------

fn param_values(infos: &[ParamInfo], wire: &[ParamWire]) -> Vec<BaeParamValue> {
    infos
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let mut v = BaeParamValueUnion { reserved: [0; 32] };
            match wire.get(i).copied().unwrap_or(ParamWire::None) {
                ParamWire::Float(f) => v.f = f,
                ParamWire::Int(n) => v.i = n,
                ParamWire::Bool(b) => v.b = u32::from(b),
                ParamWire::Color(c) => v.color = c,
                ParamWire::Point(xyz) => v.point = xyz,
                ParamWire::Choice(c) => v.choice = c,
                ParamWire::None => {}
            }
            BaeParamValue {
                id: p.id,
                type_: p.ty,
                v,
            }
        })
        .collect()
}

fn render_info(f: &FrameRequest, values: &[BaeParamValue]) -> BaeRenderInfo {
    let rational = |[num, den]: [i32; 2]| BaeRational { num, den };
    BaeRenderInfo {
        struct_size: std::mem::size_of::<BaeRenderInfo>() as u32,
        quality: f.quality,
        time: f.time,
        time_step: f.time_step,
        frame_rate: rational(f.frame_rate),
        downsample_x: rational(f.downsample),
        downsample_y: rational(f.downsample),
        pixel_aspect: BaeRational { num: 1, den: 1 },
        shutter_angle: f.shutter[0],
        shutter_phase: f.shutter[1],
        color_space: f.color_space,
        flags: f.flags,
        layer_bounds: rect(f.layer_bounds),
        render_id: f.render_id,
        params: BaeParamValues {
            count: values.len() as u32,
            reserved: 0,
            values: values.as_ptr(),
        },
        layer_duration: f.layer_duration,
    }
}

/// What `BaeRenderCtx` points to during one call.
struct RenderCtx<'a> {
    input: Option<BaeCpuImage>,
    input_rect: BaeRect,
    layer_bounds: BaeRect,
    values: &'a [BaeParamValue],
    time: BaeFlicks,
    pre_render: bool,
    /// Rect of the self input checked out in pre_render (id 0 or any id).
    self_checkout: Option<BaeRect>,
    self_ids: Vec<u32>,
    temps: Vec<AlignedBuf>,
}

impl<'a> RenderCtx<'a> {
    fn new(f: &FrameRequest, values: &'a [BaeParamValue], input: Option<BaeCpuImage>) -> Self {
        Self {
            input,
            input_rect: rect(f.input),
            layer_bounds: rect(f.layer_bounds),
            values,
            time: f.time,
            pre_render: true,
            self_checkout: None,
            self_ids: vec![0],
            temps: Vec::new(),
        }
    }

    fn phase_render(&mut self) {
        self.pre_render = false;
    }

    fn as_raw(&mut self) -> *mut BaeRenderCtx {
        std::ptr::from_mut(self).cast()
    }
}

/// # Safety
/// `ctx` is a `RenderCtx` handed to the current callback.
unsafe fn ctx_mut<'a>(ctx: *mut BaeRenderCtx) -> Option<&'a mut RenderCtx<'a>> {
    // SAFETY: guaranteed by the caller.
    unsafe { ctx.cast::<RenderCtx<'a>>().as_mut() }
}

/// Writes `value` into a host-sized out-struct, at most `struct_size` bytes (R3).
///
/// # Safety
/// `out` points to a struct whose first field is `struct_size`.
unsafe fn write_out<T: Copy>(out: *mut T, mut value: T) {
    // SAFETY: guaranteed by the caller.
    unsafe {
        let size = (*out.cast::<u32>()) as usize;
        let n = size.min(std::mem::size_of::<T>());
        *std::ptr::from_mut(&mut value).cast::<u32>() = size as u32;
        std::ptr::copy_nonoverlapping(std::ptr::from_ref(&value).cast::<u8>(), out.cast::<u8>(), n);
    }
}

// ---------------------------------------------------------------------------
// Suites
// ---------------------------------------------------------------------------

/// # Safety
/// `handle` is the `HostState` of a loaded library.
unsafe fn state<'a>(handle: *mut BaeHostHandle) -> Option<&'a HostState> {
    // SAFETY: guaranteed by the caller.
    unsafe { handle.cast::<HostState>().as_ref() }
}

unsafe extern "C" fn get_suite(
    _host: *mut BaeHostHandle,
    name: *const c_char,
    version: u32,
) -> *const c_void {
    if name.is_null() {
        return std::ptr::null();
    }
    // SAFETY: NUL-terminated suite name from the plugin.
    let name = unsafe { CStr::from_ptr(name) };
    match (name.to_bytes(), version) {
        (b"bae.core", 1) => std::ptr::from_ref(&CORE).cast(),
        (b"bae.param.define", 1) => std::ptr::from_ref(&PARAM_DEFINE).cast(),
        (b"bae.render", 1) => std::ptr::from_ref(&RENDER).cast(),
        _ => std::ptr::null(),
    }
}

static CORE: BaeCoreSuite1 = BaeCoreSuite1 {
    log: Some(core_log),
    set_error: Some(core_set_error),
    alloc: Some(core_alloc),
    free: Some(core_free),
    has_permission: Some(core_has_permission),
    get_path: Some(core_get_path),
};

static PARAM_DEFINE: BaeParamDefineSuite1 = BaeParamDefineSuite1 {
    add: Some(param_add),
};

static RENDER: BaeRenderSuite1 = BaeRenderSuite1 {
    checkout_input: Some(render_checkout_input),
    checkout_param: Some(render_checkout_param),
    get_input_cpu: Some(render_get_input_cpu),
    get_input_gpu: Some(render_get_input_gpu),
    is_aborted: Some(render_is_aborted),
    report_progress: Some(render_report_progress),
    alloc_cpu_image: Some(render_alloc_cpu_image),
    parallel_for: Some(render_parallel_for),
};

unsafe extern "C" fn core_log(host: *mut BaeHostHandle, level: u32, message: *const c_char) {
    // SAFETY: handle and string from the plugin.
    let (Some(state), Some(text)) = (unsafe { state(host) }, unsafe { c_text(message) }) else {
        return;
    };
    let mut logs = state.logs.lock();
    if logs.len() < MAX_LOGS {
        logs.push((level, text));
    }
}

unsafe extern "C" fn core_set_error(_host: *mut BaeHostHandle, message: *const c_char) {
    // SAFETY: string from the plugin.
    let text = unsafe { c_text(message) };
    LAST_ERROR.with(|e| *e.borrow_mut() = text);
}

/// Allocation header: total size and alignment, stored right before the block.
const HEADER: usize = 16;

unsafe extern "C" fn core_alloc(
    _host: *mut BaeHostHandle,
    size: usize,
    align: usize,
) -> *mut c_void {
    let align = align.max(HEADER).next_power_of_two();
    let Some(total) = size.checked_add(align) else {
        return std::ptr::null_mut();
    };
    let Ok(layout) = std::alloc::Layout::from_size_align(total, align) else {
        return std::ptr::null_mut();
    };
    // SAFETY: non-zero size (align ≥ 16).
    let base = unsafe { std::alloc::alloc(layout) };
    if base.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: the header fits in the first `align` (≥ 16) bytes.
    unsafe {
        let user = base.add(align);
        user.sub(HEADER)
            .cast::<[usize; 2]>()
            .write_unaligned([total, align]);
        user.cast()
    }
}

unsafe extern "C" fn core_free(_host: *mut BaeHostHandle, ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: `ptr` came from `core_alloc`, which wrote the header.
    unsafe {
        let user = ptr.cast::<u8>();
        let [total, align] = user.sub(HEADER).cast::<[usize; 2]>().read_unaligned();
        let layout = std::alloc::Layout::from_size_align_unchecked(total, align);
        std::alloc::dealloc(user.sub(align), layout);
    }
}

unsafe extern "C" fn core_has_permission(
    _host: *mut BaeHostHandle,
    _permission: *const c_char,
) -> BaeBool {
    // No permissioned host API is served to native plugins yet (spec §21.3).
    BAE_FALSE
}

unsafe extern "C" fn core_get_path(
    host: *mut BaeHostHandle,
    which: u32,
    out: *mut BaeWriter,
) -> BaeStatus {
    // SAFETY: handle and writer from the plugin.
    let (Some(state), Some(out)) = (unsafe { state(host) }, unsafe { out.as_ref() }) else {
        return BAE_ERR_INVALID_ARGUMENT;
    };
    let path = match which {
        BAE_PATH_PLUGIN_DIR => state.plugin_dir.clone(),
        BAE_PATH_TEMP_DIR => std::env::temp_dir().to_string_lossy().into_owned(),
        _ => return BAE_ERR_UNSUPPORTED,
    };
    match out.write {
        // SAFETY: the plugin's writer, called with valid bytes.
        Some(write) => unsafe { write(out.opaque, path.as_ptr().cast(), path.len()) },
        None => BAE_ERR_INVALID_ARGUMENT,
    }
}

#[derive(Default)]
struct ParamCollector {
    params: Vec<ParamInfo>,
    error: Option<String>,
}

fn convert_param(def: &BaeParamDef) -> Result<ParamInfo, String> {
    // SAFETY: strings from the plugin, valid during the call.
    let (key, name) = unsafe { (c_text(def.key), c_text(def.name)) };
    let key = key.ok_or("param without a key")?;
    crate::id::check_param_key(&key).map_err(|e| format!("param {}: {e}", def.id))?;
    if def.id == 0 {
        return Err(format!("param `{key}`: id must be > 0"));
    }
    let mut p = ParamInfo {
        id: def.id,
        ty: def.type_,
        group_id: def.group_id,
        name: name.unwrap_or_else(|| key.clone()),
        key,
        flags: def.flags,
        default: [0.0; 4],
        range: [0.0, 0.0],
        slider: [0.0, 0.0],
        precision: 1,
        display: BAE_DISPLAY_NUMBER,
        choices: Vec::new(),
    };
    // SAFETY: the union member read matches the type tag.
    unsafe {
        match def.type_ {
            BAE_PARAM_FLOAT => {
                let f = def.u.float_def;
                p.default[0] = f.default_value;
                p.range = [f.valid_min, f.valid_max];
                p.slider = [f.slider_min, f.slider_max];
                p.precision = f.precision;
                p.display = f.display;
            }
            BAE_PARAM_INT => {
                let f = def.u.int_def;
                p.default[0] = f64::from(f.default_value);
                p.range = [f64::from(f.valid_min), f64::from(f.valid_max)];
                p.slider = [f64::from(f.slider_min), f64::from(f.slider_max)];
                p.display = f.display;
            }
            BAE_PARAM_ANGLE => p.default[0] = def.u.angle_def.default_degrees,
            BAE_PARAM_CHECKBOX => p.default[0] = f64::from(def.u.checkbox_def.default_value),
            BAE_PARAM_COLOR => {
                p.default = def.u.color_def.default_rgba.map(f64::from);
            }
            BAE_PARAM_POINT | BAE_PARAM_POINT_3D => {
                let d = def.u.point_def.default_xyz;
                p.default = [d[0], d[1], d[2], 0.0];
            }
            BAE_PARAM_POPUP => {
                let d = def.u.popup_def;
                p.default[0] = f64::from(d.default_id);
                if !d.choices.is_null() {
                    for c in std::slice::from_raw_parts(d.choices, d.choice_count as usize) {
                        p.choices.push((c.id, c_text(c.name).unwrap_or_default()));
                    }
                }
                if p.choices.is_empty() {
                    return Err(format!("popup `{}` has no choices", p.key));
                }
            }
            BAE_PARAM_GROUP | BAE_PARAM_BUTTON => {}
            other => {
                return Err(format!(
                    "param `{}`: type {other} is not supported by native effects in this BAE yet",
                    p.key
                ));
            }
        }
    }
    Ok(p)
}

unsafe extern "C" fn param_add(
    builder: *mut BaeParamBuilder,
    def: *const BaeParamDef,
) -> BaeStatus {
    // SAFETY: our collector, and the plugin's definition for the call.
    let (Some(c), Some(def)) = (
        unsafe { builder.cast::<ParamCollector>().as_mut() },
        unsafe { def.as_ref() },
    ) else {
        return BAE_ERR_INVALID_ARGUMENT;
    };
    if (def.struct_size as usize) < std::mem::size_of::<BaeParamDef>() {
        c.error = Some("BaeParamDef.struct_size is smaller than ABI 1.0".into());
        return BAE_ERR_INVALID_ARGUMENT;
    }
    match convert_param(def) {
        Ok(p) if c.params.iter().any(|q| q.id == p.id || q.key == p.key) => {
            c.error = Some(format!("param {} `{}` is defined twice", p.id, p.key));
            BAE_ERR_INVALID_ARGUMENT
        }
        Ok(p) => {
            c.params.push(p);
            BAE_OK
        }
        Err(e) => {
            c.error = Some(e);
            BAE_ERR_UNSUPPORTED
        }
    }
}

unsafe extern "C" fn render_checkout_input(
    ctx: *mut BaeRenderCtx,
    request: *const BaeInputRequest,
    out_info: *mut BaeInputInfo,
) -> BaeStatus {
    // SAFETY: our context and the plugin's structs for the call.
    let (Some(ctx), Some(req)) = (unsafe { ctx_mut(ctx) }, unsafe { request.as_ref() }) else {
        return BAE_ERR_INVALID_ARGUMENT;
    };
    if !ctx.pre_render || out_info.is_null() {
        return BAE_ERR_BAD_STATE;
    }
    let mut info = BaeInputInfo {
        struct_size: std::mem::size_of::<BaeInputInfo>() as u32,
        pixel_aspect: BaeRational { num: 1, den: 1 },
        ..BaeInputInfo::default()
    };
    if req.source == BAE_INPUT_SELF {
        ctx.self_checkout = Some(match ctx.self_checkout {
            Some(r) => BaeRect::new(
                r.x0.min(req.rect.x0),
                r.y0.min(req.rect.y0),
                r.x1.max(req.rect.x1),
                r.y1.max(req.rect.y1),
            ),
            None => req.rect,
        });
        ctx.self_ids.push(req.checkout_id);
        info.result_rect = ctx.input_rect.intersect(&req.rect);
        info.max_result_rect = ctx.input_rect;
        info.layer_bounds = ctx.layer_bounds;
    } else {
        // Layer params and transition inputs are not rendered for native
        // effects yet: they are empty (spec: "layer param is none").
        info.flags = BAE_INPUT_INFO_EMPTY;
    }
    // SAFETY: host-sized out-struct (R3).
    unsafe { write_out(out_info, info) };
    BAE_OK
}

unsafe extern "C" fn render_checkout_param(
    ctx: *mut BaeRenderCtx,
    param_id: u32,
    time: BaeFlicks,
    out_value: *mut BaeParamValue,
) -> BaeStatus {
    // SAFETY: our context for the call.
    let Some(ctx) = (unsafe { ctx_mut(ctx) }) else {
        return BAE_ERR_INVALID_ARGUMENT;
    };
    if out_value.is_null() {
        return BAE_ERR_INVALID_ARGUMENT;
    }
    if time != ctx.time {
        // Values at other times need the render graph (WIDE_TIME_INPUT): later.
        return BAE_ERR_UNSUPPORTED;
    }
    match ctx.values.iter().find(|v| v.id == param_id) {
        Some(v) => {
            // SAFETY: valid out pointer from the plugin.
            unsafe { *out_value = *v };
            BAE_OK
        }
        None => BAE_ERR_NOT_FOUND,
    }
}

unsafe extern "C" fn render_get_input_cpu(
    ctx: *mut BaeRenderCtx,
    checkout_id: u32,
    out_image: *mut BaeCpuImage,
) -> BaeStatus {
    // SAFETY: our context for the call.
    let Some(ctx) = (unsafe { ctx_mut(ctx) }) else {
        return BAE_ERR_INVALID_ARGUMENT;
    };
    if out_image.is_null() || ctx.pre_render {
        return BAE_ERR_BAD_STATE;
    }
    match ctx.input {
        Some(image) if ctx.self_ids.contains(&checkout_id) => {
            // SAFETY: plugin-sized out-struct (R3).
            unsafe { write_out(out_image, image) };
            BAE_OK
        }
        _ => BAE_ERR_NOT_FOUND,
    }
}

unsafe extern "C" fn render_get_input_gpu(
    _ctx: *mut BaeRenderCtx,
    _checkout_id: u32,
    _out_image: *mut *mut BaeGpuImage,
) -> BaeStatus {
    BAE_ERR_UNSUPPORTED
}

unsafe extern "C" fn render_is_aborted(_ctx: *mut BaeRenderCtx) -> BaeBool {
    BAE_FALSE
}

unsafe extern "C" fn render_report_progress(_ctx: *mut BaeRenderCtx, _done: u64, _total: u64) {}

unsafe extern "C" fn render_alloc_cpu_image(
    ctx: *mut BaeRenderCtx,
    format: BaePixelFormat,
    rect: BaeRect,
    out_image: *mut BaeCpuImage,
) -> BaeStatus {
    // SAFETY: our context for the call.
    let Some(ctx) = (unsafe { ctx_mut(ctx) }) else {
        return BAE_ERR_INVALID_ARGUMENT;
    };
    if ctx.pre_render || out_image.is_null() {
        return BAE_ERR_BAD_STATE;
    }
    if !pixels::HOST_FORMATS.contains(&format) || rect.is_empty() {
        return BAE_ERR_UNSUPPORTED;
    }
    let mut buf = AlignedBuf::with_len(pixels::image_len(format, rect));
    let image = BaeCpuImage {
        struct_size: std::mem::size_of::<BaeCpuImage>() as u32,
        format,
        rect,
        row_bytes: pixels::row_bytes(format, rect.width()) as i64,
        data: buf.as_mut_slice().as_mut_ptr().cast(),
        flags: BAE_IMAGE_FLAG_WRITABLE,
        reserved: 0,
    };
    // The buffer's heap block does not move when the Vec of buffers grows.
    ctx.temps.push(buf);
    // SAFETY: plugin-sized out-struct (R3).
    unsafe { write_out(out_image, image) };
    BAE_OK
}

struct ParallelJob {
    f: BaeParallelFn,
    user: *mut c_void,
    next: AtomicU32,
    count: u32,
    error: AtomicU32,
}

// SAFETY: the plugin asked for `f(user, …)` to be called from several threads.
unsafe impl Sync for ParallelJob {}

impl ParallelJob {
    fn run(&self, thread: u32) {
        loop {
            let i = self.next.fetch_add(1, Ordering::Relaxed);
            if i >= self.count || self.error.load(Ordering::Relaxed) != 0 {
                break;
            }
            // SAFETY: the plugin's callback with its own user pointer.
            let status = unsafe { (self.f)(self.user, i, thread) };
            if status != BAE_OK {
                let _ = self.error.compare_exchange(
                    0,
                    status as u32,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                );
            }
        }
    }
}

unsafe extern "C" fn render_parallel_for(
    _ctx: *mut BaeRenderCtx,
    count: u32,
    f: BaeParallelFn,
    user: *mut c_void,
) -> BaeStatus {
    let job = ParallelJob {
        f,
        user,
        next: AtomicU32::new(0),
        count,
        error: AtomicU32::new(0),
    };
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(count as usize)
        .max(1);
    let job = &job;
    std::thread::scope(|s| {
        for t in 1..threads {
            s.spawn(move || job.run(t as u32));
        }
        job.run(0);
    });
    job.error.load(Ordering::Relaxed) as i32
}
