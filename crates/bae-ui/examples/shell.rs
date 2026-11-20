//! Shell demo: title bar, toolbar strip, workspace bar and the dock laid out
//! like AE's Default workspace, filled with widget and icon galleries.
//!
//! Run with `cargo run -p bae-ui --example shell`. Pass `--zh` for Chinese.
//! The layout is saved to `$BAE_LAYOUT` (default: the system temp dir) and
//! restored on the next start.

use bae_ui::dock::{Dock, Frame, Layout, Node, PanelHost, PanelId, TabInfo};
use bae_ui::i18n::{self, Lang, tr};
use bae_ui::icons::{self, Icon};
use bae_ui::theme::{self, color, metrics};
use bae_ui::{titlebar, widgets};
use eframe::egui::{self, Rect, Ui, pos2, vec2};

fn default_layout() -> Node {
    Node::row(vec![
        (
            1406.0,
            Node::column(vec![
                (
                    640.0,
                    Node::row(vec![
                        (320.0, Node::tabs(&["project", "effect_controls"])),
                        (1082.0, Node::tabs(&["composition"])),
                    ]),
                ),
                (295.0, Node::tabs(&["timeline"])),
            ]),
        ),
        (
            316.0,
            Node::column(vec![
                (52.0, Node::tabs(&["preview"])),
                (
                    883.0,
                    Node::frame(
                        Frame::new(&["properties", "align", "audio", "effects"])
                            .stacked(Some(601.0)),
                    ),
                ),
            ]),
        ),
    ])
}

fn layout_path() -> std::path::PathBuf {
    std::env::var_os("BAE_LAYOUT")
        .map(Into::into)
        .unwrap_or_else(|| std::env::temp_dir().join("bae-shell-demo-layout.json"))
}

struct Demo {
    position: [f64; 2],
    scale: [f64; 2],
    rotation: f64,
    opacity: f64,
    animated: [bool; 4],
    open: bool,
    snapping: bool,
    auto_open: bool,
    parent: usize,
    search: String,
    switches: [bool; 6],
    locked: bool,
}

impl PanelHost for Demo {
    fn tab(&mut self, panel: &PanelId) -> TabInfo {
        match panel.as_str() {
            "project" => TabInfo::new("Project"),
            "effect_controls" => {
                TabInfo::new("Effect Controls Shape Layer 1").label(theme::label_color(8))
            }
            "composition" => TabInfo::new("Composition Main Comp")
                .label(theme::label_color(15))
                .locked(self.locked)
                .viewer(),
            "timeline" => TabInfo::new("Main Comp")
                .label(theme::label_color(15))
                .viewer(),
            "preview" => TabInfo::new("Preview"),
            "properties" => TabInfo::new("Properties: Shape Layer 1"),
            "align" => TabInfo::new("Align"),
            "audio" => TabInfo::new("Audio"),
            "effects" => TabInfo::new("Effects & Presets"),
            other => TabInfo::new(other),
        }
    }

    fn set_locked(&mut self, _panel: &PanelId, locked: bool) {
        self.locked = locked;
    }

    fn ui(&mut self, ui: &mut Ui, panel: &PanelId) {
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(12, 4))
            .show(ui, |ui| match panel.as_str() {
                "project" => icon_gallery(ui),
                "properties" => self.properties(ui),
                "effect_controls" => self.controls(ui),
                "preview" => {
                    ui.horizontal(|ui| {
                        ui.add_space(50.0);
                        for icon in [
                            Icon::FirstFrame,
                            Icon::PrevFrame,
                            Icon::Play,
                            Icon::NextFrame,
                            Icon::LastFrame,
                        ] {
                            icons::button(ui, icon, 22.0, false);
                        }
                    });
                }
                "timeline" => self.timeline(ui),
                _ => {
                    ui.label(tr(panel.as_str()));
                }
            });
    }
}

impl Demo {
    fn properties(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tr("Layer Transform")).color(color::TEXT_STRONG));
            ui.add_space(60.0);
            widgets::link(ui, tr("Reset"));
        });
        let rows: [(&str, usize); 4] = [
            ("Position", 0),
            ("Scale", 1),
            ("Rotation", 2),
            ("Opacity", 3),
        ];
        for (name, i) in rows {
            ui.horizontal(|ui| {
                if self.animated[i] {
                    widgets::keyframe_nav(ui, true, i == 1, false);
                } else {
                    ui.add_space(8.0);
                    widgets::stopwatch(ui, &mut self.animated[i]);
                    ui.add_space(8.0);
                }
                ui.add_sized(vec2(90.0, 18.0), egui::Label::new(tr(name)));
                match i {
                    0 => {
                        ui.add(widgets::HotNumber::new(&mut self.position[0]).field_width(39.0));
                        ui.add_space(8.0);
                        ui.add(widgets::HotNumber::new(&mut self.position[1]).field_width(39.0));
                    }
                    1 => {
                        ui.add(
                            widgets::HotNumber::new(&mut self.scale[0])
                                .suffix("%")
                                .field_width(39.0),
                        );
                        ui.add_space(8.0);
                        ui.add(
                            widgets::HotNumber::new(&mut self.scale[1])
                                .suffix("%")
                                .field_width(39.0),
                        );
                    }
                    2 => {
                        ui.add(
                            widgets::HotNumber::new(&mut self.rotation)
                                .suffix("°")
                                .decimals(1)
                                .speed(0.5),
                        );
                    }
                    _ => {
                        ui.add(
                            widgets::HotNumber::new(&mut self.opacity)
                                .suffix("%")
                                .range(0.0..=100.0)
                                .field_width(39.0),
                        );
                    }
                }
            });
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(tr("Language"));
            let mut lang = (i18n::lang() == Lang::Zh) as usize;
            if widgets::dropdown(ui, "lang", &mut lang, &["English", "简体中文"], 110.0).changed()
            {
                i18n::set_lang(if lang == 1 { Lang::Zh } else { Lang::En });
            }
        });
    }

    fn controls(&mut self, ui: &mut Ui) {
        widgets::search_field(ui, &mut self.search, 280.0);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            widgets::disclosure(ui, &mut self.open);
            ui.label("Contents");
        });
        widgets::checkbox(ui, &mut self.snapping, "Snapping");
        widgets::checkbox(ui, &mut self.auto_open, "Auto-Open Panel");
        widgets::dropdown(
            ui,
            "parent",
            &mut self.parent,
            &["None", "1. Shape Layer 1", "2. Hello BAE"],
            100.0,
        );
    }

    fn timeline(&mut self, ui: &mut Ui) {
        ui.label(
            egui::RichText::new(widgets::timecode(30, 30.0))
                .size(17.0)
                .color(color::ACCENT),
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 1.0;
            let icons = [
                Icon::Eye,
                Icon::Speaker,
                Icon::Solo,
                Icon::Lock,
                Icon::Shy,
                Icon::Fx,
            ];
            for (on, icon) in self.switches.iter_mut().zip(icons) {
                widgets::switch_cell(ui, on, icon, vec2(11.0 * 1.5, 16.0));
            }
        });
    }
}

fn icon_gallery(ui: &mut Ui) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("icons")
            .spacing(vec2(6.0, 4.0))
            .show(ui, |ui| {
                for (i, icon) in Icon::ALL.iter().enumerate() {
                    icons::show(ui, *icon, 16.0, color::ICON);
                    ui.label(egui::RichText::new(format!("{icon:?}")).size(10.0));
                    if i % 3 == 2 {
                        ui.end_row();
                    }
                }
            });
    });
}

struct App {
    dock: Dock,
    demo: Demo,
    workspace: usize,
    saved: String,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, color::APP_BG);
        titlebar::show(ui, "Adobe After Effects 2026 - Untitled Project *", true);

        // Toolbar strip (tools on the left, workspaces on the right).
        let top = full.top() + titlebar::height() + 0.5;
        let toolbar = Rect::from_min_max(
            pos2(full.left(), top),
            pos2(full.right(), top + metrics::TOOLBAR_H - 0.5),
        );
        ui.painter().rect_filled(toolbar, 0.0, color::PANEL);
        let ws_rect = Rect::from_min_max(pos2(full.right() - 525.5, toolbar.top()), toolbar.max);
        ui.painter().vline(
            ws_rect.left() - 0.5,
            toolbar.y_range(),
            egui::Stroke::new(1.0, color::APP_BG),
        );
        let mut tools = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(toolbar.shrink2(vec2(20.0, 8.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        for (i, icon) in [
            Icon::Home,
            Icon::Selection,
            Icon::Hand,
            Icon::Zoom,
            Icon::Orbit,
            Icon::PanCamera,
            Icon::Dolly,
            Icon::Rotate,
            Icon::PanBehind,
            Icon::Rectangle,
            Icon::Extrude,
            Icon::Pen,
            Icon::Type,
            Icon::Brush,
            Icon::CloneStamp,
            Icon::Eraser,
            Icon::RotoBrush,
            Icon::Puppet,
        ]
        .into_iter()
        .enumerate()
        {
            icons::button(&mut tools, icon, 24.0, i == 1);
        }
        let names = ["Default", "Review", "Learn", "Small Screen", "Standard"];
        if let Some(ev) = widgets::workspace_bar(ui, ws_rect, &names, self.workspace) {
            match ev {
                widgets::WorkspaceEvent::Switch(i) => self.workspace = i,
                widgets::WorkspaceEvent::Reset => self.dock.set_layout(Layout {
                    root: default_layout(),
                    floating: Vec::new(),
                }),
                _ => {}
            }
        }

        let dock_rect = Rect::from_min_max(
            pos2(
                full.left() + 1.0,
                toolbar.bottom() + metrics::TOOLBAR_GAP - 0.5,
            ),
            pos2(full.right() - 1.0, full.bottom() - 1.0),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(dock_rect));
        self.dock.show(&mut child, &mut self.demo);

        let json = self.dock.layout.to_json();
        if json != self.saved {
            let _ = std::fs::write(layout_path(), &json);
            self.saved = json;
        }
    }
}

fn main() -> eframe::Result {
    if std::env::args().any(|a| a == "--zh") {
        i18n::set_lang(Lang::Zh);
    }
    i18n::register(&[
        ("Project", "项目"),
        ("Effect Controls Shape Layer 1", "效果控件 Shape Layer 1"),
        ("Composition Main Comp", "合成 Main Comp"),
        ("Preview", "预览"),
        ("Properties: Shape Layer 1", "属性: Shape Layer 1"),
        ("Align", "对齐"),
        ("Audio", "音频"),
        ("Effects & Presets", "效果和预设"),
        ("Layer Transform", "图层变换"),
        ("Position", "位置"),
        ("Scale", "缩放"),
        ("Rotation", "旋转"),
        ("Opacity", "不透明度"),
    ]);
    let fresh = std::env::args().any(|a| a == "--reset");
    let layout = (!fresh)
        .then(|| std::fs::read_to_string(layout_path()).ok())
        .flatten()
        .and_then(|s| Layout::from_json(&s).ok())
        .unwrap_or(Layout {
            root: default_layout(),
            floating: Vec::new(),
        });
    let options = eframe::NativeOptions {
        viewport: titlebar::viewport_builder("BAE").with_inner_size([1728.0, 1000.0]),
        ..Default::default()
    };
    eframe::run_native(
        "bae-shell-demo",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            let mut dock = Dock::from_layout(layout);
            dock.focus_panel(&"composition".into());
            Ok(Box::new(App {
                dock,
                demo: Demo {
                    position: [950.0, 540.0],
                    scale: [150.0, 150.0],
                    rotation: 0.0,
                    opacity: 100.0,
                    animated: [true, true, false, false],
                    open: false,
                    snapping: false,
                    auto_open: true,
                    parent: 0,
                    search: String::new(),
                    switches: [true, false, false, false, true, true],
                    locked: false,
                },
                workspace: 0,
                saved: String::new(),
            }))
        }),
    )
}
