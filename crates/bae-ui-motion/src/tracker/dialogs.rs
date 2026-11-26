use super::*;
use bae_model::effect::{ParamKind, param_attr};
#[derive(Default)]
pub(super) struct Target {
    layer: Option<PrimPath>,
    effect: bool,
    point: Option<(PrimPath, String)>,
}
pub(super) struct Options {
    name: String,
    channel: String,
    subpixel: bool,
    fields: bool,
    radius: f64,
    adapt: bool,
    enhance: String,
    policy: String,
    threshold: f64,
}
pub(super) enum Dialog {
    Target(Target),
    Options(Options),
    Apply(String),
}
impl Dialog {
    pub fn target(cx: &PanelContext<'_>, track: &PrimPath) -> Self {
        let dest = model::setting(cx.stage, track, "target").and_then(|v| v.as_prim_ref().cloned());
        let layer = dest.as_ref().and_then(|p| query::owning_layer(cx.stage, p));
        let point = dest
            .filter(|p| Some(p) != layer.as_ref())
            .map(|p| (p, token(cx.stage, track, "target_property")));
        Self::Target(Target {
            layer,
            effect: point.is_some(),
            point,
        })
    }
    pub fn options(stage: &Stage, track: &PrimPath) -> Self {
        Self::Options(Options {
            name: query::display_name_of(stage, track),
            channel: token(stage, track, "channel"),
            subpixel: flag(stage, track, "subpixel"),
            fields: flag(stage, track, "track_fields"),
            radius: model::setting(stage, track, "enhance_radius")
                .and_then(|v| v.as_f64())
                .unwrap_or(2.),
            adapt: flag(stage, track, "adapt"),
            enhance: token(stage, track, "enhance"),
            policy: token(stage, track, "low_confidence"),
            threshold: model::setting(stage, track, "threshold")
                .and_then(|v| v.as_f64())
                .unwrap_or(80.),
        })
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        cx: &mut PanelContext<'_>,
        track: &PrimPath,
    ) -> bool {
        let title = match self {
            Self::Target(_) => "Motion Target",
            Self::Options(_) => "Motion Tracker Options",
            Self::Apply(_) => "Motion Tracker Apply Options",
        };
        let mut keep = true;
        let response = egui::Modal::new(egui::Id::new("motion_tracker_dialog")).show(ctx, |ui| {
            ui.set_min_width(290.);
            ui.heading(tr(title));
            ui.separator();
            let valid = match self {
                Self::Target(target) => target.draw(ui, cx, track),
                Self::Options(opts) => {
                    opts.draw(ui);
                    !opts.name.trim().is_empty()
                }
                Self::Apply(axes) => {
                    ui.horizontal(|ui| {
                        ui.label(tr("Apply Dimensions:"));
                        combo(
                            ui,
                            "axes",
                            axes,
                            &[("xy", "X and Y"), ("x", "X Only"), ("y", "Y Only")],
                        );
                    });
                    true
                }
            };
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(tr("Cancel")).clicked() {
                    keep = false;
                }
                if ui.add_enabled(valid, egui::Button::new(tr("OK"))).clicked() {
                    match self {
                        Self::Target(t) => {
                            let (dest, prop) = if t.effect {
                                t.point.clone().expect("selected effect point")
                            } else {
                                (t.layer.clone().expect("selected layer"), String::new())
                            };
                            configure(
                                cx,
                                track,
                                BTreeMap::from([
                                    ("target".into(), Value::PrimRef(dest)),
                                    ("target_property".into(), Value::String(prop)),
                                ]),
                            );
                        }
                        Self::Options(opts) => configure(cx, track, opts.values()),
                        Self::Apply(axes) => {
                            cx.invoke_with("tracker.apply", json!({"tracker":track,"axes":axes}))
                        }
                    }
                    keep = false;
                }
            });
        });
        keep && !response.should_close()
    }
}
impl Target {
    fn draw(&mut self, ui: &mut Ui, cx: &PanelContext<'_>, track: &PrimPath) -> bool {
        let stage = cx.stage;
        let layers = query::owning_comp(stage, track)
            .map(|p| query::comp_layers(stage, &p))
            .unwrap_or_default();
        let source = query::owning_layer(stage, track);
        if self.layer.is_none() {
            // AE's default: the layer just above the source (never the source).
            self.layer = source
                .as_ref()
                .and_then(|s| bae_motion::tracker::layer_above(stage, s));
        }
        ui.label(tr("Apply Motion To:"));
        ui.radio_value(&mut self.effect, false, tr("Layer"));
        choose_path(ui, stage, "Layer:", &mut self.layer, &layers);
        let mut choices = Vec::new();
        if let Some(layer) = &self.layer
            && let Some(reg) = crate::effects::registries(cx.resources)
        {
            for fx in bae_motion::effect::effects_of(stage, layer) {
                if let Some(def) = reg.def(&token(stage, &fx, "match_name")) {
                    for p in &def.params {
                        if p.kind == ParamKind::Point && !p.flags.hidden {
                            choices.push((
                                (fx.clone(), param_attr(p.id)),
                                format!("{} / {}", query::display_name_of(stage, &fx), tr(&p.name)),
                            ));
                        }
                    }
                }
            }
        }
        let permitted = !matches!(
            model::mode(stage, track).as_str(),
            "parallel" | "perspective"
        ) && !choices.is_empty();
        if !permitted {
            self.effect = false;
        }
        if !choices.iter().any(|(v, _)| Some(v) == self.point.as_ref()) {
            self.point = choices.first().map(|(v, _)| v.clone());
        }
        ui.add_enabled_ui(permitted, |ui| {
            ui.radio_value(&mut self.effect, true, tr("Effect Point Control:"));
            let label = choices
                .iter()
                .find(|(v, _)| Some(v) == self.point.as_ref())
                .map_or(tr("None"), |(_, s)| s.as_str());
            egui::ComboBox::from_id_salt("epc")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for (v, name) in choices {
                        ui.selectable_value(&mut self.point, Some(v), name);
                    }
                });
        });
        self.layer
            .as_ref()
            .is_some_and(|l| !query::is_locked(stage, l))
    }
}
impl Options {
    fn draw(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(tr("Track Name:"));
            ui.text_edit_singleline(&mut self.name);
        });
        ui.horizontal(|ui| {
            ui.label(tr("Tracker Plug-in:"));
            ui.label(tr("Built-in"));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("Channel"));
            for (k, l) in [
                ("rgb", "RGB"),
                ("luminance", "Luminance"),
                ("saturation", "Saturation"),
            ] {
                ui.radio_value(&mut self.channel, k.into(), tr(l));
            }
        });
        ui.horizontal(|ui| {
            ui.label(tr("Enhance Before Match"));
            combo(
                ui,
                "enhance",
                &mut self.enhance,
                &[("none", "None"), ("blur", "Blur"), ("sharpen", "Sharpen")],
            );
        });
        if self.enhance != "none" {
            ui.add(
                egui::DragValue::new(&mut self.radius)
                    .range(1.0..=32.0)
                    .suffix(tr("px")),
            );
        }
        ui.checkbox(&mut self.fields, tr("Track Fields"));
        ui.checkbox(&mut self.subpixel, tr("Subpixel Positioning"));
        ui.checkbox(&mut self.adapt, tr("Adapt Feature On Every Frame"));
        if self.adapt && self.policy == "adapt" {
            self.policy = "continue".into();
        }
        let policies = [
            ("continue", "Continue Tracking"),
            ("stop", "Stop Tracking"),
            ("extrapolate", "Extrapolate Motion"),
            ("adapt", "Adapt Feature"),
        ];
        ui.horizontal_wrapped(|ui| {
            combo(
                ui,
                "low_confidence",
                &mut self.policy,
                if self.adapt {
                    &policies[..3]
                } else {
                    &policies
                },
            );
            ui.label(tr("If Confidence Is Below"));
            ui.add(
                egui::DragValue::new(&mut self.threshold)
                    .range(0.0..=100.0)
                    .suffix("%"),
            );
        });
    }
    fn values(&self) -> BTreeMap<String, Value> {
        [
            ("name", Value::String(self.name.trim().into())),
            ("channel", Value::Enum(self.channel.clone())),
            ("enhance", Value::Enum(self.enhance.clone())),
            ("low_confidence", Value::Enum(self.policy.clone())),
            ("threshold", Value::Float(self.threshold)),
            ("subpixel", Value::Bool(self.subpixel)),
            ("track_fields", Value::Bool(self.fields)),
            ("enhance_radius", Value::Float(self.radius)),
            ("adapt", Value::Bool(self.adapt)),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect()
    }
}
pub(super) const ZH: &[(&str, &str)] = &[
    ("Motion Trackers", "动态跟踪器"),
    ("Feature Center", "功能中心"),
    ("Feature Size", "功能大小"),
    ("Search Offset", "搜索位移"),
    ("Search Size", "搜索大小"),
    ("Confidence", "可信度"),
    ("Attach Point", "附加点"),
    ("Attach Point Offset", "附加点位移"),
    ("Stabilize", "稳定"),
    ("Parallel Corner Pin", "平行边角定位"),
    ("Perspective Corner Pin", "透视边角定位"),
    ("Motion Target", "运动目标"),
    ("Motion Tracker Options", "动态跟踪器选项"),
    ("Motion Tracker Apply Options", "动态跟踪器应用选项"),
    ("Apply Motion To:", "将运动应用于:"),
    ("Effect Point Control:", "效果点控制:"),
    ("Apply Dimensions:", "应用维度:"),
    ("X and Y", "X 和 Y"),
    ("X Only", "仅 X"),
    ("Y Only", "仅 Y"),
    ("Track Name:", "轨道名称:"),
    ("Tracker Plug-in:", "跟踪器增效工具:"),
    ("Built-in", "内置"),
    ("Luminance", "明亮度"),
    ("Enhance Before Match", "匹配前增强"),
    ("Track Fields", "跟踪场"),
    ("Subpixel Positioning", "子像素定位"),
    ("Adapt Feature On Every Frame", "每帧上的自适应特性"),
    ("If Confidence Is Below", "如果置信度低于"),
    ("Continue Tracking", "继续跟踪"),
    ("Stop Tracking", "停止跟踪"),
    ("Extrapolate Motion", "预测运动"),
    ("Adapt Feature", "自适应特性"),
    ("Analyze One Frame Backward", "向后分析一帧"),
    ("Analyze Backward", "向后分析"),
    ("Analyze Forward", "向前分析"),
    ("Analyze One Frame Forward", "向前分析一帧"),
    ("Tracker is warming up", "正在准备跟踪器"),
    ("No frame in this direction", "该方向没有可分析的帧"),
    (
        "Analysis stopped because the project changed",
        "工程已更改，跟踪分析已停止",
    ),
    ("Analysis interrupted", "分析中断"),
    (
        "Tracking stopped below the confidence threshold",
        "可信度低于阈值，跟踪已停止",
    ),
    ("Missing motion source", "找不到运动源"),
    ("Missing composition", "找不到合成"),
];
