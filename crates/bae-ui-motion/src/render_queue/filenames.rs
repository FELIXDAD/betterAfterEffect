//! File Name and Location Templates for a single Output Module.
use super::*;
use chrono::{Datelike, Local, Timelike};

const SIMPLE: &str = "[compName].[extension]";
/// AE 26's File Name and Location templates in its menu order.
const PRESETS: &[(&str, &str)] = &[
    ("Comp Name", SIMPLE),
    ("Comp Folder and Name", "[compName]/[compName].[extension]"),
    (
        "Comp and Output Module Name",
        "[compName]_[outputModule].[extension]",
    ),
    (
        "Project and Comp Name",
        "[projectName]_[compName].[extension]",
    ),
    (
        "Comp Name and Dimensions",
        "[compName]_[width]x[height].[extension]",
    ),
    (
        "Comp and Frame Range",
        "[compName]_[startFrame]-[endFrame].[extension]",
    ),
    (
        "Comp Name and Date/Time",
        "[compName]_[year]-[month]-[day]_[hour]-[minute]-[second].[extension]",
    ),
    (
        "Comp Name and Aspect Ratio",
        "[compName]_[aspectRatio].[extension]",
    ),
    (
        "Render Next to Project File",
        "[projectFolder]/[compName].[extension]",
    ),
];

const PROPERTIES: &[&str] = &[
    "compName",
    "projectName",
    "projectFolder",
    "outputModule",
    "extension",
    "width",
    "height",
    "aspectRatio",
    "startFrame",
    "endFrame",
    "year",
    "month",
    "day",
    "hour",
    "minute",
    "second",
    "####",
];

pub(super) struct Context<'a> {
    pub comp: &'a CompInfo,
    pub start: i64,
    pub end: i64,
    pub project: Option<&'a Path>,
}

#[derive(Debug, Default, Clone)]
pub(super) struct Naming {
    base: Option<PathBuf>,
    pub(super) pattern: Option<String>,
}

impl Naming {
    pub(super) fn in_comp_folder(base: &Path) -> Self {
        Self {
            base: Some(base.to_owned()),
            pattern: Some("[compName]/[compName].[extension]".into()),
        }
    }
}

impl Context<'_> {
    fn path(&self, module: &QueueOutput, pattern: &str) -> Option<PathBuf> {
        if pattern.trim().is_empty() {
            return None;
        }
        let mut result = String::new();
        let mut rest = pattern;
        let mut clock = None;
        while let Some(i) = rest.find('[') {
            result.push_str(&rest[..i]);
            let (key, tail) = rest[i + 1..].split_once(']')?;
            let part = match key {
                "compName" => sanitize(&self.comp.name),
                "projectName" => sanitize(
                    self.project
                        .and_then(Path::file_stem)
                        .and_then(|s| s.to_str())
                        .unwrap_or(tr("Untitled Project")),
                ),
                "projectFolder" if i == 0 && result.is_empty() => {
                    self.project?.parent()?.to_string_lossy().into_owned()
                }
                "outputModule" => sanitize(module.template.as_deref().unwrap_or_else(|| {
                    tr(om_template(module).unwrap_or_else(|| format_label(module.format)))
                })),
                "extension" => module.format.extension().to_owned(),
                "width" => self.comp.width.to_string(),
                "height" => self.comp.height.to_string(),
                "aspectRatio" => {
                    let (mut a, mut b) = (self.comp.width, self.comp.height);
                    while b != 0 {
                        (a, b) = (b, a % b);
                    }
                    format!(
                        "{}x{}",
                        self.comp.width / a.max(1),
                        self.comp.height / a.max(1)
                    )
                }
                "startFrame" => self.start.to_string(),
                "endFrame" => self.end.to_string(),
                "year" | "month" | "day" | "hour" | "minute" | "second" => {
                    let now = clock.get_or_insert_with(Local::now);
                    match key {
                        "year" => now.year().to_string(),
                        "month" => format!("{:02}", now.month()),
                        "day" => format!("{:02}", now.day()),
                        "hour" => format!("{:02}", now.hour()),
                        "minute" => format!("{:02}", now.minute()),
                        _ => format!("{:02}", now.second()),
                    }
                }
                n if !n.is_empty() && n.bytes().all(|c| c == b'#') => {
                    if module.format.is_image() {
                        format!("[{n}]")
                    } else {
                        String::new()
                    }
                }
                _ => return None,
            };
            result.push_str(&part);
            rest = tail;
        }
        result.push_str(rest);
        if result.ends_with(['/', '\u{5c}']) || result.contains('\0') {
            return None;
        }
        let path = PathBuf::from(result);
        let mut stem = path.file_stem()?.to_string_lossy().into_owned();
        if module.format.is_image() && !stem.contains('#') {
            stem.push_str("_[####]");
        }
        let path = path.with_file_name(format!("{stem}.{}", module.format.extension()));
        let base = module
            .naming
            .base
            .as_deref()
            .or_else(|| Path::new(&module.output).parent())
            .unwrap_or_else(|| Path::new(""));
        Some(base.join(path))
    }
    pub(super) fn apply(&self, module: &mut QueueOutput, pattern: &str) -> bool {
        let Some(path) = self.path(module, pattern) else {
            return false;
        };
        if module.naming.base.is_none() {
            module.naming.base = Path::new(&module.output).parent().map(Path::to_owned);
        }
        module.output = path.to_string_lossy().into_owned();
        module.naming.pattern = Some(pattern.to_owned());
        true
    }
}

#[derive(Debug)]
pub(super) struct Editor {
    item: u64,
    index: usize,
    pattern: String,
    name: String,
    error: Option<String>,
}

impl Editor {
    pub fn show(
        &mut self,
        ui: &egui::Ui,
        cx: &PanelContext<'_>,
        store: &Store,
        queue: &SharedRenderQueue,
    ) -> bool {
        let mut q = queue.lock();
        let Some(item) = q.item_mut(self.item) else {
            return true;
        };
        let Some(comp) = bae_motion::query::comp_info(cx.stage, &item.comp) else {
            return true;
        };
        let project = cx
            .resources
            .get::<bae_ui::files::ProjectFile>()
            .and_then(|p| p.get());
        let context = Context {
            comp: &comp,
            start: item.start,
            end: item.end,
            project: project.as_deref(),
        };
        let Some(module) = item.outputs.get_mut(self.index) else {
            return true;
        };
        let mut close = false;
        let popup = egui::Modal::new(ui.id().with("filename_template")).show(ui.ctx(), |ui| {
            ui.heading(tr("File Name and Location Templates"));
            let library = store.snapshot();
            bae_ui::widgets::menu_button(ui, tr("Preset:"), |ui| {
                for (title, p) in PRESETS.iter().map(|(n, p)| (tr(n), *p)).chain(
                    library
                        .iter()
                        .flat_map(|l| l.names.iter().map(|(n, p)| (n.as_str(), p.as_str()))),
                ) {
                    if ui.radio(self.pattern == p, title).clicked() {
                        self.pattern = p.to_owned();
                        ui.close();
                    }
                }
            });
            let edit = ui
                .horizontal(|ui| {
                    ui.label(tr("Template:"));
                    ui.text_edit_singleline(&mut self.pattern)
                })
                .inner;
            bae_ui::widgets::menu_button(ui, tr("Add Property..."), |ui| {
                for property in PROPERTIES {
                    if ui.button(format!("[{property}]")).clicked() {
                        let selected = egui::TextEdit::load_state(ui.ctx(), edit.id)
                            .and_then(|s| s.cursor.char_range());
                        let idx = |n: usize| {
                            self.pattern
                                .char_indices()
                                .nth(n)
                                .map_or(self.pattern.len(), |(i, _)| i)
                        };
                        let (a, b) =
                            selected.map_or((self.pattern.len(), self.pattern.len()), |r| {
                                let [a, b] = r.sorted_cursors();
                                (idx(a.index.into()), idx(b.index.into()))
                            });
                        self.pattern.replace_range(a..b, &format!("[{property}]"));
                        ui.close();
                    }
                }
            });
            let path = context.path(module, &self.pattern);
            if let Some(path) = &path {
                ui.label(path.display().to_string());
            }
            ui.horizontal(|ui| {
                ui.label(tr("Name:"));
                ui.text_edit_singleline(&mut self.name);
            });
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.horizontal(|ui| {
                if ui.button(tr("Cancel")).clicked() {
                    close = true;
                }
                let name = self.name.trim();
                let valid_name = !name.is_empty()
                    && !PRESETS.iter().any(|(n, _)| {
                        name.eq_ignore_ascii_case(n) || name.eq_ignore_ascii_case(tr(n))
                    })
                    && library
                        .as_ref()
                        .is_some_and(|l| !l.names.keys().any(|key| key.eq_ignore_ascii_case(name)));
                if ui
                    .add_enabled(path.is_some() && valid_name, egui::Button::new(tr("Save")))
                    .clicked()
                {
                    match store.save(name, &Template::Filename(self.pattern.clone())) {
                        Ok(()) => {
                            context.apply(module, &self.pattern);
                            close = true;
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                if ui
                    .add_enabled(path.is_some(), egui::Button::new(tr("OK")))
                    .clicked()
                {
                    context.apply(module, &self.pattern);
                    close = true;
                }
            });
        });
        close || popup.should_close()
    }
}

pub(super) fn menu(
    ui: &mut egui::Ui,
    id: u64,
    index: usize,
    module: &mut QueueOutput,
    context: &Context<'_>,
    library: Option<&Library>,
    open: &mut Option<(u64, QueueLink)>,
) {
    let button = ui.add(egui::Button::new("▾").frame(false).small()).labeled(
        egui::WidgetType::Button,
        tr("File Name and Location Templates"),
    );
    // As AE lays it out: the current template first, a separator, the others
    // in AE's order (user templates after the built-ins), a separator and
    // Custom.... No check marks; all enabled, as in AE.
    egui::Popup::menu(&button).show(|ui| {
        let items: Vec<_> = PRESETS
            .iter()
            .map(|(n, p)| (tr(n), *p))
            .chain(
                library
                    .iter()
                    .flat_map(|l| l.names.iter().map(|(n, p)| (n.as_str(), p.as_str()))),
            )
            .collect();
        let current = module.naming.pattern.as_deref().unwrap_or(SIMPLE);
        let first = items.iter().position(|(_, p)| *p == current);
        let order = first
            .into_iter()
            .chain((0..items.len()).filter(|i| Some(*i) != first));
        for (n, i) in order.enumerate() {
            let (name, pattern) = items[i];
            if ui.button(name).clicked() {
                if context.apply(module, pattern) && ui.input(|i| i.modifiers.command) {
                    *open = Some((id, QueueLink::DefaultNaming(pattern.to_owned())));
                }
                ui.close();
            }
            if n == 0 && first.is_some() {
                ui.separator();
            }
        }
        ui.separator();
        if ui.button(tr("Custom...")).clicked() {
            *open = Some((
                id,
                QueueLink::NameFile(Editor {
                    item: id,
                    index,
                    pattern: module
                        .naming
                        .pattern
                        .clone()
                        .unwrap_or_else(|| SIMPLE.into()),
                    name: String::new(),
                    error: None,
                }),
            ));
            ui.close();
        }
    });
}

/// Composition > Add to Render Queue (`frame` None) or Save Frame As: the
/// new item's output follows Preferences > Output (default name and folder,
/// else "Not yet specified"), then the default file name template.
pub(crate) fn add(cx: &PanelContext<'_>, comp: &CompInfo, frame: Option<i64>) {
    let queue = shared(cx.resources);
    let mut q = queue.lock();
    let dir = default_output_dir();
    let id = match frame {
        Some(frame) => q.add_frame(comp, &dir, frame),
        None => q.add(comp, &dir),
    };
    q.assign_output(
        crate::prefs::current(cx.resources).bool("export.use_default_filename_and_folder"),
    );
    let pattern = cx.resources.get::<Arc<Store>>().and_then(|s| s.snapshot());
    if let Some(p) = pattern.as_ref().and_then(|l| l.default_name.as_ref())
        && let Some(item) = q.item_mut(id)
        && !item.outputs[0].output.is_empty()
    {
        let project = cx
            .resources
            .get::<bae_ui::files::ProjectFile>()
            .and_then(|p| p.get());
        Context {
            comp,
            start: item.start,
            end: item.end,
            project: project.as_deref(),
        }
        .apply(&mut item.outputs[0], p);
    }
}
