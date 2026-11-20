//! Function suites: catalog, tiers and the version-negotiation contract.
//!
//! AE plug-ins obtain almost every host service by *acquiring a suite* — a
//! table of function pointers identified by a name string and an integer
//! version — through the PICA basic suite passed in `in_data`, and release it
//! afterwards. Old suite versions must keep working forever; new behaviour
//! ships as a new version (SDK guide, "Effect Details › Accessing the After
//! Effects Function Suites"; "AEGPs › AEGP Suites"). BAE's own C ABI (19)
//! follows the same "suite + version + capability negotiation" design
//! (`docs/ARCHITECTURE.md` §14), so the compat host can implement AE suites as
//! thin adapters over BAE suites.
//!
//! Suite *wire names* are header constants and are deliberately absent
//! (crate rule 3); [`SuiteKey`] uses BAE identifiers.

/// Implementation priority of a suite for the compat host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tier {
    /// Needed by nearly every effect; without it plug-ins fail at setup.
    T0,
    /// Common in commercial effects.
    T1,
    /// Niche.
    T2,
    /// Only meaningful for AEGP plug-ins (out of scope for the first host).
    T3,
}

/// BAE subsystem a suite needs access to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Subsystem {
    /// Nothing beyond the call itself (pure utilities, memory).
    None,
    /// CPU pixel kernels / thread pool.
    CpuPixels,
    /// Synchronous read access to the evaluated Stage (params, layers,
    /// cameras, lights, masks, keyframes at arbitrary times).
    StageRead,
    /// Writes through the command bus (undoable, synced).
    Commands,
    /// Render graph (render other layers/comps on demand).
    Render,
    /// GPU device interop.
    Gpu,
    /// UI (panels, cursors, dialogs, drawing).
    Ui,
}

/// BAE-side identifier of a suite family. One key may cover several AE
/// suite versions; the adapter implements each published version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SuiteKey {
    /// Basic suite acquire/release (the entry to everything else).
    PicaBasic,
    /// Handle allocation (new/lock/unlock/resize/dispose).
    Handle,
    /// Create/dispose worlds, query pixel format.
    World,
    /// Pixel format queries and conversions.
    PixelFormat,
    /// Iterate a callback over pixels (8, 16, float variants) on host threads.
    Iterate,
    /// Copy, blend, composite, convolve, transform worlds.
    WorldTransform,
    /// Fill rects / mattes.
    FillMatte,
    /// Sub-pixel and area sampling.
    Sampling,
    /// C math library wrappers.
    Ansi,
    /// RGB↔HLS/YIQ, luminance helpers.
    ColorCallbacks,
    /// Param UI update, keyframe queries, state identity checks.
    ParamUtils,
    /// Float colour param values.
    ColorParam,
    /// Float point param values.
    PointParam,
    /// Angle param values.
    AngleParam,
    /// Mask path queries and bezier data.
    Path,
    /// Const sequence-data access during render (MFR).
    EffectSequenceData,
    /// Thread-safe compute cache (MFR).
    ComputeCache,
    /// GPU device info and GPU world allocation.
    GpuDevice,
    /// App services: fg/bg colours, cursor, info palette, progress, abort.
    App,
    /// Advanced app: refresh, info drawing.
    AdvApp,
    /// Custom UI registration and per-context drawing access.
    EffectCustomUi,
    /// Overlay theme colours.
    EffectCustomUiOverlayTheme,
    /// Drawbot drawing (supplier, surface, path, pen, brush, image, font).
    Drawbot,
    /// Effect → AEGP bridge (get own layer/effect refs).
    PfInterface,
    /// Layer queries (type, flags, transforms, parent, times, masks...).
    AegpLayer,
    /// Comp queries (size, frame rate, duration, layers).
    AegpComp,
    /// Stream (property) values at arbitrary times; dynamic stream flags.
    AegpStream,
    /// Keyframe read/write.
    AegpKeyframe,
    /// Camera queries.
    AegpCamera,
    /// Light queries.
    AegpLight,
    /// Utilities: undo groups, error reporting, plug-in paths, versions.
    AegpUtility,
    /// AEGP memory.
    AegpMemory,
    /// Render other layers/items to worlds.
    AegpRender,
    /// Registration of commands, hooks, idle routines, panels.
    AegpRegister,
    /// Menu commands.
    AegpCommand,
    /// Project/item/footage manipulation.
    AegpProject,
    /// Panels hosting native UI.
    AegpPanel,
}

/// One catalog entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SuiteInfo {
    /// Key.
    pub key: SuiteKey,
    /// Priority.
    pub tier: Tier,
    /// What it needs.
    pub needs: Subsystem,
    /// Called per pixel / per row (must be in-sandbox and fast, never an IPC
    /// round trip).
    pub hot_path: bool,
}

/// The catalog the compat host plans against (see spec §3.13).
pub const CATALOG: &[SuiteInfo] = &[
    SuiteInfo {
        key: SuiteKey::PicaBasic,
        tier: Tier::T0,
        needs: Subsystem::None,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::Handle,
        tier: Tier::T0,
        needs: Subsystem::None,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::World,
        tier: Tier::T0,
        needs: Subsystem::CpuPixels,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::PixelFormat,
        tier: Tier::T0,
        needs: Subsystem::None,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::Iterate,
        tier: Tier::T0,
        needs: Subsystem::CpuPixels,
        hot_path: true,
    },
    SuiteInfo {
        key: SuiteKey::WorldTransform,
        tier: Tier::T0,
        needs: Subsystem::CpuPixels,
        hot_path: true,
    },
    SuiteInfo {
        key: SuiteKey::FillMatte,
        tier: Tier::T1,
        needs: Subsystem::CpuPixels,
        hot_path: true,
    },
    SuiteInfo {
        key: SuiteKey::Sampling,
        tier: Tier::T1,
        needs: Subsystem::CpuPixels,
        hot_path: true,
    },
    SuiteInfo {
        key: SuiteKey::Ansi,
        tier: Tier::T1,
        needs: Subsystem::None,
        hot_path: true,
    },
    SuiteInfo {
        key: SuiteKey::ColorCallbacks,
        tier: Tier::T1,
        needs: Subsystem::None,
        hot_path: true,
    },
    SuiteInfo {
        key: SuiteKey::ParamUtils,
        tier: Tier::T0,
        needs: Subsystem::StageRead,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::ColorParam,
        tier: Tier::T0,
        needs: Subsystem::StageRead,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::PointParam,
        tier: Tier::T0,
        needs: Subsystem::StageRead,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::AngleParam,
        tier: Tier::T1,
        needs: Subsystem::StageRead,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::Path,
        tier: Tier::T1,
        needs: Subsystem::StageRead,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::EffectSequenceData,
        tier: Tier::T0,
        needs: Subsystem::None,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::ComputeCache,
        tier: Tier::T1,
        needs: Subsystem::None,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::GpuDevice,
        tier: Tier::T1,
        needs: Subsystem::Gpu,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::App,
        tier: Tier::T0,
        needs: Subsystem::Ui,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::AdvApp,
        tier: Tier::T2,
        needs: Subsystem::Ui,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::EffectCustomUi,
        tier: Tier::T1,
        needs: Subsystem::Ui,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::EffectCustomUiOverlayTheme,
        tier: Tier::T1,
        needs: Subsystem::Ui,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::Drawbot,
        tier: Tier::T1,
        needs: Subsystem::Ui,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::PfInterface,
        tier: Tier::T1,
        needs: Subsystem::StageRead,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::AegpLayer,
        tier: Tier::T1,
        needs: Subsystem::StageRead,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::AegpComp,
        tier: Tier::T1,
        needs: Subsystem::StageRead,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::AegpStream,
        tier: Tier::T1,
        needs: Subsystem::StageRead,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::AegpKeyframe,
        tier: Tier::T2,
        needs: Subsystem::StageRead,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::AegpCamera,
        tier: Tier::T1,
        needs: Subsystem::StageRead,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::AegpLight,
        tier: Tier::T1,
        needs: Subsystem::StageRead,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::AegpUtility,
        tier: Tier::T1,
        needs: Subsystem::Commands,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::AegpMemory,
        tier: Tier::T1,
        needs: Subsystem::None,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::AegpRender,
        tier: Tier::T2,
        needs: Subsystem::Render,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::AegpRegister,
        tier: Tier::T3,
        needs: Subsystem::Commands,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::AegpCommand,
        tier: Tier::T3,
        needs: Subsystem::Commands,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::AegpProject,
        tier: Tier::T3,
        needs: Subsystem::Commands,
        hot_path: false,
    },
    SuiteInfo {
        key: SuiteKey::AegpPanel,
        tier: Tier::T3,
        needs: Subsystem::Ui,
        hot_path: false,
    },
];

/// Look up a catalog entry.
pub fn info(key: SuiteKey) -> Option<&'static SuiteInfo> {
    CATALOG.iter().find(|s| s.key == key)
}

/// Outcome of an acquire request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcquireOutcome {
    /// The exact requested version is implemented.
    Provided,
    /// Unknown suite or version: the host returns an error and a null table;
    /// well-behaved plug-ins fall back to an older version or disable a
    /// feature. Never substitute a different version silently.
    Missing,
}

/// Version negotiation rule: the host provides exactly the versions it
/// implements; there is no "closest version" matching.
pub fn negotiate(implemented: &[u32], requested: u32) -> AcquireOutcome {
    if implemented.contains(&requested) {
        AcquireOutcome::Provided
    } else {
        AcquireOutcome::Missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_unique_and_hot_paths_are_local() {
        let mut seen = std::collections::HashSet::new();
        for s in CATALOG {
            assert!(seen.insert(s.key), "{:?} duplicated", s.key);
            if s.hot_path {
                assert!(
                    matches!(s.needs, Subsystem::None | Subsystem::CpuPixels),
                    "{:?}",
                    s.key
                );
            }
        }
        assert_eq!(info(SuiteKey::Iterate).unwrap().tier, Tier::T0);
        assert_eq!(negotiate(&[1, 2], 2), AcquireOutcome::Provided);
        assert_eq!(negotiate(&[1, 2], 3), AcquireOutcome::Missing);
    }
}
