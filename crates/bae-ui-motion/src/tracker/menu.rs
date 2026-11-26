use super::*;

pub(super) struct DisplayOptions {
    pub magnify: bool,
    pub paths: bool,
}
impl Default for DisplayOptions {
    fn default() -> Self {
        Self {
            magnify: true,
            paths: true,
        }
    }
}

pub(super) struct SourceDialog {
    target: (PrimPath, String),
    source: Option<PrimPath>,
}
impl SourceDialog {
    pub(super) fn show(&mut self, ctx: &egui::Context, cx: &mut PanelContext<'_>) -> bool {
        let Some(comp) = query::owning_comp(cx.stage, &self.target.0) else {
            return false;
        };
        let layers: Vec<_> = source_layers(cx.stage, &comp)
            .into_iter()
            .filter(|p| !query::is_locked(cx.stage, p))
            .collect();
        if !model::trackable_property(cx.stage, &self.target.0, &self.target.1) || layers.is_empty()
        {
            return false;
        }
        if self.source.as_ref().is_none_or(|p| !layers.contains(p)) {
            self.source = layers
                .iter()
                .find(|p| Some(*p) != query::owning_layer(cx.stage, &self.target.0).as_ref())
                .or(layers.first())
                .cloned();
        }
        let mut keep = true;
        let modal = egui::Modal::new(egui::Id::new("tracker_property_source")).show(ctx, |ui| {
            ui.heading(tr("Motion Source"));
            choose_path(ui, cx.stage, "Motion Source:", &mut self.source, &layers);
            ui.horizontal(|ui| {
                if ui.button(tr("Cancel")).clicked() {
                    keep = false
                }
                if ui.button(tr("OK")).clicked()
                    && let Some(source) = &self.source
                {
                    create_for(cx, source, false, Some(self.target.clone()));
                    keep = false;
                }
            });
        });
        keep && !modal.should_close()
    }
}

const TRACK: &str = "Track This Property";
const REVEAL: &str = "Reveal Current Track in Timeline";
const MAGNIFY: &str = "Magnify Feature When Dragging";
const ADD: &str = "New Track Point";
const PATHS: &str = "Display Motion Paths";

fn entries(
    has_property: bool,
    has_track: bool,
    editable: bool,
    busy: bool,
    display: &DisplayOptions,
) -> [(&'static str, bool, Option<bool>); 7] {
    [
        (TRACK, has_property && !busy, None),
        (REVEAL, has_track, None),
        (MAGNIFY, true, Some(display.magnify)),
        ("", false, None),
        (ADD, has_track && editable && !busy, None),
        ("", false, None),
        (PATHS, true, Some(display.paths)),
    ]
}
pub(super) fn show(ui: &mut Ui, cx: &mut PanelContext<'_>) {
    let session = common::session(cx.resources);
    let property = {
        let s = session.lock();
        match s.selected_props.as_slice() {
            [(p, a)]
                if model::trackable_property(cx.stage, p, a)
                    && query::locked_layer_of(cx.stage, p).is_none() =>
            {
                Some((p.clone(), a.clone()))
            }
            _ => None,
        }
    };
    let shared = shared(cx.resources);
    let mut s = shared.lock().unwrap();
    let current = s.current.clone().filter(|t| {
        cx.stage.has_prim(t) && query::owning_comp(cx.stage, t) == session.lock().active_comp
    });
    let editable = current
        .as_ref()
        .is_some_and(|p| query::locked_layer_of(cx.stage, p).is_none());
    for (id, enabled, checked) in entries(
        property.is_some(),
        current.is_some(),
        editable,
        s.job.is_some(),
        &s.display,
    ) {
        if id.is_empty() {
            ui.separator();
            continue;
        }
        let clicked = if let Some(mut value) = checked {
            ui.add_enabled(enabled, egui::Checkbox::new(&mut value, tr(id)))
                .clicked()
        } else {
            ui.add_enabled(enabled, egui::Button::new(tr(id))).clicked()
        };
        if !clicked {
            continue;
        }
        match id {
            MAGNIFY => s.display.magnify = !s.display.magnify,
            PATHS => s.display.paths = !s.display.paths,
            ADD => {
                if let Some(t) = &current {
                    cx.invoke_with("tracker.add_point", json!({"tracker":t}));
                }
            }
            REVEAL => {
                if let Some(track) = &current
                    && let Some(layer) = query::owning_layer(cx.stage, track)
                {
                    let mut sess = session.lock();
                    sess.selected_layers = vec![layer.clone()];
                    sess.selected_keys.clear();
                    sess.selected_props.clear();
                    sess.reveal.remove(&layer);
                    sess.graph_editor = false;
                    sess.selected_groups = vec![track.to_string()];
                    sess.reveal_tracker = Some(track.clone());
                    cx.open_panel("timeline");
                }
            }
            TRACK => {
                if let Some(target) = &property {
                    s.choose_source = Some(SourceDialog {
                        target: target.clone(),
                        source: s.source.clone(),
                    });
                }
            }
            _ => unreachable!(),
        }
        ui.close();
    }
}
pub(super) const ZH: &[(&str, &str)] = &[
    (TRACK, "跟踪此属性"),
    (REVEAL, "在时间轴中显示当前跟踪"),
    (MAGNIFY, "在拖动时放大功能"),
    (ADD, "新建跟踪点"),
    (PATHS, "显示运动路径"),
    ("Motion Source", "运动源"),
];
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn menu_matches_the_empty_and_active_tracker() {
        let display = DisplayOptions::default();
        let labels = |row: (&str, bool, Option<bool>)| {
            (
                ZH.iter().find(|p| p.0 == row.0).map_or("", |p| p.1),
                row.1,
                row.2,
            )
        };
        assert_eq!(
            entries(false, false, false, false, &display).map(labels),
            [
                ("跟踪此属性", false, None),
                ("在时间轴中显示当前跟踪", false, None),
                ("在拖动时放大功能", true, Some(true)),
                ("", false, None),
                ("新建跟踪点", false, None),
                ("", false, None),
                ("显示运动路径", true, Some(true))
            ]
        );
        let active = entries(true, true, true, false, &display);
        assert!(active[0].1 && active[1].1 && active[4].1);
        assert!(!entries(true, true, true, true, &display)[4].1);
    }
}
