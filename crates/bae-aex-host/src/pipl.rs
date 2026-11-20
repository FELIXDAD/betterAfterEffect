//! Static plug-in description (what the host learns *without running code*).
//!
//! AE effect binaries carry a PiPL ("plug-in property list") resource that
//! names the plug-in kind, display name, category, per-platform entry-point
//! symbol, match name, effect/spec versions and the global out-flags, which
//! must agree with what the plug-in reports at `PF_Cmd_GLOBAL_SETUP`
//! (SDK guide, "Introduction › PiPL Resources"). Newer plug-ins may instead
//! register from code through an exported registration function (see
//! "Introduction › Exporting Symbols in Effects"), which requires calling into
//! the binary — BAE only does that inside the sandbox process (see
//! [`crate::process`]).
//!
//! The P3 loader will parse PiPLs on each platform (Windows: a resource inside
//! the DLL; macOS: a resource inside the `.plugin` bundle) into
//! [`PluginDescriptor`]. Raw flag words are kept opaque here (see crate docs,
//! rule 3) and are interpreted by the P3 FFI shim into [`crate::caps`].

use crate::error::{HostError, Result};

/// Stable identity of an effect across versions, platforms and display-name
/// localisation (`AE_Effect_Match_Name`). This is the key BAE uses to resolve
/// an imported AE effect (see [`crate::resolve`]) and the identity persisted in
/// the BAE project for an AE-backed effect Prim.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MatchName(String);

impl MatchName {
    /// Upper bound accepted by the host for sanity checking. The exact SDK
    /// limit is a header constant and is deliberately not reproduced here.
    pub const MAX_BYTES: usize = 255;

    /// Validate and wrap. Match names are compared byte-for-byte; no case
    /// folding or Unicode normalisation is applied (AE treats them as opaque).
    pub fn new(s: impl Into<String>) -> Result<MatchName> {
        let s = s.into();
        if s.is_empty() || s.len() > Self::MAX_BYTES || s.contains('\0') {
            return Err(HostError::OutOfRange { what: "match name" });
        }
        Ok(MatchName(s))
    }

    /// The match name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What kind of AE plug-in a binary declares itself to be.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PluginKind {
    /// Effect plug-in (`Kind = AEEffect`). The only kind the first compat host
    /// targets.
    Effect,
    /// General plug-in (AEGP). Out of scope for the first compat host.
    Aegp,
    /// Import/export plug-in (AEIO, a specialised AEGP). Out of scope.
    Aeio,
    /// 3D renderer (Artisan, a specialised AEGP). Out of scope.
    Artisan,
    /// Anything else (e.g. Photoshop-style filters); kept verbatim.
    Other(String),
}

/// Development stage component of an effect version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VersionStage {
    /// In development.
    Develop,
    /// Alpha.
    Alpha,
    /// Beta.
    Beta,
    /// Release.
    Release,
}

/// Decoded effect version (`AE_Effect_Version` / `out_data->my_version`). AE
/// uses it to pick between duplicate installs of the same match name; BAE uses
/// it the same way and stores it next to the effect Prim so a project can warn
/// when it is opened with a different plug-in version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EffectVersion {
    /// Major version.
    pub major: u16,
    /// Minor version.
    pub minor: u16,
    /// Bug-fix version.
    pub bug: u16,
    /// Stage.
    pub stage: VersionStage,
    /// Build number.
    pub build: u32,
}

/// Effect API specification version the plug-in was built against
/// (`AE_Effect_Spec_Version`; also what it requests in `in_data->version`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpecVersion {
    /// Major.
    pub major: u16,
    /// Minor.
    pub minor: u16,
}

/// CPU architecture + OS of one code slice of a plug-in binary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CodeArch {
    /// Windows x64 `.aex`.
    WindowsX64,
    /// Windows ARM64 `.aex`.
    WindowsArm64,
    /// Windows ARM64EC (x64-compatible ARM code) `.aex`.
    WindowsArm64Ec,
    /// macOS Intel slice of a `.plugin` bundle.
    MacX64,
    /// macOS Apple Silicon slice of a `.plugin` bundle.
    MacArm64,
}

/// One exported entry point declared for one architecture.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EntryPoint {
    /// Architecture/OS the symbol is valid for.
    pub arch: CodeArch,
    /// Exported symbol name (commonly `EffectMain`).
    pub symbol: String,
}

/// Where the static description came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DescriptorSource {
    /// Parsed from the PiPL resource without executing plug-in code.
    Pipl,
    /// Obtained by calling the plug-in's exported registration function
    /// (only ever done inside the sandbox process).
    RegistrationCallback,
}

/// Raw global out-flag words as declared statically. Opaque in P1: bit
/// meanings are applied by the P3 shim (which produces [`crate::caps::EffectCaps`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct RawOutFlags {
    /// First flag word (`AE_Effect_Global_OutFlags`).
    pub flags: u32,
    /// Second flag word (`AE_Effect_Global_OutFlags_2`).
    pub flags2: u32,
}

/// Everything the host knows about an installed effect before loading it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginDescriptor {
    /// Declared kind.
    pub kind: PluginKind,
    /// Display name (the SDK allows up to 47 characters).
    pub name: String,
    /// Effects-menu category.
    pub category: String,
    /// Stable identity.
    pub match_name: MatchName,
    /// Effect version.
    pub version: EffectVersion,
    /// API spec version built against.
    pub spec_version: SpecVersion,
    /// Entry points per architecture.
    pub entry_points: Vec<EntryPoint>,
    /// Statically declared out-flags (must match GLOBAL_SETUP; a mismatch is a
    /// warning AE reports to developers and BAE logs).
    pub declared_flags: RawOutFlags,
    /// Optional support URL (newer SDKs).
    pub support_url: Option<String>,
    /// Optional locale-keyed search keywords JSON (newer SDKs).
    pub search_keywords_json: Option<String>,
    /// Optional short description (newer SDKs).
    pub description: Option<String>,
    /// Provenance.
    pub source: DescriptorSource,
}

/// On-disk packaging of a plug-in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginPackage {
    /// Windows: a single DLL with the `.aex` extension.
    WindowsDll {
        /// Path to the `.aex` file (BAE stores paths with `/` separators).
        path: String,
    },
    /// macOS: a `.plugin` bundle directory containing a Mach-O executable.
    MacBundle {
        /// Path to the `.plugin` directory.
        bundle: String,
        /// Architectures present in the executable (universal binaries list
        /// both).
        slices: Vec<CodeArch>,
    },
}

/// How (and whether) a given binary can be hosted by a BAE build running on
/// `host` architecture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostingFeasibility {
    /// Native slice present; can run in-process or in a native sandbox.
    Native,
    /// Only a foreign-architecture slice exists, but the OS can translate it
    /// in a *separate* process (Rosetta 2 on macOS for x86_64; x64 emulation
    /// on Windows ARM64). In-process loading is impossible.
    TranslatedSandboxOnly,
    /// No usable slice.
    Unsupported,
}

/// Decide hosting feasibility from the slices a binary contains.
pub fn hosting_feasibility(host: CodeArch, slices: &[CodeArch]) -> HostingFeasibility {
    use CodeArch::*;
    if slices.contains(&host) {
        return HostingFeasibility::Native;
    }
    let translated = match host {
        MacArm64 => slices.contains(&MacX64),
        // ARM64EC binaries are loadable by x64-emulated processes and by
        // ARM64EC processes; a plain ARM64 host can only use them out of
        // process. x64 code runs under emulation on Windows on Arm.
        WindowsArm64 => slices.contains(&WindowsX64) || slices.contains(&WindowsArm64Ec),
        MacX64 | WindowsX64 | WindowsArm64Ec => false,
    };
    if translated {
        HostingFeasibility::TranslatedSandboxOnly
    } else {
        HostingFeasibility::Unsupported
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_name_validation() {
        assert!(MatchName::new("").is_err());
        assert!(MatchName::new("a\0b").is_err());
        assert!(MatchName::new("x".repeat(256)).is_err());
        assert_eq!(
            MatchName::new("VENDOR Fancy Glow").unwrap().as_str(),
            "VENDOR Fancy Glow"
        );
    }

    #[test]
    fn feasibility() {
        use CodeArch::*;
        assert_eq!(
            hosting_feasibility(MacArm64, &[MacX64, MacArm64]),
            HostingFeasibility::Native
        );
        assert_eq!(
            hosting_feasibility(MacArm64, &[MacX64]),
            HostingFeasibility::TranslatedSandboxOnly
        );
        assert_eq!(
            hosting_feasibility(WindowsX64, &[WindowsArm64]),
            HostingFeasibility::Unsupported
        );
        assert_eq!(
            hosting_feasibility(WindowsArm64, &[WindowsX64]),
            HostingFeasibility::TranslatedSandboxOnly
        );
    }
}
