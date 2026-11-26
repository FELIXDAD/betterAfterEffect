//! The Timeline's search field (AE: the magnifier field next to the current
//! time; Edit ▸ Find focuses it). While it has text the outline shows the
//! layers whose name matches and, opened down to them, the properties whose
//! name matches; the other layers are hidden.

use super::*;

/// `nodes` cut down to the properties (or whole groups) whose name contains
/// `needle` (lowercase), keeping the groups on the way to them.
pub(super) fn matching(nodes: &[Node], needle: &str) -> Vec<Node> {
    let hit = |label: &str| label.to_lowercase().contains(needle);
    nodes
        .iter()
        .filter_map(|n| match n {
            Node::Group {
                id,
                label,
                children,
            } => {
                if hit(label) {
                    return Some(n.clone());
                }
                let children = matching(children, needle);
                (!children.is_empty()).then(|| Node::Group {
                    id: id.clone(),
                    label: label.clone(),
                    children,
                })
            }
            Node::Prop { label, .. } => hit(label).then(|| n.clone()),
        })
        .collect()
}

/// Every group id in `nodes` (all shown open).
pub(super) fn group_ids(nodes: &[Node], out: &mut HashSet<String>) {
    for n in nodes {
        if let Node::Group { id, children, .. } = n {
            out.insert(id.clone());
            group_ids(children, out);
        }
    }
}

impl TimelinePanel {
    /// The field in the header row.
    pub(super) fn search_ui(&mut self, ui: &mut Ui, rect: Rect) {
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let resp = widgets::search_field(&mut child, &mut self.search, rect.width());
        if std::mem::take(&mut self.focus_search) {
            resp.request_focus();
        }
    }

    /// The rows while searching: (layer, property attr or group id) for tests.
    #[cfg(test)]
    pub(crate) fn search_rows(
        &mut self,
        stage: &Stage,
        comp: &PrimPath,
        s: &Session,
    ) -> Vec<String> {
        let layers = self.layer_list(stage, comp);
        self.rows(stage, &layers, s)
            .into_iter()
            .map(|r| match r.kind {
                RowKind::Layer => query::display_name_of(stage, &layers.paths[r.layer_index]),
                RowKind::Group { label, .. } => format!("  {label}"),
                RowKind::Prop { attr, .. } => format!("    {attr}"),
                _ => String::new(),
            })
            .collect()
    }
}
