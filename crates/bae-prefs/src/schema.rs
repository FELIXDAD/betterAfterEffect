//! What the Preferences dialog shows: pages, groups, rows and controls, with AE's
//! English labels (also the i18n keys), value types, ranges, units and defaults.
//!
//! Every control that stores a value is a [`Pref`] with a dotted key
//! (`general.levels_of_undo`). `ae` names the matching entry in AE's preferences
//! file (`docs/reference/ae/preferences.json`, `Section/Key`) when there is one.

/// Value type and valid range of a preference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// Checkbox.
    Bool,
    /// Integer field.
    Int { min: i64, max: i64 },
    /// Number field with `decimals` digits.
    Float { min: f64, max: f64, decimals: u8 },
    /// Slider from `min` to `max` with labels at both ends (Appearance).
    Slider {
        min: f64,
        max: f64,
        left: &'static str,
        right: &'static str,
    },
    /// Dropdown (or radio buttons when `radio`); the value is the English option text.
    Choice {
        options: &'static [&'static str],
        radio: bool,
    },
    /// Label index 0..=16 shown as a dropdown of label names with swatches.
    Label,
    /// `#rrggbb` color swatch.
    Color,
    /// Free text field.
    Text,
    /// Folder or file path with a "Choose…" button.
    Path { button: &'static str },
    /// Duration in seconds, shown as a timecode field.
    Duration { max: f64 },
}

/// Default value of a preference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DefaultValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(&'static str),
}

/// Platforms a control appears on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Both,
    Mac,
    Windows,
}

impl Os {
    /// Whether it appears on the platform BAE is running on.
    pub fn current(self) -> bool {
        match self {
            Os::Both => true,
            Os::Mac => cfg!(target_os = "macos"),
            Os::Windows => !cfg!(target_os = "macos"),
        }
    }
}

/// Condition that enables (un-grays) a control.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum When {
    /// A boolean preference is on.
    On(&'static str),
    /// A choice preference has this option.
    Is(&'static str, &'static str),
}

/// One stored setting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pref {
    /// Dotted key, `page.name`.
    pub key: &'static str,
    /// English label as AE shows it (also the i18n key). Fields end with `:`.
    pub label: &'static str,
    pub kind: Kind,
    pub default: DefaultValue,
    /// Unit after the field ("minutes", "%"), English, translated.
    pub unit: &'static str,
    /// `Section/Key` in AE's preferences file.
    pub ae: &'static str,
    pub enabled: Option<When>,
    pub os: Os,
    /// `false`: BAE stores the value but no feature uses it yet (docs mark it
    /// "暂不生效").
    pub live: bool,
}

impl Pref {
    const fn new(
        key: &'static str,
        label: &'static str,
        kind: Kind,
        default: DefaultValue,
    ) -> Self {
        Pref {
            key,
            label,
            kind,
            default,
            unit: "",
            ae: "",
            enabled: None,
            os: Os::Both,
            live: false,
        }
    }
    const fn unit(mut self, unit: &'static str) -> Self {
        self.unit = unit;
        self
    }
    const fn ae(mut self, ae: &'static str) -> Self {
        self.ae = ae;
        self
    }
    const fn when(mut self, key: &'static str) -> Self {
        self.enabled = Some(When::On(key));
        self
    }
    const fn when_is(mut self, key: &'static str, option: &'static str) -> Self {
        self.enabled = Some(When::Is(key, option));
        self
    }
    const fn os(mut self, os: Os) -> Self {
        self.os = os;
        self
    }
    /// A feature uses this preference.
    const fn live(mut self) -> Self {
        self.live = true;
        self
    }
}

const fn b(key: &'static str, label: &'static str, on: bool) -> Pref {
    Pref::new(key, label, Kind::Bool, DefaultValue::Bool(on))
}
const fn int(key: &'static str, label: &'static str, v: i64, min: i64, max: i64) -> Pref {
    Pref::new(key, label, Kind::Int { min, max }, DefaultValue::Int(v))
}
const fn num(key: &'static str, label: &'static str, v: f64, min: f64, max: f64, d: u8) -> Pref {
    Pref::new(
        key,
        label,
        Kind::Float {
            min,
            max,
            decimals: d,
        },
        DefaultValue::Float(v),
    )
}
const fn choice(
    key: &'static str,
    label: &'static str,
    options: &'static [&'static str],
    v: &'static str,
) -> Pref {
    Pref::new(
        key,
        label,
        Kind::Choice {
            options,
            radio: false,
        },
        DefaultValue::Str(v),
    )
}
const fn radio(
    key: &'static str,
    label: &'static str,
    options: &'static [&'static str],
    v: &'static str,
) -> Pref {
    Pref::new(
        key,
        label,
        Kind::Choice {
            options,
            radio: true,
        },
        DefaultValue::Str(v),
    )
}
const fn label(key: &'static str, text: &'static str, v: i64) -> Pref {
    Pref::new(key, text, Kind::Label, DefaultValue::Int(v))
}
const fn color(key: &'static str, text: &'static str, v: &'static str) -> Pref {
    Pref::new(key, text, Kind::Color, DefaultValue::Str(v))
}
const fn text(key: &'static str, label: &'static str, v: &'static str) -> Pref {
    Pref::new(key, label, Kind::Text, DefaultValue::Str(v))
}
const fn path(key: &'static str, label: &'static str, button: &'static str) -> Pref {
    Pref::new(key, label, Kind::Path { button }, DefaultValue::Str(""))
}
const fn slider(key: &'static str, label: &'static str, v: f64) -> Pref {
    Pref::new(
        key,
        label,
        Kind::Slider {
            min: 0.0,
            max: 1.0,
            left: "Darker",
            right: "Lighter",
        },
        DefaultValue::Float(v),
    )
}
const fn dur(key: &'static str, label: &'static str, seconds: f64) -> Pref {
    Pref::new(
        key,
        label,
        Kind::Duration { max: 10800.0 },
        DefaultValue::Float(seconds),
    )
}

/// A piece of a page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Item {
    /// A stored setting.
    P(Pref),
    /// Controls on one line.
    Row(&'static [Item]),
    /// A titled box.
    Group(&'static str, &'static [Item]),
    /// Push button: action id (handled by the host) and English label.
    Button(&'static str, &'static str),
    /// Read-only text (English, translated; `{…}` placeholders are filled by the host).
    Note(&'static str),
    /// Item that only appears on one platform.
    Only(Os, &'static Item),
    /// Vertical gap.
    Gap,
}

/// A page of the dialog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Page {
    /// Variant parameter of `app.settings` (`general`).
    pub id: &'static str,
    /// English page name in the list (and the Mac Settings submenu).
    pub title: &'static str,
    pub items: &'static [Item],
}

/// Label names in label order (index 0 is None).
pub const LABEL_NAMES: [&str; 17] = [
    "None",
    "Red",
    "Yellow",
    "Aqua",
    "Pink",
    "Lavender",
    "Peach",
    "Sea Foam",
    "Blue",
    "Green",
    "Purple",
    "Orange",
    "Brown",
    "Fuchsia",
    "Cyan",
    "Sandstone",
    "Dark Green",
];

/// Default label colors as BAE draws them (`bae_ui::theme::LABELS`, measured from
/// AE 2026 on screen). Index 0 (None) is not editable.
pub const LABEL_COLORS: [&str; 17] = [
    "#3a3a3a", "#a7413d", "#d6cd5a", "#a5c6c2", "#dcb9c5", "#a5a6c7", "#debf9f", "#b0c3b0",
    "#6b7cd9", "#529c4e", "#863793", "#de8f2b", "#774730", "#e571cf", "#459da0", "#a19276",
    "#2c432c",
];

macro_rules! label_rows {
    ($($i:literal),*) => {
        &[$(Item::Row(&[
            Item::P(color(concat!("labels.color_", $i), "", LABEL_COLORS[$i])
                .ae(concat!("Label Preference Color Section 5/Label Color ID 2 # ", $i)).live()),
            Item::P(text(concat!("labels.name_", $i), "", LABEL_NAMES[$i])
                .ae(concat!("Label Preference Text Section 7/Label Text ID 2 # ", $i)).live()),
        ])),*]
    };
}

const GRID_STYLES: &[&str] = &["Lines", "Dashed Lines", "Dots"];
const GUIDE_STYLES: &[&str] = &["Lines", "Dashed Lines"];

/// All pages, in AE 26's order (Edit > Preferences, macOS: After Effects > Settings).
/// Storage keys keep their own prefixes (`general.*`, `auto_save.*`): they are not page ids.
pub static PAGES: &[Page] = &[
    Page {
        id: "startup_repair",
        title: "Startup & Repair",
        items: &[
            Item::P(b("general.show_home_screen", "Show Home Screen at Launch", true)
                .ae("General Section/Show Welcome Screen")),
            Item::P(
                int("general.levels_of_undo", "Levels of Undo:", 99, 1, 99)
                    .ae("General Section/Levels of Undo v2")
                    .live(),
            ),
            Item::P(b("general.show_tool_tips", "Show Tool Tips", true).ae("Main Pref Section v2/Pref_TOOL_TIPS")),
            Item::P(
                b("general.system_color_picker", "Use System Color Picker", false)
                    .ae("Main Pref Section v2/Use Adobe Color Picker"),
            ),
            Item::P(
                b("general.system_shortcuts", "Use System Shortcut Keys", false).os(Os::Mac),
            ),
            Item::Gap,
            Item::Only(
                Os::Windows,
                &Item::Button("general.reveal", "Reveal Preferences in Explorer"),
            ),
            Item::Only(
                Os::Mac,
                &Item::Button("general.reveal", "Reveal Preferences in Finder"),
            ),
        ],
    },
    Page {
        id: "project",
        title: "Project",
        items: &[
            Item::P(b("general.synchronize_time", "Synchronize Time of All Related Items", true)
                .ae("General Section/Synchronize Time")),
            Item::P(b("display.disable_thumbnails", "Disable Thumbnails in Project Panel", false)
                .ae("Main Pref Section v2/Pref_RENDER_THUMBNAILS")),
            Item::Group(
                "Auto-Save",
                &[
                    Item::P(b("auto_save.enabled", "Automatically Save Projects", true).ae("Auto Save/Enable Auto Save3").live()),
                    Item::P(
                        int("auto_save.interval", "Save Every:", 5, 1, 1440)
                            .unit("Minutes")
                            .when("auto_save.enabled")
                            .ae("Auto Save/Auto Save Interval")
                            .live(),
                    ),
                    Item::P(b("auto_save.on_render", "Start auto-save when render queue starts", true)
                        .when("auto_save.enabled")
                        .ae("Auto Save/Enable Auto Save RQ2")),
                    Item::P(
                        int("auto_save.max_versions", "Maximum Project Versions:", 3, 1, 99)
                            .ae("Auto Save/Auto Save Max Project Vers")
                            .live(),
                    ),
                    Item::Group(
                        "Auto-Save Location",
                        &[
                            Item::P(radio(
                                "auto_save.location",
                                "",
                                &["Next to Project", "Custom Location"],
                                "Next to Project",
                            )
                            .ae("Auto Save/Save To Custom Location")
                            .live()),
                            Item::P(path("auto_save.folder", "", "Choose Folder...")
                                .when_is("auto_save.location", "Custom Location")
                                .ae("Auto Save/Auto Save Folder")
                                .live()),
                        ],
                    ),
                ],
            ),
            Item::Group(
                "New Project",
                &[
                    Item::Row(&[
                        Item::P(b("new_project.use_template", "New Project Loads Template:", false).ae("Template Project/Enabled")),
                        Item::P(path("new_project.template", "", "Choose Template...").when("new_project.use_template").ae("Template Project/Location")),
                    ]),
                    Item::Group(
                        "New Project Folders",
                        &[
                            Item::P(text("new_project.solids_folder", "Solids Folder:", "Solids").ae("Template Project/New Project Solids Folder")),
                            Item::P(text("new_project.fills_folder", "Fills Folder:", "Fills").ae("Template Project/New Project Fills Folder")),
                        ],
                    ),
                ],
            ),
        ],
    },
    Page {
        id: "composition",
        title: "Composition",
        items: &[
            Item::P(
                int("general.path_point_size", "Path Point and Handle Size:", 5, 1, 30)
                    .ae("General Section/Path Point and Handle Radius (pixels)"),
            ),
            Item::Gap,
            Item::P(
                b("general.create_layers_at_start", "Create Layers at Composition Start Time", true)
                    .ae("General Section/Create New Layers At Time Zero"),
            ),
            Item::P(b("general.switches_affect_nested", "Switches Affect Nested Comps", true)
                .ae("Main Pref Section v2/Pref_SET_QUAL_RECURSIVELY")),
            Item::P(
                b("general.default_spatial_linear", "Default Spatial Interpolation to Linear", false)
                    .ae("Main Pref Section v2/Pref_DEFAULT_SP_INTERP_LIN"),
            ),
            Item::P(
                b(
                    "general.preserve_mask_points",
                    "Preserve Constant Vertex and Feather Point Count when Editing Masks",
                    true,
                )
                .ae("Main Pref Section v2/Pref_USE_SMART_MASK"),
            ),
            Item::P(
                b("general.split_layers_above", "Create Split Layers Above Original Layer", false)
                    .ae("Split Layer/Grow Layer Stack Upwards"),
            ),
            Item::P(
                b("general.center_shape_anchor", "Center Anchor Point in New Shape Layers", true)
                    .ae("General Section/Shape Layer Center Anchor Point"),
            ),
            Item::Group(
                "Motion Path",
                &[
                    Item::P(radio(
                        "display.motion_path",
                        "",
                        &["No Motion Path", "All Keyframes", "No More Than (Keyframes)", "No More Than (Time)"],
                        "No More Than (Keyframes)",
                    )
                    .ae("Main Pref Section v2/Pref_COMP_PATH_DISPLAY_PRE")),
                    Item::P(
                        int("display.motion_path_keys", "Keyframes:", 15, 1, 999)
                            .when_is("display.motion_path", "No More Than (Keyframes)"),
                    ),
                    Item::P(
                        dur("display.motion_path_time", "Time:", 15.0)
                            .when_is("display.motion_path", "No More Than (Time)"),
                    ),
                ],
            ),
        ],
    },
    Page {
        id: "previews",
        title: "Previews",
        items: &[
            Item::P(
                choice(
                    "previews.adaptive_limit",
                    "Adaptive Resolution Limit:",
                    &["1/2", "1/4", "1/8", "1/16"],
                    "1/8",
                )
                .ae("Realtime Update Preference Section/Minimum Adaptive Resolution Limit"),
            ),
            Item::Button("previews.gpu_info", "GPU Information..."),
            Item::Group(
                "Viewer Quality",
                &[
                    Item::P(choice(
                        "previews.zoom_quality",
                        "Zoom Quality:",
                        &["Faster", "More Accurate Except Cached Preview", "More Accurate"],
                        "More Accurate Except Cached Preview",
                    )),
                    Item::P(choice(
                        "previews.color_quality",
                        "Color Management Quality:",
                        &["Faster", "More Accurate Except Cached Preview", "More Accurate"],
                        "More Accurate Except Cached Preview",
                    )),
                ],
            ),
            Item::Group(
                "Cache Frames When Idle",
                &[
                    Item::P(b("previews.cache_when_idle", "Cache Frames When Idle", true)
                        .ae("Speculative Preview Preference Section/Cache Frames When Idle")),
                    Item::P(
                        int("previews.idle_delay", "Delay Before Caching Starts:", 8, 1, 60)
                            .unit("seconds")
                            .when("previews.cache_when_idle")
                            .ae("Speculative Preview Preference Section/Idle Delay Before Caching Starts"),
                    ),
                ],
            ),
            Item::P(
                b("previews.mute_not_realtime", "Mute Audio When Preview Is Not Real-time", false)
                    .ae("Audio Settings Preference Section/Mute Audio When Preview Not Realtime v2"),
            ),
            Item::P(b("previews.simultaneous", "Enable Simultaneous Previews", false)
                .ae("General Section/Enable Simultaneous Previews")),
            Item::P(b(
                "display.hw_accelerate",
                "Hardware Accelerate Composition, Layer, and Footage Panels",
                true,
            )
            .ae("Hardware Accelerated Viewer 2/Enabled")),
            Item::P(b(
                "display.show_render_progress",
                "Show Rendering in Progress in Info Panel and Flowchart",
                false,
            )
            .ae("Main Pref Section v2/Pref_SHOW_INFO_FLOATER_PROGRESS")),
            Item::P(b("display.alternate_time", "Show Alternate Time Display", true)
                .ae("Prefs.Display/Show Alternate Time Display")),
        ],
    },
    Page {
        id: "appearance",
        title: "Appearance",
        items: &[
            Item::P(b("appearance.label_handles", "Use Label Color for Layer Handles and Paths", true)),
            Item::P(b("appearance.label_tabs", "Use Label Color for Related Tabs", true)),
            Item::P(b("appearance.cycle_mask_colors", "Cycle Mask Colors (uses Label Colors)", true)
                .ae("Mask Colors/Cycle Mask Label Colors")),
            Item::P(b("appearance.contrasting_mask", "Use Contrasting Color for New Masks", true)
                .ae("Mask Colors/Use Contrasting Color For Mask Path Enabled")),
            Item::P(b("appearance.gradients", "Use Gradients", true).ae("Main Pref Section v2/Enable Gradients")),
            Item::Group(
                "Brightness",
                &[
                    Item::Row(&[
                        Item::P(slider("appearance.brightness", "", 0.0)
                            .ae("Main Pref Section v2/User Interface Brightness (4) [0.0..1.0]").live()),
                        Item::Button("appearance.brightness_default", "Default"),
                    ]),
                    Item::P(b("appearance.affect_labels", "Affect Label Colors", false).live()),
                ],
            ),
            Item::Group(
                "Interactive Controls",
                &[Item::Row(&[
                    Item::P(slider("appearance.interactive", "", 0.5).live()),
                    Item::Button("appearance.interactive_default", "Default"),
                ])],
            ),
            Item::Group(
                "Focus Indicator",
                &[Item::Row(&[
                    Item::P(slider("appearance.focus", "", 0.5).live()),
                    Item::Button("appearance.focus_default", "Default"),
                ])],
            ),
            Item::P(b("appearance.accessible_contrast", "Use Accessible Contrast", false)
                .ae("Main Pref Section v2/Use Accessible Contrast")),
        ],
    },
    Page {
        id: "grids_guides",
        title: "Grids & Guides",
        items: &[
            Item::Group(
                "Grid",
                &[
                    Item::Row(&[
                        Item::P(color("grids.grid_color", "Color:", "#008000").ae("Main Pref Section v2/Pref_GRID_COLOR").live()),
                        Item::P(choice("grids.grid_style", "Style:", GRID_STYLES, "Lines").ae("Main Pref Section v2/Pref_GRID_STYLE").live()),
                    ]),
                    Item::Row(&[
                        Item::P(
                            num("grids.grid_spacing", "Gridline every:", 80.0, 1.0, 10000.0, 0)
                                .unit("pixels")
                                .ae("Main Pref Section v2/Pref_GRID_PIX_PER_DIV")
                                .live(),
                        ),
                        Item::P(
                            int("grids.grid_subdivisions", "Subdivisions:", 4, 1, 100)
                                .ae("Main Pref Section v2/Pref_GRID_SUB_PER_DIV")
                                .live(),
                        ),
                    ]),
                ],
            ),
            Item::Group(
                "Proportional Grid",
                &[Item::Row(&[
                    Item::P(int("grids.prop_horizontal", "Horizontal:", 8, 1, 100).ae("Main Pref Section v2/Pref_ANIMGRID_CELLS_HORIZ")),
                    Item::P(int("grids.prop_vertical", "Vertical:", 6, 1, 100).ae("Main Pref Section v2/Pref_ANIMGRID_CELLS_VERT v2")),
                ])],
            ),
            Item::Group(
                "Guides",
                &[Item::Row(&[
                    Item::P(color("grids.guide_color", "Color:", "#00a2ff").ae("Main Pref Section v2/Pref_GUIDE_COLOR")),
                    Item::P(choice("grids.guide_style", "Style:", GUIDE_STYLES, "Lines").ae("Main Pref Section v2/Pref_GUIDE_STYLE")),
                ])],
            ),
            Item::Group(
                "Safe Margins",
                &[
                    Item::Row(&[
                        Item::P(num("grids.action_safe", "Action-safe:", 10.0, 0.0, 100.0, 1).unit("%").ae("Grids And Guides v2/Action Safe Margin % (0-100)").live()),
                        Item::P(num("grids.title_safe", "Title-safe:", 20.0, 0.0, 100.0, 1).unit("%").ae("Grids And Guides v2/Title Safe Margin % (0-100)").live()),
                    ]),
                    Item::Row(&[
                        Item::P(num("grids.cc_action_safe", "Center-cut Action-safe:", 32.5, 0.0, 100.0, 1).unit("%").ae("Grids And Guides v2/4:3 Action Safe Margin % (0-100)").live()),
                        Item::P(num("grids.cc_title_safe", "Center-cut Title-safe:", 40.0, 0.0, 100.0, 1).unit("%").ae("Grids And Guides v2/4:3 Title Safe Margin % (0-100)").live()),
                    ]),
                ],
            ),
        ],
    },
    Page {
        id: "labels",
        title: "Labels",
        items: &[
            Item::Group(
                "Label Defaults",
                &[
                    Item::P(label("labels.default_comp", "Composition:", 15).ae("Label Preference Indices Section 5/Comp Label Index 2")),
                    Item::P(label("labels.default_video", "Video:", 3).ae("Label Preference Indices Section 5/Video Label Index 2")),
                    Item::P(label("labels.default_audio", "Audio:", 7).ae("Label Preference Indices Section 5/Audio Label Index 2")),
                    Item::P(label("labels.default_still", "Still:", 5).ae("Label Preference Indices Section 5/Still Label Index 2")),
                    Item::P(label("labels.default_folder", "Folder:", 2).ae("Label Preference Indices Section 5/Folder Label Index 2")),
                    Item::P(label("labels.default_null", "Null:", 1).ae("Label Preference Indices Section 5/Null Label Index")),
                    Item::P(label("labels.default_shape", "Shape:", 8).ae("Label Preference Indices Section 5/Shape Label Index 2")),
                    Item::P(label("labels.default_solid", "Solid:", 1).ae("Label Preference Indices Section 5/Solid Label Index 2")),
                    Item::P(label("labels.default_camera", "Camera:", 4).ae("Label Preference Indices Section 5/Camera Label Index 2")),
                    Item::P(label("labels.default_light", "Light:", 6).ae("Label Preference Indices Section 5/Light Label Index 2")),
                    Item::P(label("labels.default_adjustment", "Adjustment:", 5).ae("Label Preference Indices Section 5/Adjustment Label Index 2")),
                    Item::P(label("labels.default_text", "Text:", 1).ae("Label Preference Indices Section 5/Text Label Index")),
                    Item::P(label("labels.default_3d_model", "3D Model:", 14).ae("Label Preference Indices Section 5/3D Model Label Index")),
                ],
            ),
            Item::Group(
                "Label Colors",
                label_rows!(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16),
            ),
        ],
    },
    Page {
        id: "type",
        title: "Type",
        items: &[
            Item::Group(
                "Text Engine",
                &[Item::P(radio(
                    "type.engine",
                    "",
                    &["Latin and East Asian", "South Asian and Middle Eastern"],
                    "Latin and East Asian",
                ))],
            ),
            Item::P(radio(
                "type.paragraph_direction",
                "Default Paragraph Direction:",
                &["Left-to-Right", "Right-to-Left"],
                "Left-to-Right",
            )
            .ae("Type/Default Paragraph Direction")),
            Item::P(int("type.recent_fonts", "Number of Recent Fonts to Display:", 10, 0, 30)
                .ae("General Section/Font Panel MRU Size")),
            Item::P(b("type.english_font_names", "Show Font Names in English", false)
                .ae("Text and Fonts/Show English Font Names")),
            Item::P(b("type.missing_glyph", "Enable Missing Glyph Protection", true)),
            Item::P(b("type.smart_quotes", "Use Smart Quotes", true).ae("Text and Fonts/Smart Quotes")),
        ],
    },
    Page {
        id: "import",
        title: "Import",
        items: &[
            Item::Group(
                "Still Footage",
                &[
                    Item::P(radio(
                        "import.still_length",
                        "",
                        &["Length of Composition", "Duration"],
                        "Length of Composition",
                    )
                    .ae("Main Pref Section v2/Pref_DEFAULT_STILL_OUT_POINT v2")),
                    Item::P(
                        dur("import.still_duration", "Duration:", 5.0)
                            .when_is("import.still_length", "Duration"),
                    ),
                ],
            ),
            Item::Group(
                "Sequence Footage",
                &[
                    Item::P(
                        num("import.sequence_fps", "", 30.0, 1.0, 999.0, 2)
                            .unit("frames per second")
                            .ae("Import Options Preference Section/Import Options Default Sequence FPS"),
                    ),
                    Item::P(b("import.report_missing", "Report Missing Frames", true)
                        .ae("Import Options Preference Section/Report Missing Frames From Sequences")),
                    Item::P(b("import.verify_sequence", "Verify Individual Sequence Files (Slower)", false)
                        .ae("Import Options Preference Section/Import Options Verify Individual Sequence Files")),
                ],
            ),
            Item::P(
                choice(
                    "import.unlabeled_alpha",
                    "Interpret Unlabeled Alpha As:",
                    &[
                        "Ask User",
                        "Guess",
                        "Ignore Alpha",
                        "Straight (Unmatted)",
                        "Premultiplied (Matted With Black)",
                        "Premultiplied (Matted With White)",
                    ],
                    "Guess",
                )
                .ae("Import Options Preference Section/Import Options Default Unlabeled Alpha"),
            ),
            Item::P(
                choice(
                    "import.drag_as",
                    "Default Drag Import As:",
                    &["Footage", "Composition", "Composition - Retain Layer Sizes"],
                    "Composition",
                )
                .ae("Import Options Preference Section/Import Options Default Import Type"),
            ),
            Item::P(
                choice(
                    "import.auto_reload",
                    "Automatically Reload Footage:",
                    &["Off", "All Footage", "Non-Sequence Footage"],
                    "Non-Sequence Footage",
                )
                .ae("Auto Reload Footage On Application Focus/Enabled"),
            ),
        ],
    },
    Page {
        id: "export",
        title: "Export",
        items: &[
            Item::Row(&[
                Item::P(b("output.segment_sequences", "Segment Sequences At:", false)
                    .ae("Main Pref Section v2/Pref_LIMIT_FILES_PER_FOLDER v2")),
                Item::P(
                    int("output.segment_files", "", 700, 1, 100000)
                        .unit("files")
                        .when("output.segment_sequences"),
                ),
            ]),
            Item::Row(&[
                Item::P(b("output.segment_movies", "Segment Movie Files At:", false)
                    .ae("Main Pref Section v2/Pref_LIMIT_FILESIZE_K")),
                Item::P(
                    int("output.segment_mb", "", 1024, 1, 1_000_000)
                        .unit("MB")
                        .when("output.segment_movies"),
                ),
            ]),
            Item::P(
                int("output.min_disk_space", "Minimum Diskspace Before Overflowing:", 50, 1, 1_000_000)
                    .unit("MB")
                    .ae("Misc Section/Minimum space left on output disk in MB"),
            ),
            Item::P(
                num("output.audio_block", "Audio Block Duration:", 1.0, 0.01, 60.0, 2)
                    .unit("seconds")
                    .ae("Main Pref Section v2/Pref_SOUND_CHUNK_DURATION"),
            ),
            Item::P(b("output.default_file_name", "Use Default File Name and Folder", true)
                .ae("Misc Section/Guess Default Output File Name")),
            Item::P(b("notifications.enabled", "Notify when Render Queue completes", false)
                .ae("Notifications/Enable Queue Notifications")),
            Item::P(b("notifications.play_sound", "Play Sound When Render Finishes", true)
                .ae("Misc Section/Play sound when render finishes")),
        ],
    },
    Page {
        id: "audio",
        title: "Audio",
        items: &[
            Item::Group(
                "Audio Hardware",
                &[
                    Item::P(choice("audio.device_class", "Device Class:", &["System Default"], "System Default")),
                    Item::P(choice("audio.default_output", "Default Output:", &["System Default"], "System Default")),
                    Item::Button("audio.settings", "Settings..."),
                ],
            ),
            Item::Group(
                "Audio Output Mapping",
                &[
                    Item::P(choice("audio_map.device", "Map Output For:", &["System Default"], "System Default")),
                    Item::P(choice("audio_map.left", "Left:", &["Output 1", "Output 2"], "Output 1")),
                    Item::P(choice("audio_map.right", "Right:", &["Output 1", "Output 2"], "Output 2")),
                ],
            ),
        ],
    },
    Page {
        id: "disk",
        title: "Disk",
        items: &[
            Item::Group(
                "Disk Cache",
                &[
                    Item::P(b("cache.disk_enabled", "Enable Disk Cache", true).ae("Disk Cache Controls/Enabled 2")),
                    Item::P(
                        int("cache.disk_max_gb", "Maximum Disk Cache Size:", 100, 1, 100_000)
                            .unit("GB")
                            .when("cache.disk_enabled")
                            .ae("Disk Cache Controls/Max Size 3"),
                    ),
                    Item::P(path("cache.disk_folder", "", "Choose Folder...").when("cache.disk_enabled").ae("Disk Cache Controls/Folder 7")),
                    Item::Row(&[
                        Item::Button("cache.empty_disk", "Empty Disk Cache..."),
                        Item::Button("cache.verify", "Verify..."),
                    ]),
                ],
            ),
            Item::Group(
                "Media Cache",
                &[
                    Item::P(path("cache.media_db", "Database:", "Choose Folder...")),
                    Item::P(path("cache.media_files", "Cache:", "Choose Folder...")),
                    Item::Button("cache.clean_media", "Clean Database & Cache"),
                ],
            ),
            Item::Group(
                "XMP Metadata",
                &[
                    Item::P(b("cache.xmp_ids", "Write XMP IDs to Files on Import", false)),
                    Item::P(b("cache.xmp_markers", "Create Layer Markers from Footage XMP Metadata", true)
                        .ae("Metadata/CreateLayerMarkersFromFootageXMP")),
                ],
            ),
        ],
    },
    Page {
        id: "memory_cpu",
        title: "Memory & CPU",
        items: &[
            Item::Group(
                "Memory",
                &[
                    Item::Note("Installed RAM: {ram} GB"),
                    Item::P(int("memory.reserved_gb", "RAM reserved for other applications:", 6, 1, 1024).unit("GB")),
                    Item::Note("Total RAM shared by After Effects and other Adobe apps: {shared} GB"),
                    Item::P(b("memory.reduce_cache", "Reduce cache size when system is low on memory (this will slow down previews)", false)),
                ],
            ),
            Item::Group(
                "Performance",
                &[
                    Item::P(b("memory.mfr", "Enable Multi-Frame Rendering", true)
                        .ae("Concurrent Frame Rendering/Enable Concurrent Frame Renders")),
                    Item::P(int("memory.reserved_cpu", "CPUs reserved for other applications:", 1, 0, 256)
                        .when("memory.mfr")),
                    Item::P(b("memory.performance_alerts", "Show Performance Alerts", true)),
                ],
            ),
        ],
    },
    Page {
        id: "video",
        title: "Video",
        items: &[
            Item::P(b("video.mercury_transmit", "Enable Mercury Transmit", false).ae("Win32 Video Preview Pref Section/Video Preview Enabled")),
            Item::P(choice("video.device", "Video Device:", &["None"], "None").when("video.mercury_transmit")),
            Item::P(b("video.disable_background", "Disable video output when in background", true).when("video.mercury_transmit")),
            Item::Group(
                "Video Output During:",
                &[Item::Row(&[
                    Item::P(b("video.during_previews", "Previews", true).when("video.mercury_transmit")),
                    Item::P(b("video.during_interactions", "Interactions", true).when("video.mercury_transmit")),
                    Item::P(b("video.during_renders", "Renders", false).when("video.mercury_transmit")
                        .ae("Win32 Video Preview Pref Section/Video Preview During Render")),
                ])],
            ),
            Item::P(b("video.scale_frame", "Scale and Letterbox Output to Fit Video Monitor", true).when("video.mercury_transmit")),
            Item::Button("video.audio_hardware", "Audio Hardware..."),
        ],
    },
    Page {
        id: "3d",
        title: "3D",
        items: &[
            Item::P(choice(
                "3d.renderer",
                "Renderer for New Compositions:",
                &["Advanced 3D", "Cinema 4D", "Classic 3D"],
                "Advanced 3D",
            )
            .ae("Renderer Preference Section/New Composition Default")),
            Item::P(b("3d.multiprocessing", "Enable Multiprocessing", true).ae("Advanced 3D/Enable Multiprocessing")),
            Item::P(b("3d.labels_visible", "Show 3D View Labels", true).ae("3D View Preference Section/Labels Visible")),
            Item::P(b("3d.active_camera_preview", "Previews Favor the Active Camera", true)
                .ae("Unified Preview Options Shared/Preview Favors Active Camera")),
        ],
    },
    Page {
        id: "scripting_expressions",
        title: "Scripting & Expressions",
        items: &[
            Item::Group(
                "Application Scripting",
                &[
                    Item::P(b("scripting.allow_files_network", "Allow Scripts to Write Files and Access Network", false)
                        .ae("Main Pref Section v2/Pref_SCRIPTING_FILE_NETWORK_SECURITY")),
                    Item::P(b("scripting.debugger", "Enable JavaScript Debugger", false)
                        .ae("Main Pref Section v2/Pref_JAVASCRIPT_DEBUGGER")),
                    Item::P(b("scripting.warn_executing", "Warn User When Executing Files", true)
                        .ae("Main Pref Section v2/Pref_WARN_WHEN_RUNNING_SCRIPTS")),
                ],
            ),
            Item::Group(
                "Expressions",
                &[
                    Item::P(b("scripting.error_banner", "Enable Expression Errors Banner", true)
                        .ae("Misc Section/Show Expression Error Warning Banner")),
                    Item::P(b("scripting.compact_english", "Expression Pick Whip Writes Compact English", true)
                        .ae("Main Pref Section v2/Pref_USE_COMPACT_EXPRS")),
                ],
            ),
            Item::Group(
                "Expressions Editor",
                &[
                    Item::Row(&[
                        Item::P(choice("scripting.font", "Font:", &["Source Code Pro", "Menlo", "Consolas", "Courier New"], "Source Code Pro")),
                        Item::P(int("scripting.font_size", "Font Size:", 12, 6, 72).ae("Expression Editor Settings (v9)/Font Size")),
                    ]),
                    Item::P(choice("scripting.theme", "Theme:", &["After Effects Classic Dark", "After Effects Classic Light"], "After Effects Classic Dark")
                        .ae("Expression Editor Settings (v9)/Current Expression Theme")),
                    Item::P(b("scripting.syntax_highlighting", "Syntax Highlighting", true).ae("Expression Editor Settings (v9)/Syntax Highlighting - Enabled")),
                    Item::P(b("scripting.line_numbers", "Show Line Numbers", true).ae("Expression Editor Settings (v9)/Show Line Numbers")),
                    Item::P(b("scripting.indent_guides", "Show Indent Guides", true).ae("Expression Editor Settings (v9)/Indent Guides - Enabled")),
                    Item::P(b("scripting.wrap_lines", "Wrap Lines", true).ae("Expression Editor Settings (v9)/Word Wrap - Word Wrap Mode (0: None, 1: Word, 3: Character)")),
                    Item::P(b("scripting.code_folding", "Enable Code Folding", true).ae("Expression Editor Settings (v9)/Folding - Enable")),
                    Item::P(b("scripting.autocomplete", "Enable Auto-Complete", true).ae("Expression Editor Settings (v9)/Auto Complete - Enabled")),
                    Item::P(b("scripting.auto_close_brackets", "Auto-Close Brackets and Quotes", true).ae("Expression Editor Settings (v9)/Bracing - Enable Brace Insert")),
                    Item::P(b("scripting.match_brackets", "Highlight Matching Brackets", true).ae("Expression Editor Settings (v9)/Bracing - Enable Brace Matching")),
                    Item::P(b("scripting.auto_indent", "Auto Indent", true).ae("Expression Editor Settings (v9)/Bracing - Enable Auto Indent")),
                    Item::P(b("scripting.inline_errors", "Show Inline Errors", true).ae("Expression Editor Settings (v9)/Inline Errors - Enabled")),
                    Item::P(b("scripting.indent_tabs", "Use Tabs for Indentation", false).ae("Expression Editor Settings (v9)/Tabs - Enable Use Tabs")),
                    Item::P(int("scripting.tab_size", "Tab Size:", 4, 1, 16).ae("Expression Editor Settings (v9)/Tabs - Tab Width")),
                    Item::Row(&[
                        Item::P(color("scripting.color_default", "Default Text:", "#d4d4d4")),
                        Item::P(color("scripting.color_background", "Background:", "#1e1e1e")),
                    ]),
                    Item::Row(&[
                        Item::P(color("scripting.color_keywords", "Keywords:", "#569cd6")),
                        Item::P(color("scripting.color_strings", "Strings:", "#ce9178")),
                    ]),
                    Item::Row(&[
                        Item::P(color("scripting.color_numbers", "Numbers:", "#b5cea8")),
                        Item::P(color("scripting.color_comments", "Comments:", "#6a9955")),
                    ]),
                    Item::Button("scripting.reset_editor", "Reset to Default Settings"),
                ],
            ),
        ],
    },
];

/// Visits every preference of `items` (including platform-specific ones).
pub fn visit(items: &'static [Item], f: &mut impl FnMut(&'static Pref)) {
    for item in items {
        visit_item(item, f);
    }
}

fn visit_item(item: &'static Item, f: &mut impl FnMut(&'static Pref)) {
    match item {
        Item::P(p) => f(p),
        Item::Row(items) | Item::Group(_, items) => visit(items, f),
        Item::Only(_, item) => visit_item(item, f),
        Item::Button(..) | Item::Note(_) | Item::Gap => {}
    }
}

/// Every preference, in page order.
pub fn all() -> &'static [&'static Pref] {
    static ALL: std::sync::OnceLock<Vec<&'static Pref>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut out = Vec::new();
        for page in PAGES {
            visit(page.items, &mut |p| out.push(p));
        }
        out
    })
}

/// The preference with `key`.
pub fn find(key: &str) -> Option<&'static Pref> {
    all().iter().copied().find(|p| p.key == key)
}

/// The page with `id`.
pub fn page(id: &str) -> Option<&'static Page> {
    PAGES.iter().find(|p| p.id == id)
}
