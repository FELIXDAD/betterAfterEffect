//! Project panel: the project's folders, comps, footage and solids, with AE's
//! columns (Name, Type, Size, Duration, Frame Rate) and the New Folder / New
//! Composition / Delete buttons. Double-click a comp to open it; drag an item into
//! the timeline or viewer to add it as a layer.

use std::collections::HashSet;

use bae_anim::glam::DVec2;
use bae_model::{LayerKind, PrimPath, PrimType, Stage, Time, Value};
use bae_motion::query;
use bae_ui::Accessible as _;
use bae_ui::icons::{self, Icon};
use bae_ui::theme::{self, color};
use bae_ui::widgets;
use bae_ui::{Invocation, Panel, PanelContext, tr};
use egui::{Align2, Rect, Sense, Ui, pos2, vec2};
use serde_json::json;

use crate::common;
use crate::session::SharedSession;

/// Drag payload: a project item dragged into a comp.
#[derive(Clone, Debug)]
pub(crate) struct ItemDrag(pub PrimPath);

pub(crate) enum DropPlacement {
    Viewer(Option<DVec2>),
    Timeline { index: usize, start: Option<Time> },
}

impl ItemDrag {
    /// Add a project item to the active comp (drag and drop).
    pub(crate) fn add_as_layer(
        &self,
        cx: &mut PanelContext<'_>,
        session: &SharedSession,
        placement: DropPlacement,
    ) {
        let item = &self.0;
        let stage = cx.stage;
        let Some(active) = session.lock().active_comp.clone() else {
            return;
        };
        let kind = if query::comp_info(stage, item).is_some() {
            if query::would_nest_recursively(stage, &active, item) {
                return;
            }
            LayerKind::PreComp
        } else {
            match stage
                .prim(item)
                .and_then(|v| query::static_value(&v, "kind"))
            {
                Some(Value::Enum(k)) if k == "solid" => LayerKind::Solid,
                _ if stage.prim(item).is_some_and(|v| {
                    v.value("has_audio").and_then(Value::as_bool) == Some(true)
                        && v.value("has_video").and_then(Value::as_bool) == Some(false)
                }) =>
                {
                    LayerKind::Audio
                }
                _ => LayerKind::Footage,
            }
        };
        let mut extra = json!({"source": item, "name": query::display_name_of(stage, item)});
        match placement {
            DropPlacement::Viewer(Some(p)) => extra["position"] = json!([p.x, p.y]),
            DropPlacement::Viewer(None) => {}
            DropPlacement::Timeline { index, start } => {
                extra["index"] = json!(index);
                if let Some(start) = start {
                    extra["start"] = json!(start);
                }
            }
        }
        common::new_layer(cx, session, kind, extra);
    }
}

#[derive(Default)]
pub struct ProjectPanel {
    collapsed: HashSet<PrimPath>,
    search: String,
    /// Edit ▸ Find: focus the search field next frame.
    focus_search: bool,
    rename: Option<(PrimPath, String)>,
}

struct Row {
    path: PrimPath,
    depth: usize,
    kind: ItemKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    Folder,
    Comp,
    Footage,
    Solid,
}

fn item_kind(stage: &Stage, path: &PrimPath) -> Option<ItemKind> {
    let v = stage.prim(path)?;
    let t = v.prim_type()?;
    if *t == PrimType::FOLDER {
        Some(ItemKind::Folder)
    } else if *t == PrimType::COMP {
        Some(ItemKind::Comp)
    } else if *t == PrimType::ASSET {
        let kind = query::static_value(&v, "kind");
        Some(if matches!(&kind, Some(Value::Enum(k)) if k == "solid") {
            ItemKind::Solid
        } else {
            ItemKind::Footage
        })
    } else {
        None
    }
}

const ROW_H: f32 = 20.0;

impl ProjectPanel {
    fn rows(&self, stage: &Stage) -> Vec<Row> {
        let mut out = Vec::new();
        let needle = self.search.trim().to_lowercase();
        fn walk(
            p: &ProjectPanel,
            stage: &Stage,
            parent: &PrimPath,
            depth: usize,
            needle: &str,
            out: &mut Vec<Row>,
        ) {
            for c in stage.children(parent) {
                let Some(kind) = item_kind(stage, &c) else {
                    continue;
                };
                let name = query::display_name_of(stage, &c).to_lowercase();
                let start = out.len();
                if kind == ItemKind::Folder {
                    out.push(Row {
                        path: c.clone(),
                        depth,
                        kind,
                    });
                    if !p.collapsed.contains(&c) || !needle.is_empty() {
                        walk(p, stage, &c, depth + 1, needle, out);
                    }
                    // Hide folders without matches while searching.
                    if !needle.is_empty() && out.len() == start + 1 && !name.contains(needle) {
                        out.pop();
                    }
                } else if needle.is_empty() || name.contains(needle) {
                    out.push(Row {
                        path: c,
                        depth,
                        kind,
                    });
                }
            }
        }
        walk(self, stage, &PrimPath::root(), 0, &needle, &mut out);
        out
    }

    fn info_area(&self, ui: &mut Ui, cx: &PanelContext<'_>, selected: Option<&PrimPath>) {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::hover());
        let p = ui.painter();
        let Some(path) = selected else {
            return;
        };
        let stage = cx.stage;
        let thumb = Rect::from_min_size(rect.min + vec2(8.0, 6.0), vec2(64.0, 44.0));
        p.rect_filled(thumb, 1.0, color::FIELD);
        let icon = match item_kind(stage, path) {
            Some(ItemKind::Comp) => Icon::Composition,
            Some(ItemKind::Folder) => Icon::Folder,
            _ => Icon::Footage,
        };
        icons::paint(
            p,
            Rect::from_center_size(thumb.center(), vec2(20.0, 20.0)),
            icon,
            color::ICON_DIM,
        );
        let x = thumb.right() + 10.0;
        let mut y = rect.top() + 12.0;
        let mut line = |text: String, c| {
            p.text(pos2(x, y), Align2::LEFT_CENTER, text, theme::small(), c);
            y += 14.0;
        };
        line(query::display_name_of(stage, path), color::TEXT_STRONG);
        if let Some(info) = query::comp_info(stage, path) {
            line(
                format!(
                    "{} × {} ({:.2})",
                    info.width, info.height, info.pixel_aspect
                ),
                color::TEXT,
            );
            line(
                format!(
                    "Δ {}, {:.2} fps",
                    common::duration_code(&info, info.duration),
                    info.frame_rate.as_f64()
                ),
                color::TEXT,
            );
        } else if let Some(v) = stage.prim(path) {
            let w = query::static_value(&v, "width").and_then(|v| v.as_f64());
            let h = query::static_value(&v, "height").and_then(|v| v.as_f64());
            if let (Some(w), Some(h)) = (w, h) {
                line(format!("{w} × {h}"), color::TEXT);
            }
        }
    }
}

fn columns(width: f32) -> [f32; 5] {
    // label, name, type, size, duration (x offsets).
    let name_w = (width - 16.0 - 90.0 - 80.0 - 90.0).max(90.0);
    [
        6.0,
        22.0,
        22.0 + name_w,
        22.0 + name_w + 90.0,
        22.0 + name_w + 170.0,
    ]
}

#[cfg(test)]
impl ProjectPanel {
    /// An item is being renamed.
    pub(crate) fn is_renaming(&self) -> bool {
        self.rename.is_some()
    }

    pub(crate) fn start_rename(&mut self, item: PrimPath, text: &str) {
        self.rename = Some((item, text.to_owned()));
    }
}

impl Panel for ProjectPanel {
    fn ui(&mut self, ui: &mut Ui, cx: &mut PanelContext<'_>) {
        let panel_rect = ui.max_rect();
        let session = common::session(cx.resources);
        session.lock().sync(cx.stage);
        let stage = cx.stage;
        let selected_first = session.lock().selected_items.first().cloned();
        self.info_area(ui, cx, selected_first.as_ref());

        ui.horizontal(|ui| {
            ui.add_space(6.0);
            let field = widgets::search_field(
                ui,
                &mut self.search,
                (ui.available_width() - 12.0).max(60.0),
            );
            if std::mem::take(&mut self.focus_search) {
                field.request_focus();
            }
        });
        ui.add_space(4.0);

        // Column headers.
        let width = ui.available_width();
        let cols = columns(width);
        let (head, head_resp) = ui.allocate_exact_size(vec2(width, 18.0), Sense::click());
        common::context_menu(&head_resp, &mut |ui| {
            crate::context_menu::show(ui, cx, crate::context_menu::PROJECT_HEADER);
        });
        {
            let p = ui.painter();
            p.rect_filled(head, 0.0, color::PANEL);
            p.hline(
                head.x_range(),
                head.bottom() - 0.5,
                egui::Stroke::new(1.0, color::SEPARATOR),
            );
            icons::paint(
                p,
                Rect::from_center_size(
                    pos2(head.left() + cols[0] + 6.0, head.center().y),
                    vec2(12.0, 12.0),
                ),
                Icon::Tag,
                color::ICON_DIM,
            );
            for (i, title) in [(1, "Name"), (2, "Type"), (3, "Size"), (4, "Duration")] {
                p.text(
                    pos2(head.left() + cols[i] + 4.0, head.center().y),
                    Align2::LEFT_CENTER,
                    tr(title),
                    theme::small(),
                    color::TEXT,
                );
            }
        }

        let rows = self.rows(stage);
        let footer_h = 28.0;
        let list_h = (ui.available_height() - footer_h - ui.spacing().item_spacing.y).max(20.0);
        let mut open_comp: Option<PrimPath> = None;
        let mut open_footage: Option<PrimPath> = None;
        // The Enter that commits a rename must not start a new one.
        let mut rename_finished = false;
        let mut menu_new_comp = false;
        let mut menu_new_folder = false;
        let mut delete: Vec<PrimPath> = Vec::new();
        // Folder rows, for files dropped from Finder / Explorer.
        let mut folder_rows: Vec<(Rect, PrimPath)> = Vec::new();
        egui::ScrollArea::vertical()
            .max_height(list_h)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, row) in rows.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click_and_drag());
                    if row.kind == ItemKind::Folder {
                        folder_rows.push((rect, row.path.clone()));
                    }
                    let selected = session.lock().selected_items.contains(&row.path);
                    let resp =
                        resp.labeled_toggle(egui::WidgetType::SelectableLabel, &query::display_name_of(stage, &row.path), selected);
                    let p = ui.painter();
                    let bg = if selected {
                        color::ROW_SELECTED
                    } else if i % 2 == 0 {
                        color::PANEL
                    } else {
                        theme::brighten(color::PANEL, 4)
                    };
                    p.rect_filled(rect, 0.0, bg);
                    // Label swatch.
                    let label = stage
                        .prim(&row.path)
                        .and_then(|v| query::static_value(&v, "label"))
                        .and_then(|v| v.as_f64())
                        .map_or(0, |v| v as usize);
                    icons::swatch(p, pos2(rect.left() + cols[0] + 6.0, rect.center().y), 9.0, theme::label_color(label));
                    // Name with icon and twirl.
                    let indent = cols[1] + row.depth as f32 * 14.0;
                    let mut x = rect.left() + indent;
                    if row.kind == ItemKind::Folder {
                        let open = !self.collapsed.contains(&row.path);
                        let tw = Rect::from_center_size(pos2(x + 6.0, rect.center().y), vec2(12.0, 12.0));
                        icons::paint(p, tw, if open { Icon::ChevronDown } else { Icon::ChevronRight }, color::ICON);
                        if ui.input(|i| i.pointer.primary_clicked())
                            && ui.input(|i| i.pointer.interact_pos()).is_some_and(|pos| tw.contains(pos))
                        {
                            if open {
                                self.collapsed.insert(row.path.clone());
                            } else {
                                self.collapsed.remove(&row.path);
                            }
                        }
                    }
                    x += 14.0;
                    let icon = match row.kind {
                        ItemKind::Folder => Icon::Folder,
                        ItemKind::Comp => Icon::Composition,
                        ItemKind::Footage | ItemKind::Solid => Icon::Footage,
                    };
                    let p = ui.painter();
                    icons::paint(p, Rect::from_center_size(pos2(x + 7.0, rect.center().y), vec2(14.0, 14.0)), icon, color::ICON);
                    x += 20.0;
                    let name = query::display_name_of(stage, &row.path);
                    let renaming = self.rename.as_ref().is_some_and(|(r, _)| r == &row.path);
                    if renaming {
                        let field = Rect::from_min_max(pos2(x, rect.top() + 1.0), pos2(rect.left() + cols[2] - 4.0, rect.bottom() - 1.0));
                        let mut done = None;
                        if let Some((_, buf)) = &mut self.rename {
                            let id = ui.id().with(("rename", row.path.as_str()));
                            done = common::rename_field(ui, field, buf, id);
                        }
                        if let Some(result) = done {
                            rename_finished = true;
                            if let Some(new) = result.filter(|n| !n.trim().is_empty() && *n != name) {
                                cx.invoke_with(
                                    "attr.set",
                                    json!({"path": row.path, "attr": "name", "value": {"type": "string", "value": new}}),
                                );
                            }
                            self.rename = None;
                        }
                    } else {
                        widgets::clipped_text(p, pos2(x, rect.center().y), &name, theme::body(), color::TEXT_STRONG, rect.left() + cols[2] - x - 4.0);
                    }
                    // Type, size, duration.
                    let (ty, size, dur) = describe(stage, &row.path, row.kind);
                    let p = ui.painter();
                    for (i, text) in [(2, ty), (3, size), (4, dur)] {
                        widgets::clipped_text(p, pos2(rect.left() + cols[i] + 4.0, rect.center().y), &text, theme::small(), color::TEXT, 86.0);
                    }

                    // Interaction.
                    if resp.clicked() {
                        let mods = ui.input(|i| i.modifiers);
                        let mut s = session.lock();
                        if mods.command || mods.shift {
                            if let Some(i) = s.selected_items.iter().position(|p| p == &row.path) {
                                s.selected_items.remove(i);
                            } else {
                                s.selected_items.push(row.path.clone());
                            }
                        } else {
                            s.selected_items = vec![row.path.clone()];
                        }
                    }
                    if resp.double_clicked() {
                        match row.kind {
                            ItemKind::Comp => open_comp = Some(row.path.clone()),
                            // AE: footage opens in the Footage panel.
                            ItemKind::Footage => open_footage = Some(row.path.clone()),
                            ItemKind::Folder if !self.collapsed.remove(&row.path) => {
                                self.collapsed.insert(row.path.clone());
                            }
                            _ => {}
                        }
                    }
                    if row.kind != ItemKind::Folder {
                        resp.dnd_set_drag_payload(ItemDrag(row.path.clone()));
                    }
                    // AE 26's item menu (the same for comps, footage and folders).
                    common::context_menu(&resp, &mut |ui| {
                        {
                            let mut s = session.lock();
                            if !s.selected_items.contains(&row.path) {
                                s.selected_items = vec![row.path.clone()];
                            }
                        }
                        let chosen =
                            crate::context_menu::show(ui, cx, crate::context_menu::PROJECT_ITEM);
                        if chosen == Some("Rename") {
                            self.rename = Some((row.path.clone(), name.clone()));
                        }
                    });
                }
                // Empty area: deselect / context menu.
                let rest = ui.available_rect_before_wrap();
                if rest.height() > 4.0 {
                    let resp = ui
                        .allocate_rect(rest, Sense::click())
                        .labeled(egui::WidgetType::Other, tr("Project Items"));
                    if resp.clicked() {
                        session.lock().selected_items.clear();
                    }
                    common::context_menu(&resp, &mut |ui| {
                        crate::context_menu::show(ui, cx, crate::context_menu::PROJECT_BLANK);
                    });
                }
            });

        // Footer: item count and buttons (AE: interpret, new folder, new comp, delete).
        let (foot, _) = ui.allocate_exact_size(vec2(width, footer_h), Sense::hover());
        ui.painter().hline(
            foot.x_range(),
            foot.top() + 0.5,
            egui::Stroke::new(1.0, color::SEPARATOR),
        );
        let count = rows.iter().filter(|r| r.kind != ItemKind::Folder).count();
        ui.painter().text(
            pos2(foot.left() + 8.0, foot.center().y),
            Align2::LEFT_CENTER,
            format!("{count} {}", tr("items")),
            theme::small(),
            color::TEXT_DIM,
        );
        let mut x = foot.right() - 6.0;
        let mut button = |ui: &mut Ui, icon: Icon, tip: &str| {
            x -= 24.0;
            let r = Rect::from_min_size(pos2(x, foot.top() + 3.0), vec2(22.0, 22.0));
            let resp = ui
                .put(r, |ui: &mut Ui| icons::button(ui, icon, 22.0, false))
                .labeled(egui::WidgetType::Button, tr(tip));
            resp.on_hover_text(tr(tip)).clicked()
        };
        if button(ui, Icon::Trash, "Delete") {
            delete = session.lock().selected_items.clone();
        }
        if button(ui, Icon::Composition, "New Composition") {
            menu_new_comp = true;
        }
        if button(ui, Icon::Folder, "New Folder") {
            menu_new_folder = true;
        }

        // Apply.
        let target_folder = selected_first
            .as_ref()
            .filter(|p| item_kind(stage, p) == Some(ItemKind::Folder))
            .cloned();
        if menu_new_comp {
            common::new_comp(cx, &session, target_folder.as_ref());
            cx.open_panel("composition");
        }
        if menu_new_folder {
            let mut params = json!({"name": common::numbered(stage, &session, target_folder.as_ref().unwrap_or(&PrimPath::root()), "Untitled")});
            if let Some(f) = &target_folder {
                params["parent"] = json!(f);
            }
            cx.invoke_with("folder.create", params);
        }
        if !delete.is_empty() {
            cx.invoke_with("item.delete", json!({"paths": delete}));
            session.lock().selected_items.clear();
        }
        if let Some(asset) = open_footage {
            session.lock().footage_view = Some(asset);
            cx.invoke_with(
                bae_ui::action::ids::OPEN_PANEL,
                serde_json::json!({"id": "footage"}),
            );
        }
        if let Some(c) = open_comp {
            session.lock().open_comp(&c);
            cx.open_panel("timeline");
            cx.open_panel("composition");
        }
        // Files and folders dragged from Finder / Explorer: the panel lights up
        // while they hover over it; a drop imports them into the folder under
        // the pointer (else the default folder), like AE.
        let (hovering, dropped, pointer) = ui.input(|i| {
            (
                !i.raw.hovered_files.is_empty(),
                i.raw
                    .dropped_files
                    .iter()
                    .map(|f| f.path().to_path_buf())
                    .collect::<Vec<_>>(),
                i.pointer.latest_pos(),
            )
        });
        let over = pointer.is_none_or(|p| panel_rect.contains(p));
        if hovering && over {
            ui.painter().rect_stroke(
                panel_rect.shrink(1.0),
                2.0,
                egui::Stroke::new(2.0, color::ACCENT),
                egui::StrokeKind::Inside,
            );
        }
        if !dropped.is_empty() && over {
            let folder = pointer.and_then(|p| {
                folder_rows
                    .iter()
                    .find(|(r, _)| r.contains(p))
                    .map(|(_, f)| f.clone())
            });
            crate::import::import_paths(cx, &dropped, folder);
        }
        // Enter renames the selected item (AE) — unless a focused widget, e.g. a
        // button in another panel, took the Enter.
        if cx.focused
            && !rename_finished
            && self.rename.is_none()
            && bae_ui::panel_key_pressed(ui, egui::Key::Enter)
            && let Some(p) = &selected_first
        {
            self.rename = Some((p.clone(), query::display_name_of(stage, p)));
        }
    }

    fn on_action(&mut self, action: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        let session = common::session(cx.resources);
        // Edit ▸ Find (Ctrl+F): the search field.
        if action.id == "motion.file.find" {
            self.focus_search = true;
            return true;
        }
        if action.id == crate::actions::ids::DELETE {
            let items = session.lock().selected_items.clone();
            if !items.is_empty() {
                cx.invoke_with("item.delete", json!({"paths": items}));
                session.lock().selected_items.clear();
            }
            return true;
        }
        crate::actions::handle(action, cx, &session)
    }
}

fn describe(stage: &Stage, path: &PrimPath, kind: ItemKind) -> (String, String, String) {
    match kind {
        ItemKind::Folder => (tr("Folder").to_owned(), String::new(), String::new()),
        ItemKind::Comp => match query::comp_info(stage, path) {
            Some(info) => (
                tr("Composition").to_owned(),
                format!("{} × {}", info.width, info.height),
                format!("Δ {}", common::duration_code(&info, info.duration)),
            ),
            None => (tr("Composition").to_owned(), String::new(), String::new()),
        },
        ItemKind::Solid | ItemKind::Footage => {
            let v = stage.prim(path);
            let num = |a: &str| {
                v.as_ref()
                    .and_then(|v| query::static_value(v, a))
                    .and_then(|v| v.as_f64())
            };
            let size = match (num("width"), num("height")) {
                (Some(w), Some(h)) => format!("{w} × {h}"),
                _ => String::new(),
            };
            let ty = if kind == ItemKind::Solid {
                "Solid"
            } else {
                "Footage"
            };
            (tr(ty).to_owned(), size, String::new())
        }
    }
}

/// Chinese strings of this module (keys are the English text, see
/// `bae_ui::i18n`).
pub(crate) const ZH: &[(&str, &str)] = &[
    // Project panel
    ("Name", "名称"),
    ("Type", "类型"),
    ("Size", "大小"),
    ("Duration", "持续时间"),
    ("Frame Rate", "帧速率"),
    ("Folder", "文件夹"),
    ("Footage", "素材"),
    ("Solid", "纯色"),
    ("Placeholder", "占位符"),
    ("New Composition", "新建合成"),
    ("New Composition...", "新建合成..."),
    ("Animation", "动画"),
    ("Arrange", "排列"),
    ("Edit", "编辑"),
    ("New Folder", "新建文件夹"),
    ("Open in Timeline", "在时间轴中打开"),
    ("Rename", "重命名"),
    ("Delete", "删除"),
    ("Add to Composition", "添加到合成"),
    ("items", "项"),
    ("Comp", "合成"),
    ("Untitled", "未命名"),
    // Project panel menus (AE 26 zh_CN, MenuID 0238 / 0242 / 0244).
    (
        "New Adobe Photoshop File...",
        "新建 Adobe Photoshop 文件...",
    ),
    (
        "New Maxon Cinema 4D File...",
        "新建 Maxon Cinema 4D 文件...",
    ),
    ("Reveal Project in Finder", "在 Finder 中显示项目"),
    ("New Comp from Selection", "基于所选项新建合成"),
    ("Replace with Precomp", "替换为预合成"),
    ("Reveal in Composition", "在合成中显示"),
    ("Solids Folder", "纯色文件夹"),
    ("Hide This", "隐藏此项"),
    ("Rename This...", "重命名此项..."),
    ("Columns", "列数"),
];

/// Registers this module's Chinese strings and panel (`MotionUiModule::build`).
pub(crate) fn register(app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
    crate::register_panel(
        app,
        "project",
        bae_ui::PanelKind::new("Project", |_| Box::new(ProjectPanel::default())),
    );
}
