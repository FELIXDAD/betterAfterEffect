//! Window > Content-Aware Fill. Preview and baking both use layer-space alpha.
use crate::{
    common,
    gpu_preview::GpuPreview,
    vision_bake::{self, Job, Mode},
};
use bae_anim::EvaluatedEffect;
use bae_gpu::{GpuContext, ViewportTextureRegistry, WarmUp};
use bae_model::{PrimPath, Value};
use bae_motion::query;
use bae_ui::{Panel, PanelContext, PanelKind, tr};
use bae_vision::{FillMethod, FillSettings};
use std::sync::{Arc, Mutex};

struct TargetPreview {
    registry: Arc<ViewportTextureRegistry>,
    gpu: Mutex<Option<GpuPreview>>,
}
impl WarmUp for TargetPreview {
    fn name(&self) -> &str {
        "content-aware fill target"
    }
    fn run(&self, gpu: &Arc<GpuContext>) -> Result<(), String> {
        let mut preview = GpuPreview::with(gpu.clone(), self.registry.clone());
        preview.display.channel = 4;
        *self.gpu.lock().map_err(|e| e.to_string())? = Some(preview);
        Ok(())
    }
}
pub(super) fn register(app: &mut bae_app::AppBuilder) {
    if let Some(registry) = app.resource::<Arc<ViewportTextureRegistry>>().cloned() {
        let preview = Arc::new(TargetPreview {
            registry,
            gpu: Mutex::default(),
        });
        app.insert_resource(preview.clone());
        let _ = app.register_arc::<dyn WarmUp>("vision.fill_target", preview);
    }
    crate::register_panel(
        app,
        "content_aware_fill",
        PanelKind::new("Content-Aware Fill", |_| Box::<FillPanel>::default()),
    );
    bae_ui::i18n::register(&[
        ("Fill Layer", "填充图层"),
        ("Object", "对象"),
        ("Surface", "表面"),
        ("Edge Blend", "边缘混合"),
        ("Entire Duration", "整个持续时间"),
        ("Subtle", "轻微"),
        ("Moderate", "中等"),
        ("Strong", "强"),
        ("Select a layer to fill.", "选择要填充的图层。"),
    ]);
}
#[derive(Default)]
struct FillPanel {
    eval: common::EvalCache,
    method: usize,
    lighting: usize,
    range: usize,
    job: Option<Job>,
    message: String,
}
fn choice(ui: &mut egui::Ui, label: &str, value: &mut usize, options: &[&str]) {
    ui.horizontal(|ui| {
        ui.label(tr(label));
        egui::ComboBox::from_id_salt(label)
            .selected_text(tr(options[*value]))
            .show_ui(ui, |ui| {
                for (i, name) in options.iter().enumerate() {
                    ui.selectable_value(value, i, tr(name));
                }
            });
    });
}
impl FillPanel {
    fn preview(
        &mut self,
        ui: &mut egui::Ui,
        cx: &PanelContext<'_>,
        layer: &PrimPath,
        comp: &query::CompInfo,
        expansion: u32,
    ) {
        let session = common::session(cx.resources);
        let time = session.lock().time(&comp.path);
        let (area, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 80.), egui::Sense::hover());
        ui.painter().rect_filled(area, 0., egui::Color32::BLACK);
        let Some(preview) = cx.resources.get::<Arc<TargetPreview>>() else {
            return;
        };
        let Ok(mut lock) = preview.gpu.try_lock() else {
            return;
        };
        let Some(gpu) = lock.as_mut() else { return };
        let eval = crate::viewer::live_evaluator(&mut self.eval, cx, &session);
        let Ok(evaluated) = eval.evaluate_comp(cx.stage, &comp.path, time) else {
            return;
        };
        let Some((mut one, _)) = crate::layer_panel::alone(&evaluated, layer, usize::MAX) else {
            return;
        };
        let source = &mut one.layers[0];
        source.masks = evaluated
            .layer(layer)
            .expect("isolated layer")
            .masks
            .clone();
        if expansion > 0 {
            source.effects.push(EvaluatedEffect {
                path: layer.child("FillTargetPreview").expect("valid path"),
                match_name: "ADBE Minimax".into(),
                enabled: true,
                children: Vec::new(),
                params: [
                    ("0001", Value::Int(1)),
                    ("0002", Value::Float(f64::from(expansion))),
                    ("0003", Value::Int(6)),
                    ("0005", Value::Bool(true)),
                ]
                .into_iter()
                .map(|(id, v)| (id.to_owned(), v))
                .collect(),
            });
        }
        let dims = egui::vec2(one.width as f32, one.height as f32);
        let scale = (area.width() / dims.x).min(area.height() / dims.y).min(1.);
        let rect = egui::Rect::from_center_size(area.center(), dims * scale);
        let res = (1. / scale.max(0.001)).floor().clamp(1., 8.) as u32;
        if let Some(img) = gpu.frame(cx.resources, cx.stage, &one, res, None, false) {
            ui.painter().image(
                egui::TextureId::User(img.handle),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                egui::Color32::WHITE,
            );
        }
    }
}
impl Panel for FillPanel {
    fn ui(&mut self, ui: &mut egui::Ui, cx: &mut PanelContext<'_>) {
        if let Some(job) = &mut self.job
            && let Some(result) = job.poll(cx)
        {
            self.message = result.err().unwrap_or_default();
            self.job = None;
        }
        let session = common::session(cx.resources);
        let source = common::active_comp(cx.stage, &session).and_then(|comp| {
            let layer = session.lock().selected_layers.first()?.clone();
            (query::owning_comp(cx.stage, &layer) == Some(comp.path.clone()))
                .then_some((layer, comp))
        });
        const EXPANSION: &str = "vision:fill_expansion";
        let mut expansion = source
            .as_ref()
            .and_then(|(layer, comp)| {
                let s = session.lock();
                let time = s.time(&comp.path);
                if let Some(scrub) = &s.scrub
                    && scrub.path == *layer
                    && scrub.attr == EXPANSION
                {
                    return scrub.value.as_f64();
                }
                drop(s);
                common::current_value(
                    self.eval.get(cx.stage),
                    cx.stage,
                    layer,
                    EXPANSION,
                    query::to_key_time(cx.stage, layer, time),
                )
                .and_then(|v| v.as_f64())
            })
            .unwrap_or(0.);
        ui.label(tr("Fill Target"));
        if let Some((layer, comp)) = &source {
            self.preview(ui, cx, layer, comp, expansion as u32);
        } else {
            ui.label(tr("Select a layer to fill."));
        }
        ui.add_enabled_ui(self.job.is_none(), |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("Alpha Expansion"));
                let response = ui.add_enabled(
                    source.is_some(),
                    bae_ui::widgets::HotNumber::new(&mut expansion)
                        .range(0.0..=64.0)
                        .decimals(0)
                        .speed(1.0)
                        .suffix(" px")
                        .label(tr("Alpha Expansion")),
                );
                if let Some((layer, comp)) = &source {
                    let time = session.lock().time(&comp.path);
                    let v = Value::Int(expansion.round() as i64);
                    let edit = if response.drag_stopped() {
                        common::Edit::Commit(v)
                    } else if response.dragged() {
                        common::Edit::Live(v)
                    } else if response.changed() {
                        common::Edit::Commit(v)
                    } else {
                        common::Edit::None
                    };
                    common::apply_edit(cx, &session, layer, EXPANSION, time, edit);
                    if response.dragged() {
                        ui.ctx().request_repaint();
                    }
                }
            });
            choice(
                ui,
                "Fill Method",
                &mut self.method,
                &["Object", "Surface", "Edge Blend"],
            );
            ui.add_enabled_ui(self.method == 0, |ui| {
                choice(
                    ui,
                    "Lighting Correction",
                    &mut self.lighting,
                    &["None", "Subtle", "Moderate", "Strong"],
                )
            });
            choice(
                ui,
                "Range",
                &mut self.range,
                &["Work Area", "Entire Duration"],
            );
            ui.add_enabled(false, egui::Button::new(tr("Create Reference Frame")));
            if ui
                .add_enabled(
                    source.is_some(),
                    egui::Button::new(tr("Generate Fill Layer")),
                )
                .clicked()
                && let Some((layer, comp)) = source
            {
                let timing = query::layer_timing(cx.stage, &layer);
                let range = if self.range == 0 {
                    [
                        comp.work_area.start.max(timing.in_point),
                        comp.work_area.end.min(timing.out_point),
                    ]
                } else {
                    [timing.in_point, timing.out_point]
                };
                let settings = FillSettings {
                    method: [
                        FillMethod::Object,
                        FillMethod::Surface,
                        FillMethod::EdgeBlend,
                    ][self.method],
                    expansion: expansion.round() as u32,
                    lighting: [0., 0.33, 0.66, 1.][self.lighting],
                };
                self.message.clear();
                match vision_bake::start(
                    cx,
                    ui.ctx(),
                    layer.clone(),
                    Mode::Fill(layer, settings),
                    range,
                ) {
                    Ok(job) => self.job = Some(job),
                    Err(e) => self.message = e,
                }
            }
        });
        if let Some(job) = &self.job {
            ui.add(
                egui::ProgressBar::new(job.done as f32 / job.frames as f32)
                    .text(format!("{} / {}", job.done, job.frames)),
            );
            if ui.button(tr("Cancel")).clicked() {
                self.job = None;
            }
        }
        if !self.message.is_empty() {
            ui.label(&self.message);
        }
    }
}
