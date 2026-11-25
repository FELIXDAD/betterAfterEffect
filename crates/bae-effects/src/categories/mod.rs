//! Registration of the built-in effects by AE category.
use crate::{Builtin, Implementation, PLUGIN_ID, declarative_info};
use bae_plugin::{Diagnostic, MemorySource, declarative::DeclarativeEffect};

/// One embedded declarative package.
struct Embedded {
    dir: &'static str,
    manifest: &'static str,
    zh: &'static str,
    files: &'static [(&'static str, &'static str)],
}

macro_rules! embedded {
    ($dir:literal, [$($file:literal),* $(,)?]) => {
        $crate::categories::Embedded {
            dir: $dir,
            manifest: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/packages/", $dir, "/bae-plugin.toml")),
            zh: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/packages/", $dir, "/locales/zh-CN.toml")),
            files: &[$(($file, include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/packages/", $dir, "/", $file)))),*],
        }
    };
}

pub(super) mod blur_sharpen;
pub(super) mod channel;
pub(super) mod color;
pub(super) mod controls;
pub(super) mod distort;
pub(super) mod generate;
pub(super) mod keying;
pub(super) mod matte;
pub(super) mod noise_grain;
pub(super) mod perspective;
pub(super) mod stylize;
pub(super) mod text;
pub(super) mod time;
pub(super) mod transition;
pub(super) mod utility;

impl Embedded {
    fn register(&self, out: &mut Vec<Builtin>) -> Result<(), Vec<Diagnostic>> {
        let mut source = MemorySource::new(format!("builtin:{PLUGIN_ID}/{}", self.dir))
            .with_file("bae-plugin.toml", self.manifest)
            .with_file(
                "shaders/threshold_math.wgsl",
                include_str!("../../packages/threshold/shaders/threshold_math.wgsl"),
            )
            .with_file(
                "shaders/channel_math.wgsl",
                include_str!("../shaders/channel_math.wgsl"),
            )
            .with_file(
                "shaders/blend.wgsl",
                include_str!("../shaders/generated_blend.wgsl"),
            );
        for (path, text) in self.files {
            source = source.with_file(*path, *text);
        }
        let zh: toml::Table = toml::from_str(self.zh).map_err(|e| {
            vec![Diagnostic::error(
                format!("{}/locales/zh-CN.toml", self.dir),
                e.to_string(),
            )]
        })?;
        for fx in DeclarativeEffect::load_builtin(&source)? {
            let info = declarative_info(&fx, &zh);
            out.push(Builtin {
                info,
                implementation: Implementation::Declarative(fx),
                alias: None,
            });
        }
        Ok(())
    }
}

fn packages(registry: &mut crate::Registry, packages: &[Embedded]) {
    for pkg in packages {
        pkg.register(&mut registry.effects)
            .unwrap_or_else(|d| panic!("bae-effects: built-in packages are invalid: {d:?}"));
    }
}
