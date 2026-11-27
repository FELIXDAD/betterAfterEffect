use super::{Desktop, Result};
use bae_script::{ScriptWindow, ScriptWindows};
use bae_ui::files::{OpenRequest, SharedFilePicker};
use bae_ui::{ActionHandler, Invocation, MenuItemSpec, PanelContext, tr};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const CHOOSE: &str = "motion.file.choose_script_directory";
const APPLY: &str = "motion.file.jsx_application";
enum Phase {
    Issued,
    Save(Option<PathBuf>),
    Rendering(Vec<u64>),
    Ready(Result<Value>),
}
#[derive(Default, Clone)]
pub struct Requests(Arc<Mutex<BTreeMap<u64, Phase>>>);

impl Requests {
    pub fn take_save(&self, id: u64) -> Option<Option<PathBuf>> {
        let mut map = self.0.lock().unwrap();
        if matches!(map.get(&id), Some(Phase::Save(_)))
            && let Some(Phase::Save(path)) = map.insert(id, Phase::Issued)
        {
            return Some(path);
        }
        None
    }
    pub fn finish(&self, id: u64, result: Result<Value>) {
        self.0.lock().unwrap().insert(id, Phase::Ready(result));
    }
    pub(crate) fn draw(&self, cx: &mut PanelContext<'_>, w: &ScriptWindow) {
        use std::collections::btree_map::Entry;
        let mut pending = self.0.lock().unwrap();
        match pending.entry(w.id) {
            Entry::Vacant(slot) => {
                slot.insert(Phase::Issued);
                cx.invoke_with(APPLY, json!({"id":w.id}));
            }
            Entry::Occupied(slot) if matches!(slot.get(), Phase::Ready(_)) => {
                let Phase::Ready(result) = slot.remove() else {
                    unreachable!()
                };
                let reply = match result {
                    Ok(value) => json!({"value":value}),
                    Err(error) => json!({"error":error}),
                };
                cx.invoke_with(bae_script::RUN_SCRIPT, json!({"window":w.id,"event":reply}));
                cx.invoke(crate::scripts::REPORT);
            }
            Entry::Occupied(mut slot) if matches!(slot.get(), Phase::Rendering(_)) => {
                if let Some(queue) = cx.resources.get::<crate::SharedRenderQueue>() {
                    let queue = queue.lock();
                    if !queue.is_running() {
                        let Phase::Rendering(batch) = slot.get() else {
                            unreachable!()
                        };
                        let error = queue
                            .items()
                            .iter()
                            .filter(|i| batch.contains(&i.id))
                            .find_map(|i| match &i.status {
                                crate::render_queue::ItemStatus::Failed(e) => Some(e.clone()),
                                crate::render_queue::ItemStatus::Stopped { .. } => {
                                    Some("render stopped".into())
                                }
                                _ => None,
                            });
                        slot.insert(Phase::Ready(error.map_or(Ok(Value::Null), Err)));
                    }
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn install(app: &mut bae_app::AppBuilder, desktop: Desktop) {
    let reqs = Requests::default();
    app.insert_resource(reqs.clone());
    let actions: Arc<dyn ActionHandler> = Arc::new(Actions { desktop, reqs });
    let _ = app.register_arc("bae.script.application", actions);
    let item = MenuItemSpec::new(&["File", "Scripts"], "Choose Script Folder...", CHOOSE)
        .workspace(crate::menus::WORKSPACE)
        .at(vec![
            bae_ui::menu::MenuPos::new(i32::MAX, i32::MAX),
            bae_ui::menu::MenuPos::new(0, 4),
        ]);
    let _ = app.register(CHOOSE, item);
    bae_ui::i18n::register(&[("Choose Script Folder...", "选择脚本可访问的目录...")]);
}

struct Actions {
    desktop: Desktop,
    reqs: Requests,
}
impl Actions {
    fn choose(&self, cx: &PanelContext<'_>, title: Option<&str>) -> Result<Value> {
        let title = title.unwrap_or(tr("Choose Script Folder..."));
        let picker = cx
            .resources
            .get::<SharedFilePicker>()
            .ok_or("file picker unavailable")?;
        let Some(path) = picker.0.pick_folder(title) else {
            return Ok(Value::Null);
        };
        self.desktop.select_folder(&path)?;
        Ok(json!(path))
    }

    fn perform(&self, cx: &mut PanelContext<'_>, w: &ScriptWindow) -> Result<Option<Value>> {
        let args = &w.tree["args"];
        match w.tree["op"].as_str() {
            Some("folder") => self.choose(cx, args["title"].as_str()).map(Some),
            Some("pickFile") => {
                let request = OpenRequest {
                    title: args["title"].as_str().unwrap_or(tr("Open")).into(),
                    ..Default::default()
                };
                let Some(path) = SharedFilePicker::open(cx.resources, &request)
                    .into_iter()
                    .next()
                else {
                    return Ok(Some(Value::Null));
                };
                let grant = self.desktop.0.grant.lock().unwrap();
                Ok(Some(json!(
                    grant
                        .as_ref()
                        .ok_or("choose a script folder first")?
                        .external(&path, false)?
                )))
            }
            Some("menu") => {
                let id = args["id"].as_u64().ok_or("expected menu command id")?;
                let item = super::menu_item(&|i| u64::from(super::script_id(i.ae)) == id)
                    .ok_or("unknown menu command")?;
                let params = if item.params.is_empty() {
                    Value::Null
                } else {
                    serde_json::from_str(item.params).map_err(|e| e.to_string())?
                };
                cx.invoke_with(item.action, params);
                Ok(Some(Value::Null))
            }
            Some("save") => {
                let path = args["path"]
                    .as_str()
                    .map(|s| self.desktop.output(Path::new(s)))
                    .transpose()?;
                if let Some(p) = &path
                    && !p.extension().and_then(|x| x.to_str()).is_some_and(|s| {
                        s.eq_ignore_ascii_case("bae") || s.eq_ignore_ascii_case("baez")
                    })
                {
                    return Err("save a BAE project with a .bae or .baez extension".into());
                }
                self.reqs.0.lock().unwrap().insert(w.id, Phase::Save(path));
                cx.invoke_with(bae_ui::action::ids::SAVE, json!({"script_window":w.id}));
                Ok(None)
            }
            Some("render") => {
                let queue = &self.desktop.0.queue;
                let mut batch = Vec::new();
                {
                    let queue = queue.lock();
                    if queue.is_running() {
                        return Err("render queue is running".into());
                    }
                    for item in queue
                        .items()
                        .iter()
                        .filter(|i| i.render && i.status == crate::render_queue::ItemStatus::Queued)
                    {
                        for module in &item.outputs {
                            self.desktop.output(Path::new(&module.output))?;
                        }
                        batch.push(item.id);
                    }
                }
                if batch.is_empty() {
                    return Ok(Some(Value::Null));
                }
                crate::render_queue::start(queue, cx.stage, cx.resources)?;
                self.reqs
                    .0
                    .lock()
                    .unwrap()
                    .insert(w.id, Phase::Rendering(batch));
                Ok(None)
            }
            _ => Err("unknown application request".into()),
        }
    }
}

impl ActionHandler for Actions {
    fn actions(&self) -> Vec<String> {
        vec![CHOOSE.into(), APPLY.into()]
    }
    fn workspace(&self) -> Option<&str> {
        Some(crate::menus::WORKSPACE)
    }
    fn handle(&self, inv: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        if inv.id == CHOOSE {
            if let Err(e) = self.choose(cx, None) {
                crate::dialogs::alert(cx, tr("Choose Script Folder..."), e);
            }
            return true;
        }
        if inv.id != APPLY {
            return false;
        }
        let Some(id) = inv.params["id"].as_u64() else {
            return true;
        };
        let Some(w) = cx.resources.get::<ScriptWindows>().and_then(|s| {
            s.windows()
                .into_iter()
                .find(|w| w.id == id && w.tree["type"] == "application")
        }) else {
            return true;
        };
        match self.perform(cx, &w) {
            Ok(None) => {}
            Ok(Some(value)) => self.reqs.finish(id, Ok(value)),
            Err(e) => self.reqs.finish(id, Err(e)),
        }
        true
    }
}
