//! The menu bar of the motion workspace: After Effects 26's main menus, built from
//! the command catalog ([`crate::catalog`]) entry by entry, with AE's order,
//! separators and submenus (and, on macOS, the application menu), plus the
//! workspace's global action handler, so menus and AE shortcuts work whichever
//! panel has focus.
//!
//! Like AE, every item is shown: an item whose command does not run yet stays
//! greyed until some module handles its action ([`MenuItemSpec::needs_handler`]);
//! an item that runs is enabled when AE enables it (an open composition,
//! selected layers, selected keyframes … see [`Need`]). Window items show a
//! check mark for open panels, Workspace items for the active workspace.

use std::collections::HashSet;
use std::sync::Mutex;

use bae_ui::menu::{APP_MENU, MenuContext, MenuItemSpec, MenuPos};
use bae_ui::{ActionHandler, Invocation, PanelContext};

use crate::actions;
use crate::catalog::{self, Item, Node, Status};
use crate::common;

/// Workspace id of the motion workspace (priority for conflicts).
pub const WORKSPACE: &str = "motion";

/// What an item needs to be enabled (AE greys the item otherwise).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    Nothing,
    /// An active composition.
    Comp,
    /// Selected layers in the active composition.
    Layers,
    /// Selected keyframes.
    Keys,
    /// Selected layers or keyframes (Edit ▸ Copy, Cut, Clear).
    LayersOrKeys,
    /// Selected Project panel items (File ▸ Interpret Footage, Replace Footage …).
    Items,
    /// An active composition and something on the clipboard (Edit ▸ Paste).
    Paste,
}

/// The state the enabled checks look at, computed once per menu update.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Facts {
    comp: bool,
    layers: bool,
    keys: bool,
    items: bool,
    /// Something on the clipboard (layers or keyframes).
    clip: bool,
    /// Bit `i`: every selected layer has `actions::SWITCHES[i]` on.
    switches: u16,
    /// The selected layers' common quality (index in [`QUALITIES`] + 1; 0 =
    /// mixed or none).
    quality: u8,
}

/// Qualities with a check mark in Layer ▸ Quality.
const QUALITIES: &[&str] = &["best", "draft", "wireframe"];

impl Facts {
    fn compute(cx: &MenuContext<'_>) -> Facts {
        let session = common::session(cx.resources);
        let comp = common::active_comp(cx.stage, &session).is_some();
        let s = session.lock();
        let selected = &s.selected_layers;
        let mut switches = 0u16;
        let mut quality = 0u8;
        if comp && !selected.is_empty() {
            for (i, (attr, _)) in actions::SWITCHES.iter().enumerate() {
                if selected
                    .iter()
                    .all(|l| actions::switch_on(cx.stage, l, attr))
                {
                    switches |= 1 << i;
                }
            }
            let first = actions::quality_of(cx.stage, &selected[0]);
            if selected
                .iter()
                .all(|l| actions::quality_of(cx.stage, l) == first)
            {
                quality = QUALITIES
                    .iter()
                    .position(|q| *q == first)
                    .map_or(0, |i| i as u8 + 1);
            }
        }
        Facts {
            comp,
            layers: comp && !selected.is_empty(),
            keys: comp && !s.selected_keys.is_empty(),
            items: !s.selected_items.is_empty(),
            clip: s.clipboard.is_some() || !s.layer_clipboard.is_empty(),
            switches,
            quality,
        }
    }

    /// The facts of `cx`'s update: computed by the first item that asks, shared
    /// by the others (`generation` 0 = always computed).
    fn of(cx: &MenuContext<'_>) -> Facts {
        static CACHE: Mutex<Option<(u64, usize, Facts)>> = Mutex::new(None);
        if cx.generation == 0 {
            return Facts::compute(cx);
        }
        let key = (cx.generation, std::ptr::from_ref(cx.stage) as usize);
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        match *cache {
            Some((g, s, facts)) if (g, s) == key => facts,
            _ => {
                let facts = Facts::compute(cx);
                *cache = Some((key.0, key.1, facts));
                facts
            }
        }
    }

    fn meets(self, need: Need) -> bool {
        match need {
            Need::Nothing => true,
            Need::Comp => self.comp,
            Need::Layers => self.layers,
            Need::Keys => self.keys,
            Need::LayersOrKeys => self.layers || self.keys,
            Need::Items => self.items,
            Need::Paste => self.comp && self.clip,
        }
    }
}

/// When AE enables an item (`path`: English menu path, top-level menu first).
pub fn need(path: &[&str], item: &Item) -> Need {
    use Need::*;
    match item.action {
        "edit.copy" | "edit.cut" | "motion.delete" => return LayersOrKeys,
        "edit.paste" => return Paste,
        "motion.select_all" | "motion.deselect_all" => return Comp,
        "motion.duplicate" | "motion.split" | "motion.order" => return Layers,
        "motion.ease" | "motion.toggle_hold" | "motion.reverse_keys" => return Keys,
        "motion.new_comp" => return Nothing,
        "motion.new_layer"
        | "motion.trim_comp"
        | "motion.add_to_render_queue"
        | "motion.play"
        | "motion.reveal" => return Comp,
        "motion.anim.keyframe_interpolation" | "motion.anim.keyframe_velocity" => return Keys,
        "motion.edit.lift_work_area"
        | "motion.edit.extract_work_area"
        | "motion.edit.paste_reversed_keyframes"
        | "motion.edit.select_keyframe_label_group"
        | "motion.layer.show_all_video"
        | "motion.layer.unlock_all_layers" => return Comp,
        "motion.edit.label" | "motion.edit.select_label_group" => return Layers,
        "motion.file.reveal_in_explorer" if path[0] == "Layer" => return Layers,
        _ => {}
    }
    if item.action.starts_with("window.")
        || item.action.starts_with("workspace.")
        || item.action.starts_with("app.")
        || item.action.starts_with("help.")
        || item.action.starts_with("file.")
    {
        return Nothing;
    }
    match path[0] {
        "File" => match item.action {
            "motion.file.add_footage_to_comp"
            | "motion.file.new_comp_from_selection"
            | "motion.file.reload_footage"
            | "motion.file.reveal_in_explorer"
            | "motion.file.reveal_in_bridge" => Items,
            a if a.starts_with("motion.file.interpret_footage")
                || a.starts_with("motion.file.replace_footage")
                || a.starts_with("motion.file.set_proxy")
                || a.starts_with("motion.file.create_proxy")
                || a.ends_with("_interpretation") =>
            {
                Items
            }
            _ => Nothing,
        },
        "Composition" | "View" => Comp,
        "Layer" if path.get(1) == Some(&"New") => Comp,
        "Layer" | "Effect" => Layers,
        "Animation" => match item.action {
            "motion.anim.convert_audio_to_keyframes" => Comp,
            a if a.starts_with("motion.anim.recent_animation_preset")
                || a == "motion.anim.browse_presets"
                || a == "motion.anim.apply_animation_preset" =>
            {
                Nothing
            }
            _ => Layers,
        },
        _ => Nothing,
    }
}

/// Registry ids the menu items had before the catalog drove the menu bar; kept
/// so scripts and tests that click them keep working.
const LEGACY_IDS: &[(&str, &str)] = &[
    ("File/New/New Project", "shell.file.new_project"),
    ("File/Open Project...", "shell.file.open_project"),
    ("File/Close Project", "shell.file.close_project"),
    ("File/Save", "shell.file.save"),
    ("File/Save As/Save As...", "shell.file.save_as"),
    ("Edit/Duplicate", "motion.edit.duplicate"),
    ("Edit/Split Layer", "motion.edit.split"),
    ("Composition/New Composition...", "motion.composition.new"),
    (
        "Composition/Trim Comp to Work Area",
        "motion.composition.trim",
    ),
    (
        "Composition/Add to Render Queue",
        "motion.composition.render_queue",
    ),
    ("Layer/New/Text", "motion.layer.new.text"),
    ("Layer/New/Solid...", "motion.layer.new.solid"),
    ("Layer/New/Light...", "motion.layer.new.light"),
    ("Layer/New/Camera...", "motion.layer.new.camera"),
    ("Layer/New/Null Object", "motion.layer.new.null"),
    ("Layer/New/Shape Layer", "motion.layer.new.shape"),
    ("Layer/New/Adjustment Layer", "motion.layer.new.adjustment"),
    ("Layer/Arrange/Bring to Front", "motion.layer.arrange.front"),
    (
        "Layer/Arrange/Bring Forward",
        "motion.layer.arrange.forward",
    ),
    (
        "Layer/Arrange/Send Backward",
        "motion.layer.arrange.backward",
    ),
    ("Layer/Arrange/Send to Back", "motion.layer.arrange.back"),
    (
        "Animation/Keyframe Assistant/Easy Ease",
        "motion.animation.ease",
    ),
    (
        "Animation/Keyframe Assistant/Easy Ease In",
        "motion.animation.ease_in",
    ),
    (
        "Animation/Keyframe Assistant/Easy Ease Out",
        "motion.animation.ease_out",
    ),
    (
        "Animation/Keyframe Assistant/Time-Reverse Keyframes",
        "motion.animation.reverse",
    ),
    ("Animation/Toggle Hold Keyframe", "motion.animation.hold"),
    ("Window/Project", "shell.window.project"),
    ("Window/Composition", "shell.window.composition"),
    ("Window/Timeline", "shell.window.timeline"),
    ("Window/Effect Controls", "shell.window.effect_controls"),
    ("Window/Effects & Presets", "shell.window.effects"),
    ("Window/Preview", "shell.window.preview"),
    ("Window/Properties", "shell.window.properties"),
    ("Window/Info", "shell.window.info"),
    ("Window/Render Queue", "motion.window.render_queue"),
    // E's dialogs and import (their ids from before the catalog menu bar).
    ("File/Import/File...", "motion.file.import_file"),
    (
        "File/Import/Multiple Files...",
        "motion.file.import_multiple_files",
    ),
    (
        "Composition/Composition Settings...",
        "motion.comp.composition_settings",
    ),
    ("Layer/Layer Settings...", "motion.layer.layer_settings"),
    ("Layer/Pre-compose...", "motion.layer.pre_compose"),
];

/// `snake_case` of an English menu title (`"Save As..."` → `save_as`).
fn snake(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

/// Registry id of a menu item: its legacy id, or `menu.<path>.<title>`.
fn item_id(path: &[&str], title: &str) -> String {
    let key = format!("{}/{title}", path.join("/"));
    if let Some((_, id)) = LEGACY_IDS.iter().find(|(k, _)| *k == key) {
        return (*id).to_owned();
    }
    let mut id = String::from("menu");
    for p in path.iter().copied().chain([title]) {
        id.push('.');
        id.push_str(&snake(p));
    }
    id
}

/// Builds the specs of one menu level.
struct Builder {
    out: Vec<(String, MenuItemSpec)>,
    ids: HashSet<String>,
}

impl Builder {
    fn push(&mut self, id: String, spec: MenuItemSpec) {
        // Run-time list placeholders repeat (AE's installed scripts); one is enough.
        if self.ids.insert(id.clone()) {
            self.out.push((id, spec));
        }
    }

    /// `levels`: positions of the enclosing submenus, top-level menu first.
    fn level(&mut self, path: &mut Vec<&'static str>, levels: &[MenuPos], nodes: &'static [Node]) {
        let (mut group, mut order) = (0, 0);
        for node in nodes {
            order += 1;
            let pos = MenuPos::new(group, order);
            match node {
                Node::Separator => group += 1,
                Node::Item(item) if item.platform.current() => {
                    let pos = if item.sorted { pos.sorted() } else { pos };
                    let mut at = levels.to_vec();
                    at.push(pos);
                    let id = item_id(path, item.title);
                    let spec = spec(path, item, at);
                    self.push(id, spec);
                }
                Node::Submenu(sub) if sub.platform.current() => {
                    path.push(sub.title);
                    let mut at = levels.to_vec();
                    at.push(pos);
                    if sub.children.is_empty() {
                        // AE shows the submenu (History, Team Project) greyed.
                        self.placeholder(path, &at);
                    } else {
                        self.level(path, &at, sub.children);
                    }
                    path.pop();
                }
                Node::Effects => {
                    // One submenu per category, sorted by its localized name like
                    // AE, listing AE's effects (greyed unless BAE implements them).
                    for category in catalog::effect_categories() {
                        path.push(category);
                        let mut at = levels.to_vec();
                        at.push(pos.sorted());
                        let items = crate::effects::menu_items(path, &at, category, WORKSPACE);
                        if items.is_empty() {
                            self.placeholder(path, &at);
                        }
                        for (title, spec) in items {
                            self.push(item_id(path, title), spec);
                        }
                        path.pop();
                    }
                }
                _ => {}
            }
        }
    }

    /// An entry that only makes the submenu `path` exist.
    fn placeholder(&mut self, path: &[&'static str], levels: &[MenuPos]) {
        let mut at = levels.to_vec();
        at.push(MenuPos::new(0, 0));
        let spec = MenuItemSpec::new(path, "(empty)", "")
            .workspace(WORKSPACE)
            .at(at)
            .placeholder();
        self.push(item_id(path, "(empty)"), spec);
    }
}

/// The menu item of a catalog entry.
fn spec(path: &[&str], item: &'static Item, levels: Vec<MenuPos>) -> MenuItemSpec {
    let mut spec = MenuItemSpec::new(path, catalog::shown_title(item.title), item.action)
        .workspace(WORKSPACE)
        .at(levels)
        .mnemonic(catalog::item_mnemonic(item.ae));
    if !item.params.is_empty() {
        spec = spec.params(serde_json::from_str(item.params).expect("catalog params are JSON"));
    }
    match item.status {
        Status::Dynamic => return spec.placeholder(),
        // A further variant of a command that runs (Reveal All Modified
        // Properties next to Reveal Properties with Keyframes): greyed until the
        // catalog says it works.
        s if !s.works() && catalog::working_actions().contains(item.action) => {
            return spec.enabled_when(|_| false);
        }
        s if !s.works() => spec = spec.needs_handler(),
        _ => {}
    }
    let need = need(path, item);
    if need != Need::Nothing {
        spec = spec.enabled_when(move |cx| Facts::of(cx).meets(need));
    }
    // Layer Settings needs a layer that has settings (solid, adjustment,
    // camera, light) or a solid item in the Project panel.
    if item.action == actions::ids::LAYER_SETTINGS {
        spec = spec.enabled_when(|cx| {
            let session = common::session(cx.resources);
            actions::layer_settings_target(cx.stage, &session, true).is_some()
        });
    }
    let id_param = || {
        spec_param(item.params, "id")
            .map(str::to_owned)
            .unwrap_or_default()
    };
    match item.action {
        crate::effects::LAST_EFFECT => spec.enabled_when(crate::effects::last_effect_enabled),
        crate::effects::REMOVE_ALL => spec.enabled_when(crate::effects::remove_all_enabled),
        actions::ids::LAYER_SWITCH => {
            let attr = spec_param(item.params, "switch").unwrap_or_default();
            let bit = actions::SWITCHES
                .iter()
                .position(|(a, _)| *a == attr)
                .map_or(0, |i| 1u16 << i);
            spec.checked_when(move |cx| Facts::of(cx).switches & bit != 0)
        }
        actions::ids::QUALITY => {
            let q = spec_param(item.params, "quality").unwrap_or_default();
            let code = QUALITIES
                .iter()
                .position(|x| *x == q)
                .map_or(u8::MAX, |i| i as u8 + 1);
            spec.checked_when(move |cx| Facts::of(cx).quality == code)
        }
        crate::viewer::view_options::ids::PANEL_BACKGROUND => {
            let key = spec_param(item.params, "color")
                .unwrap_or_default()
                .to_owned();
            spec.checked_when(move |cx| {
                crate::viewer::view_options::background_checked(cx.resources, &key)
            })
        }
        crate::viewer::view3d::ids::CREATE_CAMERA => {
            spec.enabled_when(|cx| crate::viewer::view3d::can_create_camera(cx.resources))
        }

        id if crate::viewer::guides::ACTIONS.iter().any(|(a, _)| *a == id)
            && id != "motion.view.clear_guides" =>
        {
            spec.checked_when(move |cx| crate::viewer::guides::checked(cx.resources, id))
        }

        crate::viewer::view3d::ids::LAYOUT => {
            let n = item
                .params
                .trim_start_matches("{\"views\":")
                .trim_end_matches('}');
            let n: usize = n.parse().unwrap_or(1);
            spec.checked_when(move |cx| crate::viewer::view3d::layout(cx.resources) == n)
        }

        "motion.view.look_at_selected_layers" => {
            spec.enabled_when(|cx| crate::viewer::look_at_enabled(cx.resources, false))
        }
        "motion.view.look_at_all_layers" => {
            spec.enabled_when(|cx| crate::viewer::look_at_enabled(cx.resources, true))
        }
        crate::viewer::view3d::ids::SWITCH => {
            let view = spec_param(item.params, "view").unwrap_or_default();
            spec.checked_when(move |cx| crate::viewer::view3d::current(cx.resources).key() == view)
        }
        crate::masks::ids::MASK_MODE => {
            let mode = spec_param(item.params, "mode").unwrap_or_default();
            spec.checked_when(move |cx| {
                let session = common::session(cx.resources);
                crate::masks::first_target(cx.stage, &session)
                    .is_some_and(|m| crate::masks::mode(cx.stage, &m) == mode)
            })
        }
        crate::masks::ids::MASK_INVERTED | crate::masks::ids::MASK_LOCKED => {
            let locked = item.action == crate::masks::ids::MASK_LOCKED;
            spec.checked_when(move |cx| {
                let session = common::session(cx.resources);
                crate::masks::first_target(cx.stage, &session).is_some_and(|m| {
                    if locked {
                        crate::masks::locked(cx.stage, &m)
                    } else {
                        crate::masks::inverted(cx.stage, &m)
                    }
                })
            })
        }
        actions::ids::TIME_REMAP => spec.checked_when(|cx| {
            let session = common::session(cx.resources);
            let first = session.lock().selected_layers.first().cloned();
            first.is_some_and(|l| actions::switch_on(cx.stage, &l, "time_remap_enabled"))
        }),
        bae_ui::action::ids::OPEN_PANEL => {
            let panel = id_param();
            let title = item.title;
            let spec = match panel.as_str() {
                // AE: "Composition: Comp 1", "Timeline: Comp 1" ("（无）" without).
                "composition" | "timeline" | "layer" | "footage" | "flowchart"
                | "effect_controls" => {
                    let which = panel.clone();
                    spec.label_when(move |cx| Some(panel_label(cx, title, &which)))
                }
                _ => spec,
            };
            spec.checked_when(move |cx| cx.panel_open(&panel))
        }
        bae_ui::action::ids::SWITCH_WORKSPACE => {
            let workspace = id_param();
            spec.checked_when(move |cx| cx.workspace == workspace)
        }
        _ => spec,
    }
}

/// The Window menu title of a viewer-like panel: "<panel>: <what it shows>".
fn panel_label(cx: &MenuContext<'_>, title: &str, panel: &str) -> String {
    let session = common::session(cx.resources);
    let shown = match panel {
        "composition" | "timeline" => common::active_comp(cx.stage, &session).map(|c| c.name),
        "effect_controls" | "layer" => session
            .lock()
            .selected_layers
            .first()
            .map(|l| bae_motion::query::display_name_of(cx.stage, l)),
        _ => None,
    };
    let none = bae_ui::tr("(none)");
    format!(
        "{}: {}",
        bae_ui::tr(title),
        shown.as_deref().unwrap_or(none)
    )
}

/// A string parameter of a catalog entry's parameter text.
fn spec_param<'a>(params: &'a str, key: &str) -> Option<&'a str> {
    let at = params.find(&format!("\"{key}\":\""))? + key.len() + 4;
    let rest = &params[at..];
    rest.find('"').map(|end| &rest[..end])
}

/// `(registry id, item)` of every menu item: AE's menus in AE's order, the macOS
/// application menu on macOS. Each runs the same invocation its AE shortcut
/// binding carries, so the menu shows that shortcut.
pub fn items() -> Vec<(String, MenuItemSpec)> {
    let mut b = Builder {
        out: Vec::new(),
        ids: HashSet::new(),
    };
    let app = cfg!(target_os = "macos").then_some((APP_MENU, catalog::mac_app_menu()));
    for (top, nodes) in app.into_iter().chain(catalog::menus().iter().copied()) {
        let mut path = vec![top];
        b.level(&mut path, &[], nodes);
    }
    b.out
}

/// Runs the `motion.*` actions whichever panel has focus.
#[derive(Debug, Default)]
pub struct MotionActions;

impl ActionHandler for MotionActions {
    fn actions(&self) -> Vec<String> {
        actions::ACTIONS
            .iter()
            .map(|(id, _)| (*id).to_owned())
            .collect()
    }

    fn workspace(&self) -> Option<&str> {
        Some(WORKSPACE)
    }

    fn handle(&self, inv: &Invocation, cx: &mut PanelContext<'_>) -> bool {
        let session = common::session(cx.resources);
        actions::handle(inv, cx, &session)
    }
}

/// Chinese strings of this module (keys are the English text, see
/// `bae_ui::i18n`).
pub(crate) const ZH: &[(&str, &str)] = &[
    ("Text Orientation", "文字方向"),
    // New layer menu
    ("New", "新建"),
    ("Text", "文本"),
    ("Shape Layer", "形状图层"),
    ("Null Object", "空对象"),
    ("Null", "空"),
    ("Camera", "摄像机"),
    ("Light", "灯光"),
    ("Adjustment Layer", "调整图层"),
    ("Rectangle", "矩形"),
    ("Ellipse", "椭圆"),
    ("Star", "星形"),
    ("Pre-comp", "预合成"),
];

/// Registers this module's Chinese strings (`MotionUiModule::build`).
pub(crate) fn register(_app: &mut bae_app::AppBuilder) {
    bae_ui::i18n::register(ZH);
}
