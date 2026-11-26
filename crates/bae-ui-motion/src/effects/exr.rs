//! Custom parameter rows populated from the displayed EXR frame.
use crate::{common::Edit, session::SharedSession};
use bae_effects::exr_controls::{self, CryptoSelection, IdChannel, Routing};
use bae_model::{PrimPath, Time, Value};
use bae_ui::tr;
use egui::{ComboBox, Ui};

pub(super) fn control(
    ui: &mut Ui,
    session: &SharedSession,
    effect: &PrimPath,
    name: &str,
    id: u32,
    value: Option<&Value>,
    time: Time,
) -> Option<Edit> {
    if !matches!(
        (name, id),
        ("Cryptomatte", 1) | ("EXtractoR", 13) | ("IDentifier", 5)
    ) {
        return None;
    }
    let layer = super::layer_of(effect)?;
    let info = session
        .lock()
        .channel_frame
        .as_ref()
        .filter(|frame| frame.time == time)
        .and_then(|f| f.layers.get(&layer))
        .cloned();
    let Some(info) = info else {
        ui.weak(tr("Source channels unavailable"));
        return Some(Edit::None);
    };
    let mut edit = Edit::None;
    ui.push_id(effect.as_str(), |ui| match name {
        "Cryptomatte" => {
            let mut sel: CryptoSelection = exr_controls::read(value);
            let mut changed = false;
            if sel.layer.is_empty()
                && let Some(first) = info.crypto.first()
            {
                sel.layer = first.name.clone();
            }
            ComboBox::from_id_salt("crypto-layer")
                .selected_text(&sel.layer)
                .show_ui(ui, |ui| {
                    for layer in &info.crypto {
                        if ui
                            .selectable_value(&mut sel.layer, layer.name.clone(), &layer.name)
                            .changed()
                        {
                            sel.object = None;
                            changed = true;
                        }
                    }
                });
            let Some(group) = info.crypto.iter().find(|l| l.name == sel.layer) else {
                return;
            };
            let label = sel
                .object
                .map(|id| {
                    group
                        .manifest
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| id.to_string())
                })
                .unwrap_or_else(|| tr("No object selected").to_string());
            ComboBox::from_id_salt("crypto-object")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(&mut sel.object, None, tr("No object selected"))
                        .changed();
                    for (&id, name) in &group.manifest {
                        changed |= ui
                            .selectable_value(&mut sel.object, Some(id), name)
                            .changed();
                    }
                })
                .response
                .on_hover_text(tr(
                    "Enable Selection Mode and click an object in the viewer",
                ));
            if changed {
                edit = Edit::Commit(exr_controls::value(&sel));
            }
        }
        "EXtractoR" => {
            let mut settings: Routing = exr_controls::read(value);
            let mut changed = false;
            ui.vertical(|ui| {
                for (i, dest) in settings.channels.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(["R", "G", "B", "A"][i]);
                        let label = if dest.is_empty() {
                            tr("Copy source").to_string()
                        } else {
                            dest.clone()
                        };
                        ComboBox::from_id_salt(i)
                            .selected_text(label)
                            .show_ui(ui, |ui| {
                                changed |= ui
                                    .selectable_value(dest, String::new(), tr("Copy source"))
                                    .changed();
                                for c in &info.channels {
                                    changed |= ui
                                        .selectable_value(dest, c.name.clone(), &c.name)
                                        .changed();
                                }
                            });
                    });
                }
            });
            if changed {
                edit = Edit::Commit(exr_controls::value(&settings));
            }
        }
        _ => {
            let mut settings: IdChannel = exr_controls::read(value);
            let label = if settings.name.is_empty() {
                "Object ID"
            } else {
                &settings.name
            };
            ComboBox::from_id_salt("id-channel")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for c in &info.channels {
                        if ui
                            .selectable_value(&mut settings.name, c.name.clone(), &c.name)
                            .changed()
                        {
                            edit = Edit::Commit(exr_controls::value(&settings));
                        }
                    }
                });
        }
    });
    Some(edit)
}
