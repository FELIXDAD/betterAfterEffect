use std::sync::Arc;

use bae_app::{AppBuilder, Registry};
use bae_model::{DynCommand, PrimType};
use bae_motion::query;
use bae_ui::{Panel, PanelContext, PanelKind, tr};
use serde_json::json;

const COMMAND: &str = "plugin.org.bae.path-nulls.bind";
const TITLE: &str = "Create Nulls From Paths.jsx";

pub(crate) fn register(app: &mut AppBuilder) {
    bae_ui::i18n::register(&[
        ("Points Follow Nulls", "点跟随空对象"),
        ("Nulls Follow Points", "空对象跟随点"),
        ("Trace Path", "追踪路径"),
        (
            "Select a mask path or a Bezier shape path.",
            "请先选择蒙版路径或贝塞尔形状路径。",
        ),
    ]);
    let commands = app.registry::<dyn DynCommand>();
    crate::register_panel(
        app,
        "create_nulls_from_paths",
        PanelKind::new(TITLE, move |_| Box::new(PathNulls(commands.clone()))),
    );
}

struct PathNulls(Arc<Registry<dyn DynCommand>>);

impl Panel for PathNulls {
    fn ui(&mut self, ui: &mut egui::Ui, cx: &mut PanelContext<'_>) {
        ui.horizontal_wrapped(|ui| {
            for (mode, title) in [
                ("controllers", "Points Follow Nulls"),
                ("followers", "Nulls Follow Points"),
                ("trace", "Trace Path"),
            ] {
                if !ui
                    .add_enabled(self.0.get(COMMAND).is_some(), egui::Button::new(tr(title)))
                    .clicked()
                {
                    continue;
                }
                let session = crate::common::session(cx.resources);
                let s = session.lock();
                let mut paths = Vec::new();
                for path in s
                    .selected_props
                    .iter()
                    .filter(|(_, a)| a == "path")
                    .map(|(p, _)| p.clone())
                    .chain(
                        s.selected_groups
                            .iter()
                            .filter_map(|g| crate::session::group_prim(g)),
                    )
                {
                    if cx
                        .stage
                        .prim(&path)
                        .is_some_and(|v| v.is_a(&PrimType::MASK) || v.is_a(&PrimType::PATH))
                        && query::owning_comp(cx.stage, &path) == s.active_comp
                        && !paths.contains(&path)
                    {
                        paths.push(path);
                    }
                }
                drop(s);
                if paths.is_empty() {
                    crate::dialogs::alert(
                        cx,
                        TITLE,
                        tr("Select a mask path or a Bezier shape path.").to_owned(),
                    );
                } else {
                    cx.invoke_with(COMMAND, json!({"mode": mode, "paths": paths}));
                }
            }
        });
    }
}
