//! AE 26's main menus (`docs/reference/ae/menus.json`, Windows), entry by entry in
//! AE's order with AE's separators, each mapped to a BAE action. The macOS
//! application menu follows at the end ([`MAC_APP_MENU`]).
//!
//! Every item is listed, implemented or not: the menu bar shows the ones whose
//! command does not run yet greyed, like AE shows unavailable items. The test in
//! `catalog::tests` walks `menus.json` alongside these tables, so a missing,
//! extra or reordered entry fails.

use super::Owner::{self, *};
use super::Status::{self, *};
use super::{Item, Node, Platform, Submenu};

const fn it(
    ae: u32,
    title: &'static str,
    action: &'static str,
    params: &'static str,
    owner: Owner,
    status: Status,
) -> Node {
    Node::Item(Item {
        ae,
        title,
        action,
        params,
        owner,
        status,
        platform: Platform::All,
        sorted: false,
    })
}

/// An item AE shows on Windows only (macOS: in the application menu).
const fn win(node: Node) -> Node {
    match node {
        Node::Item(mut item) => {
            item.platform = Platform::Windows;
            Node::Item(item)
        }
        other => other,
    }
}

const fn sub(title: &'static str, children: &'static [Node]) -> Node {
    Node::Submenu(Submenu {
        title,
        children,
        platform: Platform::All,
    })
}

/// An item AE sorts by its localized title among the neighbours that are marked
/// the same way in its group (Keyframe Assistant, Export, the Window panel list,
/// workspaces).
const fn s(node: Node) -> Node {
    match node {
        Node::Item(mut item) => {
            item.sorted = true;
            Node::Item(item)
        }
        other => other,
    }
}

/// A submenu AE shows on Windows only (Edit ▸ Preferences).
const fn win_sub(title: &'static str, children: &'static [Node]) -> Node {
    Node::Submenu(Submenu {
        title,
        children,
        platform: Platform::Windows,
    })
}

const SEP: Node = Node::Separator;
const EFFECTS: Node = Node::Effects;

/// The top-level menus in AE's order: `(English title, entries)`.
pub const MENUS: &[(&str, &[Node])] = &[
    ("File", FILE),
    ("Edit", EDIT),
    ("Composition", COMPOSITION),
    ("Layer", LAYER),
    ("Effect", EFFECT),
    ("Animation", ANIMATION),
    ("View", VIEW),
    ("Window", WINDOW),
    ("Help", HELP),
];

/// AE's effect categories (English, as in AE), in English alphabetical order; the
/// menu sorts them by their localized title like AE.
pub const EFFECT_CATEGORIES: &[&str] = &[
    "3D Channel",
    "Audio",
    "Blur & Sharpen",
    "Boris FX Mocha",
    "Channel",
    "Cinema 4D",
    "Color Correction",
    "Distort",
    "Expression Controls",
    "Generate",
    "Immersive Video",
    "Keying",
    "Matte",
    "Noise & Grain",
    "Obsolete",
    "Perspective",
    "Simulation",
    "Stylize",
    "Text",
    "Time",
    "Transition",
    "Utility",
];

/// The macOS application menu (AE 26.2.1, `docs/reference/ae/mac/menus.md`): About,
/// Settings ▸ (the Preferences pages), Services, Hide, Hide Others, Show All, Quit.
/// AE ids are 0: `menus.json` is the Windows menu bar.
#[rustfmt::skip]
pub const MAC_APP_MENU: &[Node] = &[
    it(0, "About BAE...", "app.about", "", B, Done),
    SEP,
    sub("Settings", MAC_SETTINGS),
    SEP,
    it(0, "Services", "app.services", "", B, Done),
    SEP,
    it(0, "Hide BAE", "app.hide", "", B, Done),
    it(0, "Hide Others", "app.hide_others", "", B, Done),
    it(0, "Show All", "app.show_all", "", B, Done),
    SEP,
    it(0, "Quit BAE", "app.quit", "", B, Done),
];

/// The settings pages of the macOS application menu (AE 26: the same pages as
/// Windows' Edit ▸ Preferences, with AE's "Settings" naming).
#[rustfmt::skip]
const MAC_SETTINGS: &[Node] = &[
    it(0, "Startup & Repair...", "app.settings", r#"{"page":"startup_repair"}"#, D, Todo),
    it(0, "Project...", "app.settings", r#"{"page":"project"}"#, D, Todo),
    it(0, "Composition...", "app.settings", r#"{"page":"composition"}"#, D, Todo),
    it(0, "Previews...", "app.settings", r#"{"page":"previews"}"#, D, Todo),
    SEP,
    it(0, "Appearance...", "app.settings", r#"{"page":"appearance"}"#, D, Todo),
    it(0, "Grids & Guides...", "app.settings", r#"{"page":"grids_guides"}"#, D, Todo),
    it(0, "Labels...", "app.settings", r#"{"page":"labels"}"#, D, Todo),
    it(0, "Type...", "app.settings", r#"{"page":"type"}"#, D, Todo),
    SEP,
    it(0, "Import...", "app.settings", r#"{"page":"import"}"#, D, Todo),
    it(0, "Export...", "app.settings", r#"{"page":"export"}"#, D, Todo),
    SEP,
    it(0, "Audio...", "app.settings", r#"{"page":"audio"}"#, D, Todo),
    it(0, "Disk...", "app.settings", r#"{"page":"disk"}"#, D, Todo),
    it(0, "Memory & CPU...", "app.settings", r#"{"page":"memory_cpu"}"#, D, Todo),
    it(0, "Video...", "app.settings", r#"{"page":"video"}"#, D, Todo),
    SEP,
    it(0, "3D...", "app.settings", r#"{"page":"3d"}"#, D, Todo),
    it(0, "Scripting & Expressions...", "app.settings", r#"{"page":"scripting_expressions"}"#, D, Todo),
];

/// File.
#[rustfmt::skip]
const FILE: &[Node] = &[
    sub(
        "New",
        &[
            it(16187393, "New Project", "file.new_project", "", B, Done),
            it(16187394, "New Folder", "motion.file.new_folder", "", B, Done),
            it(16187395, "Adobe Photoshop File...", "motion.file.adobe_photoshop_file", "", B, Adobe),
            it(16187396, "Maxon Cinema 4D File...", "motion.file.maxon_cinema_4d_file", "", B, Adobe),
        ],
    ),
    it(131074, "Open Project...", "file.open_project", "", B, Done),
    sub(
        "Open Recent",
        &[
            it(0, "(recent projects)", "motion.file.open_recent", "", B, Dynamic),
        ],
    ),
    it(131076, "Browse in Bridge...", "motion.file.browse_in_bridge", "", B, Adobe),
    SEP,
    it(131078, "Close", "window.close_panel", "", B, Done),
    it(131079, "Close Project", "file.close_project", "", B, Done),
    it(131080, "Save", "file.save", "", B, Done),
    sub(
        "Save As",
        &[
            it(21889025, "Save As...", "file.save_as", "", B, Done),
            it(21889026, "Save a Copy...", "file.save_a_copy", "", B, Done),
            it(21889027, "Save a Copy As XML...", "motion.file.save_a_copy_as_xml", "", B, Adobe),
            it(21889028, "Save a Copy As 25.x...", "motion.file.save_a_copy_as_25_x", "", B, Adobe),
            it(21889029, "Save a Copy As 24.x...", "motion.file.save_a_copy_as_24_x", "", B, Adobe),
        ],
    ),
    it(131082, "Increment and Save", "file.increment_and_save", "", B, Done),
    it(131083, "Revert", "file.revert", "", B, Done),
    SEP,
    sub(
        "Import",
        &[
            it(9764865, "File...", "motion.file.import_file", "", E, Done),
            it(9764866, "Multiple Files...", "motion.file.import_multiple_files", "", E, Done),
            it(9764867, "From Libraries...", "motion.file.from_libraries", "", B, Adobe),
            it(9764868, "Import Adobe Premiere Project...", "motion.file.import_adobe_premiere_project", "", B, Adobe),
            it(9764869, "Vanishing Point (.vpe)...", "motion.file.vanishing_point_vpe", "", B, Adobe),
            it(9764870, "Placeholder...", "motion.file.import_placeholder", "", E, Todo),
            it(9764871, "Solid...", "motion.file.import_solid", "", E, Todo),
        ],
    ),
    sub(
        "Import Recent Footage",
        &[
            it(0, "(recent footage)", "motion.file.import_recent_footage", "", E, Dynamic),
        ],
    ),
    sub(
        "Export",
        &[
            s(it(11665409, "Maxon Cinema 4D Exporter...", "motion.file.maxon_cinema_4d_exporter", "", B, Adobe)),
            s(it(11665410, "Export Adobe Premiere Project...", "motion.file.export_adobe_premiere_project", "", B, Adobe)),
            s(it(11665411, "Add to Adobe Media Encoder Queue...", "motion.comp.add_to_adobe_media_encoder_queue", "", B, Adobe)),
            s(it(11665412, "Add to Render Queue", "motion.add_to_render_queue", "", B, Done)),
        ],
    ),
    SEP,
    it(131089, "Add Fonts from Adobe...", "motion.file.add_fonts_from_adobe", "", B, Adobe),
    SEP,
    sub(
        "Adobe Dynamic Link",
        &[
            it(12845057, "New Premiere Sequence...", "motion.file.new_premiere_sequence", "", B, Adobe),
            it(12845058, "Import Premiere Sequence...", "motion.file.import_premiere_sequence", "", B, Adobe),
        ],
    ),
    SEP,
    it(131093, "Find", "motion.file.find", "", E, Done),
    SEP,
    it(131095, "Add Footage to Comp", "motion.file.add_footage_to_comp", "", E, Done),
    it(131096, "New Comp from Selection...", "motion.file.new_comp_from_selection", "", E, Done),
    SEP,
    sub(
        "Dependencies",
        &[
            it(22806529, "Collect Files...", "motion.file.collect_files", "", B, Later),
            SEP,
            it(22806531, "Consolidate All Footage", "motion.file.consolidate_all_footage", "", B, Later),
            it(22806532, "Remove Unused Footage", "motion.file.remove_unused_footage", "", B, Later),
            it(22806533, "Reduce Project", "motion.file.reduce_project", "", B, Later),
            SEP,
            it(22806535, "Find Missing Effects", "motion.file.find_missing_effects", "", E, Later),
            it(22806536, "Find Missing Fonts", "motion.file.find_missing_fonts", "", E, Later),
            it(22806537, "Find Missing Footage", "motion.file.find_missing_footage", "", E, Later),
        ],
    ),
    it(131099, "Watch Folder...", "motion.file.watch_folder", "", B, Adobe),
    SEP,
    sub(
        "Scripts",
        &[
            it(917505, "Run Script File...", "motion.file.run_script_file", "", B, Later),
            it(917506, "Install Script File...", "motion.file.install_script_file", "", B, Later),
            it(917507, "Install ScriptUI Panel...", "motion.file.install_scriptui_panel", "", B, Later),
            sub(
                "Recent Script Files",
                &[
                    it(0, "(recent scripts)", "motion.file.recent_script", "", B, Dynamic),
                ],
            ),
            SEP,
            it(917510, "(installed script)", "motion.file.run_script", "", B, Dynamic),
            it(917511, "(installed script)", "motion.file.run_script", "", B, Dynamic),
            it(917512, "(installed script)", "motion.file.run_script", "", B, Dynamic),
            it(917513, "(installed script)", "motion.file.run_script", "", B, Dynamic),
            it(917514, "(installed script)", "motion.file.run_script", "", B, Dynamic),
            it(917515, "(installed script)", "motion.file.run_script", "", B, Dynamic),
            it(917516, "(installed script)", "motion.file.run_script", "", B, Dynamic),
            it(917517, "(installed script)", "motion.file.run_script", "", B, Dynamic),
            it(917518, "(installed script)", "motion.file.run_script", "", B, Dynamic),
            it(917519, "(installed script)", "motion.file.run_script", "", B, Dynamic),
            it(917520, "(installed script)", "motion.file.run_script", "", B, Dynamic),
            it(917521, "(installed script)", "motion.file.run_script", "", B, Dynamic),
        ],
    ),
    SEP,
    sub(
        "Create Proxy",
        &[
            it(10813441, "Still...", "motion.file.create_proxy_still", "", E, Later),
            it(10813442, "Movie...", "motion.file.create_proxy_movie", "", E, Later),
        ],
    ),
    sub(
        "Set Proxy",
        &[
            it(9830401, "File...", "motion.file.set_proxy_file", "", E, Later),
            SEP,
            it(9830403, "None", "motion.file.set_proxy_none", "", E, Later),
        ],
    ),
    sub(
        "Interpret Footage",
        &[
            it(10420225, "Main...", "motion.file.interpret_footage_main", "", E, Done),
            it(10420226, "Proxy...", "motion.file.interpret_footage_proxy", "", E, Later),
            SEP,
            it(10420228, "Remember Interpretation", "motion.file.remember_interpretation", "", E, Done),
            it(10420229, "Apply Interpretation", "motion.file.apply_interpretation", "", E, Done),
        ],
    ),
    sub(
        "Replace Footage",
        &[
            it(11141121, "File...", "motion.file.replace_footage_file", "", E, Done),
            it(11141122, "With Layered Comp", "motion.file.replace_footage_with_layered_comp", "", B, Adobe),
            SEP,
            it(11141124, "Placeholder...", "motion.file.replace_footage_placeholder", "", E, Later),
            it(11141125, "Solid...", "motion.file.replace_footage_solid", "", E, Later),
        ],
    ),
    it(131107, "Reload Footage", "motion.file.reload_footage", "", E, Done),
    it(131108, "License...", "motion.file.license", "", B, Adobe),
    it(131109, "Reveal in Explorer", "motion.file.reveal_in_explorer", "", B, Done),
    it(131110, "Reveal in Bridge", "motion.file.reveal_in_bridge", "", B, Adobe),
    SEP,
    it(131112, "Project Settings...", "motion.file.project_settings", "", E, Done),
    SEP,
    win(it(131114, "Exit", "app.quit", "", B, Done)),
];

/// Edit.
#[rustfmt::skip]
const EDIT: &[Node] = &[
    it(196609, "Undo", "edit.undo", "", B, Done),
    it(196610, "Redo", "edit.redo", "", B, Done),
    sub(
        "History",
        &[
        ],
    ),
    SEP,
    it(196613, "Cut", "edit.cut", "", B, Done),
    it(196614, "Copy", "edit.copy", "", B, Partial),
    it(196615, "Copy with Property Links", "motion.edit.copy_with_property_links", "", B, Later),
    it(196616, "Copy with Relative Property Links", "motion.edit.copy_with_relative_property_links", "", B, Later),
    it(196617, "Copy Expression Only", "motion.edit.copy_expression_only", "", C, Later),
    it(196618, "Paste", "edit.paste", "", B, Partial),
    it(196619, "Paste Reversed Keyframes", "motion.edit.paste_reversed_keyframes", "", C, Todo),
    it(196620, "Paste Text and Match Formatting", "motion.edit.paste_text_and_match_formatting", "", E, Later),
    it(196621, "Paste Text Formatting Only", "motion.edit.paste_text_formatting_only", "", E, Later),
    it(196622, "Clear", "motion.delete", "", B, Done),
    SEP,
    it(196624, "Duplicate", "motion.duplicate", "", B, Done),
    it(196625, "Split Layer", "motion.split", "", B, Done),
    it(196626, "Lift Work Area", "motion.edit.lift_work_area", "", B, Todo),
    it(196627, "Extract Work Area", "motion.edit.extract_work_area", "", B, Todo),
    it(196628, "Select All", "motion.select_all", "", B, Done),
    it(196629, "Deselect All", "motion.deselect_all", "", B, Done),
    SEP,
    sub(
        "Label",
        &[
            it(16646145, "Select Label Group", "motion.edit.select_label_group", "", B, Done),
            SEP,
            it(16646147, "None", "motion.edit.label", r#"{"label":0}"#, B, Done),
            SEP,
            it(16646149, "Red", "motion.edit.label", r#"{"label":1}"#, B, Done),
            it(16646150, "Yellow", "motion.edit.label", r#"{"label":2}"#, B, Done),
            it(16646151, "Aqua", "motion.edit.label", r#"{"label":3}"#, B, Done),
            it(16646152, "Pink", "motion.edit.label", r#"{"label":4}"#, B, Done),
            it(16646153, "Lavender", "motion.edit.label", r#"{"label":5}"#, B, Done),
            it(16646154, "Peach", "motion.edit.label", r#"{"label":6}"#, B, Done),
            it(16646155, "Sea Foam", "motion.edit.label", r#"{"label":7}"#, B, Done),
            it(16646156, "Blue", "motion.edit.label", r#"{"label":8}"#, B, Done),
            it(16646157, "Green", "motion.edit.label", r#"{"label":9}"#, B, Done),
            it(16646158, "Purple", "motion.edit.label", r#"{"label":10}"#, B, Done),
            it(16646159, "Orange", "motion.edit.label", r#"{"label":11}"#, B, Done),
            it(16646160, "Brown", "motion.edit.label", r#"{"label":12}"#, B, Done),
            it(16646161, "Fuchsia", "motion.edit.label", r#"{"label":13}"#, B, Done),
            it(16646162, "Cyan", "motion.edit.label", r#"{"label":14}"#, B, Done),
            it(16646163, "Sandstone", "motion.edit.label", r#"{"label":15}"#, B, Done),
            it(16646164, "Dark Green", "motion.edit.label", r#"{"label":16}"#, B, Done),
            SEP,
            it(16646166, "Edit Label Colors...", "motion.edit.edit_label_colors", "", D, Done),
        ],
    ),
    sub(
        "Select Keyframe Label Group",
        &[
            it(23592961, "On Selected Layers", "motion.edit.select_keyframe_label_group", r#"{"scope":"selected_layers"}"#, C, Done),
            it(23592962, "On All Layers", "motion.edit.select_keyframe_label_group", r#"{"scope":"all_layers"}"#, C, Done),
            SEP,
            it(23592964, "Visible Keyframes on Selected Layers", "motion.edit.select_keyframe_label_group", r#"{"scope":"visible_selected_layers"}"#, C, Partial),
            it(23592965, "Visible Keyframes on All Layers", "motion.edit.select_keyframe_label_group", r#"{"scope":"visible_all_layers"}"#, C, Partial),
        ],
    ),
    SEP,
    sub(
        "Purge",
        &[
            it(11403265, "All Cache...", "motion.edit.purge", r#"{"what":"all_caches"}"#, B, Later),
            it(11403266, "All Memory & Disk Cache...", "motion.edit.purge", r#"{"what":"memory_and_disk_cache"}"#, B, Later),
            it(11403267, "All Memory", "motion.edit.purge", r#"{"what":"all_memory"}"#, B, Later),
            it(11403268, "All Disk Cache...", "motion.edit.purge", r#"{"what":"disk_cache"}"#, B, Later),
            it(11403269, "All 3D Cache...", "motion.edit.purge", r#"{"what":"3d_cache"}"#, B, Later),
            it(11403270, "Undo", "motion.edit.purge", r#"{"what":"undo"}"#, B, Todo),
            it(11403271, "Image Cache Memory", "motion.edit.purge", r#"{"what":"image_cache_memory"}"#, B, Later),
            it(11403272, "Snapshot", "motion.edit.purge", r#"{"what":"snapshot"}"#, B, Later),
        ],
    ),
    SEP,
    it(196636, "Edit Original...", "motion.edit.edit_original", "", B, Done),
    it(196637, "Edit in Adobe Audition", "motion.edit.edit_in_adobe_audition", "", B, Adobe),
    SEP,
    sub(
        "Team Project",
        &[
        ],
    ),
    SEP,
    sub(
        "Templates",
        &[
            it(16252929, "Render Settings...", "motion.edit.templates_render_settings", "", E, Later),
            it(16252930, "Output Module...", "motion.edit.templates_output_module", "", E, Later),
        ],
    ),
    win_sub(
        "Preferences",
        &[
            it(24182785, "Startup & Repair...", "app.settings", r#"{"page":"startup_repair"}"#, D, Todo),
            it(24182786, "Project...", "app.settings", r#"{"page":"project"}"#, D, Todo),
            it(24182787, "Composition...", "app.settings", r#"{"page":"composition"}"#, D, Todo),
            it(24182788, "Previews...", "app.settings", r#"{"page":"previews"}"#, D, Todo),
            SEP,
            it(24182790, "Appearance...", "app.settings", r#"{"page":"appearance"}"#, D, Todo),
            it(24182791, "Grids & Guides...", "app.settings", r#"{"page":"grids_guides"}"#, D, Todo),
            it(24182792, "Labels...", "app.settings", r#"{"page":"labels"}"#, D, Todo),
            it(24182793, "Type...", "app.settings", r#"{"page":"type"}"#, D, Todo),
            SEP,
            it(24182795, "Import...", "app.settings", r#"{"page":"import"}"#, D, Todo),
            it(24182796, "Export...", "app.settings", r#"{"page":"export"}"#, D, Todo),
            SEP,
            it(24182798, "Audio...", "app.settings", r#"{"page":"audio"}"#, D, Todo),
            it(24182799, "Disk...", "app.settings", r#"{"page":"disk"}"#, D, Todo),
            it(24182800, "Memory & CPU...", "app.settings", r#"{"page":"memory_cpu"}"#, D, Todo),
            it(24182801, "Video...", "app.settings", r#"{"page":"video"}"#, D, Todo),
            SEP,
            it(24182803, "3D...", "app.settings", r#"{"page":"3d"}"#, D, Todo),
            it(24182804, "Scripting & Expressions...", "app.settings", r#"{"page":"scripting_expressions"}"#, D, Todo),
        ],
    ),
    it(196643, "Keyboard Shortcuts", "app.keyboard_shortcuts", "", B, Done),
    it(196644, "Paste Mocha Mask", "motion.edit.paste_mocha_mask", "", B, Adobe),
];

/// Composition.
#[rustfmt::skip]
const COMPOSITION: &[Node] = &[
    it(327681, "New Composition...", "motion.new_comp", "", B, Partial),
    SEP,
    it(327683, "Composition Settings...", "motion.comp.composition_settings", "", E, Done),
    it(327684, "Set Poster Time", "motion.comp.set_poster_time", "", B, Todo),
    it(327685, "Trim Comp to Work Area", "motion.trim_comp", "", B, Done),
    it(327686, "Crop Comp to Region of Interest", "motion.comp.crop_comp_to_region_of_interest", "", B, Later),
    it(327687, "Crop Comp to Selected Layer(s) Bounds", "motion.comp.crop_comp_to_selected_layer_bounds", "", B, Later),
    SEP,
    it(327689, "Add to Adobe Media Encoder Queue...", "motion.comp.add_to_adobe_media_encoder_queue", "", B, Adobe),
    it(327690, "Add to Render Queue", "motion.add_to_render_queue", "", B, Done),
    it(327691, "Add Output Module", "motion.comp.add_output_module", "", B, Later),
    SEP,
    sub(
        "Preview",
        &[
            it(10682369, "Play Current Preview", "motion.play", "", B, Done),
            it(10682370, "Cache Frames When Idle", "motion.comp.cache_frames_when_idle", "", B, Done),
            SEP,
            it(10682372, "Audio", "motion.comp.preview_audio", "", B, Later),
        ],
    ),
    sub(
        "Save Frame As",
        &[
            it(15335425, "File...", "motion.comp.save_frame_as_file", "", B, Done),
            SEP,
            it(15335427, "Photoshop Layers...", "motion.comp.save_frame_as_photoshop_layers", "", B, Later),
            it(15335428, "ProEXR...", "motion.comp.save_frame_as_proexr", "", B, Adobe),
        ],
    ),
    it(327695, "Pre-render...", "motion.comp.pre_render", "", B, Later),
    it(327696, "Save Current Preview...", "motion.comp.save_current_preview", "", B, Later),
    SEP,
    it(327698, "Open in Essential Graphics", "motion.comp.open_in_essential_graphics", "", E, Later),
    sub(
        "Responsive Design — Time",
        &[
            it(2097153, "Create Intro", "motion.comp.create_intro", "", B, Later),
            it(2097154, "Create Outro", "motion.comp.create_outro", "", B, Later),
            it(2097155, "Create Protected Region from Work Area", "motion.comp.create_protected_region_from_work_area", "", B, Later),
        ],
    ),
    SEP,
    it(327701, "Composition Flowchart", "motion.comp.composition_flowchart", "", E, Done),
    it(327702, "Composition Mini-Flowchart", "motion.comp.composition_mini_flowchart", "", E, Done),
    SEP,
    sub(
        "VR",
        &[
            it(328466433, "Create VR Environment...", "motion.comp.create_vr_environment", "", B, Later),
            it(328466434, "Extract Cubemap...", "motion.comp.extract_cubemap", "", B, Later),
        ],
    ),
];

/// Layer.
#[rustfmt::skip]
const LAYER: &[Node] = &[
    sub(
        "New",
        &[
            it(11862017, "Text", "motion.new_layer", r#"{"kind":"text"}"#, B, Done),
            it(11862018, "Solid...", "motion.new_layer", r#"{"kind":"solid"}"#, E, Partial),
            it(11862019, "Light...", "motion.new_layer", r#"{"kind":"light"}"#, E, Partial),
            it(11862020, "Camera...", "motion.new_layer", r#"{"kind":"camera"}"#, E, Partial),
            it(11862021, "Null Object", "motion.new_layer", r#"{"kind":"null"}"#, B, Done),
            it(11862022, "Shape Layer", "motion.new_layer", r#"{"kind":"shape"}"#, B, Done),
            it(11862023, "Adjustment Layer", "motion.new_layer", r#"{"kind":"adjustment"}"#, B, Done),
            it(11862024, "Content-Aware Fill Layer...", "motion.layer.new_content_aware_fill_layer", "", E, Later),
            it(11862025, "Adobe Photoshop File...", "motion.layer.new_adobe_photoshop_file", "", B, Adobe),
            it(11862026, "Maxon Cinema 4D File...", "motion.layer.new_maxon_cinema_4d_file", "", B, Adobe),
            SEP,
            it(11862028, "Cube", "motion.layer.new_3d_shape", r#"{"shape":"cube"}"#, B, Later),
            it(11862029, "Sphere", "motion.layer.new_3d_shape", r#"{"shape":"sphere"}"#, B, Later),
            it(11862030, "Plane", "motion.layer.new_3d_shape", r#"{"shape":"plane"}"#, B, Later),
            it(11862031, "Torus", "motion.layer.new_3d_shape", r#"{"shape":"torus"}"#, B, Later),
            it(11862032, "Cone", "motion.layer.new_3d_shape", r#"{"shape":"cone"}"#, B, Later),
            it(11862033, "Cylinder", "motion.layer.new_3d_shape", r#"{"shape":"cylinder"}"#, B, Later),
        ],
    ),
    it(393218, "Layer Settings...", "motion.layer.layer_settings", "", E, Done),
    SEP,
    it(393220, "Open Layer", "motion.layer.open_layer", "", E, Done),
    it(393221, "Open Layer Source", "motion.layer.open_layer_source", "", B, Done),
    it(393222, "Reveal in Explorer", "motion.file.reveal_in_explorer", "", B, Done),
    SEP,
    sub(
        "Mask",
        &[
            it(8519681, "New Mask", "motion.layer.new_mask", "", A, Done),
            SEP,
            it(8519683, "Mask Shape...", "motion.layer.mask_shape", "", E, Done),
            it(8519684, "Mask Feather...", "motion.layer.mask_feather", "", E, Done),
            it(8519685, "Mask Opacity...", "motion.layer.mask_opacity", "", E, Done),
            it(8519686, "Mask Expansion...", "motion.layer.mask_expansion", "", E, Done),
            SEP,
            it(8519688, "Reset Mask", "motion.layer.reset_mask", "", B, Done),
            it(8519689, "Remove Mask", "motion.layer.remove_mask", "", B, Done),
            it(8519690, "Remove All Masks", "motion.layer.remove_all_masks", "", B, Done),
            SEP,
            sub(
                "Mode",
                &[
                    it(11468801, "None", "motion.layer.mask_mode", r#"{"mode":"none"}"#, B, Done),
                    SEP,
                    it(11468803, "Add", "motion.layer.mask_mode", r#"{"mode":"add"}"#, B, Done),
                    it(11468804, "Subtract", "motion.layer.mask_mode", r#"{"mode":"subtract"}"#, B, Done),
                    it(11468805, "Intersect", "motion.layer.mask_mode", r#"{"mode":"intersect"}"#, B, Done),
                    it(11468806, "Lighten", "motion.layer.mask_mode", r#"{"mode":"lighten"}"#, B, Done),
                    it(11468807, "Darken", "motion.layer.mask_mode", r#"{"mode":"darken"}"#, B, Done),
                    it(11468808, "Difference", "motion.layer.mask_mode", r#"{"mode":"difference"}"#, B, Done),
                ],
            ),
            it(8519693, "Inverted", "motion.layer.mask_inverted", "", B, Done),
            it(8519694, "Locked", "motion.layer.mask_locked", "", B, Done),
            sub(
                "Motion Blur",
                &[
                    it(14352385, "Same As Layer", "motion.layer.mask_motion_blur", r#"{"mode":"same_as_layer"}"#, B, Later),
                    it(14352386, "On", "motion.layer.mask_motion_blur", r#"{"mode":"on"}"#, B, Later),
                    it(14352387, "Off", "motion.layer.mask_motion_blur", r#"{"mode":"off"}"#, B, Later),
                ],
            ),
            sub(
                "Feather Falloff",
                &[
                    it(22544385, "Smooth", "motion.layer.mask_feather_falloff", r#"{"falloff":"smooth"}"#, B, Later),
                    it(22544386, "Linear", "motion.layer.mask_feather_falloff", r#"{"falloff":"linear"}"#, B, Later),
                ],
            ),
            SEP,
            it(8519698, "Unlock All Masks", "motion.layer.unlock_all_masks", "", B, Done),
            it(8519699, "Lock Other Masks", "motion.layer.lock_other_masks", "", B, Done),
            it(8519700, "Hide Locked Masks", "motion.layer.hide_locked_masks", "", B, Later),
        ],
    ),
    sub(
        "Mask and Shape Path",
        &[
            it(11010049, "RotoBezier", "motion.layer.rotobezier", "", A, Todo),
            it(11010050, "Closed", "motion.layer.path_closed", "", A, Todo),
            SEP,
            it(11010052, "Convert To Bezier Path", "motion.layer.convert_to_bezier_path", "", A, Todo),
            SEP,
            it(11010054, "Group Shapes", "motion.layer.group_shapes", "", A, Todo),
            it(11010055, "Ungroup Shapes", "motion.layer.ungroup_shapes", "", A, Todo),
            SEP,
            it(11010057, "Set First Vertex", "motion.layer.set_first_vertex", "", A, Todo),
            SEP,
            it(11010059, "Free Transform Points", "motion.layer.free_transform_points", "", A, Todo),
        ],
    ),
    sub(
        "Quality",
        &[
            it(8585217, "Best", "motion.layer.quality", r#"{"quality":"best"}"#, B, Done),
            it(8585218, "Draft", "motion.layer.quality", r#"{"quality":"draft"}"#, B, Done),
            it(8585219, "Wireframe", "motion.layer.quality", r#"{"quality":"wireframe"}"#, B, Todo),
            SEP,
            it(8585221, "Bilinear", "motion.layer.sampling", r#"{"sampling":"bilinear"}"#, B, Todo),
            it(8585222, "Bicubic", "motion.layer.sampling", r#"{"sampling":"bicubic"}"#, B, Todo),
        ],
    ),
    sub(
        "Switches",
        &[
            it(8847361, "Hide Other Video", "motion.layer.hide_other_video", "", B, Done),
            it(8847362, "Show All Video", "motion.layer.show_all_video", "", B, Done),
            it(8847363, "Unlock All Layers", "motion.layer.unlock_all_layers", "", B, Done),
            SEP,
            it(8847365, "Enable Expressions", "motion.layer.enable_expressions", "", C, Later),
            it(8847366, "Disable Expressions", "motion.layer.disable_expressions", "", C, Later),
            SEP,
            it(8847368, "Shy", "motion.layer.switch", r#"{"switch":"shy"}"#, B, Done),
            it(8847369, "Lock", "motion.layer.switch", r#"{"switch":"locked"}"#, B, Done),
            it(8847370, "Audio", "motion.layer.switch", r#"{"switch":"audio_enabled"}"#, B, Done),
            it(8847371, "Video", "motion.layer.switch", r#"{"switch":"enabled"}"#, B, Done),
            it(8847372, "Solo", "motion.layer.switch", r#"{"switch":"solo"}"#, B, Done),
            it(8847373, "Effect", "motion.layer.switch", r#"{"switch":"effects_enabled"}"#, B, Done),
            it(8847374, "Collapse", "motion.layer.switch", r#"{"switch":"collapse_transformation"}"#, B, Done),
            it(8847375, "Motion Blur", "motion.layer.switch", r#"{"switch":"motion_blur"}"#, B, Done),
            it(8847376, "Adjustment Layer", "motion.layer.switch", r#"{"switch":"adjustment_layer"}"#, B, Done),
        ],
    ),
    sub(
        "Transform",
        &[
            it(8912897, "Reset", "motion.layer.reset_transform", "", B, Done),
            SEP,
            it(8912899, "Anchor Point...", "motion.layer.transform_dialog", r#"{"attr":"anchor_point"}"#, E, Done),
            it(8912900, "Position...", "motion.layer.transform_dialog", r#"{"attr":"position"}"#, E, Done),
            it(8912901, "Scale...", "motion.layer.transform_dialog", r#"{"attr":"scale"}"#, E, Done),
            it(8912902, "Orientation...", "motion.layer.transform_dialog", r#"{"attr":"orientation"}"#, E, Done),
            it(8912903, "Rotation...", "motion.layer.transform_dialog", r#"{"attr":"rotation"}"#, E, Done),
            it(8912904, "Opacity...", "motion.layer.transform_dialog", r#"{"attr":"opacity"}"#, E, Done),
            SEP,
            it(8912906, "Flip Horizontal", "motion.layer.flip_horizontal", "", B, Done),
            it(8912907, "Flip Vertical", "motion.layer.flip_vertical", "", B, Done),
            it(8912908, "Center In View", "motion.layer.center_in_view", "", B, Done),
            it(8912909, "Center Anchor Point in Layer Content", "motion.layer.center_anchor_point_in_layer_content", "", B, Done),
            SEP,
            it(8912911, "Fit to Comp", "motion.layer.fit_to_comp", "", B, Done),
            it(8912912, "Fit to Comp Width", "motion.layer.fit_to_comp_width", "", B, Done),
            it(8912913, "Fit to Comp Height", "motion.layer.fit_to_comp_height", "", B, Done),
            SEP,
            it(8912915, "Auto-Orient...", "motion.layer.auto_orient", "", E, Done),
        ],
    ),
    sub(
        "Time",
        &[
            it(1966081, "Enable Time Remapping", "motion.layer.enable_time_remapping", "", B, Done),
            it(1966082, "Time-Reverse Layer", "motion.layer.time_reverse_layer", "", B, Done),
            it(1966083, "Time Stretch...", "motion.layer.time_stretch", "", E, Done),
            it(1966084, "Freeze Frame", "motion.layer.freeze_frame", "", B, Done),
            it(1966085, "Freeze On Last Frame", "motion.layer.freeze_on_last_frame", "", B, Done),
            it(1966086, "Align Video to Data", "motion.layer.align_video_to_data", "", B, Later),
        ],
    ),
    sub(
        "Frame Blending",
        &[
            it(10616833, "Off", "motion.layer.frame_blending", r#"{"mode":"off"}"#, B, Later),
            it(10616834, "Frame Mix", "motion.layer.frame_blending", r#"{"mode":"frame_mix"}"#, B, Later),
            it(10616835, "Pixel Motion", "motion.layer.frame_blending", r#"{"mode":"pixel_motion"}"#, B, Later),
        ],
    ),
    it(393231, "3D Layer", "motion.layer.switch", r#"{"switch":"three_d"}"#, B, Done),
    it(393232, "Guide Layer", "motion.layer.switch", r#"{"switch":"guide"}"#, B, Done),
    it(393233, "Environment Layer", "motion.layer.switch", r#"{"switch":"environment"}"#, B, Later),
    sub(
        "Markers",
        &[
            it(23265281, "Add Marker", "motion.layer.add_marker", "", C, Done),
            it(23265282, "Update Markers From Source", "motion.layer.update_markers_from_source", "", C, Later),
            it(23265283, "Lock Markers", "motion.layer.lock_markers", "", C, Todo),
            it(23265284, "Delete All Markers", "motion.layer.delete_all_markers", "", C, Done),
        ],
    ),
    SEP,
    it(393236, "Preserve Transparency", "motion.layer.switch", r#"{"switch":"preserve_transparency"}"#, B, Done),
    sub(
        "Blending Mode",
        &[
            it(16318465, "Normal", "motion.layer.blending_mode", r#"{"mode":"normal"}"#, B, Done),
            it(16318466, "Dissolve", "motion.layer.blending_mode", r#"{"mode":"dissolve"}"#, B, Done),
            it(16318467, "Dancing Dissolve", "motion.layer.blending_mode", r#"{"mode":"dancing_dissolve"}"#, B, Done),
            SEP,
            it(16318469, "Darken", "motion.layer.blending_mode", r#"{"mode":"darken"}"#, B, Done),
            it(16318470, "Multiply", "motion.layer.blending_mode", r#"{"mode":"multiply"}"#, B, Done),
            it(16318471, "Color Burn", "motion.layer.blending_mode", r#"{"mode":"color_burn"}"#, B, Done),
            it(16318472, "Classic Color Burn", "motion.layer.blending_mode", r#"{"mode":"classic_color_burn"}"#, B, Done),
            it(16318473, "Linear Burn", "motion.layer.blending_mode", r#"{"mode":"linear_burn"}"#, B, Done),
            it(16318474, "Darker Color", "motion.layer.blending_mode", r#"{"mode":"darker_color"}"#, B, Done),
            SEP,
            it(16318476, "Add", "motion.layer.blending_mode", r#"{"mode":"add"}"#, B, Done),
            it(16318477, "Lighten", "motion.layer.blending_mode", r#"{"mode":"lighten"}"#, B, Done),
            it(16318478, "Screen", "motion.layer.blending_mode", r#"{"mode":"screen"}"#, B, Done),
            it(16318479, "Color Dodge", "motion.layer.blending_mode", r#"{"mode":"color_dodge"}"#, B, Done),
            it(16318480, "Classic Color Dodge", "motion.layer.blending_mode", r#"{"mode":"classic_color_dodge"}"#, B, Done),
            it(16318481, "Linear Dodge", "motion.layer.blending_mode", r#"{"mode":"linear_dodge"}"#, B, Done),
            it(16318482, "Lighter Color", "motion.layer.blending_mode", r#"{"mode":"lighter_color"}"#, B, Done),
            SEP,
            it(16318484, "Overlay", "motion.layer.blending_mode", r#"{"mode":"overlay"}"#, B, Done),
            it(16318485, "Soft Light", "motion.layer.blending_mode", r#"{"mode":"soft_light"}"#, B, Done),
            it(16318486, "Hard Light", "motion.layer.blending_mode", r#"{"mode":"hard_light"}"#, B, Done),
            it(16318487, "Linear Light", "motion.layer.blending_mode", r#"{"mode":"linear_light"}"#, B, Done),
            it(16318488, "Vivid Light", "motion.layer.blending_mode", r#"{"mode":"vivid_light"}"#, B, Done),
            it(16318489, "Pin Light", "motion.layer.blending_mode", r#"{"mode":"pin_light"}"#, B, Done),
            it(16318490, "Hard Mix", "motion.layer.blending_mode", r#"{"mode":"hard_mix"}"#, B, Done),
            SEP,
            it(16318492, "Difference", "motion.layer.blending_mode", r#"{"mode":"difference"}"#, B, Done),
            it(16318493, "Classic Difference", "motion.layer.blending_mode", r#"{"mode":"classic_difference"}"#, B, Done),
            it(16318494, "Exclusion", "motion.layer.blending_mode", r#"{"mode":"exclusion"}"#, B, Done),
            it(16318495, "Subtract", "motion.layer.blending_mode", r#"{"mode":"subtract"}"#, B, Done),
            it(16318496, "Divide", "motion.layer.blending_mode", r#"{"mode":"divide"}"#, B, Done),
            SEP,
            it(16318498, "Hue", "motion.layer.blending_mode", r#"{"mode":"hue"}"#, B, Done),
            it(16318499, "Saturation", "motion.layer.blending_mode", r#"{"mode":"saturation"}"#, B, Done),
            it(16318500, "Color", "motion.layer.blending_mode", r#"{"mode":"color"}"#, B, Done),
            it(16318501, "Luminosity", "motion.layer.blending_mode", r#"{"mode":"luminosity"}"#, B, Done),
            SEP,
            it(16318503, "Stencil Alpha", "motion.layer.blending_mode", r#"{"mode":"stencil_alpha"}"#, B, Done),
            it(16318504, "Stencil Luma", "motion.layer.blending_mode", r#"{"mode":"stencil_luma"}"#, B, Done),
            it(16318505, "Silhouette Alpha", "motion.layer.blending_mode", r#"{"mode":"silhouette_alpha"}"#, B, Done),
            it(16318506, "Silhouette Luma", "motion.layer.blending_mode", r#"{"mode":"silhouette_luma"}"#, B, Done),
            SEP,
            it(16318508, "Alpha Add", "motion.layer.blending_mode", r#"{"mode":"alpha_add"}"#, B, Done),
            it(16318509, "Luminescent Premul", "motion.layer.blending_mode", r#"{"mode":"luminescent_premul"}"#, B, Done),
        ],
    ),
    it(393238, "Next Blending Mode", "motion.layer.blending_mode_step", r#"{"step":1}"#, B, Done),
    it(393239, "Previous Blending Mode", "motion.layer.blending_mode_step", r#"{"step":-1}"#, B, Done),
    sub(
        "Track Matte",
        &[
            it(16384001, "No Track Matte", "motion.layer.track_matte", r#"{"matte":"none"}"#, B, Todo),
            SEP,
            it(16384003, "Alpha Matte", "motion.layer.track_matte", r#"{"matte":"alpha"}"#, B, Todo),
            it(16384004, "Alpha Inverted Matte", "motion.layer.track_matte", r#"{"matte":"alpha_inverted"}"#, B, Todo),
            it(16384005, "Luma Matte", "motion.layer.track_matte", r#"{"matte":"luma"}"#, B, Todo),
            it(16384006, "Luma Inverted Matte", "motion.layer.track_matte", r#"{"matte":"luma_inverted"}"#, B, Todo),
            SEP,
            sub(
                "Matte with Layer Above",
                &[
                    it(23658497, "Alpha Matte", "motion.layer.track_matte", r#"{"matte":"alpha","source":"above"}"#, B, Todo),
                    it(23658498, "Alpha Inverted Matte", "motion.layer.track_matte", r#"{"matte":"alpha_inverted","source":"above"}"#, B, Todo),
                    it(23658499, "Luma Matte", "motion.layer.track_matte", r#"{"matte":"luma","source":"above"}"#, B, Todo),
                    it(23658500, "Luma Inverted Matte", "motion.layer.track_matte", r#"{"matte":"luma_inverted","source":"above"}"#, B, Todo),
                ],
            ),
            sub(
                "Matte with Layer Below",
                &[
                    it(23724033, "Alpha Matte", "motion.layer.track_matte", r#"{"matte":"alpha","source":"below"}"#, B, Todo),
                    it(23724034, "Alpha Inverted Matte", "motion.layer.track_matte", r#"{"matte":"alpha_inverted","source":"below"}"#, B, Todo),
                    it(23724035, "Luma Matte", "motion.layer.track_matte", r#"{"matte":"luma","source":"below"}"#, B, Todo),
                    it(23724036, "Luma Inverted Matte", "motion.layer.track_matte", r#"{"matte":"luma_inverted","source":"below"}"#, B, Todo),
                ],
            ),
        ],
    ),
    sub(
        "Layer Styles",
        &[
        ],
    ),
    SEP,
    sub(
        "Arrange",
        &[
            it(12910593, "Bring to Front", "motion.order", r#"{"to":"front"}"#, B, Done),
            it(12910594, "Bring Forward", "motion.order", r#"{"to":"forward"}"#, B, Done),
            it(12910595, "Send Backward", "motion.order", r#"{"to":"backward"}"#, B, Done),
            it(12910596, "Send to Back", "motion.order", r#"{"to":"back"}"#, B, Done),
        ],
    ),
    SEP,
    sub(
        "Reveal",
        &[
            it(23003137, "Reveal in Explorer", "motion.file.reveal_in_explorer", "", B, Done),
            it(23003138, "Reveal Layer Source in Project", "motion.layer.reveal_layer_source_in_project", "", B, Done),
            it(23003139, "Reveal Layer in Project Flowchart", "motion.layer.reveal_layer_in_project_flowchart", "", E, Later),
            it(23003140, "Reveal Composition in Project", "motion.layer.reveal_composition_in_project", "", B, Done),
            it(23003141, "Reveal Expression Errors", "motion.layer.reveal_expression_errors", "", C, Later),
        ],
    ),
    sub(
        "Create",
        &[
            it(23068673, "Convert to Editable Text", "motion.layer.convert_to_editable_text", "", B, Later),
            it(23068674, "Create Shapes from Text", "motion.layer.create_shapes_from_text", "", A, Later),
            it(23068675, "Create Masks from Text", "motion.layer.create_masks_from_text", "", A, Later),
            it(23068676, "Create Shapes from Vector Layer", "motion.layer.create_shapes_from_vector_layer", "", A, Later),
            it(23068677, "Create Keyframes from Data", "motion.anim.create_keyframes_from_data", "", B, Later),
            it(23068678, "Null Controllers for Positional Points", "motion.anim.null_controllers_for_positional_points", "", B, Later),
            it(23068679, "Null Controllers for Path Points", "motion.anim.null_controllers_for_path_points", "", B, Later),
            it(23068680, "Nulls Following Path Points", "motion.anim.nulls_following_path_points", "", B, Later),
            it(23068681, "Null Tracing Along Path", "motion.anim.null_tracing_along_path", "", B, Later),
            it(23068682, "Create 3D Layer Instance", "motion.layer.create_3d_layer_instance", "", B, Later),
        ],
    ),
    SEP,
    sub(
        "Camera",
        &[
            it(21954561, "Create Camera from 3D View", "motion.view.create_camera_from_3d_view", "", A, Later),
            it(21954562, "Create Stereo 3D Rig", "motion.layer.create_stereo_3d_rig", "", B, Later),
            it(21954563, "Create Orbit Null", "motion.layer.create_orbit_null", "", B, Later),
            it(21954564, "Create Cameras from 3D Model", "motion.layer.create_cameras_from_3d_model", "", B, Later),
            SEP,
            it(21954566, "Link Focus Distance to Point of Interest", "motion.layer.link_focus_distance_to_point_of_interest", "", B, Later),
            it(21954567, "Link Focus Distance to Layer", "motion.layer.link_focus_distance_to_layer", "", B, Later),
            it(21954568, "Set Focus Distance to Layer", "motion.layer.set_focus_distance_to_layer", "", B, Later),
            SEP,
            it(21954570, "Camera Settings...", "motion.layer.camera_settings", "", E, Done),
            it(21954571, "Reset 3D View", "motion.view.reset_3d_view", "", A, Later),
        ],
    ),
    sub(
        "Light",
        &[
            it(328990721, "Create Lights from 3D Model", "motion.layer.create_lights_from_3d_model", "", B, Later),
            it(328990722, "Control Light with Camera", "motion.layer.control_light_with_camera", "", B, Later),
            it(328990723, "Create Environment Light Background Layer", "motion.layer.create_environment_light_background_layer", "", B, Later),
        ],
    ),
    it(393250, "Auto-trace...", "motion.layer.auto_trace", "", B, Later),
    it(393251, "Pre-compose...", "motion.layer.pre_compose", "", E, Done),
    it(393252, "Scene Edit Detection...", "motion.layer.scene_edit_detection", "", B, Later),
];

/// Effect.
#[rustfmt::skip]
const EFFECT: &[Node] = &[
    it(458753, "Effect Controls", "window.panel", r#"{"id":"effect_controls","toggle":true}"#, B, Done),
    it(458754, "Last Effect", "motion.effect.last_effect", "", B, Done),
    it(458755, "Remove All", "motion.effect.remove_all", "", B, Done),
    SEP,
    it(458757, "Manage Effects...", "motion.effect.manage_effects", "", E, Done),
    SEP,
    EFFECTS,
];

/// Animation.
#[rustfmt::skip]
const ANIMATION: &[Node] = &[
    it(851969, "Track in Boris FX Mocha", "motion.anim.track_in_boris_fx_mocha", "", B, Adobe),
    it(851970, "Save Animation Preset...", "motion.anim.save_animation_preset", "", E, Done),
    it(851971, "Apply Animation Preset...", "motion.anim.apply_animation_preset", "", E, Done),
    sub(
        "Recent Animation Presets",
        &[
            it(11599873, "(recent presets)", "motion.anim.recent_animation_preset", "", E, Dynamic),
        ],
    ),
    it(851973, "Browse Presets...", "motion.anim.browse_presets", "", E, Partial),
    SEP,
    it(851975, "Set Keyframe", "motion.anim.set_keyframe", "", B, Todo),
    it(851976, "Toggle Hold Keyframe", "motion.toggle_hold", "", B, Done),
    it(851977, "Keyframe Interpolation...", "motion.anim.keyframe_interpolation", "", C, Done),
    it(851978, "Keyframe Velocity...", "motion.anim.keyframe_velocity", "", C, Done),
    sub(
        "Keyframe Assistant",
        &[
            s(it(11075585, "RPF Camera Import", "motion.anim.rpf_camera_import", "", B, Adobe)),
            s(it(11075586, "Create Keyframes from Data", "motion.anim.create_keyframes_from_data", "", B, Later)),
            s(it(11075587, "Null Controllers for Positional Points", "motion.anim.null_controllers_for_positional_points", "", B, Later)),
            s(it(11075588, "Convert Expression to Keyframes", "motion.anim.convert_expression_to_keyframes", "", C, Later)),
            s(it(11075589, "Convert Audio to Keyframes", "motion.anim.convert_audio_to_keyframes", "", B, Done)),
            s(it(11075590, "Sequence Layers...", "motion.anim.sequence_layers", "", E, Done)),
            s(it(11075591, "Exponential Scale", "motion.anim.exponential_scale", "", B, Done)),
            s(it(11075592, "Time-Reverse Keyframes", "motion.reverse_keys", "", B, Done)),
            s(it(11075593, "Null Tracing Along Path", "motion.anim.null_tracing_along_path", "", B, Later)),
            s(it(11075594, "Nulls Following Path Points", "motion.anim.nulls_following_path_points", "", B, Later)),
            s(it(11075595, "Easy Ease In", "motion.ease", r#"{"mode":"ease_in"}"#, B, Done)),
            s(it(11075596, "Easy Ease Out", "motion.ease", r#"{"mode":"ease_out"}"#, B, Done)),
            s(it(11075597, "Easy Ease", "motion.ease", r#"{"mode":"easy_ease"}"#, B, Done)),
            s(it(11075598, "Null Controllers for Path Points", "motion.anim.null_controllers_for_path_points", "", B, Later)),
        ],
    ),
    it(851980, "Add Property to Essential Graphics", "motion.anim.add_property_to_essential_graphics", "", E, Later),
    SEP,
    sub(
        "Animate Text",
        &[
            it(1179649, "Enable Per-character 3D", "motion.anim.enable_per_character_3d", "", A, Later),
            SEP,
            it(1179651, "Anchor Point", "motion.anim.animate_text", r#"{"property":"anchor_point"}"#, A, Later),
            it(1179652, "Position", "motion.anim.animate_text", r#"{"property":"position"}"#, A, Done),
            it(1179653, "Scale", "motion.anim.animate_text", r#"{"property":"scale"}"#, A, Done),
            it(1179654, "Skew", "motion.anim.animate_text", r#"{"property":"skew"}"#, A, Later),
            it(1179655, "Rotation", "motion.anim.animate_text", r#"{"property":"rotation"}"#, A, Done),
            it(1179656, "Opacity", "motion.anim.animate_text", r#"{"property":"opacity"}"#, A, Done),
            it(1179657, "All Transform Properties", "motion.anim.animate_text", r#"{"property":"all_transform"}"#, A, Later),
            SEP,
            sub(
                "Fill Color",
                &[
                    it(1310721, "RGB", "motion.anim.animate_text", r#"{"property":"fill_rgb"}"#, A, Done),
                    it(1310722, "Hue", "motion.anim.animate_text", r#"{"property":"fill_hue"}"#, A, Later),
                    it(1310723, "Saturation", "motion.anim.animate_text", r#"{"property":"fill_saturation"}"#, A, Later),
                    it(1310724, "Brightness", "motion.anim.animate_text", r#"{"property":"fill_brightness"}"#, A, Later),
                    it(1310725, "Opacity", "motion.anim.animate_text", r#"{"property":"fill_opacity"}"#, A, Later),
                ],
            ),
            sub(
                "Stroke Color",
                &[
                    it(1376257, "RGB", "motion.anim.animate_text", r#"{"property":"stroke_rgb"}"#, A, Later),
                    it(1376258, "Hue", "motion.anim.animate_text", r#"{"property":"stroke_hue"}"#, A, Later),
                    it(1376259, "Saturation", "motion.anim.animate_text", r#"{"property":"stroke_saturation"}"#, A, Later),
                    it(1376260, "Brightness", "motion.anim.animate_text", r#"{"property":"stroke_brightness"}"#, A, Later),
                    it(1376261, "Opacity", "motion.anim.animate_text", r#"{"property":"stroke_opacity"}"#, A, Later),
                ],
            ),
            it(1179661, "Stroke Width", "motion.anim.animate_text", r#"{"property":"stroke_width"}"#, A, Later),
            SEP,
            it(1179663, "Tracking", "motion.anim.animate_text", r#"{"property":"tracking"}"#, A, Done),
            it(1179664, "Line Anchor", "motion.anim.animate_text", r#"{"property":"line_anchor"}"#, A, Later),
            it(1179665, "Line Spacing", "motion.anim.animate_text", r#"{"property":"line_spacing"}"#, A, Later),
            SEP,
            it(1179667, "Character Offset", "motion.anim.animate_text", r#"{"property":"character_offset"}"#, A, Later),
            it(1179668, "Character Value", "motion.anim.animate_text", r#"{"property":"character_value"}"#, A, Later),
            SEP,
            it(1179670, "Blur", "motion.anim.animate_text", r#"{"property":"blur"}"#, A, Later),
            SEP,
            sub(
                "Variable Font Axes",
                &[
                ],
            ),
        ],
    ),
    sub(
        "Add Text Selector",
        &[
            it(1245185, "Range", "motion.anim.add_text_selector", r#"{"selector":"range"}"#, A, Done),
            it(1245186, "Wiggly", "motion.anim.add_text_selector", r#"{"selector":"wiggly"}"#, A, Later),
            it(1245187, "Expression", "motion.anim.add_text_selector", r#"{"selector":"expression"}"#, A, Later),
        ],
    ),
    it(851984, "Remove All Text Animators", "motion.anim.remove_all_text_animators", "", A, Done),
    SEP,
    it(851986, "Add Expression", "motion.anim.add_expression", "", C, Todo),
    it(851987, "Separate Dimensions", "motion.anim.separate_dimensions", "", B, Done),
    it(851988, "Track Camera", "motion.anim.track_camera", "", B, Later),
    it(851989, "Warp Stabilizer VFX", "motion.anim.warp_stabilizer_vfx", "", B, Later),
    it(851990, "Track Motion", "motion.anim.track_motion", "", B, Done),
    it(851991, "Track Mask", "motion.anim.track_mask", "", B, Later),
    it(851992, "Track this Property", "motion.anim.track_this_property", "", B, Later),
    SEP,
    it(851994, "Reveal Properties with Keyframes", "motion.reveal", r#"{"what":"animated","add":false}"#, B, Done),
    it(851995, "Reveal Properties with Animation", "motion.reveal", r#"{"what":"animation","add":false}"#, B, Done),
    it(851996, "Reveal All Modified Properties", "motion.reveal", r#"{"what":"modified","add":false}"#, B, Done),
];

/// View.
#[rustfmt::skip]
const VIEW: &[Node] = &[
    it(589825, "Split with New Locked Viewer", "motion.view.split_with_new_locked_viewer", "", A, Later),
    SEP,
    it(589827, "Zoom In", "viewer.zoom_in", "", A, Done),
    it(589828, "Zoom Out", "viewer.zoom_out", "", A, Done),
    SEP,
    sub(
        "Resolution",
        &[
            it(8650753, "Full", "motion.view.resolution", r#"{"factor":1}"#, B, Todo),
            it(8650754, "Half", "motion.view.resolution", r#"{"factor":2}"#, B, Todo),
            it(8650755, "Third", "motion.view.resolution", r#"{"factor":3}"#, B, Todo),
            it(8650756, "Quarter", "motion.view.resolution", r#"{"factor":4}"#, B, Todo),
            SEP,
            it(8650758, "Custom...", "motion.view.resolution_custom", "", E, Later),
        ],
    ),
    SEP,
    it(589832, "Use Display Color Management", "motion.view.use_display_color_management", "", B, Later),
    sub(
        "Simulate Output",
        &[
            it(12517377, "No Output Simulation", "motion.view.simulate_output", r#"{"profile":"none"}"#, B, Later),
            SEP,
            it(12517379, "HDTV (Rec. 709)", "motion.view.simulate_output", r#"{"profile":"rec709"}"#, B, Later),
            it(12517380, "SDTV NTSC", "motion.view.simulate_output", r#"{"profile":"ntsc"}"#, B, Later),
            it(12517381, "SDTV PAL", "motion.view.simulate_output", r#"{"profile":"pal"}"#, B, Later),
            it(12517382, "Legacy Macintosh RGB (Gamma 1.8)", "motion.view.simulate_output", r#"{"profile":"legacy_mac"}"#, B, Later),
            it(12517383, "Internet Standard RGB (sRGB)", "motion.view.simulate_output", r#"{"profile":"srgb"}"#, B, Later),
            it(12517384, "Kodak 5218 to Kodak 2383", "motion.view.simulate_output", r#"{"profile":"kodak_5218_2383"}"#, B, Later),
            it(12517385, "Universal Camera Film to Kodak 2383", "motion.view.simulate_output", r#"{"profile":"camera_film_2383"}"#, B, Later),
            SEP,
            it(12517387, "My Custom RGB", "motion.view.simulate_output", r#"{"profile":"custom_rgb"}"#, B, Later),
            it(12517388, "Custom...", "motion.view.simulate_output_custom", "", B, Later),
        ],
    ),
    SEP,
    it(589835, "Show Rulers", "motion.view.show_rulers", "", B, Todo),
    sub(
        "Panel Background Color",
        &[
            it(24051713, "Black", "motion.view.panel_background_color", r#"{"color":"black"}"#, B, Later),
            it(24051714, "Dark Gray", "motion.view.panel_background_color", r#"{"color":"dark_gray"}"#, B, Later),
            it(24051715, "Medium Gray (Default)", "motion.view.panel_background_color", r#"{"color":"medium_gray"}"#, B, Later),
            it(24051716, "Light Gray", "motion.view.panel_background_color", r#"{"color":"light_gray"}"#, B, Later),
            it(24051717, "White", "motion.view.panel_background_color", r#"{"color":"white"}"#, B, Later),
            it(24051718, "Custom", "motion.view.panel_background_color", r#"{"color":"custom"}"#, B, Later),
            SEP,
            it(24051720, "Select Custom Background Color...", "motion.view.select_custom_background_color", "", B, Later),
        ],
    ),
    SEP,
    it(589838, "Show Guides", "motion.view.show_guides", "", B, Todo),
    it(589839, "Snap to Guides", "motion.view.snap_to_guides", "", B, Todo),
    it(589840, "Lock Guides", "motion.view.lock_guides", "", B, Todo),
    it(589841, "Clear Guides", "motion.view.clear_guides", "", B, Todo),
    it(589842, "Import Guides...", "motion.view.import_guides", "", B, Later),
    it(589843, "Export Guides...", "motion.view.export_guides", "", B, Later),
    SEP,
    it(589845, "Show Grid", "motion.view.show_grid", "", B, Todo),
    it(589846, "Snap to Grid", "motion.view.snap_to_grid", "", B, Todo),
    SEP,
    it(589848, "View Options...", "motion.view.view_options", "", E, Later),
    it(589849, "Show Layer Controls", "motion.view.show_layer_controls", "", B, Done),
    SEP,
    it(589851, "Camera Settings...", "motion.layer.camera_settings", "", E, Done),
    it(589852, "Reset 3D View", "motion.view.reset_3d_view", "", A, Later),
    it(589853, "Create Camera from 3D View", "motion.view.create_camera_from_3d_view", "", A, Later),
    sub(
        "Switch View Layout",
        &[
            it(23396353, "1 View", "motion.view.view_layout", r#"{"views":1}"#, A, Later),
            it(23396354, "2 Views", "motion.view.view_layout", r#"{"views":2}"#, A, Later),
            it(23396355, "4 Views", "motion.view.view_layout", r#"{"views":4}"#, A, Later),
            SEP,
            it(23396357, "Share View Options", "motion.view.share_view_options", "", A, Later),
        ],
    ),
    sub(
        "Switch 3D View",
        &[
            it(12058625, "Active Camera (Default)", "motion.view.switch_3d_view", r#"{"view":"active_camera"}"#, A, Later),
            SEP,
            it(12058627, "Default", "motion.view.switch_3d_view", r#"{"view":"default"}"#, A, Later),
            SEP,
            it(12058629, "Front", "motion.view.switch_3d_view", r#"{"view":"front"}"#, A, Later),
            it(12058630, "Left", "motion.view.switch_3d_view", r#"{"view":"left"}"#, A, Later),
            it(12058631, "Top", "motion.view.switch_3d_view", r#"{"view":"top"}"#, A, Later),
            it(12058632, "Back", "motion.view.switch_3d_view", r#"{"view":"back"}"#, A, Later),
            it(12058633, "Right", "motion.view.switch_3d_view", r#"{"view":"right"}"#, A, Later),
            it(12058634, "Bottom", "motion.view.switch_3d_view", r#"{"view":"bottom"}"#, A, Later),
            SEP,
            it(12058636, "Custom View 1", "motion.view.switch_3d_view", r#"{"view":"custom_1"}"#, A, Later),
            it(12058637, "Custom View 2", "motion.view.switch_3d_view", r#"{"view":"custom_2"}"#, A, Later),
            it(12058638, "Custom View 3", "motion.view.switch_3d_view", r#"{"view":"custom_3"}"#, A, Later),
            SEP,
            it(12058640, "Reset 3D View", "motion.view.reset_3d_view", "", A, Later),
        ],
    ),
    sub(
        "Assign 3D View Shortcut",
        &[
            it(11993089, "A", "motion.view.assign_3d_view_shortcut", r#"{"slot":"a"}"#, A, Later),
            it(11993090, "B", "motion.view.assign_3d_view_shortcut", r#"{"slot":"b"}"#, A, Later),
            it(11993091, "C", "motion.view.assign_3d_view_shortcut", r#"{"slot":"c"}"#, A, Later),
        ],
    ),
    it(589857, "Switch to Last 3D View", "motion.view.switch_to_last_3d_view", "", A, Later),
    it(589858, "Look at Selected Layers", "motion.view.look_at_selected_layers", "", A, Later),
    it(589859, "Look at All Layers", "motion.view.look_at_all_layers", "", A, Later),
    SEP,
    it(589861, "Go to Time...", "motion.view.go_to_time", "", B, Done),
];

/// Window.
#[rustfmt::skip]
const WINDOW: &[Node] = &[
    sub(
        "Workspace",
        &[
            s(it(9961473, "Animation", "workspace.switch", r#"{"id":"ae_animation"}"#, B, Done)),
            s(it(9961474, "Essential Graphics", "workspace.switch", r#"{"id":"ae_essential_graphics"}"#, B, Done)),
            s(it(9961475, "Learn", "workspace.switch", r#"{"id":"ae_learn"}"#, B, Done)),
            s(it(9961476, "Review", "workspace.switch", r#"{"id":"review"}"#, B, Done)),
            s(it(9961477, "Small Screen", "workspace.switch", r#"{"id":"ae_small_screen"}"#, B, Done)),
            s(it(9961478, "Libraries", "workspace.switch", r#"{"id":"ae_libraries"}"#, B, Done)),
            s(it(9961479, "All Panels", "workspace.switch", r#"{"id":"ae_all_panels"}"#, B, Done)),
            s(it(9961480, "Effects", "workspace.switch", r#"{"id":"ae_effects"}"#, B, Done)),
            s(it(9961481, "Text", "workspace.switch", r#"{"id":"ae_text"}"#, B, Done)),
            s(it(9961482, "Standard", "workspace.switch", r#"{"id":"ae_standard"}"#, B, Done)),
            s(it(9961483, "Undocked Panels", "workspace.switch", r#"{"id":"ae_undocked_panels"}"#, B, Done)),
            s(it(9961484, "Minimal", "workspace.switch", r#"{"id":"ae_minimal"}"#, B, Done)),
            s(it(9961485, "Paint", "workspace.switch", r#"{"id":"ae_paint"}"#, B, Done)),
            s(it(9961486, "Motion Tracking", "workspace.switch", r#"{"id":"ae_motion_tracking"}"#, B, Done)),
            s(it(9961487, "Color", "workspace.switch", r#"{"id":"ae_color"}"#, B, Done)),
            s(it(9961488, "Default", "workspace.switch", r#"{"id":"motion"}"#, B, Done)),
            SEP,
            it(9961490, "Reset to Saved Layout", "workspace.reset", "", B, Done),
            it(9961491, "Save Changes to this Workspace", "workspace.save", "", B, Done),
            it(9961492, "Save as New Workspace...", "workspace.new", "", B, Done),
            SEP,
            it(9961494, "Edit Workspaces...", "workspace.edit", "", B, Done),
        ],
    ),
    sub(
        "Assign Shortcut to Workspace",
        &[
            it(12779521, "Shift+F10", "workspace.assign_shortcut", r#"{"slot":0}"#, B, Done),
            it(12779522, "Shift+F11", "workspace.assign_shortcut", r#"{"slot":1}"#, B, Done),
            it(12779523, "Shift+F12", "workspace.assign_shortcut", r#"{"slot":2}"#, B, Done),
        ],
    ),
    SEP,
    it(524292, "Find Extensions on Exchange...", "motion.window.find_extensions_on_exchange", "", B, Adobe),
    SEP,
    sub(
        "Extensions",
        &[
            it(0, "(extensions)", "motion.window.extension", "", B, Dynamic),
        ],
    ),
    SEP,
    s(it(524296, "Lumetri Scopes", "window.panel", r#"{"id":"lumetri_scopes","toggle":true}"#, E, Later)),
    s(it(524297, "Review with Frame.io", "window.panel", r#"{"id":"frame_io","toggle":true}"#, B, Done)),
    s(it(524298, "Info", "window.panel", r#"{"id":"info","toggle":true}"#, E, Todo)),
    s(it(524299, "Metadata", "window.panel", r#"{"id":"metadata","toggle":true}"#, E, Later)),
    s(it(524300, "Content-Aware Fill", "window.panel", r#"{"id":"content_aware_fill","toggle":true}"#, E, Later)),
    s(it(524301, "Motion Sketch", "window.panel", r#"{"id":"motion_sketch","toggle":true}"#, E, Later)),
    s(it(524302, "Essential Graphics", "window.panel", r#"{"id":"essential_graphics","toggle":true}"#, E, Later)),
    s(it(524303, "Media Browser", "window.panel", r#"{"id":"media_browser","toggle":true}"#, E, Later)),
    s(it(524304, "Character", "window.panel", r#"{"id":"character","toggle":true}"#, E, Todo)),
    s(it(524305, "Learn", "window.panel", r#"{"id":"learn","toggle":true}"#, E, Later)),
    s(it(524306, "Align", "window.panel", r#"{"id":"align","toggle":true}"#, E, Todo)),
    s(it(524307, "Properties", "window.panel", r#"{"id":"properties","toggle":true}"#, B, Done)),
    s(it(524308, "Tools", "window.panel", r#"{"id":"tools","toggle":true}"#, A, Todo)),
    s(it(524309, "Smoother", "window.panel", r#"{"id":"smoother","toggle":true}"#, E, Later)),
    s(it(524310, "Libraries", "window.panel", r#"{"id":"libraries","toggle":true}"#, B, Adobe)),
    s(it(524311, "Wiggler", "window.panel", r#"{"id":"wiggler","toggle":true}"#, E, Later)),
    s(it(524312, "Effects & Presets", "window.panel", r#"{"id":"effects","toggle":true}"#, E, Done)),
    s(it(524313, "Paragraph", "window.panel", r#"{"id":"paragraph","toggle":true}"#, E, Todo)),
    s(it(524314, "Brushes", "window.panel", r#"{"id":"brushes","toggle":true}"#, E, Later)),
    s(it(524315, "Paint", "window.panel", r#"{"id":"paint","toggle":true}"#, E, Later)),
    s(it(524316, "Mask Interpolation", "window.panel", r#"{"id":"mask_interpolation","toggle":true}"#, E, Later)),
    s(it(524317, "Tracker", "window.panel", r#"{"id":"tracker","toggle":true}"#, E, Later)),
    s(it(524318, "Progress", "window.panel", r#"{"id":"progress","toggle":true}"#, E, Later)),
    s(it(524319, "Audio", "window.panel", r#"{"id":"audio","toggle":true}"#, E, Done)),
    s(it(524320, "Preview", "window.panel", r#"{"id":"preview","toggle":true}"#, B, Done)),
    SEP,
    s(it(524322, "Composition", "window.panel", r#"{"id":"composition","toggle":true}"#, B, Done)),
    s(it(524323, "Layer", "window.panel", r#"{"id":"layer","toggle":true}"#, E, Later)),
    s(it(524324, "Effect Controls", "window.panel", r#"{"id":"effect_controls","toggle":true}"#, B, Done)),
    s(it(524325, "Timeline", "window.panel", r#"{"id":"timeline","toggle":true}"#, B, Done)),
    s(it(524326, "Flowchart", "window.panel", r#"{"id":"flowchart","toggle":true}"#, E, Later)),
    s(it(524327, "Render Queue", "window.panel", r#"{"id":"render_queue","toggle":true}"#, B, Done)),
    s(it(524328, "Footage", "window.panel", r#"{"id":"footage","toggle":true}"#, E, Later)),
    s(it(524329, "Project", "window.panel", r#"{"id":"project","toggle":true}"#, B, Done)),
    SEP,
    win(it(524331, "Create Nulls From Paths.jsx", "window.panel", r#"{"id":"create_nulls_from_paths","toggle":true}"#, B, Done)),
    win(it(524338, "VR Comp Editor.jsx", "window.panel", r#"{"id":"vr_comp_editor","toggle":true}"#, B, Done)),
];

/// Help.
#[rustfmt::skip]
const HELP: &[Node] = &[
    win(it(720897, "About BAE...", "app.about", "", B, Done)),
    SEP,
    it(720899, "BAE Help...", "help.docs", "", B, Done),
    it(720900, "BAE In-App Tutorials...", "motion.help.in_app_tutorials", "", B, Later),
    it(720901, "BAE Online Tutorials...", "motion.help.online_tutorials", "", B, Later),
    it(720902, "Scripting Help...", "motion.help.scripting_help", "", B, Later),
    it(720903, "Expression Reference...", "motion.help.expression_reference", "", B, Later),
    it(720904, "Effect Reference...", "motion.help.effect_reference", "", B, Later),
    it(720905, "Animation Presets...", "motion.help.animation_presets", "", B, Later),
    it(720906, "Keyboard Shortcuts...", "motion.help.keyboard_shortcuts", "", B, Todo),
    SEP,
    it(720908, "System Compatibility Report...", "motion.help.system_compatibility_report", "", B, Later),
    it(720909, "Enable Logging", "motion.help.enable_logging", "", B, Todo),
    it(720910, "Reveal Logging File", "motion.help.reveal_logging_file", "", B, Todo),
    SEP,
    it(720912, "Online User Forums...", "motion.help.online_user_forums", "", B, Later),
    it(720913, "Provide Feedback...", "motion.help.provide_feedback", "", B, Todo),
];
