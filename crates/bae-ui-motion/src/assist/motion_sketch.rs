//! Window > Motion Sketch: capture speed, smoothing, display options, the work
//! area it records over, and Start Capture (arms the Composition panel: the
//! next drag records the selected layer's position, see `viewer::motion_sketch`).

use bae_ui::theme::color;
use bae_ui::widgets;
use bae_ui::{Invocation, Panel, PanelContext, TabInfo, tr};
use egui::Ui;

use crate::common;
use crate::session::SketchArm;

pub struct MotionSketchPanel {
    speed: f64,
    smoothing: f64,
    wireframe: bool,
    background: bool,
    message: Option<&'static str>,
}

impl Default for MotionSketchPanel {
    fn default() -> Self {
        MotionSketchPanel {
            speed: 100.0,
            smoothing: 1.0,
            wireframe: false,
            background: true,
            message: None,
        }
    }
}

impl Panel for MotionSketchPanel {
    fn tab(&self, _cx: &PanelContext<'_>) -> TabInfo {
        TabInfo::new(tr("Motion Sketch"))
    }

    fn on_action(&mut self, action: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        let session = common::session(cx.resources);
        crate::actions::handle(action, cx, &session)
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let session = common::session(cx.resources);
        ui.horizontal(|ui| {
            ui.label(tr("Capture speed at:"));
            ui.add(egui::DragValue::new(&mut self.speed).range(1.0..=1000.0));
            ui.label("%");
        });
        ui.horizontal(|ui| {
            ui.label(tr("Smoothing:"));
            ui.add(
                egui::DragValue::new(&mut self.smoothing)
                    .range(0.0..=100.0)
                    .speed(0.1),
            );
        });
        ui.horizontal(|ui| {
            ui.label(tr("Show:"));
            widgets::checkbox(ui, &mut self.wireframe, tr("Wireframe"));
            widgets::checkbox(ui, &mut self.background, tr("Background"));
        });
        if let Some(info) = common::active_comp(cx.stage, &session) {
            ui.horizontal(|ui| {
                ui.label(tr("Start:"));
                ui.label(common::timecode(&info, info.work_area.start));
                ui.label(tr("Duration:"));
                ui.label(common::timecode(
                    &info,
                    info.work_area.end - info.work_area.start,
                ));
            });
        }
        let armed = session.lock().motion_sketch.is_some();
        if armed {
            ui.colored_label(color::ACCENT, tr("Click to start capture"));
        } else if let Some(m) = self.message {
            ui.colored_label(color::TEXT_DIM, tr(m));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tr("Start Capture")).clicked() {
                let layer = session.lock().selected_layers.first().cloned();
                match layer {
                    Some(layer) if !crate::common::is_3d(cx.stage, &layer) => {
                        self.message = None;
                        session.lock().motion_sketch = Some(SketchArm {
                            layer,
                            speed: self.speed,
                            smoothing: self.smoothing,
                        });
                    }
                    Some(_) => self.message = Some("Can't motion sketch a 3D layer."),
                    None => self.message = Some("Select a layer to sketch."),
                }
            }
        });
    }
}
