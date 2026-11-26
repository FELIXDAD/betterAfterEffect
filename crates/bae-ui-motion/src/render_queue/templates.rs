//! User render and output templates, shared by every Render Queue panel.
use super::*;
use bae_platform::{SettingsFile, VersionedSettings};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::RwLock;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Span {
    WorkArea,
    Composition,
    Custom(Time, Time),
}

impl Span {
    pub fn range(&self, comp: &CompInfo) -> (i64, i64) {
        let rate = comp.frame_rate;
        let last = (comp.frame_count() - 1).max(0);
        let (first, end) = match self {
            Self::WorkArea => (
                rate.time_to_frame(comp.work_area.start),
                rate.time_to_frame(comp.work_area.end) - 1,
            ),
            Self::Composition => (0, last),
            Self::Custom(start, end) => (rate.time_to_frame(*start), rate.time_to_frame(*end) - 1),
        };
        let first = first.clamp(0, last);
        (first, end.clamp(first, last))
    }

    pub fn capture(item: &QueueItem, comp: &CompInfo) -> Self {
        for span in [Self::WorkArea, Self::Composition] {
            if (item.start, item.end) == span.range(comp) {
                return span;
            }
        }
        let rate = comp.frame_rate;
        Self::Custom(
            rate.frame_to_time(item.start),
            rate.frame_to_time(item.end + 1),
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Output {
    format: String,
    alpha: bool,
    mbps: f64,
}

impl Output {
    pub fn capture(module: &QueueOutput) -> Self {
        Self {
            format: module.format.name().into(),
            alpha: module.channels == Channels::RgbAlpha,
            mbps: module.bit_rate_mbps,
        }
    }

    pub fn settings(&self) -> Option<OmSettings> {
        let format = ExportFormat::parse(&self.format)?;
        let channels = if self.alpha && supports_alpha(format) {
            Channels::RgbAlpha
        } else {
            Channels::Rgb
        };
        Some((format, channels, self.mbps))
    }
}

#[derive(Debug)]
pub(super) enum Template {
    Render(Span),
    Output(Output),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Library {
    pub render: BTreeMap<String, Span>,
    pub outputs: BTreeMap<String, Output>,
}

impl VersionedSettings for Library {
    const KIND: &'static str = "render-queue-templates";
    const VERSION: u32 = 1;
}

#[derive(Debug)]
pub(super) struct Store {
    file: Option<PathBuf>,
    library: RwLock<Option<Arc<Library>>>,
}

impl Store {
    fn new(file: Option<PathBuf>) -> Self {
        let library = RwLock::new(file.is_none().then(Arc::default));
        Self { file, library }
    }

    pub fn snapshot(&self) -> Option<Arc<Library>> {
        self.library
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn save(&self, name: &str, value: &Template) -> Result<(), String> {
        let mut current = self.library.write().unwrap_or_else(|e| e.into_inner());
        let mut library = current
            .as_deref()
            .cloned()
            .ok_or_else(|| tr("Please wait.").to_owned())?;
        match value {
            Template::Render(s) => {
                library.render.insert(name.into(), s.clone());
            }
            Template::Output(s) => {
                library.outputs.insert(name.into(), s.clone());
            }
        }
        if let Some(file) = &self.file {
            SettingsFile::new(file)
                .save(&library)
                .map_err(|e| e.to_string())?;
        }
        *current = Some(Arc::new(library));
        Ok(())
    }
}

impl bae_gpu::WarmUp for Store {
    fn name(&self) -> &str {
        "render queue templates and output formats"
    }
    fn run(&self, _: &Arc<GpuContext>) -> Result<(), String> {
        if self.snapshot().is_none()
            && let Some(file) = &self.file
        {
            let saved = SettingsFile::<Library>::new(file).load().value;
            self.library
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .get_or_insert_with(|| Arc::new(saved));
        }
        for format in FORMATS {
            format_available(format);
        }
        Ok(())
    }
}

pub(super) fn register(app: &mut bae_app::AppBuilder) {
    let store = Arc::new(Store::new(
        app.resource::<bae_platform::AppDirs>()
            .map(|d| d.config_file("render-queue-templates.json")),
    ));
    let _ = app.register_arc::<dyn bae_gpu::WarmUp>("bae.render/queue-templates", store.clone());
    app.insert_resource(store);
}

#[derive(Debug)]
pub(super) struct NameDialog {
    pub value: Template,
    pub name: String,
    pub saved: bool,
    error: Option<String>,
}

impl NameDialog {
    pub fn new(value: Template) -> Self {
        Self {
            value,
            name: String::new(),
            saved: false,
            error: None,
        }
    }

    pub fn show(&mut self, ui: &egui::Ui, store: &Store) -> bool {
        let mut close = false;
        let modal = egui::Modal::new(ui.id().with("make_template")).show(ui.ctx(), |ui| {
            ui.heading(tr("Make Template..."));
            ui.horizontal(|ui| {
                ui.label(tr("Name:"));
                ui.text_edit_singleline(&mut self.name);
            });
            let name = self.name.trim();
            let available = !name.is_empty()
                && store.snapshot().is_some_and(|library| {
                    let taken = |s: &str| name.eq_ignore_ascii_case(s);
                    let reserved = |s: &str| taken(s) || taken(tr(s));
                    match &self.value {
                        Template::Render(_) => {
                            !RS_TEMPLATES.iter().any(|s| reserved(s))
                                && !library.render.keys().any(|s| taken(s))
                        }
                        Template::Output(_) => {
                            !OM_TEMPLATES.iter().any(|(s, _)| reserved(s))
                                && !library.outputs.keys().any(|s| taken(s))
                        }
                    }
                });
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.horizontal(|ui| {
                close = ui.button(tr("Cancel")).clicked();
                if ui
                    .add_enabled(available, egui::Button::new(tr("OK")))
                    .clicked()
                    || available && ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    match store.save(name, &self.value) {
                        Ok(()) => {
                            self.saved = true;
                            close = true;
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
            });
        });
        close || modal.should_close()
    }
}
