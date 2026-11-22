//! What the dialogs remember between runs (AE keeps the last New Composition,
//! New Solid … settings in its preferences file): `dialogs.json` in the config
//! folder, read when the module starts and written when a dialog's OK changes
//! it. Without a config folder (tests, smoke runs) nothing is persisted.

use std::path::PathBuf;

use bae_platform::{SettingsFile, VersionedSettings};
use serde::{Deserialize, Serialize};

use super::{CompSettingsMemory, Dialogs};

/// File name in the config folder.
pub const FILE_NAME: &str = "dialogs.json";

/// The persisted part of [`Dialogs`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Remembered {
    pub comp: CompSettingsMemory,
}

impl VersionedSettings for Remembered {
    const KIND: &'static str = "dialog-memory";
    const VERSION: u32 = 1;
}

impl Remembered {
    pub(crate) fn of(d: &Dialogs) -> Self {
        Remembered {
            comp: d.comp.clone(),
        }
    }

    pub(crate) fn apply(self, d: &mut Dialogs) {
        d.comp = self.comp;
    }
}

/// Load the remembered values from `path`.
pub(crate) fn load(path: &PathBuf) -> Remembered {
    SettingsFile::<Remembered>::new(path).load().value
}

/// Save `d`'s remembered values to its file, if it has one.
pub(crate) fn save(d: &Dialogs) {
    if let Some(path) = &d.file
        && let Err(e) = SettingsFile::<Remembered>::new(path).save(&Remembered::of(d))
    {
        tracing::warn!("could not save {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembered_values_survive_a_restart() {
        let path = std::env::temp_dir().join(format!("bae-dialogs-{}.json", std::process::id()));
        let shared = super::super::SharedDialogs::with_file(path.clone());
        {
            let mut d = shared.lock();
            d.comp.values = Some(super::super::comp_settings::Values {
                width: 960,
                fps: 60.0,
                ..Default::default()
            });
            d.comp.lock_aspect = true;
            save(&d);
        }
        // A new run reads them back.
        let again = super::super::SharedDialogs::with_file(path.clone());
        let d = again.lock();
        let v = d.comp.values.as_ref().expect("remembered");
        assert_eq!((v.width, v.fps, d.comp.lock_aspect), (960, 60.0, true));
        let _ = std::fs::remove_file(path);
    }
}
