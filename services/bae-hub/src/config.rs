//! Hub settings: `hub.toml` in the data directory, then environment variables and
//! command-line flags (later sources win).

use std::fs;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use bae_sync::HubLimits;
use serde::{Deserialize, Serialize};

use crate::error::HubError;

/// Default TCP port of the hub.
pub const DEFAULT_PORT: u16 = 7341;
/// Name of the configuration file in the data directory.
pub const CONFIG_FILE: &str = "hub.toml";

/// Environment variable with an extra write token (handy for Docker).
pub const ENV_TOKEN: &str = "BAE_HUB_TOKEN";
/// Environment variable with an extra read-only token.
pub const ENV_READ_TOKEN: &str = "BAE_HUB_READ_TOKEN";
/// Environment variable overriding `anonymous` (`none`, `read`, `write`).
pub const ENV_ANONYMOUS: &str = "BAE_HUB_ANONYMOUS";

/// What a connection may do with a project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    /// Receive everything; edits sent by the client are dropped.
    Read,
    /// Receive and send edits; may create projects.
    Write,
}

impl Access {
    pub fn as_str(self) -> &'static str {
        match self {
            Access::Read => "read",
            Access::Write => "write",
        }
    }
}

/// What the handshake response tells the client (`x-bae-access`).
impl From<Access> for bae_sync::Access {
    fn from(access: Access) -> Self {
        match access {
            Access::Read => bae_sync::Access::Read,
            Access::Write => bae_sync::Access::Write,
        }
    }
}

/// Access granted to connections without a token.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Anonymous {
    /// A token is required (default).
    #[default]
    None,
    Read,
    Write,
}

impl Anonymous {
    pub fn access(self) -> Option<Access> {
        match self {
            Anonymous::None => None,
            Anonymous::Read => Some(Access::Read),
            Anonymous::Write => Some(Access::Write),
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" | "" => Some(Anonymous::None),
            "read" => Some(Anonymous::Read),
            "write" => Some(Anonymous::Write),
            _ => None,
        }
    }
}

/// One team token (`[[token]]` in `hub.toml`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenConfig {
    /// Shown in logs and the status API instead of the secret.
    pub name: String,
    pub token: String,
    pub access: Access,
    /// Projects this token may open; `None` (or `"*"` in the list) means all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projects: Option<Vec<String>>,
}

/// Contents of `hub.toml`. Every field is optional.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    pub listen: Option<SocketAddr>,
    pub anonymous: Option<Anonymous>,
    /// Unload a project from memory after this many seconds without connections
    /// (0: never).
    pub idle_unload_secs: Option<u64>,
    /// `[limits]`: see [`HubConfig::limits`].
    pub limits: Option<LimitsConfig>,
    #[serde(default, rename = "token")]
    pub tokens: Vec<TokenConfig>,
}

/// `[limits]` in `hub.toml`: limits of client connections
/// ([`HubConfig::limits`], [`HubConfig::read_only_max_incoming`]). Every field
/// is optional.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitsConfig {
    /// Largest message sent to clients and taken from writers (MiB).
    pub max_message_mib: Option<u64>,
    /// Flow control: document data waits while more than this waits for a
    /// client (MiB).
    pub max_backlog_mib: Option<u64>,
    /// Hard cap on what waits for one client; over it the connection is
    /// dropped (MiB).
    pub max_unsent_mib: Option<u64>,
    /// Drop a client that read nothing for this long while more than
    /// `max_backlog_mib` waits for it (seconds).
    pub stall_timeout_secs: Option<u64>,
    /// Largest message taken from read-only connections (KiB).
    pub read_only_max_message_kib: Option<u64>,
}

fn bytes(n: u64, unit: u64) -> usize {
    usize::try_from(n.saturating_mul(unit)).unwrap_or(usize::MAX)
}

impl FileConfig {
    pub fn parse(text: &str) -> Result<Self, HubError> {
        toml::from_str(text).map_err(|e| HubError::Config(e.to_string()))
    }
}

/// Everything the hub needs to run.
#[derive(Clone, Debug)]
pub struct HubConfig {
    pub listen: SocketAddr,
    /// Holds `hub.toml` and `projects/<id>/`.
    pub data_dir: PathBuf,
    pub tokens: Vec<TokenConfig>,
    pub anonymous: Anonymous,
    /// `None`: keep projects loaded until shutdown.
    pub idle_unload: Option<Duration>,
    /// Drop connections that sent nothing for this long (clients say `Hello`
    /// every few seconds, so silence means a dead link, e.g. a sleeping laptop).
    /// Counted in bytes: a large message that takes longer to upload is fine.
    pub peer_timeout: Duration,
    /// How often a project relay polls its connections right after traffic...
    pub active_tick: Duration,
    /// ...backing off to this while they are quiet (worst-case added latency).
    pub quiet_tick: Duration,
    /// Poll interval while a project has no connections (presence expiry,
    /// reopening a failed project directory). Writes the store held back are not
    /// left waiting for it: they are flushed as soon as the last client leaves.
    pub empty_tick: Duration,
    /// A project relay writes to its project directory at most this often
    /// (spec 4.5: a few hundred milliseconds for shared-folder peers), so a drag
    /// that reaches the hub as dozens of updates per second becomes a few
    /// appends per second. Clients still get every update at once.
    pub store_send_interval: Duration,
    /// Limits of client connections (`bae_sync::HubLimits`, spec 3.1): the
    /// largest message, flow control (the relay holds document data back while
    /// more than `max_backlog` waits for a client, and goes on once it read
    /// that), the hard cap on what waits for one client, the stall timeout.
    /// Read-only connections take less, see `read_only_max_incoming`.
    pub limits: HubLimits,
    /// Largest message a read-only connection may send (its
    /// `HubLimits::max_incoming`, at most `limits.max_incoming`), so a
    /// read-only token cannot make the hub take in messages as large as
    /// writers may send, which it would only drop. Default 16 MiB.
    ///
    /// Not smaller: what a viewer must send grows too. Its `Hello` holds a
    /// version vector per document, about 11 bytes per (document, Loro peer)
    /// pair, and every editing session adds a peer to the documents it edits
    /// (about 95 000 pairs per MiB). And viewers edit locally: a session that
    /// does not know it is read-only (`SessionConfig::send_data`) sends those
    /// edits after every `Hello` of the hub and every reconnect. This build's
    /// clients learn this limit in the handshake (`x-bae-max-incoming`, at
    /// most `max_message_mib`) and skip a document or presence over it
    /// (`SyncError::MessageTooLarge`); a client that does not read the
    /// header (an older build, a hand-written handshake) with more than this
    /// in one document is dropped every time it connects. A viewer whose
    /// `Hello` is over it cannot connect at all: its session's pump fails
    /// (`SyncError::MessageTooLarge` without a document) and `Connection`
    /// retries with backoff.
    /// Clients that know their token is read-only (`GET /v1/status`) set
    /// `send_data = false` and never send data at all.
    pub read_only_max_incoming: usize,
}

impl HubConfig {
    /// Defaults for `data_dir`, without tokens.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        HubConfig {
            listen: SocketAddr::from((Ipv4Addr::UNSPECIFIED, DEFAULT_PORT)),
            data_dir: data_dir.into(),
            tokens: Vec::new(),
            anonymous: Anonymous::None,
            idle_unload: Some(Duration::from_secs(600)),
            peer_timeout: Duration::from_secs(60),
            active_tick: Duration::from_millis(1),
            quiet_tick: Duration::from_millis(10),
            empty_tick: Duration::from_millis(500),
            store_send_interval: Duration::from_millis(300),
            limits: HubLimits::default(),
            read_only_max_incoming: 16 << 20,
        }
    }

    /// The limits of a client connection with `access`.
    pub fn client_limits(&self, access: Access) -> HubLimits {
        match access {
            Access::Write => self.limits,
            Access::Read => HubLimits {
                max_incoming: self.read_only_max_incoming.min(self.limits.max_incoming),
                ..self.limits
            },
        }
    }

    /// Apply the settings of a `hub.toml`.
    pub fn apply_file(&mut self, file: FileConfig) {
        if let Some(listen) = file.listen {
            self.listen = listen;
        }
        if let Some(a) = file.anonymous {
            self.anonymous = a;
        }
        if let Some(secs) = file.idle_unload_secs {
            self.idle_unload = (secs > 0).then(|| Duration::from_secs(secs));
        }
        if let Some(l) = file.limits {
            if let Some(mib) = l.max_message_mib {
                self.limits.max_message = bytes(mib, 1 << 20);
                self.limits.max_incoming = self.limits.max_message;
            }
            if let Some(mib) = l.max_backlog_mib {
                self.limits.max_backlog = bytes(mib, 1 << 20);
            }
            match l.max_unsent_mib {
                Some(mib) => self.limits.max_unsent = bytes(mib, 1 << 20),
                // Follow the other two, as the default does.
                None if l.max_message_mib.is_some() || l.max_backlog_mib.is_some() => {
                    self.limits.max_unsent =
                        HubLimits::max_unsent_for(self.limits.max_message, self.limits.max_backlog);
                }
                None => {}
            }
            if let Some(secs) = l.stall_timeout_secs {
                self.limits.stall_timeout = Duration::from_secs(secs);
            }
            if let Some(kib) = l.read_only_max_message_kib {
                self.read_only_max_incoming = bytes(kib, 1 << 10);
            }
        }
        self.tokens.extend(file.tokens);
    }

    /// Apply `BAE_HUB_TOKEN`, `BAE_HUB_READ_TOKEN` and `BAE_HUB_ANONYMOUS`.
    pub fn apply_env(&mut self, env: impl Fn(&str) -> Option<String>) -> Result<(), HubError> {
        for (var, name, access) in [
            (ENV_TOKEN, "env", Access::Write),
            (ENV_READ_TOKEN, "env-read", Access::Read),
        ] {
            if let Some(token) = env(var).filter(|t| !t.trim().is_empty()) {
                self.tokens.push(TokenConfig {
                    name: name.into(),
                    token: token.trim().to_owned(),
                    access,
                    projects: None,
                });
            }
        }
        if let Some(v) = env(ENV_ANONYMOUS) {
            self.anonymous = Anonymous::parse(&v).ok_or_else(|| {
                HubError::Config(format!(
                    "{ENV_ANONYMOUS} must be none, read or write, not {v:?}"
                ))
            })?;
        }
        Ok(())
    }

    /// Check the settings (tokens long enough and unique, names present, limits
    /// not zero and fitting together).
    pub fn validate(&self) -> Result<(), HubError> {
        let l = &self.limits;
        let mib = |n: usize| n.div_ceil(1 << 20);
        for (name, zero) in [
            ("max_message_mib", l.max_message == 0 || l.max_incoming == 0),
            // 0 would hold back document data whenever a single byte waits:
            // it works, but only as a misconfiguration.
            ("max_backlog_mib", l.max_backlog == 0),
            ("max_unsent_mib", l.max_unsent == 0),
            ("stall_timeout_secs", l.stall_timeout.is_zero()),
            (
                "read_only_max_message_kib",
                self.read_only_max_incoming == 0,
            ),
        ] {
            if zero {
                return Err(HubError::Config(format!("limits: {name} must not be 0")));
            }
        }
        // Clients connect with the default limits.
        let clients_take = HubLimits::default().max_incoming;
        if l.max_message > clients_take {
            return Err(HubError::Config(format!(
                "limits: max_message_mib ({} MiB) is more than clients take ({} MiB): \
                 they would refuse such messages and get them again after every reconnect",
                mib(l.max_message),
                mib(clients_take)
            )));
        }
        let held_back = l.max_message.saturating_add(l.max_backlog);
        if l.max_unsent < held_back {
            return Err(HubError::Config(format!(
                "limits: max_unsent_mib ({} MiB) must be at least max_message_mib + \
                 max_backlog_mib ({} MiB), what may wait for a client that flow control \
                 held back; leave it out to get 2 x max_message_mib + max_backlog_mib",
                l.max_unsent >> 20,
                mib(held_back)
            )));
        }
        for (i, t) in self.tokens.iter().enumerate() {
            if t.name.trim().is_empty() {
                return Err(HubError::Config(format!("token #{} has no name", i + 1)));
            }
            if t.token.len() < MIN_TOKEN_LEN {
                return Err(HubError::Config(format!(
                    "token {:?} is shorter than {MIN_TOKEN_LEN} characters",
                    t.name
                )));
            }
            if self.tokens[..i].iter().any(|o| o.token == t.token) {
                return Err(HubError::Config(format!(
                    "token {:?} has the same secret as another token",
                    t.name
                )));
            }
        }
        Ok(())
    }

    /// The directory that holds one directory per project.
    pub fn projects_dir(&self) -> PathBuf {
        self.data_dir.join("projects")
    }
}

/// Shorter secrets are rejected (guessable on a LAN).
pub const MIN_TOKEN_LEN: usize = 16;

/// A fresh random token (64 hex digits, 244 random bits).
pub fn generate_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// What [`load`] did besides reading settings.
#[derive(Debug, Default)]
pub struct LoadReport {
    /// The configuration file that was read or created.
    pub config_path: PathBuf,
    /// A write token generated because no access was configured at all; it was
    /// saved in a new `hub.toml`. Show it to the administrator once.
    pub generated_token: Option<String>,
}

/// Build the configuration for `data_dir`: read `config_path` (default
/// `<data_dir>/hub.toml`), then the environment. When nothing grants access (no
/// file, no token variables, anonymous access off), write a new `hub.toml` with a
/// generated write token so the hub is never open by accident.
pub fn load(
    data_dir: &Path,
    config_path: Option<&Path>,
    env: impl Fn(&str) -> Option<String>,
) -> Result<(HubConfig, LoadReport), HubError> {
    let mut config = HubConfig::new(data_dir);
    let path = config_path.map_or_else(|| data_dir.join(CONFIG_FILE), Path::to_owned);
    let mut report = LoadReport {
        config_path: path.clone(),
        generated_token: None,
    };
    let file_exists = path.is_file();
    if file_exists {
        let text = fs::read_to_string(&path).map_err(HubError::io(&path))?;
        let file = FileConfig::parse(&text)
            .map_err(|e| HubError::Config(format!("{}: {e}", path.display())))?;
        config.apply_file(file);
    }
    config.apply_env(&env)?;
    if !file_exists && config.tokens.is_empty() && config.anonymous == Anonymous::None {
        let token = generate_token();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(HubError::io(dir))?;
        }
        fs::write(&path, default_config_text(&token)).map_err(HubError::io(&path))?;
        config.tokens.push(TokenConfig {
            name: "team".into(),
            token: token.clone(),
            access: Access::Write,
            projects: None,
        });
        report.generated_token = Some(token);
    }
    config.validate()?;
    Ok((config, report))
}

/// The `hub.toml` written on first start.
pub fn default_config_text(token: &str) -> String {
    format!(
        r#"# bae-hub 配置文件（首次启动时自动生成）。修改后重启服务生效。
# 说明见 deploy/README.md。

# 监听地址和端口。
# listen = "0.0.0.0:{DEFAULT_PORT}"

# 不带令牌的连接能做什么：none（拒绝，默认）、read（只读）、write（可写）。
# 只有在完全可信的局域网里才考虑 read 或 write。
# anonymous = "none"

# 工程多久没人连接就从内存里卸载（秒，0 表示不卸载）。数据都在磁盘上，下次连接时重新加载。
# idle_unload_secs = 600

# 客户端连接的上限（一般不用改；都不能是 0）。
# [limits]
# 单条消息的上限（MiB）：发给客户端的、可写客户端发来的。一个工程层的快照要放得下。
# 不能超过 256（客户端最多收这么大）；调小的话，比它大的工程层就同步不了。
# max_message_mib = 256
# 给某个客户端排队等它读的数据超过这么多（MiB）时，先不发工程数据，等它读完再发。
# 平时每个读得慢的客户端那里最多排队这么多加一条消息；NAS 内存小就调小它。
# max_backlog_mib = 16
# 给一个客户端排队的数据最多这么多（MiB），超过就断开它。只是兜底的硬上限，
# 不能小于 max_message_mib + max_backlog_mib；不写就是 2 × max_message_mib + max_backlog_mib。
# max_unsent_mib = 528
# 排队超过 max_backlog_mib、并且这么多秒内对方一个字节都没读时断开它。
# stall_timeout_secs = 30
# 只读连接发来的单条消息的上限（KiB）。只读客户端不发修改（应用知道令牌只读时就不发），
# 但 Hello 随文档数和编辑过的会话数增长，超过上限的查看者连不上；不知道自己只读的客户端还会发它本地的修改，
# 新版本的客户端从握手知道上限、放不下的自己跳过，旧版本的超过上限就会被断开。
# read_only_max_message_kib = 16384

# 团队令牌。客户端连接时带上其中一个。
#   access = "write"：可以编辑，也可以新建工程
#   access = "read"：只能看，客户端发来的修改会被丢弃
# 可以写多个 [[token]]。projects 限制这个令牌能打开的工程，不写表示全部，例如：
#   projects = ["film-a", "film-b"]
[[token]]
name = "team"
token = "{token}"
access = "write"
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn first_start_generates_a_token_file() {
        let dir = tempfile::tempdir().unwrap();
        let (config, report) = load(dir.path(), None, no_env).unwrap();
        let token = report.generated_token.expect("generated");
        assert_eq!(token.len(), 64);
        assert_eq!(config.tokens.len(), 1);
        assert_eq!(config.tokens[0].access, Access::Write);
        // The second start reads the file and generates nothing.
        let (again, report) = load(dir.path(), None, no_env).unwrap();
        assert!(report.generated_token.is_none());
        assert_eq!(again.tokens, config.tokens);
    }

    #[test]
    fn env_tokens_need_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let env = |k: &str| match k {
            ENV_TOKEN => Some("0123456789abcdef-write".to_owned()),
            ENV_READ_TOKEN => Some("0123456789abcdef-read".to_owned()),
            _ => None,
        };
        let (config, report) = load(dir.path(), None, env).unwrap();
        assert!(report.generated_token.is_none());
        assert!(!dir.path().join(CONFIG_FILE).exists());
        assert_eq!(config.tokens.len(), 2);
        assert_eq!(config.tokens[1].access, Access::Read);
    }

    #[test]
    fn file_settings() {
        let file = FileConfig::parse(
            r#"
            listen = "127.0.0.1:9000"
            anonymous = "read"
            idle_unload_secs = 0
            [[token]]
            name = "a"
            token = "aaaaaaaaaaaaaaaaaaaa"
            access = "read"
            projects = ["film-a"]
            "#,
        )
        .unwrap();
        let mut c = HubConfig::new("/data");
        c.apply_file(file);
        assert_eq!(c.listen.port(), 9000);
        assert_eq!(c.anonymous, Anonymous::Read);
        assert_eq!(c.idle_unload, None);
        assert_eq!(
            c.tokens[0].projects.as_deref(),
            Some(&["film-a".into()][..])
        );
        c.validate().unwrap();

        assert!(FileConfig::parse("bogus = 1").is_err());
        c.tokens[0].token = "short".into();
        assert!(c.validate().is_err());
    }

    #[test]
    fn limits() {
        let mut c = HubConfig::new("/data");
        assert_eq!(c.client_limits(Access::Write), HubLimits::default());
        assert_eq!(c.client_limits(Access::Read).max_incoming, 16 << 20);
        // Read-only connections only take less; they are sent as much.
        assert_eq!(
            c.client_limits(Access::Read).max_message,
            HubLimits::default().max_message
        );
        let file = FileConfig::parse(
            r#"
            [limits]
            max_message_mib = 64
            max_backlog_mib = 8
            max_unsent_mib = 200
            stall_timeout_secs = 90
            read_only_max_message_kib = 256
            "#,
        )
        .unwrap();
        c.apply_file(file);
        c.validate().unwrap();
        let write = c.client_limits(Access::Write);
        assert_eq!(
            (write.max_message, write.max_incoming),
            (64 << 20, 64 << 20)
        );
        assert_eq!((write.max_backlog, write.max_unsent), (8 << 20, 200 << 20));
        assert_eq!(write.stall_timeout, Duration::from_secs(90));
        let read = c.client_limits(Access::Read);
        assert_eq!((read.max_message, read.max_incoming), (64 << 20, 256 << 10));
        assert_eq!(read.max_backlog, write.max_backlog);

        // Never more for read-only connections than for writers.
        c.read_only_max_incoming = 1 << 30;
        assert_eq!(c.client_limits(Access::Read).max_incoming, 64 << 20);
        assert!(FileConfig::parse("[limits]\nmax_mesage_mib = 1").is_err());
        let zero = FileConfig::parse("[limits]\nmax_message_mib = 0").unwrap();
        c.apply_file(zero);
        assert!(c.validate().is_err());
    }

    /// No limit may be 0, `max_backlog_mib` included (accepted before,
    /// although the others were refused).
    #[test]
    fn zero_limits_are_refused() {
        for field in [
            "max_message_mib",
            "max_backlog_mib",
            "max_unsent_mib",
            "stall_timeout_secs",
            "read_only_max_message_kib",
        ] {
            let mut c = HubConfig::new("/data");
            c.apply_file(FileConfig::parse(&format!("[limits]\n{field} = 0")).unwrap());
            let e = c.validate().unwrap_err().to_string();
            assert!(
                e.contains(&format!("{field} must not be 0")),
                "{field}: {e}"
            );
        }
        let mut c = HubConfig::new("/data");
        c.limits.max_backlog = 0;
        assert!(c.validate().is_err());
    }

    /// The limits must fit together: what may wait for a client held back by
    /// flow control (`max_backlog` plus one message) under the hard cap, and
    /// messages no larger than clients take. Raising `max_message_mib` alone
    /// raises the cap with it, as the default is made.
    #[test]
    fn limits_fit_together() {
        let limits = |text: &str| {
            let mut c = HubConfig::new("/data");
            c.apply_file(FileConfig::parse(&format!("[limits]\n{text}")).unwrap());
            c.validate().map(|()| c.limits)
        };
        let l = limits("max_message_mib = 64").unwrap();
        assert_eq!(l.max_unsent, (2 * 64 + 16) << 20);
        let l = limits("max_message_mib = 100\nmax_backlog_mib = 4").unwrap();
        assert_eq!(l.max_unsent, (2 * 100 + 4) << 20);
        let l = limits("max_backlog_mib = 32").unwrap();
        assert_eq!(l.max_unsent, (2 * 256 + 32) << 20);
        // Given explicitly, as long as a held-back client fits.
        let l = limits("max_message_mib = 64\nmax_unsent_mib = 80").unwrap();
        assert_eq!(l.max_unsent, 80 << 20);
        let e = limits("max_unsent_mib = 64").unwrap_err().to_string();
        assert!(e.contains("max_unsent_mib"), "{e}");
        let e = limits("max_message_mib = 64\nmax_unsent_mib = 79")
            .unwrap_err()
            .to_string();
        assert!(e.contains("max_unsent_mib"), "{e}");
        // Clients take at most 256 MiB: larger messages would be refused,
        // and sent again after every reconnect.
        let e = limits("max_message_mib = 512").unwrap_err().to_string();
        assert!(e.contains("max_message_mib"), "{e}");
        assert!(limits("max_message_mib = 256").is_ok());
        // The same checks for settings made in code.
        let mut c = HubConfig::new("/data");
        c.limits.max_unsent = c.limits.max_message;
        assert!(c.validate().is_err());
    }

    #[test]
    fn generated_file_parses() {
        let file = FileConfig::parse(&default_config_text(&generate_token())).unwrap();
        assert_eq!(file.tokens.len(), 1);
        assert_eq!(file.listen, None);
    }
}
