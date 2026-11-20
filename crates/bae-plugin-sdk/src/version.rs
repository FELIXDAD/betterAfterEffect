//! Plugin API version and host/plugin negotiation (spec §6.6, §14.6).
//!
//! BAE has **one** plugin API version `MAJOR.MINOR` covering the C ABI
//! (`BAE_ABI_VERSION` in `bae_plugin.h`), the declarative shader interface
//! (spec §11) and the script API (spec §17). A plugin states the *minimum* API it
//! needs (`api = "0.1"` in `bae-plugin.toml`, `abi_required` in
//! `BaePluginExports`).
//!
//! Rules (normative copy in the spec):
//!
//! * **Stable majors (≥ 1).** A host whose current API is `H.h` accepts a plugin
//!   requiring `A.a` iff `A == H && a <= h`, or `A` is an older major the host
//!   still serves through a compatibility shim ([`HostApiSupport::older_majors`]).
//!   A plugin never runs on a host older than the minor it requires.
//! * **Unstable major 0.** A host `0.h` accepts a plugin requiring `0.a` iff
//!   `a == h` or `a` is listed in [`HostApiSupport::compatible_0x_minors`]
//!   (older 0.x minors this host explicitly still serves). 0.x minors may break
//!   the ABI, so nothing is accepted implicitly.
//! * The host presents to each plugin the **highest version of the plugin's
//!   required major** it implements ([`HostApiSupport::check`] returns it); that
//!   is the value of `BaeHost.abi_version` the plugin sees.

use core::fmt;
use core::str::FromStr;

/// A plugin API version `major.minor`.
///
/// Ordering is lexicographic (`major`, then `minor`). The C ABI packs it as
/// `(major << 16) | minor` ([`ApiVersion::pack`], `BAE_MAKE_VERSION`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApiVersion {
    /// Major version. 0 = unstable design-phase ABI.
    pub major: u16,
    /// Minor version: additions only (within a stable major).
    pub minor: u16,
}

impl ApiVersion {
    /// The API version this SDK was built against (`BAE_ABI_VERSION_MAJOR/MINOR`).
    pub const CURRENT: ApiVersion = ApiVersion::new(0, 1);

    /// Creates a version.
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    /// Packs into the C representation `(major << 16) | minor`.
    pub const fn pack(self) -> u32 {
        ((self.major as u32) << 16) | self.minor as u32
    }

    /// Unpacks the C representation.
    pub const fn unpack(packed: u32) -> Self {
        Self {
            major: (packed >> 16) as u16,
            minor: (packed & 0xFFFF) as u16,
        }
    }

    /// `true` for the unstable design-phase major 0.
    pub const fn is_unstable(self) -> bool {
        self.major == 0
    }
}

impl fmt::Display for ApiVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// Error returned when an API version string is not `MAJOR.MINOR`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseApiVersionError {
    input: String,
}

impl fmt::Display for ParseApiVersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid plugin API version {:?}: expected \"MAJOR.MINOR\" with integers 0..=65535 (e.g. \"0.1\")",
            self.input
        )
    }
}

impl std::error::Error for ParseApiVersionError {}

impl FromStr for ApiVersion {
    type Err = ParseApiVersionError;

    /// Parses `"MAJOR.MINOR"`. Leading zeros (`"01.2"`), signs, whitespace and
    /// a patch component are rejected so that the manifest value is canonical.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseApiVersionError {
            input: s.to_owned(),
        };
        let (major, minor) = s.split_once('.').ok_or_else(err)?;
        let parse = |part: &str| -> Option<u16> {
            let canonical = !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_digit())
                && (part == "0" || !part.starts_with('0'));
            if canonical { part.parse().ok() } else { None }
        };
        Ok(Self {
            major: parse(major).ok_or_else(err)?,
            minor: parse(minor).ok_or_else(err)?,
        })
    }
}

/// Why a host refuses a plugin's required API version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiMismatch {
    /// The plugin needs a newer minor (or major) than the host implements.
    /// Fix: update BAE.
    HostTooOld {
        /// What the plugin requires.
        required: ApiVersion,
        /// The newest version the host implements for that major (or its current API).
        host: ApiVersion,
    },
    /// The plugin was built for a major the host no longer serves.
    /// Fix: update the plugin.
    MajorUnsupported {
        /// What the plugin requires.
        required: ApiVersion,
        /// The host's current API.
        host: ApiVersion,
    },
    /// Unstable 0.x: the plugin's minor is neither the host's nor one of the
    /// minors the host declares compatible. Fix: rebuild the plugin with the
    /// matching SDK.
    UnstableMismatch {
        /// What the plugin requires.
        required: ApiVersion,
        /// The host's current API.
        host: ApiVersion,
    },
}

impl fmt::Display for ApiMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiMismatch::HostTooOld { required, host } => write!(
                f,
                "plugin requires BAE plugin API {required}, this BAE provides {host}; update BAE"
            ),
            ApiMismatch::MajorUnsupported { required, host } => write!(
                f,
                "plugin was built for BAE plugin API {required}, which this BAE ({host}) no longer supports; update the plugin"
            ),
            ApiMismatch::UnstableMismatch { required, host } => write!(
                f,
                "plugin was built for unstable BAE plugin API {required}, this BAE provides {host}; rebuild the plugin with the matching SDK"
            ),
        }
    }
}

impl std::error::Error for ApiMismatch {}

/// What API versions a host implements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostApiSupport {
    /// The host's current API version.
    pub current: ApiVersion,
    /// Older **stable** majors served through compatibility shims, each with the
    /// newest minor of that major the shim implements (e.g. `[ApiVersion::new(1, 7)]`
    /// on a 2.x host). Must all have `major < current.major` and `major >= 1`.
    pub older_majors: &'static [ApiVersion],
    /// Only for an unstable `current` (major 0): older 0.x minors this host
    /// still serves unchanged. Ignored for stable majors.
    pub compatible_0x_minors: &'static [u16],
}

impl HostApiSupport {
    /// What this SDK version, used as a host, supports: exactly [`ApiVersion::CURRENT`].
    pub const SDK_DEFAULT: HostApiSupport = HostApiSupport {
        current: ApiVersion::CURRENT,
        older_majors: &[],
        compatible_0x_minors: &[],
    };

    /// Checks a plugin's required API version (spec §6.6 rule 1).
    ///
    /// On success returns the version the host presents to the plugin
    /// (`BaeHost.abi_version`): the newest version of the required major the
    /// host implements.
    pub fn check(&self, required: ApiVersion) -> Result<ApiVersion, ApiMismatch> {
        let host = self.current;
        if required.major == host.major {
            if host.is_unstable() {
                return if required.minor == host.minor
                    || (required.minor < host.minor
                        && self.compatible_0x_minors.contains(&required.minor))
                {
                    Ok(host)
                } else if required.minor > host.minor {
                    Err(ApiMismatch::HostTooOld { required, host })
                } else {
                    Err(ApiMismatch::UnstableMismatch { required, host })
                };
            }
            return if required.minor <= host.minor {
                Ok(host)
            } else {
                Err(ApiMismatch::HostTooOld { required, host })
            };
        }
        if required.major > host.major {
            return Err(ApiMismatch::HostTooOld { required, host });
        }
        // required.major < host.major
        if required.is_unstable() {
            // 0.x plugins never run on a stable host.
            return Err(ApiMismatch::MajorUnsupported { required, host });
        }
        match self
            .older_majors
            .iter()
            .find(|v| v.major == required.major && v.major >= 1 && v.major < host.major)
        {
            Some(shim) if required.minor <= shim.minor => Ok(*shim),
            Some(shim) => Err(ApiMismatch::HostTooOld {
                required,
                host: *shim,
            }),
            None => Err(ApiMismatch::MajorUnsupported { required, host }),
        }
    }

    /// Binary check after `bae_plugin_entry` (spec §6.6 rule 2): the library's
    /// `abi_required` must equal the manifest's `api` (stale-manifest guard),
    /// then rule 1 applies.
    pub fn check_binary(
        &self,
        manifest_api: ApiVersion,
        abi_required_packed: u32,
    ) -> Result<ApiVersion, BinaryApiError> {
        let abi_required = ApiVersion::unpack(abi_required_packed);
        if abi_required != manifest_api {
            return Err(BinaryApiError::ManifestMismatch {
                manifest: manifest_api,
                library: abi_required,
            });
        }
        self.check(abi_required).map_err(BinaryApiError::Api)
    }
}

/// Plugin-side check (spec §6.6 rule 3): may a plugin that requires `required`
/// run on a host presenting `host`? The SDK's generated entry point returns
/// `BAE_ERR_VERSION_MISMATCH` when this is `false`.
pub const fn plugin_accepts_host(required: ApiVersion, host: ApiVersion) -> bool {
    if required.major != host.major {
        return false;
    }
    if required.major == 0 {
        // Unstable: the host only presents 0.h to plugins it decided to serve.
        // A plugin requiring a newer minor than presented must refuse.
        return required.minor <= host.minor;
    }
    required.minor <= host.minor
}

/// Error of [`HostApiSupport::check_binary`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryApiError {
    /// `abi_required` in the library differs from `api` in the manifest.
    ManifestMismatch {
        /// Manifest `api`.
        manifest: ApiVersion,
        /// Library `abi_required`.
        library: ApiVersion,
    },
    /// Rule 1 failed.
    Api(ApiMismatch),
}

impl fmt::Display for BinaryApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BinaryApiError::ManifestMismatch { manifest, library } => write!(
                f,
                "native library requires plugin API {library} but bae-plugin.toml declares api = \"{manifest}\" (stale manifest or library)"
            ),
            BinaryApiError::Api(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for BinaryApiError {}

#[cfg(test)]
mod tests {
    use super::*;

    const fn v(major: u16, minor: u16) -> ApiVersion {
        ApiVersion::new(major, minor)
    }

    #[test]
    fn pack_roundtrip_matches_c_macro() {
        // BAE_MAKE_VERSION(major, minor) = (major << 16) | minor
        assert_eq!(v(0, 1).pack(), 0x0000_0001);
        assert_eq!(v(1, 2).pack(), 0x0001_0002);
        assert_eq!(v(65535, 65535).pack(), u32::MAX);
        for x in [v(0, 0), v(0, 1), v(1, 0), v(3, 17), v(65535, 1)] {
            assert_eq!(ApiVersion::unpack(x.pack()), x);
        }
    }

    #[test]
    fn parse_canonical_only() {
        assert_eq!("0.1".parse::<ApiVersion>(), Ok(v(0, 1)));
        assert_eq!("12.345".parse::<ApiVersion>(), Ok(v(12, 345)));
        for bad in [
            "", "1", "1.", ".1", "1.2.3", "01.2", "1.02", "+1.2", "-1.2", " 1.2", "1.2 ", "a.b",
            "1.65536",
        ] {
            assert!(
                bad.parse::<ApiVersion>().is_err(),
                "{bad:?} must be rejected"
            );
        }
        assert_eq!(v(2, 7).to_string(), "2.7");
    }

    #[test]
    fn ordering() {
        assert!(v(0, 9) < v(1, 0));
        assert!(v(1, 2) < v(1, 10));
    }

    #[test]
    fn stable_same_major() {
        let host = HostApiSupport {
            current: v(1, 3),
            older_majors: &[],
            compatible_0x_minors: &[],
        };
        assert_eq!(host.check(v(1, 0)), Ok(v(1, 3)));
        assert_eq!(host.check(v(1, 3)), Ok(v(1, 3)));
        assert_eq!(
            host.check(v(1, 4)),
            Err(ApiMismatch::HostTooOld {
                required: v(1, 4),
                host: v(1, 3)
            })
        );
        assert!(matches!(
            host.check(v(2, 0)),
            Err(ApiMismatch::HostTooOld { .. })
        ));
    }

    #[test]
    fn stable_older_major_through_shim() {
        let host = HostApiSupport {
            current: v(2, 1),
            older_majors: const { &[v(1, 7)] },
            compatible_0x_minors: &[],
        };
        // Presented version is the shim's newest 1.x, not 2.1.
        assert_eq!(host.check(v(1, 0)), Ok(v(1, 7)));
        assert_eq!(host.check(v(1, 7)), Ok(v(1, 7)));
        assert_eq!(
            host.check(v(1, 8)),
            Err(ApiMismatch::HostTooOld {
                required: v(1, 8),
                host: v(1, 7)
            })
        );
        // Major 1 dropped on a host without the shim.
        let host3 = HostApiSupport {
            current: v(3, 0),
            older_majors: const { &[v(2, 4)] },
            compatible_0x_minors: &[],
        };
        assert_eq!(
            host3.check(v(1, 0)),
            Err(ApiMismatch::MajorUnsupported {
                required: v(1, 0),
                host: v(3, 0)
            })
        );
        // 0.x plugins never run on stable hosts.
        assert!(matches!(
            host.check(v(0, 1)),
            Err(ApiMismatch::MajorUnsupported { .. })
        ));
    }

    #[test]
    fn shim_entries_must_be_older_stable_majors() {
        // A malformed older_majors entry (same or newer major, or 0) is ignored.
        let host = HostApiSupport {
            current: v(2, 0),
            older_majors: const { &[v(0, 5), v(2, 9), v(3, 0)] },
            compatible_0x_minors: &[],
        };
        assert!(matches!(
            host.check(v(0, 5)),
            Err(ApiMismatch::MajorUnsupported { .. })
        ));
        assert_eq!(host.check(v(2, 0)), Ok(v(2, 0)));
        assert!(host.check(v(2, 1)).is_err());
    }

    #[test]
    fn unstable_major_zero() {
        let host = HostApiSupport {
            current: v(0, 4),
            older_majors: &[],
            compatible_0x_minors: &[2],
        };
        assert_eq!(host.check(v(0, 4)), Ok(v(0, 4)));
        assert_eq!(host.check(v(0, 2)), Ok(v(0, 4)));
        assert_eq!(
            host.check(v(0, 3)),
            Err(ApiMismatch::UnstableMismatch {
                required: v(0, 3),
                host: v(0, 4)
            })
        );
        assert_eq!(
            host.check(v(0, 5)),
            Err(ApiMismatch::HostTooOld {
                required: v(0, 5),
                host: v(0, 4)
            })
        );
        assert!(matches!(
            host.check(v(1, 0)),
            Err(ApiMismatch::HostTooOld { .. })
        ));
        // A compatible-minors entry newer than the host is meaningless.
        let odd = HostApiSupport {
            current: v(0, 1),
            older_majors: &[],
            compatible_0x_minors: &[5],
        };
        assert!(odd.check(v(0, 5)).is_err());
    }

    #[test]
    fn sdk_default_accepts_itself() {
        assert_eq!(
            HostApiSupport::SDK_DEFAULT.check(ApiVersion::CURRENT),
            Ok(ApiVersion::CURRENT)
        );
    }

    #[test]
    fn binary_check() {
        let host = HostApiSupport::SDK_DEFAULT;
        let api = ApiVersion::CURRENT;
        assert_eq!(host.check_binary(api, api.pack()), Ok(api));
        assert_eq!(
            host.check_binary(api, v(0, 9).pack()),
            Err(BinaryApiError::ManifestMismatch {
                manifest: api,
                library: v(0, 9)
            })
        );
    }

    #[test]
    fn plugin_side_check() {
        assert!(plugin_accepts_host(v(1, 2), v(1, 2)));
        assert!(plugin_accepts_host(v(1, 2), v(1, 5)));
        assert!(!plugin_accepts_host(v(1, 2), v(1, 1)));
        assert!(!plugin_accepts_host(v(1, 2), v(2, 0)));
        assert!(plugin_accepts_host(v(0, 1), v(0, 1)));
        assert!(!plugin_accepts_host(v(0, 2), v(0, 1)));
    }

    #[test]
    fn error_messages_are_actionable() {
        let e = HostApiSupport::SDK_DEFAULT.check(v(0, 99)).unwrap_err();
        assert!(e.to_string().contains("update BAE"), "{e}");
    }
}
