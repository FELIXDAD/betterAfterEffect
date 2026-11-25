//! Manifest script contributions, published in the same registry as model commands.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use bae_app::{Owner, Registry};
use bae_model::{CommandError, CommandInfo, CommandRegistry, DynCommand, EffectDef, Transaction};
use bae_script::{ModuleEntry, ProjectAccess, ScriptEnv, ScriptView};
use serde_json::{Value, json};

use crate::diag::Diagnostic;
use crate::manifest::{MANIFEST_FILE, PluginManifest};
use crate::package::PackageSource;

/// Connects installed command and automation entries to the shell's menus and shortcuts.
pub trait ScriptBindings: std::fmt::Debug + Send + Sync {
    /// Publish entries after a successful load. A reload replaces this owner's entries.
    fn update(&self, id: &str, entries: &[ScriptEntry]);
}

/// One callable export and its optional shell bindings.
#[derive(Debug, Clone)]
pub struct ScriptEntry {
    /// Registry ID, English name and MCP schema.
    pub info: CommandInfo,
    export: String,
    /// Slash-separated English menu path.
    pub menu: Option<String>,
    /// Name from the package's Simplified Chinese locale.
    pub name_zh: Option<String>,
    /// Default chords by workspace.
    pub shortcuts: BTreeMap<String, String>,
}

/// An installed module. Its exports share the source and limits.
#[derive(Debug, Clone)]
pub struct ScriptPackage {
    source: Arc<str>,
    heap_bytes: usize,
    access: ProjectAccess,
    pub(crate) entries: Vec<ScriptEntry>,
}

impl ScriptPackage {
    pub(crate) fn read(
        m: &PluginManifest,
        files: &dyn PackageSource,
    ) -> Result<Option<Self>, Diagnostic> {
        let Some(script) = &m.script else {
            return Ok(None);
        };
        let mut entries = Vec::new();
        let locale_path = "locales/zh-CN.toml";
        let locale: toml::Table = if files.exists(locale_path) {
            let text = files
                .read_text(locale_path)
                .map_err(|e| Diagnostic::error(locale_path, e.to_string()))?;
            toml::from_str(&text).map_err(|e| Diagnostic::error(locale_path, e.to_string()))?
        } else {
            toml::Table::new()
        };
        let name_zh = |kind: &str, id: &str| {
            locale
                .get(kind)?
                .get(id)?
                .get("name")?
                .as_str()
                .map(str::to_owned)
        };
        let info =
            |prefix: &str, id: &str, name: &str, description: &str, params_schema| CommandInfo {
                id: format!("{prefix}.{}.{id}", m.plugin.id),
                title: name.into(),
                description: description.into(),
                params_schema,
                owner: Some(m.plugin.id.clone()),
            };
        for c in m.commands.iter().filter(|c| c.implementation == "script") {
            let text = files
                .read_text(&c.schema)
                .map_err(|e| Diagnostic::error(&c.schema, e.to_string()))?;
            let schema: Value = serde_json::from_str(&text)
                .map_err(|e| Diagnostic::error(&c.schema, e.to_string()))?;
            if !schema.is_object() {
                return Err(Diagnostic::error(
                    &c.schema,
                    "expected an object describing command parameters",
                ));
            }
            entries.push(ScriptEntry {
                info: info("plugin", &c.id, &c.name, &c.description, schema),
                export: c.entry.clone().expect("validated script entry"),
                menu: c.menu.clone(),
                name_zh: name_zh("command", &c.id),
                shortcuts: c.shortcut.clone(),
            });
        }
        for a in &m.automations {
            entries.push(ScriptEntry {
                info: info(
                    "script",
                    &a.id,
                    &a.name,
                    &a.description,
                    json!({"type":"object","properties":{}}),
                ),
                export: a.entry.clone(),
                menu: Some(a.menu.clone().unwrap_or_else(|| "File/Scripts".into())),
                name_zh: name_zh("automation", &a.id),
                shortcuts: BTreeMap::new(),
            });
        }
        let source = files
            .read_text(&script.entry)
            .map_err(|e| Diagnostic::error(&script.entry, e.to_string()))?;
        let heap_bytes = (script.memory_limit_mb.unwrap_or(64) as usize) << 20;
        bae_script::check_module(
            &source,
            heap_bytes,
            entries.iter().map(|e| e.export.as_str()),
        )
        .map_err(|e| Diagnostic::error(&script.entry, e))?;
        Ok(Some(Self {
            source: source.into(),
            heap_bytes,
            access: m.permissions.project.unwrap_or(ProjectAccess::Read),
            entries,
        }))
    }
}

#[derive(Debug)]
struct Services {
    commands: OnceLock<CommandRegistry>,
    defs: Arc<Registry<EffectDef>>,
    view: Option<Arc<dyn ScriptView>>,
}

struct ScriptCommand {
    info: CommandInfo,
    export: String,
    source: Arc<str>,
    heap_bytes: usize,
    access: ProjectAccess,
    services: Arc<Services>,
}

impl DynCommand for ScriptCommand {
    fn info(&self) -> &CommandInfo {
        &self.info
    }

    fn run_json(&self, tx: &mut Transaction<'_>, params: Value) -> Result<Value, CommandError> {
        let selection = self
            .services
            .view
            .as_ref()
            .map(|v| v.selection())
            .unwrap_or_default();
        let commands = self
            .services
            .commands
            .get_or_init(|| bae_script::standard_commands(self.services.defs.clone()));
        let env = ScriptEnv {
            commands: &|id| commands.get(id).cloned(),
            effects: &|name| bae_script::find_effect(&self.services.defs, name),
            active_comp: selection.comp,
            selected_layers: selection.layers,
            time: selection.time,
            time_limit: Some(Duration::from_secs(10)),
        };
        let entry = ModuleEntry {
            source: &self.source,
            export: &self.export,
            args: &params,
            heap_bytes: self.heap_bytes,
        };
        let report = bae_script::run_entry(tx, &env, &self.info.id, entry, self.access);
        match report.error {
            Some(error) => Err(CommandError::Failed(format!(
                "{} ({}): {error}",
                self.info.id, self.export
            ))),
            None => Ok(report.result),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ScriptCommands {
    pub registry: Arc<Registry<dyn DynCommand>>,
    services: Arc<Services>,
}

impl ScriptCommands {
    pub fn new(
        registry: Arc<Registry<dyn DynCommand>>,
        defs: Arc<Registry<EffectDef>>,
        view: Option<Arc<dyn ScriptView>>,
    ) -> Self {
        Self {
            registry,
            services: Arc::new(Services {
                commands: OnceLock::new(),
                defs,
                view,
            }),
        }
    }

    pub fn install(
        &self,
        id: &str,
        package: Option<&ScriptPackage>,
        scope: crate::Scope,
    ) -> Result<(), Diagnostic> {
        let owner = Owner::plugin(id);
        self.registry.batch(|tx| {
            let snapshot = self.registry.snapshot();
            let entries = package.into_iter().flat_map(|p| p.entries.iter());
            for entry in entries {
                let info = &entry.info;
                if let Some(other) = snapshot.entry(&info.id).filter(|e| e.owner() != &owner) {
                    return Err(Diagnostic::error(
                        MANIFEST_FILE,
                        format!(
                            "command {} is already registered by {}",
                            info.id,
                            other.owner()
                        ),
                    ));
                }
            }
            tx.unregister_owner(&owner);
            if let Some(p) = package {
                for entry in &p.entries {
                    let info = &entry.info;
                    let command = ScriptCommand {
                        info: info.clone(),
                        export: entry.export.clone(),
                        source: p.source.clone(),
                        heap_bytes: p.heap_bytes,
                        access: if scope == crate::Scope::Project {
                            p.access.min(ProjectAccess::Read)
                        } else {
                            p.access
                        },
                        services: self.services.clone(),
                    };
                    tx.register_arc(owner.clone(), info.id.as_str(), Arc::new(command))
                        .expect("validated unique contribution id");
                }
            }
            Ok(())
        })
    }
}

impl Default for ScriptCommands {
    fn default() -> Self {
        Self::new(Arc::default(), Arc::default(), None)
    }
}
