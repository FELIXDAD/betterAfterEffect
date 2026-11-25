//! Present installed script commands with the shell's existing menu and keymap registries.

use bae_app::{AppBuilder, Owner, Registry};
use bae_plugin::scripts::{ScriptBindings, ScriptEntry};
use bae_ui::{Binding, Chord, KeymapBindings, MenuItemSpec};
use std::sync::Arc;

#[derive(Debug)]
struct Bindings {
    menu: Arc<Registry<MenuItemSpec>>,
    keys: Arc<Registry<KeymapBindings>>,
}

pub(super) fn install(app: &mut AppBuilder) {
    let bindings = Bindings {
        menu: app.registry(),
        keys: app.registry(),
    };
    app.insert_resource::<Arc<dyn ScriptBindings>>(Arc::new(bindings));
}

impl ScriptBindings for Bindings {
    fn update(&self, id: &str, entries: &[ScriptEntry]) {
        let owner = Owner::plugin(id);
        self.menu.batch(|tx| {
            tx.unregister_owner(&owner);
            for entry in entries {
                let Some(path) = &entry.menu else { continue };
                let action = &entry.info.id;
                let parts: Vec<_> = path.split('/').collect();
                let mut item = MenuItemSpec::new(&parts, &entry.info.title, action);
                item.title_zh = entry.name_zh.clone();
                if let Err(error) = tx.register(owner.clone(), action.as_str(), item) {
                    tracing::warn!("{error}");
                }
            }
        });
        self.keys.batch(|tx| {
            tx.unregister_owner(&owner);
            for entry in entries {
                let action = &entry.info.id;
                for (workspace, shortcut) in &entry.shortcuts {
                    let chord = match Chord::parse(shortcut) {
                        Ok(chord) => chord,
                        Err(error) => {
                            tracing::warn!("{action}: {error}");
                            continue;
                        }
                    };
                    let binding = Binding {
                        chord,
                        action: action.clone(),
                        params: serde_json::Value::Null,
                        when: None,
                    };
                    let bindings = KeymapBindings::new(bae_ui::presets::AE_KEYMAP, vec![binding])
                        .for_workspace(workspace);
                    if let Err(error) =
                        tx.register(owner.clone(), format!("{action}.{workspace}"), bindings)
                    {
                        tracing::warn!("{error}");
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use bae_plugin::{DirSource, PluginModule, PluginRuntime, Scope};

    #[test]
    fn installed_grid_actions_follow_the_plugin_lifetime_in_an_open_document() {
        let mut builder = AppBuilder::new();
        install(&mut builder);
        builder.add_module(PluginModule);
        let app = builder.build();
        let mut document = Document::new(None);
        document.add_module_commands(&app);
        let runtime = app.resource::<PluginRuntime>().unwrap();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../plugins/examples/align-to-grid");
        let id = runtime.load(&DirSource::new(dir), Scope::User).unwrap();
        let menu = app.get_registry::<MenuItemSpec>().unwrap();
        let keys = app.get_registry::<KeymapBindings>().unwrap();
        let command = "plugin.com.example.align-to-grid.align-to-grid";
        let automation = "script.com.example.align-to-grid.report-off-grid";
        let item = menu.get(command).unwrap();
        assert_eq!(item.path, ["Layer", "Transform"]);
        assert_eq!(item.title_zh.as_deref(), Some("将图层对齐到网格"));
        assert_eq!(menu.get(automation).unwrap().path, ["File", "Scripts"]);
        let key = format!("{command}.motion");
        assert_eq!(keys.get(&key).unwrap().bindings[0].action, command);
        assert!(
            document
                .command_titles()
                .iter()
                .any(|(id, _)| id == command)
        );
        runtime.unload(&id).unwrap();
        assert!(menu.get(automation).is_none() && keys.get(&key).is_none());
        assert!(
            !document
                .command_titles()
                .iter()
                .any(|(id, _)| id == command)
        );
    }
}
