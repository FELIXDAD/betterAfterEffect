//! File ▸ Scripts: Run Script File…, Install Script File… and the installed
//! scripts (AE's Scripts folder, here `<app data>/Scripts`, listed at startup).
//!
//! The script runs with the `script.run` command (`bae-script`) as one undo
//! step, with the active comp, the selected layers and the current time. Its
//! `alert()` messages and any error are shown afterwards in one alert: the
//! handler queues `motion.file.script_report` right after `script.run`, and that
//! reads the report `script.run` left in [`ScriptConsole`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bae_app::{Owner, Registry};
use bae_script::{RUN_SCRIPT, ScriptConsole};
use bae_ui::files::{OpenRequest, SharedFilePicker};
use bae_ui::menu::{MenuItemSpec, MenuPos};
use bae_ui::{ActionHandler, Invocation, PanelContext, tr};
use serde_json::json;

use crate::common;

impl bae_script::ScriptView for crate::SharedSession {
    fn time(&self, comp: &bae_model::PrimPath) -> bae_model::Time {
        self.lock().time(comp)
    }

    fn set_time(&self, comp: &bae_motion::query::CompInfo, time: bae_model::Time) {
        self.lock().set_time(comp, time);
    }

    fn selection(&self) -> bae_script::ScriptSelection {
        let session = self.lock();
        bae_script::ScriptSelection {
            comp: session.active_comp.clone(),
            layers: session.selected_layers.clone(),
            time: session.active_time(),
        }
    }
}

/// File ▸ Scripts ▸ Run Script File… (catalog action).
pub const RUN_SCRIPT_FILE: &str = "motion.file.run_script_file";
/// File ▸ Scripts ▸ Install Script File… (catalog action).
pub const INSTALL_SCRIPT_FILE: &str = "motion.file.install_script_file";
/// An installed script, `{"path": …}` (catalog action of the Scripts list).
pub const RUN_INSTALLED: &str = "motion.file.run_script";
/// Shows the report of the script that just ran.
pub(super) const REPORT: &str = "motion.file.script_report";

pub(crate) const ZH: &[(&str, &str)] = &[
    ("Run Script", "运行脚本"),
    ("Scripts", "脚本"),
    ("Script error", "脚本错误"),
    ("The script could not be read:", "无法读取脚本："),
    ("The script could not be installed:", "无法安装脚本："),
];

/// Script files AE runs (`.jsx`) and plain JavaScript.
const EXTENSIONS: &[&str] = &["jsx", "js"];

fn is_script(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Scripts in `dir`, sorted by file name.
pub fn installed_scripts(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_script(p))
        .collect();
    out.sort();
    out
}

/// The menu item of an installed script: below the separator of File ▸
/// Scripts, where the catalog keeps AE's run-time list (group 1 of that
/// submenu; the File-level position never moves the submenu).
fn menu_item(path: &Path, order: i32) -> (String, MenuItemSpec) {
    let title = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let spec = MenuItemSpec::new(&["File", "Scripts"], title.clone(), RUN_INSTALLED)
        .params(json!({ "path": path }))
        .workspace(crate::menus::WORKSPACE)
        .at(vec![
            MenuPos::new(i32::MAX, i32::MAX),
            MenuPos::new(1, 1000 + order),
        ]);
    (format!("motion.file.script.{title}"), spec)
}

/// Registers the installed scripts of `dir` under File ▸ Scripts.
pub fn register_menu(menu: &Registry<MenuItemSpec>, owner: &Owner, dir: &Path) {
    menu.batch(|tx| {
        for (i, path) in installed_scripts(dir).iter().enumerate() {
            let (id, spec) = menu_item(path, i as i32);
            if !tx.contains(&id) {
                let _ = tx.register(owner.clone(), id, spec);
            }
        }
    });
}

/// The File ▸ Scripts actions.
pub struct ScriptActions {
    /// `<app data>/Scripts`, when known.
    pub dir: Option<PathBuf>,
    /// For scripts installed while the app runs.
    pub menu: Arc<Registry<MenuItemSpec>>,
    pub owner: Owner,
}

impl ScriptActions {
    fn run_file(&self, cx: &mut PanelContext<'_>, path: &Path) {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                let message = format!(
                    "{}\n{}\n{e}",
                    tr("The script could not be read:"),
                    path.display()
                );
                crate::dialogs::alert(cx, tr("Run Script"), message);
                return;
            }
        };
        let session = common::session(cx.resources);
        let params = {
            let s = session.lock();
            json!({
                "source": source,
                "interactive": true,
                "name": path.file_name().map(|n| n.to_string_lossy().into_owned()),
                "active_comp": s.active_comp,
                "selected_layers": s.selected_layers,
                "time": s.active_time().as_seconds(),
            })
        };
        if let Some(console) = cx.resources.get::<ScriptConsole>() {
            console.take();
        }
        cx.invoke_with(RUN_SCRIPT, params);
        cx.invoke(REPORT);
    }

    fn install(&self, cx: &PanelContext<'_>, file: &Path) {
        let Some(dir) = &self.dir else { return };
        let target = dir.join(file.file_name().unwrap_or_default());
        let copied = std::fs::create_dir_all(dir).and_then(|()| std::fs::copy(file, &target));
        match copied {
            Ok(_) => {
                // AE needs a restart; the list here updates at once.
                let order = installed_scripts(dir)
                    .iter()
                    .position(|p| *p == target)
                    .unwrap_or(0);
                let (id, spec) = menu_item(&target, order as i32);
                let _ = self
                    .menu
                    .replace(self.owner.clone(), id.as_str(), Arc::new(spec));
            }
            Err(e) => {
                let message = format!(
                    "{}\n{}\n{e}",
                    tr("The script could not be installed:"),
                    file.display()
                );
                crate::dialogs::alert(cx, tr("Install Script File..."), message);
            }
        }
    }
}

fn pick(cx: &PanelContext<'_>, title: &str) -> Option<PathBuf> {
    let request = OpenRequest {
        title: tr(title).to_owned(),
        filters: vec![(
            tr("Scripts").to_owned(),
            EXTENSIONS.iter().map(|e| (*e).to_owned()).collect(),
        )],
        multiple: false,
        folders: false,
    };
    SharedFilePicker::open(cx.resources, &request)
        .into_iter()
        .next()
}

impl ActionHandler for ScriptActions {
    fn actions(&self) -> Vec<String> {
        let mut out = vec![
            RUN_SCRIPT_FILE.to_owned(),
            RUN_INSTALLED.to_owned(),
            REPORT.to_owned(),
        ];
        if self.dir.is_some() {
            out.push(INSTALL_SCRIPT_FILE.to_owned());
        }
        out
    }

    fn workspace(&self) -> Option<&str> {
        Some(crate::menus::WORKSPACE)
    }

    fn handle(&self, inv: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        match inv.id.as_str() {
            RUN_SCRIPT_FILE => {
                if let Some(path) = pick(cx, "Run Script File...") {
                    self.run_file(cx, &path);
                }
            }
            RUN_INSTALLED => {
                if let Some(path) = inv.params.get("path").and_then(|p| p.as_str()) {
                    self.run_file(cx, Path::new(path));
                }
            }
            INSTALL_SCRIPT_FILE => {
                if let Some(path) = pick(cx, "Install Script File...") {
                    self.install(cx, &path);
                }
            }
            REPORT => {
                let Some(report) = cx.resources.get::<ScriptConsole>().and_then(|c| c.take())
                else {
                    return true;
                };
                let mut lines = report.alerts;
                if let Some(error) = report.error {
                    lines.push(format!("{}: {error}", tr("Script error")));
                }
                if !lines.is_empty() {
                    crate::dialogs::alert(cx, &report.name, lines.join("\n\n"));
                }
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_scripts_sorted_below_the_separator() {
        let dir = std::env::temp_dir().join(format!("bae-scripts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["b.jsx", "a.js", "notes.txt"] {
            std::fs::write(dir.join(name), "1").unwrap();
        }
        let found: Vec<String> = installed_scripts(&dir)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(found, ["a.js", "b.jsx"]);
        let (_, spec) = menu_item(&dir.join("a.js"), 0);
        assert_eq!(spec.path, ["File", "Scripts"]);
        assert_eq!(spec.action, RUN_INSTALLED);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Choosing an installed script runs `script.run` with the session's comp,
    /// selection and time, then shows its alerts and error in one alert.
    #[test]
    fn runs_with_the_session_and_reports_alerts() {
        use bae_app::Resources;
        use bae_model::{AuthorId, EffectDef, Stage};
        use bae_ui::{PanelId, PanelRequests};

        use crate::dialogs::{Dialog, SharedDialogs};
        use crate::session::SharedSession;

        let console = ScriptConsole::default();
        let defs = Arc::new(Registry::<EffectDef>::new());
        let mut reg = bae_script::standard_commands(defs.clone());
        reg.register(bae_script::RunScript::standalone(defs).with_console(console.clone()))
            .unwrap();
        let mut stage = Stage::new();
        let author = AuthorId::new("user:test").unwrap();
        let comp = reg
            .execute(&mut stage, &author, "comp.create", json!({}))
            .unwrap()
            .output["path"]
            .clone();
        let comp = bae_model::PrimPath::new(comp.as_str().unwrap()).unwrap();
        let mut resources = Resources::new();
        resources.insert(console);
        resources.insert(SharedDialogs::default());
        let session = SharedSession::default();
        session.lock().active_comp = Some(comp.clone());
        resources.insert(session);

        let dir = std::env::temp_dir().join(format!("bae-scripts-run-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("hello.jsx");
        std::fs::write(
            &file,
            "alert(app.project.activeItem.name);\napp.project.activeItem.layers.addNull();\nnope();",
        )
        .unwrap();
        let actions = ScriptActions {
            dir: Some(dir.clone()),
            menu: Arc::new(Registry::new()),
            owner: Owner::Core,
        };
        let handle = |stage: &Stage, inv: Invocation| {
            let mut requests = PanelRequests::default();
            let id = PanelId::new("timeline");
            let mut cx =
                PanelContext::new(&id, "Timeline", stage, &author, &resources, &mut requests);
            assert!(actions.handle(&inv, &mut cx));
            requests.invocations
        };
        let invs = handle(
            &stage,
            Invocation::with_params(RUN_INSTALLED, json!({ "path": file })),
        );
        assert_eq!(invs.len(), 2);
        assert_eq!(invs[0].id, RUN_SCRIPT);
        assert_eq!(invs[0].params["active_comp"], json!(comp));
        assert_eq!(invs[1].id, REPORT);
        reg.execute(&mut stage, &author, &invs[0].id, invs[0].params.clone())
            .unwrap();
        assert_eq!(bae_motion::query::comp_layers(&stage, &comp).len(), 1);
        assert!(handle(&stage, invs[1].clone()).is_empty());
        let dialogs = resources.get::<SharedDialogs>().unwrap().lock();
        match &dialogs.open {
            Some(Dialog::Alert { title, message }) => {
                assert_eq!(title, "hello.jsx");
                assert!(message.starts_with("Comp 1\n\nScript error: "), "{message}");
                assert!(message.contains("line 3"), "{message}");
            }
            other => panic!("expected an alert, got {other:?}"),
        }
        drop(dialogs);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
