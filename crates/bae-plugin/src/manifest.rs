//! `bae-plugin.toml` (spec §5): types, parsing and validation.
//!
//! Parsing ([`PluginManifest::parse`]) checks TOML syntax, value types and
//! unknown keys (with line and column). [`PluginManifest::validate`] then checks
//! the rules that serde cannot express: id syntax, uniqueness, ranges, form
//! specific keys, pass/texture consistency and expression syntax. Shader
//! compilation is a separate step ([`crate::wgsl`]).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use bae_plugin_sdk::ApiVersion;
use serde::{Deserialize, Deserializer};

use crate::diag::Diagnostic;
use crate::expr::Expr;
use crate::id;

/// File name of the manifest at the package root.
pub const MANIFEST_FILE: &str = "bae-plugin.toml";

/// The manifest format version this host reads.
pub const SCHEMA_VERSION: u32 = 1;

/// A parsed `bae-plugin.toml`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    /// Manifest format version (must be [`SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// `[plugin]`.
    pub plugin: PluginInfo,
    /// `[native]`: native libraries (required iff a contribution is native).
    #[serde(default)]
    pub native: Option<NativeSection>,
    /// `[script]`: the JS entry module (required iff a contribution is a script).
    #[serde(default)]
    pub script: Option<ScriptSection>,
    /// `[permissions]`.
    #[serde(default)]
    pub permissions: Permissions,
    /// `[dependencies]`: plugin id → requirement.
    #[serde(default)]
    pub dependencies: BTreeMap<String, DependencySpec>,
    /// `[exports]`.
    #[serde(default)]
    pub exports: Exports,
    /// `[[effect]]`.
    #[serde(default, rename = "effect")]
    pub effects: Vec<EffectDecl>,
    /// `[[command]]`.
    #[serde(default, rename = "command")]
    pub commands: Vec<CommandDecl>,
    /// `[[automation]]` (automation scripts, AE File > Scripts).
    #[serde(default, rename = "automation")]
    pub automations: Vec<AutomationDecl>,
    /// `[[shape_operator]]` (parsed; loaded in a later API version).
    #[serde(default, rename = "shape_operator")]
    pub shape_operators: Vec<OtherDecl>,
    /// `[[text_selector]]` (reserved).
    #[serde(default, rename = "text_selector")]
    pub text_selectors: Vec<OtherDecl>,
    /// `[[importer]]`.
    #[serde(default, rename = "importer")]
    pub importers: Vec<OtherDecl>,
    /// `[[exporter]]`.
    #[serde(default, rename = "exporter")]
    pub exporters: Vec<OtherDecl>,
    /// `[[codec]]`.
    #[serde(default, rename = "codec")]
    pub codecs: Vec<OtherDecl>,
    /// `[[panel]]`.
    #[serde(default, rename = "panel")]
    pub panels: Vec<OtherDecl>,
    /// `[[workspace]]`.
    #[serde(default, rename = "workspace")]
    pub workspaces: Vec<OtherDecl>,
    /// `[[keymap]]`.
    #[serde(default, rename = "keymap")]
    pub keymaps: Vec<OtherDecl>,
    /// `[[expression_library]]`.
    #[serde(default, rename = "expression_library")]
    pub expression_libraries: Vec<OtherDecl>,
    /// `[x]`: vendor extensions, ignored by the host.
    #[serde(default)]
    pub x: Option<toml::Table>,
}

/// `[plugin]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginInfo {
    /// Reverse-DNS plugin id (spec §5.2).
    pub id: String,
    /// Plugin version (semver 2.0).
    #[serde(deserialize_with = "de_version")]
    pub version: semver::Version,
    /// Minimum BAE plugin API (`"MAJOR.MINOR"`), spec §6.6.
    #[serde(deserialize_with = "de_api")]
    pub api: ApiVersion,
    /// Default (English) display name.
    pub name: String,
    /// One-line description.
    #[serde(default)]
    pub description: String,
    /// Authors.
    #[serde(default)]
    pub authors: Vec<String>,
    /// SPDX expression (informational).
    #[serde(default)]
    pub license: Option<String>,
    /// Home page.
    #[serde(default)]
    pub homepage: Option<String>,
    /// Requirement on the BAE application version.
    #[serde(default, deserialize_with = "de_opt_req")]
    pub host: Option<semver::VersionReq>,
    /// Platform restriction (empty = everything the forms allow).
    #[serde(default)]
    pub platforms: Vec<String>,
    /// Plugin ids this plugin supersedes.
    #[serde(default)]
    pub replaces: Vec<String>,
    /// Icon (SVG, package-relative).
    #[serde(default)]
    pub icon: Option<String>,
}

/// `[native]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSection {
    /// Entry symbol; only `bae_plugin_entry` is allowed in API 0.x/1.x.
    #[serde(default)]
    pub entry: Option<String>,
    /// Isolation mode.
    #[serde(default)]
    pub isolation: Isolation,
    /// Platform key → package-relative library path.
    #[serde(default)]
    pub libraries: BTreeMap<String, String>,
}

/// Where a native plugin runs (spec §15).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Isolation {
    /// In the host process (default, fastest).
    #[default]
    InProcess,
    /// In `bae-plugin-runner`, frames through shared memory.
    OutOfProcess,
    /// Out of process unless the user switched it to in-process.
    PreferOutOfProcess,
}

/// `[script]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptSection {
    /// ES module entry (package-relative).
    pub entry: String,
    /// Heap limit in MiB (default 64, max 1024).
    #[serde(default)]
    pub memory_limit_mb: Option<u32>,
}

/// `[permissions]` (spec §5.3.4, §21).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    /// Project access; `None` = the default of the form (scripts: read).
    #[serde(default)]
    pub project: Option<ProjectAccess>,
    /// Filesystem scopes such as `read:project`.
    #[serde(default)]
    pub filesystem: Vec<String>,
    /// Network origins.
    #[serde(default)]
    pub network: Vec<String>,
    /// Clipboard access: `none`, `read`, `write`, `read-write`.
    #[serde(default)]
    pub clipboard: Option<String>,
    /// May spawn child processes.
    #[serde(default)]
    pub process: bool,
    /// May use `bae.gpu.native`.
    #[serde(default)]
    pub gpu_native: bool,
    /// May request rendered frames.
    #[serde(default)]
    pub render: bool,
}

pub use bae_script::ProjectAccess;

/// One `[dependencies]` entry.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum DependencySpec {
    /// `"^1.2"`.
    Req(#[serde(deserialize_with = "de_req")] semver::VersionReq),
    /// `{ version = "^1.2", optional = true }`.
    Detailed {
        /// Requirement.
        #[serde(deserialize_with = "de_req")]
        version: semver::VersionReq,
        /// The plugin works without it.
        #[serde(default)]
        optional: bool,
    },
}

impl DependencySpec {
    /// The version requirement.
    pub fn requirement(&self) -> &semver::VersionReq {
        match self {
            DependencySpec::Req(r) | DependencySpec::Detailed { version: r, .. } => r,
        }
    }

    /// `true` for optional dependencies.
    pub fn is_optional(&self) -> bool {
        matches!(self, DependencySpec::Detailed { optional: true, .. })
    }
}

/// `[exports]`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exports {
    /// WGSL files dependents may include.
    #[serde(default)]
    pub wgsl: Vec<String>,
    /// JS modules dependents may import.
    #[serde(default)]
    pub js: Vec<String>,
}

/// Effect class (spec §3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EffectClass {
    /// Processes its layer (most effects).
    #[default]
    Filter,
    /// Creates pixels; the input is optional.
    Generator,
    /// Blends from the layer (`input`) to `to` by a progress parameter.
    Transition,
}

/// How an effect is implemented.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EffectImplementation {
    /// Declarative WGSL package (spec §11).
    Wgsl,
    /// Native code behind the C ABI.
    Native,
}

/// Effect capability flags (manifest spelling of `BAE_EFFECT_FLAG_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EffectFlag {
    /// Output pixel depends only on the same input pixel.
    PixelIndependent,
    /// Result may exceed the input extent.
    ExpandsBuffer,
    /// Result may be smaller than the input.
    ShrinksBuffer,
    /// Output changes over time at constant parameters.
    NonParamVary,
    /// Reads inputs or parameters at other times.
    WideTimeInput,
    /// Reads the shutter angle / phase.
    UsesShutter,
    /// Depends on the comp camera.
    UsesCamera,
    /// Depends on the comp lights.
    UsesLights,
    /// Never cache.
    NonDeterministic,
    /// (Native CPU) output may alias input.
    InPlace,
}

impl EffectFlag {
    /// The `BAE_EFFECT_FLAG_*` bit of `bae_plugin.h`.
    pub const fn bit(self) -> u64 {
        1 << match self {
            EffectFlag::PixelIndependent => 0,
            EffectFlag::ExpandsBuffer => 1,
            EffectFlag::ShrinksBuffer => 2,
            EffectFlag::NonParamVary => 3,
            EffectFlag::WideTimeInput => 4,
            EffectFlag::UsesShutter => 5,
            EffectFlag::UsesCamera => 6,
            EffectFlag::UsesLights => 7,
            EffectFlag::NonDeterministic => 8,
            EffectFlag::InPlace => 9,
        }
    }
}

/// Native threading declaration (informational in the manifest).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Threading {
    /// The host serializes every render call into the effect.
    SerialGlobal,
    /// Concurrent across render instances only.
    SerialInstance,
    /// No restriction (AE multi-frame rendering safe).
    Concurrent,
}

/// Native render device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Device {
    /// `render_cpu`.
    Cpu,
    /// `render_gpu`.
    Gpu,
}

/// Declarative shader style (spec §11.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShaderStyle {
    /// One pixel function; the host generates the kernel and may fuse chains.
    Pixel,
    /// Author-written `@compute` passes.
    Compute,
}

/// `[[effect]]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectDecl {
    /// Contribution id.
    pub id: String,
    /// Default display name.
    pub name: String,
    /// Effects & Presets category (well-known English key).
    #[serde(default)]
    pub category: String,
    /// Description (tooltip, MCP).
    #[serde(default)]
    pub description: String,
    /// Class.
    #[serde(default)]
    pub class: EffectClass,
    /// Implementation form.
    pub implementation: EffectImplementation,
    /// Search keywords.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// AE matchNames this effect can stand in for on AE import.
    #[serde(default)]
    pub ae_match_names: Vec<String>,
    /// Capability flags.
    #[serde(default)]
    pub flags: Vec<EffectFlag>,
    /// Native: threading (informational listing value).
    #[serde(default)]
    pub threading: Option<Threading>,
    /// Native: devices.
    #[serde(default)]
    pub devices: Vec<Device>,
    /// Declarative: the WGSL file.
    #[serde(default)]
    pub shader: Option<String>,
    /// Declarative: WGSL files concatenated before `shader`.
    #[serde(default)]
    pub includes: Vec<String>,
    /// Declarative: WGSL features (`f16`).
    #[serde(default)]
    pub features: Vec<String>,
    /// Declarative: style; default compute iff `[[effect.pass]]` exists.
    #[serde(default)]
    pub style: Option<ShaderStyle>,
    /// Declarative pixel style: the pixel function (default: id with `-` → `_`).
    #[serde(default)]
    pub entry: Option<String>,
    /// Declarative: skip the effect when this expression is true.
    #[serde(default)]
    pub identity_when: Option<String>,
    /// Declarative: ROI rules.
    #[serde(default)]
    pub roi: Option<RoiDecl>,
    /// Parameters.
    #[serde(default, rename = "param")]
    pub params: Vec<ParamDecl>,
    /// Compute style: intermediate textures.
    #[serde(default, rename = "texture")]
    pub textures: Vec<TextureDecl>,
    /// Compute style: passes, in execution order.
    #[serde(default, rename = "pass")]
    pub passes: Vec<PassDecl>,
    /// Data migrations from older plugin versions.
    #[serde(default, rename = "migration")]
    pub migrations: Vec<MigrationDecl>,
}

impl EffectDecl {
    /// The effective shader style.
    pub fn shader_style(&self) -> ShaderStyle {
        self.style.unwrap_or(if self.passes.is_empty() {
            ShaderStyle::Pixel
        } else {
            ShaderStyle::Compute
        })
    }

    /// Pixel style: the pixel function name.
    pub fn pixel_entry(&self) -> String {
        self.entry
            .clone()
            .unwrap_or_else(|| self.id.replace('-', "_"))
    }

    /// `true` if the flag is declared.
    pub fn has_flag(&self, flag: EffectFlag) -> bool {
        self.flags.contains(&flag)
    }

    /// `BAE_EFFECT_FLAG_*` bits, including flags implied by the declaration
    /// (pixel style → pixel-independent, `grows` → expands-buffer).
    pub fn flag_bits(&self) -> u64 {
        let mut bits = self.flags.iter().fold(0, |b, f| b | f.bit());
        if self.implementation == EffectImplementation::Wgsl
            && self.shader_style() == ShaderStyle::Pixel
        {
            bits |= EffectFlag::PixelIndependent.bit();
        }
        if self.roi.as_ref().is_some_and(|r| r.grows) {
            bits |= EffectFlag::ExpandsBuffer.bit();
        }
        bits
    }

    /// A parameter by key.
    pub fn param(&self, key: &str) -> Option<&ParamDecl> {
        self.params.iter().find(|p| p.key == key)
    }
}

/// `[effect.roi]` (spec §11.6).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoiDecl {
    /// Read the complete upstream image (e.g. a spherical reprojection).
    #[serde(default)]
    pub whole_input: bool,
    /// Output width and height expressions in full-resolution pixels.
    #[serde(default)]
    pub size: Option<[String; 2]>,
    /// Extent of the result.
    #[serde(default)]
    pub extent: Option<RoiExtent>,
    /// Read margin on all sides (expression, full-resolution layer pixels).
    #[serde(default)]
    pub read: Option<String>,
    /// Left margin (overrides `read`).
    #[serde(default)]
    pub read_left: Option<String>,
    /// Right margin.
    #[serde(default)]
    pub read_right: Option<String>,
    /// Top margin.
    #[serde(default)]
    pub read_top: Option<String>,
    /// Bottom margin.
    #[serde(default)]
    pub read_bottom: Option<String>,
    /// The result grows by the mirrored read margins.
    #[serde(default)]
    pub grows: bool,
}

impl RoiDecl {
    /// Margin expressions `[left, top, right, bottom]` (falling back to `read`).
    pub fn margin_sources(&self) -> [Option<&str>; 4] {
        fn side<'a>(s: &'a Option<String>, read: Option<&'a str>) -> Option<&'a str> {
            s.as_deref().or(read)
        }
        let read = self.read.as_deref();
        [
            side(&self.read_left, read),
            side(&self.read_top, read),
            side(&self.read_right, read),
            side(&self.read_bottom, read),
        ]
    }
}

/// Result extent of a declarative effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RoiExtent {
    /// The input's result rect (default for filters and transitions).
    Input,
    /// The layer bounds (default for generators).
    Layer,
    /// Whatever is requested.
    Infinite,
}

/// Parameter types (spec §5.3.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ParamType {
    /// Float slider → `f32`.
    Float,
    /// Integer slider → `i32`.
    Int,
    /// Angle in degrees → `f32`.
    Angle,
    /// Checkbox → `u32` 0/1.
    Checkbox,
    /// Straight RGBA color → `vec4<f32>`.
    Color,
    /// 2D point (layer pixels) → `vec2<f32>` render pixels.
    Point,
    /// 3D point → `vec3<f32>`.
    #[serde(rename = "point3d")]
    Point3d,
    /// Popup; the value is the 1-based choice index (AE) → `u32`.
    Popup,
    /// A layer of the comp → texture `bae_tex_<key>`.
    Layer,
    /// Starts a twirly group; members name it in `group`.
    Group,
    /// A button.
    Button,
    /// Text (reserved: not in the API 0.1 document model).
    String,
    /// An asset reference (reserved).
    Asset,
    /// Opaque bytes for native effects or UI-only metadata in WGSL effects.
    Arbitrary,
    /// A mask reference (native only).
    Path,
}

impl ParamType {
    /// Manifest spelling.
    pub fn name(self) -> &'static str {
        match self {
            ParamType::Float => "float",
            ParamType::Int => "int",
            ParamType::Angle => "angle",
            ParamType::Checkbox => "checkbox",
            ParamType::Color => "color",
            ParamType::Point => "point",
            ParamType::Point3d => "point3d",
            ParamType::Popup => "popup",
            ParamType::Layer => "layer",
            ParamType::Group => "group",
            ParamType::Button => "button",
            ParamType::String => "string",
            ParamType::Asset => "asset",
            ParamType::Arbitrary => "arbitrary",
            ParamType::Path => "path",
        }
    }

    /// The WGSL type of the `BaeParams` field, `None` if the param has no field.
    pub fn wgsl_type(self) -> Option<&'static str> {
        Some(match self {
            ParamType::Float | ParamType::Angle => "f32",
            ParamType::Int => "i32",
            ParamType::Checkbox | ParamType::Popup => "u32",
            ParamType::Color => "vec4<f32>",
            ParamType::Point => "vec2<f32>",
            ParamType::Point3d => "vec3<f32>",
            _ => return None,
        })
    }

    /// `true` if expressions may read the value (`params.<key>`).
    pub fn is_scalar(self) -> bool {
        matches!(
            self,
            ParamType::Float
                | ParamType::Int
                | ParamType::Angle
                | ParamType::Checkbox
                | ParamType::Popup
        )
    }
}

impl fmt::Display for ParamType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Parameter flags (spec §5.3.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ParamFlag {
    /// Not shown.
    Hidden,
    /// Shown greyed out.
    Disabled,
    /// The plugin is told when the value changes (native).
    Supervise,
    /// No stopwatch.
    NotAnimatable,
    /// The value never varies over time (AE `CANNOT_TIME_VARY`).
    CannotTimeVary,
    /// Group starts collapsed.
    StartCollapsed,
    /// Does not affect pixels (excluded from cache keys).
    UiOnly,
    /// No expressions on this parameter.
    NoExpressions,
    /// The transition progress parameter (0..1).
    TransitionProgress,
}

/// Display unit of a numeric parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NumberDisplay {
    /// Plain number.
    Number,
    /// Percent.
    Percent,
    /// Pixels.
    Pixels,
    /// Degrees.
    Degrees,
    /// Seconds.
    Seconds,
    /// Frames.
    Frames,
}

impl NumberDisplay {
    /// The `bae_model::ParamKind::Slider::display` token.
    pub fn model_token(self) -> &'static str {
        match self {
            NumberDisplay::Number => "none",
            NumberDisplay::Percent => "percent",
            NumberDisplay::Pixels => "pixels",
            NumberDisplay::Degrees => "degrees",
            NumberDisplay::Seconds => "seconds",
            NumberDisplay::Frames => "frames",
        }
    }
}

/// `[[effect.param]]`. Keys that do not apply to `type` are rejected by
/// [`PluginManifest::validate`].
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamDecl {
    /// Stable numeric id (AE parameter index / disk id), never reused.
    pub id: u32,
    /// Stable key: WGSL field, expression and script name.
    pub key: String,
    /// Type.
    #[serde(rename = "type")]
    pub ty: ParamType,
    /// Default display name.
    pub name: String,
    /// Key of the `group` param this param belongs to.
    #[serde(default)]
    pub group: Option<String>,
    /// Flags.
    #[serde(default)]
    pub flags: Vec<ParamFlag>,
    /// Shorthand for the `hidden` flag.
    #[serde(default)]
    pub hidden: bool,
    /// Shorthand for the `disabled` flag.
    #[serde(default)]
    pub disabled: bool,
    /// Default value (type-specific).
    #[serde(default)]
    pub default: Option<toml::Value>,
    /// Valid range `[min, max]` (float, int).
    #[serde(default)]
    pub range: Option<[f64; 2]>,
    /// Visible slider range (float, int).
    #[serde(default)]
    pub slider: Option<[f64; 2]>,
    /// Decimal places shown (float).
    #[serde(default)]
    pub precision: Option<u8>,
    /// Display unit (float, int).
    #[serde(default)]
    pub display: Option<NumberDisplay>,
    /// Checkbox / button label.
    #[serde(default)]
    pub label: Option<String>,
    /// Color: the alpha channel is editable.
    #[serde(default)]
    pub alpha: Option<bool>,
    /// Point: `default` is a fraction of the layer size (default true).
    #[serde(default)]
    pub relative: Option<bool>,
    /// Popup: choice labels; the value is the 1-based index. Only append.
    #[serde(default)]
    pub choices: Option<Vec<String>>,
    /// Group: start collapsed.
    #[serde(default)]
    pub collapsed: Option<bool>,
    /// Group: the id of the group's end marker (AE counts it as a parameter).
    #[serde(default)]
    pub end_id: Option<u32>,
    /// String: maximum length.
    #[serde(default)]
    pub max_length: Option<u32>,
    /// String: multi-line editor.
    #[serde(default)]
    pub multiline: Option<bool>,
    /// Asset: accepted extensions.
    #[serde(default)]
    pub extensions: Option<Vec<String>>,
}

impl ParamDecl {
    /// `true` if the flag is set (including the `hidden`/`disabled` shorthands).
    pub fn has_flag(&self, flag: ParamFlag) -> bool {
        self.flags.contains(&flag)
            || (flag == ParamFlag::Hidden && self.hidden)
            || (flag == ParamFlag::Disabled && self.disabled)
    }

    /// Valid range of a numeric param.
    pub fn valid_range(&self) -> [f64; 2] {
        self.range.unwrap_or([-1_000_000.0, 1_000_000.0])
    }

    /// `true` for points whose default is a fraction of the layer size.
    pub fn is_relative_point(&self) -> bool {
        matches!(self.ty, ParamType::Point | ParamType::Point3d) && self.relative.unwrap_or(true)
    }

    /// Default as numbers (`[x]` for scalars), already validated.
    pub fn default_numbers(&self) -> Vec<f64> {
        let n = match self.ty {
            ParamType::Color => 4,
            ParamType::Point => 2,
            ParamType::Point3d => 3,
            _ => 1,
        };
        let mut out = match &self.default {
            None => Vec::new(),
            Some(v) => numbers(v).unwrap_or_default(),
        };
        match self.ty {
            ParamType::Float | ParamType::Int | ParamType::Angle => {
                if out.is_empty() {
                    let [lo, hi] = self.valid_range();
                    out.push(0f64.clamp(lo, hi));
                }
            }
            ParamType::Checkbox => {
                let b = matches!(self.default, Some(toml::Value::Boolean(true)));
                out = vec![f64::from(u8::from(b))];
            }
            ParamType::Popup => {
                if out.is_empty() {
                    out.push(1.0);
                }
            }
            ParamType::Color => {
                if out.is_empty() {
                    out = vec![1.0, 1.0, 1.0, 1.0];
                } else if out.len() == 3 {
                    out.push(1.0);
                }
            }
            ParamType::Point | ParamType::Point3d if out.is_empty() => {
                let fill = if self.is_relative_point() { 0.5 } else { 0.0 };
                out = vec![fill; n];
            }
            _ => {}
        }
        out
    }
}

/// Numbers of a TOML scalar or array (integers become floats).
pub(crate) fn numbers(v: &toml::Value) -> Option<Vec<f64>> {
    let one = |v: &toml::Value| match v {
        toml::Value::Integer(i) => Some(*i as f64),
        toml::Value::Float(f) => Some(*f),
        _ => None,
    };
    match v {
        toml::Value::Array(items) => items.iter().map(one).collect(),
        other => one(other).map(|x| vec![x]),
    }
}

/// `[[effect.texture]]` (spec §11.5).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextureDecl {
    /// Name; bound as `bae_tex_<name>`, written by `bae_store` in its pass.
    pub name: String,
    /// Format.
    #[serde(default)]
    pub format: TextureFormat,
    /// Which rect it covers.
    #[serde(default)]
    pub rect: TextureRect,
    /// Resolution relative to that rect (reserved: must be 1.0 in API 0.1).
    #[serde(default = "one")]
    pub scale: f64,
}

fn one() -> f64 {
    1.0
}

/// Intermediate texture format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextureFormat {
    /// The working format (RGBA16F or RGBA32F).
    #[default]
    Working,
    /// RGBA16F.
    Rgba16f,
    /// RGBA32F.
    Rgba32f,
    /// R32F.
    R32f,
    /// RG32F.
    Rg32f,
    /// RGBA8 unorm.
    Rgba8unorm,
}

impl TextureFormat {
    /// WGSL storage texel format, `None` for `working`.
    pub fn wgsl_storage_format(self) -> Option<&'static str> {
        Some(match self {
            TextureFormat::Working => return None,
            TextureFormat::Rgba16f => "rgba16float",
            TextureFormat::Rgba32f => "rgba32float",
            TextureFormat::R32f => "r32float",
            TextureFormat::Rg32f => "rg32float",
            TextureFormat::Rgba8unorm => "rgba8unorm",
        })
    }
}

/// The rect an intermediate texture covers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextureRect {
    /// The effect's output rect.
    #[default]
    Output,
    /// The (inflated) input rect.
    Input,
}

/// `[[effect.pass]]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PassDecl {
    /// `@compute` entry point.
    pub entry: String,
    /// Sources it reads: `input`, `to`, layer-param keys, texture names.
    #[serde(default)]
    pub reads: Vec<String>,
    /// `output` or a texture name.
    pub writes: String,
}

/// `[[effect.migration]]` (spec §6.5).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationDecl {
    /// Requirement on the plugin version that wrote the data.
    #[serde(deserialize_with = "de_req")]
    pub from: semver::VersionReq,
    /// Old key → new key.
    #[serde(default)]
    pub rename: BTreeMap<String, String>,
    /// Keys whose values are dropped.
    #[serde(default)]
    pub drop: Vec<String>,
    /// Values for params that did not exist.
    #[serde(default)]
    pub defaults: toml::Table,
}

/// `[[command]]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandDecl {
    /// Contribution id; registry id `plugin.<plugin-id>.<id>`.
    pub id: String,
    /// Default display name.
    pub name: String,
    /// `native` or `script`.
    pub implementation: String,
    /// Script: exported function name.
    #[serde(default)]
    pub entry: Option<String>,
    /// JSON Schema of the arguments (package-relative, required).
    pub schema: String,
    /// Menu path (well-known English keys).
    #[serde(default)]
    pub menu: Option<String>,
    /// Default shortcut per workspace.
    #[serde(default)]
    pub shortcut: BTreeMap<String, String>,
    /// Description (also the MCP tool description).
    #[serde(default)]
    pub description: String,
}

/// `[[automation]]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationDecl {
    /// Contribution id; registered as command `script.<plugin-id>.<id>`.
    pub id: String,
    /// Default display name.
    pub name: String,
    /// Exported function name.
    pub entry: String,
    /// Menu path.
    #[serde(default)]
    pub menu: Option<String>,
    /// Description.
    #[serde(default)]
    pub description: String,
}

/// A contribution of a kind this host version parses but does not load yet.
#[derive(Clone, Debug, Deserialize)]
pub struct OtherDecl {
    /// Contribution id.
    pub id: String,
    /// Default display name.
    pub name: String,
    /// Implementation form, if the kind has several.
    #[serde(default)]
    pub implementation: Option<String>,
    /// Every other key, kept verbatim.
    #[serde(flatten)]
    pub rest: toml::Table,
}

/// Error returned when a manifest is not valid TOML or does not match the schema.
#[derive(Clone, Debug, thiserror::Error)]
#[error("{0}")]
pub struct ManifestError(pub Diagnostic);

impl PluginManifest {
    /// Parses a manifest. Syntax errors, wrong value types and unknown keys are
    /// reported with line and column.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        toml::from_str::<PluginManifest>(text).map_err(|e| {
            let mut d = Diagnostic::error(MANIFEST_FILE, e.message().trim().to_owned());
            if let Some(span) = e.span() {
                let (line, column) = line_col(text, span.start);
                d = d.at(line, column);
            }
            ManifestError(d)
        })
    }

    /// Validates everything serde cannot (spec §5.4). File existence and shader
    /// compilation are checked by [`crate::package::Package::load`].
    pub fn validate(&self) -> Vec<Diagnostic> {
        let mut v = Validator::default();
        v.manifest(self);
        v.out
    }

    /// Full id of an effect (its document `match_name`).
    pub fn effect_full_id(&self, effect: &EffectDecl) -> String {
        id::full_id(&self.plugin.id, &effect.id)
    }

    /// Every package-relative file the manifest references.
    pub fn referenced_files(&self) -> Vec<&str> {
        let mut files: Vec<&str> = Vec::new();
        files.extend(self.plugin.icon.as_deref());
        if let Some(native) = &self.native {
            files.extend(native.libraries.values().map(String::as_str));
        }
        if let Some(script) = &self.script {
            files.push(&script.entry);
        }
        files.extend(self.exports.wgsl.iter().map(String::as_str));
        files.extend(self.exports.js.iter().map(String::as_str));
        for e in &self.effects {
            files.extend(e.shader.as_deref());
            files.extend(
                e.includes
                    .iter()
                    .map(String::as_str)
                    .filter(|p| !p.starts_with("plugin:")),
            );
        }
        files.extend(self.commands.iter().map(|c| c.schema.as_str()));
        files
    }
}

/// 1-based line and column of byte `offset` in `text`.
pub(crate) fn line_col(text: &str, offset: usize) -> (u32, u32) {
    let offset = offset.min(text.len());
    let prefix = &text[..offset];
    let line = prefix.matches('\n').count() + 1;
    let col = offset - prefix.rfind('\n').map_or(0, |p| p + 1) + 1;
    (line as u32, col as u32)
}

#[derive(Default)]
struct Validator {
    out: Vec<Diagnostic>,
    context: String,
}

impl Validator {
    fn err(&mut self, message: impl Into<String>) {
        self.out
            .push(Diagnostic::error(MANIFEST_FILE, message).in_context(self.context.clone()));
    }

    fn warn(&mut self, message: impl Into<String>) {
        self.out
            .push(Diagnostic::warning(MANIFEST_FILE, message).in_context(self.context.clone()));
    }

    fn check(&mut self, r: Result<(), String>) {
        if let Err(e) = r {
            self.err(e);
        }
    }

    fn path(&mut self, path: &str) {
        self.check(id::check_package_path(path));
    }

    fn manifest(&mut self, m: &PluginManifest) {
        if m.schema_version != SCHEMA_VERSION {
            self.err(format!(
                "schema_version {} is not supported (this host reads {SCHEMA_VERSION})",
                m.schema_version
            ));
        }
        self.plugin(&m.plugin);
        self.sections(m);

        let mut ids: BTreeSet<(&'static str, String)> = BTreeSet::new();
        let mut unique = |v: &mut Self, table: &'static str, cid: &str| {
            v.context = format!("{table} \"{cid}\"");
            v.check(id::check_contribution_id(cid));
            if !ids.insert((table, cid.to_owned())) {
                v.err(format!("duplicate {table} id `{cid}`"));
            }
        };
        for e in &m.effects {
            unique(self, "effect", &e.id);
            self.effect(e);
        }
        for c in &m.commands {
            unique(self, "command", &c.id);
            self.command(c);
        }
        for a in &m.automations {
            unique(self, "automation", &a.id);
            if a.entry.trim().is_empty() {
                self.err("automation scripts need an exported function name");
            }
            if m.script.is_none() {
                self.err("automation scripts need a [script] table with the entry module");
            }
        }
        let others: [(&'static str, &Vec<OtherDecl>); 9] = [
            ("shape_operator", &m.shape_operators),
            ("text_selector", &m.text_selectors),
            ("importer", &m.importers),
            ("exporter", &m.exporters),
            ("codec", &m.codecs),
            ("panel", &m.panels),
            ("workspace", &m.workspaces),
            ("keymap", &m.keymaps),
            ("expression_library", &m.expression_libraries),
        ];
        for (table, list) in others {
            for o in list {
                unique(self, table, &o.id);
                self.warn(format!(
                    "[[{table}]] contributions are parsed but not loaded by plugin API {}",
                    ApiVersion::CURRENT
                ));
            }
        }
        self.context.clear();
        let any = m.effects.len()
            + m.commands.len()
            + m.automations.len()
            + others.iter().map(|(_, l)| l.len()).sum::<usize>();
        if any == 0 {
            self.warn("the plugin declares no contributions");
        }
    }

    fn plugin(&mut self, p: &PluginInfo) {
        self.context = "[plugin]".into();
        self.check(id::check_plugin_id(&p.id));
        if p.name.trim().is_empty() {
            self.err("name must not be empty");
        }
        if let Err(e) = bae_plugin_sdk::HostApiSupport::SDK_DEFAULT.check(p.api) {
            self.err(format!("api = \"{}\": {e}", p.api));
        }
        for platform in &p.platforms {
            if !id::PLATFORM_KEYS.contains(&platform.as_str()) {
                self.err(format!("unknown platform key `{platform}`"));
            }
        }
        for r in &p.replaces {
            self.check(id::check_plugin_id(r));
        }
        if let Some(icon) = &p.icon {
            self.path(icon);
        }
    }

    fn sections(&mut self, m: &PluginManifest) {
        let native_used = m
            .effects
            .iter()
            .any(|e| e.implementation == EffectImplementation::Native)
            || m.commands.iter().any(|c| c.implementation == "native");
        let script_used =
            m.commands.iter().any(|c| c.implementation == "script") || !m.automations.is_empty();
        self.context = "[native]".into();
        match &m.native {
            Some(n) => {
                if !native_used {
                    self.warn("[native] is present but no contribution is native");
                }
                if let Some(entry) = &n.entry
                    && entry != "bae_plugin_entry"
                {
                    self.err("entry must be `bae_plugin_entry` in plugin API 0.x and 1.x");
                }
                if n.libraries.is_empty() && native_used {
                    self.err("[native.libraries] must name a library for at least one platform");
                }
                for (platform, path) in &n.libraries {
                    if !id::PLATFORM_KEYS.contains(&platform.as_str()) {
                        self.err(format!("unknown platform key `{platform}`"));
                    }
                    self.path(path);
                }
            }
            None if native_used => {
                self.err("native contributions need a [native] table with libraries");
            }
            None => {}
        }
        self.context = "[script]".into();
        match &m.script {
            Some(s) => {
                self.path(&s.entry);
                if s.memory_limit_mb.is_some_and(|mb| mb == 0 || mb > 1024) {
                    self.err("memory_limit_mb must be 1..=1024");
                }
                if !script_used {
                    self.warn("[script] is present but no contribution is a script");
                }
            }
            None if script_used => {
                self.err("script contributions need a [script] table with the entry module")
            }
            None => {}
        }
        self.context = "[dependencies]".into();
        for dep in m.dependencies.keys() {
            self.check(id::check_plugin_id(dep));
            if *dep == m.plugin.id {
                self.err("a plugin cannot depend on itself");
            }
        }
        self.context = "[exports]".into();
        for p in m.exports.wgsl.iter().chain(&m.exports.js) {
            self.path(p);
        }
    }

    fn command(&mut self, c: &CommandDecl) {
        match c.implementation.as_str() {
            "script" => {
                if c.entry.as_deref().is_none_or(|name| name.trim().is_empty()) {
                    self.err("script commands need `entry` (the exported function)");
                }
            }
            "native" => {}
            other => self.err(format!(
                "implementation `{other}` is not allowed for commands (native, script)"
            )),
        }
        self.path(&c.schema);
        if !c.schema.ends_with(".json") {
            self.warn("schema should be a .json file");
        }
    }

    fn effect(&mut self, e: &EffectDecl) {
        let base = self.context.clone();
        if e.name.trim().is_empty() {
            self.err("name must not be empty");
        }
        let wgsl = e.implementation == EffectImplementation::Wgsl;
        if wgsl {
            self.declarative(e);
        } else {
            let declarative_keys = [
                ("shader", e.shader.is_some()),
                ("includes", !e.includes.is_empty()),
                ("features", !e.features.is_empty()),
                ("style", e.style.is_some()),
                ("entry", e.entry.is_some()),
                ("identity_when", e.identity_when.is_some()),
                ("roi", e.roi.is_some()),
                ("texture", !e.textures.is_empty()),
                ("pass", !e.passes.is_empty()),
            ];
            for (key, present) in declarative_keys {
                if present {
                    self.err(format!(
                        "`{key}` is only valid for implementation = \"wgsl\""
                    ));
                }
            }
            if e.devices.is_empty() {
                self.warn("devices is empty; native effects should list \"cpu\" and/or \"gpu\"");
            }
        }
        if wgsl && (e.threading.is_some() || !e.devices.is_empty()) {
            self.err("threading/devices are only valid for native effects (WGSL effects are always concurrent GPU effects)");
        }
        if e.has_flag(EffectFlag::InPlace) && wgsl {
            self.err("`in-place` only applies to native CPU effects");
        }

        // Parameters.
        let mut ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        let mut groups: Vec<&str> = Vec::new();
        let mut open_groups = Vec::<&str>::new();
        let mut progress = 0;
        for (index, p) in e.params.iter().enumerate() {
            self.context = format!("{base}, param \"{}\"", p.key);
            self.check(id::check_param_key(&p.key));
            if p.id == 0 || p.id > i32::MAX as u32 {
                self.err("id must be 1..=2147483647");
            }
            if !ids.insert(p.id) {
                self.err(format!("duplicate param id {}", p.id));
            }
            if !keys.insert(p.key.as_str()) {
                self.err(format!("duplicate param key `{}`", p.key));
            }
            if p.name.trim().is_empty() {
                self.err("name must not be empty");
            }
            if p.has_flag(ParamFlag::TransitionProgress) {
                progress += 1;
                if p.ty != ParamType::Float {
                    self.err("the transition-progress param must be a float");
                }
            }
            // Group membership: members follow their group contiguously.
            if let Some(parent) = p.group.as_deref() {
                if let Some(index) = open_groups.iter().position(|&key| key == parent) {
                    open_groups.truncate(index + 1);
                } else if groups.contains(&parent) {
                    self.err(format!("params of group `{parent}` must directly follow the group param (contiguously)"));
                } else {
                    self.err(format!(
                        "group `{parent}` is not a group param defined before this one"
                    ));
                }
            } else {
                open_groups.clear();
            }
            if p.ty == ParamType::Group {
                match p.end_id {
                    None => self.err("group params need `end_id` (the stable id of the group end, AE counts it as a parameter)"),
                    Some(end) => {
                        if end == 0 || end > i32::MAX as u32 {
                            self.err("end_id must be 1..=2147483647");
                        }
                        if !ids.insert(end) {
                            self.err(format!("duplicate param id {end} (end_id)"));
                        }
                    }
                }
                groups.push(&p.key);
                open_groups.push(&p.key);
            }
            self.param(e, p, index);
        }
        self.context = base.clone();
        match e.class {
            EffectClass::Transition if progress != 1 => self.err(
                "transitions need exactly one float param with the `transition-progress` flag",
            ),
            EffectClass::Filter | EffectClass::Generator if progress != 0 => {
                self.err("`transition-progress` is only valid for transitions")
            }
            _ => {}
        }

        // Migrations.
        for mig in &e.migrations {
            for new in mig.rename.values() {
                if e.param(new).is_none() {
                    self.err(format!("migration renames to unknown param `{new}`"));
                }
            }
            for key in mig.defaults.keys() {
                if e.param(key).is_none() {
                    self.err(format!("migration default for unknown param `{key}`"));
                }
            }
        }
        self.context = base;
    }

    fn param(&mut self, e: &EffectDecl, p: &ParamDecl, _index: usize) {
        let t = p.ty;
        let wgsl = e.implementation == EffectImplementation::Wgsl;
        // Keys allowed per type.
        let numeric = matches!(t, ParamType::Float | ParamType::Int);
        let allowed: &[(&str, bool)] = &[
            ("range", p.range.is_some() && !numeric),
            ("slider", p.slider.is_some() && !numeric),
            ("precision", p.precision.is_some() && t != ParamType::Float),
            ("display", p.display.is_some() && !numeric),
            (
                "label",
                p.label.is_some() && !matches!(t, ParamType::Checkbox | ParamType::Button),
            ),
            ("alpha", p.alpha.is_some() && t != ParamType::Color),
            (
                "relative",
                p.relative.is_some() && !matches!(t, ParamType::Point | ParamType::Point3d),
            ),
            ("choices", p.choices.is_some() && t != ParamType::Popup),
            ("collapsed", p.collapsed.is_some() && t != ParamType::Group),
            ("end_id", p.end_id.is_some() && t != ParamType::Group),
            (
                "max_length",
                p.max_length.is_some() && t != ParamType::String,
            ),
            ("multiline", p.multiline.is_some() && t != ParamType::String),
            (
                "extensions",
                p.extensions.is_some() && t != ParamType::Asset,
            ),
        ];
        for (key, bad) in allowed {
            if *bad {
                self.err(format!("`{key}` is not valid for {t} params"));
            }
        }
        match t {
            ParamType::String | ParamType::Asset => self.err(format!(
                "{t} params are reserved; the plugin API 1.0 document model has no {t} parameter values yet"
            )),
            // A disabled path can preserve imported mask references as metadata.
            ParamType::Path if p.has_flag(ParamFlag::Disabled) => {}
            // UI-only arbitrary data has no shader uniform.
            ParamType::Arbitrary if p.has_flag(ParamFlag::UiOnly) => {}
            ParamType::Arbitrary | ParamType::Path if wgsl => {
                self.err(format!("{t} params are only available to native effects"))
            }
            ParamType::Group | ParamType::Button if p.default.is_some() => {
                self.err(format!("{t} params have no value (remove `default`)"))
            }
            _ => {}
        }
        if numeric {
            let [lo, hi] = p.valid_range();
            if !(lo.is_finite() && hi.is_finite() && lo <= hi) {
                self.err("range must be [min, max] with min <= max");
            }
            if let Some([slo, shi]) = p.slider
                && !(slo <= shi && slo >= lo && shi <= hi)
            {
                self.err("slider must lie within range");
            }
            if t == ParamType::Int && (lo.fract() != 0.0 || hi.fract() != 0.0) {
                self.err("int ranges must be integers");
            }
        }
        if let Some(d) = &p.default {
            self.default_value(p, d);
        }
        if t == ParamType::Popup {
            match &p.choices {
                None => self.err("popup params need `choices`"),
                Some(c) if c.is_empty() => self.err("choices must not be empty"),
                Some(c) => {
                    if c.iter().any(|s| s.trim().is_empty()) {
                        self.err("choice labels must not be empty");
                    }
                }
            }
        }
    }

    fn default_value(&mut self, p: &ParamDecl, d: &toml::Value) {
        let t = p.ty;
        let nums = numbers(d);
        let want = |n: usize| nums.as_ref().is_some_and(|v| v.len() == n);
        match t {
            ParamType::Float | ParamType::Int | ParamType::Angle => {
                let scalar = matches!(d, toml::Value::Integer(_) | toml::Value::Float(_));
                if !scalar {
                    self.err(format!("default of a {t} param must be a number"));
                    return;
                }
                let x = nums.unwrap()[0];
                if t == ParamType::Int && !matches!(d, toml::Value::Integer(_)) {
                    self.err("default of an int param must be an integer");
                }
                if t != ParamType::Angle {
                    let [lo, hi] = p.valid_range();
                    if !(lo..=hi).contains(&x) {
                        self.err(format!("default {x} is outside range [{lo}, {hi}]"));
                    }
                }
            }
            ParamType::Checkbox => {
                if !matches!(d, toml::Value::Boolean(_)) {
                    self.err("default of a checkbox must be true or false");
                }
            }
            ParamType::Color => {
                if !(want(3) || want(4)) {
                    self.err("default of a color must be [r, g, b] or [r, g, b, a]");
                }
            }
            ParamType::Point => {
                if !want(2) {
                    self.err("default of a point must be [x, y]");
                }
            }
            ParamType::Point3d => {
                if !want(3) {
                    self.err("default of a point3d must be [x, y, z]");
                }
            }
            ParamType::Popup => match d {
                toml::Value::Integer(i) => {
                    let n = p.choices.as_ref().map_or(0, Vec::len) as i64;
                    if *i < 1 || *i > n {
                        self.err(format!(
                            "default {i} must be a 1-based choice index (1..={n})"
                        ));
                    }
                }
                _ => self.err("default of a popup must be a 1-based choice index"),
            },
            ParamType::Layer => match d {
                toml::Value::String(s) if s == "none" || s == "self" => {}
                _ => self.err("default of a layer param must be \"none\" or \"self\""),
            },
            _ => {}
        }
    }

    fn declarative(&mut self, e: &EffectDecl) {
        let style = e.shader_style();
        match &e.shader {
            Some(s) => self.path(s),
            None => self.err("declarative effects need `shader`"),
        }
        for inc in &e.includes {
            if let Some(rest) = inc.strip_prefix("plugin:") {
                match rest.split_once('/') {
                    Some((pid, path)) => {
                        self.check(id::check_plugin_id(pid));
                        self.path(path);
                    }
                    None => self.err(format!(
                        "include `{inc}` must be `plugin:<plugin-id>/<path>`"
                    )),
                }
            } else {
                self.path(inc);
            }
        }
        for f in &e.features {
            if f != "f16" {
                self.err(format!("unknown WGSL feature `{f}` (allowed: f16)"));
            }
        }
        // Expressions.
        let scalar_keys: Vec<&str> = e
            .params
            .iter()
            .filter(|p| p.ty.is_scalar())
            .map(|p| p.key.as_str())
            .collect();
        let expr = |v: &mut Self, what: &str, src: &str| {
            if let Err(err) = Expr::parse(src, &scalar_keys) {
                v.err(format!("{what} `{src}`: {err}"));
            }
        };
        if let Some(src) = &e.identity_when {
            expr(self, "identity_when", src);
        }
        if let Some(roi) = &e.roi {
            for (name, src) in [
                ("read", &roi.read),
                ("read_left", &roi.read_left),
                ("read_right", &roi.read_right),
                ("read_top", &roi.read_top),
                ("read_bottom", &roi.read_bottom),
            ] {
                if let Some(src) = src {
                    expr(self, name, src);
                }
            }
            if let Some([w, h]) = &roi.size {
                expr(self, "output width", w);
                expr(self, "output height", h);
            }
            if style == ShaderStyle::Pixel
                && (roi.whole_input || roi.margin_sources().iter().any(Option::is_some))
            {
                self.err("pixel-style effects read only their own pixel; remove the read margins or use style = \"compute\"");
            }
        }
        if e.class == EffectClass::Transition && style == ShaderStyle::Pixel {
            self.err("transitions read `to`; use style = \"compute\"");
        }

        // Styles, passes and textures.
        match style {
            ShaderStyle::Pixel => {
                if !e.passes.is_empty() || !e.textures.is_empty() {
                    self.err("pixel style cannot have [[effect.pass]] or [[effect.texture]]");
                }
                let entry = e.pixel_entry();
                if !is_wgsl_ident(&entry) {
                    self.err(format!("entry `{entry}` is not a WGSL identifier"));
                }
                if e.params.iter().any(|p| p.ty == ParamType::Layer) {
                    self.err(
                        "layer params need style = \"compute\" (pixel functions read no textures)",
                    );
                }
            }
            ShaderStyle::Compute => {
                if e.entry.is_some() {
                    self.err("`entry` is only for pixel style; compute style names entry points in [[effect.pass]]");
                }
                if e.passes.is_empty() {
                    self.err("compute style needs at least one [[effect.pass]]");
                }
                let mut names = BTreeSet::new();
                for t in &e.textures {
                    self.check(id::check_texture_name(&t.name));
                    if !names.insert(t.name.as_str()) {
                        self.err(format!("duplicate texture `{}`", t.name));
                    }
                    if e.param(&t.name).is_some() {
                        self.err(format!("texture `{}` has the same name as a param", t.name));
                    }
                    if t.scale != 1.0 {
                        self.err(format!(
                            "texture `{}`: scale is reserved; plugin API 1.0 supports only scale = 1.0",
                            t.name
                        ));
                    }
                }
                let layer_keys: BTreeSet<&str> = e
                    .params
                    .iter()
                    .filter(|p| p.ty == ParamType::Layer)
                    .map(|p| p.key.as_str())
                    .collect();
                let mut written: BTreeSet<&str> = BTreeSet::new();
                for (i, pass) in e.passes.iter().enumerate() {
                    let n = i + 1;
                    if !is_wgsl_ident(&pass.entry) {
                        self.err(format!(
                            "pass {n}: entry `{}` is not a WGSL identifier",
                            pass.entry
                        ));
                    }
                    for r in &pass.reads {
                        let r = r.as_str();
                        let known = r == "input"
                            || (r == "to" && e.class == EffectClass::Transition)
                            || layer_keys.contains(r)
                            || names.contains(r);
                        if !known {
                            self.err(format!("pass {n} reads unknown source `{r}`"));
                        } else if names.contains(r) && !written.contains(r) {
                            self.err(format!(
                                "pass {n} reads texture `{r}` before a pass writes it"
                            ));
                        }
                        if r == pass.writes {
                            self.err(format!("pass {n} reads its own target `{r}`"));
                        }
                    }
                    let w = pass.writes.as_str();
                    if w != "output" && !names.contains(w) {
                        self.err(format!("pass {n} writes unknown target `{w}`"));
                    }
                    written.insert(w);
                }
                if let Some(last) = e.passes.last()
                    && last.writes != "output"
                {
                    self.err("the last pass must write `output`");
                }
                for t in &e.textures {
                    if !written.contains(t.name.as_str()) {
                        self.warn(format!("texture `{}` is never written", t.name));
                    }
                }
            }
        }
    }
}

/// `true` for a plain WGSL identifier (ASCII subset).
pub(crate) fn is_wgsl_ident(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && s != "_"
        && !s.starts_with("__")
}

fn de_version<'de, D: Deserializer<'de>>(d: D) -> Result<semver::Version, D::Error> {
    let s = String::deserialize(d)?;
    semver::Version::parse(&s)
        .map_err(|e| serde::de::Error::custom(format!("invalid version `{s}`: {e}")))
}

fn de_req<'de, D: Deserializer<'de>>(d: D) -> Result<semver::VersionReq, D::Error> {
    let s = String::deserialize(d)?;
    semver::VersionReq::parse(&s)
        .map_err(|e| serde::de::Error::custom(format!("invalid version requirement `{s}`: {e}")))
}

fn de_opt_req<'de, D: Deserializer<'de>>(d: D) -> Result<Option<semver::VersionReq>, D::Error> {
    de_req(d).map(Some)
}

fn de_api<'de, D: Deserializer<'de>>(d: D) -> Result<ApiVersion, D::Error> {
    let s = String::deserialize(d)?;
    s.parse().map_err(serde::de::Error::custom)
}
