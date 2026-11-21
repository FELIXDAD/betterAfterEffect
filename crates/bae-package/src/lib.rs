//! Project and asset packaging (workstream 21, ARCHITECTURE §16).
//!
//! - [`collect`]: which `Asset` prims a project has, which the timeline uses
//!   (directly or through nested comps), which fonts text layers need.
//! - [`resolve`]: where asset files are — inside the `.baez` the project was
//!   opened from, relative path, original path, path mappings (Windows drive
//!   letters, UNC, `/Volumes`), search by name and fingerprint — and batch
//!   relinking ([`plan_relink`]).
//! - [`pack`]: write a self-contained `.baez` ([`pack()`](pack::pack)), collect
//!   files into a `.bae` folder ([`collect_files`]), dry-run [`scan`], [`unpack`],
//!   and [`open_project`] to open a `.baez` directly with assets read in place.
//! - [`container`]: [`PackageReader`] — byte ranges and readers of embedded
//!   assets (stored uncompressed, so decoders can read them in place).
//! - [`report`]: [`PackageReport`] — missing assets, sizes, fonts, warnings.
//! - [`commands`]: `package.scan`, `package.pack`, `package.collect`,
//!   `package.unpack`, `asset.relink`, `asset.set_files`
//!   ([`register_commands`]).
//!
//! Packaging never modifies the source project or its asset files: the package
//! gets a copy whose asset references point into it (`file:relative =
//! assets/…`).
//!
//! Video trimming (only the used source ranges, workstream 08's GOP-aligned
//! copies) is reserved in [`TrimOptions`]; this version packs whole files.
//!
//! This crate has no GPU, window or platform dependencies.

pub mod collect;
pub mod commands;
pub mod container;
mod error;
pub mod pack;
pub mod report;
pub mod resolve;
mod sequence;

pub use collect::{AssetEntry, AssetKind, CollectOptions, FontUse, Inventory, SequencePattern};
pub use container::{EntryReader, PackageReader, PackagedEntry};
pub use error::PackageError;
pub use pack::{
    FontPolicy, OpenedProject, PackOptions, PackSource, TrimMode, TrimOptions, UnpackReport,
    WriteOptions, collect_files, open_project, pack, read_manifest, scan, unpack,
};
pub use report::{
    AssetReport, AssetStatus, FontReport, PackageReport, PackageStats, ReportKind, Warning,
    WarningCode,
};
pub use resolve::{
    AssetLocation, AssetLocator, FileSource, FoundVia, Relink, RelinkPlan, plan_relink,
};

/// Register the packaging commands (`package.*`, `asset.relink`,
/// `asset.set_files`).
pub fn register_commands(registry: &mut bae_model::CommandRegistry) {
    commands::register(registry);
}
