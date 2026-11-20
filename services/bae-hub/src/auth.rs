//! Team tokens (prototype of the hub's permissions): every connection presents a
//! token and gets read or write access to some or all projects.
//!
//! Secrets are kept only as BLAKE3 hashes and compared with BLAKE3's
//! constant-time equality, so response timing does not leak how much of a guess
//! was right.

use std::sync::Arc;

use crate::config::{Access, Anonymous, TokenConfig};
use crate::project::ProjectId;

/// Name reported for connections without a token.
pub const ANONYMOUS: &str = "anonymous";

struct Entry {
    name: Arc<str>,
    hash: blake3::Hash,
    access: Access,
    projects: Option<Arc<[String]>>,
}

/// The configured tokens.
pub struct Auth {
    entries: Vec<Entry>,
    anonymous: Anonymous,
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Auth")
            .field("tokens", &self.entries.len())
            .field("anonymous", &self.anonymous)
            .finish()
    }
}

/// Why a request was not authenticated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthError {
    /// No token, and anonymous access is off.
    Missing,
    /// The token matches no configured token.
    Invalid,
}

/// Who a connection is and what it may do.
#[derive(Clone, Debug)]
pub struct Grant {
    /// The token's name (never the secret).
    pub name: Arc<str>,
    pub access: Access,
    projects: Option<Arc<[String]>>,
}

impl Grant {
    /// Access to `project`, or `None` if this token may not open it.
    pub fn access_to(&self, project: &ProjectId) -> Option<Access> {
        match &self.projects {
            None => Some(self.access),
            Some(list) => list
                .iter()
                .any(|p| p == "*" || p == project.as_str())
                .then_some(self.access),
        }
    }
}

impl Auth {
    pub fn new(tokens: &[TokenConfig], anonymous: Anonymous) -> Self {
        Auth {
            entries: tokens
                .iter()
                .map(|t| Entry {
                    name: Arc::from(t.name.as_str()),
                    hash: blake3::hash(t.token.as_bytes()),
                    access: t.access,
                    projects: t.projects.as_ref().map(|p| Arc::from(p.as_slice())),
                })
                .collect(),
            anonymous,
        }
    }

    /// Anonymous access level (`None` when a token is required).
    pub fn anonymous(&self) -> Option<Access> {
        self.anonymous.access()
    }

    pub fn authenticate(&self, token: Option<&str>) -> Result<Grant, AuthError> {
        let token = token.map(str::trim).filter(|t| !t.is_empty());
        let Some(token) = token else {
            return self
                .anonymous
                .access()
                .map(|access| Grant {
                    name: Arc::from(ANONYMOUS),
                    access,
                    projects: None,
                })
                .ok_or(AuthError::Missing);
        };
        let hash = blake3::hash(token.as_bytes());
        // Look at every entry so the time taken does not depend on which matched.
        let mut found = None;
        for e in &self.entries {
            if e.hash == hash && found.is_none() {
                found = Some(e);
            }
        }
        found
            .map(|e| Grant {
                name: e.name.clone(),
                access: e.access,
                projects: e.projects.clone(),
            })
            .ok_or(AuthError::Invalid)
    }
}

/// The token of a request: `Authorization: Bearer <token>`, or the `token` query
/// parameter (for clients that cannot set headers on a WebSocket handshake).
pub fn token_from_request<'a>(
    authorization: Option<&'a str>,
    query_token: Option<&'a str>,
) -> Option<&'a str> {
    authorization
        .and_then(|h| {
            let (scheme, rest) = h.trim().split_once(' ')?;
            scheme.eq_ignore_ascii_case("bearer").then_some(rest.trim())
        })
        .or(query_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(name: &str, secret: &str, access: Access, projects: Option<&[&str]>) -> TokenConfig {
        TokenConfig {
            name: name.into(),
            token: secret.into(),
            access,
            projects: projects.map(|p| p.iter().map(|s| (*s).to_owned()).collect()),
        }
    }

    #[test]
    fn tokens_and_projects() {
        let auth = Auth::new(
            &[
                token("w", "write-secret-0000", Access::Write, None),
                token("r", "read-secret-00000", Access::Read, Some(&["film-a"])),
            ],
            Anonymous::None,
        );
        let a = ProjectId::new("film-a").unwrap();
        let b = ProjectId::new("film-b").unwrap();
        let w = auth.authenticate(Some("write-secret-0000")).unwrap();
        assert_eq!(&*w.name, "w");
        assert_eq!(w.access_to(&b), Some(Access::Write));
        let r = auth.authenticate(Some(" read-secret-00000 ")).unwrap();
        assert_eq!(r.access_to(&a), Some(Access::Read));
        assert_eq!(r.access_to(&b), None);
        assert_eq!(
            auth.authenticate(Some("nope")).unwrap_err(),
            AuthError::Invalid
        );
        assert_eq!(auth.authenticate(None).unwrap_err(), AuthError::Missing);
        assert_eq!(auth.authenticate(Some("")).unwrap_err(), AuthError::Missing);
    }

    #[test]
    fn anonymous_access() {
        let auth = Auth::new(&[], Anonymous::Read);
        let g = auth.authenticate(None).unwrap();
        assert_eq!(&*g.name, ANONYMOUS);
        assert_eq!(g.access, Access::Read);
        // A wrong token is still an error, not a silent downgrade.
        assert_eq!(
            auth.authenticate(Some("x")).unwrap_err(),
            AuthError::Invalid
        );
    }

    #[test]
    fn header_parsing() {
        assert_eq!(token_from_request(Some("Bearer abc"), None), Some("abc"));
        assert_eq!(token_from_request(Some("bearer  abc "), None), Some("abc"));
        assert_eq!(token_from_request(Some("Basic abc"), Some("q")), Some("q"));
        assert_eq!(token_from_request(None, Some("q")), Some("q"));
        assert_eq!(token_from_request(None, None), None);
    }
}
