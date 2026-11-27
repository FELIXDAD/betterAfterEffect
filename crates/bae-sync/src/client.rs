use std::fmt;
use std::sync::Arc;

use crate::error::SyncError;

/// Identifies one running client (an app instance, an agent process, a hub) in the
/// sync protocol and names its append-only log in shared-folder mode
/// (`updates/<doc>/<client-id>.log`).
///
/// Two sessions that run at the same time must use different IDs. The app should
/// keep one ID per installation (so a client keeps appending to the same log across
/// launches) and agents should use their own. 1-64 ASCII letters, digits, `_`, `-`,
/// used as a file name as it is. Not checked yet: IDs that differ only in case
/// name the same file on case-insensitive file systems (macOS, Windows, most SMB
/// shares), and Windows device names (`nul`, `con`, ...) are no regular files
/// there; [`ClientId::random`] has neither problem.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClientId(Arc<str>);

impl ClientId {
    pub fn new(s: &str) -> Result<Self, SyncError> {
        let ok = !s.is_empty()
            && s.len() <= 64
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
        if ok {
            Ok(ClientId(Arc::from(s)))
        } else {
            Err(SyncError::InvalidClientId(s.to_owned()))
        }
    }

    /// A fresh random ID (16 hex digits).
    pub fn random() -> Self {
        let hex = uuid::Uuid::new_v4().simple().to_string();
        ClientId(Arc::from(&hex[..16]))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ClientId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for ClientId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ClientId({})", self.0)
    }
}

impl std::str::FromStr for ClientId {
    type Err = SyncError;
    fn from_str(s: &str) -> Result<Self, SyncError> {
        ClientId::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation() {
        assert!(ClientId::new("felix-mbp_01").is_ok());
        for bad in ["", "a b", "a/b", "a.log", "é", &"x".repeat(65)] {
            assert!(ClientId::new(bad).is_err(), "{bad:?}");
        }
        let r = ClientId::random();
        assert_eq!(r.as_str().len(), 16);
        assert_ne!(r, ClientId::random());
    }
}
