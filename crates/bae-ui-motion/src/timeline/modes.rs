//! The Modes column (AE Transfer Controls): blending mode, T (Preserve Underlying
//! Transparency) and Track Matte (AE 2023+: pick whip, matte layer, Alpha / Luma,
//! Inverted). F4 alternates the Switches and Modes columns, Shift+F4 shows or hides
//! Parent & Link. A change on a selected layer applies to every selected layer.

use std::sync::OnceLock;

use super::*;

/// Width of the Modes column.
pub(super) const MODES_W: f32 = 286.0;
const MODE_W: f32 = 96.0;

/// Which column groups the outline shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Panes {
    pub switches: bool,
    pub modes: bool,
    pub parent: bool,
}

impl Default for Panes {
    fn default() -> Self {
        Panes {
            switches: true,
            modes: true,
            parent: true,
        }
    }
}

impl Panes {
    /// F4: with both shown, hide Modes; then alternate Switches ↔ Modes.
    pub(crate) fn toggle_switches_modes(&mut self) {
        if self.switches && self.modes {
            self.modes = false;
        } else {
            self.switches = !self.switches;
            self.modes = !self.switches;
        }
    }
}

/// The blending modes in AE's menu order with their English names (the Layer ▸
/// Blending Mode items of the command catalog).
fn blend_modes() -> &'static [(&'static str, String)] {
    static MODES: OnceLock<Vec<(&'static str, String)>> = OnceLock::new();
    MODES.get_or_init(|| {
        let mut out = Vec::new();
        if let Some(crate::catalog::Node::Submenu(sub)) =
            menus::catalog_entry("Layer", "Blending Mode")
        {
            for n in sub.children {
                if let crate::catalog::Node::Item(it) = n
                    && let Ok(p) = serde_json::from_str::<serde_json::Value>(it.params)
                    && let Some(m) = p["mode"].as_str()
                {
                    out.push((it.title, m.to_owned()));
                }
            }
        }
        out
    })
}

fn enum_attr(stage: &Stage, layer: &PrimPath, attr: &str, default: &str) -> String {
    stage
        .prim(layer)
        .and_then(|v| query::static_value(&v, attr))
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| default.to_owned())
}

/// A layer's matte layer: its `track_matte_layer`, else the layer above.
fn matte_source(stage: &Stage, layer: &PrimPath, comp_layers: &[PrimPath]) -> Option<PrimPath> {
    stage
        .prim(layer)
        .and_then(|v| {
            v.value("track_matte_layer")
                .and_then(|v| v.as_prim_ref())
                .cloned()
        })
        .or_else(|| {
            let i = comp_layers.iter().position(|l| l == layer)?;
            i.checked_sub(1).map(|j| comp_layers[j].clone())
        })
}

/// The matte type with Alpha / Luma or Inverted flipped.
pub(crate) fn flip_matte(matte: &str, luma: bool, inverted: bool) -> String {
    let is_luma = matte.starts_with("luma");
    let is_inv = matte.ends_with("inverted");
    let l = if luma { !is_luma } else { is_luma };
    let i = if inverted { !is_inv } else { is_inv };
    format!(
        "{}{}",
        if l { "luma" } else { "alpha" },
        if i { "_inverted" } else { "" }
    )
}

impl TimelinePanel {
    /// The Modes cells of a layer row.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn modes_cells(
        &mut self,
        ui: &mut Ui,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        cols: &Cols,
        rect: Rect,
        layer: &PrimPath,
        comp_layers: &[PrimPath],
        selected: bool,
        layer_ctx: &str,
        base: Id,
    ) {
        let stage = cx.stage;
        let x0 = rect.left() + cols.modes;
        let cy = rect.center().y;
        let targets: Vec<PrimPath> = if selected {
            session
                .lock()
                .selected_layers
                .iter()
                .filter(|l| !query::is_locked(stage, l))
                .cloned()
                .collect()
        } else {
            vec![layer.clone()]
        };
        let locked = query::is_locked(stage, layer);

        // Mode.
        let modes = blend_modes();
        let current = enum_attr(stage, layer, "blend_mode", "normal");
        let names: Vec<&str> = modes.iter().map(|(t, _)| tr(t)).collect();
        let mut sel = modes.iter().position(|(_, m)| *m == current).unwrap_or(0);
        let before = sel;
        let r = Rect::from_min_max(
            pos2(x0 + 2.0, rect.top() + 1.0),
            pos2(x0 + MODE_W, rect.bottom() - 1.0),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(r)
                .id_salt(base.with("mode")),
        );
        child.add_enabled_ui(!locked, |ui| {
            widgets::dropdown(ui, base.with("mode_dd"), &mut sel, &names, r.width()).labeled_value(
                egui::WidgetType::ComboBox,
                &bae_ui::a11y::context(&[layer_ctx, tr("Mode")]),
                names.get(sel).copied().unwrap_or(""),
            );
        });
        if sel != before {
            cx.invoke_with(
                "layer.set_attr",
                json!({"layers": targets, "attr": "blend_mode", "value": {"type": "enum", "value": modes[sel].1}}),
            );
        }

        // T: Preserve Underlying Transparency.
        let t = Rect::from_center_size(pos2(x0 + MODE_W + 4.0 + SW / 2.0, cy), vec2(12.0, 12.0));
        let on = bool_attr(stage, layer, "preserve_transparency");
        let resp = ui
            .interact(t, base.with("t"), Sense::click())
            .labeled_toggle(
                egui::WidgetType::Checkbox,
                &bae_ui::a11y::context(&[layer_ctx, tr("Preserve Underlying Transparency")]),
                on,
            );
        ui.painter().rect_stroke(
            t,
            2.0,
            Stroke::new(1.0, color::ICON_DIM),
            egui::StrokeKind::Inside,
        );
        if on {
            widgets::paint_check(ui.painter(), t.shrink(1.0), color::TEXT_STRONG);
        }
        if resp.clicked() && !locked {
            cx.invoke_with(
                "layer.set_attr",
                json!({"layers": targets, "attr": "preserve_transparency", "value": {"type": "bool", "value": !on}}),
            );
        }

        // Track Matte: pick whip, matte layer, Alpha / Luma, Inverted.
        let tx0 = x0 + MODE_W + SW + 8.0;
        let matte = enum_attr(stage, layer, "track_matte", "none");
        let source = matte_source(stage, layer, comp_layers).filter(|_| matte != "none");
        let whip = Rect::from_center_size(pos2(tx0 + 7.0, cy), vec2(14.0, 14.0));
        icons::paint(ui.painter(), whip, Icon::Pickwhip, color::ICON);
        let wresp = ui
            .interact(whip, base.with("matte_whip"), Sense::drag())
            .labeled(
                egui::WidgetType::Button,
                &bae_ui::a11y::context(&[layer_ctx, tr("Track Matte Pick Whip")]),
            );
        #[cfg(test)]
        self.last_modes.push((layer.clone(), t, whip.center()));
        if wresp.drag_started() && !locked {
            self.drag = Some(Drag::MatteWhip {
                layer: layer.clone(),
                origin: whip.center(),
            });
        }
        let others: Vec<&PrimPath> = comp_layers.iter().filter(|l| *l != layer).collect();
        let mut names: Vec<String> = vec![tr("No Track Matte").to_owned()];
        names.extend(others.iter().map(|p| {
            let i = comp_layers.iter().position(|x| x == *p).unwrap_or(0);
            format!("{}. {}", i + 1, query::display_name_of(stage, p))
        }));
        let items: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut sel = source
            .as_ref()
            .and_then(|s| others.iter().position(|o| *o == s))
            .map_or(0, |i| i + 1);
        let before = sel;
        let toggles_x = x0 + MODES_W - 36.0;
        let dr = Rect::from_min_max(
            pos2(tx0 + 16.0, rect.top() + 1.0),
            pos2(toggles_x - 2.0, rect.bottom() - 1.0),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(dr)
                .id_salt(base.with("matte")),
        );
        child.add_enabled_ui(!locked, |ui| {
            widgets::dropdown(ui, base.with("matte_dd"), &mut sel, &items, dr.width())
                .labeled_value(
                    egui::WidgetType::ComboBox,
                    &bae_ui::a11y::context(&[layer_ctx, tr("Track Matte")]),
                    items.get(sel).copied().unwrap_or(""),
                );
        });
        if sel != before {
            let kind = if matte == "none" {
                "alpha"
            } else {
                matte.as_str()
            };
            let params = match sel {
                0 => json!({"layers": [layer], "matte": "none"}),
                i => json!({"layers": [layer], "matte": kind, "source": others[i - 1]}),
            };
            cx.invoke_with("layer.set_track_matte", params);
        }
        // Alpha / Luma and Inverted toggles (only with a matte).
        let has = matte != "none";
        for (i, (luma, label, text)) in [
            (
                true,
                "Alpha / Luma Matte",
                if matte.starts_with("luma") { "L" } else { "α" },
            ),
            (false, "Invert Matte", ""),
        ]
        .into_iter()
        .enumerate()
        {
            let b = Rect::from_min_size(
                pos2(toggles_x + i as f32 * 17.0, rect.top() + 2.0),
                vec2(16.0, ROW_H - 4.0),
            );
            let on = if luma {
                true
            } else {
                matte.ends_with("inverted")
            };
            let resp = ui
                .interact(b, base.with(("matte_toggle", i)), Sense::click())
                .labeled_toggle(
                    egui::WidgetType::Button,
                    &bae_ui::a11y::context(&[layer_ctx, tr(label)]),
                    has && on,
                );
            let c = if !has {
                color::TEXT_DISABLED
            } else if !luma && !on {
                color::ICON_DIM
            } else {
                color::TEXT_STRONG
            };
            if luma {
                ui.painter()
                    .text(b.center(), Align2::CENTER_CENTER, text, theme::small(), c);
            } else {
                // Inverted: a half-filled square.
                let sq = Rect::from_center_size(b.center(), vec2(9.0, 9.0));
                ui.painter()
                    .rect_stroke(sq, 0.0, Stroke::new(1.0, c), egui::StrokeKind::Inside);
                ui.painter().rect_filled(
                    Rect::from_min_max(sq.min, pos2(sq.center().x, sq.max.y)),
                    0.0,
                    c,
                );
            }
            if resp.clicked() && has && !locked {
                cx.invoke_with(
                    "layer.set_track_matte",
                    json!({"layers": [layer], "matte": flip_matte(&matte, luma, !luma)}),
                );
            }
        }
    }
}
