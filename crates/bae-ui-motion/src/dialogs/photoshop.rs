use crate::import::ImportDefaults;
use bae_interop_psd::import::Document;
use bae_model::{
    PrimPath,
    psd::{LayerImage, LayerKey},
};
use bae_motion::photoshop::ImportKind;
use bae_ui::{
    PanelContext,
    dialog::{self, Outcome},
    tr,
};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

#[derive(Debug)]
struct Prepared {
    source: Arc<bae_interop_psd::format::PsdFile>,
    document: Document,
}

#[derive(Clone, Debug)]
pub struct PhotoshopDialog {
    path: PathBuf,
    parent: Option<PrimPath>,
    comp: Option<PrimPath>,
    folder: Option<String>,
    defaults: ImportDefaults,
    replace: Option<(PrimPath, Option<LayerImage>, Option<String>, bool)>,
    loaded: Option<Arc<OnceLock<Result<Prepared, String>>>>,
    kind: ImportKind,
    single: bool,
    key: Option<LayerKey>,
    editable: bool,
    ignore: bool,
}
impl PhotoshopDialog {
    pub(crate) fn new(
        path: PathBuf,
        parent: Option<PrimPath>,
        comp: Option<PrimPath>,
        folder: Option<String>,
        defaults: ImportDefaults,
        kind: ImportKind,
    ) -> Self {
        Self {
            path,
            parent,
            comp,
            folder,
            defaults,
            replace: None,
            loaded: None,
            kind,
            single: false,
            key: None,
            editable: true,
            ignore: false,
        }
    }
    pub(crate) fn replace(&mut self, item: PrimPath, view: &bae_model::PrimView<'_>, reload: bool) {
        self.ignore = view
            .value("psd:ignore_styles")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        self.replace = Some((
            item,
            LayerImage::from_asset(view),
            view.value("psd:layer_name")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            reload,
        ));
    }
    pub(crate) fn ui(&mut self, ctx: &egui::Context, cx: &mut PanelContext<'_>) -> bool {
        let slot = self
            .loaded
            .get_or_insert_with(|| {
                let slot = Arc::new(OnceLock::new());
                let result = slot.clone();
                let path = self.path.clone();
                let refresh = self.replace.is_some();
                let redraw = ctx.clone();
                if let Err(e) =
                    std::thread::Builder::new()
                        .name("psd-import".into())
                        .spawn(move || {
                            let read = bae_media::psd::prepare(&path, refresh)
                                .map_err(|e| e.to_string())
                                .and_then(|file| {
                                    let document = bae_interop_psd::import::describe(&file)
                                        .map_err(|e| e.to_string())?;
                                    Ok(Prepared {
                                        source: file,
                                        document,
                                    })
                                });
                            let _ = result.set(read);
                            redraw.request_repaint();
                        })
                {
                    let _ = slot.set(Err(e.to_string()));
                }
                slot
            })
            .clone();
        let id = egui::Id::new("motion.psd.import");
        let Some(result) = slot.get() else {
            return dialog::window(ctx, id, tr("Import Photoshop File"), |ui| {
                ui.spinner();
                ui.label(tr("Reading layers..."));
                dialog::buttons(ui, false)
            }) != Some(Outcome::Cancel)
                && !dialog::closed_by_title_bar(ctx, id);
        };
        let prepared = match result {
            Ok(doc) => doc,
            Err(e) => {
                super::alert(
                    cx,
                    tr("Import Photoshop File"),
                    format!("{}\n{e}", self.path.display()),
                );
                return false;
            }
        };
        let doc = &prepared.document;
        if let Some((item, sel, name, reload)) = &self.replace {
            let mut selected = *sel;
            if let Some(s) = &mut selected {
                let Some(key) = doc.replacement(s.key, name.as_deref()) else {
                    super::alert(
                        cx,
                        tr("Replace Footage File"),
                        format!(
                            "{}\n{}",
                            self.path.display(),
                            tr("Matching layer could not be identified.")
                        ),
                    );
                    return false;
                };
                s.key = key;
            }
            bae_media::psd::keep_for_open(prepared.source.clone());
            self.commit(cx, doc, selected, Some(item), *reload);
            return false;
        }
        let layers = doc.leaves();
        if self.key.is_none() {
            self.key = layers.first().map(|l| l.key);
            if !doc.merged {
                self.single = true;
            }
        }
        let title = self.path.file_name().unwrap_or_default().to_string_lossy();
        let outcome = dialog::window(ctx, id, &title, |ui| {
            egui::ComboBox::from_label(tr("Import Kind"))
                .selected_text(tr(label(self.kind)))
                .show_ui(ui, |ui| {
                    for k in [
                        ImportKind::Footage,
                        ImportKind::Composition,
                        ImportKind::RetainSize,
                    ] {
                        ui.selectable_value(&mut self.kind, k, tr(label(k)));
                    }
                });
            if self.kind == ImportKind::Footage {
                ui.add_enabled_ui(doc.merged, |ui| {
                    ui.radio_value(&mut self.single, false, tr("Merged Layers"));
                });
                ui.add_enabled_ui(!layers.is_empty(), |ui| {
                    ui.radio_value(&mut self.single, true, tr("Choose Layer"));
                });
                if self.single {
                    egui::ComboBox::from_label(tr("Layer"))
                        .selected_text(
                            layers
                                .iter()
                                .find(|l| Some(l.key) == self.key)
                                .map_or("", |l| &l.name),
                        )
                        .show_ui(ui, |ui| {
                            for l in &layers {
                                ui.selectable_value(&mut self.key, Some(l.key), &l.name);
                            }
                        });
                    ui.radio_value(
                        &mut self.ignore,
                        false,
                        tr("Merge Layer Styles into Footage"),
                    );
                    ui.radio_value(&mut self.ignore, true, tr("Ignore Layer Styles"));
                }
            } else {
                ui.label(tr("Layer Styles"));
                ui.radio_value(&mut self.editable, true, tr("Editable Layer Styles"));
                ui.radio_value(
                    &mut self.editable,
                    false,
                    tr("Merge Layer Styles into Footage"),
                );
            }
            dialog::buttons(
                ui,
                if self.kind == ImportKind::Footage && !self.single {
                    doc.merged
                } else {
                    !layers.is_empty()
                },
            )
        });
        if dialog::closed_by_title_bar(ctx, id) || outcome == Some(Outcome::Cancel) {
            return false;
        }
        if outcome == Some(Outcome::Ok) {
            let selection = if self.kind == ImportKind::Footage && self.single {
                self.key.map(|key| LayerImage {
                    key,
                    canvas: false,
                    mask: Default::default(),
                })
            } else {
                None
            };
            bae_media::psd::keep_for_open(prepared.source.clone());
            self.commit(cx, doc, selection, None, false);
            return false;
        }
        true
    }
    fn commit(
        &self,
        cx: &mut PanelContext<'_>,
        doc: &Document,
        selection: Option<LayerImage>,
        replace: Option<&PrimPath>,
        reload: bool,
    ) {
        let prefs = crate::prefs::current(cx.resources);
        let keep = reload || prefs.str("import.footage_interpretation_on_replace") == "remember";
        let folder_label = prefs.int("labels.label_default_folder");
        cx.invoke_with("item.import_photoshop", serde_json::json!({
            "folder_label": folder_label,
            "file": self.path, "document": doc, "kind": self.kind, "selection": selection,
            "parent": self.parent, "comp": self.comp, "folder": self.folder, "replace": replace, "keep_name": reload, "keep_interpretation": keep,
            "editable": self.editable, "ignore_styles": self.kind == ImportKind::Footage && self.ignore,
            "fps": self.defaults.sequence_fps, "duration": self.defaults.still_duration,
            "label": self.defaults.labels.map(|labels| labels[0])
        }));
    }
}
fn label(kind: ImportKind) -> &'static str {
    match kind {
        ImportKind::Footage => "Footage",
        ImportKind::Composition => "Composition",
        ImportKind::RetainSize => "Composition - Retain Layer Sizes",
    }
}
pub(crate) const ZH: &[(&str, &str)] = &[
    ("Import Photoshop File", "导入 Photoshop 文件"),
    ("Reading layers...", "正在读取图层…"),
    ("Import Kind", "导入种类"),
    ("Footage", "素材"),
    ("Composition", "合成"),
    ("Composition - Retain Layer Sizes", "合成 - 保持图层大小"),
    ("Merged Layers", "合并图层"),
    ("Choose Layer", "选择图层"),
    ("Layer Styles", "图层样式"),
    ("Editable Layer Styles", "可编辑的图层样式"),
    ("Merge Layer Styles into Footage", "合并图层样式到素材"),
    ("Ignore Layer Styles", "忽略图层样式"),
    (
        "Matching layer could not be identified.",
        "找不到对应的图层。",
    ),
];
