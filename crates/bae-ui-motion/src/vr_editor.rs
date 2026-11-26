use std::sync::Arc;

use bae_app::{AppBuilder, Registry};
use bae_model::{DynCommand, PrimPath, Stage, Value};
use bae_motion::query;
use bae_ui::{Panel, PanelContext, PanelKind, tr};
use serde_json::json;

const COMMAND: &str = "plugin.org.bae.vr-editor.edit";
const TITLE: &str = "VR Comp Editor.jsx";

pub(crate) fn register(app: &mut AppBuilder) {
    let commands = app.registry::<dyn DynCommand>();
    crate::register_panel(
        app,
        "vr_comp_editor",
        PanelKind::new(TITLE, move |_| {
            Box::new(Editor {
                commands: commands.clone(),
                revision: None,
                comps: Vec::new(),
                form: None,
                pending: None,
            })
        }),
    );
    bae_ui::i18n::register(&[
        ("Add 2D Edit", "添加 2D 编辑"),
        ("Add 3D Edit", "添加 3D 编辑"),
        ("Create VR Master", "创建 VR 主合成"),
        ("Extract Cubemap", "提取立方图"),
        ("Open Output/Render", "打开输出/渲染"),
        ("Output Settings", "输出设置"),
        ("Comp Width", "合成宽度"),
        ("Aspect Ratio", "长宽比"),
        ("Conversion Resolution", "转换分辨率"),
        ("Output Format", "输出格式"),
        ("Use 2-node camera", "使用双节点摄像机"),
        ("Use 3D Null camera control", "使用 3D 空对象控制摄像机"),
        ("Center camera", "居中摄像机"),
        ("Cube-map 4:3", "立方图 4:3"),
        ("Sphere-map", "球形映射"),
        ("Fisheye (FullDome)", "鱼眼（全天域）"),
        ("Equirectangular 2:1", "等距柱状投影 2:1"),
        ("Cube-map Facebook 3:2", "Facebook 立方图 3:2"),
        ("Cube-map Pano2VR 3:2", "Pano2VR 立方图 3:2"),
        ("Cube-map GearVR 6:1", "GearVR 立方图 6:1"),
        ("Equirectangular 16:9", "等距柱状投影 16:9"),
        ("Pan (Y Axis)", "平移（Y 轴）"),
        ("Tilt (X Axis)", "倾斜（X 轴）"),
        ("Front View", "前面"),
        ("Back View", "后面"),
        ("Left View", "左面"),
        ("Right View", "右面"),
        ("Top View", "顶面"),
        ("Bottom View", "底面"),
    ]);
}

fn value(stage: &Stage, path: &PrimPath, key: &str) -> Option<Value> {
    query::static_value(&stage.prim(path)?, key)
}
fn role(stage: &Stage, path: &PrimPath) -> String {
    value(stage, path, "vr:role")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

struct Form {
    action: &'static str,
    master: Option<PrimPath>,
    edit: Option<PrimPath>,
    width: u32,
    aspect: usize,
    format: usize,
    fps: String,
    duration: String,
    pan: String,
    tilt: String,
    two_node: bool,
    null_control: bool,
    center: bool,
}
const FORMATS: &[(usize, &str)] = &[
    (3, "Equirectangular 2:1"),
    (1, "Cube-map 4:3"),
    (2, "Sphere-map"),
    (4, "Fisheye (FullDome)"),
    (5, "Cube-map Facebook 3:2"),
    (6, "Cube-map Pano2VR 3:2"),
    (7, "Cube-map GearVR 6:1"),
    (8, "Equirectangular 16:9"),
];
const ASPECTS: &[(usize, &str)] = &[(1, "1:1"), (2, "4:3"), (3, "16:9"), (4, "16:10")];
impl Form {
    fn new(action: &'static str, master: Option<PrimPath>) -> Self {
        Self {
            action,
            master,
            edit: None,
            width: 1024,
            aspect: 3,
            format: 3,
            fps: "30".into(),
            duration: "30".into(),
            pan: "0".into(),
            tilt: "0".into(),
            two_node: true,
            null_control: false,
            center: true,
        }
    }
    fn title(&self) -> &'static str {
        match self.action {
            "master" => "Create VR Master",
            "edit2d" => "Add 2D Edit",
            "edit3d" => "Add 3D Edit",
            "cube" => "Extract Cubemap",
            "properties" => "Properties",
            _ => "Output Settings",
        }
    }
    fn args(&self) -> Option<serde_json::Value> {
        let fps = self.fps.parse::<f64>().ok()?;
        let duration = self.duration.parse::<f64>().ok()?;
        let pan = self.pan.parse::<f64>().ok()?;
        let tilt = self.tilt.parse::<f64>().ok()?;
        if !(fps > 0.0
            && fps <= 999.0
            && duration.is_finite()
            && duration > 0.0
            && pan.is_finite()
            && tilt.is_finite())
        {
            return None;
        }
        let mut args = json!({
            "action": self.action, "width": self.width, "aspect": self.aspect, "format": self.format,
            "fps": fps, "duration": duration, "pan": pan, "tilt": tilt,
            "twoNode": self.two_node, "nullControl": self.null_control, "center": self.center,
        });
        if let Some(master) = &self.master {
            args["master"] = json!(master);
        }
        if let Some(edit) = &self.edit {
            args["edit"] = json!(edit);
        }
        Some(args)
    }
}
fn choices(ui: &mut egui::Ui, label: &str, value: &mut usize, options: &[(usize, &str)]) {
    egui::ComboBox::from_label(tr(label))
        .selected_text(tr(options
            .iter()
            .find(|(i, _)| i == value)
            .unwrap_or(&options[0])
            .1))
        .show_ui(ui, |ui| {
            for &(i, name) in options {
                ui.selectable_value(value, i, tr(name));
            }
        });
}
fn field(ui: &mut egui::Ui, label: &str, text: &mut String) {
    ui.horizontal(|ui| {
        ui.label(tr(label));
        ui.add(egui::TextEdit::singleline(text).desired_width(70.));
    });
}

struct Editor {
    commands: Arc<Registry<dyn DynCommand>>,
    revision: Option<u64>,
    comps: Vec<PrimPath>,
    form: Option<Form>,
    pending: Option<(&'static str, Option<PrimPath>, Vec<PrimPath>)>,
}

impl Panel for Editor {
    fn ui(&mut self, ui: &mut egui::Ui, cx: &mut PanelContext<'_>) {
        let session = crate::common::session(cx.resources);
        let stage = cx.stage;
        if self.revision != Some(stage.revision()) {
            self.comps = query::comps(stage);
            self.revision = Some(stage.revision());
        }
        let comps = &self.comps;
        if let Some((wanted, master, before)) = self.pending.take()
            && let Some(path) = comps.iter().find(|p| {
                role(stage, p) == wanted
                    && !before.contains(p)
                    && (master.is_none()
                        || value(stage, p, "vr:master")
                            .as_ref()
                            .and_then(Value::as_prim_ref)
                            == master.as_ref())
            })
        {
            session.lock().open_comp(path);
        }
        let active = session.lock().active_comp.clone();
        let root = active
            .as_ref()
            .and_then(|p| value(stage, p, "vr:master"))
            .and_then(|v| v.as_prim_ref().cloned())
            .or(active)
            .filter(|p| comps.contains(p));
        let members: Vec<_> = comps
            .iter()
            .filter(|p| {
                root.is_some()
                    && value(stage, p, "vr:master")
                        .as_ref()
                        .and_then(Value::as_prim_ref)
                        == root.as_ref()
            })
            .collect();
        let output = members.iter().copied().find(|p| role(stage, p) == "output");
        ui.add_enabled_ui(self.commands.get(COMMAND).is_some(), |ui| {
            if let Some(f) = &mut self.form {
                ui.label(tr(f.title()));
                if matches!(f.action, "edit2d" | "edit3d" | "cube") {
                    egui::ComboBox::from_label(tr("Composition"))
                        .selected_text(
                            f.master
                                .as_ref()
                                .map(|p| query::display_name_of(stage, p))
                                .unwrap_or_default(),
                        )
                        .show_ui(ui, |ui| {
                            for p in comps
                                .iter()
                                .filter(|p| matches!(role(stage, p).as_str(), "" | "master3d"))
                            {
                                ui.selectable_value(
                                    &mut f.master,
                                    Some(p.clone()),
                                    query::display_name_of(stage, p),
                                );
                            }
                        });
                }
                egui::ComboBox::from_label(tr(if f.action == "cube" {
                    "Conversion Resolution"
                } else {
                    "Comp Width"
                }))
                .selected_text(f.width.to_string())
                .show_ui(ui, |ui| {
                    for w in [256, 512, 640, 1024, 1920, 2048, 3840, 4096, 7680, 8192] {
                        if f.action == "cube" && w > 4096 {
                            continue;
                        }
                        ui.selectable_value(&mut f.width, w, w.to_string());
                    }
                });
                match f.action {
                    "master" => {
                        field(ui, "Frame Rate", &mut f.fps);
                        field(ui, "Duration", &mut f.duration);
                        ui.checkbox(&mut f.two_node, tr("Use 2-node camera"));
                        ui.checkbox(&mut f.null_control, tr("Use 3D Null camera control"));
                        ui.checkbox(&mut f.center, tr("Center camera"));
                    }
                    "output" => choices(ui, "Output Format", &mut f.format, FORMATS),
                    "cube" => {}
                    _ => {
                        choices(ui, "Aspect Ratio", &mut f.aspect, ASPECTS);
                        field(ui, "Pan (Y Axis)", &mut f.pan);
                        field(ui, "Tilt (X Axis)", &mut f.tilt);
                    }
                }
                let args = f.args();
                let accept = ui
                    .add_enabled(
                        args.is_some(),
                        egui::Button::new(tr(if ["output", "properties"].contains(&f.action) {
                            "Save"
                        } else {
                            f.title()
                        })),
                    )
                    .clicked();
                if accept {
                    cx.invoke_with(COMMAND, args.unwrap());
                    let target = match f.action {
                        "master" => "master3d",
                        "cube" => "cubemap",
                        other => other,
                    };
                    if target == "properties" {
                        if let Some(path) = &f.edit {
                            session.lock().open_comp(path);
                        }
                    } else if target == "output"
                        && let Some(path) = output
                    {
                        session.lock().open_comp(path);
                    } else {
                        self.pending = Some((
                            target,
                            f.master.clone(),
                            if target == "cubemap" {
                                Vec::new()
                            } else {
                                comps.clone()
                            },
                        ));
                    }
                }
                let delete = f.action == "properties" && ui.button(tr("Delete")).clicked();
                if delete {
                    cx.invoke_with(
                        COMMAND,
                        json!({"action":"delete","edit":f.edit,"master":f.master}),
                    );
                    if let Some(p) = &f.master {
                        session.lock().open_comp(p);
                    }
                }
                if accept || delete || ui.button(tr("Cancel")).clicked() {
                    self.form = None;
                }
                return;
            }
            let source = root.as_ref().or_else(|| {
                comps
                    .iter()
                    .find(|p| matches!(role(stage, p).as_str(), "" | "master3d"))
            });
            ui.horizontal_wrapped(|ui| {
                for (title, action) in [
                    ("Add 2D Edit", "edit2d"),
                    ("Add 3D Edit", "edit3d"),
                    ("Create VR Master", "master"),
                    ("Extract Cubemap", "cube"),
                ] {
                    if ui
                        .add_enabled(
                            source.is_some() || action == "master",
                            egui::Button::new(tr(title)),
                        )
                        .clicked()
                    {
                        self.form = Some(Form::new(
                            action,
                            source.filter(|_| action != "master").cloned(),
                        ));
                    }
                }
            });
            let Some(master) = &root else { return };
            if ui.button(query::display_name_of(stage, master)).clicked() {
                session.lock().open_comp(master);
            }
            for edit in members
                .iter()
                .copied()
                .filter(|p| matches!(role(stage, p).as_str(), "edit2d" | "edit3d"))
            {
                ui.push_id(edit.as_str(), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(query::display_name_of(stage, edit)).clicked() {
                            session.lock().open_comp(edit);
                        }
                        if ui.button(tr("Properties")).clicked() {
                            let mut f = Form::new("properties", root.clone());
                            f.edit = Some(edit.clone());
                            f.width = query::comp_info(stage, edit).unwrap().width;
                            f.aspect = value(stage, edit, "vr:aspect")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(3.0) as usize;
                            f.pan = value(stage, edit, "vr:pan")
                                .and_then(|v| v.as_f64())
                                .unwrap_or_default()
                                .to_string();
                            f.tilt = value(stage, edit, "vr:tilt")
                                .and_then(|v| v.as_f64())
                                .unwrap_or_default()
                                .to_string();
                            self.form = Some(f);
                        }
                    })
                });
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button(tr("Open Output/Render")).clicked() {
                    cx.invoke_with(COMMAND, json!({"action":"output","master":master}));
                    if let Some(path) = output {
                        session.lock().open_comp(path);
                    } else {
                        self.pending = Some(("output", root.clone(), comps.clone()));
                    }
                }
                if ui.button(tr("Output Settings")).clicked() {
                    let mut f = Form::new("output", root.clone());
                    f.width = query::comp_info(stage, output.unwrap_or(master))
                        .unwrap()
                        .width
                        .clamp(256, 8192);
                    f.format = output
                        .and_then(|o| value(stage, o, "vr:format"))
                        .and_then(|v| v.as_f64())
                        .unwrap_or(3.0) as usize;
                    self.form = Some(f);
                }
            });
            if let Some(atlas) = members
                .iter()
                .copied()
                .find(|p| role(stage, p) == "cubemap")
            {
                ui.horizontal_wrapped(|ui| {
                    for face in members.iter().copied().filter(|p| {
                        value(stage, p, "vr:atlas")
                            .as_ref()
                            .and_then(Value::as_prim_ref)
                            == Some(atlas)
                    }) {
                        if let Some(Value::String(name)) = value(stage, face, "vr:face")
                            && ui.button(tr(&format!("{name} View"))).clicked()
                        {
                            session.lock().open_comp(face);
                        }
                    }
                });
            }
        });
    }
}
