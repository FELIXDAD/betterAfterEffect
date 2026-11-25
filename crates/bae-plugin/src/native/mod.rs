//! Native plugins (spec §7.2, §8, §14, §15): libraries behind the stable C
//! ABI (`sdk/c/bae_plugin.h`, ABI 1.0), loaded with `libloading`.
//!
//! * [`library`]: the library in this process — entry point, host suites,
//!   effect callbacks.
//! * [`runner`]: the same library in a child process (crash isolation).
//! * [`NativeEffect`] implements `bae_render::Effect` like the declarative and
//!   built-in effects: per frame it reads the input back from the GPU, calls
//!   the plugin's `render_cpu` (CPU path, spec §10.6) and uploads the result.
//!
//! **Isolation.** A crash must never take BAE down. Native plugins therefore
//! run out of process unless they are trusted for in-process use: bundled and
//! system plugins whose manifest says `isolation = "in-process"`, or plugins
//! the user switched to in-process ([`NativeConfig::in_process`],
//! `BAE_PLUGIN_IN_PROCESS`). In process, a panic or an error is reported and
//! the plugin is disabled; out of process, a crash or hang kills only the
//! runner, which restarts on the next frame (three crashes within a minute
//! disable the plugin for the session). A failing effect passes its input
//! through, so the frame still renders; every failure is posted to
//! [`Notices`] for the UI.

pub mod library;
pub mod pixels;
pub mod runner;
mod shm;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bae_gpu::transfer::{self, TextureReadback, TextureRegion};
use bae_gpu::wgpu;
use bae_model::{EffectDef, Value};
use bae_plugin_sdk::abi::*;
use bae_plugin_sdk::{ApiVersion, HostApiSupport};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, ImageAlpha, RenderError};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::diag::Diagnostic;
use crate::host::Scope;
use crate::manifest::{
    EffectDecl, EffectImplementation, Isolation, MANIFEST_FILE, NumberDisplay, ParamDecl,
    ParamFlag, ParamType, PluginManifest,
};
use library::Library;
use pixels::AlignedBuf;
pub use runner::{RunnerClient, RunnerCommand, runner_main};

/// Plugin ids allowed to run in process (comma-separated, `*` = all), for
/// plugin development and trusted plugins.
pub const IN_PROCESS_ENV: &str = "BAE_PLUGIN_IN_PROCESS";
/// A dedicated runner executable (no arguments) instead of `bae plugin runner`.
pub const RUNNER_ENV: &str = "BAE_PLUGIN_RUNNER";
const RUNNER_CHILD_ENV: &str = "BAE_PLUGIN_RUNNER_CHILD";

/// How the host runs native plugins.
#[derive(Clone, Debug, Default)]
pub struct NativeConfig {
    /// Runner process command; `None` = [`RUNNER_ENV`] or `bae plugin runner`.
    pub runner: Option<RunnerCommand>,
    /// Plugin ids trusted to run in process (`*` = every plugin).
    pub in_process: Vec<String>,
    /// No UI (`BAE_HOST_CAP_HEADLESS`).
    pub headless: bool,
}

impl NativeConfig {
    /// Defaults plus [`IN_PROCESS_ENV`] and [`RUNNER_ENV`].
    pub fn from_env() -> Self {
        Self {
            runner: std::env::var_os(RUNNER_ENV)
                .filter(|v| !v.is_empty())
                .map(RunnerCommand::program),
            in_process: std::env::var(IN_PROCESS_ENV)
                .map(|v| {
                    v.split(',')
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            headless: false,
        }
    }

    fn trusted(&self, plugin_id: &str) -> bool {
        self.in_process.iter().any(|t| t == "*" || t == plugin_id)
    }

    /// Whether plugin `id` from `scope` with manifest isolation `isolation`
    /// runs out of process (see the module documentation).
    pub fn out_of_process(&self, id: &str, scope: Scope, isolation: Isolation) -> bool {
        match isolation {
            Isolation::OutOfProcess => true,
            Isolation::PreferOutOfProcess => !self.trusted(id),
            Isolation::InProcess => {
                !(matches!(scope, Scope::Bundled | Scope::System) || self.trusted(id))
            }
        }
    }

    fn runner_command(&self) -> Result<RunnerCommand, String> {
        match &self.runner {
            Some(r) => Ok(r.clone()),
            None => RunnerCommand::current_exe()
                .map_err(|e| format!("cannot locate the plugin runner: {e}")),
        }
    }
}

/// The plugin API versions this host serves (ABI 1.0 and the 0.1 pre-release).
pub const HOST_API: HostApiSupport = HostApiSupport::SDK_DEFAULT;

// ---------------------------------------------------------------------------
// Notices
// ---------------------------------------------------------------------------

/// What happened to a plugin (the UI words it, in its language).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoticeKind {
    /// The plugin could not be loaded; its effects show as missing.
    LoadFailed,
    /// An effect failed on a frame and passed its input through.
    RenderFailed,
    /// The plugin process crashed or hung and will be restarted.
    Crashed,
    /// The plugin was disabled for this session.
    Disabled,
}

/// A message about a plugin for the user.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Notice {
    /// What happened.
    pub kind: NoticeKind,
    /// Plugin id.
    pub plugin: String,
    /// Details (English, from the plugin or the host).
    pub detail: String,
}

impl Notice {
    /// English text.
    pub fn text(&self) -> String {
        let what = match self.kind {
            NoticeKind::LoadFailed => "could not be loaded and is disabled",
            NoticeKind::RenderFailed => "failed to render a frame",
            NoticeKind::Crashed => "crashed and will be restarted",
            NoticeKind::Disabled => "stopped working and was disabled for this session",
        };
        format!("Plugin {} {what}: {}", self.plugin, self.detail)
    }
}

/// Notices waiting for the UI (shared by the host and every native plugin).
#[derive(Clone, Debug, Default)]
pub struct Notices(Arc<Mutex<Vec<Notice>>>);

impl Notices {
    /// Posts a notice (also logged).
    pub fn post(&self, notice: Notice) {
        tracing::warn!("{}", notice.text());
        let mut list = self.0.lock();
        if list.len() < 64 {
            list.push(notice);
        }
    }

    /// Takes the pending notices.
    pub fn take(&self) -> Vec<Notice> {
        std::mem::take(&mut *self.0.lock())
    }
}

// ---------------------------------------------------------------------------
// Data exchanged with a library (in process or through the runner)
// ---------------------------------------------------------------------------

/// A parameter as the plugin defined it (`BaeParamDef`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamInfo {
    /// Stable id.
    pub id: u32,
    /// `BAE_PARAM_*`.
    pub ty: u32,
    /// Enclosing group id (0 = top level).
    pub group_id: u32,
    /// Stable key.
    pub key: String,
    /// Default display name.
    pub name: String,
    /// `BAE_PARAM_FLAG_*`.
    pub flags: u64,
    /// Default value (scalar in `[0]`; color RGBA; point xyz; popup choice id).
    pub default: [f64; 4],
    /// Valid range (float, int).
    pub range: [f64; 2],
    /// Slider range (float, int).
    pub slider: [f64; 2],
    /// Decimal places (float).
    pub precision: u32,
    /// `BAE_DISPLAY_*`.
    pub display: u32,
    /// Popup choices `(id, name)`.
    pub choices: Vec<(u32, String)>,
}

/// An effect contribution after `global_setup` / `define_params`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectInfo {
    /// Local effect id.
    pub id: String,
    /// `BAE_EFFECT_CLASS_*`.
    pub class: u32,
    /// `BAE_EFFECT_FLAG_*`.
    pub flags: u64,
    /// `BAE_THREADING_*`.
    pub threading: u32,
    /// `BAE_DEVICE_*` mask.
    pub devices: u32,
    /// CPU formats in the plugin's preference order.
    pub cpu_formats: Vec<u32>,
    /// The effect has a `pre_render` (ROI, bounds, identity).
    pub has_pre_render: bool,
    /// Parameters in definition order.
    pub params: Vec<ParamInfo>,
}

/// What a library exported.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LibraryInfo {
    /// `plugin_id` from the exports.
    pub plugin_id: String,
    /// `plugin_version` from the exports.
    pub plugin_version: String,
    /// `abi_built`.
    pub abi_built: u32,
    /// `abi_required`.
    pub abi_required: u32,
    /// Effect contributions.
    pub effects: Vec<EffectInfo>,
    /// Contributions of kinds this host does not run yet.
    pub ignored: Vec<String>,
}

/// One evaluated parameter value.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ParamWire {
    /// No value (groups, buttons).
    None,
    /// FLOAT, ANGLE.
    Float(f64),
    /// INT.
    Int(i32),
    /// CHECKBOX.
    Bool(bool),
    /// COLOR (straight RGBA).
    Color([f32; 4]),
    /// POINT, POINT_3D (render pixels).
    Point([f64; 3]),
    /// POPUP (choice id).
    Choice(u32),
}

/// One frame request (the serializable form of `BaeRenderInfo` + rects).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrameRequest {
    /// Effect index in [`LibraryInfo::effects`].
    pub effect: u32,
    /// CPU pixel format of both images.
    pub format: u32,
    /// Layer time (flicks).
    pub time: i64,
    /// One comp frame (flicks).
    pub time_step: i64,
    /// Comp frame rate.
    pub frame_rate: [i32; 2],
    /// Downsample factor (render pixels per layer pixel).
    pub downsample: [i32; 2],
    /// `BAE_QUALITY_*`.
    pub quality: u32,
    /// `BAE_RENDER_FLAG_*`.
    pub flags: u32,
    /// `BAE_COLOR_SPACE_*`.
    pub color_space: u32,
    /// Shutter angle and phase (degrees).
    pub shutter: [f32; 2],
    /// Layer bounds (render pixels).
    pub layer_bounds: [i32; 4],
    /// Unique per request.
    pub render_id: u64,
    /// Layer duration (flicks).
    pub layer_duration: i64,
    /// Values in definition order.
    pub params: Vec<ParamWire>,
    /// Region of the output the host needs (render pixels).
    pub request: [i32; 4],
    /// Region the input image covers (render pixels).
    pub input: [i32; 4],
}

/// Result of a `pre_render` query.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreRenderOut {
    /// `BAE_PRERENDER_*`.
    pub flags: u32,
    /// What the effect will produce.
    pub result: [i32; 4],
    /// The largest output for any request.
    pub max_result: [i32; 4],
    /// The self-input region checked out.
    pub input: Option<[i32; 4]>,
}

/// Result of a frame.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum RenderOut {
    /// Output = input (render skipped).
    Identity,
    /// The output region that was written (possibly empty).
    Rendered([i32; 4]),
}

/// Why a call into a plugin failed.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum CallError {
    /// The plugin returned an error status.
    #[error("{message}")]
    Status {
        /// `BAE_ERR_*`.
        status: i32,
        /// The plugin's message.
        message: String,
    },
    /// The runner process died or hung.
    #[error("{message}")]
    Crashed {
        /// What happened.
        message: String,
        /// Too many crashes: the plugin is disabled for the session.
        disabled: bool,
    },
    /// The plugin is disabled for this session.
    #[error("the plugin is disabled for this session")]
    Disabled,
    /// A host-side problem (protocol, buffers, missing runner …).
    #[error("{0}")]
    Failed(String),
}

pub(crate) fn rect(r: [i32; 4]) -> BaeRect {
    BaeRect::new(r[0], r[1], r[2], r[3])
}

pub(crate) fn rect_of(r: BaeRect) -> [i32; 4] {
    [r.x0, r.y0, r.x1, r.y1]
}

// ---------------------------------------------------------------------------
// A loaded native plugin
// ---------------------------------------------------------------------------

enum Backend {
    InProcess(Library),
    OutOfProcess(RunnerClient),
}

/// A native library of one plugin, in or out of process, with its health.
pub struct NativePlugin {
    id: String,
    backend: Backend,
    info: LibraryInfo,
    disabled: AtomicBool,
    notices: Notices,
    /// Render errors already reported (one notice per distinct message).
    reported: Mutex<BTreeSet<String>>,
}

impl std::fmt::Debug for NativePlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativePlugin")
            .field("id", &self.id)
            .field("out_of_process", &self.is_out_of_process())
            .finish_non_exhaustive()
    }
}

impl NativePlugin {
    /// Plugin id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// What the library exported.
    pub fn info(&self) -> &LibraryInfo {
        &self.info
    }

    /// Runs in a runner process.
    pub fn is_out_of_process(&self) -> bool {
        matches!(self.backend, Backend::OutOfProcess(_))
    }

    /// Disabled for this session after a crash or panic.
    pub fn is_disabled(&self) -> bool {
        self.disabled.load(Ordering::Relaxed)
    }

    /// The runner client (out-of-process plugins).
    pub fn runner(&self) -> Option<&RunnerClient> {
        match &self.backend {
            Backend::OutOfProcess(c) => Some(c),
            Backend::InProcess(_) => None,
        }
    }

    fn logs(&self) {
        if let Backend::InProcess(lib) = &self.backend {
            for (level, line) in lib.take_logs() {
                runner::log_line(&self.id, level, &line);
            }
        }
    }

    fn pre_render(&self, frame: &FrameRequest) -> Result<PreRenderOut, CallError> {
        if self.is_disabled() {
            return Err(CallError::Disabled);
        }
        let out = match &self.backend {
            Backend::InProcess(lib) => lib.pre_render(frame),
            Backend::OutOfProcess(c) => c.pre_render(frame),
        };
        self.logs();
        out
    }

    fn render(
        &self,
        frame: &FrameRequest,
        input: &[u8],
        output: &mut [u8],
    ) -> Result<RenderOut, CallError> {
        if self.is_disabled() {
            return Err(CallError::Disabled);
        }
        let out = match &self.backend {
            Backend::InProcess(lib) => lib.render(frame, input, output),
            Backend::OutOfProcess(c) => c.render(frame, input, output),
        };
        self.logs();
        out
    }

    /// Reports a failed call: notices, and disabling after a panic or
    /// repeated crashes.
    fn fail(&self, effect: &str, error: CallError) {
        let detail = format!("{effect}: {error}");
        let (kind, disable) = match &error {
            CallError::Disabled => return,
            CallError::Crashed { disabled, .. } => (
                if *disabled {
                    NoticeKind::Disabled
                } else {
                    NoticeKind::Crashed
                },
                *disabled,
            ),
            // A panic may have left the plugin's state broken.
            CallError::Status { status, .. } if *status == BAE_ERR_PANIC => {
                (NoticeKind::Disabled, true)
            }
            CallError::Status { .. } | CallError::Failed(_) => {
                if !self.reported.lock().insert(detail.clone()) {
                    return;
                }
                (NoticeKind::RenderFailed, false)
            }
        };
        if disable && self.disabled.swap(true, Ordering::Relaxed) {
            return;
        }
        self.notices.post(Notice {
            kind,
            plugin: self.id.clone(),
            detail,
        });
    }
}

/// Loads the native library of `manifest` for this platform (in or out of
/// process, see [`NativeConfig::out_of_process`]) and checks its exports
/// against the manifest (spec §7.2 step 4).
pub fn load_plugin(
    manifest: &PluginManifest,
    root: &Path,
    scope: Scope,
    config: &NativeConfig,
    notices: &Notices,
) -> Result<Arc<NativePlugin>, Vec<Diagnostic>> {
    let err = |m: String| vec![Diagnostic::error(MANIFEST_FILE, m)];
    if std::env::var_os(RUNNER_CHILD_ENV).is_some() {
        return Err(err(
            "a plugin runner cannot load nested native plugins".into()
        ));
    }
    let id = manifest.plugin.id.as_str();
    let native = manifest
        .native
        .as_ref()
        .ok_or_else(|| err("native effects need a [native] table".into()))?;
    let platform = crate::id::current_platform()
        .ok_or_else(|| err("native plugins are not supported on this platform".into()))?;
    let rel = native.libraries.get(platform).ok_or_else(|| {
        err(format!(
            "no native library for this platform ({platform}) in [native.libraries]"
        ))
    })?;
    let path: PathBuf = rel.split('/').fold(root.to_owned(), |p, s| p.join(s));
    if !path.is_file() {
        return Err(err(format!("native library `{rel}` has not been built")));
    }
    let presented = HOST_API
        .check(manifest.plugin.api)
        .map_err(|e| err(e.to_string()))?;
    let out_of_process = config.out_of_process(id, scope, native.isolation);
    let (backend, info) = if out_of_process {
        let command = config.runner_command().map_err(err)?;
        let (client, info) = RunnerClient::start(command, &path, id, root, presented.pack())
            .map_err(|e| err(format!("{rel}: {e}")))?;
        (Backend::OutOfProcess(client), info)
    } else {
        let caps = if config.headless {
            BAE_HOST_CAP_HEADLESS
        } else {
            0
        };
        let lib = Library::load(&path, id, root, presented.pack(), caps).map_err(err)?;
        let info = lib.info().clone();
        (Backend::InProcess(lib), info)
    };
    check_exports(manifest, &info).map_err(err)?;
    Ok(Arc::new(NativePlugin {
        id: id.to_owned(),
        backend,
        info,
        disabled: AtomicBool::new(false),
        notices: notices.clone(),
        reported: Mutex::new(BTreeSet::new()),
    }))
}

fn check_exports(manifest: &PluginManifest, info: &LibraryInfo) -> Result<(), String> {
    let p = &manifest.plugin;
    if info.plugin_id != p.id {
        return Err(format!(
            "the library exports plugin id `{}`, the manifest says `{}`",
            info.plugin_id, p.id
        ));
    }
    if info.plugin_version != p.version.to_string() {
        return Err(format!(
            "the library exports version {}, the manifest says {}",
            info.plugin_version, p.version
        ));
    }
    HOST_API
        .check_binary(p.api, info.abi_required)
        .map_err(|e| e.to_string())?;
    for decl in manifest
        .effects
        .iter()
        .filter(|e| e.implementation == EffectImplementation::Native)
    {
        if !info.effects.iter().any(|e| e.id == decl.id) {
            return Err(format!(
                "effect `{}` is declared native in the manifest but the library does not export it",
                decl.id
            ));
        }
    }
    for e in &info.effects {
        let declared = manifest
            .effects
            .iter()
            .any(|d| d.id == e.id && d.implementation == EffectImplementation::Native);
        if !declared {
            return Err(format!(
                "the library exports effect `{}`, which the manifest does not declare as native",
                e.id
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Parameters: plugin definitions ↔ manifest / document model
// ---------------------------------------------------------------------------

/// The manifest form of the plugin's parameter definitions (used when the
/// manifest has no listing copy).
pub fn param_decls(params: &[ParamInfo]) -> Vec<ParamDecl> {
    let display = |d: u32| match d {
        BAE_DISPLAY_PERCENT => Some(NumberDisplay::Percent),
        BAE_DISPLAY_PIXELS => Some(NumberDisplay::Pixels),
        BAE_DISPLAY_DEGREES => Some(NumberDisplay::Degrees),
        BAE_DISPLAY_SECONDS => Some(NumberDisplay::Seconds),
        BAE_DISPLAY_FRAMES => Some(NumberDisplay::Frames),
        _ => None,
    };
    params
        .iter()
        .filter_map(|p| {
            let (ty, default) = match p.ty {
                BAE_PARAM_FLOAT => (ParamType::Float, toml::Value::Float(p.default[0])),
                BAE_PARAM_INT => (ParamType::Int, toml::Value::Integer(p.default[0] as i64)),
                BAE_PARAM_ANGLE => (ParamType::Angle, toml::Value::Float(p.default[0])),
                BAE_PARAM_CHECKBOX => (
                    ParamType::Checkbox,
                    toml::Value::Boolean(p.default[0] != 0.0),
                ),
                BAE_PARAM_COLOR => (
                    ParamType::Color,
                    toml::Value::Array(p.default.iter().map(|v| toml::Value::Float(*v)).collect()),
                ),
                BAE_PARAM_POINT => (
                    ParamType::Point,
                    toml::Value::Array(
                        p.default[..2]
                            .iter()
                            .map(|v| toml::Value::Float(*v))
                            .collect(),
                    ),
                ),
                BAE_PARAM_POPUP => {
                    let index = p
                        .choices
                        .iter()
                        .position(|(id, _)| f64::from(*id) == p.default[0])
                        .unwrap_or(0);
                    (ParamType::Popup, toml::Value::Integer(index as i64 + 1))
                }
                _ => return None,
            };
            let numeric = matches!(ty, ParamType::Float | ParamType::Int);
            let mut flags = Vec::new();
            if p.flags & BAE_PARAM_FLAG_HIDDEN != 0 {
                flags.push(ParamFlag::Hidden);
            }
            if p.flags & BAE_PARAM_FLAG_NOT_ANIMATABLE != 0 {
                flags.push(ParamFlag::NotAnimatable);
            }
            Some(ParamDecl {
                id: p.id,
                key: p.key.clone(),
                ty,
                name: p.name.clone(),
                group: None,
                flags,
                hidden: false,
                disabled: false,
                default: Some(default),
                range: numeric.then_some(p.range),
                slider: numeric.then_some(p.slider),
                precision: (ty == ParamType::Float).then(|| p.precision.min(255) as u8),
                display: if numeric { display(p.display) } else { None },
                label: None,
                alpha: None,
                relative: (ty == ParamType::Point)
                    .then_some(p.flags & BAE_PARAM_FLAG_RELATIVE_DEFAULT != 0),
                choices: (ty == ParamType::Popup)
                    .then(|| p.choices.iter().map(|(_, n)| n.clone()).collect()),
                collapsed: None,
                end_id: None,
                max_length: None,
                multiline: None,
                extensions: None,
            })
        })
        .collect()
}

fn manifest_type(ty: u32) -> Option<ParamType> {
    Some(match ty {
        BAE_PARAM_FLOAT => ParamType::Float,
        BAE_PARAM_INT => ParamType::Int,
        BAE_PARAM_ANGLE => ParamType::Angle,
        BAE_PARAM_CHECKBOX => ParamType::Checkbox,
        BAE_PARAM_COLOR => ParamType::Color,
        BAE_PARAM_POINT => ParamType::Point,
        BAE_PARAM_POINT_3D => ParamType::Point3d,
        BAE_PARAM_POPUP => ParamType::Popup,
        BAE_PARAM_GROUP => ParamType::Group,
        BAE_PARAM_BUTTON => ParamType::Button,
        _ => return None,
    })
}

/// Checks the manifest's listing copy against `define_params` (spec §5.3.7:
/// the native code is authoritative, the copy must match).
pub fn check_listing(decl: &EffectDecl, info: &EffectInfo) -> Result<(), String> {
    if decl.params.is_empty() {
        return Ok(());
    }
    for p in &info.params {
        let Some(d) = decl.params.iter().find(|d| d.id == p.id) else {
            return Err(format!(
                "define_params adds param {} `{}`, missing from [[effect.param]]",
                p.id, p.key
            ));
        };
        if d.key != p.key || Some(d.ty) != manifest_type(p.ty) {
            return Err(format!(
                "param {}: [[effect.param]] says `{}` ({}), define_params says `{}` (type {})",
                p.id,
                d.key,
                d.ty.name(),
                p.key,
                p.ty
            ));
        }
    }
    if let Some(d) = decl
        .params
        .iter()
        .find(|d| !info.params.iter().any(|p| p.id == d.id))
    {
        return Err(format!(
            "[[effect.param]] lists param {} `{}`, which define_params does not add",
            d.id, d.key
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The render-graph effect
// ---------------------------------------------------------------------------

/// A native effect in the render graph (`bae_render::Effect`).
pub struct NativeEffect {
    def: EffectDef,
    index: u32,
    info: EffectInfo,
    format: BaePixelFormat,
    plugin: Arc<NativePlugin>,
    /// Reused CPU buffers (input, output).
    scratch: Mutex<(AlignedBuf, AlignedBuf)>,
}

impl std::fmt::Debug for NativeEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeEffect")
            .field("match_name", &self.def.match_name)
            .finish_non_exhaustive()
    }
}

impl NativeEffect {
    /// The effect `id` of a loaded plugin, with its definition `def`.
    pub fn new(plugin: &Arc<NativePlugin>, id: &str, def: EffectDef) -> Option<Self> {
        let index = plugin.info.effects.iter().position(|e| e.id == id)?;
        let info = plugin.info.effects[index].clone();
        let format = pixels::choose_format(&info.cpu_formats)?;
        Some(Self {
            def,
            index: index as u32,
            info,
            format,
            plugin: plugin.clone(),
            scratch: Mutex::new((AlignedBuf::default(), AlignedBuf::default())),
        })
    }

    /// The plugin.
    pub fn plugin(&self) -> &Arc<NativePlugin> {
        &self.plugin
    }

    /// The plugin's description of the effect.
    pub fn info(&self) -> &EffectInfo {
        &self.info
    }

    /// Evaluated values in definition order; points in render pixels at
    /// `1 / pixel_scale` per layer pixel.
    fn wire_params(&self, params: &EffectParams<'_>, pixel_scale: f64) -> Vec<ParamWire> {
        let to_render = 1.0 / pixel_scale.max(1e-9);
        let layer = params.layer_bounds;
        self.info
            .params
            .iter()
            .map(|p| {
                let v = params.value(p.id);
                let num = |v: Option<&Value>| match v {
                    Some(Value::Float(x)) => Some(*x),
                    Some(Value::Int(i)) => Some(*i as f64),
                    Some(Value::Bool(b)) => Some(f64::from(u8::from(*b))),
                    _ => None,
                };
                let clamp = |x: f64| {
                    if p.range[0] < p.range[1] {
                        x.clamp(p.range[0], p.range[1])
                    } else {
                        x
                    }
                };
                match p.ty {
                    BAE_PARAM_FLOAT => ParamWire::Float(clamp(num(v).unwrap_or(p.default[0]))),
                    BAE_PARAM_ANGLE => ParamWire::Float(num(v).unwrap_or(p.default[0])),
                    BAE_PARAM_INT => {
                        ParamWire::Int(clamp(num(v).unwrap_or(p.default[0]).round()) as i32)
                    }
                    BAE_PARAM_CHECKBOX => ParamWire::Bool(num(v).unwrap_or(p.default[0]) != 0.0),
                    BAE_PARAM_COLOR => ParamWire::Color(match v {
                        Some(Value::Color(c)) => c.map(|x| x as f32),
                        _ => p.default.map(|x| x as f32),
                    }),
                    BAE_PARAM_POINT | BAE_PARAM_POINT_3D => {
                        let xyz = match v {
                            Some(Value::Vec2([x, y])) => [*x, *y, 0.0],
                            Some(Value::Vec3([x, y, z])) => [*x, *y, *z],
                            _ if p.flags & BAE_PARAM_FLAG_RELATIVE_DEFAULT != 0 => [
                                layer.x0 as f64 + p.default[0] * f64::from(layer.width()),
                                layer.y0 as f64 + p.default[1] * f64::from(layer.height()),
                                p.default[2],
                            ],
                            _ => [p.default[0], p.default[1], p.default[2]],
                        };
                        ParamWire::Point(xyz.map(|c| c * to_render))
                    }
                    BAE_PARAM_POPUP => {
                        // The document stores the 1-based menu index (AE).
                        let index = num(v).map_or_else(
                            || {
                                p.choices
                                    .iter()
                                    .position(|(id, _)| f64::from(*id) == p.default[0])
                                    .unwrap_or(0)
                            },
                            |i| {
                                (i.round().max(1.0) as usize - 1)
                                    .min(p.choices.len().saturating_sub(1))
                            },
                        );
                        ParamWire::Choice(p.choices.get(index).map_or(0, |(id, _)| *id))
                    }
                    _ => ParamWire::None,
                }
            })
            .collect()
    }

    fn frame(
        &self,
        params: &EffectParams<'_>,
        pixel_scale: f64,
        request: BaeRect,
        input: BaeRect,
    ) -> FrameRequest {
        let s = pixel_scale.max(1e-9);
        let layer = params.layer_bounds;
        let layer = if layer.is_empty() {
            input
        } else {
            BaeRect::new(
                (f64::from(layer.x0) / s).floor() as i32,
                (f64::from(layer.y0) / s).floor() as i32,
                (f64::from(layer.x1) / s).ceil() as i32,
                (f64::from(layer.y1) / s).ceil() as i32,
            )
        };
        // Downsample as a rational: 1/2 at half resolution, 4/1 when supersampled.
        let downsample = if s >= 1.0 {
            [1, s.round().max(1.0) as i32]
        } else {
            [(1.0 / s).round().max(1.0) as i32, 1]
        };
        let rate = params.frame_rate;
        let mut flags = match params.purpose {
            bae_render::RenderPurpose::Preview => BAE_RENDER_FLAG_PREVIEW,
            _ => BAE_RENDER_FLAG_FINAL,
        };
        if params.linear {
            flags |= BAE_RENDER_FLAG_LINEAR_BLENDING;
        }
        FrameRequest {
            effect: self.index,
            format: self.format,
            time: params.time.0,
            time_step: params.time_step.0,
            frame_rate: [rate.num as i32, rate.den as i32],
            downsample,
            quality: BAE_QUALITY_HIGH,
            flags,
            color_space: if params.linear {
                BAE_COLOR_SPACE_LINEAR_SRGB
            } else {
                BAE_COLOR_SPACE_SRGB
            },
            shutter: [params.shutter_angle as f32, params.shutter_phase as f32],
            layer_bounds: rect_of(layer),
            render_id: params.seed ^ params.time.0 as u64,
            layer_duration: 0,
            params: self.wire_params(params, pixel_scale),
            request: rect_of(request),
            input: rect_of(input),
        }
    }

    /// The GPU → CPU → plugin → GPU node.
    fn render_cpu(
        &self,
        ctx: &mut EncodeContext<'_>,
        params: &EffectParams<'_>,
        input: &GpuImage,
        output_roi: IRect,
    ) -> Result<GpuImage, CallError> {
        let failed = |m: String| CallError::Failed(m);
        let s = input.pixel_scale;
        let grid = if params.layer_bounds.is_empty() {
            s
        } else {
            1.0 / params.scale
        };
        let [tw, th] = input.texel_size();
        if tw == 0 || th == 0 {
            return Ok(input.clone());
        }
        let x0 = (f64::from(input.rect.x0) / s).round() as i32;
        let y0 = (f64::from(input.rect.y0) / s).round() as i32;
        let input_rect = BaeRect::new(x0, y0, x0 + tw as i32, y0 + th as i32);
        // output_roi is in render pixels of `grid`; the CPU images use `s`.
        let to_s = |v: i32, up: bool| {
            let x = f64::from(v) * grid / s;
            if up {
                x.ceil() as i32
            } else {
                x.floor() as i32
            }
        };
        let request = BaeRect::new(
            to_s(output_roi.x0, false),
            to_s(output_roi.y0, false),
            to_s(output_roi.x1, true),
            to_s(output_roi.y1, true),
        );
        if request.is_empty() {
            return Ok(input.clone());
        }
        let working = input.texture.format();
        if !pixels::working_supported(working) {
            return Err(failed(format!(
                "working format {working:?} is not supported"
            )));
        }

        // Read the input back: submit what the frame recorded so far.
        let gpu = ctx.gpu.clone();
        let region = TextureRegion {
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            size: wgpu::Extent3d {
                width: tw,
                height: th,
                depth_or_array_layers: 1,
            },
        };
        let readback = TextureReadback::record(&gpu, ctx.encoder, &input.texture, region)
            .map_err(|e| failed(format!("input readback: {e}")))?;
        let fresh = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("bae.native-effect"),
            });
        let recorded = std::mem::replace(&mut *ctx.encoder, fresh);
        gpu.queue().submit([recorded.finish()]);
        let data = readback
            .map()
            .wait(&gpu)
            .map_err(|e| failed(format!("input readback: {e}")))?;

        let frame = self.frame(params, s, request, input_rect);
        // Reuse the effect's buffers unless another thread renders it right now.
        let mut shared = self.scratch.try_lock();
        let mut own = (AlignedBuf::default(), AlignedBuf::default());
        let (inbuf, outbuf) = match shared.as_deref_mut() {
            Some((i, o)) => (i, o),
            None => (&mut own.0, &mut own.1),
        };
        inbuf.resize(pixels::image_len(self.format, input_rect));
        outbuf.resize(pixels::image_len(self.format, request));
        pixels::from_working(
            &data.data,
            data.bytes_per_row as usize,
            working,
            input.alpha,
            [tw, th],
            self.format,
            inbuf.as_mut_slice(),
            pixels::row_bytes(self.format, tw),
        );
        let out_rect = match self
            .plugin
            .render(&frame, inbuf.as_slice(), outbuf.as_mut_slice())?
        {
            RenderOut::Identity => return Ok(input.clone()),
            RenderOut::Rendered(r) => rect(r).intersect(&request),
        };
        let (w, h) = (out_rect.width().max(1), out_rect.height().max(1));
        let stride = pixels::row_bytes(self.format, request.width());
        let offset = if out_rect.is_empty() {
            0
        } else {
            (out_rect.y0 - request.y0) as usize * stride
                + (out_rect.x0 - request.x0) as usize * pixels::bytes_per_pixel(self.format)
        };
        let bytes = if out_rect.is_empty() {
            // Nothing produced: one transparent texel.
            vec![0u8; working.block_copy_size(None).unwrap_or(8) as usize]
        } else {
            pixels::to_working(
                &outbuf.as_slice()[offset..],
                stride,
                self.format,
                [w, h],
                working,
            )
        };
        let px = |v: i32| (f64::from(v) * s).round() as i32;
        let origin = if out_rect.is_empty() {
            BaeRect::new(request.x0, request.y0, request.x0 + 1, request.y0 + 1)
        } else {
            out_rect
        };
        let mut image = ctx
            .allocate(
                IRect::new(px(origin.x0), px(origin.y0), px(origin.x1), px(origin.y1)),
                s,
            )
            .map_err(|e| failed(e.to_string()))?;
        let region = TextureRegion {
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        };
        transfer::upload_texture(&gpu, &image.texture, region, &bytes)
            .map_err(|e| failed(format!("output upload: {e}")))?;
        image.alpha = ImageAlpha::Premultiplied;
        Ok(image)
    }

    fn query(
        &self,
        params: &EffectParams<'_>,
        request: IRect,
        input: IRect,
    ) -> Option<PreRenderOut> {
        if !self.info.has_pre_render || self.plugin.is_disabled() {
            return None;
        }
        let to = |r: IRect| BaeRect::new(r.x0, r.y0, r.x1, r.y1);
        let frame = self.frame(params, 1.0 / params.scale.max(1e-9), to(request), to(input));
        match self.plugin.pre_render(&frame) {
            Ok(out) => Some(out),
            Err(e) => {
                self.plugin.fail(&self.info.id, e);
                None
            }
        }
    }
}

const UNBOUNDED: IRect = IRect {
    x0: BAE_COORD_MIN,
    y0: BAE_COORD_MIN,
    x1: BAE_COORD_MAX,
    y1: BAE_COORD_MAX,
};

impl Effect for NativeEffect {
    fn def(&self) -> &EffectDef {
        &self.def
    }

    fn output_bounds(&self, params: &EffectParams<'_>, input: IRect) -> IRect {
        match self.query(params, UNBOUNDED, input) {
            Some(out) if out.flags & BAE_PRERENDER_IDENTITY == 0 => {
                let r = out.max_result;
                IRect::new(r[0], r[1], r[2], r[3])
            }
            _ => input,
        }
    }

    fn input_roi(&self, params: &EffectParams<'_>, output_roi: IRect) -> IRect {
        match self.query(params, output_roi, UNBOUNDED) {
            Some(PreRenderOut {
                input: Some(r),
                flags,
                ..
            }) if flags & BAE_PRERENDER_IDENTITY == 0 => IRect::new(r[0], r[1], r[2], r[3]),
            _ => output_roi,
        }
    }

    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        params: &EffectParams<'_>,
        input: &GpuImage,
        output_roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        if self.plugin.is_disabled() {
            return Ok(input.clone());
        }
        match self.render_cpu(ctx, params, input, output_roi) {
            Ok(image) => Ok(image),
            Err(e) => {
                // Pass the input through so the frame still renders (spec §10.10).
                self.plugin.fail(&self.info.id, e);
                Ok(input.clone())
            }
        }
    }
}

/// Builds the render-graph effects of a loaded native plugin, one per native
/// manifest effect, with definitions from the manifest (listing copy checked)
/// or from `define_params`.
pub fn effects_of(
    manifest: &PluginManifest,
    plugin: &Arc<NativePlugin>,
    package_defs: &[(EffectDecl, EffectDef)],
) -> Result<Vec<Arc<NativeEffect>>, Vec<Diagnostic>> {
    let mut out = Vec::new();
    for (decl, def) in package_defs {
        if decl.implementation != EffectImplementation::Native {
            continue;
        }
        let ctx = format!("effect \"{}\"", decl.id);
        let err = |m: String| vec![Diagnostic::error(MANIFEST_FILE, m).in_context(ctx.clone())];
        let info = plugin
            .info
            .effects
            .iter()
            .find(|e| e.id == decl.id)
            .ok_or_else(|| err("not exported by the library".into()))?;
        check_listing(decl, info).map_err(err)?;
        let mut def = if decl.params.is_empty() {
            let mut full = decl.clone();
            full.params = param_decls(&info.params);
            crate::effect_def::effect_def(&manifest.plugin, &full)
        } else {
            def.clone()
        };
        // global_setup is authoritative for threading (the manifest copy is informational).
        def.thread_safe = info.threading == BAE_THREADING_CONCURRENT;
        def.gpu = false;
        let effect = NativeEffect::new(plugin, &decl.id, def)
            .ok_or_else(|| err("no supported CPU pixel format".into()))?;
        out.push(Arc::new(effect));
    }
    Ok(out)
}

/// The API version a host presents to a plugin requiring `api`.
pub fn presented_api(api: ApiVersion) -> Option<ApiVersion> {
    HOST_API.check(api).ok()
}
