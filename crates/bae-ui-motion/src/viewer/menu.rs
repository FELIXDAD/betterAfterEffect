//! Right-click menus of the Composition panel (AE 26): on a layer, the layer menu
//! and Select ▸ (the layers under the pointer); elsewhere the composition menu.

use bae_anim::EvaluatedComp;
use bae_model::PrimPath;
use bae_motion::query;
use bae_ui::{PanelContext, tr};
use egui::Response;

use crate::common;
use crate::context_menu::{self, VIEWER_BLANK, VIEWER_LAYER, VIEWER_TEXT};
use crate::render::View;
use crate::session::SharedSession;
use crate::timeline::{invert_layer_selection, select_children};

/// Opens AE's menu on a right-click. `under` keeps the layers that were under the
/// pointer, topmost first, while the menu is open.
#[allow(clippy::too_many_arguments)]
pub(super) fn frame(
    under: &mut Vec<PrimPath>,
    key: Option<&bae_motion::KeyRef>,
    resp: &Response,
    cx: &mut PanelContext<'_>,
    session: &SharedSession,
    comp: &EvaluatedComp,
    view: &View,
    editing: bool,
) {
    if resp.secondary_clicked()
        && key.is_none()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        *under = super::select_tool::layers_at(cx.stage, comp, view, pos).collect();
        // AE selects the layer it opens the menu for.
        if let Some(top) = under.first() {
            let mut s = session.lock();
            if !s.selected_layers.contains(top) {
                s.selected_layers = vec![top.clone()];
                s.selected_keys.clear();
            }
        }
    }
    common::context_menu(resp, &mut |ui| {
        // A keyframe square on a motion path: AE's keyframe menu.
        if let Some(k) = key {
            crate::timeline::key_menu(ui, cx, session, k);
            return;
        }
        let Some(top) = under.first().cloned() else {
            context_menu::show(ui, cx, VIEWER_BLANK);
            return;
        };
        // A text layer with a Type tool: AE's text menu.
        let typing = crate::tools::shared(cx.resources)
            .lock()
            .current()
            .is_type();
        if typing
            && comp
                .layer(&top)
                .is_some_and(|l| l.kind == bae_model::LayerKind::Text)
        {
            // The orientation is fixed while editing (AE's editing menu has no Horizontal / Vertical).
            let off: &[&str] = if editing {
                &["Horizontal", "Vertical"]
            } else {
                &[]
            };
            match context_menu::show_with(ui, cx, VIEWER_TEXT, off) {
                Some("Edit Text...") => {
                    crate::tools::shared(cx.resources).lock().edit_text_request = Some(top.clone());
                }
                Some(direction @ ("Horizontal" | "Vertical")) => cx.invoke_with(
                    crate::actions::ids::TEXT_ORIENTATION,
                    serde_json::json!({ "orientation": direction.to_lowercase() }),
                ),
                Some("Show Text Panels") => {
                    cx.open_panel("character");
                    cx.open_panel("paragraph");
                }
                Some("Reveal Source Text in Timeline") => {
                    let mut s = session.lock();
                    s.selected_layers = vec![top.clone()];
                    s.reveal.insert(
                        top.clone(),
                        crate::session::Reveal::Attrs(vec!["text:source".to_owned()]),
                    );
                }
                _ => {}
            }
            return;
        }
        let chosen = context_menu::show_with(ui, cx, VIEWER_LAYER, &["Rename"]);
        common::submenu(ui, tr("Select"), &mut |ui| {
            for l in under.iter() {
                if ui.button(query::display_name_of(cx.stage, l)).clicked() {
                    let mut s = session.lock();
                    s.selected_layers = vec![l.clone()];
                    s.selected_keys.clear();
                    ui.close();
                }
            }
        });
        match chosen {
            Some("Invert Selection") => invert_layer_selection(cx.stage, session, &top),
            Some("Select Children") => select_children(cx.stage, session),
            _ => {}
        }
    });
}
