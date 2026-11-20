//! BAE application module system.
//!
//! Every feature crate contributes to the application through an [`AppModule`]:
//! in [`AppModule::build`] it inserts resources and registers commands, panels,
//! workspaces, CLI subcommands, importers, exporters, effects … into typed
//! [`Registry`]s. `apps/bae/src/main.rs` only lists modules; adding a feature
//! never requires editing the shell.
//!
//! ```
//! use bae_app::{AppBuilder, AppModule};
//!
//! struct Importer { extensions: &'static [&'static str] }
//!
//! struct PngModule;
//! impl AppModule for PngModule {
//!     fn build(&self, app: &mut AppBuilder) {
//!         app.register("bae.media.png", Importer { extensions: &["png"] })
//!             .expect("unique id");
//!     }
//! }
//!
//! let mut builder = AppBuilder::new();
//! builder.add_module(PngModule);
//! let mut app = builder.build();
//! let importers = app.registry::<Importer>();
//! assert_eq!(importers.get("bae.media.png").unwrap().extensions, &["png"]);
//! ```
//!
//! # App modules vs. plugins
//!
//! *App modules* are internal: Rust crates compiled into the binary and wired up at
//! startup. *Plugins* are user-facing, third-party extensions loaded by the plugin
//! host (`bae-plugin`, workstream 19) — possibly long after startup, and possibly
//! unloaded again. Both register into the **same** registries, which is why every
//! registration records an [`Owner`] and why registries are internally synchronized
//! and support registration and removal at any time.
//!
//! This crate has no platform, GPU or UI dependencies so that every crate —
//! including headless domain crates — can implement [`AppModule`].

mod builder;
mod registry;
mod resources;

pub use builder::{App, AppBuilder, AppModule, ModuleInfo};
pub use registry::{
    AnyRegistry, Entry, Owner, Registry, RegistryChange, RegistryError, RegistryEvent,
    RegistrySnapshot, RegistryTransaction, SubscriptionId,
};
pub use resources::Resources;
