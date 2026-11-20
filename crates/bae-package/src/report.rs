//! Packaging reports: what was packed, what is missing, sizes.
//!
//! Reports are plain data (JSON for agents and logs); the UI formats and
//! localizes them.

use bae_model::PrimPath;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::collect::AssetKind;

/// What produced the report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReportKind {
    /// Dry run: what packing would do.
    Scan,
    /// A `.baez` was written.
    Baez,
    /// A project folder was written ("collect files").
    Folder,
}

/// What happened to one asset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssetStatus {
    /// Its files are in the package (for a scan: would be).
    Packed,
    /// The file could not be found; the reference is left as it was.
    Missing,
    /// Not used by any comp in scope and unused assets were excluded; the
    /// reference is left as it was.
    Excluded,
    /// No file to pack (solid, placeholder).
    NoFile,
}

/// One asset in a report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AssetReport {
    pub path: PrimPath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub kind: AssetKind,
    /// Referenced by something in scope.
    pub used: bool,
    pub status: AssetStatus,
    /// The original reference (`file:original`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original: Option<String>,
    /// Where the file was found (disk path, or `<package>!/<entry>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The asset's new `file:relative` in the package (`assets/…`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packaged_as: Option<String>,
    /// Files (a sequence has one per frame).
    pub files: u64,
    /// Bytes of those files.
    pub bytes: u64,
}

/// A font in a report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FontReport {
    /// PostScript name.
    pub name: String,
    /// Text layers using it.
    pub users: Vec<PrimPath>,
    /// Font file packed (reserved; fonts are only recorded for now — the
    /// receiver needs the font installed or gets a fallback).
    pub embedded: bool,
}

/// Totals.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PackageStats {
    /// Asset prims in the project.
    pub assets: usize,
    pub packed: usize,
    pub missing: usize,
    pub excluded: usize,
    /// Distinct files packed (assets sharing a file count it once).
    pub files: u64,
    /// Bytes of those files.
    pub asset_bytes: u64,
    /// Project data (manifest and sublayer snapshots), uncompressed.
    pub project_bytes: u64,
    /// Size of the written `.baez`, or of everything written into the folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_bytes: Option<u64>,
}

/// The result of scanning, packing or collecting a project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PackageReport {
    pub kind: ReportKind,
    /// The written `.baez` or folder (`/`-separated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Every asset prim, in project order.
    pub assets: Vec<AssetReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fonts: Vec<FontReport>,
    /// Other projects referenced (`Reference::project`); not packed, so these
    /// references break unless the receiver has the projects too.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_projects: Vec<String>,
    /// Things the user should know (stable English sentences; the UI shows them
    /// as-is or maps them by `code`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<Warning>,
    pub stats: PackageStats,
}

impl PackageReport {
    /// Assets whose file could not be found.
    pub fn missing(&self) -> impl Iterator<Item = &AssetReport> {
        self.assets
            .iter()
            .filter(|a| a.status == AssetStatus::Missing)
    }

    pub fn asset(&self, path: &PrimPath) -> Option<&AssetReport> {
        self.assets.iter().find(|a| &a.path == path)
    }

    /// Whether the package is complete: nothing missing, nothing outside it.
    pub fn is_self_contained(&self) -> bool {
        self.stats.missing == 0 && self.stats.excluded == 0 && self.external_projects.is_empty()
    }
}

/// A warning with a stable code (for i18n in the UI) and an English message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Warning {
    pub code: WarningCode,
    pub message: String,
}

/// Warning codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WarningCode {
    /// Some asset files are missing.
    MissingAssets,
    /// Fonts are recorded by name only; mind font licenses before embedding.
    FontsNotEmbedded,
    /// Other projects are referenced and not packed.
    ExternalProjects,
    /// Clip trimming was requested but is not available yet; full files were packed.
    TrimNotAvailable,
}

impl Warning {
    pub(crate) fn new(code: WarningCode, message: impl Into<String>) -> Self {
        Warning {
            code,
            message: message.into(),
        }
    }
}
