//! Direct composition relationships in the timeline's mini-flowchart.

use super::*;

impl TimelinePanel {
    pub(super) fn open_precomp(
        &mut self,
        cx: &PanelContext<'_>,
        session: &SharedSession,
        layer: &PrimPath,
    ) -> bool {
        let Some(source) = query::precomp_source(cx.stage, layer) else {
            return false;
        };
        let mut s = session.lock();
        if crate::prefs::current(cx.resources)
            .bool("composition.synchronize_time_all_related_items")
            && let Some(info) = query::comp_info(cx.stage, &source)
        {
            let now = s.active_time();
            let source_time = if bool_attr(cx.stage, layer, "time_remap_enabled") {
                self.eval
                    .get(cx.resources)
                    .evaluate_attribute(cx.stage, layer, "time_remap", now)
                    .ok()
                    .and_then(|v| v.as_time().or_else(|| v.as_f64().map(Time::from_seconds)))
            } else {
                None
            };
            s.set_time(
                &info,
                source_time.unwrap_or_else(|| query::layer_timing(cx.stage, layer).layer_time(now)),
            );
        }
        // The Composition Navigator's path runs through the comp it was opened
        // from.
        match query::owning_comp(cx.stage, layer) {
            Some(parent) => s.open_from(&parent, &source),
            None => s.open_comp(&source),
        }
        true
    }
}

#[derive(Debug)]
pub(crate) struct MiniFlow {
    parents: Vec<PrimPath>,
    current: PrimPath,
    children: Vec<PrimPath>,
    anchor: Option<Pos2>,
}

impl MiniFlow {
    pub(crate) fn new(stage: &Stage, current: &PrimPath) -> Self {
        let mut children = Vec::new();
        for source in query::child_comps(stage, current) {
            if !children.contains(&source) {
                children.push(source);
            }
        }
        Self {
            parents: query::comps(stage)
                .into_iter()
                .filter(|comp| query::child_comps(stage, comp).any(|source| source == *current))
                .collect(),
            current: current.clone(),
            children,
            anchor: None,
        }
    }
}

fn nodes(ui: &mut Ui, stage: &Stage, comps: &[PrimPath], target: &mut Option<PrimPath>) {
    ui.vertical(|ui| {
        for comp in comps {
            if ui.button(query::display_name_of(stage, comp)).clicked() {
                *target = Some(comp.clone());
            }
        }
    });
}

pub(super) fn show(ui: &Ui, stage: &Stage, session: &SharedSession) {
    let mut s = session.lock();
    let Some(flow) = &mut s.mini_flow else { return };
    let anchor = *flow.anchor.get_or_insert_with(|| {
        ui.ctx()
            .pointer_hover_pos()
            .unwrap_or(ui.clip_rect().center())
    });
    let mut open = true;
    let mut target = None;
    egui::Popup::new(
        ui.id().with("mini_flow_popup"),
        ui.ctx().clone(),
        anchor,
        ui.layer_id(),
    )
    .open_bool(&mut open)
    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
    .show(|ui| {
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if !flow.parents.is_empty() {
                        nodes(ui, stage, &flow.parents, &mut target);
                        ui.label("→");
                    }
                    ui.strong(query::display_name_of(stage, &flow.current));
                    if !flow.children.is_empty() {
                        ui.label("→");
                        nodes(ui, stage, &flow.children, &mut target);
                    }
                });
            });
    });
    if !open || target.is_some() {
        s.mini_flow = None;
    }
    if let Some(comp) = target {
        s.open_comp(&comp);
    }
}
