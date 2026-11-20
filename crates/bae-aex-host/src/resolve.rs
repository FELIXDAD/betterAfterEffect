//! Effect resolution by match name (AE project import and project open).
//!
//! `docs/ARCHITECTURE.md` §15: an imported (or re-opened) effect identified
//! by an AE match name is resolved in this order — BAE native implementation
//! → installed BAE plug-in claiming that match name → this compat host (if
//! the AE plug-in is installed and hostable) → "missing effect" placeholder
//! that keeps every parameter value, keyframe and the flat sequence-data blob
//! so nothing is lost when the project goes back to AE.

use crate::pipl::{HostingFeasibility, MatchName};

/// Who will implement an effect instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// A BAE built-in effect claims the match name (parameter translation is
    /// done by the 15 interop mapping table).
    Native {
        /// BAE effect type id.
        effect_type: String,
    },
    /// A BAE plug-in declares compatibility with this match name.
    BaePlugin {
        /// Plug-in id.
        plugin_id: String,
    },
    /// The AE plug-in itself, through the compat host.
    AeCompat {
        /// How it will be hosted.
        feasibility: HostingFeasibility,
    },
    /// Nothing can render it: placeholder keeping all data.
    Missing,
}

/// Lookup interface implemented by the effect registry (19) and the compat
/// host's plug-in scanner.
pub trait EffectProviders {
    /// BAE native effect claiming `m`.
    fn native(&self, m: &MatchName) -> Option<String>;
    /// BAE plug-in claiming `m`.
    fn bae_plugin(&self, m: &MatchName) -> Option<String>;
    /// Installed AE plug-in for `m` and how it could be hosted.
    fn ae_plugin(&self, m: &MatchName) -> Option<HostingFeasibility>;
    /// Whether the user enabled the AE compat layer at all.
    fn compat_enabled(&self) -> bool;
}

/// Resolve in the documented order.
pub fn resolve(m: &MatchName, p: &dyn EffectProviders) -> Resolution {
    if let Some(effect_type) = p.native(m) {
        return Resolution::Native { effect_type };
    }
    if let Some(plugin_id) = p.bae_plugin(m) {
        return Resolution::BaePlugin { plugin_id };
    }
    if p.compat_enabled() {
        match p.ae_plugin(m) {
            Some(HostingFeasibility::Unsupported) | None => {}
            Some(feasibility) => return Resolution::AeCompat { feasibility },
        }
    }
    Resolution::Missing
}

#[cfg(test)]
mod tests {
    use super::*;

    struct P {
        native: bool,
        plugin: bool,
        ae: Option<HostingFeasibility>,
        enabled: bool,
    }
    impl EffectProviders for P {
        fn native(&self, _: &MatchName) -> Option<String> {
            self.native.then(|| "bae.blur".into())
        }
        fn bae_plugin(&self, _: &MatchName) -> Option<String> {
            self.plugin.then(|| "com.vendor.glow".into())
        }
        fn ae_plugin(&self, _: &MatchName) -> Option<HostingFeasibility> {
            self.ae
        }
        fn compat_enabled(&self) -> bool {
            self.enabled
        }
    }

    #[test]
    fn order() {
        let m = MatchName::new("VENDOR Glow").unwrap();
        let all = P {
            native: true,
            plugin: true,
            ae: Some(HostingFeasibility::Native),
            enabled: true,
        };
        assert!(matches!(resolve(&m, &all), Resolution::Native { .. }));
        let p = P {
            native: false,
            ..all
        };
        assert!(matches!(resolve(&m, &p), Resolution::BaePlugin { .. }));
        let p = P { plugin: false, ..p };
        assert_eq!(
            resolve(&m, &p),
            Resolution::AeCompat {
                feasibility: HostingFeasibility::Native
            }
        );
        let p = P {
            enabled: false,
            ..p
        };
        assert_eq!(resolve(&m, &p), Resolution::Missing);
        let p = P {
            enabled: true,
            ae: Some(HostingFeasibility::Unsupported),
            ..p
        };
        assert_eq!(resolve(&m, &p), Resolution::Missing);
    }
}
