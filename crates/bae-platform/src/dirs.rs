//! Per-user application directories and plugin search paths.
//!
//! | Purpose | macOS | Windows | Linux |
//! |---|---|---|---|
//! | config (settings) | `~/Library/Application Support/BAE` | `%APPDATA%\BAE` | `$XDG_CONFIG_HOME/bae` |
//! | data | `~/Library/Application Support/BAE` | `%LOCALAPPDATA%\BAE` | `$XDG_DATA_HOME/bae` |
//! | state (window layout, recent files) | `…/Application Support/BAE/State` | `%LOCALAPPDATA%\BAE\State` | `$XDG_STATE_HOME/bae` |
//! | cache | `~/Library/Caches/BAE` | `%LOCALAPPDATA%\BAE\Cache` | `$XDG_CACHE_HOME/bae` |
//! | logs | `~/Library/Logs/BAE` | `%LOCALAPPDATA%\BAE\Logs` | `$XDG_STATE_HOME/bae/logs` |
//! | crash reports | `<logs>/Crashes` | `<logs>\Crashes` | `<logs>/crashes` |
//! | user plugins | `<data>/Plug-ins` | `%LOCALAPPDATA%\BAE\Plug-ins` | `<data>/plugins` |
//! | system plugins | `/Library/Application Support/BAE/Plug-ins` | `%CommonProgramFiles%\BAE\Plug-ins` | `/usr/lib/bae/plugins` |
//! | bundled plugins | `BAE.app/Contents/PlugIns` | `<exe dir>\Plug-ins` | `<exe dir>/plugins` |
//!
//! Setting `BAE_HOME=<dir>` puts everything under `<dir>` (portable installs, tests,
//! CI). Project-local plugins (`<project>/plugins`) are resolved by the plugin host.

use std::io;
use std::path::{Path, PathBuf};

/// Environment variable that relocates every directory under one root.
pub const HOME_ENV: &str = "BAE_HOME";

/// Where a plugin directory comes from. Search order is
/// project → user → system → bundled (project directories are added by the host).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginScope {
    /// Installed for the current user.
    User,
    /// Installed for all users (requires administrator rights to write).
    System,
    /// Shipped inside the application bundle / install directory.
    Bundled,
}

/// A plugin search directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDir {
    /// Directory path (may not exist).
    pub path: PathBuf,
    /// Scope of plugins found there.
    pub scope: PluginScope,
}

/// Errors resolving directories.
#[derive(Debug, thiserror::Error)]
pub enum DirsError {
    /// The user's home directory could not be determined.
    #[error("cannot determine the user's home directory; set {HOME_ENV} to choose a location")]
    NoHome,
}

/// Resolved application directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDirs {
    /// Settings files (roaming on Windows).
    pub config: PathBuf,
    /// Persistent application data.
    pub data: PathBuf,
    /// Machine-local UI state (window geometry, recent files, lock files).
    pub state: PathBuf,
    /// Disposable caches (shader cache, thumbnails, proxies).
    pub cache: PathBuf,
    /// Session log files.
    pub logs: PathBuf,
    /// Crash reports written by the panic hook.
    pub crash_reports: PathBuf,
    /// Per-user plugin directory.
    pub user_plugins: PathBuf,
    /// System-wide plugin directory, if the platform has one.
    pub system_plugins: Option<PathBuf>,
    /// Plugins shipped with the application, if the executable location is known.
    pub bundled_plugins: Option<PathBuf>,
    /// `true` when relocated through [`HOME_ENV`] or [`AppDirs::under_root`].
    pub portable: bool,
}

impl AppDirs {
    /// Resolves the platform directories, honoring [`HOME_ENV`].
    pub fn resolve() -> Result<Self, DirsError> {
        if let Some(root) = std::env::var_os(HOME_ENV).filter(|v| !v.is_empty()) {
            return Ok(Self::under_root(PathBuf::from(root)));
        }
        let base = directories::BaseDirs::new().ok_or(DirsError::NoHome)?;
        Ok(Self::from_base(&base))
    }

    /// Puts every directory under `root` (portable mode / tests).
    pub fn under_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let logs = root.join("logs");
        Self {
            config: root.join("config"),
            data: root.join("data"),
            state: root.join("state"),
            cache: root.join("cache"),
            crash_reports: logs.join("crashes"),
            logs,
            user_plugins: root.join("plugins"),
            system_plugins: None,
            bundled_plugins: bundled_plugins_dir(),
            portable: true,
        }
    }

    #[cfg(target_os = "macos")]
    fn from_base(base: &directories::BaseDirs) -> Self {
        let support = base.data_dir().join(crate::APP_NAME); // ~/Library/Application Support/BAE
        let logs = base.home_dir().join("Library/Logs").join(crate::APP_NAME);
        Self {
            config: support.clone(),
            data: support.clone(),
            state: support.join("State"),
            cache: base.cache_dir().join(crate::APP_NAME),
            crash_reports: logs.join("Crashes"),
            logs,
            user_plugins: support.join("Plug-ins"),
            system_plugins: Some(PathBuf::from("/Library/Application Support/BAE/Plug-ins")),
            bundled_plugins: bundled_plugins_dir(),
            portable: false,
        }
    }

    #[cfg(windows)]
    fn from_base(base: &directories::BaseDirs) -> Self {
        let roaming = base.config_dir().join(crate::APP_NAME); // %APPDATA%\BAE
        let local = base.data_local_dir().join(crate::APP_NAME); // %LOCALAPPDATA%\BAE
        let logs = local.join("Logs");
        let system_plugins = std::env::var_os("CommonProgramFiles")
            .filter(|v| !v.is_empty())
            .map(|dir| PathBuf::from(dir).join(crate::APP_NAME).join("Plug-ins"));
        Self {
            config: roaming,
            data: local.clone(),
            state: local.join("State"),
            cache: local.join("Cache"),
            crash_reports: logs.join("Crashes"),
            logs,
            user_plugins: local.join("Plug-ins"),
            system_plugins,
            bundled_plugins: bundled_plugins_dir(),
            portable: false,
        }
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    fn from_base(base: &directories::BaseDirs) -> Self {
        let name = crate::BINARY_NAME;
        let data = base.data_dir().join(name);
        let state = base
            .state_dir()
            .map(|dir| dir.join(name))
            .unwrap_or_else(|| data.join("state"));
        let logs = state.join("logs");
        Self {
            config: base.config_dir().join(name),
            data: data.clone(),
            cache: base.cache_dir().join(name),
            crash_reports: logs.join("crashes"),
            logs,
            state,
            user_plugins: data.join("plugins"),
            system_plugins: Some(PathBuf::from("/usr/lib/bae/plugins")),
            bundled_plugins: bundled_plugins_dir(),
            portable: false,
        }
    }

    /// Creates the writable directories (config, data, state, cache, logs, crash
    /// reports, user plugins). System and bundled directories are never created.
    pub fn create_all(&self) -> io::Result<()> {
        for dir in [
            &self.config,
            &self.data,
            &self.state,
            &self.cache,
            &self.logs,
            &self.crash_reports,
            &self.user_plugins,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    /// Plugin search directories in priority order (user → system → bundled).
    /// Project-local directories are prepended by the plugin host.
    pub fn plugin_dirs(&self) -> Vec<PluginDir> {
        let mut dirs = vec![PluginDir {
            path: self.user_plugins.clone(),
            scope: PluginScope::User,
        }];
        if let Some(path) = &self.system_plugins {
            dirs.push(PluginDir {
                path: path.clone(),
                scope: PluginScope::System,
            });
        }
        if let Some(path) = &self.bundled_plugins {
            dirs.push(PluginDir {
                path: path.clone(),
                scope: PluginScope::Bundled,
            });
        }
        dirs
    }

    /// Path of a settings file in [`Self::config`].
    pub fn config_file(&self, name: &str) -> PathBuf {
        self.config.join(name)
    }

    /// Path of a state file in [`Self::state`].
    pub fn state_file(&self, name: &str) -> PathBuf {
        self.state.join(name)
    }
}

/// `BAE.app/Contents/PlugIns` when running from a macOS bundle, otherwise a
/// `Plug-ins` (`plugins` on Linux) directory next to the executable.
fn bundled_plugins_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    Some(bundled_plugins_for(exe_dir))
}

fn bundled_plugins_for(exe_dir: &Path) -> PathBuf {
    if cfg!(target_os = "macos")
        && exe_dir.ends_with("Contents/MacOS")
        && let Some(contents) = exe_dir.parent()
    {
        return contents.join("PlugIns");
    }
    if cfg!(any(target_os = "macos", windows)) {
        exe_dir.join("Plug-ins")
    } else {
        exe_dir.join("plugins")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_root_is_self_contained() {
        let root = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under_root(root.path());
        assert!(dirs.portable);
        for dir in [
            &dirs.config,
            &dirs.data,
            &dirs.state,
            &dirs.cache,
            &dirs.logs,
        ] {
            assert!(dir.starts_with(root.path()), "{dir:?}");
        }
        dirs.create_all().unwrap();
        assert!(dirs.crash_reports.is_dir());
        assert!(dirs.user_plugins.is_dir());
        assert_eq!(dirs.plugin_dirs()[0].scope, PluginScope::User);
        assert_eq!(
            dirs.config_file("settings.json"),
            dirs.config.join("settings.json")
        );
    }

    #[test]
    fn platform_dirs_resolve_without_env() {
        // Only meaningful when BAE_HOME is unset in the test environment.
        if std::env::var_os(HOME_ENV).is_some() {
            return;
        }
        let dirs = AppDirs::resolve().unwrap();
        assert!(!dirs.portable);
        assert!(dirs.logs.is_absolute());
        assert!(dirs.system_plugins.is_some());
        let scopes: Vec<_> = dirs.plugin_dirs().iter().map(|d| d.scope).collect();
        assert_eq!(&scopes[..2], &[PluginScope::User, PluginScope::System]);
    }

    #[test]
    fn bundle_layout_on_macos() {
        let dir = bundled_plugins_for(Path::new("/Applications/BAE.app/Contents/MacOS"));
        if cfg!(target_os = "macos") {
            assert_eq!(dir, Path::new("/Applications/BAE.app/Contents/PlugIns"));
        }
        let plain = bundled_plugins_for(Path::new("/opt/bae/bin"));
        assert!(plain.starts_with("/opt/bae/bin"));
    }
}
