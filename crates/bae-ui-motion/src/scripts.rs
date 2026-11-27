//! File ▸ Scripts: Run Script File…, Install Script File… and the installed
//! scripts (AE's Scripts folder, here `<app data>/Scripts`, listed at startup).
//!
//! The script runs with the `script.run` command (`bae-script`) as one undo
//! step, with the active comp, the selected layers and the current time. Its
//! `alert()` messages and any error are shown afterwards in one alert: the
//! handler queues `motion.file.script_report` right after `script.run`, and that
//! reads the report `script.run` left in [`ScriptConsole`].

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
const EDITOR: &str = "motion.file.script_editor";
const EDITOR_PANEL: &str = "script_editor";
const RECENT_LIMIT: usize = 10;

pub(crate) const ZH: &[(&str, &str)] = &[
    ("Open Script Editor...", "打开脚本编辑器..."),
    ("Script Editor", "脚本编辑器"),
    ("Script Source", "脚本源码"),
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
    // Use the filename itself whenever it is a valid registry ID. Encoded
    // names contain no extension separator, so they cannot collide with it.
    let id = if title.chars().any(|c| c.is_whitespace() || c.is_control()) {
        title.bytes().map(|b| format!("{b:02x}")).collect()
    } else {
        title
    };
    (format!("motion.file.script.{id}"), spec)
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

#[derive(Default)]
pub(crate) struct RecentScripts {
    entries: Vec<RecentScript>,
    file: Option<PathBuf>,
}

#[derive(Clone)]
struct RecentScript {
    path: PathBuf,
    available: Arc<AtomicBool>,
}

impl RecentScript {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            available: Arc::new(AtomicBool::new(true)),
        }
    }
}

impl RecentScripts {
    pub(crate) fn load(file: Option<PathBuf>) -> Self {
        let mut paths: Vec<PathBuf> = file
            .as_ref()
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default();
        paths.retain(|p| is_script(p));
        paths.truncate(RECENT_LIMIT);
        Self {
            entries: paths.into_iter().map(RecentScript::new).collect(),
            file,
        }
    }

    pub(crate) fn check_files(&self) {
        if self.entries.is_empty() {
            return;
        }
        let entries = self.entries.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("bae-recent-scripts".into())
            .spawn(move || {
                for entry in entries {
                    if !entry.path.is_file() {
                        entry.available.store(false, Ordering::Relaxed);
                        bae_gpu::warm_up::request_redraw();
                    }
                }
            })
        {
            tracing::warn!("checking recent scripts: {error}");
        }
    }

    pub(crate) fn publish(&self, menu: &Registry<MenuItemSpec>, owner: &Owner) {
        menu.batch(|tx| {
            for i in 0..RECENT_LIMIT {
                let id = format!("motion.file.recent_script.{i}");
                if let Some(entry) = self.entries.get(i) {
                    let path = &entry.path;
                    let available = entry.available.clone();
                    let spec = MenuItemSpec::new(
                        &["File", "Scripts", "Recent Script Files"],
                        path.display().to_string(),
                        RUN_INSTALLED,
                    )
                    .params(json!({"path": path}))
                    .enabled_when(move |_| available.load(Ordering::Relaxed))
                    .workspace(crate::menus::WORKSPACE)
                    .at(vec![
                        MenuPos::new(i32::MAX, i32::MAX),
                        MenuPos::new(i32::MAX, i32::MAX),
                        MenuPos::new(0, i as i32),
                    ]);
                    let _ = tx.replace(owner.clone(), id, Arc::new(spec));
                } else {
                    tx.unregister(&id);
                }
            }
        });
    }

    fn record(&mut self, path: &Path) {
        self.entries.retain(|entry| entry.path != path);
        // A successful open supersedes any pending startup check for this path.
        self.entries.insert(0, RecentScript::new(path.to_owned()));
        self.entries.truncate(RECENT_LIMIT);
        let paths: Vec<_> = self.entries.iter().map(|entry| &entry.path).collect();
        if let Some(file) = &self.file
            && let Err(error) = bae_platform::settings::write_atomic(
                file,
                &serde_json::to_vec(&paths).expect("script paths"),
            )
        {
            tracing::warn!("saving recent scripts: {error}");
        }
    }
}

#[derive(Default)]
pub(crate) struct ScriptEditor(String);

impl bae_ui::Panel for ScriptEditor {
    fn ui(&mut self, ui: &mut egui::Ui, cx: &mut PanelContext<'_>) {
        use bae_ui::Accessible as _;
        if ui
            .add_enabled(
                !self.0.trim().is_empty(),
                egui::Button::new(tr("Run Script")),
            )
            .clicked()
        {
            run_source(cx, &self.0, tr("Script Editor"));
        }
        egui::ScrollArea::both().show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.0)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(16),
            )
            .labeled(egui::WidgetType::TextEdit, tr("Script Source"));
        });
    }
    fn save_state(&self) -> Option<serde_json::Value> {
        Some(json!(self.0))
    }
}

pub(crate) fn editor_panel() -> bae_ui::PanelKind {
    bae_ui::PanelKind::new("Script Editor", |spawn| {
        Box::new(ScriptEditor(
            spawn
                .state
                .as_ref()
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .into(),
        ))
    })
    .hidden_from_window_menu()
}

fn run_source(cx: &mut PanelContext<'_>, source: &str, name: &str) {
    let session = common::session(cx.resources);
    let s = session.lock();
    let params = json!({"source": source, "interactive": true, "name": name, "active_comp": s.active_comp,
        "selected_layers": s.selected_layers, "time": s.active_time().as_seconds()});
    drop(s);
    if let Some(console) = cx.resources.get::<ScriptConsole>() {
        console.take();
    }
    cx.invoke_with(RUN_SCRIPT, params);
    cx.invoke(REPORT);
}

/// The File ▸ Scripts actions.
pub struct ScriptActions {
    pub(crate) recent: Mutex<RecentScripts>,
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
        let mut recent = self.recent.lock().unwrap_or_else(|e| e.into_inner());
        recent.record(path);
        recent.publish(&self.menu, &self.owner);
        run_source(
            cx,
            &source,
            &path.file_name().unwrap_or_default().to_string_lossy(),
        );
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
            EDITOR.to_owned(),
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
            EDITOR => cx.open_panel(bae_ui::PanelId::new(EDITOR_PANEL)),
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
        let (id, spec) = menu_item(&dir.join("a.js"), 0);
        assert_eq!(id, "motion.file.script.a.js");
        assert_eq!(spec.path, ["File", "Scripts"]);
        assert_eq!(spec.action, RUN_INSTALLED);
        let odd = dir.join("A B.jsx");
        std::fs::write(&odd, "1").unwrap();
        let menu = Registry::new();
        register_menu(&menu, &Owner::Core, &dir);
        assert!(menu.get(&menu_item(&odd, 0).0).is_some());
        let config = dir.join("recent.json");
        let mut recent = RecentScripts::load(Some(config.clone()));
        recent.record(&odd);
        recent.record(&dir.join("a.js"));
        recent.record(&odd);
        std::fs::remove_file(&odd).unwrap();
        RecentScripts::load(Some(config)).publish(&menu, &Owner::Core);
        let stage = bae_model::Stage::new();
        let resources = bae_app::Resources::new();
        let context = bae_ui::menu::MenuContext::new(&stage, &resources, crate::menus::WORKSPACE);
        assert!(
            menu.get("motion.file.recent_script.0")
                .unwrap()
                .is_enabled(&context)
        );
        assert_eq!(
            menu.get("motion.file.recent_script.0").unwrap().params["path"],
            json!(odd)
        );
        assert!(menu.get("motion.file.recent_script.2").is_none());
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
            recent: Default::default(),
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
