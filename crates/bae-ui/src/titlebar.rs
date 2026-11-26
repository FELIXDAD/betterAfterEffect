//! Compact title bar drawn by the app, like AE on macOS.
//!
//! The window keeps its native frame (rounded corners, shadow, edge resizing)
//! but hides the system title and traffic lights; we draw a 19pt bar with
//! small traffic lights and the document title instead. Window actions go
//! through `ViewportCommand`s. On other platforms the native title bar stays
//! and [`show`] draws nothing.

use egui::{
    Color32, CursorIcon, Id, PointerButton, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2,
    ViewportBuilder, ViewportCommand, pos2, vec2,
};

use crate::a11y::Accessible as _;
use crate::theme::{self, color, metrics};

/// Measured from AE 2026 (points).
mod geom {
    pub const LIGHT_RADIUS: f32 = 4.5;
    pub const LIGHT_Y: f32 = 9.25;
    pub const LIGHT_X0: f32 = 9.25;
    pub const LIGHT_STEP: f32 = 15.0;
    /// Slightly above 12pt so SF matches the width of AE's title.
    pub const TITLE_FONT: f32 = 12.6;
    pub const TITLE_BASELINE: f32 = 13.5;
}

/// Height reserved for the title bar on this platform.
pub fn height() -> f32 {
    if cfg!(target_os = "macos") {
        metrics::TITLE_BAR_H
    } else {
        0.0
    }
}

/// Window settings that hide the system title bar on macOS while keeping a
/// native, resizable window.
pub fn viewport_builder(title: &str) -> ViewportBuilder {
    let builder = ViewportBuilder::default().with_title(title);
    if cfg!(target_os = "macos") {
        builder
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
            .with_titlebar_buttons_shown(false)
    } else {
        builder
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Light {
    Close,
    Minimize,
    Zoom,
}

/// Draw the title bar at the top of `ui` and handle window actions.
/// `dirty` shows the unsaved-changes dot in the close button.
pub fn show(ui: &mut Ui, title: &str, dirty: bool) -> Response {
    let h = height();
    let full = ui.max_rect();
    let rect = Rect::from_min_size(full.min, vec2(full.width(), h));
    if h <= 0.0 {
        return ui.allocate_rect(rect, Sense::hover());
    }
    let bar = ui
        .allocate_rect(rect, Sense::click_and_drag())
        .labeled(egui::WidgetType::Other, title);
    let p = ui.painter();
    crate::theme::bar_fill(p, rect, color::TITLE_BAR);
    p.hline(
        rect.x_range(),
        rect.top() + 0.25,
        Stroke::new(0.5, color::TITLE_HIGHLIGHT),
    );
    p.hline(
        rect.x_range(),
        rect.bottom() + 0.25,
        Stroke::new(0.5, Color32::BLACK),
    );

    let galley = p.layout_no_wrap(
        title.to_owned(),
        theme::font(geom::TITLE_FONT),
        color::TITLE_TEXT,
    );
    let tx = rect.center().x - galley.size().x / 2.0;
    let ty = rect.top() + geom::TITLE_BASELINE - theme::baseline_offset(&galley);
    p.galley(pos2(tx, ty), galley, color::TITLE_TEXT);

    let (focused, fullscreen, maximized) = ui.input(|i| {
        let v = i.viewport();
        (
            v.focused.unwrap_or(true),
            v.fullscreen.unwrap_or(false),
            v.maximized.unwrap_or(false),
        )
    });

    // Hovering any light reveals the glyphs on all three, as on macOS.
    let group = Rect::from_min_max(
        pos2(rect.left() + 2.0, rect.top()),
        pos2(
            rect.left() + geom::LIGHT_X0 + 2.0 * geom::LIGHT_STEP + 7.0,
            rect.bottom(),
        ),
    );
    let group_hovered = ui.rect_contains_pointer(group);
    let mut clicked = None;
    for (i, light) in [Light::Close, Light::Minimize, Light::Zoom]
        .into_iter()
        .enumerate()
    {
        let c = pos2(
            rect.left() + geom::LIGHT_X0 + i as f32 * geom::LIGHT_STEP,
            rect.top() + geom::LIGHT_Y,
        );
        let hit = Rect::from_center_size(c, Vec2::splat(geom::LIGHT_RADIUS * 2.0 + 3.0));
        let name = match light {
            Light::Close => "Close",
            Light::Minimize => "Minimize",
            Light::Zoom => "Zoom",
        };
        let resp = ui
            .interact(hit, Id::new(("bae-titlebar-light", i)), Sense::click())
            .labeled(egui::WidgetType::Button, crate::i18n::tr(name));
        if resp.clicked() {
            clicked = Some(light);
        }
        paint_light(
            ui,
            c,
            light,
            focused || group_hovered,
            group_hovered,
            resp.is_pointer_button_down_on(),
            dirty,
        );
    }

    match clicked {
        Some(Light::Close) => ui.ctx().send_viewport_cmd(ViewportCommand::Close),
        Some(Light::Minimize) => ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true)),
        Some(Light::Zoom) => {
            if ui.input(|i| i.modifiers.alt) {
                ui.ctx()
                    .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
            } else {
                ui.ctx()
                    .send_viewport_cmd(ViewportCommand::Fullscreen(!fullscreen));
            }
        }
        None => {}
    }

    if bar.double_clicked() {
        ui.ctx()
            .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    } else if bar.drag_started_by(PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if bar.hovered() && !group_hovered {
        ui.ctx().set_cursor_icon(CursorIcon::Default);
    }
    bar
}

fn paint_light(
    ui: &Ui,
    c: Pos2,
    light: Light,
    active: bool,
    glyphs: bool,
    pressed: bool,
    dirty: bool,
) {
    let p = ui.painter();
    let (fill, rim, glyph) = match light {
        Light::Close => (
            color::TRAFFIC_CLOSE,
            Color32::from_rgb(0xd0, 0x55, 0x4b),
            Color32::from_rgb(0x8e, 0x1b, 0x12),
        ),
        Light::Minimize => (
            color::TRAFFIC_MIN,
            Color32::from_rgb(0xce, 0xa4, 0x4b),
            Color32::from_rgb(0x98, 0x57, 0x12),
        ),
        Light::Zoom => (
            color::TRAFFIC_ZOOM,
            Color32::from_rgb(0x50, 0xa8, 0x45),
            Color32::from_rgb(0x0b, 0x65, 0x0d),
        ),
    };
    let (fill, rim) = if active {
        (fill, rim)
    } else {
        (color::TRAFFIC_IDLE, color::TRAFFIC_IDLE)
    };
    let fill = if pressed {
        theme::lerp(fill, Color32::BLACK, 0.2)
    } else {
        fill
    };
    let r = geom::LIGHT_RADIUS;
    p.circle_filled(c, r, rim);
    p.circle_filled(c, r - 0.5, fill);
    let s = Stroke::new(1.0, glyph);
    if glyphs {
        let d = 2.0;
        match light {
            Light::Close => {
                p.line_segment([c + vec2(-d, -d), c + vec2(d, d)], s);
                p.line_segment([c + vec2(d, -d), c + vec2(-d, d)], s);
            }
            Light::Minimize => {
                p.line_segment([c + vec2(-2.5, 0.0), c + vec2(2.5, 0.0)], s);
            }
            Light::Zoom => {
                // Two small triangles pointing to opposite corners (full screen).
                let a = [
                    c + vec2(-2.4, 1.0),
                    c + vec2(-2.4, -2.4),
                    c + vec2(1.0, -2.4),
                ];
                let b = [c + vec2(2.4, -1.0), c + vec2(2.4, 2.4), c + vec2(-1.0, 2.4)];
                for t in [a, b] {
                    p.add(egui::Shape::convex_polygon(t.to_vec(), glyph, Stroke::NONE));
                }
            }
        }
    } else if dirty && light == Light::Close && active {
        p.circle_filled(c, 1.5, glyph);
    }
}
