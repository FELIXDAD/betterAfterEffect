//! Desktop services for user-run JSX files. Plugin entries have no access to these.
mod actions;
pub use actions::Requests;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bae_model::{PrimPath, Stage};
use cap_std::fs::{Dir, OpenOptions};
use serde_json::{Value, json};

type Result<T> = std::result::Result<T, String>;
type Settings = BTreeMap<String, BTreeMap<String, String>>;

struct Grant {
    root: PathBuf,
    dir: Dir,
}

impl Grant {
    fn relative<'a>(&self, path: &'a Path) -> Result<&'a Path> {
        let path = if path.is_absolute() {
            path.strip_prefix(&self.root)
                .map_err(|_| "path is outside the selected script folder")?
        } else {
            path
        };
        if path.components().any(|c| {
            !matches!(
                c,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        }) {
            return Err("expected a path inside the selected script folder".into());
        }
        Ok(if path.as_os_str().is_empty() {
            Path::new(".")
        } else {
            path
        })
    }

    fn external(&self, path: &Path, new_file: bool) -> Result<PathBuf> {
        let rel = self.relative(path)?;
        let resolved = match self.dir.canonicalize(rel) {
            Ok(path) => path,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && new_file => {
                if self.dir.symlink_metadata(rel).is_ok() {
                    return Err("invalid output link".into());
                }
                let parent = rel
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new("."));
                self.dir
                    .canonicalize(parent)
                    .map_err(|e| e.to_string())?
                    .join(rel.file_name().ok_or("expected a file name")?)
            }
            Err(e) => return Err(e.to_string()),
        };
        Ok(self.root.join(resolved))
    }

    fn file(&self, op: &str, args: &Value) -> Result<Value> {
        let path = Path::new(args["path"].as_str().unwrap_or("."));
        let rel = self.relative(path)?;
        let io = |e: std::io::Error| e.to_string();
        match op {
            "root" => Ok(json!(self.root)),
            "absolute" => Ok(json!(self.root.join(rel))),
            "parent" => Ok(if rel == Path::new(".") {
                Value::Null
            } else {
                json!(self.root.join(rel.parent().unwrap_or(Path::new(""))))
            }),
            "exists" => Ok(json!(if args["folder"] == true {
                self.dir.is_dir(rel)
            } else {
                self.dir.is_file(rel)
            })),
            "mkdir" => self
                .dir
                .create_dir_all(rel)
                .map(|()| json!(true))
                .map_err(io),
            "list" => {
                let mut list = Vec::new();
                for entry in self.dir.read_dir(rel).map_err(io)? {
                    let entry = entry.map_err(io)?;
                    let name = entry.file_name();
                    let p = rel.join(&name);
                    if let Ok(meta) = self.dir.metadata(&p) {
                        list.push((name.to_string_lossy().into_owned(), meta.is_dir()));
                    }
                }
                list.sort();
                Ok(Value::Array(
                    list.into_iter()
                        .map(|(name, folder)| json!({"name":name,"folder":folder}))
                        .collect(),
                ))
            }
            "open" => match args["mode"].as_str() {
                Some("r") => {
                    let file = match self.dir.open(rel) {
                        Ok(f) => f,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            return Ok(Value::Null);
                        }
                        Err(e) => return Err(io(e)),
                    };
                    // Bound memory before the text is copied into QuickJS.
                    const LIMIT: usize = 16 << 20;
                    let mut bytes = Vec::new();
                    file.take(LIMIT as u64 + 1)
                        .read_to_end(&mut bytes)
                        .map_err(io)?;
                    if bytes.len() > LIMIT {
                        return Err("script text file exceeds 16 MiB".into());
                    }
                    let text = std::str::from_utf8(&bytes)
                        .map_err(|e| e.to_string())?
                        .trim_start_matches('\u{feff}');
                    Ok(json!(text))
                }
                Some("w" | "a") => {
                    let mut opts = OpenOptions::new();
                    opts.write(true)
                        .create(true)
                        .truncate(args["mode"] == "w")
                        .append(args["mode"] == "a");
                    self.dir
                        .open_with(rel, &opts)
                        .map(|_| json!(true))
                        .map_err(io)
                }
                _ => Err("invalid file mode".into()),
            },
            "append" => {
                let mut opts = OpenOptions::new();
                opts.append(true);
                let mut f = self.dir.open_with(rel, &opts).map_err(io)?;
                f.write_all(args["text"].as_str().ok_or("expected text")?.as_bytes())
                    .map(|()| json!(true))
                    .map_err(io)
            }
            _ => Err(format!("unknown script file operation {op}")),
        }
    }
}

#[derive(Clone)]
pub struct Desktop(Arc<State>);

struct State {
    grant: Mutex<Option<Grant>>,
    settings: Mutex<Result<Settings>>,
    settings_file: Option<PathBuf>,
    queue: crate::SharedRenderQueue,
    project: bae_ui::files::ProjectFile,
}

pub(crate) fn install(app: &mut bae_app::AppBuilder) {
    let project = app
        .resource::<bae_ui::files::ProjectFile>()
        .cloned()
        .unwrap_or_default();
    app.insert_resource(project.clone());
    let desktop = Desktop::new(
        app.resource::<crate::SharedRenderQueue>()
            .expect("render queue")
            .clone(),
        project,
        app.resource::<bae_platform::AppDirs>()
            .map(|d| d.config_file("jsx-settings.json")),
    );
    app.insert_resource::<Arc<dyn bae_script::ScriptApplication>>(Arc::new(desktop.clone()));
    actions::install(app, desktop);
}

impl Desktop {
    pub(crate) fn new(
        queue: crate::SharedRenderQueue,
        project: bae_ui::files::ProjectFile,
        file: Option<PathBuf>,
    ) -> Self {
        let settings = match file.as_deref().map(std::fs::read) {
            Some(Ok(bytes)) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
            Some(Err(e)) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(Settings::default()),
        };
        Self(Arc::new(State {
            grant: Mutex::default(),
            settings: Mutex::new(settings),
            settings_file: file,
            queue,
            project,
        }))
    }

    pub fn select_folder(&self, path: &Path) -> Result<()> {
        let root = std::path::absolute(path).map_err(|e| e.to_string())?;
        let dir = Dir::open_ambient_dir(&root, cap_std::ambient_authority())
            .map_err(|e| e.to_string())?;
        *self.0.grant.lock().unwrap() = Some(Grant { root, dir });
        Ok(())
    }

    pub fn output(&self, path: &Path) -> Result<PathBuf> {
        self.0
            .grant
            .lock()
            .unwrap()
            .as_ref()
            .ok_or("choose a script folder first")?
            .external(path, true)
    }
    fn setting(&self, op: &str, args: &Value) -> Result<Value> {
        let section = args["section"]
            .as_str()
            .ok_or("expected settings section")?;
        let key = args["key"].as_str().ok_or("expected setting name")?;
        let mut guard = self.0.settings.lock().unwrap();
        let settings = guard.as_mut().map_err(|e| e.to_string())?;
        if op == "setting" {
            return Ok(json!(settings.get(section).and_then(|s| s.get(key))));
        }
        let mut updated = settings.clone();
        updated.entry(section.to_owned()).or_default().insert(
            key.to_owned(),
            args["value"]
                .as_str()
                .ok_or("expected setting text")?
                .to_owned(),
        );
        if let Some(file) = &self.0.settings_file {
            bae_platform::settings::write_atomic(
                file,
                &serde_json::to_vec(&updated).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        }
        *settings = updated;
        Ok(Value::Null)
    }
}

fn script_id(native: u32) -> u32 {
    const IDS: &[(u32, u32)] = include!(concat!(env!("OUT_DIR"), "/script_ids.rs"));
    IDS.binary_search_by_key(&native, |&(key, _)| key)
        .map_or(0, |index| IDS[index].1)
}

pub(crate) fn menu_item(
    check: &impl Fn(&crate::catalog::Item) -> bool,
) -> Option<crate::catalog::Item> {
    fn visit(
        nodes: &[crate::catalog::Node],
        check: &impl Fn(&crate::catalog::Item) -> bool,
    ) -> Option<crate::catalog::Item> {
        for node in nodes {
            match node {
                crate::catalog::Node::Item(i)
                    if script_id(i.ae) != 0 && i.platform.current() && check(i) =>
                {
                    return Some(*i);
                }
                crate::catalog::Node::Submenu(s) if s.platform.current() => {
                    if let Some(i) = visit(s.children, check) {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        None
    }
    crate::catalog::menus()
        .iter()
        .find_map(|(_, nodes)| visit(nodes, check))
        .or_else(|| {
            cfg!(target_os = "macos")
                .then(|| visit(crate::catalog::mac_app_menu(), check))
                .flatten()
        })
}

impl bae_script::ScriptApplication for Desktop {
    fn call(&self, stage: &Stage, op: &str, args: &Value) -> Result<Value> {
        match op {
            "setting" | "saveSetting" => self.setting(op, args),
            "projectFile" => Ok(json!(self.0.project.get())),
            "menuId" => {
                let name = args["name"].as_str().ok_or("expected menu title")?;
                Ok(json!(
                    menu_item(&|i| i.title == name || bae_ui::tr(i.title) == name)
                        .map_or(0, |i| script_id(i.ae))
                ))
            }
            "queueRunning" => Ok(json!(self.0.queue.lock().is_running())),
            "queueItems" => Ok(Value::Array(
                self.0
                    .queue
                    .lock()
                    .items()
                    .iter()
                    .map(|i| {
                        let outputs: Vec<_> =
                            i.outputs.iter().map(|module| &module.output).collect();
                        json!({"id":i.id,"comp":i.comp,"outputs":outputs,"render":i.render})
                    })
                    .collect(),
            )),
            "queueAdd" => {
                let p = PrimPath::new(args["comp"].as_str().ok_or("expected composition")?)
                    .map_err(|e| e.to_string())?;
                let info = bae_motion::query::comp_info(stage, &p).ok_or("expected composition")?;
                let mut queue = self.0.queue.lock();
                if queue.is_running() {
                    return Err("render queue is running".into());
                }
                Ok(json!(
                    queue.add(&info, &crate::render_queue::default_output_dir())
                ))
            }
            "queueSet" | "queueRemove" => {
                let id = args["id"].as_u64().ok_or("expected queue item")?;
                let mut queue = self.0.queue.lock();
                if queue.is_running() {
                    return Err("render queue is running".into());
                }
                if op == "queueRemove" {
                    return queue
                        .remove(id)
                        .then_some(Value::Null)
                        .ok_or_else(|| "invalid queue item".into());
                }
                let item = queue.item_mut(id).ok_or("invalid queue item")?;
                if let Some(path) = args["path"].as_str() {
                    let index: usize = serde_json::from_value(args["module"].clone())
                        .map_err(|_| "invalid output module index")?;
                    let output = item
                        .outputs
                        .get_mut(index)
                        .ok_or("invalid output module index")?;
                    output.output = self.output(Path::new(path))?.to_string_lossy().into_owned();
                }
                if let Some(render) = args["render"].as_bool() {
                    item.render = render;
                }
                Ok(Value::Null)
            }
            "absolute"
                if args["path"]
                    .as_str()
                    .is_some_and(|s| Path::new(s).is_absolute()) =>
            {
                Ok(args["path"].clone())
            }
            _ => {
                let guard = self.0.grant.lock().unwrap();
                let grant = guard.as_ref().ok_or("choose a script folder first")?;
                if op == "import" {
                    let path = grant.external(
                        Path::new(args["path"].as_str().ok_or("expected file")?),
                        false,
                    )?;
                    let file = crate::import::script_footage(&path, args["sequence"] == true)?;
                    Ok(json!(file))
                } else {
                    grant.file(op, args)
                }
            }
        }
    }
}
