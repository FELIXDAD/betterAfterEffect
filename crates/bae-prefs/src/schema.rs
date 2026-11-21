//! What the Preferences dialog shows: pages, groups, rows and controls, with AE's
//! English labels (also the i18n keys), value types, ranges, units and defaults.
//!
//! The pages themselves are generated from R1's reading of AE 26.0
//! (`docs/reference/ae/ui/preferences-dialog.json`) into [`crate::pages`]; this
//! module has the types, the constructors the generated file uses and the
//! preferences AE 26 keeps without showing them ([`HIDDEN`]).
//!
//! Every control that stores a value is a [`Pref`] with a dotted key
//! (`<page>.<control id>`, e.g. `project.auto_save_interval_minutes`).

pub use crate::pages::PAGES;

/// One option of a dropdown or radio group: the stored value and its English text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Opt {
    pub value: &'static str,
    pub label: &'static str,
}

impl Opt {
    pub(crate) const fn new(value: &'static str, label: &'static str) -> Self {
        Opt { value, label }
    }
}

/// How a choice is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Dropdown,
    Radio,
    /// Thumbnail cards (Appearance > Color Theme).
    Theme,
}

/// Value type and valid range of a preference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// Checkbox.
    Bool,
    /// Integer field.
    Int { min: i64, max: i64 },
    /// Number field with `decimals` digits.
    Float { min: f64, max: f64, decimals: u8 },
    /// Slider with English labels at both ends.
    Slider {
        min: f64,
        max: f64,
        left: &'static str,
        right: &'static str,
    },
    /// Dropdown, radio buttons or theme cards; the value is the option's `value`.
    Choice {
        options: &'static [Opt],
        style: Style,
    },
    /// Label index 0..=16, shown as label names with swatches.
    Label,
    /// `#rrggbb` color swatch.
    Color,
    /// Free text field.
    Text,
    /// Timecode `h:mm:ss:ff`.
    Timecode,
    /// A folder or file path (chosen with a separate Choose… button).
    Path,
}

/// Default value of a preference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DefaultValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(&'static str),
}

/// Platforms an item appears on.
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
    /// A checkbox is on.
    On(&'static str),
    /// A checkbox is off.
    Off(&'static str),
    /// A choice has this value.
    Is(&'static str, &'static str),
    /// All of these hold.
    All(&'static [When]),
}

/// One stored setting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pref {
    /// Dotted key, `<page>.<control id>`.
    pub key: &'static str,
    /// English label as AE shows it (also the i18n key).
    pub label: &'static str,
    pub kind: Kind,
    pub default: DefaultValue,
    /// Unit after the field, English, translated.
    pub unit: &'static str,
    /// `Section/Key` in AE's preferences file. Debug builds only (empty in
    /// release builds, to keep the strings out of the executable).
    pub ae: &'static str,
    pub enabled: Option<When>,
    /// Drawn indented under the control it depends on.
    pub indent: bool,
    /// The label is only the accessible name (AE draws none: a value inline
    /// after its checkbox, a radio group under its group title, a path display).
    pub quiet: bool,
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
            indent: false,
            quiet: false,
        }
    }
    pub(crate) const fn unit(mut self, unit: &'static str) -> Self {
        self.unit = unit;
        self
    }
    pub(crate) const fn ae(mut self, ae: &'static str) -> Self {
        if cfg!(debug_assertions) {
            self.ae = ae;
        }
        self
    }
    pub(crate) const fn when(mut self, w: When) -> Self {
        self.enabled = Some(w);
        self
    }
    pub(crate) const fn indent(mut self) -> Self {
        self.indent = true;
        self
    }
    pub(crate) const fn quiet(mut self) -> Self {
        self.quiet = true;
        self
    }
}

pub(crate) const fn b(key: &'static str, label: &'static str, on: bool) -> Pref {
    Pref::new(key, label, Kind::Bool, DefaultValue::Bool(on))
}
pub(crate) const fn int(
    key: &'static str,
    label: &'static str,
    v: i64,
    min: i64,
    max: i64,
) -> Pref {
    Pref::new(key, label, Kind::Int { min, max }, DefaultValue::Int(v))
}
pub(crate) const fn num(
    key: &'static str,
    label: &'static str,
    v: f64,
    min: f64,
    max: f64,
    decimals: u8,
) -> Pref {
    Pref::new(
        key,
        label,
        Kind::Float { min, max, decimals },
        DefaultValue::Float(v),
    )
}
pub(crate) const fn slider(
    key: &'static str,
    label: &'static str,
    v: f64,
    min: f64,
    max: f64,
    left: &'static str,
    right: &'static str,
) -> Pref {
    Pref::new(
        key,
        label,
        Kind::Slider {
            min,
            max,
            left,
            right,
        },
        DefaultValue::Float(v),
    )
}
pub(crate) const fn choice(
    key: &'static str,
    label: &'static str,
    options: &'static [Opt],
    v: &'static str,
    style: Style,
) -> Pref {
    Pref::new(
        key,
        label,
        Kind::Choice { options, style },
        DefaultValue::Str(v),
    )
}
pub(crate) const fn label(key: &'static str, text: &'static str, v: i64) -> Pref {
    Pref::new(key, text, Kind::Label, DefaultValue::Int(v))
}
pub(crate) const fn color(key: &'static str, text: &'static str, v: &'static str) -> Pref {
    Pref::new(key, text, Kind::Color, DefaultValue::Str(v))
}
pub(crate) const fn text(key: &'static str, label: &'static str, v: &'static str) -> Pref {
    Pref::new(key, label, Kind::Text, DefaultValue::Str(v))
}
pub(crate) const fn timecode(key: &'static str, label: &'static str, v: &'static str) -> Pref {
    Pref::new(key, label, Kind::Timecode, DefaultValue::Str(v))
}
pub(crate) const fn path(key: &'static str, label: &'static str) -> Pref {
    Pref::new(key, label, Kind::Path, DefaultValue::Str(""))
}

/// A piece of a page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Item {
    /// A stored setting.
    P(Pref),
    /// Items on one line.
    Row(&'static [Item]),
    /// Side-by-side columns.
    Cols(&'static [&'static [Item]]),
    /// A titled box.
    Group(&'static str, &'static [Item]),
    /// Push button: action id (`choose:<key>` for Choose…; others are handled by
    /// the dialog or the host, or drawn disabled) and English label.
    Button(&'static str, &'static str),
    /// Read-only English label and value (`<…>` placeholders: filled by the host
    /// or shown as "—").
    Info(&'static str, &'static str),
    /// A list AE fills from the system (Mercury Transmit devices): label and
    /// column titles.
    Table(&'static str, &'static [&'static str]),
    /// Live preview drawn by the dialog (the expression editor's).
    Preview(&'static str),
    /// An item enabled only when the condition holds.
    When(When, &'static Item),
    /// Item that only appears on one platform.
    Only(Os, &'static Item),
}

/// A page of the dialog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Page {
    /// Variant parameter of `app.settings` (`startup_repair`).
    pub id: &'static str,
    /// English page name in the list.
    pub title: &'static str,
    pub items: &'static [Item],
}

/// Preferences AE 26 keeps without a control in the dialog.
pub static HIDDEN: &[Pref] = &[
    // AE 26 removed Levels of Undo from the dialog; AE's own default is 99. BAE
    // keeps its deep undo stack by default (bae_model::DEFAULT_UNDO_LIMIT).
    int("general.levels_of_undo", "Levels of Undo", 1000, 1, 100_000)
        .ae("General Section/Levels of Undo v2"),
];

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
        Item::Cols(cols) => {
            for col in *cols {
                visit(col, f);
            }
        }
        Item::Only(_, item) | Item::When(_, item) => visit_item(item, f),
        Item::Button(..) | Item::Info(..) | Item::Table(..) | Item::Preview(_) => {}
    }
}

/// Every preference: the pages' in page order, then [`HIDDEN`].
pub fn all() -> &'static [&'static Pref] {
    static ALL: std::sync::OnceLock<Vec<&'static Pref>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut out = Vec::new();
        for page in PAGES {
            visit(page.items, &mut |p| out.push(p));
        }
        out.extend(HIDDEN.iter());
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
