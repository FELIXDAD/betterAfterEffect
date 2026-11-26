use super::*;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(super) struct Display {
    pub cache: bool,
    pub rectified: bool,
    pub live: bool,
    pub indices: bool,
}

impl Default for Display {
    fn default() -> Self {
        Self {
            cache: true,
            rectified: true,
            live: true,
            indices: false,
        }
    }
}

impl TimelinePanel {
    pub(super) fn panel_menu(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let session = common::session(cx.resources);
        let comp = common::active_comp(cx.stage, &session);
        ui.add_enabled_ui(comp.is_some(), |ui| {
            if ui.button(tr("Composition Settings...")).clicked() {
                cx.invoke(ids::COMP_SETTINGS);
                ui.close();
            }
            if ui.button(tr("Reveal Composition in Project")).clicked()
                && let Some(info) = &comp
            {
                session.lock().selected_items = vec![info.path.clone()];
                cx.open_panel("project");
                ui.close();
            }
        });
        ui.separator();
        self.columns.menu(ui);
        ui.separator();
        ui.checkbox(&mut self.display.cache, tr("Show Cache Indicators"));
        ui.separator();
        ui.checkbox(&mut self.display.rectified, tr("Rectified Audio Waveforms"));
        ui.separator();
        ui.add_enabled_ui(comp.is_some(), |ui| {
            for (attr, label) in [
                ("hide_shy_layers", "Hide Shy Layers"),
                ("frame_blending", "Enable Frame Blending"),
                ("motion_blur", "Enable Motion Blur"),
            ] {
                let mut on = comp.as_ref().is_some_and(|i| bool_attr(cx.stage, &i.path, attr));
                if ui.checkbox(&mut on, tr(label)).changed() && let Some(info) = &comp {
                    cx.invoke_with("attr.set", json!({"path": info.path, "attr": attr, "value": {"type": "bool", "value": on}}));
                }
            }
            ui.checkbox(&mut session.lock().auto_key, tr("Enable Auto-keyframe"));
        });
        ui.separator();
        ui.checkbox(&mut self.display.live, tr("Live Update"));
        ui.separator();
        ui.radio_value(&mut self.display.indices, false, tr("Use Keyframe Icons"));
        ui.radio_value(&mut self.display.indices, true, tr("Use Keyframe Indices"));
        ui.separator();
        ui.add_enabled_ui(comp.is_some(), |ui| {
            if ui.button(tr("Composition Flowchart")).clicked() {
                cx.open_panel("flowchart");
                ui.close();
            }
            if ui.button(tr("Composition Mini-Flowchart")).clicked() {
                cx.invoke(ids::MINI_FLOW);
                ui.close();
            }
        });
    }
}
